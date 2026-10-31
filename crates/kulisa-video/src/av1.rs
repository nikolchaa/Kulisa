//! Linux: a software decoder for what `kulisa build linux` converts videos into, `NAME.ivf` (AV1)
//! with `NAME.ogg` (Vorbis) beside it.

use std::ffi::c_int;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::mem::MaybeUninit;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::ptr::NonNull;

use kulisa_platform::MediaLocation;
use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_flush, dav1d_get_picture,
    dav1d_open, dav1d_picture_unref, dav1d_send_data,
};
use symphonia::core::audio::conv::IntoSample;
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoder;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::units::Time;

use crate::yuv::{Planes, to_rgba};
use crate::{Info, Source};

/// The decoder's "not now", which is this system's errno value.
const EAGAIN: c_int = if cfg!(target_os = "macos") { 35 } else { 11 };

/// One compressed picture's place in the file.
struct Entry {
    at: u64,
    size: u32,
    /// Timestamp in the file's time base.
    pts: u64,
    /// Starts with a sequence header, which encoders write at key frames.
    key: bool,
}

struct Ivf {
    file: BufReader<File>,
    entries: Vec<Entry>,
    next: usize,
    width: u32,
    height: u32,
    /// Seconds per timestamp unit.
    tick: f64,
    duration: f64,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// Reads the headers and notes every picture.
fn read_ivf(path: &Path) -> Result<Ivf, String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let bad = || format!("{} is not an IVF video with AV1", path.display());
    let mut file = BufReader::new(File::open(path).map_err(fail)?);
    let length = file.get_ref().metadata().map_err(fail)?.len();
    let mut header = [0u8; 32];
    file.read_exact(&mut header).map_err(|_| bad())?;
    if &header[..4] != b"DKIF" || &header[8..12] != b"AV01" {
        return Err(bad());
    }
    let header_size = u64::from(u16::from_le_bytes([header[6], header[7]]));
    let width = u32::from(u16::from_le_bytes([header[12], header[13]]));
    let height = u32::from(u16::from_le_bytes([header[14], header[15]]));
    let (rate, scale) = (u32_at(&header, 16), u32_at(&header, 20));
    if rate == 0 || scale == 0 || width == 0 || height == 0 {
        return Err(bad());
    }
    let tick = f64::from(scale) / f64::from(rate);
    let mut entries = Vec::new();
    let mut at = header_size;
    while at + 12 <= length {
        file.seek(SeekFrom::Start(at)).map_err(fail)?;
        let mut frame = [0u8; 12];
        file.read_exact(&mut frame).map_err(fail)?;
        let size = u32_at(&frame, 0);
        let pts = u64::from_le_bytes(frame[4..12].try_into().unwrap());
        // A temporal delimiter (OBU type 2, two bytes), then a sequence header (type 1).
        let mut start = [0u8; 3];
        let key = file.read_exact(&mut start).is_ok() && (start[2] >> 3) & 15 == 1;
        entries.push(Entry { at: at + 12, size, pts, key });
        at += 12 + u64::from(size);
    }
    let last = entries.iter().map(|e| e.pts).max().unwrap_or(0);
    let duration = (last + 1) as f64 * tick;
    Ok(Ivf { file, entries, next: 0, width, height, tick, duration })
}

/// The decoder's context, closed when dropped.
struct Decoder(Option<Dav1dContext>);

impl Decoder {
    fn new() -> Result<Decoder, String> {
        let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
        let mut context = None;
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
        // SAFETY: `settings` is filled by the first call before the second reads it; both
        // pointers are valid for the calls.
        let result = unsafe {
            dav1d_default_settings(NonNull::from(&mut settings).cast());
            (*settings.as_mut_ptr()).n_threads = threads as c_int;
            dav1d_open(Some(NonNull::from(&mut context)), Some(NonNull::from(&mut settings).cast()))
        };
        if result.0 != 0 || context.is_none() {
            return Err(format!("the AV1 decoder did not start ({})", result.0));
        }
        Ok(Decoder(context))
    }

