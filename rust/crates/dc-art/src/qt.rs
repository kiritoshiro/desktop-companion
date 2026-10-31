//! Small Qt behaviours the art modules lean on, reproduced exactly:
//! `QColor::lighter` / `darker` (16-bit HSV, as Qt 5.15 does it), `qRound`,
//! `QRectF::toAlignedRect`, the non-antialiased `fillRect` pixel rule, and
//! Qt's smooth (area-averaging) image downscale for one 8-bit channel.

use dc_core::{Color, Rect};
use dc_render::{Image, parse_color};

/// `QColor("#rrggbb")`. Panics on a malformed literal (all call sites are constants).
pub fn hex(s: &str) -> Color {
    parse_color(s).unwrap_or_else(|| panic!("bad colour literal {s}"))
}

/// `qRound(double)`: half away from zero.
pub fn qround(d: f64) -> i64 {
    if d >= 0.0 { (d + 0.5).floor() as i64 } else { (d - 0.5).ceil() as i64 }
}

/// `QRectF::toAlignedRect()`: the smallest integer rectangle containing `r`.
pub fn to_aligned_rect(r: Rect) -> Rect {
    let x1 = r.x.floor();
    let y1 = r.y.floor();
    let x2 = (r.x + r.w).ceil();
    let y2 = (r.y + r.h).ceil();
    Rect::new(x1, y1, x2 - x1, y2 - y1)
}

/// `QRectF::intersected`: empty (zero) rect when they do not overlap.
pub fn intersected(a: Rect, b: Rect) -> Rect {
    a.intersection(&b).unwrap_or_default()
}

/// `QRectF::isEmpty`.
pub fn is_empty(r: Rect) -> bool {
    r.w <= 0.0 || r.h <= 0.0
}

/// The pixels a non-antialiased `fillRect(QRectF)` covers (Qt's
/// `toNormalizedFillRect`): edges rounded with `qRound`. Returns
/// `(x0, y0, x1, y1)`, exclusive end, unclipped.
pub fn fill_rect_pixels(r: Rect) -> (i64, i64, i64, i64) {
    let mut x1 = qround(r.x);
    let mut y1 = qround(r.y);
    let mut x2 = qround(r.x + r.w);
    let mut y2 = qround(r.y + r.h);
    if x2 < x1 {
        std::mem::swap(&mut x1, &mut x2);
    }
    if y2 < y1 {
        std::mem::swap(&mut y1, &mut y2);
    }
    (x1, y1, x2, y2)
}

// -- QColor HSV ------------------------------------------------------------------

const USHRT_MAX: f32 = 65535.0;

fn qround_f32(f: f32) -> i32 {
    if f >= 0.0 { (f + 0.5).floor() as i32 } else { (f - 0.5).ceil() as i32 }
}

fn fuzzy_compare(a: f32, b: f32) -> bool {
    (a - b).abs() * 100000.0 <= a.abs().min(b.abs())
}

/// `qt_div_257`: 16-bit channel back to 8 bits.
fn div_257(x: u32) -> u8 {
    ((x - (x >> 8) + 0x80) >> 8) as u8
}

/// (hue in 1/100 degree or 65535 for achromatic, saturation 0..65535, value 0..65535).
fn to_hsv16(c: Color) -> (u32, u32, u32) {
    let r = (c.r as u32 * 0x101) as f32 / USHRT_MAX;
    let g = (c.g as u32 * 0x101) as f32 / USHRT_MAX;
    let b = (c.b as u32 * 0x101) as f32 / USHRT_MAX;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let value = qround_f32(max * USHRT_MAX) as u32;
    if delta.abs() <= 0.00001 {
        return (65535, 0, value);
    }
    let sat = qround_f32((delta / max) * USHRT_MAX) as u32;
    let mut hue = if fuzzy_compare(r, max) {
        (g - b) / delta
    } else if fuzzy_compare(g, max) {
        2.0 + (b - r) / delta
    } else if fuzzy_compare(b, max) {
        4.0 + (r - g) / delta
    } else {
        0.0
    };
    hue *= 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    (qround_f32(hue * 100.0) as u32, sat, value)
}

