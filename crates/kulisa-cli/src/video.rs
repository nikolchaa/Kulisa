//! Videos in builds. Most platforms play the game's files as they are, so builds copy them beside
//! the pack. Linux plays what FFmpeg converts them into here, on the developer's computer.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, time::SystemTime};

use crate::{Game, Result};

/// Encoders to try, best first, with the options that make them quick enough for a build.
const ENCODERS: &[(&str, &[&str])] = &[
    ("libsvtav1", &["-preset", "8", "-crf", "32"]),
    ("libaom-av1", &["-cpu-used", "6", "-row-mt", "1", "-crf", "32", "-b:v", "0"]),
    ("librav1e", &["-speed", "6", "-qp", "90"]),
];

/// A key frame this often lets seeking and loops land quickly.
const KEY_FRAME_EVERY: &str = "120";
const NO_FFMPEG: &str = "Linux builds convert videos with FFmpeg, which was not found. Install it \
(https://ffmpeg.org/download.html) and build again, or remove the videos from the game.";

/// Copies the game's videos under `to`, at their game paths.
pub(crate) fn copy(game: &Game, to: &Path) -> Result {
    for (rel, from) in kulisa_platform::game_videos(&game.dir)? {
        let target = to.join(&rel);
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::copy(&from, &target).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    Ok(())
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn ffmpeg() -> Command {
    Command::new("ffmpeg")
}

/// The first AV1 encoder this FFmpeg has.
fn encoder() -> Result<(&'static str, &'static [&'static str])> {
    let out = ffmpeg().args(["-hide_banner", "-encoders"]).output().map_err(|_| NO_FFMPEG)?;
    let list = String::from_utf8_lossy(&out.stdout);
    ENCODERS.iter().find(|(name, _)| list.split_whitespace().any(|word| word == *name)).copied().ok_or_else(
        || "this FFmpeg has no AV1 encoder (libsvtav1, libaom or rav1e), which Linux builds need".into(),
    )
}

fn has_sound(video: &Path) -> Result<bool> {
    // With no output file FFmpeg describes the input and exits with an error, which is fine here.
    let out = ffmpeg().arg("-hide_banner").arg("-i").arg(video).output().map_err(|_| NO_FFMPEG)?;
    Ok(String::from_utf8_lossy(&out.stderr).contains("Audio:"))
}

fn run_ffmpeg(args: &mut Command, what: &Path) -> Result {
    let out = args.output().map_err(|_| NO_FFMPEG)?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(4).collect();
    Err(format!("converting {}: {}", what.display(), tail.into_iter().rev().collect::<Vec<_>>().join("\n")))
}

/// Converts the game's videos into `.video-cache` in its folder (a hidden folder, so it stays
/// out of the pack), skipping those that have not changed. Returns each game path's converted
/// picture file.
pub fn convert(game: &Game) -> Result<Vec<(String, PathBuf)>> {
    let videos = kulisa_platform::game_videos(&game.dir)?;
    let mut converted = Vec::new();
    let cache = game.dir.join(kulisa_platform::VIDEO_CACHE);
    for (rel, from) in videos {
        let picture = cache.join(&rel).with_extension("ivf");
        let sound = picture.with_extension("ogg");
        converted.push((rel.clone(), picture.clone()));
        if let (Some(have), Some(source)) = (modified(&picture), modified(&from))
            && have >= source
        {
            continue;
        }
        if let Some(dir) = picture.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let (name, options) = encoder()?;
        eprintln!("converting {rel} for Linux with {name}...");
        // Written under another name first, so a stopped build leaves nothing that looks finished.
        let partial = picture.with_extension("part.ivf");
        run_ffmpeg(
            ffmpeg()
                .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
                .arg(&from)
                .args(["-an", "-pix_fmt", "yuv420p", "-c:v", name])
                .args(options)
                .args(["-g", KEY_FRAME_EVERY])
                .arg(&partial),
            &from,
        )?;
        let _ = fs::remove_file(&sound);
        if has_sound(&from)? {
            run_ffmpeg(
                ffmpeg()
                    .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
                    .arg(&from)
                    .args(["-vn", "-c:a", "libvorbis", "-q:a", "5"])
                    .arg(&sound),
                &from,
            )?;
        }
        fs::rename(&partial, &picture).map_err(|e| format!("{}: {e}", picture.display()))?;
    }
    Ok(converted)
}

/// Converts the game's videos and copies the results under `to`, at their game paths.
pub(crate) fn copy_converted(game: &Game, to: &Path) -> Result {
    for (rel, picture) in convert(game)? {
        let target = to.join(&rel).with_extension("ivf");
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        fs::copy(&picture, &target).map_err(|e| format!("{}: {e}", picture.display()))?;
        let sound = picture.with_extension("ogg");
        if sound.is_file() {
            fs::copy(&sound, target.with_extension("ogg"))
                .map_err(|e| format!("{}: {e}", sound.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_with_clip(name: &str) -> Game {
        let dir = std::env::temp_dir().join(format!("kulisa-video-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("movies")).unwrap();
        let clip = Path::new(env!("CARGO_MANIFEST_DIR")).join("../kulisa-video/tests/data/clip.mp4");
        fs::copy(clip, dir.join("movies/clip.mp4")).unwrap();
        fs::write(dir.join("main.luau"), "").unwrap();
        Game::open(&dir).unwrap()
    }

    #[test]
    fn builds_copy_videos_beside_the_pack() {
        let game = game_with_clip("copy");
        let out = game.dir.join("dist/x");
        copy(&game, &out).unwrap();
        assert!(out.join("movies/clip.mp4").is_file());
        let _ = fs::remove_dir_all(&game.dir);
    }

    #[test]
    fn converting_makes_picture_and_sound_once() {
        if ffmpeg().arg("-version").output().is_err() || encoder().is_err() {
            eprintln!("skipping: no FFmpeg with an AV1 encoder");
            return;
        }
        let game = game_with_clip("convert");
        let out = game.dir.join("dist/linux");
        copy_converted(&game, &out).unwrap();
        let picture = out.join("movies/clip.ivf");
        assert_eq!(&fs::read(&picture).unwrap()[8..12], b"AV01");
        assert!(out.join("movies/clip.ogg").is_file());

        // Unchanged videos are not converted again.
        let cached = game.dir.join(kulisa_platform::VIDEO_CACHE).join("movies/clip.ivf");
        let before = modified(&cached).unwrap();
        convert(&game).unwrap();
        assert_eq!(modified(&cached).unwrap(), before);
        assert!(kulisa_platform::is_game_file("main.luau"));
        assert!(!kulisa_platform::is_game_file(&format!("{}/movies/clip.ivf", kulisa_platform::VIDEO_CACHE)));
        let _ = fs::remove_dir_all(&game.dir);
    }
}