    /// Hands over one compressed picture. False when the decoder must give out pictures first.
    fn send(&mut self, data: &mut Dav1dData) -> Result<bool, String> {
        // SAFETY: the context is open and `data` was made by `dav1d_data_create`.
        let result = unsafe { dav1d_send_data(self.0, Some(NonNull::from(data))) };
        match result.0 {
            0 => Ok(true),
            e if e == -EAGAIN => Ok(false),
            e => Err(format!("decoding video: error {e}")),
        }
    }

    fn picture(&mut self) -> Result<Option<Picture>, String> {
        let mut picture = MaybeUninit::<Dav1dPicture>::zeroed();
        // SAFETY: a zeroed picture is what the call expects; on success it holds a reference.
        let result = unsafe { dav1d_get_picture(self.0, Some(NonNull::from(&mut picture).cast())) };
        match result.0 {
            // SAFETY: it succeeded, so the picture was filled in.
            0 => Ok(Some(Picture(unsafe { picture.assume_init() }))),
            e if e == -EAGAIN => Ok(None),
            e => Err(format!("decoding video: error {e}")),
        }
    }

    fn flush(&mut self) {
        if let Some(context) = self.0 {
            // SAFETY: the context is open.
            unsafe { dav1d_flush(context) };
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: the context is open and is not used again.
        unsafe { dav1d_close(Some(NonNull::from(&mut self.0))) };
    }
}

struct Picture(Dav1dPicture);

impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: holds the reference from `dav1d_get_picture`.
        unsafe { dav1d_picture_unref(Some(NonNull::from(&mut self.0))) };
    }
}

struct Ogg {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
}

fn open_ogg(path: &Path) -> Result<Ogg, String> {
    let fail = |e: symphonia::core::errors::Error| format!("{}: {e}", path.display());
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let format = symphonia::default::get_probe()
        .probe(&Hint::default(), stream, Default::default(), Default::default())
        .map_err(fail)?;
    let track = format.default_track(TrackType::Audio).ok_or("the Ogg file has no sound")?;
    let Some(CodecParameters::Audio(params)) = track.codec_params.as_ref() else {
        return Err("the Ogg file has no sound".into());
    };
    let rate = params.sample_rate.ok_or("the Ogg file does not say its rate")?;
    let decoder =
        symphonia::default::get_codecs().make_audio_decoder(params, &Default::default()).map_err(fail)?;
    let track = track.id;
    Ok(Ogg { format, decoder, track, rate })
}

fn add_frames<S>(buffer: &symphonia::core::audio::AudioBuffer<S>, out: &mut Vec<[f32; 2]>)
where
    S: symphonia::core::audio::sample::Sample + IntoSample<f32>,
{
    let Some(left) = buffer.plane(0) else { return };
    let right = buffer.plane(1).unwrap_or(left);
    out.extend(left.iter().zip(right).map(|(l, r)| [(*l).into_sample(), (*r).into_sample()]));
}

struct Av1 {
    ivf: Ivf,
    decoder: Decoder,
    ogg: Option<Ogg>,
    drained: bool,
    /// A compressed picture the decoder could not take yet.
    waiting: Option<Dav1dData>,
    info: Info,
}

// SAFETY: the data's reference is what the decoder's own threads use too.
unsafe impl Send for Av1 {}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn open(location: &MediaLocation) -> Result<Box<dyn Source>, String> {
    let MediaLocation::File(path) = location;
    Ok(Box::new(open_file(path)?))
}

fn open_file(path: &Path) -> Result<Av1, String> {
    let converted = path.with_extension("ivf");
    if !converted.is_file() {
        return Err(format!(
            "no `{}`: Linux plays videos that Kulisa converted (`kulisa build linux` or `kulisa run`)",
            converted.display()
        ));
    }
    let ivf = read_ivf(&converted)?;
    let sound = path.with_extension("ogg");
    let ogg = if sound.is_file() { Some(open_ogg(&sound)?) } else { None };
    let info = Info {
        width: ivf.width,
        height: ivf.height,
        duration: ivf.duration,
        audio_rate: ogg.as_ref().map(|o| o.rate),
    };
    Ok(Av1 { ivf, decoder: Decoder::new()?, ogg, drained: false, waiting: None, info })
}

