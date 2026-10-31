//! Windows: Media Foundation's source reader. Its DLLs are loaded when a video opens, because
//! Windows "N" editions leave them out and a game with no video must still start there.

use std::ffi::c_void;

use kulisa_platform::MediaLocation;
use libloading::Library;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{GUID, HRESULT, HSTRING, Interface, PCWSTR};

use crate::{Info, Source};

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const TICKS: f64 = 10_000_000.0;
const END: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;

struct Foundation {
    plat: Library,
    /// Keeps `MFCreateSourceReaderFromURL`'s code loaded.
    readwrite: Library,
}

type Startup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type Shutdown = unsafe extern "system" fn() -> HRESULT;
type CreateOne = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type CreateAttributes = unsafe extern "system" fn(*mut *mut c_void, u32) -> HRESULT;
type CreateReader = unsafe extern "system" fn(PCWSTR, *mut c_void, *mut *mut c_void) -> HRESULT;

impl Foundation {
    fn load() -> Result<Foundation, String> {
        let missing = |e| {
            format!("this edition of Windows has no Media Foundation (install its Media Feature Pack): {e}")
        };
        // SAFETY: these are system libraries, whose load has no conditions.
        let (plat, readwrite) = unsafe {
            (Library::new("mfplat.dll").map_err(missing)?, Library::new("mfreadwrite.dll").map_err(missing)?)
        };
        let foundation = Foundation { plat, readwrite };
        // SAFETY: the documented signature; every other Media Foundation call comes after it.
        unsafe {
            let startup = foundation.plat.get::<Startup>(b"MFStartup\0").map_err(|e| e.to_string())?;
            startup(MF_VERSION, MFSTARTUP_FULL).ok().map_err(|e| format!("Media Foundation: {e}"))?;
        }
        Ok(foundation)
    }

    fn media_type(&self) -> Result<IMFMediaType, String> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: the documented signature, returning a new reference on success.
        unsafe {
            let create = self.plat.get::<CreateOne>(b"MFCreateMediaType\0").map_err(|e| e.to_string())?;
            create(&mut raw).ok().map_err(|e| e.to_string())?;
            Ok(IMFMediaType::from_raw(raw))
        }
    }

    fn reader(&self, path: &std::path::Path) -> Result<IMFSourceReader, String> {
        let url = HSTRING::from(path.as_os_str());
        let mut attributes = std::ptr::null_mut();
        let mut raw = std::ptr::null_mut();
        // SAFETY: the documented signatures; both results are new references on success.
        unsafe {
            let create =
                self.plat.get::<CreateAttributes>(b"MFCreateAttributes\0").map_err(|e| e.to_string())?;
            create(&mut attributes, 1).ok().map_err(|e| e.to_string())?;
            let attributes = IMFAttributes::from_raw(attributes);
            // Lets the reader convert any decoder's output to the RGB asked for below.
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(|e| e.to_string())?;
            let reader = self
                .readwrite
                .get::<CreateReader>(b"MFCreateSourceReaderFromURL\0")
                .map_err(|e| e.to_string())?;
            reader(PCWSTR(url.as_ptr()), attributes.as_raw(), &mut raw)
                .ok()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(IMFSourceReader::from_raw(raw))
        }
    }
}

impl Drop for Foundation {
    fn drop(&mut self) {
        // SAFETY: balances the MFStartup in `load`; the reader was dropped first.
        unsafe {
            if let Ok(shutdown) = self.plat.get::<Shutdown>(b"MFShutdown\0") {
                let _ = shutdown();
            }
        }
    }
}

struct MediaFoundation {
    // Declared first: dropped before the libraries it runs from.
    reader: IMFSourceReader,
    info: Info,
    stride: i32,
    channels: usize,
    _foundation: Foundation,
}

fn pack(hi: u64) -> (u32, u32) {
    ((hi >> 32) as u32, hi as u32)
}

