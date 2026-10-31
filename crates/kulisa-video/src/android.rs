//! Android: NDK MediaCodec, which uses the phone's hardware decoders. Pictures go through an image
//! reader, whose YUV layout is the one thing every device must describe the same way.

use std::ffi::{CStr, CString, c_char};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use kulisa_platform::MediaLocation;
use ndk_sys::*;

use crate::yuv::{Planes, to_rgba};
use crate::{Info, Source};

const YUV_420_888: i32 = 0x23;
const END_OF_STREAM: u32 = 4;
const OUTPUT_FORMAT_CHANGED: isize = -2;
/// A picture must come out of the reader by then.
const IMAGE_PATIENCE: Duration = Duration::from_secs(2);
/// Microseconds to wait for a decoder buffer on each try.
const BUFFER_WAIT: i64 = 5_000;
/// Tries with nothing coming out before a decoder counts as stuck.
const STUCK_AFTER: u32 = 1_000;

static ASSETS: AtomicPtr<AAssetManager> = AtomicPtr::new(null_mut());

/// Gives videos access to the APK's assets. The manager stays valid for the app's life.
pub fn set_asset_manager(manager: *mut AAssetManager) {
    ASSETS.store(manager, Ordering::Relaxed);
}

/// An extractor on one track of the file, with that track's format.
struct Track {
    extractor: *mut AMediaExtractor,
    format: *mut AMediaFormat,
    mime: CString,
}

impl Drop for Track {
    fn drop(&mut self) {
        // SAFETY: both are owned here and not used after this.
        unsafe {
            AMediaFormat_delete(self.format);
            AMediaExtractor_delete(self.extractor);
        }
    }
}

fn new_extractor(location: &MediaLocation) -> Result<*mut AMediaExtractor, String> {
    // SAFETY: plain creation; the paths and file descriptor are valid for the calls, which
    // keep their own reference to the file.
    unsafe {
        let extractor = AMediaExtractor_new();
        let status = match location {
            MediaLocation::File(path) => {
                let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
                AMediaExtractor_setDataSource(extractor, path.as_ptr())
            }
            MediaLocation::Asset(name) => {
                let c = CString::new(name.as_str()).map_err(|e| e.to_string())?;
                let asset = AAssetManager_open(ASSETS.load(Ordering::Relaxed), c.as_ptr(), 0);
                if asset.is_null() {
                    AMediaExtractor_delete(extractor);
                    return Err(format!("no video `{name}` in the APK"));
                }
                let (mut start, mut length) = (0, 0);
                let fd = AAsset_openFileDescriptor64(asset, &mut start, &mut length);
                AAsset_close(asset);
                if fd < 0 {
                    AMediaExtractor_delete(extractor);
                    return Err(format!("`{name}` is compressed in the APK, so it cannot be played"));
                }
                let status = AMediaExtractor_setDataSourceFd(extractor, fd, start, length);
                libc_close(fd);
                status
            }
        };
        if status != media_status_t::AMEDIA_OK {
            AMediaExtractor_delete(extractor);
            return Err(format!("{location:?}: {status:?}"));
        }
        Ok(extractor)
    }
}

unsafe extern "C" {
    #[link_name = "close"]
    fn libc_close(fd: i32) -> i32;
}

/// The first track whose type starts with `kind` (`video/`, `audio/`), or none.
fn open_track(location: &MediaLocation, kind: &str) -> Result<Option<Track>, String> {
    let extractor = new_extractor(location)?;
    // SAFETY: the extractor is valid; formats returned here are owned by the caller, and the
    // format's strings live as long as it does.
    unsafe {
        for i in 0..AMediaExtractor_getTrackCount(extractor) {
            let format = AMediaExtractor_getTrackFormat(extractor, i);
            let mut mime: *const c_char = std::ptr::null();
            if AMediaFormat_getString(format, AMEDIAFORMAT_KEY_MIME, &mut mime) {
                let name = CStr::from_ptr(mime);
                if name.to_bytes().starts_with(kind.as_bytes()) {
                    AMediaExtractor_selectTrack(extractor, i);
                    return Ok(Some(Track { extractor, format, mime: name.to_owned() }));
                }
            }
            AMediaFormat_delete(format);
        }
        AMediaExtractor_delete(extractor);
    }
    Ok(None)
}

