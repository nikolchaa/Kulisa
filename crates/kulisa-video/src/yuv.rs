//! YUV pictures (subsampled by two each way) to RGBA, for decoders that do not do it themselves.
#![cfg_attr(not(any(test, target_os = "android", target_os = "linux")), allow(dead_code))]

/// One decoded picture's planes.
pub(crate) struct Planes<'a> {
    pub y: &'a [u8],
    pub y_stride: usize,
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub uv_stride: usize,
    /// 1 when U and V have planes of their own, 2 when they alternate in one.
    pub uv_step: usize,
}

/// Fixed-point coefficients, times 256: Y scale, then V to red, U and V to green, U to blue.
struct Matrix(i32, i32, i32, i32, i32);

const BT601: Matrix = Matrix(298, 409, 100, 208, 516);
const BT709: Matrix = Matrix(298, 459, 55, 136, 541);

/// HD video is BT.709 and SD is BT.601, unless a file says otherwise (few do).
fn matrix(height: u32) -> &'static Matrix {
    if height >= 720 { &BT709 } else { &BT601 }
}

/// Fills `out` with `width * height` RGBA pixels. Limited range, as video almost always is.
pub(crate) fn to_rgba(p: &Planes, width: u32, height: u32, out: &mut Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    out.resize(w * h * 4, 255);
    let m = matrix(height);
    for (row, dst) in out.chunks_exact_mut(w * 4).enumerate() {
        let y = &p.y[row * p.y_stride..][..w];
        let c = (row / 2) * p.uv_stride;
        let chroma = w.div_ceil(2);
        let span = (chroma - 1) * p.uv_step + 1;
        row_to_rgba(y, &p.u[c..][..span], &p.v[c..][..span], p.uv_step, m, dst);
    }
}

#[inline]
fn clamp8(v: i32) -> u8 {
    (v >> 8).clamp(0, 255) as u8
}

fn row_to_rgba(y: &[u8], u: &[u8], v: &[u8], step: usize, m: &Matrix, dst: &mut [u8]) {
    for (i, (px, luma)) in dst.as_chunks_mut::<8>().0.iter_mut().zip(y.as_chunks::<2>().0).enumerate() {
        let (cu, cv) = (i32::from(u[i * step]) - 128, i32::from(v[i * step]) - 128);
        let (r, g, b) = (m.1 * cv + 128, -m.2 * cu - m.3 * cv + 128, m.4 * cu + 128);
        for (k, &l) in luma.iter().enumerate() {
            let l = m.0 * (i32::from(l) - 16);
            px[k * 4] = clamp8(l + r);
            px[k * 4 + 1] = clamp8(l + g);
            px[k * 4 + 2] = clamp8(l + b);
            px[k * 4 + 3] = 255;
        }
    }
    // An odd width leaves one pixel.
    if y.len() % 2 == 1 {
        let i = y.len() / 2;
        let (cu, cv) = (i32::from(u[i * step]) - 128, i32::from(v[i * step]) - 128);
        let l = m.0 * (i32::from(y[y.len() - 1]) - 16);
        let px = &mut dst[(y.len() - 1) * 4..];
        px[0] = clamp8(l + m.1 * cv + 128);
        px[1] = clamp8(l - m.2 * cu - m.3 * cv + 128);
        px[2] = clamp8(l + m.4 * cu + 128);
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: &[u8], b: [u8; 4]) {
        for (x, y) in a.iter().zip(b) {
            assert!(x.abs_diff(y) <= 2, "{a:?} is not {b:?}");
        }
    }

    #[test]
    fn known_colours_come_out_right() {
        // Limited-range values for pure colours, as an encoder writes them.
        for (height, y, u, v, rgba) in [
            (1080, 16, 128, 128, [0, 0, 0, 255]),
            (1080, 235, 128, 128, [255, 255, 255, 255]),
            (1080, 63, 102, 240, [255, 0, 0, 255]),
            (1080, 32, 240, 118, [0, 0, 255, 255]),
            (480, 81, 90, 240, [255, 0, 0, 255]),
            (480, 41, 240, 110, [0, 0, 255, 255]),
        ] {
            let planes = Planes { y: &[y; 4], y_stride: 2, u: &[u; 2], v: &[v; 2], uv_stride: 1, uv_step: 1 };
            let mut out = Vec::new();
            let mut row = [0u8; 8];
            row_to_rgba(planes.y, planes.u, planes.v, 1, matrix(height), &mut row);
            near(&row[..4], rgba);
            near(&row[4..], rgba);
            to_rgba(&Planes { y: &[y; 4], ..planes }, 2, 2, &mut out);
            assert_eq!(out.len(), 16);
        }
    }

    #[test]
    fn strides_steps_and_odd_sizes_are_followed() {
        // 3x3 with padding; U and V alternate in one plane, as on many phones.
        let y = [235, 235, 235, 0, 235, 235, 235, 0, 16, 16, 16, 0];
        let uv = [128, 128, 128, 128, 0, 128, 128, 128, 128, 0];
        let planes = Planes { y: &y, y_stride: 4, u: &uv[0..], v: &uv[1..], uv_stride: 5, uv_step: 2 };
        let mut out = Vec::new();
        to_rgba(&planes, 3, 3, &mut out);
        assert_eq!(out.len(), 36);
        near(&out[8..12], [255, 255, 255, 255]);
        near(&out[24..28], [0, 0, 0, 255]);
    }
}
