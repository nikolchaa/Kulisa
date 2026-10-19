//! CPU images: decoding and premultiplication.

use std::sync::OnceLock;

/// An RGBA8 image: sRGB-encoded color, premultiplied alpha (premultiplied in linear space).
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

fn to_linear() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        })
    })
}

const SRGB_STEPS: usize = 4096;

fn to_srgb() -> &'static [u8; SRGB_STEPS] {
    static LUT: OnceLock<[u8; SRGB_STEPS]> = OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / (SRGB_STEPS - 1) as f32;
            let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0 + 0.5) as u8
        })
    })
}

impl Image {
    /// Decodes PNG, JPEG or WebP bytes.
    pub fn decode(bytes: &[u8]) -> Result<Image, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.into_rgba8();
        let (width, height) = img.dimensions();
        Ok(Image::from_straight_rgba(width, height, img.into_raw()))
    }

    /// Wraps straight-alpha sRGB RGBA8 pixels, premultiplying them.
    pub fn from_straight_rgba(width: u32, height: u32, mut pixels: Vec<u8>) -> Image {
        assert_eq!(pixels.len(), (width * height * 4) as usize, "pixel buffer size mismatch");
        let (lin, srgb) = (to_linear(), to_srgb());
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3];
            if a == 255 {
                continue;
            }
            let af = a as f32 / 255.0;
            for c in &mut px[..3] {
                let l = lin[*c as usize] * af;
                *c = srgb[(l * (SRGB_STEPS - 1) as f32 + 0.5) as usize];
            }
        }
        Image { width, height, pixels }
    }

    /// A single-color image from straight-alpha sRGB RGBA.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Image {
        let pixels = rgba.repeat((width * height) as usize);
        Image::from_straight_rgba(width, height, pixels)
    }

    /// Copy of the image with a `pad`-pixel border that repeats the edge pixels, so linear
    /// filtering in an atlas never samples a neighbour.
    pub(crate) fn extruded(&self, pad: u32) -> Image {
        let (w, h) = (self.width + 2 * pad, self.height + 2 * pad);
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let sy = (y as i64 - pad as i64).clamp(0, self.height as i64 - 1) as u32;
            for x in 0..w {
                let sx = (x as i64 - pad as i64).clamp(0, self.width as i64 - 1) as u32;
                let i = ((sy * self.width + sx) * 4) as usize;
                pixels.extend_from_slice(&self.pixels[i..i + 4]);
            }
        }
        Image { width: w, height: h, pixels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplies_in_linear_space() {
        let img = Image::from_straight_rgba(1, 1, vec![255, 255, 255, 128]);
        // Linear 1.0 * 0.502 = 0.502 linear, which is ~188 in sRGB (not 128).
        assert!((187..=189).contains(&img.pixels[0]), "{}", img.pixels[0]);
        assert_eq!(img.pixels[3], 128);
        let opaque = Image::from_straight_rgba(1, 1, vec![10, 20, 30, 255]);
        assert_eq!(opaque.pixels, vec![10, 20, 30, 255]);
    }

    #[test]
    fn decodes_png() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let img = Image::decode(&png).unwrap();
        assert_eq!((img.width, img.height), (3, 2));
        assert_eq!(&img.pixels[..4], &[255, 0, 0, 255]);
        assert!(Image::decode(b"not an image").is_err());
    }

    #[test]
    fn extrudes_edges() {
        let img = Image::from_straight_rgba(2, 1, vec![1, 1, 1, 255, 2, 2, 2, 255]);
        let e = img.extruded(1);
        assert_eq!((e.width, e.height), (4, 3));
        let row: Vec<u8> = e.pixels[16..32].chunks(4).map(|p| p[0]).collect();
        assert_eq!(row, vec![1, 1, 2, 2]);
    }
}