pub(crate) fn open(location: &MediaLocation) -> Result<Box<dyn Source>, String> {
    let MediaLocation::File(path) = location;
    if !path.is_file() {
        return Err(format!("no video `{}`", path.display()));
    }
    // SAFETY: this thread only serves the video. It may already be initialized, which is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let foundation = Foundation::load()?;
    let reader = foundation.reader(path)?;
    let fail = |e: windows::core::Error| format!("{}: {e}", path.display());

    // SAFETY: plain calls on the reader and media types made above.
    unsafe {
        let want = foundation.media_type()?;
        want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(fail)?;
        want.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(fail)?;
        reader
            .SetCurrentMediaType(VIDEO, None, &want)
            .map_err(|e| format!("{}: no decoder for this video: {e}", path.display()))?;
        let got = reader.GetCurrentMediaType(VIDEO).map_err(fail)?;
        let (width, height) = pack(got.GetUINT64(&MF_MT_FRAME_SIZE).map_err(fail)?);
        let stride = got.GetUINT32(&MF_MT_DEFAULT_STRIDE).map_or(width as i32 * 4, |s| s as i32);

        let (mut audio_rate, mut channels) = (None, 0);
        let want = foundation.media_type()?;
        want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(fail)?;
        want.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).map_err(fail)?;
        if reader.SetCurrentMediaType(AUDIO, None, &want).is_ok() {
            let got = reader.GetCurrentMediaType(AUDIO).map_err(fail)?;
            audio_rate = got.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok();
            channels = got.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0) as usize;
        }
        if channels == 0 {
            audio_rate = None;
            let _ = reader.SetStreamSelection(AUDIO, false);
        }

        let source = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
        let duration = reader
            .GetPresentationAttribute(source, &MF_PD_DURATION)
            .ok()
            .and_then(|v| u64::try_from(&v).ok())
            .map_or(0.0, |ticks| ticks as f64 / TICKS);
        let info = Info { width, height, duration, audio_rate };
        Ok(Box::new(MediaFoundation { reader, info, stride, channels, _foundation: foundation }))
    }
}

impl MediaFoundation {
    /// The next sample of a stream, with its time; `None` at the end.
    fn sample(&mut self, stream: u32) -> Result<Option<(IMFSample, f64)>, String> {
        loop {
            let (mut flags, mut time, mut sample) = (0u32, 0i64, None);
            // SAFETY: a synchronous read, with out-pointers valid for the call.
            unsafe {
                self.reader
                    .ReadSample(stream, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample))
                    .map_err(|e| format!("decoding video: {e}"))?;
            }
            if let Some(sample) = sample {
                return Ok(Some((sample, time as f64 / TICKS)));
            }
            if flags & END != 0 {
                return Ok(None);
            }
        }
    }
}

impl Source for MediaFoundation {
    fn info(&self) -> Info {
        self.info
    }

    fn read_video(&mut self, rgba: &mut Vec<u8>) -> Result<Option<f64>, String> {
        let Some((sample, time)) = self.sample(VIDEO)? else { return Ok(None) };
        let fail = |e: windows::core::Error| format!("decoding video: {e}");
        let (w, h) = (self.info.width as usize, self.info.height as usize);
        rgba.resize(w * h * 4, 255);
        // SAFETY: the locked buffer holds `h` rows of `|pitch|` bytes from its first row, as the
        // lock reports, and stays locked until the end of the copy.
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer().map_err(fail)?;
            let (mut first, mut pitch) = (std::ptr::null_mut(), 0i32);
            let two_d = buffer.cast::<IMF2DBuffer>().ok();
            match &two_d {
                Some(b) => b.Lock2D(&mut first, &mut pitch).map_err(fail)?,
                None => {
                    buffer.Lock(&mut first, None, None).map_err(fail)?;
                    pitch = self.stride;
                    // A negative stride means rows run upward from the end of the buffer.
                    if pitch < 0 {
                        first = first.offset((h as isize - 1) * -(pitch as isize));
                    }
                }
            }
            for (y, row) in rgba.chunks_exact_mut(w * 4).enumerate() {
                let from = first.offset(y as isize * pitch as isize);
                let from = std::slice::from_raw_parts(from, w * 4);
                for (out, px) in row.as_chunks_mut::<4>().0.iter_mut().zip(from.as_chunks::<4>().0) {
                    *out = [px[2], px[1], px[0], 255];
                }
            }
            match &two_d {
                Some(b) => b.Unlock2D().map_err(fail)?,
                None => buffer.Unlock().map_err(fail)?,
            }
        }
        Ok(Some(time))
    }

    fn read_audio(&mut self, out: &mut Vec<[f32; 2]>) -> Result<Option<f64>, String> {
        let Some((sample, time)) = self.sample(AUDIO)? else { return Ok(None) };
        let fail = |e: windows::core::Error| format!("decoding sound: {e}");
        // SAFETY: the locked buffer holds `len` bytes of 32-bit floats, aligned for them as the
        // format promises, until it is unlocked below.
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer().map_err(fail)?;
            let (mut data, mut len) = (std::ptr::null_mut(), 0u32);
            buffer.Lock(&mut data, None, Some(&mut len)).map_err(fail)?;
            let samples = std::slice::from_raw_parts(data.cast::<f32>(), len as usize / 4);
            let n = self.channels;
            out.extend(samples.chunks_exact(n).map(|f| [f[0], f[if n > 1 { 1 } else { 0 }]]));
            buffer.Unlock().map_err(fail)?;
        }
        Ok(Some(time))
    }

    fn seek(&mut self, time: f64) -> Result<(), String> {
        let position =
            windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from((time * TICKS) as i64);
        // SAFETY: an all-zero GUID asks for the default time format, 100-nanosecond units.
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &position) }
            .map_err(|e| format!("seeking: {e}"))
    }
}
