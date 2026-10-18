//! Brushes, gradients, pens and composition modes (`QBrush`, `QLinearGradient`,
//! `QRadialGradient`, `QConicalGradient`, `QPen`, `QPainter::CompositionMode`).
//!
//! Everything here is `Copy` and heap-free so it can be set per draw call
//! without allocation. Gradients keep up to [`MAX_STOPS`] stops inline.
//! Colours are straight (non-premultiplied) [`Color`]s, like `QColor`;
//! gradient interpolation happens in premultiplied space, like Qt's default
//! `QGradient::ColorInterpolation`.

use dc_core::{Color, Vec2};

/// Inline gradient stop capacity. The Python painters use at most 6.
pub const MAX_STOPS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub pos: f32,
    pub color: Color,
}

/// Sorted gradient stops with `QGradient::setColorAt` semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStops {
    len: u8,
    stops: [GradientStop; MAX_STOPS],
}

impl Default for GradientStops {
    fn default() -> Self {
        Self { len: 0, stops: [GradientStop { pos: 0.0, color: Color::TRANSPARENT }; MAX_STOPS] }
    }
}

impl GradientStops {
    /// `QGradient::setColorAt`: keeps stops sorted, replaces an equal position,
    /// ignores positions outside `0..=1` (Qt warns and ignores them).
    pub fn set_color_at(&mut self, pos: f64, color: Color) {
        if !(0.0..=1.0).contains(&pos) {
            log::warn!("gradient stop position {pos} outside 0..1 ignored");
            return;
        }
        let pos = pos as f32;
        let n = self.len as usize;
        let mut i = 0;
        while i < n && self.stops[i].pos < pos {
            i += 1;
        }
        if i < n && self.stops[i].pos == pos {
            self.stops[i].color = color;
            return;
        }
        if n == MAX_STOPS {
            log::warn!("gradient has more than {MAX_STOPS} stops; extra stop dropped");
            return;
        }
        self.stops.copy_within(i..n, i + 1);
        self.stops[i] = GradientStop { pos, color };
        self.len += 1;
    }

    pub fn as_slice(&self) -> &[GradientStop] {
        &self.stops[..self.len as usize]
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Premultiplied colour (0..1 floats) at `t` (already spread-adjusted).
    pub fn eval_premul(&self, t: f32) -> [f32; 4] {
        let s = self.as_slice();
        match s.len() {
            0 => [0.0; 4],
            1 => premul_f32(s[0].color),
            _ => {
                if t <= s[0].pos {
                    return premul_f32(s[0].color);
                }
                for w in s.windows(2) {
                    if t <= w[1].pos {
                        let span = w[1].pos - w[0].pos;
                        let f = if span <= 0.0 { 1.0 } else { (t - w[0].pos) / span };
                        let a = premul_f32(w[0].color);
                        let b = premul_f32(w[1].color);
                        return [
                            a[0] + (b[0] - a[0]) * f,
                            a[1] + (b[1] - a[1]) * f,
                            a[2] + (b[2] - a[2]) * f,
                            a[3] + (b[3] - a[3]) * f,
                        ];
                    }
                }
                premul_f32(s[s.len() - 1].color)
            }
        }
    }

    /// True when every stop is fully opaque.
    pub fn is_opaque(&self) -> bool {
        self.as_slice().iter().all(|s| s.color.a == 255)
    }
}

/// `QGradient::Spread`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Spread {
    #[default]
    Pad,
    Reflect,
    Repeat,
}

impl Spread {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Spread::Pad => t.clamp(0.0, 1.0),
            Spread::Repeat => t - t.floor(),
            Spread::Reflect => {
                let m = t.rem_euclid(2.0);
                if m > 1.0 { 2.0 - m } else { m }
            }
        }
    }
}

/// `QLinearGradient(x1, y1, x2, y2)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub stops: GradientStops,
    pub spread: Spread,
}

