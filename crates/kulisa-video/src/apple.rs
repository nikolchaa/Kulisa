//! macOS and iOS: AVFoundation. A reader per track, so neither waits on the other. A reader only
//! goes forward, so seeking makes new ones starting from that time.

use std::ffi::c_void;

use kulisa_platform::MediaLocation;
use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send, msg_send_id};
use objc2_foundation::{NSArray, NSString, NSURL};

use crate::{Info, Source};

#[link(name = "AVFoundation", kind = "framework")]
#[link(name = "CoreMedia", kind = "framework")]
#[link(name = "CoreVideo", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CMSampleBufferGetPresentationTimeStamp(buffer: *mut c_void) -> CMTime;
    fn CMSampleBufferGetImageBuffer(buffer: *mut c_void) -> *mut c_void;
    fn CMSampleBufferGetDataBuffer(buffer: *mut c_void) -> *mut c_void;
    fn CMBlockBufferGetDataLength(buffer: *mut c_void) -> usize;
    fn CMBlockBufferCopyDataBytes(buffer: *mut c_void, offset: usize, length: usize, to: *mut c_void) -> i32;
    fn CVPixelBufferLockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(buffer: *mut c_void) -> *mut u8;
    fn CVPixelBufferGetBytesPerRow(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetWidth(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(buffer: *mut c_void) -> usize;
    fn CFRelease(object: *const c_void);
}

/// CoreMedia's time: `value / timescale` seconds.
#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

// SAFETY: this is how the Objective-C runtime describes CMTime.
unsafe impl Encode for CMTime {
    const ENCODING: Encoding =
        Encoding::Struct("?", &[Encoding::LongLong, Encoding::Int, Encoding::UInt, Encoding::LongLong]);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTimeRange {
    start: CMTime,
    duration: CMTime,
}

// SAFETY: this is how the Objective-C runtime describes CMTimeRange.
unsafe impl Encode for CMTimeRange {
    const ENCODING: Encoding = Encoding::Struct("?", &[CMTime::ENCODING, CMTime::ENCODING]);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

// SAFETY: this is how the Objective-C runtime describes CGSize on 64-bit systems.
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
}

/// What a CMSampleBufferRef points at, named as the Objective-C runtime expects.
#[repr(C)]
struct SampleBuffer([u8; 0]);

// SAFETY: this is how the Objective-C runtime describes CMSampleBufferRef.
unsafe impl RefEncode for SampleBuffer {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("opaqueCMSampleBuffer", &[]));
}

const TIME_VALID: u32 = 1;
const TIME_POSITIVE_INFINITY: u32 = 1 | 4;
const FOUR_CC_BGRA: u32 = u32::from_be_bytes(*b"BGRA");
const FOUR_CC_LPCM: u32 = u32::from_be_bytes(*b"lpcm");

fn cm_time(seconds: f64) -> CMTime {
    const SCALE: i32 = 600_000;
    CMTime { value: (seconds * f64::from(SCALE)) as i64, timescale: SCALE, flags: TIME_VALID, epoch: 0 }
}

fn seconds(t: CMTime) -> f64 {
    if t.timescale > 0 && t.flags & TIME_VALID != 0 { t.value as f64 / f64::from(t.timescale) } else { 0.0 }
}

type Object = Retained<AnyObject>;

/// Which of AVFoundation's readers a track gets.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Video,
    Audio(u32),
}

/// A dictionary of NSNumbers, as output settings are.
fn settings(entries: &[(&str, u32)]) -> Object {
    // SAFETY: documented class methods with their documented argument types.
    unsafe {
        let d: Object = msg_send_id![class!(NSMutableDictionary), dictionary];
        for (key, value) in entries {
            let number: Object = msg_send_id![class!(NSNumber), numberWithUnsignedInt: *value];
            let key = NSString::from_str(key);
            let _: () = msg_send![&*d, setObject: &*number, forKey: &*key];
        }
        d
    }
}

