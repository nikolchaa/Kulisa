//! The decoding thread and the clock that picks each frame.

use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::time::Duration;

use kulisa_audio::{Audio, PcmHandle, PcmWriter};
use kulisa_platform::MediaLocation;

use crate::{Info, Options, Source};

/// Pictures decoded ahead. A 1080p frame is 8 MB, so few, and they are reused.
const FRAMES_AHEAD: usize = 3;
/// Seconds of sound kept ready (the audio ring holds more).
const AUDIO_AHEAD: f64 = 0.5;
/// How close to the end counts as the end, since sound rarely lines up exactly.
const END_SLACK: f64 = 0.05;

enum Command {
    /// Where the sound goes, once the rate is known. `None`: no sound.
    Start(Option<PcmWriter>),
    Seek {
        time: f64,
        generation: u64,
        writer: Option<PcmWriter>,
    },
}

enum Event {
    Opened(Info),
    /// Everything was decoded.
    Ended,
    Failed(String),
}

struct Frame {
    pts: f64,
    generation: u64,
    rgba: Vec<u8>,
}

/// A playing video.
pub struct Video {
    commands: Sender<Command>,
    events: Receiver<Event>,
    frames: Receiver<Frame>,
    /// Shown pictures' memory, going back to be filled again.
    spare: Sender<Vec<u8>>,
    options: Options,
    info: Option<Info>,
    next: Option<Frame>,
    current: Option<Frame>,
    pcm: Option<PcmHandle>,
    /// What the clock reads when the sound's position is 0.
    base: f64,
    clock: f64,
    generation: u64,
    paused: bool,
    decoded_all: bool,
    done: bool,
    error: Option<String>,
}

impl Video {
    /// Starts opening `location`. It plays once [`Video::poll`] sees it has opened.
    pub fn open(location: MediaLocation, options: Options) -> Video {
        Video::with_source(options, move || crate::open(&location))
    }

    pub(crate) fn with_source(
        options: Options,
        open: impl FnOnce() -> Result<Box<dyn Source>, String> + Send + 'static,
    ) -> Video {
        let (commands, command_rx) = channel();
        let (event_tx, events) = channel();
        let (frame_tx, frames) = sync_channel(FRAMES_AHEAD);
        let (spare, spare_rx) = channel();
        let looping = options.looping;
        let start = move || {
            let source = match open() {
                Ok(source) => source,
                Err(e) => {
                    let _ = event_tx.send(Event::Failed(e));
                    return;
                }
            };
            let _ = event_tx.send(Event::Opened(source.info()));
            let worker =
                Worker { source, commands: command_rx, events: event_tx, frames: frame_tx, spare: spare_rx };
            worker.run(looping);
        };
        if let Err(e) = std::thread::Builder::new().name("kulisa-video".into()).spawn(start) {
            log::warn!("video: {e}");
        }
        let paused = options.paused;
        Video {
            commands,
            events,
            frames,
            spare,
            options,
            info: None,
            next: None,
            current: None,
            pcm: None,
            base: 0.0,
            clock: 0.0,
            generation: 0,
            paused,
            decoded_all: false,
            done: false,
            error: None,
        }
    }

    fn sound(&mut self, audio: Option<&mut Audio>, info: Info) -> Option<PcmWriter> {
        let (rate, channel, audio) = (info.audio_rate?, self.options.channel.clone()?, audio?);
        match audio.play_pcm(&channel, rate, self.options.volume) {
            Ok((handle, writer)) => {
                handle.set_paused(self.paused);
                self.pcm = Some(handle);
                Some(writer)
            }
            Err(e) => {
                log::warn!("video: {e}");
                None
            }
        }
    }

