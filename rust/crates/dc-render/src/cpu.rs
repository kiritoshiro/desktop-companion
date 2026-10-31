//! CPU backend: rasterises a [`Scene`] with tiny-skia into an [`Image`].
//!
//! Used for tests, offscreen art (`Image::paint`) and as the software
//! overlay fallback. It honours every composition mode tiny-skia offers,
//! exact fill rules, clip paths (AA masks) and Qt's premultiplied gradient
//! interpolation (emulated by sub-dividing stops whose alpha differs).

use crate::image::Image;
use crate::paint::{Brush, Cap, CompositionMode, GradientStops, Join, Spread};
use crate::path::{FillRule, Path, PathEl};
use crate::scene::{Command, Op, SOLID, Scene, Shape};
use crate::text::{self, FaceId};
use crate::transform::Transform;
use dc_core::Color;
use std::cell::RefCell;
use std::collections::HashMap;
use tiny_skia as sk;

#[derive(Default)]
pub struct CpuRenderer {
    glyphs: HashMap<(FaceId, u16), Option<Path>>,
    mask: Option<(u32, sk::Mask)>,
}

thread_local! {
    static THREAD_RENDERER: RefCell<CpuRenderer> = RefCell::new(CpuRenderer::new());
}

pub(crate) fn with_thread_renderer<R>(f: impl FnOnce(&mut CpuRenderer) -> R) -> R {
    THREAD_RENDERER.with(|r| f(&mut r.borrow_mut()))
}

fn ts(m: &[f32; 6]) -> sk::Transform {
    sk::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5])
}

fn sk_color(c: Color, opacity: f32) -> sk::Color {
    sk::Color::from_rgba8(c.r, c.g, c.b, ((c.a as f32) * opacity).round().clamp(0.0, 255.0) as u8)
}

fn blend(m: CompositionMode) -> sk::BlendMode {
    use CompositionMode as C;
    use sk::BlendMode as B;
    match m {
        C::SourceOver => B::SourceOver,
        C::DestinationOver => B::DestinationOver,
        C::Clear => B::Clear,
        C::Source => B::Source,
        C::Destination => B::Destination,
        C::SourceIn => B::SourceIn,
        C::DestinationIn => B::DestinationIn,
        C::SourceOut => B::SourceOut,
        C::DestinationOut => B::DestinationOut,
        C::SourceAtop => B::SourceAtop,
        C::DestinationAtop => B::DestinationAtop,
        C::Xor => B::Xor,
        C::Plus => B::Plus,
        C::Multiply => B::Multiply,
        C::Screen => B::Screen,
        C::Overlay => B::Overlay,
        C::Darken => B::Darken,
        C::Lighten => B::Lighten,
        C::ColorDodge => B::ColorDodge,
        C::ColorBurn => B::ColorBurn,
        C::HardLight => B::HardLight,
        C::SoftLight => B::SoftLight,
        C::Difference => B::Difference,
        C::Exclusion => B::Exclusion,
    }
}

fn spread(s: Spread) -> sk::SpreadMode {
    match s {
        Spread::Pad => sk::SpreadMode::Pad,
        Spread::Reflect => sk::SpreadMode::Reflect,
        Spread::Repeat => sk::SpreadMode::Repeat,
    }
}

/// tiny-skia interpolates straight colours; Qt interpolates premultiplied.
/// Intervals whose alpha changes get premultiplied sub-stops.
fn sk_stops(stops: &GradientStops, opacity: f32, reverse: bool) -> Vec<sk::GradientStop> {
    let s = stops.as_slice();
    let mut out = Vec::with_capacity(s.len() * 2);
    let to_sk = |p: [f32; 4]| {
        let a = p[3];
        if a <= 0.0 {
            sk::Color::from_rgba(0.0, 0.0, 0.0, 0.0).unwrap()
        } else {
            sk::Color::from_rgba((p[0] / a).min(1.0), (p[1] / a).min(1.0), (p[2] / a).min(1.0), (a * opacity).min(1.0))
                .unwrap()
        }
    };
    for (i, st) in s.iter().enumerate() {
        out.push((st.pos, to_sk(crate::paint::premul_f32(st.color))));
        if let Some(next) = s.get(i + 1)
            && next.color.a != st.color.a
            && next.pos > st.pos
        {
            const N: usize = 8;
            for k in 1..N {
                let t = st.pos + (next.pos - st.pos) * k as f32 / N as f32;
                out.push((t, to_sk(stops.eval_premul(t))));
            }
        }
    }
    if reverse {
        out.reverse();
        for o in &mut out {
            o.0 = 1.0 - o.0;
        }
    }
    out.into_iter().map(|(p, c)| sk::GradientStop::new(p, c)).collect()
}