impl LinearGradient {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            x1: x1 as f32,
            y1: y1 as f32,
            x2: x2 as f32,
            y2: y2 as f32,
            stops: GradientStops::default(),
            spread: Spread::Pad,
        }
    }

    /// `QLinearGradient(QPointF, QPointF)`.
    pub fn from_points(a: impl Into<Vec2>, b: impl Into<Vec2>) -> Self {
        let (a, b) = (a.into(), b.into());
        Self::new(a.x, a.y, b.x, b.y)
    }

    pub fn set_color_at(&mut self, pos: f64, color: Color) {
        self.stops.set_color_at(pos, color);
    }

    pub fn with_stop(mut self, pos: f64, color: Color) -> Self {
        self.stops.set_color_at(pos, color);
        self
    }

    pub fn set_spread(&mut self, spread: Spread) {
        self.spread = spread;
    }

    /// Gradient parameter at a point (before spread).
    pub fn t_at(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (self.x2 - self.x1, self.y2 - self.y1);
        let l2 = dx * dx + dy * dy;
        if l2 <= 0.0 { 0.0 } else { ((x - self.x1) * dx + (y - self.y1) * dy) / l2 }
    }
}

/// `QRadialGradient(center, radius[, focal[, focalRadius]])` – Qt's extended
/// two-circle radial gradient; with the focal point at the centre it is the
/// plain radial gradient every Python painter uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialGradient {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub fx: f32,
    pub fy: f32,
    pub focal_radius: f32,
    pub stops: GradientStops,
    pub spread: Spread,
}

impl RadialGradient {
    /// `QRadialGradient(cx, cy, radius)`.
    pub fn new(cx: f64, cy: f64, radius: f64) -> Self {
        Self::with_focal(cx, cy, radius, cx, cy)
    }

    /// `QRadialGradient(QPointF center, radius)`.
    pub fn from_center(c: impl Into<Vec2>, radius: f64) -> Self {
        let c = c.into();
        Self::new(c.x, c.y, radius)
    }

    /// `QRadialGradient(cx, cy, radius, fx, fy)`. Like Qt's simple radial
    /// gradient, a focal point outside the circle is pulled just inside it.
    pub fn with_focal(cx: f64, cy: f64, radius: f64, fx: f64, fy: f64) -> Self {
        let (mut fx, mut fy) = (fx, fy);
        let (dx, dy) = (fx - cx, fy - cy);
        let d = dx.hypot(dy);
        let limit = radius * 0.9999;
        if d > limit && d > 0.0 {
            fx = cx + dx / d * limit;
            fy = cy + dy / d * limit;
        }
        Self {
            cx: cx as f32,
            cy: cy as f32,
            radius: radius.max(0.0) as f32,
            fx: fx as f32,
            fy: fy as f32,
            focal_radius: 0.0,
            stops: GradientStops::default(),
            spread: Spread::Pad,
        }
    }

    pub fn set_focal_point(&mut self, fx: f64, fy: f64) {
        let stops = self.stops;
        let spread = self.spread;
        *self = Self::with_focal(self.cx as f64, self.cy as f64, self.radius as f64, fx, fy);
        self.stops = stops;
        self.spread = spread;
    }

    pub fn set_color_at(&mut self, pos: f64, color: Color) {
        self.stops.set_color_at(pos, color);
    }

    pub fn with_stop(mut self, pos: f64, color: Color) -> Self {
        self.stops.set_color_at(pos, color);
        self
    }

    pub fn set_spread(&mut self, spread: Spread) {
        self.spread = spread;
    }

    pub fn is_simple(&self) -> bool {
        self.fx == self.cx && self.fy == self.cy && self.focal_radius == 0.0
    }

