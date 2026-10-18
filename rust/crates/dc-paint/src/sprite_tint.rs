//! Port of `creature/sprite_tint.py`: recolour sprite-rig art by swapping
//! hue (or re-shading to one dark tone) while keeping saturation, value and
//! alpha, exactly as the Python pixel loop did (`colorsys` arithmetic,
//! straight-alpha ARGB32 pixels).

use dc_render::Image;
use std::collections::HashMap;

/// `PART_COLOR_KEYS`: which palette entry each sprite part takes its colour from.
pub const PART_COLOR_KEYS: [(&str, &str); 6] = [
    ("abdomen", "body"),
    ("cephalothorax", "body"),
    ("leg_upper", "legs"),
    ("leg_lower", "legs"),
    ("leg_knuckle", "legs"),
    ("leg_tip", "leg_tip"),
];

/// `NEUTRAL_SATURATION`.
pub const NEUTRAL_SATURATION: f64 = 0.08;
/// `SHADE_KEY`.
pub const SHADE_KEY: &str = "shade_to";

/// The palette key a part reads, if any.
pub fn part_color_key(part: &str) -> Option<&'static str> {
    PART_COLOR_KEYS.iter().find(|(p, _)| *p == part).map(|(_, k)| *k)
}

/// `colorsys.rgb_to_hsv`.
pub fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let v = maxc;
    if minc == maxc {
        return (0.0, 0.0, v);
    }
    let rangec = maxc - minc;
    let s = rangec / maxc;
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    (dc_core::math::py_mod(h / 6.0, 1.0), s, v)
}

/// `colorsys.hsv_to_rgb`.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0) as i64;
    let f = h * 6.0 - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// `_target_hue(rgb)`: `None` when the target itself is neutral.
pub fn target_hue(rgb: [f64; 3]) -> Option<f64> {
    let c = |v: f64| (v as i64).clamp(0, 255) as f64 / 255.0;
    let (hue, sat, _) = rgb_to_hsv(c(rgb[0]), c(rgb[1]), c(rgb[2]));
    if sat < NEUTRAL_SATURATION { None } else { Some(hue) }
}

/// `tint_pixmap(pixmap, rgb)`.
pub fn tint_pixmap(img: &Image, rgb: [f64; 3]) -> Image {
    let Some(hue) = target_hue(rgb) else { return img.clone() };
    if img.is_null() {
        return img.clone();
    }
    let mut data = img.to_straight_rgba();
    let mut lookup: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    for px in data.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let key = [px[0], px[1], px[2]];
        let swapped = *lookup.entry(key).or_insert_with(|| {
            let (_h, sat, value) = rgb_to_hsv(key[0] as f64 / 255.0, key[1] as f64 / 255.0, key[2] as f64 / 255.0);
            if sat < NEUTRAL_SATURATION {
                key
            } else {
                let (r, g, b) = hsv_to_rgb(hue, sat, value);
                [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
            }
        });
        px[..3].copy_from_slice(&swapped);
    }
    Image::from_straight_rgba(img.width(), img.height(), data).unwrap_or_else(|| img.clone())
}

/// `shade_pixmap(pixmap, rgb)`.
pub fn shade_pixmap(img: &Image, rgb: [f64; 3]) -> Image {
    if img.is_null() {
        return img.clone();
    }
    let tone = rgb.map(|c| (c as i64).clamp(0, 255) as f64);
    let mut data = img.to_straight_rgba();
    let mut lookup: HashMap<u8, [u8; 3]> = HashMap::new();
    for px in data.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let value = px[0].max(px[1]).max(px[2]);
        let shaded = *lookup.entry(value).or_insert_with(|| {
            let gain = 0.45 + 1.7 * (value as f64 / 255.0);
            tone.map(|c| ((c * gain) as i64).min(255) as u8)
        });
        px[..3].copy_from_slice(&shaded);
    }
    Image::from_straight_rgba(img.width(), img.height(), data).unwrap_or_else(|| img.clone())
}

/// `palette_signature(overrides)`: the overridden keys a part actually reads,
/// sorted, as `(key, rgb)` pairs.
pub fn palette_signature(overrides: &[(String, [f64; 3])]) -> Vec<(String, [i64; 3])> {
    let mut sig: Vec<(String, [i64; 3])> = overrides
        .iter()
        .filter(|(k, _)| k == SHADE_KEY || PART_COLOR_KEYS.iter().any(|(_, v)| v == k))
        .map(|(k, v)| (k.clone(), v.map(|c| c as i64)))
        .collect();
    sig.sort();
    sig.dedup_by(|a, b| a.0 == b.0);
    sig
}

/// `tint_assets(assets, overrides)`: tinted copies of the overridden parts.
pub fn tint_assets(assets: &mut HashMap<String, Image>, signature: &[(String, [i64; 3])]) {
    if signature.is_empty() {
        return;
    }
    let get = |key: &str| signature.iter().find(|(k, _)| k == key).map(|(_, v)| v.map(|c| c as f64));
    let shade = get(SHADE_KEY);
    for (name, img) in assets.iter_mut() {
        let key = part_color_key(name);
        if let (Some(shade), Some(_)) = (shade, key) {
            *img = shade_pixmap(img, shade);
            continue;
        }
        if let Some(rgb) = key.and_then(get) {
            *img = tint_pixmap(img, rgb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorsys_round_trip() {
        let (h, s, v) = rgb_to_hsv(0.8, 0.4, 0.2);
        assert!((h - 0.05555555555555555).abs() < 1e-12);
        assert!((s - 0.75).abs() < 1e-12 && (v - 0.8).abs() < 1e-12);
        let (r, g, b) = hsv_to_rgb(h, s, v);
        assert!((r - 0.8).abs() < 1e-12 && (g - 0.4).abs() < 1e-12 && (b - 0.2).abs() < 1e-12);
        assert_eq!(target_hue([128.0, 128.0, 128.0]), None);
    }

    #[test]
    fn tint_keeps_neutral_pixels_and_alpha() {
        let mut img = Image::new(2, 1);
        img.set_pixel(0, 0, dc_core::Color::rgba(200, 100, 50, 255));
        img.set_pixel(1, 0, dc_core::Color::rgba(120, 120, 120, 255));
        let out = tint_pixmap(&img, [0.0, 0.0, 255.0]);
        let a = out.pixel(0, 0);
        assert!(a.b > a.r && a.a == 255);
        assert_eq!(out.pixel(1, 0), dc_core::Color::rgba(120, 120, 120, 255));
    }
}
