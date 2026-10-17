//! 8-bit RGBA colour shared by content, painters and renderers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Clamp-and-round constructor, like `QColor(int(...))` call sites.
    pub fn from_f64(r: f64, g: f64, b: f64, a: f64) -> Self {
        let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        Self { r: c(r), g: c(g), b: c(b), a: c(a) }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// `QColor.lighter(factor)` approximation in HSV value space (factor in percent).
    pub fn lighter(self, factor: u32) -> Self {
        let (h, s, v) = self.to_hsv();
        let mut v = v * factor as f64 / 100.0;
        let mut s = s;
        if v > 1.0 {
            s = (s - (v - 1.0)).max(0.0);
            v = 1.0;
        }
        Self::from_hsv(h, s, v, self.a)
    }

    /// `QColor.darker(factor)` (factor in percent).
    pub fn darker(self, factor: u32) -> Self {
        if factor == 0 {
            return self;
        }
        let (h, s, v) = self.to_hsv();
        Self::from_hsv(h, s, v * 100.0 / factor as f64, self.a)
    }

    /// Linear blend: `t = 0` gives `self`, `t = 1` gives `other`.
    pub fn mix(self, other: Color, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| a as f64 + (b as f64 - a as f64) * t;
        Self::from_f64(m(self.r, other.r), m(self.g, other.g), m(self.b, other.b), m(self.a, other.a))
    }

    pub fn to_hsv(self) -> (f64, f64, f64) {
        let r = self.r as f64 / 255.0;
        let g = self.g as f64 / 255.0;
        let b = self.b as f64 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / d) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };
        let s = if max == 0.0 { 0.0 } else { d / max };
        (h, s, max)
    }

    pub fn from_hsv(h: f64, s: f64, v: f64, a: u8) -> Self {
        let c = v * s;
        let hp = (h % 360.0) / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r1, g1, b1) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        Self::from_f64((r1 + m) * 255.0, (g1 + m) * 255.0, (b1 + m) * 255.0, a as f64)
    }

    pub fn to_array_f32(self) -> [f32; 4] {
        [self.r as f32 / 255.0, self.g as f32 / 255.0, self.b as f32 / 255.0, self.a as f32 / 255.0]
    }
}

/// JSON form used by the Python files: `[r, g, b]` or `[r, g, b, a]`.
impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.a == 255 {
            [self.r, self.g, self.b].serialize(s)
        } else {
            [self.r, self.g, self.b, self.a].serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: Vec<f64> = Vec::deserialize(d)?;
        match v.as_slice() {
            [r, g, b] => Ok(Color::from_f64(*r, *g, *b, 255.0)),
            [r, g, b, a] => Ok(Color::from_f64(*r, *g, *b, *a)),
            _ => Err(serde::de::Error::custom("colour must be [r, g, b] or [r, g, b, a]")),
        }
    }
}
