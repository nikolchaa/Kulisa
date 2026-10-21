//! Watches a game folder for live reload by polling file times and sizes: no OS watcher
//! dependency, and a few hundred files take well under a millisecond to check.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use web_time::Instant;

const SCAN_EVERY: Duration = Duration::from_millis(250);
/// Editors often write a file in steps; changes are reported once things are quiet.
const SETTLE: Duration = Duration::from_millis(150);

type Stamp = (SystemTime, u64);

pub struct Watcher {
    root: PathBuf,
    stamps: HashMap<String, Stamp>,
    pending: BTreeSet<String>,
    last_scan: Instant,
    last_change: Instant,
}

fn scan(root: &Path) -> HashMap<String, Stamp> {
    let mut out = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else { continue };
            // Build output, hidden files and editor leftovers are not the game.
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !kulisa_platform::is_game_file(&rel) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else {
                out.insert(rel, (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()));
            }
        }
    }
    out
}

impl Watcher {
    pub fn new(root: PathBuf) -> Self {
        let stamps = scan(&root);
        let now = Instant::now();
        Self { root, stamps, pending: BTreeSet::new(), last_scan: now, last_change: now }
    }

    /// The game paths that changed (added, edited or removed) since the last report, once
    /// they have settled; empty otherwise.
    pub fn poll(&mut self) -> Vec<String> {
        let now = Instant::now();
        if now - self.last_scan >= SCAN_EVERY {
            self.last_scan = now;
            let stamps = scan(&self.root);
            let mut changed = false;
            for (path, stamp) in &stamps {
                if self.stamps.get(path) != Some(stamp) {
                    changed |= self.pending.insert(path.clone());
                }
            }
            for path in self.stamps.keys().filter(|p| !stamps.contains_key(*p)) {
                changed |= self.pending.insert(path.clone());
            }
            if changed {
                self.last_change = now;
            }
            self.stamps = stamps;
        }
        if self.pending.is_empty() || now - self.last_change < SETTLE {
            return Vec::new();
        }
        std::mem::take(&mut self.pending).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_settled_changes() {
        let dir = std::env::temp_dir().join(format!("kulisa-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("story")).unwrap();
        std::fs::write(dir.join("main.luau"), "-- a").unwrap();
        std::fs::write(dir.join(".hidden"), "x").unwrap();
        let mut w = Watcher::new(dir.clone());
        assert!(w.poll().is_empty());

        std::fs::write(dir.join("story/day.kulisa"), "label start:").unwrap();
        std::fs::write(dir.join("main.luau"), "-- changed").unwrap();
        std::fs::write(dir.join("notes.swp"), "x").unwrap();
        let start = Instant::now();
        let mut got = Vec::new();
        while got.is_empty() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
            got = w.poll();
        }
        assert_eq!(got, ["main.luau", "story/day.kulisa"]);

        std::fs::remove_file(dir.join("main.luau")).unwrap();
        let mut got = Vec::new();
        while got.is_empty() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
            got = w.poll();
        }
        assert_eq!(got, ["main.luau"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