impl Av1 {
    /// The next compressed picture as decoder data.
    fn compressed(&mut self) -> Result<Option<Dav1dData>, String> {
        let Some(entry) = self.ivf.entries.get(self.ivf.next) else { return Ok(None) };
        self.ivf.next += 1;
        let mut data = MaybeUninit::<Dav1dData>::zeroed();
        // SAFETY: the call allocates `size` writable bytes and returns them, null on failure.
        let bytes = unsafe { dav1d_data_create(Some(NonNull::from(&mut data).cast()), entry.size as usize) };
        if bytes.is_null() {
            return Err("out of memory decoding video".into());
        }
        // SAFETY: as above; the data was filled in by the successful call.
        let mut data = unsafe { data.assume_init() };
        data.m.timestamp = entry.pts as i64;
        let fail = |e: std::io::Error| format!("reading video: {e}");
        self.ivf.file.seek(SeekFrom::Start(entry.at)).map_err(fail)?;
        // SAFETY: `bytes` is the `size` bytes allocated above, not otherwise referenced yet.
        let target = unsafe { std::slice::from_raw_parts_mut(bytes, entry.size as usize) };
        self.ivf.file.read_exact(target).map_err(fail)?;
        Ok(Some(data))
    }

    fn drop_waiting(&mut self) {
        if let Some(mut data) = self.waiting.take() {
            // SAFETY: it holds the reference from `dav1d_data_create`.
            unsafe { dav1d_data_unref(Some(NonNull::from(&mut data))) };
        }
    }
}

impl Drop for Av1 {
    fn drop(&mut self) {
        self.drop_waiting();
    }
}

impl Source for Av1 {
    fn info(&self) -> Info {
        self.info
    }

    fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String> {
        loop {
            if let Some(picture) = self.decoder.picture()? {
                let p = &picture.0;
                let (w, h) = (p.p.w as usize, p.p.h as usize);
                let (ys, uvs) = (p.stride[0] as usize, p.stride[1] as usize);
                let plane = |i: usize, len: usize| -> Result<&[u8], String> {
                    let at = p.data[i].ok_or("the video has no colour planes")?;
                    // SAFETY: the decoder gives each plane as `height * stride` bytes.
                    Ok(unsafe { std::slice::from_raw_parts(at.as_ptr().cast::<u8>(), len) })
                };
                let chroma = h.div_ceil(2) * uvs;
                let planes = Planes {
                    y: plane(0, h * ys)?,
                    y_stride: ys,
                    u: plane(1, chroma)?,
                    v: plane(2, chroma)?,
                    uv_stride: uvs,
                    uv_step: 1,
                };
                to_rgba(&planes, w as u32, h as u32, rgba);
                return Ok(Some(p.m.timestamp as f64 * self.ivf.tick));
            }
            let data = match self.waiting.take() {
                Some(data) => Some(data),
                None => self.compressed()?,
            };
            match data {
                Some(mut data) => {
                    if !self.decoder.send(&mut data)? {
                        self.waiting = Some(data);
                    }
                }
                None if self.drained => return Ok(None),
                // With nothing sent, asking again gives what the decoder still holds.
                None => self.drained = true,
            }
        }
    }