fn from_hsv16(hue: u32, sat: u32, value: u32, alpha: u8) -> Color {
    if sat == 0 || hue == 65535 {
        let v = div_257(value);
        return Color::rgba(v, v, v, alpha);
    }
    let h = if hue == 36000 { 0.0 } else { hue as f32 / 6000.0 };
    let s = sat as f32 / USHRT_MAX;
    let v = value as f32 / USHRT_MAX;
    let i = h as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let (r, g, b) = if i & 1 == 1 {
        let q = v * (1.0 - s * f);
        match i {
            1 => (q, v, p),
            3 => (p, q, v),
            _ => (v, p, q),
        }
    } else {
        let t = v * (1.0 - s * (1.0 - f));
        match i {
            0 => (v, t, p),
            2 => (p, v, t),
            _ => (t, p, v),
        }
    };
    let ch = |x: f32| div_257(qround_f32(x * USHRT_MAX).clamp(0, 65535) as u32);
    Color::rgba(ch(r), ch(g), ch(b), alpha)
}

/// `QColor::lighter(factor)`, exactly as Qt 5.15.
pub fn lighter(c: Color, factor: i32) -> Color {
    if factor <= 0 {
        return c;
    }
    if factor < 100 {
        return darker(c, 10000 / factor);
    }
    let (h, s, v) = to_hsv16(c);
    let mut s = s as i64;
    let mut v = (factor as u64 * v as u64 / 100) as i64;
    if v > 65535 {
        s -= v - 65535;
        if s < 0 {
            s = 0;
        }
        v = 65535;
    }
    from_hsv16(h, s as u32, v as u32, c.a)
}

/// `QColor::darker(factor)`, exactly as Qt 5.15.
pub fn darker(c: Color, factor: i32) -> Color {
    if factor <= 0 {
        return c;
    }
    if factor < 100 {
        return lighter(c, 10000 / factor);
    }
    let (h, s, v) = to_hsv16(c);
    let v = (v as u64 * 100 / factor as u64) as u32;
    from_hsv16(h, s, v, c.a)
}

/// `QColor::lightness()` (HSL lightness, 0..255).
pub fn lightness(c: Color) -> i32 {
    let max = c.r.max(c.g).max(c.b) as i32;
    let min = c.r.min(c.g).min(c.b) as i32;
    (max + min) / 2
}

// -- pixel helpers -----------------------------------------------------------------

/// Qt's `qt_div_255` (`BYTE_MUL` rounding).
#[inline]
pub fn div255(x: u32) -> u32 {
    (x + (x >> 8) + 0x80) >> 8
}

/// Premultiplied source-over of one pixel, Qt's integer arithmetic.
#[inline]
pub fn over(dst: &mut [u8], src: &[u8]) {
    let sa = src[3] as u32;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        dst[..4].copy_from_slice(&src[..4]);
        return;
    }
    let inv = 255 - sa;
    for i in 0..4 {
        dst[i] = (src[i] as u32 + div255(dst[i] as u32 * inv)).min(255) as u8;
    }
}

/// Grey (`Format_Grayscale8`) as one byte per pixel at the image's own size.
pub fn grey_of(image: &Image) -> Vec<u8> {
    image.to_gray8()
}

// -- Qt's smooth downscale (qimagescale.cpp, `qt_qimageScaleAARGBA_down_xy`) ------------

fn calc_points(s: usize, d: usize) -> Vec<usize> {
    // qimageCalcXPoints / YPoints for scaling down: val starts at 0.
    let inc = ((s as i64) << 16) / d as i64;
    let mut val: i64 = 0;
    (0..d)
        .map(|_| {
            let p = (val >> 16).max(0) as usize;
            val += inc;
            p
        })
        .collect()
}

fn calc_apoints(s: usize, d: usize) -> Vec<(i64, i64)> {
    // Scaling down: (ap, Cp).
    let inc = ((s as i64) << 16) / d as i64;
    let cp = (((d as i64) << 14) + s as i64 - 1) / s as i64;
    let mut val: i64 = 0;
    (0..d)
        .map(|_| {
            let ap = ((0x10000 - (val & 0xffff)) * cp) >> 16;
            val += inc;
            (ap, cp)
        })
        .collect()
}

