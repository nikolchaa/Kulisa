//! Named binary blobs (save files, settings) in the platform-correct place.
//!
//! Desktop and iOS: files in the OS data directory. Android: the app's internal storage,
//! set by the runtime at startup. Web: `localStorage`, base64-encoded.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

/// Stores blobs for one game.
pub struct Storage {
    #[cfg(not(target_arch = "wasm32"))]
    dir: PathBuf,
    #[cfg(target_arch = "wasm32")]
    prefix: String,
}

/// Names become file names: keep them simple so a slot can never point outside its folder.
fn check_name(name: &str) -> Result<(), String> {
    let lower = name.to_ascii_lowercase();
    // Device names Windows reserves regardless of folder.
    let reserved = ["con", "prn", "aux", "nul"].contains(&lower.as_str())
        || (lower.len() == 4
            && (lower.starts_with("com") || lower.starts_with("lpt"))
            && lower.as_bytes()[3].is_ascii_digit());
    let ok = !name.is_empty()
        && name.len() <= 64
        && !reserved
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!("`{name}` is not a valid storage name (use letters, digits, `_` and `-`)"))
    }
}

#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Android only: the app's internal data directory (`AndroidApp::internal_data_path`).
#[cfg(target_os = "android")]
pub fn set_android_data_dir(path: PathBuf) {
    let _ = ANDROID_DATA_DIR.set(path);
}

/// Where a game keeps its data on this platform.
#[cfg(not(target_arch = "wasm32"))]
pub fn data_dir(game_id: &str) -> Result<PathBuf, String> {
    use std::env::var_os;
    let base: Option<PathBuf> = if cfg!(target_os = "android") {
        #[cfg(target_os = "android")]
        {
            ANDROID_DATA_DIR.get().cloned()
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    } else if cfg!(windows) {
        var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join(game_id)).ok_or_else(|| "cannot find a data directory on this system".into())
}

impl Storage {
    /// Storage under the game's data directory, in `folder` (e.g. `saves`).
    pub fn new(game_id: &str, folder: &str) -> Result<Storage, String> {
        check_name(game_id)?;
        check_name(folder)?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(Storage::at(data_dir(game_id)?.join(folder)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Storage { prefix: format!("kulisa/{game_id}/{folder}/") })
        }
    }

    /// Storage in an explicit directory (tests, tools).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn at(dir: PathBuf) -> Storage {
        Storage { dir }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Writes atomically: a crash mid-write never leaves a half-written save.
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        check_name(name)?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
            let tmp = self.dir.join(format!("{name}.tmp"));
            std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
            std::fs::rename(&tmp, self.dir.join(name)).map_err(|e| format!("saving `{name}`: {e}"))
        }
        #[cfg(target_arch = "wasm32")]
        {
            web::local_storage()?
                .set_item(&format!("{}{name}", self.prefix), &base64::encode(bytes))
                .map_err(|_| format!("browser storage refused `{name}` (full or disabled?)"))
        }
    }

    /// Reads a blob; `None` if it does not exist.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        check_name(name)?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            match std::fs::read(self.dir.join(name)) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("reading `{name}`: {e}")),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            match web::local_storage()?.get_item(&format!("{}{name}", self.prefix)) {
                Ok(Some(s)) => base64::decode(&s).map(Some).ok_or_else(|| format!("`{name}` is damaged")),
                _ => Ok(None),
            }
        }
    }

    /// Names of stored blobs, sorted.
    pub fn list(&self) -> Vec<String> {
        #[cfg(not(target_arch = "wasm32"))]
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| check_name(n).is_ok())
            .collect();
        #[cfg(target_arch = "wasm32")]
        let mut names: Vec<String> = web::local_storage()
            .ok()
            .map(|s| {
                let len = s.length().unwrap_or(0);
                (0..len)
                    .filter_map(|i| s.key(i).ok().flatten())
                    .filter_map(|k| k.strip_prefix(&self.prefix).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn delete(&self, name: &str) -> Result<(), String> {
        check_name(name)?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            match std::fs::remove_file(self.dir.join(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("deleting `{name}`: {e}")),
                _ => Ok(()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            web::local_storage()?
                .remove_item(&format!("{}{name}", self.prefix))
                .map_err(|_| format!("deleting `{name}` failed"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    pub fn local_storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| "browser storage is not available (private mode?)".to_string())
    }
}

/// Base64 for text-only storage (the web).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod base64 {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(s: &str) -> Option<Vec<u8>> {
        let s = s.trim_end_matches('=');
        let mut out = Vec::with_capacity(s.len() * 3 / 4);
        let (mut acc, mut bits) = (0u32, 0);
        for c in s.bytes() {
            acc = acc << 6 | ABC.iter().position(|&a| a == c)? as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        Some(out)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn temp() -> Storage {
        let dir = std::env::temp_dir().join(format!("kulisa-storage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Storage::at(dir)
    }

    #[test]
    fn writes_reads_lists_and_deletes() {
        let s = temp();
        assert_eq!(s.read("quick").unwrap(), None);
        s.write("quick", b"one").unwrap();
        s.write("slot-1", b"two").unwrap();
        s.write("quick", b"three").unwrap();
        assert_eq!(s.read("quick").unwrap().as_deref(), Some(&b"three"[..]));
        assert_eq!(s.list(), ["quick", "slot-1"], "no leftover temp files");
        s.delete("quick").unwrap();
        s.delete("quick").unwrap();
        assert_eq!(s.list(), ["slot-1"]);
        let _ = std::fs::remove_dir_all(s.dir());
    }

    #[test]
    fn refuses_unsafe_names() {
        let s = temp();
        for bad in ["../evil", "a/b", "", "con.txt", "CON", "lpt1", &"x".repeat(65)] {
            assert!(s.write(bad, b"x").is_err(), "{bad}");
        }
        assert!(Storage::new("../x", "saves").is_err());
    }

    #[test]
    fn base64_roundtrips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0, 255, 128, 7, 1]] {
            assert_eq!(base64::decode(&base64::encode(data)).unwrap(), data);
        }
        assert_eq!(base64::encode(b"Kulisa"), "S3VsaXNh");
    }

    #[test]
    fn data_dir_is_per_game() {
        if let Ok(d) = data_dir("my_game") {
            assert!(d.ends_with("my_game"));
        }
    }
}
