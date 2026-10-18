//! Painting colours: the palette lookups of `creature/render_procedural.py`
//! (`_qcolor`, `_qcolor_triplet`, `_appearance_color`, `_mix`) with the
//! camouflage blend.
//!
//! The Python cached the blended triples per camouflage signature because a
//! spider asks for about 150 colours a frame; here the blend is a handful of
//! multiplies and is simply recomputed.

use dc_core::Color;
use dc_core::math::clamp;
use std::collections::HashMap;

/// The resolved palette of one spider (`Creature.colors`, overrides applied)
/// and its camouflage state.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Palette {
    /// `Creature.colors`: every slot, model colours with overrides applied.
    pub colors: HashMap<String, [f64; 3]>,
    /// The keys of `Creature.color_overrides` (only presence matters here).
    pub color_overrides: Vec<String>,
    /// `_camouflage_color`, the desktop colour the spider is blending into.
    pub camouflage_color: Option<[f64; 3]>,
    /// `_camouflage_strength` (0..1).
    pub camouflage_strength: f64,
    /// `personality["camouflage_color_blend"]`.
    pub camouflage_color_blend: f64,
}

/// Python `int()` of a clamped channel.
#[inline]
fn chan(v: f64) -> u8 {
    clamp(v, 0.0, 255.0) as i64 as u8
}

impl Palette {
    pub fn has_override(&self, key: &str) -> bool {
        self.color_overrides.iter().any(|k| k == key)
    }

    #[inline]
    fn blend_amount(&self) -> Option<([f64; 3], f64)> {
        let camo = self.camouflage_color?;
        let strength = clamp(self.camouflage_strength, 0.0, 1.0);
        let color_blend = clamp(self.camouflage_color_blend, 0.0, 1.0);
        if strength > 0.001 && color_blend > 0.001 { Some((camo, strength * color_blend)) } else { None }
    }

    /// `_blend_palette_color(key)` as an RGB triple.
    pub fn blend_palette_color(&self, key: &str) -> [u8; 3] {
        let raw = self.colors.get(key).copied().unwrap_or([35.0, 30.0, 25.0]);
        let mut rgb = raw;
        if let Some((camo, base)) = self.blend_amount() {
            let k = if key == "eyes" {
                0.72
            } else if key == "highlight" {
                0.96
            } else {
                1.0
            };
            let blend = base * k;
            for i in 0..3 {
                rgb[i] = rgb[i] * (1.0 - blend) + camo[i] * blend;
            }
        }
        [chan(rgb[0]), chan(rgb[1]), chan(rgb[2])]
    }

    /// `_qcolor(key, alpha)`.
    #[inline]
    pub fn qcolor(&self, key: &str, alpha: i64) -> Color {
        let [r, g, b] = self.blend_palette_color(key);
        Color::rgba(r, g, b, alpha.clamp(0, 255) as u8)
    }

    /// `_blend_triplet(rgb)`.
    pub fn blend_triplet(&self, rgb: [f64; 3]) -> [u8; 3] {
        let mut out = rgb;
        if let Some((camo, base)) = self.blend_amount() {
            let blend = base * 0.92;
            for i in 0..3 {
                out[i] = out[i] * (1.0 - blend) + camo[i] * blend;
            }
        }
        [chan(out[0]), chan(out[1]), chan(out[2])]
    }

    /// `_qcolor_triplet(rgb, alpha)`.
    #[inline]
    pub fn qcolor_triplet(&self, rgb: [f64; 3], alpha: i64) -> Color {
        let [r, g, b] = self.blend_triplet(rgb);
        Color::rgba(r, g, b, alpha.clamp(0, 255) as u8)
    }

    /// `_appearance_color(key, fallback_key)`: an explicit slot override wins
    /// over the model's appearance accent. `appearance_value` is the model's
    /// `appearance[key]` when it is an RGB triple. `None` is Python's `None`
    /// (an overridden slot whose key is missing from `colors`).
    pub fn appearance_color(&self, key: &str, fallback_key: &str, appearance_value: Option<[f64; 3]>) -> Option<[f64; 3]> {
        if self.has_override(key) {
            return self.colors.get(key).copied();
        }
        if self.has_override(fallback_key) {
            return self.colors.get(fallback_key).copied();
        }
        match appearance_value {
            Some(v) => Some(v),
            None => Some(self.colors.get(fallback_key).copied().unwrap_or([120.0, 80.0, 60.0])),
        }
    }
}

/// `_mix(base, other, amount)`: warm one colour toward another, keeping the
/// first one's alpha.
pub fn mix(base: Color, other: Option<Color>, amount: f64) -> Color {
    let Some(other) = other else { return base };
    if amount <= 0.0 {
        return base;
    }
    let inv = 1.0 - amount;
    let m = |a: u8, b: u8| (a as f64 * inv + b as f64 * amount) as i64 as u8;
    Color::rgba(m(base.r, other.r), m(base.g, other.g), m(base.b, other.b), base.a)
}

/// `QColor(r, g, b, a)` from Python ints (Qt clamps nothing; callers pass
/// values already in range, so out-of-range values are clamped defensively).
#[inline]
pub fn qrgba(r: i64, g: i64, b: i64, a: i64) -> Color {
    Color::rgba(r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8, a.clamp(0, 255) as u8)
}

