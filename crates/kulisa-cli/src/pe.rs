//! Finishes a Windows player without compiling or a Windows-only tool, so any computer can build
//! a game for Windows: adds the game's icon as a new resource section and picks whether the
//! program opens a console window.

use crate::Result;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
/// US English, which resource compilers use by default.
const LANGUAGE: u32 = 0x0409;
const GUI_SUBSYSTEM: u16 = 2;
/// Initialized, readable data.
const RSRC_FLAGS: u32 = 0x4000_0040;
const SECTION_HEADER: usize = 40;

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    let bytes = b.get(at..at + 2).ok_or("not a Windows program: it ends too early")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    let bytes = b.get(at..at + 4).ok_or("not a Windows program: it ends too early")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn put_u16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn align(value: u32, to: u32) -> u32 {
    value.div_ceil(to) * to
}

/// The places in the file a change needs.
struct Headers {
    /// Where the COFF header starts.
    coff: usize,
    /// Where the optional header starts.
    optional: usize,
    sections: usize,
    count: usize,
}

fn headers(b: &[u8]) -> Result<Headers> {
    if b.get(..2) != Some(b"MZ") {
        return Err("not a Windows program".into());
    }
    let pe = u32_at(b, 0x3c)? as usize;
    if b.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("not a Windows program".into());
    }
    let coff = pe + 4;
    let optional = coff + 20;
    if u16_at(b, optional)? != 0x20b {
        return Err("not a 64-bit Windows program".into());
    }
    let size = u16_at(b, coff + 16)? as usize;
    Ok(Headers { coff, optional, sections: optional + size, count: u16_at(b, coff + 2)? as usize })
}

/// Makes the program open no console window. Standard output and errors then go nowhere, which
/// is what a game wants; `kulisa run` keeps the console.
pub fn hide_console(exe: &mut [u8]) -> Result {
    let h = headers(exe)?;
    put_u16(exe, h.optional + 68, GUI_SUBSYSTEM);
    Ok(())
}