fn int32(format: *mut AMediaFormat, key: *const c_char) -> Option<i32> {
    let mut v = 0;
    // SAFETY: the format is valid and `v` outlives the call.
    unsafe { AMediaFormat_getInt32(format, key, &mut v) }.then_some(v)
}

fn int64(format: *mut AMediaFormat, key: *const c_char) -> Option<i64> {
    let mut v = 0;
    // SAFETY: as above.
    unsafe { AMediaFormat_getInt64(format, key, &mut v) }.then_some(v)
}

/// A started decoder for one track.
struct Decoder {
    track: Track,
    codec: *mut AMediaCodec,
    input_ended: bool,
    output_ended: bool,
}

impl Decoder {
    fn new(track: Track, surface: *mut ANativeWindow) -> Result<Decoder, String> {
        let name = track.mime.to_string_lossy().into_owned();
        // SAFETY: the format and surface are valid for the calls; the codec is owned here.
        unsafe {
            let codec = AMediaCodec_createDecoderByType(track.mime.as_ptr());
            if codec.is_null() {
                return Err(format!("this device has no decoder for {name}"));
            }
            let mut status = AMediaCodec_configure(codec, track.format, surface, null_mut(), 0);
            if status == media_status_t::AMEDIA_OK {
                status = AMediaCodec_start(codec);
            }
            if status != media_status_t::AMEDIA_OK {
                AMediaCodec_delete(codec);
                return Err(format!("the {name} decoder would not start: {status:?}"));
            }
            Ok(Decoder { track, codec, input_ended: false, output_ended: false })
        }
    }

    /// Passes on one compressed sample, if the decoder has room.
    fn feed(&mut self) {
        if self.input_ended {
            return;
        }
        // SAFETY: the buffer index comes from the decoder, which keeps it valid until queued,
        // and the extractor writes at most `capacity` bytes.
        unsafe {
            let index = AMediaCodec_dequeueInputBuffer(self.codec, BUFFER_WAIT);
            if index < 0 {
                return;
            }
            let mut capacity = 0;
            let buffer = AMediaCodec_getInputBuffer(self.codec, index as usize, &mut capacity);
            let size = AMediaExtractor_readSampleData(self.track.extractor, buffer, capacity);
            if size < 0 {
                self.input_ended = true;
                AMediaCodec_queueInputBuffer(self.codec, index as usize, 0, 0, 0, END_OF_STREAM);
            } else {
                let time = AMediaExtractor_getSampleTime(self.track.extractor).max(0) as u64;
                AMediaCodec_queueInputBuffer(self.codec, index as usize, 0, size as usize, time, 0);
                AMediaExtractor_advance(self.track.extractor);
            }
        }
    }

    fn seek(&mut self, time: f64) -> Result<(), String> {
        // SAFETY: both are valid; flushing returns the decoder's buffers to it.
        let status = unsafe {
            AMediaCodec_flush(self.codec);
            AMediaExtractor_seekTo(
                self.track.extractor,
                (time * 1_000_000.0) as i64,
                SeekMode::AMEDIAEXTRACTOR_SEEK_PREVIOUS_SYNC,
            )
        };
        self.input_ended = false;
        self.output_ended = false;
        if status != media_status_t::AMEDIA_OK {
            return Err(format!("seeking: {status:?}"));
        }
        Ok(())
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: owned here and not used after this.
        unsafe {
            AMediaCodec_stop(self.codec);
            AMediaCodec_delete(self.codec);
        }
    }
}

/// Deleted when dropped.
struct ImageReader(*mut AImageReader);