    fn read_audio(&mut self, out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String> {
        let Some(ogg) = &mut self.ogg else { return Ok(None) };
        let packet = loop {
            match ogg.format.next_packet() {
                Ok(Some(p)) if p.track_id == ogg.track => break p,
                Ok(Some(_)) => {}
                Ok(None) => return Ok(None),
                Err(e) => return Err(format!("reading sound: {e}")),
            }
        };
        let time = packet.pts.get() as f64 / f64::from(ogg.rate);
        match ogg.decoder.decode(&packet) {
            Ok(buffer) => match &buffer {
                GenericAudioBufferRef::U8(b) => add_frames(b, out),
                GenericAudioBufferRef::U16(b) => add_frames(b, out),
                GenericAudioBufferRef::U24(b) => add_frames(b, out),
                GenericAudioBufferRef::U32(b) => add_frames(b, out),
                GenericAudioBufferRef::S8(b) => add_frames(b, out),
                GenericAudioBufferRef::S16(b) => add_frames(b, out),
                GenericAudioBufferRef::S24(b) => add_frames(b, out),
                GenericAudioBufferRef::S32(b) => add_frames(b, out),
                GenericAudioBufferRef::F32(b) => add_frames(b, out),
                GenericAudioBufferRef::F64(b) => add_frames(b, out),
            },
            Err(e) => log::warn!("video sound: {e}"),
        }
        Ok(Some(time))
    }

    fn seek(&mut self, time: f64) -> Result<(), String> {
        self.drop_waiting();
        self.decoder.flush();
        self.drained = false;
        let wanted = (time / self.ivf.tick) as u64;
        self.ivf.next = self.ivf.entries.iter().rposition(|e| e.key && e.pts <= wanted).unwrap_or(0);
        if let Some(ogg) = &mut self.ogg {
            let to = SeekTo::Time {
                time: Time::try_from_secs_f64(time).unwrap_or_default(),
                track_id: Some(ogg.track),
            };
            match ogg.format.seek(SeekMode::Accurate, to) {
                Ok(_) => ogg.decoder.reset(),
                Err(e) => log::warn!("seeking in video sound: {e}"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/clip.mp4")
    }

    #[test]
    fn decodes_the_converted_clip() {
        let mut v = open_file(&clip()).unwrap();
        let info = v.info();
        assert_eq!((info.width, info.height), (160, 90));
        assert!((info.duration - 2.0).abs() < 0.1, "{}", info.duration);
        assert_eq!(info.audio_rate, Some(44100));
        let mut rgba = Vec::new();
        let mut times = Vec::new();
        while let Some(t) = v.read_video(&mut rgba).unwrap() {
            assert_eq!(rgba.len(), 160 * 90 * 4);
            let (r, b) = (rgba[0], rgba[2]);
            if t < 0.9 {
                assert!(r > 200 && b < 60, "red at {t}: {:?}", &rgba[..4]);
            } else if t > 1.1 {
                assert!(b > 200 && r < 60, "blue at {t}: {:?}", &rgba[..4]);
            }
            times.push(t);
        }
        assert_eq!(times.len(), 30, "every picture came out");

        let mut sound = Vec::new();
        let mut first = None;
        while let Some(t) = v.read_audio(&mut sound).unwrap() {
            first.get_or_insert(t);
        }
        assert!(first.unwrap() < 0.1);
        let seconds = sound.len() as f64 / 44100.0;
        assert!((seconds - 2.0).abs() < 0.1, "{seconds}");
        assert!(sound.iter().any(|s| s[0].abs() > 0.05), "there is a tone");
    }

    #[test]
    fn seeking_comes_back_to_a_key_frame_and_can_play_again() {
        let mut v = open_file(&clip()).unwrap();
        let mut rgba = Vec::new();
        while v.read_video(&mut rgba).unwrap().is_some() {}
        v.seek(1.5).unwrap();
        let t = v.read_video(&mut rgba).unwrap().unwrap();
        assert!(t <= 1.5, "{t}");
        let mut last = t;
        while let Some(t) = v.read_video(&mut rgba).unwrap() {
            last = t;
        }
        assert!(last > 1.8, "{last}");
    }

    #[test]
    fn a_missing_conversion_says_what_to_do() {
        let e = open_file(Path::new("nope/intro.mp4")).err().unwrap();
        assert!(e.contains("kulisa build linux"), "{e}");
    }
}
