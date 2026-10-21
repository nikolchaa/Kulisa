//! Builds Kulisa games for every platform from the engine's source: the player is compiled
//! with cargo, the game is packed into a `.kpak`, and the platform's package (web page, APK,
//! iOS app, desktop folder) is assembled from the game's `kulisa.toml`. The `kulisa` command
//! and the engine's `cargo xtask` both use this.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kulisa_platform::{CONFIG_FILE, GameConfig};

pub mod android;
pub mod desktop;
pub mod icon;
pub mod ios;
pub mod serve;
pub mod web;

pub type Result<T = ()> = std::result::Result<T, String>;

/// The engine's source tree (the cargo workspace with `crates/kulisa-player`): `KULISA_ENGINE`,
/// or the tree this tool was built from.
pub fn engine_root() -> Result<PathBuf> {
    let built = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = env::var_os("KULISA_ENGINE").map(PathBuf::from).unwrap_or(built);
    if root.join("crates/kulisa-player/Cargo.toml").is_file() {
        Ok(fs::canonicalize(&root).unwrap_or(root))
    } else {
        Err(format!(
            "the Kulisa engine source was not found at {}; set KULISA_ENGINE to its folder",
            root.display()
        ))
    }
}

/// One game to build.
pub struct Game {
    pub dir: PathBuf,
    pub config: GameConfig,
}

impl Game {
    pub fn open(dir: &Path) -> Result<Game> {
        if !dir.join("main.luau").is_file() {
            return Err(format!("{} is not a Kulisa game (it has no main.luau)", dir.display()));
        }
        let source = fs::read_to_string(dir.join(CONFIG_FILE)).ok();
        let config = GameConfig::parse(source.as_deref()).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Game { dir: dir.to_path_buf(), config })
    }

    /// The game packed as `.kpak` bytes.
    pub fn pack(&self) -> Result<Vec<u8>> {
        let bytes = kulisa_platform::build_pack(&self.dir)?;
        eprintln!("packed {} ({:.1} MB)", self.dir.display(), bytes.len() as f64 / 1e6);
        Ok(bytes)
    }

    /// A file name from the title: letters, digits, spaces, `-` and `_` (`Cat Tales`).
    pub fn file_name(&self) -> String {
        let name: String = self
            .config
            .title
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .collect();
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() { self.config.id.clone() } else { name }
    }
}

/// How to build the player.
#[derive(Clone, Debug)]
pub struct Options {
    pub engine: PathBuf,
    pub release: bool,
}

impl Options {
    pub fn profile(&self) -> &'static str {
        if self.release { "release" } else { "debug" }
    }

    /// `cargo build -p kulisa-player` in the engine tree, plus `args`.
    pub fn cargo_player(&self, args: &[&str]) -> Command {
        let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cargo.current_dir(&self.engine).args(["build", "-p", "kulisa-player"]).args(args);
        if self.release {
            cargo.arg("--release");
        }
        cargo
    }
}

pub fn run(cmd: &mut Command) -> Result {
    eprintln!("> {cmd:?}");
    let status = cmd.status().map_err(|e| format!("failed to run {:?}: {e}", cmd.get_program()))?;
    if status.success() { Ok(()) } else { Err(format!("{:?} failed with {status}", cmd.get_program())) }
}

pub(crate) fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Escapes text for XML and HTML.
pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Downloads and unpacks `url` into `dir` unless `marker` (a file the archive contains)
/// already exists. A file, not a directory: partial restores must trigger a re-download.
pub(crate) fn fetch(url: &str, dir: &Path, marker: &Path) -> Result {
    if marker.exists() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let archive = dir.join("download.tar.gz");
    run(Command::new("curl").args(["-fsSL", "-o"]).arg(&archive).arg(url))?;
    run(Command::new("tar").arg("xzf").arg(&archive).arg("-C").arg(dir))?;
    fs::remove_file(&archive).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_from_titles() {
        let game = |title: &str| Game {
            dir: PathBuf::new(),
            config: GameConfig::parse(Some(&format!("[game]\nid = \"g\"\ntitle = \"{title}\""))).unwrap(),
        };
        assert_eq!(game("Whiskers & Sunbeams").file_name(), "Whiskers Sunbeams");
        assert_eq!(game("???").file_name(), "g");
        assert_eq!(escape("a < b & \"c\""), "a &lt; b &amp; &quot;c&quot;");
    }
}
