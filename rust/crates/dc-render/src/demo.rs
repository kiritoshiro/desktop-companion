//! Deterministic demo scenes for tests, the benchmark and the smoke example.
//! Not part of the port; nothing in the app depends on these.

use crate::{
    Canvas, Cap, Color, CompositionMode, Image, Join, LinearGradient, Path, Pen, PenStyle, RadialGradient, Rect,
    RenderHint,
};

/// Tiny deterministic generator (xorshift), so scenes need no RNG crate.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

/// One crude procedural spider at `(x, y)`: shadow, eight three-segment
/// curved legs with joints, hair and feet, two gradient body parts, a
/// marking and eyes – about 100 primitives, like `render_procedural`.
pub fn draw_spider(c: &mut Canvas<'_>, x: f64, y: f64, size: f64, heading_deg: f64, phase: f64, hue: f64) {
    let body = Color::from_hsv(hue, 0.55, 0.45, 255);
    let dark = body.darker(180);
    c.save();
    c.translate(x, y);
    c.rotate(heading_deg);
    // Shadow.
    c.set_pen(Pen::NONE);
    c.set_brush(Color::rgba(0, 0, 0, 60));
    c.draw_ellipse_at(size * 0.05, size * 0.12, size * 0.9, size * 0.45);
    // Legs.
    for side in [-1.0f64, 1.0] {
        for i in 0..4 {
            let a = (-50.0 + i as f64 * 33.0 + (phase + i as f64 * 1.7).sin() * 8.0).to_radians();
            let (ax, ay) = (size * 0.15 * a.cos(), side * size * 0.18);
            let reach = size * (1.25 + 0.1 * (i as f64 % 2.0));
            let knee = (ax + reach * 0.45 * a.cos(), ay + side * reach * 0.55 * a.sin().abs().max(0.4));
            let ankle = (ax + reach * 0.85 * a.cos(), ay + side * reach * 0.8);
            let foot = (ax + reach * a.cos() * 1.05, ay + side * reach * 0.95);
            let w = size * 0.07;
            let segs = [((ax, ay), knee, w), (knee, ankle, w * 0.8), (ankle, foot, w * 0.55)];
            for (k, (p0, p1, sw)) in segs.iter().enumerate() {
                let mut p = Path::new();
                p.move_to(p0.0, p0.1);
                let mx = (p0.0 + p1.0) * 0.5 + side * size * 0.04;
                let my = (p0.1 + p1.1) * 0.5 - size * 0.05;
                p.cubic_to(mx, my, mx, my, p1.0, p1.1);
                c.set_brush(crate::Brush::None);
                c.set_pen(Pen::full(if k == 2 { dark } else { body }, *sw, PenStyle::Solid, Cap::Round, Join::Round));
                c.draw_path(&p);
                // Hair.
                if k < 2 {
                    c.set_pen(Pen::full(dark.with_alpha(160), sw * 0.25, PenStyle::Solid, Cap::Round, Join::Round));
                    let (hx, hy) = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
                    c.draw_line(hx, hy, hx + side * size * 0.06, hy - size * 0.04);
                }
            }
            c.set_pen(Pen::NONE);
            c.set_brush(dark);
            c.draw_ellipse_at(knee.0, knee.1, w * 0.6, w * 0.6);
            c.draw_ellipse_at(ankle.0, ankle.1, w * 0.5, w * 0.5);
            c.set_brush(Color::rgb(20, 15, 10));
            c.draw_ellipse_at(foot.0, foot.1, w * 0.4, w * 0.3);
        }
    }
    // Abdomen and cephalothorax with radial gradients.
    c.set_pen(Pen::new(dark, size * 0.03));
    let ab = RadialGradient::new(-size * 0.55, -size * 0.12, size * 0.6)
        .with_stop(0.0, body.lighter(160))
        .with_stop(0.6, body)
        .with_stop(1.0, dark);
    c.set_brush(ab);
    c.draw_ellipse_at(-size * 0.45, 0.0, size * 0.5, size * 0.38);
    let ce = RadialGradient::new(size * 0.2, -size * 0.08, size * 0.35)
        .with_stop(0.0, body.lighter(140))
        .with_stop(1.0, dark);
    c.set_brush(ce);
    c.draw_ellipse_at(size * 0.18, 0.0, size * 0.28, size * 0.24);
    // Marking.
    let mut m = Path::new();
    m.move_to(-size * 0.7, 0.0);
    m.quad_to(-size * 0.45, -size * 0.2, -size * 0.2, 0.0);
    m.quad_to(-size * 0.45, size * 0.2, -size * 0.7, 0.0);
    m.close_subpath();
    c.set_pen(Pen::NONE);
    c.set_brush(Color::rgba(230, 200, 120, 150));
    c.draw_path(&m);
    // Eyes.
    for (ex, ey) in [(0.36, -0.08), (0.36, 0.08), (0.32, -0.15), (0.32, 0.15)] {
        c.set_brush(Color::rgb(10, 10, 10));
        c.draw_ellipse_at(size * ex, size * ey, size * 0.035, size * 0.035);
        c.set_brush(Color::rgba(255, 255, 255, 200));
        c.draw_ellipse_at(size * ex + size * 0.01, size * ey - size * 0.01, size * 0.012, size * 0.012);
    }
    c.restore();
}