/// Starts reading `track` from `from` seconds, returning the reader and its output.
fn start_reader(
    asset: &AnyObject,
    track: &AnyObject,
    kind: Kind,
    from: f64,
) -> Result<(Object, Object), String> {
    let output_settings = match kind {
        // CoreVideo's kCVPixelBufferPixelFormatTypeKey.
        Kind::Video => settings(&[("PixelFormatType", FOUR_CC_BGRA)]),
        // AVFoundation's AVFormatIDKey and friends: 32-bit floats, two channels, side by side.
        Kind::Audio(rate) => settings(&[
            ("AVFormatIDKey", FOUR_CC_LPCM),
            ("AVSampleRateKey", rate),
            ("AVNumberOfChannelsKey", 2),
            ("AVLinearPCMBitDepthKey", 32),
            ("AVLinearPCMIsFloatKey", 1),
            ("AVLinearPCMIsBigEndianKey", 0),
            ("AVLinearPCMIsNonInterleaved", 0),
        ]),
    };
    // SAFETY: documented methods with their documented argument types. The time range is set
    // before reading starts, as AVAssetReader requires.
    unsafe {
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let reader: Option<Object> =
            msg_send_id![class!(AVAssetReader), assetReaderWithAsset: asset, error: &mut error];
        let reader = reader.ok_or("AVFoundation could not read this file")?;
        let output: Object = msg_send_id![
            class!(AVAssetReaderTrackOutput),
            assetReaderTrackOutputWithTrack: track,
            outputSettings: &*output_settings
        ];
        let _: () = msg_send![&*reader, addOutput: &*output];
        if from > 0.0 {
            let forever = CMTime { value: 0, timescale: 0, flags: TIME_POSITIVE_INFINITY, epoch: 0 };
            let range = CMTimeRange { start: cm_time(from), duration: forever };
            let _: () = msg_send![&*reader, setTimeRange: range];
        }
        let started: Bool = msg_send![&*reader, startReading];
        if !started.as_bool() {
            let error: Option<Object> = msg_send_id![&*reader, error];
            let message = error.map_or_else(String::new, |e| {
                let text: Retained<NSString> = msg_send_id![&*e, localizedDescription];
                text.to_string()
            });
            return Err(format!("AVFoundation could not start reading: {message}"));
        }
        Ok((reader, output))
    }
}

struct AvFoundation {
    asset: Object,
    video_track: Object,
    audio_track: Option<Object>,
    video: (Object, Object),
    audio: Option<(Object, Object)>,
    info: Info,
}

fn tracks(asset: &AnyObject, kind: &str) -> Option<Object> {
    let kind = NSString::from_str(kind);
    // SAFETY: documented method; the array's first element is retained before it goes.
    unsafe {
        let list: Retained<NSArray<AnyObject>> = msg_send_id![asset, tracksWithMediaType: &*kind];
        list.firstObject()
    }
}

pub(crate) fn open(location: &MediaLocation) -> Result<Box<dyn Source>, String> {
    let MediaLocation::File(path) = location;
    if !path.is_file() {
        return Err(format!("no video `{}`", path.display()));
    }
    autoreleasepool(|_| {
        let name = NSString::from_str(&path.to_string_lossy());
        // SAFETY: documented methods with their documented argument types; the struct results
        // are returned by value as declared.
        unsafe {
            let url: Retained<NSURL> = msg_send_id![class!(NSURL), fileURLWithPath: &*name];
            let none: *const AnyObject = std::ptr::null();
            let asset: Object = msg_send_id![class!(AVURLAsset), URLAssetWithURL: &*url, options: none];
            let video_track = tracks(&asset, "vide").ok_or("the file has no video AVFoundation can read")?;
            let size: CGSize = msg_send![&*video_track, naturalSize];
            let duration: CMTime = msg_send![&*asset, duration];
            let audio_track = tracks(&asset, "soun");
            // Sound is asked for at the rate most files have and the mixer resamples anyway.
            let audio_rate = audio_track.as_ref().map(|_| 48_000);
            let video = start_reader(&asset, &video_track, Kind::Video, 0.0)?;
            let audio = match (&audio_track, audio_rate) {
                (Some(track), Some(rate)) => Some(start_reader(&asset, track, Kind::Audio(rate), 0.0)?),
                _ => None,
            };
            let info = Info {
                width: size.width as u32,
                height: size.height as u32,
                duration: seconds(duration),
                audio_rate,
            };
            let source = AvFoundation { asset, video_track, audio_track, video, audio, info };
            Ok(Box::new(source) as Box<dyn Source>)
        }
    })
}