    /// Advances by `dt` seconds. Returns a picture when a new one is due, as RGBA, with what it
    /// is a picture of.
    pub fn poll(&mut self, dt: f64, audio: Option<&mut Audio>) -> Option<(&[u8], Info)> {
        let mut audio = audio;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Opened(info) => {
                    self.info = Some(info);
                    let writer = self.sound(audio.as_deref_mut(), info);
                    let _ = self.commands.send(Command::Start(writer));
                }
                Event::Ended => self.decoded_all = true,
                Event::Failed(e) => {
                    self.error = Some(e);
                    self.done = true;
                }
            }
        }
        let info = self.info?;
        if self.done {
            return None;
        }
        if self.next.is_none() {
            self.next = self.take_frame();
        }
        let started = self.current.is_some() || self.next.is_some();
        if !self.paused && started {
            self.clock = match &self.pcm {
                Some(pcm) => self.base + pcm.position(),
                None => self.clock + dt,
            };
        }
        let mut shown = false;
        while let Some(frame) = &self.next {
            // The first picture shows at once, even when paused.
            if frame.pts > self.clock && self.current.is_some() {
                break;
            }
            let frame = self.next.take().unwrap();
            if let Some(old) = self.current.replace(frame) {
                let _ = self.spare.send(old.rgba);
            }
            shown = true;
            self.next = self.take_frame();
        }
        if self.decoded_all && self.next.is_none() {
            let sound_over = self.pcm.as_ref().is_none_or(|p| p.buffered() <= 0.0);
            if self.clock >= info.duration - END_SLACK || sound_over && self.pcm.is_some() {
                self.done = true;
            }
        }
        if self.done
            && let Some(pcm) = self.pcm.take()
        {
            pcm.close();
        }
        if shown { self.current.as_ref().map(|f| (f.rgba.as_slice(), info)) } else { None }
    }

    /// The next frame of this generation; earlier ones are from before a seek.
    fn take_frame(&mut self) -> Option<Frame> {
        while let Ok(frame) = self.frames.try_recv() {
            if frame.generation == self.generation {
                return Some(frame);
            }
            let _ = self.spare.send(frame.rgba);
        }
        None
    }

    pub fn info(&self) -> Option<Info> {
        self.info
    }

    /// The time in the video, in seconds.
    pub fn time(&self) -> f64 {
        match self.info {
            Some(i) if self.options.looping && i.duration > 0.0 => self.clock % i.duration,
            Some(i) => self.clock.min(i.duration),
            None => 0.0,
        }
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(pcm) = &self.pcm {
            pcm.set_paused(paused);
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.options.volume = volume;
        if let Some(pcm) = &self.pcm {
            pcm.set_volume(volume);
        }
    }

    /// Jumps to `time` seconds. The next picture from there shows when it is ready.
    pub fn seek(&mut self, time: f64, audio: Option<&mut Audio>) {
        let Some(info) = self.info else { return };
        let time = time.clamp(0.0, info.duration.max(0.0));
        if let Some(pcm) = self.pcm.take() {
            pcm.close();
        }
        let writer = self.sound(audio, info);
        self.generation += 1;
        self.base = time;
        self.clock = time;
        self.decoded_all = false;
        self.done = false;
        if let Some(frame) = self.next.take() {
            let _ = self.spare.send(frame.rgba);
        }
        // Lets the next picture show at once, whatever its time.
        if let Some(frame) = self.current.take() {
            let _ = self.spare.send(frame.rgba);
        }
        let _ = self.commands.send(Command::Seek { time, generation: self.generation, writer });
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        if let Some(pcm) = &self.pcm {
            pcm.close();
        }
    }
}

struct Worker {
    source: Box<dyn Source>,
    commands: Receiver<Command>,
    events: Sender<Event>,
    frames: SyncSender<Frame>,
    spare: Receiver<Vec<u8>>,
}

/// Sound on its way into the stream, lined up with the video's time.
struct AudioOut {
    writer: PcmWriter,
    rate: f64,
    /// Frames written since `start`.
    written: u64,
    /// The video time the next frame written belongs to, when the stream began.
    start: f64,
    /// Read but not written yet.
    pending: Vec<[f32; 2]>,
    at: usize,
    ended: bool,
}

impl AudioOut {
    fn new(writer: PcmWriter, rate: u32, start: f64) -> AudioOut {
        AudioOut {
            writer,
            rate: f64::from(rate),
            written: 0,
            start,
            pending: Vec::new(),
            at: 0,
            ended: false,
        }
    }

    fn frames_for(&self, seconds: f64) -> u64 {
        (seconds.max(0.0) * self.rate) as u64
    }

    /// Writes silence until the stream reaches `time`. True when it has.
    fn pad_to(&mut self, time: f64) -> bool {
        const SILENCE: [[f32; 2]; 1024] = [[0.0; 2]; 1024];
        let target = self.frames_for(time - self.start);
        while self.written < target {
            let n = (target - self.written).min(SILENCE.len() as u64) as usize;
            let wrote = self.writer.write(&SILENCE[..n]);
            self.written += wrote as u64;
            if wrote < n {
                return false;
            }
        }
        true
    }

    /// Takes what fits from `pending`. True when all of it went.
    fn flush(&mut self) -> bool {
        let wrote = self.writer.write(&self.pending[self.at..]);
        self.at += wrote;
        self.written += wrote as u64;
        if self.at == self.pending.len() {
            self.pending.clear();
            self.at = 0;
            true
        } else {
            false
        }
    }