/// `count` spiders scattered over a `w × h` desktop.
pub fn spider_field(c: &mut Canvas<'_>, count: usize, w: f64, h: f64, seed: u64) {
    c.set_render_hint(RenderHint::Antialiasing, true);
    let mut r = Lcg::new(seed);
    for _ in 0..count {
        let (x, y) = (r.range(40.0, w - 40.0), r.range(40.0, h - 40.0));
        let size = r.range(14.0, 30.0);
        draw_spider(c, x, y, size, r.range(0.0, 360.0), r.range(0.0, std::f64::consts::TAU), r.range(0.0, 360.0));
    }
}

/// A feature sampler: gradients, strokes, dashes, clips, images, composition.
/// Contains no text, so it renders identically with or without fonts.
pub fn feature_scene(c: &mut Canvas<'_>) {
    c.set_render_hint(RenderHint::Antialiasing, true);
    c.set_render_hint(RenderHint::SmoothPixmapTransform, true);
    // Background.
    let bg = LinearGradient::new(0.0, 0.0, 0.0, 240.0)
        .with_stop(0.0, Color::rgb(40, 60, 90))
        .with_stop(1.0, Color::rgb(10, 15, 25));
    c.fill_rect(Rect::new(0.0, 0.0, 320.0, 240.0), bg);
    // Gradient ellipse with outline.
    c.set_pen(Pen::new(Color::rgb(250, 230, 160), 3.0));
    c.set_brush(
        RadialGradient::new(70.0, 60.0, 50.0)
            .with_stop(0.0, Color::rgb(255, 240, 200))
            .with_stop(0.5, Color::rgb(220, 120, 40))
            .with_stop(1.0, Color::rgba(120, 30, 10, 0)),
    );
    c.draw_ellipse_at(70.0, 70.0, 50.0, 40.0);
    // Rounded rect, rotated.
    c.save();
    c.translate(200.0, 70.0);
    c.rotate(20.0);
    c.set_pen(Pen::full(Color::rgb(90, 200, 255), 2.0, PenStyle::Dash, Cap::Flat, Join::Miter));
    c.set_brush(Color::rgba(90, 200, 255, 70));
    c.draw_rounded_rect(Rect::new(-60.0, -30.0, 120.0, 60.0), 12.0, 12.0);
    c.restore();
    // Polyline with joins.
    c.set_brush(crate::Brush::None);
    c.set_pen(Pen::full(Color::rgba(255, 255, 255, 180), 8.0, PenStyle::Solid, Cap::Round, Join::Round));
    c.draw_polyline(&[(20.0, 200.0), (60.0, 140.0), (100.0, 210.0), (140.0, 150.0)]);
    c.set_pen(Pen::full(Color::rgba(255, 120, 120, 200), 6.0, PenStyle::Solid, Cap::Square, Join::Bevel));
    c.draw_polyline(&[(20.0, 230.0), (70.0, 180.0), (120.0, 232.0)]);
    // Thin silk lines.
    c.set_pen(Pen::new(Color::rgba(230, 230, 255, 200), 0.6));
    for i in 0..12 {
        let a = i as f64 * 15.0f64.to_radians();
        c.draw_line(250.0, 180.0, 250.0 + 55.0 * a.cos(), 180.0 + 55.0 * a.sin());
    }
    // Clip path + image + SourceAtop.
    let mut img = Image::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            let on = (x / 8 + y / 8) % 2 == 0;
            img.set_pixel(x, y, if on { Color::rgb(240, 240, 240) } else { Color::rgb(60, 160, 90) });
        }
    }
    c.save();
    let mut clip = Path::new();
    clip.add_ellipse_c(180.0, 180.0, 40.0, 40.0);
    c.set_clip_path(&clip);
    c.draw_image_rect(Rect::new(140.0, 140.0, 80.0, 80.0), &img);
    c.set_composition_mode(CompositionMode::SourceAtop);
    c.set_pen(Pen::NONE);
    c.set_brush(Color::rgba(255, 0, 0, 90));
    c.draw_rect(Rect::new(140.0, 140.0, 40.0, 80.0));
    c.restore();
    // Clear a hole.
    c.set_composition_mode(CompositionMode::Clear);
    c.set_pen(Pen::NONE);
    c.set_brush(Color::BLACK);
    c.draw_ellipse_at(300.0, 20.0, 14.0, 14.0);
    c.set_composition_mode(CompositionMode::SourceOver);
    // A small spider and an opacity-faded copy.
    draw_spider(c, 140.0, 100.0, 22.0, 30.0, 0.0, 30.0);
    c.set_opacity(0.5);
    draw_spider(c, 290.0, 120.0, 16.0, 200.0, 1.0, 200.0);
}
