//! Builds Kulisa games for every platform: the player is a ready-made one or compiled from the
//! engine's source (see [`players`]), the game is packed into a `.kpak`, and the platform's
//! package (web page, APK, iOS app, desktop folder) is assembled from the game's `kulisa.toml`.
//! The `kulisa` command and the engine's `cargo xtask` both use this.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kulisa_platform::{CONFIG_FILE, GameConfig};

pub mod android;
pub mod desktop;
pub mod icon;
pub mod ios;
pub mod macos;
mod pe;
pub mod players;
pub mod serve;
mod tar;
mod video;
pub use video::convert as convert_videos;
pub mod web;

pub type Result<T = ()> = std::result::Result<T, String>;

/// Where this tool gets its players and templates.
#[derive(Clone, Debug)]
pub enum Engine {
    /// The engine's source tree (the cargo workspace with `crates/kulisa-player`): players are
    /// compiled.
    Source(PathBuf),
    /// The folder of a released tool: players are ready-made.
    Released(PathBuf),
}

impl Engine {
    /// `KULISA_ENGINE`, the release this tool came in (its templates are beside it), or the tree
    /// it was built from.
    pub fn locate() -> Result<Engine> {
        if let Some(root) = env::var_os("KULISA_ENGINE") {
            return Self::source(root.into());
        }
        let exe = env::current_exe().and_then(fs::canonicalize).map(strip_verbatim);
        if let Some(dir) = exe.ok().and_then(|e| e.parent().map(Path::to_path_buf))
            && dir.join("templates").is_dir()
        {
            return Ok(Engine::Released(dir));
        }
        Self::source(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    fn source(root: PathBuf) -> Result<Engine> {
        if root.join("crates/kulisa-player/Cargo.toml").is_file() {
            Ok(Engine::Source(strip_verbatim(fs::canonicalize(&root).unwrap_or(root))))
        } else {
            Err(format!(
                "the Kulisa engine source was not found at {}; set KULISA_ENGINE to its folder",
                root.display()
            ))
        }
    }

    pub fn templates(&self) -> PathBuf {
        match self {
            Engine::Source(root) | Engine::Released(root) => root.join("templates"),
        }
    }
}

/// Windows `canonicalize` gives `\\?\C:\...`, which some tools (cargo, aapt) refuse.
pub fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p,
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

/// The cargo profile players are compiled with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    Debug,
    Release,
    /// What releases ship: smaller and slower to build.
    Dist,
}

/// How to build the player.
#[derive(Clone, Debug)]
pub struct Options {
    pub engine: Engine,
    pub profile: Profile,
}

impl Options {
    /// The folder under `target` that the profile builds into.
    pub fn profile_dir(&self) -> &'static str {
        match self.profile {
            Profile::Debug => "debug",
            Profile::Release => "release",
            Profile::Dist => "dist",
        }
    }

    /// Whether this build is for players of the game, not for working on it.
    pub fn shipping(&self) -> bool {
        self.profile != Profile::Debug || matches!(self.engine, Engine::Released(_))
    }

    /// `cargo build -p package` in the engine tree `root`, plus `args`.
    pub fn cargo(&self, root: &Path, package: &str, args: &[&str]) -> Command {
        let mut cargo = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cargo.current_dir(root).args(["build", "-p", package]).args(args);
        match self.profile {
            Profile::Debug => {}
            Profile::Release => {
                cargo.arg("--release");
            }
            Profile::Dist => {
                cargo.args(["--profile", "dist"]);
            }
        }
        cargo
    }

    /// `cargo build -p kulisa-player` in the engine tree `root`, plus `args`.
    pub fn cargo_player(&self, root: &Path, args: &[&str]) -> Command {
        self.cargo(root, "kulisa-player", args)
    }

    /// A folder for what a build makes along the way.
    pub fn work_dir(&self, name: &str) -> Result<PathBuf> {
        Ok(match &self.engine {
            Engine::Source(root) => root.join("target").join(name),
            Engine::Released(_) => players::cache_dir()?.join("work").join(name),
        })
    }

    /// A folder for tools a build downloads.
    pub fn tools_dir(&self) -> Result<PathBuf> {
        Ok(match &self.engine {
            Engine::Source(root) => root.join(".tools"),
            Engine::Released(_) => players::cache_dir()?.join("tools"),
        })
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

/// Copies the files under `from` into `to`, keeping their folders.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Copies the game files under `from` into `to`, leaving out builds and hidden files.
pub fn copy_game(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten() {
        let name = entry.file_name();
        if !kulisa_platform::is_game_file(&name.to_string_lossy()) {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            copy_game(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
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