    fn has_enough(&self) -> bool {
        self.writer.room() == 0 || self.writer.buffered() >= AUDIO_AHEAD
    }
}

impl Worker {
    /// Applies waiting commands. False when the video is gone.
    fn commands(&mut self, state: &mut Run) -> bool {
        loop {
            match self.commands.try_recv() {
                Ok(Command::Start(writer)) => {
                    let rate = self.source.info().audio_rate.unwrap_or(1);
                    state.audio = writer.map(|w| AudioOut::new(w, rate, 0.0));
                    state.started = true;
                }
                Ok(Command::Seek { time, generation, writer }) => {
                    let rate = self.source.info().audio_rate.unwrap_or(1);
                    state.audio = writer.map(|w| AudioOut::new(w, rate, time));
                    state.generation = generation;
                    state.from = time;
                    state.base = 0.0;
                    state.video_ended = false;
                    state.told_end = false;
                    state.pending = None;
                    if let Err(e) = self.source.seek(time) {
                        let _ = self.events.send(Event::Failed(e));
                        return false;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
            }
        }
    }

    fn video(&mut self, state: &mut Run) -> Result<bool, String> {
        let frame = match state.pending.take() {
            Some(frame) => frame,
            None => {
                if state.video_ended {
                    return Ok(false);
                }
                let mut rgba = self.spare.try_recv().unwrap_or_default();
                match self.source.read_video(&mut rgba)? {
                    Some(pts) if pts + 0.001 < state.from => {
                        // Before the point asked for: decoded only to get there.
                        return Ok(true);
                    }
                    Some(pts) => Frame { pts: pts + state.base, generation: state.generation, rgba },
                    None => {
                        state.video_ended = true;
                        return Ok(true);
                    }
                }
            }
        };
        match self.frames.try_send(frame) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(frame)) => {
                state.pending = Some(frame);
                Ok(false)
            }
            Err(TrySendError::Disconnected(_)) => Err(String::new()),
        }
    }

    fn audio(&mut self, state: &mut Run) -> Result<bool, String> {
        let Some(out) = &mut state.audio else { return Ok(false) };
        if !out.pending.is_empty() && !out.flush() {
            return Ok(false);
        }
        if out.ended || out.has_enough() {
            return Ok(false);
        }
        let mut block = Vec::new();
        match self.source.read_audio(&mut block)? {
            Some(pts) => {
                let at = state.base + pts;
                // Sound from before a seek point is cut, and any gap filled, so it stays in line.
                let start = out.start + out.written as f64 / out.rate;
                let skip = ((start - at) * out.rate).max(0.0) as usize;
                if skip < block.len() {
                    if !out.pad_to(at) {
                        out.pending = block.split_off(skip);
                        return Ok(true);
                    }
                    out.pending = block.split_off(skip);
                }
            }
            None => out.ended = true,
        }
        Ok(true)
    }