/// `QColor.darker(factor)` exactly as Qt 5 computes it (integer HSV with
/// 16-bit value channel), which the dc-core approximation does in floats.
pub fn qt_darker(c: Color, factor: i32) -> Color {
    if factor <= 0 {
        return c;
    }
    if factor < 100 {
        return qt_lighter(c, 10000 / factor);
    }
    let (h, s, v) = to_qt_hsv(c);
    let v = (v as i64 * 100 / factor as i64) as u32;
    from_qt_hsv(h, s, v, c.a)
}

/// `QColor.lighter(factor)` exactly as Qt 5 computes it.
pub fn qt_lighter(c: Color, factor: i32) -> Color {
    if factor <= 0 {
        return c;
    }
    if factor < 100 {
        return qt_darker(c, 10000 / factor);
    }
    let (h, mut s, v) = to_qt_hsv(c);
    let mut v = (factor as i64 * v as i64 / 100) as i64;
    if v > 65535 {
        s -= (v - 65535) as i32;
        if s < 0 {
            s = 0;
        }
        v = 65535;
    }
    from_qt_hsv(h, s, v as u32, c.a)
}

/// Qt's `toHsv()`: hue in 1/100 degree (-1 for achromatic), saturation and
/// value in 0..65535.
fn to_qt_hsv(c: Color) -> (i32, i32, u32) {
    let r = c.r as f64 * 257.0 / 65535.0;
    let g = c.g as f64 * 257.0 / 65535.0;
    let b = c.b as f64 * 257.0 / 65535.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let value = (max * 65535.0).round() as u32;
    if delta.abs() < f64::EPSILON {
        return (-1, 0, value);
    }
    let sat = ((delta / max) * 65535.0).round() as i32;
    let mut hue = if r == max {
        (g - b) / delta
    } else if g == max {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    hue *= 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    ((hue * 100.0).round() as i32, sat, value)
}

/// Qt's HSV -> RGB conversion (`QColor::toRgb` from an HSV spec).
fn from_qt_hsv(hue: i32, sat: i32, val: u32, alpha: u8) -> Color {
    // `QColor::red()` is `qt_div_257` of the rounded 16-bit channel.
    let to8 = |v: f64| -> u8 {
        // Qt's `qt_div_257`: x += 0x80; (x - (x >> 8)) >> 8.
        let x = (v * 65535.0).round() as u32 + 0x80;
        ((x - (x >> 8)) >> 8).min(255) as u8
    };
    if sat == 0 || hue == -1 {
        let v = val as f64 / 65535.0;
        let g = to8(v);
        return Color::rgba(g, g, g, alpha);
    }
    let h = if hue == 36000 { 0.0 } else { hue as f64 / 6000.0 };
    let s = sat as f64 / 65535.0;
    let v = val as f64 / 65535.0;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let (r, g, b) = if i & 1 != 0 {
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
    Color::rgba(to8(r), to8(g), to8(b), alpha)
}

/// `QColor("#rrggbb")` with an alpha (`armour_art._c`).
pub fn hex(value: &str, alpha: u8) -> Color {
    dc_render::parse_color(value).unwrap_or(Color::BLACK).with_alpha(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_darker_lighter_match_known_values() {
        // Values checked against PyQt5: QColor(200, 120, 40).darker(170) etc.
        let c = Color::rgb(200, 120, 40);
        let d = qt_darker(c, 170);
        assert_eq!((d.r, d.g, d.b), (118, 71, 24));
        let l = qt_lighter(Color::rgb(100, 50, 25), 155);
        assert_eq!((l.r, l.g, l.b), (155, 78, 39));
        let cases = [
            ((240, 230, 220), 125, false, (192, 184, 176)),
            ((120, 200, 250), 180, false, (67, 111, 139)),
            ((30, 30, 30), 155, true, (46, 46, 46)),
            ((250, 250, 250), 155, true, (255, 255, 255)),
            ((10, 200, 30), 110, false, (9, 182, 27)),
            ((30, 30, 30), 110, false, (27, 27, 27)),
            ((77, 66, 55), 140, false, (55, 47, 39)),
            ((200, 180, 160), 155, true, (255, 255, 255)),
        ];
        for ((r, g, b), f, light, want) in cases {
            let c = Color::rgb(r, g, b);
            let got = if light { qt_lighter(c, f) } else { qt_darker(c, f) };
            assert_eq!((got.r, got.g, got.b), want, "{r},{g},{b} {f}");
        }
    }

    #[test]
    fn camouflage_blend() {
        let mut p = Palette::default();
        p.colors.insert("body".into(), [100.0, 50.0, 0.0]);
        assert_eq!(p.blend_palette_color("body"), [100, 50, 0]);
        assert_eq!(p.blend_palette_color("missing"), [35, 30, 25]);
        p.camouflage_color = Some([200.0, 200.0, 200.0]);
        p.camouflage_strength = 1.0;
        p.camouflage_color_blend = 0.5;
        assert_eq!(p.blend_palette_color("body"), [150, 125, 100]);
    }
}
