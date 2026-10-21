//! Game files: a folder during development, a single `.kpak` file when shipped (inside the
//! APK, the iOS app, or fetched once on the web).
//!
//! Paths are relative, `/`-separated and may not leave the game (`..`, absolute paths).
//!
//! `.kpak` layout: `"KPAK" | version u32 | count u32 | count × (path_len u16, path,
//! offset u64, len u64) | data`, all little-endian; offsets are relative to `data`.

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAGIC: &[u8; 4] = b"KPAK";
const VERSION: u32 = 1;

/// A game's files.
#[derive(Clone)]
pub enum Assets {
    #[cfg(not(target_arch = "wasm32"))]
    Dir(PathBuf),
    Pack(Arc<Pack>),
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Assets::Dir(dir) => write!(f, "Assets::Dir({})", dir.display()),
            Assets::Pack(pack) => write!(f, "Assets::Pack({} files)", pack.index.len()),
        }
    }
}

/// An in-memory `.kpak`.
pub struct Pack {
    index: BTreeMap<String, (usize, usize)>,
    data: Vec<u8>,
}

/// Normalizes a game path, refusing anything that could escape the game.
pub fn normalize(path: &str) -> Result<String, String> {
    let p = path.replace('\\', "/");
    if p.starts_with('/') || p.contains(':') {
        return Err(format!("`{path}`: asset paths are relative to the game folder"));
    }
    let mut parts = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("`{path}`: asset paths cannot use `..`")),
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return Err(format!("`{path}` is not a file path"));
    }
    Ok(parts.join("/"))
}

impl Pack {
    pub fn parse(bytes: Vec<u8>) -> Result<Pack, String> {
        let err = || "game pack is damaged".to_string();
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let s = bytes.get(at..at + n).ok_or_else(err)?;
            at += n;
            Ok(s)
        };
        if take(4)? != MAGIC {
            return Err("not a Kulisa game pack".into());
        }
        let version = u32::from_le_bytes(take(4)?.try_into().unwrap());
        if version > VERSION {
            return Err(format!("game pack version {version} needs a newer Kulisa"));
        }
        let count = u32::from_le_bytes(take(4)?.try_into().unwrap());
        let mut entries = Vec::new();
        for _ in 0..count {
            let len = u16::from_le_bytes(take(2)?.try_into().unwrap()) as usize;
            let path = String::from_utf8(take(len)?.to_vec()).map_err(|_| err())?;
            let offset = u64::from_le_bytes(take(8)?.try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(take(8)?.try_into().unwrap()) as usize;
            entries.push((path, offset, size));
        }
        let data_start = at;
        let mut index = BTreeMap::new();
        for (path, offset, size) in entries {
            let start = data_start.checked_add(offset).ok_or_else(err)?;
            if start.checked_add(size).is_none_or(|end| end > bytes.len()) {
                return Err(err());
            }
            index.insert(path, (start, size));
        }
        Ok(Pack { index, data: bytes })
    }

    fn get(&self, path: &str) -> Option<&[u8]> {
        self.index.get(path).map(|&(start, len)| &self.data[start..start + len])
    }
}

/// Whether a path in a game folder is part of the game: build output (`dist/`), hidden
/// files (`.git`, `.DS_Store`) and editor leftovers (`~`, `.swp`, `.tmp`) are not.
pub fn is_game_file(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let first = parts.clone().next();
    if first == Some("dist") {
        return false;
    }
    !parts.any(|p| p.starts_with('.') || p.ends_with('~') || p.ends_with(".swp") || p.ends_with(".tmp"))
}

