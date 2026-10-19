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

/// Packs every file under `dir` into `.kpak` bytes (build tools; native only).
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
                out.push((rel, path));
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

impl Assets {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn dir(path: impl Into<PathBuf>) -> Assets {
        Assets::Dir(path.into())
    }

    pub fn pack(bytes: Vec<u8>) -> Result<Assets, String> {
        Ok(Assets::Pack(Arc::new(Pack::parse(bytes)?)))
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

    fn game() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kulisa-assets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("story/extra")).unwrap();
        std::fs::write(dir.join("main.luau"), "print('hi')").unwrap();
        std::fs::write(dir.join("story/a.kulisa"), "label start:\n").unwrap();
        std::fs::write(dir.join("story/extra/b.kulisa"), "").unwrap();
        std::fs::write(dir.join("logo.bin"), [0u8, 159, 255]).unwrap();
        dir
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
        let mut good = build_pack(&game()).unwrap();
        good.truncate(good.len() - 2);
        assert!(Assets::pack(good).unwrap_err().contains("damaged"));
    }
}