    fn run(mut self, looping: bool) {
        let mut state = Run::default();
        let duration = self.source.info().duration;
        loop {
            if !self.commands(&mut state) {
                return;
            }
            if !state.started {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            let busy = match self.video(&mut state).and_then(|a| Ok(a | self.audio(&mut state)?)) {
                Ok(busy) => busy,
                Err(e) => {
                    // Empty: the video was dropped.
                    if !e.is_empty() {
                        let _ = self.events.send(Event::Failed(e));
                    }
                    return;
                }
            };
            let audio_over = state.audio.as_ref().is_none_or(|a| a.ended && a.pending.is_empty());
            if state.video_ended && state.pending.is_none() && audio_over {
                // The sound lasts as long as the video, so the clock can reach the end.
                let end = state.base + duration;
                let filled = state.audio.as_mut().is_none_or(|a| a.pad_to(end));
                if filled && looping && duration > 0.0 {
                    if let Err(e) = self.source.seek(0.0) {
                        let _ = self.events.send(Event::Failed(e));
                        return;
                    }
                    state.base = end;
                    state.from = 0.0;
                    state.video_ended = false;
                    if let Some(a) = &mut state.audio {
                        a.ended = false;
                    }
                    continue;
                }
                if filled && !state.told_end {
                    state.told_end = true;
                    let _ = self.events.send(Event::Ended);
                }
            }
            if !busy {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[derive(Default)]
struct Run {
    started: bool,
    audio: Option<AudioOut>,
    pending: Option<Frame>,
    video_ended: bool,
    told_end: bool,
    generation: u64,
    /// Pictures before this time are not wanted (after a seek).
    from: f64,
    /// What loops have added to the source's timestamps.
    base: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 frames per second for a second; each picture's first byte says which it is.
    struct Fake {
        frame: u32,
        audio: u32,
    }

    impl Source for Fake {
        fn info(&self) -> Info {
            Info { width: 1, height: 1, duration: 1.0, audio_rate: None }
        }

        fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String> {
            if self.frame == 10 {
                return Ok(None);
            }
            *rgba = vec![self.frame as u8, 0, 0, 255];
            self.frame += 1;
            Ok(Some(f64::from(self.frame - 1) / 10.0))
        }

        fn read_audio(&mut self, _out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String> {
            self.audio += 1;
            Ok(None)
        }

        fn seek(&mut self, time: f64) -> Result<(), String> {
            // Key frames every 0.5 seconds.
            self.frame = ((time / 0.5).floor() * 5.0) as u32;
            Ok(())
        }
    }

    fn video(options: Options) -> Video {
        Video::with_source(options, || Ok(Box::new(Fake { frame: 0, audio: 0 })))
    }

    /// Steps until `until` says stop, collecting the pictures shown.
    fn drive(v: &mut Video, dt: f64, until: impl Fn(&Video, &[u8]) -> bool) -> Vec<u8> {
        let mut shown = Vec::new();
        for _ in 0..20000 {
            if let Some((px, _)) = v.poll(dt, None) {
                shown.push(px[0]);
            }
            if until(v, &shown) {
                return shown;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        panic!("the video did not get there: {shown:?}, time {}", v.time());
    }

    #[test]
    fn plays_every_picture_in_order_and_ends() {
        let mut v = video(Options::default());
        let shown = drive(&mut v, 0.01, |v, _| v.done());
        assert_eq!(shown, (0..10).collect::<Vec<u8>>());
        assert!(v.error().is_none());
        assert!((v.time() - 1.0).abs() < 0.1, "{}", v.time());
    }

    #[test]
    fn late_pictures_are_skipped() {
        let mut v = video(Options::default());
        let shown = drive(&mut v, 0.25, |v, _| v.done());
        assert!(shown.len() < 10, "{shown:?}");
        assert_eq!(*shown.last().unwrap(), 9);
    }

    #[test]
    fn pausing_stops_the_clock_but_shows_the_first_picture() {
        let mut v = video(Options { paused: true, ..Default::default() });
        let shown = drive(&mut v, 0.1, |_, s| !s.is_empty());
        assert_eq!(shown, [0]);
        for _ in 0..20 {
            assert!(v.poll(0.1, None).is_none());
        }
        assert_eq!(v.time(), 0.0);
        v.set_paused(false);
        let shown = drive(&mut v, 0.1, |v, _| v.done());
        assert_eq!(*shown.last().unwrap(), 9);
    }

    #[test]
    fn seeking_lands_on_the_right_picture() {
        let mut v = video(Options { paused: true, ..Default::default() });
        drive(&mut v, 0.01, |_, s| !s.is_empty());
        v.seek(0.7, None);
        // The source lands on the key frame at 0.5; pictures up to 0.7 are decoded and dropped.
        let shown = drive(&mut v, 0.01, |_, s| !s.is_empty());
        assert_eq!(shown, [7]);
        assert!((v.time() - 0.7).abs() < 1e-9);
        v.set_paused(false);
        let shown = drive(&mut v, 0.01, |v, _| v.done());
        assert_eq!(shown, [8, 9]);
        // A finished video can go back.
        v.seek(0.0, None);
        assert!(!v.done());
        let shown = drive(&mut v, 0.01, |_, s| !s.is_empty());
        assert_eq!(shown, [0]);
    }

    #[test]
    fn loops_go_round_without_ending() {
        let mut v = video(Options { looping: true, ..Default::default() });
        let shown = drive(&mut v, 0.01, |_, s| s.len() > 25);
        assert!(!v.done());
        assert_eq!(&shown[..12], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1]);
        assert!(v.time() < 1.0);
    }

    #[test]
    fn a_file_that_will_not_open_reports_why() {
        let mut v = Video::with_source(Options::default(), || Err("no such file".into()));
        drive(&mut v, 0.01, |v, _| v.done());
        assert_eq!(v.error(), Some("no such file"));
    }

    #[test]
    fn dropping_a_video_stops_its_thread() {
        let v = video(Options::default());
        drop(v);
    }
}