impl Drop for ImageReader {
    fn drop(&mut self) {
        // SAFETY: owned here and not used after this.
        unsafe { AImageReader_delete(self.0) };
    }
}

struct MediaCodec {
    // The decoder is dropped before the reader whose surface it draws on.
    video: Decoder,
    audio: Option<Decoder>,
    channels: usize,
    reader: ImageReader,
    info: Info,
}

pub(crate) fn open(location: &MediaLocation) -> Result<Box<dyn Source>, String> {
    let track = open_track(location, "video/")?.ok_or("the file has no video")?;
    let width = int32(track.format, unsafe { AMEDIAFORMAT_KEY_WIDTH }).unwrap_or(0);
    let height = int32(track.format, unsafe { AMEDIAFORMAT_KEY_HEIGHT }).unwrap_or(0);
    if width <= 0 || height <= 0 {
        return Err("the video does not say its size".into());
    }
    let duration =
        int64(track.format, unsafe { AMEDIAFORMAT_KEY_DURATION }).unwrap_or(0) as f64 / 1_000_000.0;

    let (mut reader, mut window) = (null_mut(), null_mut());
    // SAFETY: out-pointers valid for the calls; the window belongs to the reader.
    let status = unsafe {
        let status = AImageReader_new(width, height, YUV_420_888, 4, &mut reader);
        if status == media_status_t::AMEDIA_OK { AImageReader_getWindow(reader, &mut window) } else { status }
    };
    if status != media_status_t::AMEDIA_OK {
        return Err(format!("no image reader for this video: {status:?}"));
    }
    let reader = ImageReader(reader);
    let video = Decoder::new(track, window)?;

    // A video whose sound will not decode still plays, silently.
    let (mut audio, mut audio_rate, mut channels) = (None, None, 2);
    if let Ok(Some(track)) = open_track(location, "audio/") {
        audio_rate = int32(track.format, unsafe { AMEDIAFORMAT_KEY_SAMPLE_RATE }).map(|r| r as u32);
        channels = int32(track.format, unsafe { AMEDIAFORMAT_KEY_CHANNEL_COUNT }).unwrap_or(2) as usize;
        match Decoder::new(track, null_mut()) {
            Ok(d) => audio = Some(d),
            Err(e) => {
                log::warn!("video sound: {e}");
                audio_rate = None;
            }
        }
    }
    let info = Info { width: width as u32, height: height as u32, duration, audio_rate };
    Ok(Box::new(MediaCodec { video, audio, channels, reader, info }))
}

