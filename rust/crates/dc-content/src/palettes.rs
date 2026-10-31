//! Port of `content/palettes.py`: named colour palettes and random ones.
//!
//! A palette is derived from a single hue: a spider is mostly one colour in
//! shade, its bands and highlights a little brighter and warmer, its eyes
//! brighter still. `ENEMY_COLORS` is the hand-set black-and-crimson enemy look.

use std::sync::LazyLock;

use dc_core::math::{py_mod, py_round};
use dc_core::{Color, PyRandom};

use crate::util::ColorMap;

/// Every colour key the renderer understands, in settings-window order.
pub const PALETTE_KEYS: [&str; 7] = ["body", "legs", "leg_band", "leg_dark", "leg_tip", "highlight", "eyes"];

/// `colorsys.hsv_to_rgb`, bit for bit.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).trunc();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// `colorsys.rgb_to_hsv`.
pub fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let rangec = maxc - minc;
    let v = maxc;
    if minc == maxc {
        return (0.0, 0.0, v);
    }
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
    (py_mod(h / 6.0, 1.0), s, v)
}

/// `int(round(channel * 255))` for each channel.
pub fn rgb_to_color(r: f64, g: f64, b: f64) -> Color {
    let c = |x: f64| py_round(x * 255.0).clamp(0.0, 255.0) as u8;
    Color::rgb(c(r), c(g), c(b))
}

fn rgb(hue: f64, saturation: f64, value: f64) -> Color {
    let (r, g, b) = hsv_to_rgb(py_mod(hue, 1.0), saturation.clamp(0.0, 1.0).min(1.0), value.clamp(0.0, 1.0));
    rgb_to_color(r, g, b)
}

/// `palette_from_hue(hue, saturation=0.55, value=0.78)`.
pub fn palette_from_hue(hue: f64, saturation: f64, value: f64) -> ColorMap {
    let mut m = ColorMap::new();
    m.insert("body", rgb(hue, saturation, value * 0.30));
    m.insert("legs", rgb(hue, saturation * 0.94, value * 0.44));
    m.insert("leg_band", rgb(hue + 0.030, saturation * 0.86, value * 0.86));
    m.insert("leg_dark", rgb(hue, saturation, value * 0.21));
    m.insert("leg_tip", rgb(hue, saturation * 0.90, value * 0.17));
    m.insert("highlight", rgb(hue + 0.018, saturation * 0.64, value * 0.92));
    m.insert("eyes", rgb(hue + 0.090, 0.64, 0.97));
    m
}

/// A named palette (`Palette`).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: ColorMap,
}

impl Palette {
    /// `as_overrides`: a fresh copy.
    pub fn as_overrides(&self) -> ColorMap {
        self.colors.clone()
    }
}

const NAMED: [(&str, &str, f64, f64, f64); 12] = [
    ("cocoa", "Cocoa", 0.055, 0.62, 0.74),
    ("copper", "Copper", 0.075, 0.78, 0.88),
    ("ember", "Ember", 0.015, 0.74, 0.82),
    ("amber", "Amber", 0.115, 0.76, 0.92),
    ("moss", "Moss", 0.270, 0.52, 0.70),
    ("jade", "Jade", 0.430, 0.55, 0.76),
    ("frost", "Frost", 0.545, 0.44, 0.88),
    ("ink", "Ink", 0.620, 0.58, 0.60),
    ("plum", "Plum", 0.775, 0.54, 0.74),
    ("rose", "Rose", 0.925, 0.50, 0.90),
    ("ash", "Ash", 0.600, 0.08, 0.72),
    ("bone", "Bone", 0.100, 0.16, 0.95),
];

static NAMED_PALETTES: LazyLock<Vec<Palette>> = LazyLock::new(|| {
    NAMED.iter().map(|(id, name, h, s, v)| Palette { id, name, colors: palette_from_hue(*h, *s, *v) }).collect()
});

/// `NAMED_PALETTES`, spread around the hue circle.
pub fn named_palettes() -> &'static [Palette] {
    &NAMED_PALETTES
}

/// `PALETTE_BY_ID`.
pub fn palette_by_id(id: &str) -> Option<&'static Palette> {
    NAMED_PALETTES.iter().find(|p| p.id == id)
}

/// `random_palette(rng)`: hue, then saturation, then value, in that draw order.
pub fn random_palette(rng: &mut PyRandom) -> ColorMap {
    let hue = rng.random();
    let saturation = rng.uniform(0.34, 0.78);
    let value = rng.uniform(0.62, 0.94);
    palette_from_hue(hue, saturation, value)
}

/// `ENEMY_COLORS`.
pub const ENEMY_COLORS: [(&str, [u8; 3]); 12] = [
    ("body", [20, 11, 12]),
    ("legs", [30, 14, 16]),
    ("leg_band", [204, 20, 30]),
    ("leg_dark", [14, 7, 8]),
    ("leg_tip", [18, 8, 9]),
    ("highlight", [176, 34, 40]),
    ("eyes", [244, 58, 46]),
    ("rim", [190, 18, 28]),
    ("stripe_color", [212, 28, 36]),
    ("fluff_color", [74, 18, 22]),
    ("marking", [214, 24, 34]),
    ("shade_to", [34, 13, 15]),
];

/// `enemy_palette()`: overrides that make any model a black-and-crimson enemy.
pub fn enemy_palette() -> ColorMap {
    ENEMY_COLORS.iter().map(|(k, [r, g, b])| (k.to_string(), Color::rgb(*r, *g, *b))).collect()
}