    /// Gradient parameter at a point (before spread); `None` outside the cone.
    pub fn t_at(&self, x: f32, y: f32) -> Option<f32> {
        let (cdx, cdy) = (self.cx - self.fx, self.cy - self.fy);
        let dr = self.radius - self.focal_radius;
        let (pdx, pdy) = (x - self.fx, y - self.fy);
        let a = cdx * cdx + cdy * cdy - dr * dr;
        let b = pdx * cdx + pdy * cdy + self.focal_radius * dr;
        let c = pdx * pdx + pdy * pdy - self.focal_radius * self.focal_radius;
        if a.abs() < 1e-9 {
            if b.abs() < 1e-12 {
                return None;
            }
            let t = c / (2.0 * b);
            return (self.focal_radius + t * dr >= 0.0).then_some(t);
        }
        let det = b * b - a * c;
        if det < 0.0 {
            return None;
        }
        let sq = det.sqrt();
        let t1 = (b + sq) / a;
        let t2 = (b - sq) / a;
        let (hi, lo) = if t1 > t2 { (t1, t2) } else { (t2, t1) };
        if self.focal_radius + hi * dr >= 0.0 {
            Some(hi)
        } else if self.focal_radius + lo * dr >= 0.0 {
            Some(lo)
        } else {
            None
        }
    }
}

/// `QConicalGradient(cx, cy, angle)`: colours sweep counter-clockwise from
/// `angle` degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConicalGradient {
    pub cx: f32,
    pub cy: f32,
    pub angle: f32,
    pub stops: GradientStops,
}

impl ConicalGradient {
    pub fn new(cx: f64, cy: f64, angle_deg: f64) -> Self {
        Self { cx: cx as f32, cy: cy as f32, angle: angle_deg as f32, stops: GradientStops::default() }
    }

    pub fn set_color_at(&mut self, pos: f64, color: Color) {
        self.stops.set_color_at(pos, color);
    }

    pub fn with_stop(mut self, pos: f64, color: Color) -> Self {
        self.stops.set_color_at(pos, color);
        self
    }

    /// `t` grows counter-clockwise on screen from 0 at `angle` (0° = 3 o'clock).
    pub fn t_at(&self, x: f32, y: f32) -> f32 {
        let a = (self.cy - y).atan2(x - self.cx) - self.angle.to_radians();
        let t = a / std::f32::consts::TAU;
        t - t.floor()
    }
}

/// `QBrush`: what fills shapes.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Brush {
    /// `Qt.NoBrush`.
    #[default]
    None,
    Solid(Color),
    Linear(LinearGradient),
    Radial(RadialGradient),
    Conical(ConicalGradient),
}

impl Brush {
    pub const NONE: Brush = Brush::None;

    pub fn is_none(&self) -> bool {
        matches!(self, Brush::None)
    }

    /// The solid colour, if this is a solid brush.
    pub fn solid(&self) -> Option<Color> {
        match self {
            Brush::Solid(c) => Some(*c),
            _ => None,
        }
    }

    /// Premultiplied colour (0..1) at a point in brush space; the reference
    /// evaluator used by the CPU backend for conical gradients and by tests.
    pub fn eval_premul(&self, x: f32, y: f32) -> [f32; 4] {
        match self {
            Brush::None => [0.0; 4],
            Brush::Solid(c) => premul_f32(*c),
            Brush::Linear(g) => g.stops.eval_premul(g.spread.apply(g.t_at(x, y))),
            Brush::Radial(g) => match g.t_at(x, y) {
                Some(t) => g.stops.eval_premul(g.spread.apply(t)),
                None => [0.0; 4],
            },
            Brush::Conical(g) => g.stops.eval_premul(g.t_at(x, y)),
        }
    }
}

impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Brush::Solid(c)
    }
}
impl From<LinearGradient> for Brush {
    fn from(g: LinearGradient) -> Self {
        Brush::Linear(g)
    }
}
impl From<RadialGradient> for Brush {
    fn from(g: RadialGradient) -> Self {
        Brush::Radial(g)
    }
}
impl From<ConicalGradient> for Brush {
    fn from(g: ConicalGradient) -> Self {
        Brush::Conical(g)
    }
}
impl From<Option<Color>> for Brush {
    fn from(c: Option<Color>) -> Self {
        c.map_or(Brush::None, Brush::Solid)
    }
}

/// `Qt::PenCapStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Cap {
    Flat,
    /// Qt's default.
    #[default]
    Square,
    Round,
}

