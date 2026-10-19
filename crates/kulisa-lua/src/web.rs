//! Glue for running Luau's C++ on `wasm32-unknown-unknown` (see docs/DESIGN.md, risk 1).
//!
//! The C++ side is linked against wasi-libc. The few WASI functions it touches are defined
//! here so the module has no `wasi_snapshot_preview1` imports and loads in any browser.

use std::sync::Once;

use web_time::{Instant, SystemTime, UNIX_EPOCH};

unsafe extern "C" {
    fn __wasm_call_ctors();
}

/// Runs C++ static constructors exactly once. Referencing `__wasm_call_ctors` also stops
/// wasm-ld from wrapping every export with a constructor call, which would re-run them.
pub(crate) fn init() {
    static CTORS: Once = Once::new();
    CTORS.call_once(|| unsafe { __wasm_call_ctors() });
}

fn monotonic_ns() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

/// `clock()` for Luau's `os.clock`. wasi-libc's `CLOCKS_PER_SEC` is 1e9.
#[unsafe(no_mangle)]
extern "C" fn clock() -> i64 {
    monotonic_ns() as i64
}

const ERRNO_SUCCESS: u16 = 0;
const ERRNO_BADF: u16 = 8;

#[unsafe(no_mangle)]
unsafe extern "C" fn __imported_wasi_snapshot_preview1_clock_time_get(
    id: u32,
    _precision: u64,
    out: *mut u64,
) -> u16 {
    let ns = match id {
        0 => SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64),
        _ => monotonic_ns(),
    };
    unsafe { *out = ns };
    ERRNO_SUCCESS
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __imported_wasi_snapshot_preview1_environ_sizes_get(
    count: *mut usize,
    size: *mut usize,
) -> u16 {
    unsafe {
        *count = 0;
        *size = 0;
    }
    ERRNO_SUCCESS
}

#[unsafe(no_mangle)]
extern "C" fn __imported_wasi_snapshot_preview1_environ_get(_: *mut *mut u8, _: *mut u8) -> u16 {
    ERRNO_SUCCESS
}

#[unsafe(no_mangle)]
extern "C" fn __imported_wasi_snapshot_preview1_fd_close(_: u32) -> u16 {
    ERRNO_BADF
}

#[unsafe(no_mangle)]
extern "C" fn __imported_wasi_snapshot_preview1_fd_fdstat_get(_: u32, _: *mut u8) -> u16 {
    ERRNO_BADF
}

#[unsafe(no_mangle)]
extern "C" fn __imported_wasi_snapshot_preview1_fd_seek(_: u32, _: i64, _: u32, _: *mut u64) -> u16 {
    ERRNO_BADF
}

/// C stdio writes (e.g. libc++abi abort messages) go to the browser console.
#[unsafe(no_mangle)]
unsafe extern "C" fn __imported_wasi_snapshot_preview1_fd_write(
    _fd: u32,
    iovs: *const [usize; 2],
    iovs_len: usize,
    written: *mut usize,
) -> u16 {
    let mut total = 0;
    let mut text = String::new();
    for i in 0..iovs_len {
        let [ptr, len] = unsafe { *iovs.add(i) };
        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
        text.push_str(&String::from_utf8_lossy(bytes));
        total += len;
    }
    log::error!(target: "luau", "{}", text.trim_end());
    unsafe { *written = total };
    ERRNO_SUCCESS
}

#[unsafe(no_mangle)]
extern "C" fn __imported_wasi_snapshot_preview1_proc_exit(code: u32) -> ! {
    panic!("C code called exit({code})");
}