/// The next sample buffer, owned by the caller; `None` at the end.
fn next_sample(output: &AnyObject) -> Option<*mut c_void> {
    // SAFETY: the "copy" method returns a +1 reference, or null at the end.
    let sample: *mut SampleBuffer = unsafe { msg_send![output, copyNextSampleBuffer] };
    (!sample.is_null()).then_some(sample.cast())
}

impl Source for AvFoundation {
    fn info(&self) -> Info {
        self.info
    }

    fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String> {
        autoreleasepool(|_| {
            let Some(sample) = next_sample(&self.video.1) else { return Ok(None) };
            // SAFETY: the sample and its pixel buffer stay valid until released below; the base
            // address holds `height` rows of `row` bytes while the buffer is locked (read-only).
            unsafe {
                let time = seconds(CMSampleBufferGetPresentationTimeStamp(sample));
                let image = CMSampleBufferGetImageBuffer(sample);
                if image.is_null() {
                    CFRelease(sample);
                    return Err("the decoder gave no picture".into());
                }
                const READ_ONLY: u64 = 1;
                CVPixelBufferLockBaseAddress(image, READ_ONLY);
                let (w, h) = (CVPixelBufferGetWidth(image), CVPixelBufferGetHeight(image));
                let row = CVPixelBufferGetBytesPerRow(image);
                let base = CVPixelBufferGetBaseAddress(image);
                rgba.resize(w * h * 4, 255);
                for (y, out) in rgba.chunks_exact_mut(w * 4).enumerate() {
                    let from = std::slice::from_raw_parts(base.add(y * row), w * 4);
                    for (o, px) in out.as_chunks_mut::<4>().0.iter_mut().zip(from.as_chunks::<4>().0) {
                        *o = [px[2], px[1], px[0], 255];
                    }
                }
                CVPixelBufferUnlockBaseAddress(image, READ_ONLY);
                CFRelease(sample);
                Ok(Some(time))
            }
        })
    }

    fn read_audio(&mut self, out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String> {
        let Some(audio) = &self.audio else { return Ok(None) };
        autoreleasepool(|_| {
            let Some(sample) = next_sample(&audio.1) else { return Ok(None) };
            // SAFETY: the block buffer is valid until the sample is released, and the copy fills
            // `frames` whole stereo frames of floats.
            unsafe {
                let time = seconds(CMSampleBufferGetPresentationTimeStamp(sample));
                let block = CMSampleBufferGetDataBuffer(sample);
                if !block.is_null() {
                    let length = CMBlockBufferGetDataLength(block);
                    let frames = length / size_of::<[f32; 2]>();
                    let at = out.len();
                    out.resize(at + frames, [0.0; 2]);
                    let to = out[at..].as_mut_ptr().cast::<c_void>();
                    CMBlockBufferCopyDataBytes(block, 0, frames * size_of::<[f32; 2]>(), to);
                }
                CFRelease(sample);
                Ok(Some(time))
            }
        })
    }

    fn seek(&mut self, time: f64) -> Result<(), String> {
        autoreleasepool(|_| {
            self.video = start_reader(&self.asset, &self.video_track, Kind::Video, time)?;
            if let (Some(track), Some(rate)) = (&self.audio_track, self.info.audio_rate) {
                self.audio = Some(start_reader(&self.asset, track, Kind::Audio(rate), time)?);
            }
            Ok(())
        })
    }
}