/// The bytes of a resource section at `rva`: icon images under RT_ICON 1, 2, ... and the group
/// that lists them under RT_GROUP_ICON 1.
fn resources(images: &[(u32, Vec<u8>)], rva: u32) -> Vec<u8> {
    // A group entry is an icon directory entry that ends in the image's id, not its offset.
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (i, (size, data)) in images.iter().enumerate() {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(data.len() as u32).to_le_bytes());
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }

    // Every leaf, as (type, id, data), sorted as the format asks.
    let mut leaves: Vec<(u32, u32, &[u8])> = Vec::new();
    for (i, (_, data)) in images.iter().enumerate() {
        leaves.push((RT_ICON, i as u32 + 1, data));
    }
    leaves.push((RT_GROUP_ICON, 1, &group));
    let types = [(RT_ICON, images.len()), (RT_GROUP_ICON, 1)];

    // Three levels of directories (type, id, language): the sizes come first, as a directory is
    // 16 bytes plus 8 for each entry.
    const DIRECTORY: usize = 16;
    const ENTRY: usize = 8;
    let root = DIRECTORY + ENTRY * types.len();
    let names: usize = types.iter().map(|(_, n)| DIRECTORY + ENTRY * n).sum();
    let languages = (DIRECTORY + ENTRY) * leaves.len();
    let entries = root + names + languages;
    let mut data_at = entries + 16 * leaves.len();

    let mut out = Vec::new();
    let directory = |out: &mut Vec<u8>, count: usize| {
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(count as u16).to_le_bytes());
    };
    let subdirectory = |out: &mut Vec<u8>, id: u32, at: usize| {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(at as u32 | 0x8000_0000).to_le_bytes());
    };

    directory(&mut out, types.len());
    let mut name_at = root;
    for (kind, count) in types {
        subdirectory(&mut out, kind, name_at);
        name_at += DIRECTORY + ENTRY * count;
    }
    let mut language_at = root + names;
    let mut index = 0;
    for (_, count) in types {
        directory(&mut out, count);
        for _ in 0..count {
            subdirectory(&mut out, leaves[index].1, language_at);
            language_at += DIRECTORY + ENTRY;
            index += 1;
        }
    }
    for i in 0..leaves.len() {
        directory(&mut out, 1);
        out.extend_from_slice(&LANGUAGE.to_le_bytes());
        out.extend_from_slice(&((entries + 16 * i) as u32).to_le_bytes());
    }
    for (_, _, data) in &leaves {
        out.extend_from_slice(&(rva + data_at as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        data_at = align(data_at as u32 + data.len() as u32, 8) as usize;
    }
    for (_, _, data) in &leaves {
        out.extend_from_slice(data);
        out.resize(align(out.len() as u32, 8) as usize, 0);
    }
    out
}

/// Adds `images` (square size, PNG data) to the program as its icon.
pub fn set_icon(exe: &mut Vec<u8>, images: &[(u32, Vec<u8>)]) -> Result {
    let h = headers(exe)?;
    let section_align = u32_at(exe, h.optional + 32)?;
    let file_align = u32_at(exe, h.optional + 36)?;
    let headers_size = u32_at(exe, h.optional + 60)? as usize;
    // The third entry of the data directories.
    let resource_dir = h.optional + 112 + 2 * 8;
    if u32_at(exe, resource_dir)? != 0 {
        return Err("this program already has resources".into());
    }
    let table_end = h.sections + h.count * SECTION_HEADER;
    if table_end + SECTION_HEADER > headers_size {
        return Err("this program has no room for another section".into());
    }

    let (mut virtual_end, mut raw_end) = (0, 0);
    for i in 0..h.count {
        let s = h.sections + i * SECTION_HEADER;
        let size = u32_at(exe, s + 8)?.max(u32_at(exe, s + 16)?);
        virtual_end = virtual_end.max(u32_at(exe, s + 12)? + size);
        raw_end = raw_end.max(u32_at(exe, s + 20)? + u32_at(exe, s + 16)?);
    }
    if exe.len() != raw_end as usize {
        return Err("this program has data after its last section".into());
    }

    let rva = align(virtual_end, section_align);
    let body = resources(images, rva);
    let raw_size = align(body.len() as u32, file_align);

    let s = table_end;
    exe[s..s + 8].copy_from_slice(b".rsrc\0\0\0");
    put_u32(exe, s + 8, body.len() as u32);
    put_u32(exe, s + 12, rva);
    put_u32(exe, s + 16, raw_size);
    put_u32(exe, s + 20, raw_end);
    exe[s + 24..s + 36].fill(0);
    put_u32(exe, s + 36, RSRC_FLAGS);

    put_u16(exe, h.coff + 2, h.count as u16 + 1);
    let initialized = u32_at(exe, h.optional + 8)?;
    put_u32(exe, h.optional + 8, initialized + raw_size);
    put_u32(exe, h.optional + 56, align(rva + body.len() as u32, section_align));
    put_u32(exe, resource_dir, rva);
    put_u32(exe, resource_dir + 4, body.len() as u32);

    exe.extend_from_slice(&body);
    exe.resize(raw_end as usize + raw_size as usize, 0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program with one section, enough for the headers to make sense.
    fn program() -> Vec<u8> {
        let mut b = vec![0u8; 0x600];
        b[..2].copy_from_slice(b"MZ");
        put_u32(&mut b, 0x3c, 0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let (coff, optional) = (0x84, 0x98);
        put_u16(&mut b, coff + 2, 1);
        put_u16(&mut b, coff + 16, 240);
        put_u16(&mut b, optional, 0x20b);
        put_u32(&mut b, optional + 32, 0x1000);
        put_u32(&mut b, optional + 36, 0x200);
        put_u32(&mut b, optional + 56, 0x2000);
        put_u32(&mut b, optional + 60, 0x400);
        let section = optional + 240;
        b[section..section + 6].copy_from_slice(b".text\0");
        put_u32(&mut b, section + 8, 0x100);
        put_u32(&mut b, section + 12, 0x1000);
        put_u32(&mut b, section + 16, 0x200);
        put_u32(&mut b, section + 20, 0x400);
        b
    }

    #[test]
    fn hiding_the_console_sets_the_subsystem() {
        let mut exe = program();
        put_u16(&mut exe, 0x98 + 68, 3);
        hide_console(&mut exe).unwrap();
        assert_eq!(u16_at(&exe, 0x98 + 68).unwrap(), GUI_SUBSYSTEM);
        assert!(hide_console(&mut [0; 100]).is_err());
    }

    /// The data a resource path leads to, reading the tree the way Windows does.
    fn find(section: &[u8], rva: u32, path: [u32; 2]) -> Option<&[u8]> {
        let entry = |dir: usize, id: Option<u32>| -> Option<usize> {
            let count = u16_at(section, dir + 14).ok()? as usize;
            (0..count).find_map(|i| {
                let e = dir + 16 + i * 8;
                let matches = id.is_none_or(|id| u32_at(section, e).unwrap() == id);
                matches.then(|| (u32_at(section, e + 4).unwrap() & 0x7fff_ffff) as usize)
            })
        };
        let names = entry(0, Some(path[0]))?;
        let languages = entry(names, Some(path[1]))?;
        let leaf = entry(languages, None)?;
        let at = (u32_at(section, leaf).ok()? - rva) as usize;
        Some(&section[at..at + u32_at(section, leaf + 4).ok()? as usize])
    }

    #[test]
    fn the_icon_lands_in_a_new_section() {
        let mut exe = program();
        let images = vec![(32, b"small".to_vec()), (256, vec![7; 300])];
        set_icon(&mut exe, &images).unwrap();

        let h = headers(&exe).unwrap();
        assert_eq!(h.count, 2);
        let s = h.sections + SECTION_HEADER;
        assert_eq!(&exe[s..s + 5], b".rsrc");
        let rva = u32_at(&exe, s + 12).unwrap();
        assert_eq!(rva, 0x2000);
        assert_eq!(u32_at(&exe, s + 20).unwrap(), 0x600);
        assert_eq!(exe.len(), 0x600 + u32_at(&exe, s + 16).unwrap() as usize);
        assert_eq!(exe.len() % 0x200, 0);
        assert_eq!(u32_at(&exe, h.optional + 112 + 16).unwrap(), rva);
        assert_eq!(u32_at(&exe, h.optional + 56).unwrap(), 0x3000);

        let section = &exe[0x600..];
        assert_eq!(find(section, rva, [RT_ICON, 1]), Some(&b"small"[..]));
        assert_eq!(find(section, rva, [RT_ICON, 2]), Some(&[7u8; 300][..]));
        let group = find(section, rva, [RT_GROUP_ICON, 1]).unwrap();
        assert_eq!(&group[..6], &[0, 0, 1, 0, 2, 0]);
        // Each entry is 14 bytes: the second is 256 px (written as 0), with image id 2.
        assert_eq!(group[6], 32);
        assert_eq!(group[6 + 14], 0);
        assert_eq!(&group[6 + 14 + 8..6 + 14 + 12], &300u32.to_le_bytes());
        assert_eq!(u16_at(group, 6 + 14 + 12).unwrap(), 2);

        // One icon is all a program has.
        assert!(set_icon(&mut exe, &images).is_err());
    }

    /// The real thing: the test program itself still starts with an icon and no console.
    #[cfg(windows)]
    #[test]
    fn a_changed_program_still_runs() {
        let mut exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        set_icon(&mut exe, &[(32, vec![0; 64])]).unwrap();
        hide_console(&mut exe).unwrap();
        let dir = std::env::temp_dir().join(format!("kulisa-pe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("changed.exe");
        std::fs::write(&path, exe).unwrap();
        let out = std::process::Command::new(&path).arg("--list").output().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("a_changed_program_still_runs"));
    }
}
