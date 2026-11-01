//! Writes `.tar.gz` archives. Linux and macOS programs need their "executable" mark to start,
//! which a folder on Windows cannot hold, so builds made there go out as an archive that has it.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::write::GzEncoder;

use crate::Result;

const BLOCK: usize = 512;

/// A ustar header for a file of `size` bytes.
fn header(path: &str, size: u64, mode: u32) -> Result<[u8; BLOCK]> {
    if size >= 1 << 33 {
        return Err(format!("{path}: over 8 GB, too large for an archive"));
    }
    let mut h = [0u8; BLOCK];
    // A long path splits at a folder into a prefix and a name.
    let (prefix, name) = if path.len() <= 100 {
        ("", path)
    } else {
        let split = path
            .char_indices()
            .filter(|&(i, c)| c == '/' && i <= 155 && path.len() - i - 1 <= 100)
            .map(|(i, _)| i)
            .next_back()
            .ok_or(format!("{path}: the path is too long for an archive"))?;
        (&path[..split], &path[split + 1..])
    };
    h[..name.len()].copy_from_slice(name.as_bytes());
    let octal = |h: &mut [u8; BLOCK], at: usize, len: usize, value: u64| {
        let text = format!("{value:0width$o}\0", width = len - 1);
        h[at..at + len].copy_from_slice(text.as_bytes());
    };
    octal(&mut h, 100, 8, mode as u64);
    octal(&mut h, 108, 8, 0);
    octal(&mut h, 116, 8, 0);
    octal(&mut h, 124, 12, size);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    octal(&mut h, 136, 12, now);
    h[148..156].fill(b' ');
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    let sum: u64 = h.iter().map(|&b| b as u64).sum();
    h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    Ok(h)
}

fn add_folder(
    out: &mut impl Write,
    dir: &Path,
    inside: &str,
    executable: &dyn Fn(&str) -> bool,
    error: &dyn Fn(std::io::Error) -> String,
) -> Result {
    let mut entries: Vec<_> = fs::read_dir(dir).map_err(error)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = if inside.is_empty() { name.into_owned() } else { format!("{inside}/{name}") };
        if entry.path().is_dir() {
            add_folder(out, &entry.path(), &path, executable, error)?;
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(error)?;
        let mode = if executable(&path) { 0o755 } else { 0o644 };
        out.write_all(&header(&path, bytes.len() as u64, mode)?).map_err(error)?;
        out.write_all(&bytes).map_err(error)?;
        out.write_all(&vec![0; bytes.len().next_multiple_of(BLOCK) - bytes.len()]).map_err(error)?;
    }
    Ok(())
}

/// Packs the files under `folder` into `archive`, inside a folder named `root` (if not empty).
/// Paths for which `executable` says yes can be started after unpacking.
pub fn pack(folder: &Path, root: &str, executable: &dyn Fn(&str) -> bool, archive: &Path) -> Result {
    let error = |e: std::io::Error| format!("{}: {e}", archive.display());
    if let Some(dir) = archive.parent() {
        fs::create_dir_all(dir).map_err(error)?;
    }
    let file = fs::File::create(archive).map_err(error)?;
    let mut gz = GzEncoder::new(file, Compression::default());
    add_folder(&mut gz, folder, root, executable, &error)?;
    gz.write_all(&[0; BLOCK * 2]).map_err(error)?;
    gz.finish().map_err(error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn long_paths_split_at_a_folder() {
        let long = format!("{}/{}.txt", "a".repeat(120), "b".repeat(60));
        let h = header(&long, 1, 0o644).unwrap();
        assert_eq!(&h[345..345 + 120], "a".repeat(120).as_bytes());
        assert_eq!(&h[..60], "b".repeat(60).as_bytes());
        assert!(header(&"c".repeat(300), 1, 0o644).is_err());
    }

    #[test]
    fn the_system_tar_reads_it_with_its_marks() {
        let dir = std::env::temp_dir().join(format!("kulisa-tar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("in/sub")).unwrap();
        fs::write(dir.join("in/game"), vec![7; 1000]).unwrap();
        fs::write(dir.join("in/sub/game.kpak"), b"pack").unwrap();
        let archive = dir.join("out.tar.gz");
        pack(&dir.join("in"), "Cat Tales", &|p| p == "Cat Tales/game", &archive).unwrap();

        let listed = Command::new("tar").arg("tvzf").arg("out.tar.gz").current_dir(&dir).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout).into_owned();
        let line = |name: &str| listed.lines().find(|l| l.ends_with(name)).expect(name).to_string();
        assert!(line("Cat Tales/game").starts_with("-rwxr-xr-x"), "{listed}");
        assert!(line("Cat Tales/sub/game.kpak").starts_with("-rw-r--r--"), "{listed}");

        fs::create_dir_all(dir.join("back")).unwrap();
        let status = Command::new("tar")
            .args(["xzf", "../out.tar.gz"])
            .current_dir(dir.join("back"))
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read(dir.join("back/Cat Tales/game")).unwrap(), vec![7; 1000]);
        assert_eq!(fs::read(dir.join("back/Cat Tales/sub/game.kpak")).unwrap(), b"pack");
        let _ = fs::remove_dir_all(&dir);
    }
}
