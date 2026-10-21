//! Port of the drawing half of `creature/web_net.py`: `web_strength`,
//! `_net_points` and `_draw_web_net`, the silk laid over a webbed spider.

use crate::draw::CreatureDraw;
use dc_core::Color;
use dc_core::math::{clamp, py_round};
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen, PenStyle};

/// `NET_RINGS`.
pub const NET_RINGS: [f64; 4] = [0.28, 0.52, 0.76, 1.0];
/// `NET_OUTER`: the outer ring, in body sizes.
pub const NET_OUTER: f64 = 1.35;
/// `NET_SAG`.
pub const NET_SAG: f64 = 0.16;
/// `creature/constants.py` `WEBBED_SECONDS`.
pub const WEBBED_SECONDS: f64 = 4.6;

impl CreatureDraw {
    /// `web_strength()`: 1 for a fresh net, falling to 0.
    pub fn web_strength(&self) -> f64 {
        clamp(self.webbed_timer / WEBBED_SECONDS, 0.0, 1.0)
    }

    /// `_net_points()`: each spoke's ring points in world space.
    pub fn net_points(&self) -> Vec<Vec<(f64, f64)>> {
        let strain = 1.0 + 0.035 * self.struggle * (self.struggle_clock * 27.0).sin();
        let outer = self.size * NET_OUTER * strain;
        let (cx, cy) = (self.x, self.y - self.jump_z);
        self.web_net
            .iter()
            .map(|spoke| {
                let (c, s) = (spoke.angle.cos(), spoke.angle.sin());
                spoke.rings.iter().map(|r| (cx + c * outer * r, cy + s * outer * r)).collect()
            })
            .collect()
    }
}

fn round_pen(color: Color, width: f64) -> Pen {
    Pen::full(color, width, PenStyle::Solid, Cap::Round, Join::Round)
}

/// `_draw_web_net(painter)`.
pub fn draw_web_net(p: &mut Canvas, d: &CreatureDraw) {
    if !d.webbed || d.dead || d.web_net.is_empty() {
        return;
    }
    let strength = d.web_strength();
    let shimmer = 0.5 + 0.5 * (d.struggle_clock * 27.0).sin();
    let alpha = clamp(110.0 + 130.0 * strength + 25.0 * d.struggle * shimmer, 0.0, 255.0) as i64;
    let width = (d.size * 0.030).max(0.9);
    let (cx, cy) = (d.x, d.y - d.jump_z);
    let points = d.net_points();
    let count = points.len();

    let mut threads = Path::new();
    for (spoke, pts) in d.web_net.iter().zip(&points) {
        if spoke.tear > strength {
            continue;
        }
        let hub = pts[0];
        threads.move_to(cx + (hub.0 - cx) * 0.25, cy + (hub.1 - cy) * 0.25);
        threads.line_to(spoke.anchor.0, spoke.anchor.1);
    }
    let mut dew: Vec<(f64, f64)> = Vec::new();
    for ring in 0..NET_RINGS.len() {
        for i in 0..count {
            let spoke = &d.web_net[i];
            let pts = &points[i];
            let next = &points[(i + 1) % count];
            if spoke.ring_tears[ring] > strength * 1.05 {
                continue;
            }
            let (a, b) = (pts[ring], next[ring]);
            let (mid_x, mid_y) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let control = (mid_x + (cx - mid_x) * NET_SAG, mid_y + (cy - mid_y) * NET_SAG);
            threads.move_to(a.0, a.1);
            threads.quad_to(control.0, control.1, b.0, b.1);
            dew.push(a);
        }
    }

    let mut wrap = Path::new();
    let (hc, hs) = (d.heading.cos(), d.heading.sin());
    let body_point = |forward: f64, side: f64| (cx + hc * forward - hs * side, cy + hs * forward + hc * side);
    let bands = 2 + py_round(2.0 * strength) as i64;
    for i in 0..bands {
        let f = d.size * (-0.55 + 1.0 * (i as f64 + 0.5) / bands as f64);
        let lean = d.size * 0.18 * if i % 2 == 1 { 1.0 } else { -1.0 };
        let a = body_point(f - lean, -d.size * 0.55);
        let c = body_point(f, 0.0);
        let b = body_point(f + lean, d.size * 0.55);
        wrap.move_to(a.0, a.1);
        wrap.quad_to(c.0, c.1, b.0, b.1);
    }

    let a8 = |v: i64| v.clamp(0, 255) as u8;
    p.save();
    p.set_brush(Brush::None);
    p.set_pen(round_pen(Color::rgba(20, 20, 26, a8((alpha as f64 * 0.35) as i64)), width + 1.6));
    p.draw_path(&threads);
    p.draw_path(&wrap);
    p.set_pen(round_pen(Color::rgba(240, 242, 246, a8(alpha)), width));
    p.draw_path(&threads);
    p.set_pen(round_pen(Color::rgba(250, 250, 252, a8((alpha + 20).min(255))), width * 1.6));
    p.draw_path(&wrap);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(255, 255, 255, a8((alpha + 15).min(255))));
    let radius = (width * 0.9).max(0.8);
    for (x, y) in dew {
        p.draw_ellipse_at(x, y, radius, radius);
    }
    p.restore();
}
