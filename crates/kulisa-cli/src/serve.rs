//! A small static file server for web builds (wasm needs the right MIME type). With a game
//! folder to watch it is also the dev server for live reload: `game.kpak` is packed from the
//! folder whenever it changed, and `__kulisa/version` tells the page when that happened.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// `bytes=START-` or `bytes=START-END`.
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()))
}

/// A video file or the piece asked for, which browsers need to seek.
fn video_response(path: &Path, range: Option<(u64, Option<u64>)>) -> Vec<u8> {
    /// Browsers ask for more as they play.
    const PIECE: u64 = 4 << 20;
    const NOT_FOUND: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let Ok(mut file) = fs::File::open(path) else { return NOT_FOUND.to_vec() };
    let Ok(total) = file.metadata().map(|m| m.len()) else { return NOT_FOUND.to_vec() };
    let (start, wanted) = range.unwrap_or((0, None));
    let last = wanted.unwrap_or(u64::MAX).min(total.saturating_sub(1)).min(start.saturating_add(PIECE - 1));
    let mime = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("webm")) {
        "video/webm"
    } else {
        "video/mp4"
    };
    let mut body = vec![0; (last + 1).saturating_sub(start) as usize];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
        return NOT_FOUND.to_vec();
    }
    let head = format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{last}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    [head.into_bytes(), body].concat()
}

/// Serves `dir` at `http://127.0.0.1:port/` until the process ends. With `live`, the game
/// folder is packed on the fly for live reload.
pub fn serve(dir: &Path, port: u16, live: Option<&Path>) -> Result {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let live = match live {
        Some(game) => {
            let pack = kulisa_platform::build_pack(game)?;
            Some(Live { dir: game.to_path_buf(), stamp: stamp(game), version: 1, pack })
        }
        None => None,
    };
    let page = if live.is_some() { "?dev=1" } else { "" };
    eprintln!("serving {} at http://127.0.0.1:{port}/{page}", dir.display());
    let live = Arc::new(Mutex::new(live));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let (dir, live) = (dir.to_path_buf(), live.clone());
        // A connection of its own: browsers read videos slowly and can leave others idle.
        std::thread::spawn(move || answer(stream, &dir, &live));
    }
    Ok(())
}

fn answer(mut stream: TcpStream, dir: &Path, live: &Mutex<Option<Live>>) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut range = None;
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("range:") {
            range = parse_range(value.trim());
        }
        header.clear();
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/").trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let mut live = live.lock().unwrap();
    if kulisa_platform::is_video(path) && !path.contains("..") {
        let from = live.as_ref().map_or(dir, |l| l.dir.as_path()).join(path);
        drop(live);
        let _ = stream.write_all(&video_response(&from, range));
        return;
    }
    let body = match (path, &mut *live) {
        ("__kulisa/version", Some(l)) => {
            l.refresh();
            Some(l.version.to_string().into_bytes())
        }
        ("game.kpak", Some(l)) => Some(l.pack.clone()),
        _ if path.contains("..") => None,
        _ => fs::read(dir.join(path)).ok(),
    };
    drop(live);
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