/// `Qt::PenJoinStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Join {
    Miter,
    /// Qt's default.
    #[default]
    Bevel,
    Round,
    SvgMiter,
}

/// `Qt::PenStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PenStyle {
    NoPen,
    #[default]
    Solid,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
    CustomDash,
}

pub const MAX_DASHES: usize = 8;

/// A dash pattern in units of the pen width (`QPen::setDashPattern`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DashPattern {
    len: u8,
    values: [f32; MAX_DASHES],
}

impl DashPattern {
    pub fn new(values: &[f64]) -> Self {
        let mut d = DashPattern::default();
        for &v in values.iter().take(MAX_DASHES) {
            d.values[d.len as usize] = v.max(0.0) as f32;
            d.len += 1;
        }
        // Qt requires an even count; an odd one repeats its last value.
        if d.len % 2 == 1 && (d.len as usize) < MAX_DASHES {
            d.values[d.len as usize] = d.values[d.len as usize - 1];
            d.len += 1;
        }
        d
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values[..self.len as usize]
    }

    /// Qt's built-in patterns (`QPenPrivate::dashPattern`).
    pub fn for_style(style: PenStyle) -> Self {
        match style {
            PenStyle::Dash => Self::new(&[4.0, 2.0]),
            PenStyle::Dot => Self::new(&[1.0, 2.0]),
            PenStyle::DashDot => Self::new(&[4.0, 2.0, 1.0, 2.0]),
            PenStyle::DashDotDot => Self::new(&[4.0, 2.0, 1.0, 2.0, 1.0, 2.0]),
            _ => Self::default(),
        }
    }
}

/// `QPen`. `Pen::from(color)` is `QPen(color)`: width 1, solid, square cap,
/// bevel join, miter limit 2 – Qt 5 defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub brush: Brush,
    pub width: f64,
    pub style: PenStyle,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f64,
    pub dash: DashPattern,
    pub dash_offset: f64,
    /// Width in device pixels regardless of transform. Width 0 is always cosmetic.
    pub cosmetic: bool,
}

impl Default for Pen {
    /// `QPen()`: solid black, width 1.
    fn default() -> Self {
        Pen::new(Color::BLACK, 1.0)
    }
}

impl Pen {
    /// `Qt.NoPen`.
    pub const NONE: Pen = Pen {
        brush: Brush::None,
        width: 1.0,
        style: PenStyle::NoPen,
        cap: Cap::Square,
        join: Join::Bevel,
        miter_limit: 2.0,
        dash: DashPattern { len: 0, values: [0.0; MAX_DASHES] },
        dash_offset: 0.0,
        cosmetic: false,
    };

    /// `QPen(color, width)`.
    pub fn new(color: Color, width: f64) -> Self {
        Pen { brush: Brush::Solid(color), style: PenStyle::Solid, width, ..Pen::NONE }
    }

    /// `QPen(brush, width, style, cap, join)` – the five-argument constructor.
    pub fn full(brush: impl Into<Brush>, width: f64, style: PenStyle, cap: Cap, join: Join) -> Self {
        let mut p = Pen { brush: brush.into(), width, style, cap, join, ..Pen::NONE };
        p.dash = DashPattern::for_style(style);
        p
    }

    pub fn with_brush(brush: impl Into<Brush>, width: f64) -> Self {
        Pen { brush: brush.into(), style: PenStyle::Solid, width, ..Pen::NONE }
    }

    pub fn cap(mut self, cap: Cap) -> Self {
        self.cap = cap;
        self
    }
    pub fn join(mut self, join: Join) -> Self {
        self.join = join;
        self
    }
    pub fn style(mut self, style: PenStyle) -> Self {
        self.set_style(style);
        self
    }
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    pub fn dashes(mut self, pattern: &[f64]) -> Self {
        self.set_dash_pattern(pattern);
        self
    }