/// `QImage::scaled(dw, dh, IgnoreAspectRatio, SmoothTransformation)` of an
/// opaque single-channel image when both sides shrink (Qt's area averaging,
/// integer for integer). Other sizes fall back to the nearest equivalent:
/// equal size copies, any growth uses `Image::smooth_scaled`.
pub fn smooth_scale_gray(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if dw == 0 || dh == 0 || sw == 0 || sh == 0 {
        return vec![0; dw * dh];
    }
    if dw == sw && dh == sh {
        return src.to_vec();
    }
    if dw >= sw || dh >= sh {
        let img = Image::from_gray8(sw as u32, sh as u32, src).expect("size");
        return img.smooth_scaled(dw as u32, dh as u32).to_gray8();
    }
    let xpoints = calc_points(sw, dw);
    let ypoints = calc_points(sh, dh);
    let xapoints = calc_apoints(sw, dw);
    let yapoints = calc_apoints(sh, dh);
    let mut out = vec![0u8; dw * dh];
    // helper: weighted run along x starting at `start`.
    let helper = |start: usize, xap: i64, cx: i64| -> i64 {
        let mut idx = start;
        let mut r = src[idx] as i64 * xap;
        let mut j = (1 << 14) - xap;
        while j > cx {
            idx += 1;
            r += src[idx] as i64 * cx;
            j -= cx;
        }
        idx += 1;
        r + src[idx.min(src.len() - 1)] as i64 * j
    };
    for y in 0..dh {
        let (yap, cy) = yapoints[y];
        let row0 = ypoints[y] * sw;
        for x in 0..dw {
            let (xap, cx) = xapoints[x];
            let mut sptr = row0 + xpoints[x];
            let rx = helper(sptr, xap, cx);
            let mut r = (rx >> 4) * yap;
            let mut j = (1 << 14) - yap;
            while j > cy {
                sptr += sw;
                let rx = helper(sptr, xap, cx);
                r += (rx >> 4) * cy;
                j -= cy;
            }
            sptr += sw;
            if sptr < src.len() {
                let rx = helper(sptr, xap, cx);
                r += (rx >> 4) * j;
            }
            out[y * dw + x] = (r >> 24).clamp(0, 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighter_and_darker_match_qt() {
        // Values printed by PyQt5 5.15.2.
        assert_eq!(lighter(hex("#76853f"), 130), Color::rgb(0x99, 0xad, 0x52));
        assert_eq!(darker(hex("#76853f"), 140), Color::rgb(0x54, 0x5f, 0x2d));
        assert_eq!(darker(hex("#c0503c"), 260), Color::rgb(0x4a, 0x1f, 0x17));
        assert_eq!(lighter(Color::rgb(128, 116, 98), 150), Color::rgb(192, 174, 147));
        assert_eq!(lighter(Color::rgb(250, 250, 250), 150), Color::rgb(255, 255, 255));
        assert_eq!(lighter(Color::rgb(70, 64, 58), 140), Color::rgb(98, 90, 81));
        assert_eq!(darker(hex("#5c301d"), 10000 / 90), Color::rgb(83, 43, 26));
    }

    #[test]
    fn four_to_one_is_a_floored_mean() {
        let src: Vec<u8> = (0..64).map(|i| (i * 3) as u8).collect();
        let out = smooth_scale_gray(&src, 8, 8, 2, 2);
        let mean = |x0: usize, y0: usize| {
            let mut s = 0u32;
            for y in y0..y0 + 4 {
                for x in x0..x0 + 4 {
                    s += src[y * 8 + x] as u32;
                }
            }
            (s / 16) as u8
        };
        assert_eq!(out, vec![mean(0, 0), mean(4, 0), mean(0, 4), mean(4, 4)]);
    }

    #[test]
    fn fill_rule_rounds_edges() {
        assert_eq!(fill_rect_pixels(Rect::new(20.0, 53.0, 1.4, 7.0)), (20, 53, 21, 60));
        assert_eq!(fill_rect_pixels(Rect::new(23.6, 53.0, 1.4, 7.0)), (24, 53, 25, 60));
        assert_eq!(fill_rect_pixels(Rect::new(20.0, 56.0, 5.0, 1.2)), (20, 56, 25, 57));
    }
}
