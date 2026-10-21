//! A small static file server for web builds (wasm needs the right MIME type). With a game
//! folder to watch it is also the dev server for live reload: `game.kpak` is packed from the
//! folder whenever it changed, and `__kulisa/version` tells the page when that happened.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use crate::Result;

/// A hash of every game file's path, size and modification time.
fn stamp(dir: &Path) -> u64 {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).into_iter().flatten().flatten() {
            let rel =
                entry.path().strip_prefix(dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default();
            if !kulisa_platform::is_game_file(&rel) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                files.push((entry.path(), meta.len(), meta.modified().ok()));
            }
        }
    }
    files.sort();
    let mut h = DefaultHasher::new();
    files.hash(&mut h);
    h.finish()
}

/// The live game: its pack and a version that changes with it.
struct Live {
    dir: PathBuf,
    stamp: u64,
    version: u32,
    pack: Vec<u8>,
}

impl Live {
    fn refresh(&mut self) {
        let now = stamp(&self.dir);
        if now == self.stamp {
            return;
        }
        self.stamp = now;
        match kulisa_platform::build_pack(&self.dir) {
            Ok(pack) => {
                self.pack = pack;
                self.version += 1;
                eprintln!("game changed: version {}", self.version);
            }
            Err(e) => eprintln!("packing the game: {e}"),
        }
    }
}

/// Serves `dir` at `http://127.0.0.1:port/` until the process ends. With `live`, the game
/// folder is packed on the fly for live reload.
pub fn serve(dir: &Path, port: u16, live: Option<&Path>) -> Result {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let mut live = match live {
        Some(game) => {
            let pack = kulisa_platform::build_pack(game)?;
            Some(Live { dir: game.to_path_buf(), stamp: stamp(game), version: 1, pack })
        }
        None => None,
    };
    let page = if live.is_some() { "?dev=1" } else { "" };
    eprintln!("serving {} at http://127.0.0.1:{port}/{page}", dir.display());
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        let path = line.split_whitespace().nth(1).unwrap_or("/");
        let path = path.split('?').next().unwrap_or("/").trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        let body = match (path, &mut live) {
            ("__kulisa/version", Some(l)) => {
                l.refresh();
                Some(l.version.to_string().into_bytes())
            }
            ("game.kpak", Some(l)) => Some(l.pack.clone()),
            _ if path.contains("..") => None,
            _ => fs::read(dir.join(path)).ok(),
        };
        let response = match body {
            Some(body) => {
                let mime = match path.rsplit_once('.').map(|(_, e)| e) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "text/javascript",
                    Some("wasm") => "application/wasm",
                    Some("png") => "image/png",
                    Some("json") => "application/json",
                    Some("css") => "text/css",
                    _ if path.starts_with("__kulisa/") => "text/plain",
                    _ => "application/octet-stream",
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                [head.into_bytes(), body].concat()
            }
            None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        };
        let _ = stream.write_all(&response);
    }
    Ok(())
}
