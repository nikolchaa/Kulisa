//! The app icon for every package, made from one square PNG in the game (`[game] icon` in
//! `kulisa.toml`, default `icon.png`): a Windows program's icon, a macOS `.icns`, the Android launcher
//! icon, the iOS asset catalog's icon and the web page's icons.

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

/// The icon as a `size`x`size` PNG without transparency, on `background` (`#rrggbb`): the App
/// Store refuses icons with an alpha channel.
pub fn png_opaque(img: &DynamicImage, size: u32, background: &str) -> Vec<u8> {
    let hex = background.trim_start_matches('#');
    let channel = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
    let (r, g, b) = (channel(0) as u32, channel(2) as u32, channel(4) as u32);
    let scaled = img.resize_exact(size, size, FilterType::Lanczos3).to_rgba8();
    let flat = image::RgbImage::from_fn(size, size, |x, y| {
        let [pr, pg, pb, a] = scaled.get_pixel(x, y).0;
        let mix = |p: u8, bg: u32| ((p as u32 * a as u32 + bg * (255 - a as u32)) / 255) as u8;
        image::Rgb([mix(pr, r), mix(pg, g), mix(pb, b)])
    });
    let mut out = Cursor::new(Vec::new());
    flat.write_to(&mut out, image::ImageFormat::Png).expect("encoding a PNG in memory");
    out.into_inner()
}

/// A macOS `.icns`: PNG images from 16 to 1024 pixels under their type codes.
pub fn icns(img: &DynamicImage) -> Vec<u8> {
    const SIZES: [(&[u8; 4], u32); 8] = [
        (b"icp4", 16),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic13", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
    ];
    let mut body = Vec::new();
    for (code, size) in SIZES {
        let data = png(img, size);
        body.extend_from_slice(code);
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&data);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

/// A Windows program's icon: PNG images from 16 to 256 pixels, with their sizes.
pub fn windows_images(img: &DynamicImage) -> Vec<(u32, Vec<u8>)> {
    [16, 24, 32, 48, 64, 256].map(|size| (size, png(img, size))).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_images_hold_every_size() {
        let img = DynamicImage::new_rgba8(300, 300);
        let images = windows_images(&img);
        assert_eq!(images.iter().map(|(s, _)| *s).collect::<Vec<_>>(), [16, 24, 32, 48, 64, 256]);
        assert_eq!(&images[5].1[1..4], b"PNG");
        let small = image::load_from_memory(&png(&img, 120)).unwrap();
        assert_eq!((small.width(), small.height()), (120, 120));
    }

    #[test]
    fn icns_chunks_add_up() {
        let icns = icns(&DynamicImage::new_rgba8(32, 32));
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize, icns.len());
        let (mut at, mut codes) = (8, Vec::new());
        while at < icns.len() {
            codes.push(String::from_utf8_lossy(&icns[at..at + 4]).into_owned());
            assert_eq!(&icns[at + 9..at + 12], b"PNG");
            at += u32::from_be_bytes(icns[at + 4..at + 8].try_into().unwrap()) as usize;
        }
        assert_eq!(at, icns.len());
        assert_eq!(codes.len(), 8);
        assert!(codes.contains(&"ic10".to_string()));
    }

    #[test]
    fn opaque_icons_have_no_alpha() {
        // A fully transparent image becomes the background colour.
        let flat = png_opaque(&DynamicImage::new_rgba8(4, 4), 4, "#102030");
        let img = image::load_from_memory(&flat).unwrap();
        assert!(!img.color().has_alpha());
        assert_eq!(img.to_rgb8().get_pixel(1, 1).0, [0x10, 0x20, 0x30]);
    }
}