    pub fn set_cap_style(&mut self, cap: Cap) {
        self.cap = cap;
    }
    pub fn set_join_style(&mut self, join: Join) {
        self.join = join;
    }
    pub fn set_width_f(&mut self, w: f64) {
        self.width = w;
    }
    pub fn set_color(&mut self, c: Color) {
        self.brush = Brush::Solid(c);
    }
    pub fn set_brush(&mut self, b: impl Into<Brush>) {
        self.brush = b.into();
    }
    pub fn set_miter_limit(&mut self, l: f64) {
        self.miter_limit = l;
    }
    pub fn set_cosmetic(&mut self, c: bool) {
        self.cosmetic = c;
    }
    pub fn set_dash_offset(&mut self, o: f64) {
        self.dash_offset = o;
    }
    pub fn set_style(&mut self, style: PenStyle) {
        self.style = style;
        if style != PenStyle::CustomDash {
            self.dash = DashPattern::for_style(style);
        }
    }
    /// `QPen::setDashPattern` (also switches the style to `CustomDashLine`).
    pub fn set_dash_pattern(&mut self, pattern: &[f64]) {
        self.dash = DashPattern::new(pattern);
        self.style = PenStyle::CustomDash;
    }

    /// Draws nothing (`Qt.NoPen` or a `NoBrush` brush).
    pub fn is_none(&self) -> bool {
        self.style == PenStyle::NoPen || self.brush.is_none()
    }

    pub fn is_cosmetic(&self) -> bool {
        self.cosmetic || self.width <= 0.0
    }

    pub fn is_dashed(&self) -> bool {
        !matches!(self.style, PenStyle::Solid | PenStyle::NoPen) && !self.dash.as_slice().is_empty()
    }
}

impl From<Color> for Pen {
    fn from(c: Color) -> Self {
        Pen::new(c, 1.0)
    }
}

impl From<Option<Color>> for Pen {
    fn from(c: Option<Color>) -> Self {
        c.map_or(Pen::NONE, Pen::from)
    }
}

/// `QPainter::CompositionMode`. The GPU backend implements every Porter-Duff
/// mode (the first twelve); the blend modes after `Plus` are CPU-only and
/// fall back to `SourceOver` on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CompositionMode {
    #[default]
    SourceOver,
    DestinationOver,
    Clear,
    Source,
    Destination,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    Xor,
    Plus,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

/// `QPainter::RenderHint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderHint {
    Antialiasing,
    TextAntialiasing,
    SmoothPixmapTransform,
}

/// Premultiplied 0..1 floats from a straight colour.
#[inline]
pub fn premul_f32(c: Color) -> [f32; 4] {
    let a = c.a as f32 / 255.0;
    [c.r as f32 / 255.0 * a, c.g as f32 / 255.0 * a, c.b as f32 / 255.0 * a, a]
}

/// Premultiplied RGBA8 from a straight colour (rounded like Qt's `qPremultiply`).
#[inline]
pub fn premultiply(c: Color) -> [u8; 4] {
    let a = c.a as u32;
    let m = |v: u8| ((v as u32 * a + 127) / 255) as u8;
    [m(c.r), m(c.g), m(c.b), c.a]
}

/// Straight colour from premultiplied RGBA8.
#[inline]
pub fn unpremultiply(p: [u8; 4]) -> Color {
    let a = p[3] as u32;
    if a == 0 {
        return Color::TRANSPARENT;
    }
    if a == 255 {
        return Color::rgba(p[0], p[1], p[2], 255);
    }
    let u = |v: u8| ((v as u32 * 255 + a / 2) / a).min(255) as u8;
    Color::rgba(u(p[0]), u(p[1]), u(p[2]), p[3])
}