fn shader(brush: &Brush, opacity: f32) -> Option<sk::Shader<'static>> {
    match brush {
        Brush::None => None,
        Brush::Solid(c) => Some(sk::Shader::SolidColor(sk_color(*c, opacity))),
        Brush::Linear(g) => {
            if g.stops.len() == 1 || (g.x1 == g.x2 && g.y1 == g.y2) {
                let s = g.stops.as_slice();
                let c = if g.x1 == g.x2 && g.y1 == g.y2 { s.last()? } else { s.first()? };
                return Some(sk::Shader::SolidColor(sk_color(c.color, opacity)));
            }
            sk::LinearGradient::new(
                sk::Point::from_xy(g.x1, g.y1),
                sk::Point::from_xy(g.x2, g.y2),
                sk_stops(&g.stops, opacity, false),
                spread(g.spread),
                sk::Transform::identity(),
            )
        }
        Brush::Radial(g) => {
            if g.stops.len() == 1 {
                return Some(sk::Shader::SolidColor(sk_color(g.stops.as_slice()[0].color, opacity)));
            }
            sk::RadialGradient::new(
                sk::Point::from_xy(g.fx, g.fy),
                g.focal_radius,
                sk::Point::from_xy(g.cx, g.cy),
                g.radius,
                sk_stops(&g.stops, opacity, false),
                spread(g.spread),
                sk::Transform::identity(),
            )
        }
        Brush::Conical(g) => {
            if g.stops.len() == 1 {
                return Some(sk::Shader::SolidColor(sk_color(g.stops.as_slice()[0].color, opacity)));
            }
            let mut t = Transform::from_rotate_deg(-(g.angle as f64));
            t.dx = g.cx as f64;
            t.dy = g.cy as f64;
            sk::SweepGradient::new(
                sk::Point::from_xy(0.0, 0.0),
                0.0,
                360.0,
                sk_stops(&g.stops, opacity, true),
                sk::SpreadMode::Pad,
                ts(&t.to_f32()),
            )
        }
    }
}

fn push_path(pb: &mut sk::PathBuilder, path: &Path, t: Option<&Transform>) {
    let m = |p: [f32; 2]| match t {
        Some(t) => {
            let (x, y) = t.map(p[0] as f64, p[1] as f64);
            (x as f32, y as f32)
        }
        None => (p[0], p[1]),
    };
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                let (x, y) = m(p);
                pb.move_to(x, y)
            }
            PathEl::LineTo(p) => {
                let (x, y) = m(p);
                pb.line_to(x, y)
            }
            PathEl::QuadTo(c, p) => {
                let (cx, cy) = m(c);
                let (x, y) = m(p);
                pb.quad_to(cx, cy, x, y)
            }
            PathEl::CubicTo(a, b, p) => {
                let (ax, ay) = m(a);
                let (bx, by) = m(b);
                let (x, y) = m(p);
                pb.cubic_to(ax, ay, bx, by, x, y)
            }
            PathEl::Close => pb.close(),
        }
    }
}

