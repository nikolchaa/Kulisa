//! The app icon for every package, made from one square PNG in the game (`[game] icon` in
//! `kulisa.toml`, default `icon.png`): a Windows `.ico`, the Android launcher icon, iOS
//! home-screen icons and the web page's icons.

use std::io::Cursor;

use image::DynamicImage;
use image::imageops::FilterType;

use crate::{Game, Result};

/// The game's icon, if it has one. Naming a file in `kulisa.toml` that does not exist (or
/// is not an image) is an error; a missing default `icon.png` is not.
pub fn load(game: &Game) -> Result<Option<DynamicImage>> {
    let path = game.dir.join(&game.config.icon);
    let named = game.config.raw.get("game.icon").is_some();
    if !path.is_file() {
        return if named { Err(format!("the icon {} does not exist", path.display())) } else { Ok(None) };
    }
    let img = image::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if img.width() != img.height() {
        eprintln!("warning: the icon {} is not square; it will be stretched", path.display());
    }
    if img.width() < 512 {
        eprintln!(
            "warning: the icon {} is small ({} px); 1024x1024 looks sharp everywhere",
            path.display(),
            img.width()
        );
    }
    Ok(Some(img))
}

/// The icon as a `size`x`size` PNG.
pub fn png(img: &DynamicImage, size: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.resize_exact(size, size, FilterType::Lanczos3)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("encoding a PNG in memory");
    out.into_inner()
}

/// A Windows `.ico` with PNG images from 16 to 256 pixels.
pub fn ico(img: &DynamicImage) -> Vec<u8> {
    const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&s| png(img, s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (size, data) in SIZES.iter().zip(&images) {
        // 0 stands for 256.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in images {
        out.extend_from_slice(&data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ico_holds_every_size() {
        let img = DynamicImage::new_rgba8(300, 300);
        let ico = ico(&img);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 6, 0]);
        // The last entry is 256 (written as 0); its PNG data starts where it says.
        let entry = 6 + 16 * 5;
        assert_eq!(ico[entry], 0);
        let at = u32::from_le_bytes(ico[entry + 12..entry + 16].try_into().unwrap()) as usize;
        assert_eq!(&ico[at + 1..at + 4], b"PNG");
        let small = image::load_from_memory(&png(&img, 120)).unwrap();
        assert_eq!((small.width(), small.height()), (120, 120));
    }
}