/// Parses `QColor(str)` forms: `#rgb`, `#rrggbb`, `#aarrggbb` (Qt order) and
/// the common SVG colour names used by the Python code.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let v = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
        return match hex.len() {
            3 => Some(Color::rgb(v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17)),
            6 => Some(Color::rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
            8 => Some(Color::rgba(v(2, 2)?, v(4, 2)?, v(6, 2)?, v(0, 2)?)),
            _ => None,
        };
    }
    Some(match s.to_ascii_lowercase().as_str() {
        "black" => Color::BLACK,
        "white" => Color::WHITE,
        "transparent" => Color::TRANSPARENT,
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "lime" => Color::rgb(0, 255, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "cyan" => Color::rgb(0, 255, 255),
        "magenta" => Color::rgb(255, 0, 255),
        "gray" | "grey" => Color::rgb(128, 128, 128),
        "darkgray" | "darkgrey" => Color::rgb(169, 169, 169),
        "lightgray" | "lightgrey" => Color::rgb(211, 211, 211),
        "orange" => Color::rgb(255, 165, 0),
        "gold" => Color::rgb(255, 215, 0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_sort_and_replace_like_qt() {
        let mut s = GradientStops::default();
        s.set_color_at(1.0, Color::WHITE);
        s.set_color_at(0.0, Color::BLACK);
        s.set_color_at(0.5, Color::rgb(255, 0, 0));
        s.set_color_at(0.5, Color::rgb(0, 0, 255));
        s.set_color_at(1.5, Color::rgb(1, 2, 3));
        let v: Vec<f32> = s.as_slice().iter().map(|s| s.pos).collect();
        assert_eq!(v, vec![0.0, 0.5, 1.0]);
        assert_eq!(s.as_slice()[1].color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn premultiplied_interpolation() {
        let g = LinearGradient::new(0.0, 0.0, 10.0, 0.0)
            .with_stop(0.0, Color::rgba(255, 0, 0, 255))
            .with_stop(1.0, Color::rgba(0, 0, 255, 0));
        let c = Brush::Linear(g).eval_premul(5.0, 0.0);
        // Premultiplied: transparent end contributes no blue.
        assert!((c[0] - 0.5).abs() < 1e-6 && c[2].abs() < 1e-6 && (c[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn radial_t_simple_and_focal() {
        let g = RadialGradient::new(10.0, 10.0, 5.0);
        assert!((g.t_at(10.0, 10.0).unwrap()).abs() < 1e-6);
        assert!((g.t_at(15.0, 10.0).unwrap() - 1.0).abs() < 1e-5);
        assert!((g.t_at(10.0, 12.5).unwrap() - 0.5).abs() < 1e-5);
        let f = RadialGradient::with_focal(0.0, 0.0, 10.0, 5.0, 0.0);
        assert!(f.t_at(5.0, 0.0).unwrap().abs() < 1e-5);
        assert!((f.t_at(10.0, 0.0).unwrap() - 1.0).abs() < 1e-4);
        assert!((f.t_at(-10.0, 0.0).unwrap() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn conical_and_spread() {
        let g = ConicalGradient::new(0.0, 0.0, 0.0);
        assert!(g.t_at(1.0, 0.0).abs() < 1e-5 || (g.t_at(1.0, 0.0) - 1.0).abs() < 1e-5);
        // Counter-clockwise on screen (towards -y) increases t.
        assert!((g.t_at(0.0, -1.0) - 0.25).abs() < 1e-5);
        assert_eq!(Spread::Reflect.apply(1.25), 0.75);
        assert_eq!(Spread::Repeat.apply(1.25), 0.25);
        assert_eq!(Spread::Pad.apply(-3.0), 0.0);
    }

    #[test]
    fn pen_defaults_and_colors() {
        let p = Pen::from(Color::WHITE);
        assert_eq!((p.width, p.cap, p.join, p.miter_limit), (1.0, Cap::Square, Join::Bevel, 2.0));
        assert!(Pen::NONE.is_none());
        let d = Pen::new(Color::BLACK, 2.0).style(PenStyle::Dash);
        assert_eq!(d.dash.as_slice(), &[4.0, 2.0]);
        assert_eq!(parse_color("#f7df93"), Some(Color::rgb(0xf7, 0xdf, 0x93)));
        assert_eq!(parse_color("#80ff0000"), Some(Color::rgba(255, 0, 0, 0x80)));
        let rt = unpremultiply(premultiply(Color::rgba(200, 100, 50, 128)));
        assert!((rt.r as i32 - 200).abs() <= 1 && rt.g == 100 && rt.a == 128);
    }
}