fn shape_path(scene: &Scene, shape: &Shape) -> Option<sk::Path> {
    match *shape {
        Shape::Path { verbs, points } => {
            let mut pb = sk::PathBuilder::new();
            for el in scene.path_ref(verbs, points).elements() {
                match el {
                    PathEl::MoveTo(p) => pb.move_to(p[0], p[1]),
                    PathEl::LineTo(p) => pb.line_to(p[0], p[1]),
                    PathEl::QuadTo(c, p) => pb.quad_to(c[0], c[1], p[0], p[1]),
                    PathEl::CubicTo(a, b, p) => pb.cubic_to(a[0], a[1], b[0], b[1], p[0], p[1]),
                    PathEl::Close => pb.close(),
                }
            }
            pb.finish()
        }
        Shape::Ellipse { cx, cy, rx, ry } => {
            sk::Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0).map(sk::PathBuilder::from_oval)?
        }
        Shape::Rect { x, y, w, h } => {
            let (l, r) = if w < 0.0 { (x + w, x) } else { (x, x + w) };
            let (t, b) = if h < 0.0 { (y + h, y) } else { (y, y + h) };
            sk::Rect::from_ltrb(l, t, r, b).map(sk::PathBuilder::from_rect)
        }
        Shape::Line { x1, y1, x2, y2 } => {
            let mut pb = sk::PathBuilder::new();
            pb.move_to(x1, y1);
            pb.line_to(x2, y2);
            pb.finish()
        }
    }
}