/// Packs every game file under `dir` ([`is_game_file`]) into `.kpak` bytes (build tools;
/// native only).
#[cfg(not(target_arch = "wasm32"))]
pub fn build_pack(dir: &Path) -> Result<Vec<u8>, String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else {
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                if is_game_file(&rel) {
                    out.push((rel, path));
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut header = MAGIC.to_vec();
    header.extend_from_slice(&VERSION.to_le_bytes());
    header.extend_from_slice(&(files.len() as u32).to_le_bytes());
    let mut data = Vec::new();
    for (rel, path) in &files {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = rel.as_bytes();
        let len = u16::try_from(name.len()).map_err(|_| format!("`{rel}`: path too long"))?;
        header.extend_from_slice(&len.to_le_bytes());
        header.extend_from_slice(name);
        header.extend_from_slice(&(data.len() as u64).to_le_bytes());
        header.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        data.extend_from_slice(&bytes);
    }
    header.extend_from_slice(&data);
    Ok(header)
}

/// Downloads `url` (relative to the page). Web only; games ship as a `.kpak` there.
#[cfg(target_arch = "wasm32")]
pub async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let fail = |e: wasm_bindgen::JsValue| format!("fetching `{url}`: {e:?}");
    let window = web_sys::window().ok_or("no browser window")?;
    let response: web_sys::Response =
        JsFuture::from(window.fetch_with_str(url)).await.map_err(fail)?.dyn_into().map_err(fail)?;
    if !response.ok() {
        return Err(format!("fetching `{url}`: HTTP {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(fail)?).await.map_err(fail)?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

impl Assets {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn dir(path: impl Into<PathBuf>) -> Assets {
        Assets::Dir(path.into())
    }

    pub fn pack(bytes: Vec<u8>) -> Result<Assets, String> {
        Ok(Assets::Pack(Arc::new(Pack::parse(bytes)?)))
    }

    /// No files (a game that could not be found still gets an error view).
    pub fn empty() -> Assets {
        Assets::Pack(Arc::new(Pack { index: BTreeMap::new(), data: Vec::new() }))
    }

    /// The paths whose contents differ between two packs, including added and removed files
    /// (live reload on the web). Folders compare as unchanged.
    pub fn changed_since(&self, older: &Assets) -> Vec<String> {
        let (Assets::Pack(new), Assets::Pack(old)) = (self, older) else { return Vec::new() };
        let mut paths: Vec<String> = new.index.keys().chain(old.index.keys()).cloned().collect();
        paths.sort();
        paths.dedup();
        paths.retain(|p| new.get(p) != old.get(p));
        paths
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let p = normalize(path)?;
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Assets::Dir(dir) => std::fs::read(dir.join(&p)).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => format!("no asset `{p}` in the game folder"),
                _ => format!("reading `{p}`: {e}"),
            }),
            Assets::Pack(pack) => {
                pack.get(&p).map(<[u8]>::to_vec).ok_or_else(|| format!("no asset `{p}` in the game"))
            }
        }
    }

    pub fn read_string(&self, path: &str) -> Result<String, String> {
        String::from_utf8(self.read(path)?).map_err(|_| format!("`{path}` is not UTF-8 text"))
    }

    pub fn exists(&self, path: &str) -> bool {
        let Ok(p) = normalize(path) else { return false };
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Assets::Dir(dir) => dir.join(&p).is_file(),
            Assets::Pack(pack) => pack.index.contains_key(&p),
        }
    }

    /// Files under `dir` (recursively), as game paths, sorted.
    pub fn list(&self, dir: &str) -> Vec<String> {
        let prefix = normalize(dir).map(|d| format!("{d}/")).unwrap_or_default();
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Assets::Dir(root) => {
                let mut out = Vec::new();
                let mut stack = vec![root.join(&prefix)];
                while let Some(d) = stack.pop() {
                    for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if let Ok(rel) = path.strip_prefix(root) {
                            out.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
                out.sort();
                out
            }
            Assets::Pack(pack) => pack.index.keys().filter(|k| k.starts_with(&prefix)).cloned().collect(),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// A fresh game folder per call: tests run in parallel.
    fn game() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kulisa-assets-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("story/extra")).unwrap();
        std::fs::write(dir.join("main.luau"), "print('hi')").unwrap();
        std::fs::write(dir.join("story/a.kulisa"), "label start:\n").unwrap();
        std::fs::write(dir.join("story/extra/b.kulisa"), "").unwrap();
        std::fs::write(dir.join("logo.bin"), [0u8, 159, 255]).unwrap();
        dir
    }

    #[test]
    fn packs_leave_out_build_output_and_hidden_files() {
        let dir = game();
        std::fs::create_dir_all(dir.join("dist/web")).unwrap();
        std::fs::write(dir.join("dist/web/game.kpak"), "x").unwrap();
        std::fs::write(dir.join(".DS_Store"), "x").unwrap();
        std::fs::write(dir.join("story/a.kulisa~"), "x").unwrap();
        let pack = Assets::pack(build_pack(&dir).unwrap()).unwrap();
        assert_eq!(pack.list(""), ["logo.bin", "main.luau", "story/a.kulisa", "story/extra/b.kulisa"]);
        assert!(is_game_file("story/dist/x.png"), "only the top-level dist folder is build output");
        assert!(!is_game_file("images/.thumbs/x.png"));

        // Live reload on the web compares packs.
        std::fs::write(dir.join("main.luau"), "print('bye')").unwrap();
        std::fs::write(dir.join("new.txt"), "n").unwrap();
        let newer = Assets::pack(build_pack(&dir).unwrap()).unwrap();
        assert_eq!(newer.changed_since(&pack), ["main.luau", "new.txt"]);
    }

    #[test]
    fn folder_and_pack_agree() {
        let dir = game();
        let folder = Assets::dir(&dir);
        let pack = Assets::pack(build_pack(&dir).unwrap()).unwrap();
        for a in [&folder, &pack] {
            assert_eq!(a.read_string("main.luau").unwrap(), "print('hi')");
            assert_eq!(a.read("./story\\a.kulisa").unwrap(), b"label start:\n");
            assert_eq!(a.read("logo.bin").unwrap(), vec![0, 159, 255]);
            assert!(a.exists("story/extra/b.kulisa"));
            assert!(!a.exists("nope"));
            assert_eq!(a.list("story"), ["story/a.kulisa", "story/extra/b.kulisa"]);
            assert!(a.read("missing.png").unwrap_err().contains("no asset `missing.png`"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paths_cannot_escape_the_game() {
        assert_eq!(normalize("a//b/./c").unwrap(), "a/b/c");
        for bad in ["../secret", "a/../../b", "/etc/passwd", "C:/Windows", "", "./"] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_damaged_packs() {
        assert!(Assets::pack(b"PK\x03\x04".to_vec()).is_err());
        let dir = game();
        let mut good = build_pack(&dir).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        good.truncate(good.len() - 2);
        assert!(Assets::pack(good).unwrap_err().contains("damaged"));
    }
}
