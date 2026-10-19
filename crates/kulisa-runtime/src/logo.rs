//! Rasterizes the Kulisa mark (docs/brand/logo.svg) as the M0 placeholder texture.

use kulisa_render::Image;

type Shape = (&'static [(f32, f32)], [f32; 3], f32);

// Polygons in the 256x256 viewBox of logo.svg, painted in order. Colors are sRGB.
const SHAPES: &[Shape] = &[
    (&[(64.0, 56.0), (100.0, 56.0), (100.0, 200.0), (64.0, 200.0)], PAPER, 1.0),
    (&[(112.0, 128.0), (164.0, 56.0), (204.0, 56.0), (152.0, 128.0)], FOOTLIGHT, 1.0),
    (&[(112.0, 128.0), (152.0, 128.0), (204.0, 200.0), (164.0, 200.0)], PAPER, 0.45),
];
const INK: [f32; 3] = [0.067, 0.067, 0.086];
const PAPER: [f32; 3] = [0.957, 0.945, 0.918];
const FOOTLIGHT: [f32; 3] = [1.0, 0.416, 0.239];
const RADIUS: f32 = 56.0;
const SAMPLES: usize = 4;

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn inside_polygon(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn inside_rounded_square(x: f32, y: f32) -> bool {
    let cx = x.clamp(RADIUS, 256.0 - RADIUS);
    let cy = y.clamp(RADIUS, 256.0 - RADIUS);
    (x - cx).powi(2) + (y - cy).powi(2) <= RADIUS * RADIUS
}

/// Linear premultiplied color of one sample point.
fn sample(x: f32, y: f32) -> [f32; 4] {
    if !inside_rounded_square(x, y) {
        return [0.0; 4];
    }
    let mut c = INK.map(to_linear);
    for (poly, color, alpha) in SHAPES {
        if inside_polygon(poly, x, y) {
            for k in 0..3 {
                c[k] = c[k] * (1.0 - alpha) + to_linear(color[k]) * alpha;
            }
        }
    }
    [c[0], c[1], c[2], 1.0]
}

/// A `size`x`size` premultiplied sRGB image of the logo, supersampled for smooth edges.
pub fn image(size: u32) -> Image {
    let scale = 256.0 / size as f32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * scale;
                    let s = sample(x, y);
                    for k in 0..4 {
                        acc[k] += s[k];
                    }
                }
            }
            let n = (SAMPLES * SAMPLES) as f32;
            let a = acc[3] / n;
            // Encode linear premultiplied color for an sRGB texture.
            for k in 0..3 {
                pixels.push((to_srgb(acc[k] / n) * 255.0 + 0.5) as u8);
            }
            pixels.push((a * 255.0 + 0.5) as u8);
        }
    }
    Image { width: size, height: size, pixels }
}

#[cfg(test)]
mod tests {
    #[test]
    fn corners_are_transparent_and_center_is_opaque() {
        let img = super::image(64);
        let px = |x: u32, y: u32| &img.pixels[((y * 64 + x) * 4) as usize..][..4];
        assert_eq!(px(0, 0)[3], 0);
        assert_eq!(px(32, 32)[3], 255);
    }
}