impl MediaCodec {
    /// Copies the reader's next image, waiting for it to arrive.
    fn next_image(&mut self, rgba: &mut Vec<u8>) -> Result<(), String> {
        let start = Instant::now();
        let mut image = null_mut();
        // SAFETY: the reader is valid and owns what it hands out until the image is deleted.
        unsafe {
            while AImageReader_acquireNextImage(self.reader.0, &mut image) != media_status_t::AMEDIA_OK {
                if start.elapsed() > IMAGE_PATIENCE {
                    return Err("the decoder gave no picture".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let plane = |i: i32| {
                let (mut data, mut len, mut row, mut step) = (null_mut(), 0, 0, 0);
                AImage_getPlaneData(image, i, &mut data, &mut len);
                AImage_getPlaneRowStride(image, i, &mut row);
                AImage_getPlanePixelStride(image, i, &mut step);
                (std::slice::from_raw_parts(data.cast_const(), len as usize), row as usize, step as usize)
            };
            let (y, y_stride, _) = plane(0);
            let (u, uv_stride, uv_step) = plane(1);
            let (v, _, _) = plane(2);
            let planes = Planes { y, y_stride, u, v, uv_stride, uv_step };
            to_rgba(&planes, self.info.width, self.info.height, rgba);
            AImage_delete(image);
        }
        Ok(())
    }

    /// Throws away images left in the reader (before a seek).
    fn drop_images(&mut self) {
        let mut image = null_mut();
        // SAFETY: as in `next_image`.
        unsafe {
            while AImageReader_acquireNextImage(self.reader.0, &mut image) == media_status_t::AMEDIA_OK {
                AImage_delete(image);
            }
        }
    }
}

impl Source for MediaCodec {
    fn info(&self) -> Info {
        self.info
    }

    fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String> {
        let mut tries = 0;
        while !self.video.output_ended {
            self.video.feed();
            let mut info = AMediaCodecBufferInfo { offset: 0, size: 0, presentationTimeUs: 0, flags: 0 };
            // SAFETY: `info` outlives the call and the index comes from the decoder.
            let index = unsafe { AMediaCodec_dequeueOutputBuffer(self.video.codec, &mut info, BUFFER_WAIT) };
            if index < 0 {
                tries += 1;
                if tries > STUCK_AFTER {
                    return Err("the video decoder stopped responding".into());
                }
                continue;
            }
            self.video.output_ended = info.flags & END_OF_STREAM != 0;
            let picture = info.size > 0;
            // SAFETY: as above; `true` sends the picture to the image reader.
            unsafe { AMediaCodec_releaseOutputBuffer(self.video.codec, index as usize, picture) };
            if picture {
                self.next_image(rgba)?;
                return Ok(Some(info.presentationTimeUs as f64 / 1_000_000.0));
            }
        }
        Ok(None)
    }

    fn read_audio(&mut self, out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String> {
        let Some(audio) = &mut self.audio else { return Ok(None) };
        let mut tries = 0;
        while !audio.output_ended {
            audio.feed();
            let mut info = AMediaCodecBufferInfo { offset: 0, size: 0, presentationTimeUs: 0, flags: 0 };
            // SAFETY: `info` outlives the call.
            let index = unsafe { AMediaCodec_dequeueOutputBuffer(audio.codec, &mut info, BUFFER_WAIT) };
            if index == OUTPUT_FORMAT_CHANGED {
                // SAFETY: returns a format owned by the caller.
                unsafe {
                    let format = AMediaCodec_getOutputFormat(audio.codec);
                    if let Some(n) = int32(format, AMEDIAFORMAT_KEY_CHANNEL_COUNT) {
                        self.channels = n.max(1) as usize;
                    }
                    AMediaFormat_delete(format);
                }
            }
            if index < 0 {
                tries += 1;
                if tries > STUCK_AFTER {
                    return Err("the sound decoder stopped responding".into());
                }
                continue;
            }
            audio.output_ended = info.flags & END_OF_STREAM != 0;
            let index = index as usize;
            // SAFETY: the buffer stays valid until released and holds 16-bit samples at the
            // offset and size reported, which is what decoders give unless asked otherwise.
            unsafe {
                let mut capacity = 0;
                let buffer = AMediaCodec_getOutputBuffer(audio.codec, index, &mut capacity);
                if !buffer.is_null() && info.size > 0 {
                    let start = buffer.add(info.offset as usize);
                    let bytes = std::slice::from_raw_parts(start, info.size as usize);
                    let n = self.channels;
                    let sample = |f: &[u8], i: usize| {
                        f32::from(i16::from_le_bytes([f[i * 2], f[i * 2 + 1]])) / 32768.0
                    };
                    out.extend(
                        bytes.chunks_exact(n * 2).map(|f| [sample(f, 0), sample(f, usize::from(n > 1))]),
                    );
                }
                AMediaCodec_releaseOutputBuffer(audio.codec, index, false);
            }
            if info.size > 0 {
                return Ok(Some(info.presentationTimeUs as f64 / 1_000_000.0));
            }
        }
        Ok(None)
    }

    fn seek(&mut self, time: f64) -> Result<(), String> {
        self.video.seek(time)?;
        self.drop_images();
        if let Some(audio) = &mut self.audio {
            audio.seek(time)?;
        }
        Ok(())
    }
}