fn fill_rule(r: FillRule) -> sk::FillRule {
    match r {
        FillRule::OddEven => sk::FillRule::EvenOdd,
        FillRule::Winding => sk::FillRule::Winding,
    }
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders into a new `width × height` image.
    pub fn render(&mut self, scene: &Scene, width: u32, height: u32, clear: Option<Color>) -> Image {
        let mut img = Image::new(width, height);
        self.render_into(scene, &mut img, clear);
        img
    }

    /// Renders on top of `image` (after filling it with `clear`, if given).
    pub fn render_into(&mut self, scene: &Scene, image: &mut Image, clear: Option<Color>) {
        if image.is_null() {
            return;
        }
        let (w, h) = image.size();
        self.mask = self.mask.take().map(|(_, m)| (u32::MAX, m));
        let data = image.data_mut();
        let Some(mut pm) = sk::PixmapMut::from_bytes(data, w, h) else { return };
        if let Some(c) = clear {
            pm.fill(sk_color(c, 1.0));
        }
        for cmd in &scene.cmds {
            self.draw(scene, cmd, &mut pm);
        }
    }

    fn prepare_mask(&mut self, scene: &Scene, clip: u32, w: u32, h: u32) -> bool {
        if clip == 0 {
            return false;
        }
        if let Some((id, m)) = &self.mask
            && *id == clip
            && m.width() == w
            && m.height() == h
        {
            return true;
        }
        let c = scene.clips[clip as usize];
        let mut mask = match self.mask.take() {
            Some((_, mut m)) if m.width() == w && m.height() == h => {
                m.clear();
                m
            }
            _ => match sk::Mask::new(w, h) {
                Some(m) => m,
                None => return false,
            },
        };
        let mut first = true;
        if let Some(s) = c.scissor {
            let r = sk::Rect::from_ltrb(s[0].round(), s[1].round(), s[2].round(), s[3].round());
            if let Some(r) = r {
                mask.fill_path(&sk::PathBuilder::from_rect(r), sk::FillRule::Winding, false, sk::Transform::identity());
            }
            first = false;
        }
        for i in c.shapes.0..c.shapes.1 {
            let cs = scene.clip_shapes[i as usize];
            if let Some(p) = shape_path(scene, &cs.shape) {
                if first {
                    mask.fill_path(&p, fill_rule(cs.rule), true, ts(&cs.transform));
                    first = false;
                } else {
                    mask.intersect_path(&p, fill_rule(cs.rule), true, ts(&cs.transform));
                }
            } else {
                mask.clear();
            }
        }
        self.mask = Some((clip, mask));
        true
    }

    fn draw(&mut self, scene: &Scene, cmd: &Command, pm: &mut sk::PixmapMut<'_>) {
        let (w, h) = (pm.width(), pm.height());
        // A scissor covering the whole target needs no mask.
        let clip = if cmd.clip != 0 {
            let c = scene.clips[cmd.clip as usize];
            let full = c.shapes.0 == c.shapes.1
                && c.scissor.is_some_and(|s| s[0] <= 0.0 && s[1] <= 0.0 && s[2] >= w as f32 && s[3] >= h as f32);
            if full { 0 } else { cmd.clip }
        } else {
            0
        };
        let has_mask = self.prepare_mask(scene, clip, w, h);
        let brush = if cmd.paint == SOLID { Brush::Solid(cmd.color) } else { scene.paints[cmd.paint as usize] };
        let t = ts(&cmd.transform);
        let blend_mode = blend(cmd.comp);
        match cmd.op {
            Op::Fill { shape, rule } => {
                let Some(shader) = shader(&brush, cmd.opacity) else { return };
                let Some(path) = shape_path(scene, &shape) else { return };
                let paint = sk::Paint { shader, blend_mode, anti_alias: cmd.aa, ..Default::default() };
                let mask = if has_mask { self.mask.as_ref().map(|m| &m.1) } else { None };
                pm.fill_path(&path, &paint, fill_rule(rule), t, mask);
            }
            Op::Stroke { shape, stroke } => {
                let st = scene.strokes[stroke as usize];
                let Some(shader) = shader(&brush, cmd.opacity) else { return };
                let Some(mut path) = shape_path(scene, &shape) else { return };
                let paint = sk::Paint { shader, blend_mode, anti_alias: cmd.aa, ..Default::default() };
                let mut stroke = sk::Stroke {
                    width: st.width,
                    miter_limit: st.miter_limit * 2.0,
                    line_cap: match st.cap {
                        Cap::Flat => sk::LineCap::Butt,
                        Cap::Square => sk::LineCap::Square,
                        Cap::Round => sk::LineCap::Round,
                    },
                    line_join: match st.join {
                        Join::Miter | Join::SvgMiter => sk::LineJoin::Miter,
                        Join::Bevel => sk::LineJoin::Bevel,
                        Join::Round => sk::LineJoin::Round,
                    },
                    dash: None,
                };
                let mut t2 = t;
                if st.cosmetic {
                    if st.width > 0.0 {
                        let Some(p) = path.transform(t) else { return };
                        path = p;
                        t2 = sk::Transform::identity();
                    } else {
                        stroke.width = 0.0;
                    }
                }
                let mask = if has_mask { self.mask.as_ref().map(|m| &m.1) } else { None };
                pm.stroke_path(&path, &paint, &stroke, t2, mask);
            }
            Op::Image { image, src, dst, smooth } => {
                let img = &scene.images[image as usize];
                let Some(pix) = sk::PixmapRef::from_bytes(img.data(), img.width(), img.height()) else { return };
                let mut pt = Transform::from_translate(-src[0] as f64, -src[1] as f64);
                pt = pt.then(&Transform::from_scale((dst[2] / src[2]) as f64, (dst[3] / src[3]) as f64));
                pt = pt.then(&Transform::from_translate(dst[0] as f64, dst[1] as f64));
                let quality = if smooth { sk::FilterQuality::Bilinear } else { sk::FilterQuality::Nearest };
                let shader = sk::Pattern::new(pix, sk::SpreadMode::Pad, quality, cmd.opacity, ts(&pt.to_f32()));
                let paint = sk::Paint { shader, blend_mode, anti_alias: cmd.aa, ..Default::default() };
                let (l, r) = if dst[2] < 0.0 { (dst[0] + dst[2], dst[0]) } else { (dst[0], dst[0] + dst[2]) };
                let (tp, b) = if dst[3] < 0.0 { (dst[1] + dst[3], dst[1]) } else { (dst[1], dst[1] + dst[3]) };
                let Some(rect) = sk::Rect::from_ltrb(l, tp, r, b) else { return };
                let mask = if has_mask { self.mask.as_ref().map(|m| &m.1) } else { None };
                pm.fill_rect(rect, &paint, t, mask);
            }
            Op::Glyphs { start, end, px } => {
                let Some(shader) = shader(&brush, cmd.opacity) else { return };
                let mut pb = sk::PathBuilder::new();
                for g in &scene.glyphs[start as usize..end as usize] {
                    let outline = self
                        .glyphs
                        .entry((g.face, g.glyph))
                        .or_insert_with(|| text::face(g.face).and_then(|f| text::glyph_outline_em(&f, g.glyph)));
                    if let Some(o) = outline {
                        let gt = Transform::new(px as f64, 0.0, 0.0, px as f64, g.x as f64, g.y as f64);
                        push_path(&mut pb, o, Some(&gt));
                    }
                }
                let Some(path) = pb.finish() else { return };
                let paint = sk::Paint { shader, blend_mode, anti_alias: true, ..Default::default() };
                let mask = if has_mask { self.mask.as_ref().map(|m| &m.1) } else { None };
                pm.fill_path(&path, &paint, sk::FillRule::Winding, t, mask);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Pen, RadialGradient, RenderHint};
    use dc_core::Rect;

    fn render(w: u32, h: u32, f: impl FnOnce(&mut Canvas)) -> Image {
        let mut s = Scene::new();
        {
            let mut c = s.canvas();
            c.set_render_hint(RenderHint::Antialiasing, true);
            f(&mut c);
        }
        CpuRenderer::new().render(&s, w, h, None)
    }

    #[test]
    fn fills_and_clear() {
        let img = render(20, 20, |c| {
            c.set_pen(Pen::NONE);
            c.set_brush(Color::rgb(255, 0, 0));
            c.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0));
            c.set_composition_mode(CompositionMode::Clear);
            c.draw_rect(Rect::new(0.0, 0.0, 10.0, 20.0));
        });
        assert_eq!(img.pixel(5, 5), Color::TRANSPARENT);
        assert_eq!(img.pixel(15, 5), Color::rgb(255, 0, 0));
    }

    #[test]
    fn source_atop_and_clip() {
        let img = render(20, 20, |c| {
            c.set_pen(Pen::NONE);
            c.set_brush(Color::rgb(0, 0, 255));
            c.draw_rect(Rect::new(0.0, 0.0, 10.0, 20.0));
            c.set_composition_mode(CompositionMode::SourceAtop);
            c.set_brush(Color::rgb(0, 255, 0));
            c.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0));
            c.set_composition_mode(CompositionMode::SourceOver);
            let mut p = Path::new();
            p.add_ellipse_c(15.0, 15.0, 3.0, 3.0);
            c.set_clip_path(&p);
            c.set_brush(Color::WHITE);
            c.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0));
        });
        assert_eq!(img.pixel(5, 5), Color::rgb(0, 255, 0));
        assert_eq!(img.pixel(15, 2), Color::TRANSPARENT);
        assert_eq!(img.pixel(15, 15), Color::WHITE);
        assert_eq!(img.pixel(5, 15), Color::rgb(0, 255, 0));
    }

    #[test]
    fn radial_gradient_premultiplied() {
        let img = render(41, 41, |c| {
            c.set_pen(Pen::NONE);
            let g = RadialGradient::new(20.5, 20.5, 20.0)
                .with_stop(0.0, Color::rgba(255, 255, 255, 255))
                .with_stop(1.0, Color::rgba(255, 0, 0, 0));
            c.set_brush(g);
            c.draw_rect(Rect::new(0.0, 0.0, 41.0, 41.0));
        });
        let centre = img.pixel(20, 20);
        assert!(centre.a > 245 && centre.g > 240);
        // Halfway out: premultiplied interpolation keeps it white-ish, not pink.
        let mid = img.pixel(30, 20);
        assert!((mid.a as i32 - 128).abs() < 12, "{mid:?}");
        assert!(mid.g > 235, "{mid:?}");
        assert_eq!(img.pixel(0, 0).a, 0);
    }

    #[test]
    fn image_draw_and_opacity() {
        let src = Image::filled(4, 4, Color::rgb(10, 200, 30));
        let img = render(16, 16, |c| {
            c.set_opacity(0.5);
            c.draw_image_rect(Rect::new(0.0, 0.0, 16.0, 16.0), &src);
        });
        let p = img.pixel(8, 8);
        assert!((p.r as i32 - 10).abs() <= 2 && (p.g as i32 - 200).abs() <= 2 && (p.b as i32 - 30).abs() <= 2);
        assert!((p.a as i32 - 128).abs() <= 1);
    }

    #[test]
    fn paint_on_existing_image() {
        let mut img = Image::filled(8, 8, Color::BLACK);
        img.paint(|c| {
            c.set_pen(Pen::NONE);
            c.fill_rect(Rect::new(0.0, 0.0, 4.0, 8.0), Color::WHITE);
        });
        assert_eq!(img.pixel(1, 1), Color::WHITE);
        assert_eq!(img.pixel(6, 1), Color::BLACK);
    }
}
