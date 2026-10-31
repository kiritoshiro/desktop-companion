//! Port of the drawing half of `world/mouse_webs.py`: `_StickyHold.draw`,
//! `TrapCapture.draw`, `ShoveCapture.draw`, `_Projectile.draw`,
//! `_SpentSplat.draw`, their bounding boxes and `MouseWebWorld.render`.

use super::{Clip, round_cap_pen};
use dc_core::Color;
use dc_core::math::clamp;
use dc_render::{Brush, Canvas, Pen};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

type Point = (f64, f64);

pub const TRAP_RETRACT: f64 = 0.55;
pub const SPENT_FADE: f64 = 0.45;
pub const SILK: (u8, u8, u8) = (224, 233, 245);
pub const SILK_BRIGHT: (u8, u8, u8) = (240, 247, 255);
pub const SILK_STICKY: (u8, u8, u8) = (238, 245, 255);

fn silk(c: (u8, u8, u8), a: i64) -> Color {
    Color::rgba(c.0, c.1, c.2, a.clamp(0, 255) as u8)
}

fn unit(dx: f64, dy: f64) -> Point {
    let d = dx.hypot(dy);
    if d <= 1e-9 { (1.0, 0.0) } else { (dx / d, dy / d) }
}

/// `_StickyHold`: the drawn state of a hold.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct StickyHoldDraw {
    pub anchor: Point,
    pub leash: f64,
    pub escape: f64,
    pub struggle: f64,
    pub strain: Point,
    pub wobble: f64,
    /// `((dx, dy), base_alpha)` per peg.
    pub pegs: Vec<(Point, f64)>,
}

impl StickyHoldDraw {
    /// `progress()`.
    pub fn progress(&self) -> f64 {
        clamp(self.struggle / self.escape.max(1.0), 0.0, 1.0)
    }

    pub fn bbox(&self) -> Clip {
        let (ax, ay) = self.anchor;
        let reach = self.leash * 4.6 + 14.0;
        (ax - reach, ay - reach, ax + reach, ay + reach)
    }
}

/// `_StickyHold.draw(painter, alpha_mult)`.
pub fn draw_sticky_hold(p: &mut Canvas, h: &StickyHoldDraw, alpha_mult: f64) {
    let prog = h.progress();
    let (ax, ay) = h.anchor;
    let mut cx = ax + (h.strain.0 - ax) * TRAP_RETRACT;
    let mut cy = ay + (h.strain.1 - ay) * TRAP_RETRACT;
    let wob_n = unit(h.strain.0 - ax, h.strain.1 - ay);
    cx += -wob_n.1 * h.wobble;
    cy += wob_n.0 * h.wobble;
    for (i, &(off, base)) in h.pegs.iter().enumerate() {
        let px = ax + off.0;
        let py = ay + off.1;
        let snapped = prog > 0.62 && i % 3 == 0;
        let a = (170.0 * base * alpha_mult * if snapped { 0.25 } else { 1.0 } * (1.0 - prog * 0.35)) as i64;
        if a <= 4 {
            continue;
        }
        p.set_pen(round_cap_pen(silk(SILK, a), 1.0));
        p.set_brush(Brush::None);
        let mxp = (px + cx) * 0.5 + (cx - ax) * 0.12;
        let myp = (py + cy) * 0.5 + (cy - ay) * 0.12;
        p.draw_polyline(&[(px, py), (mxp, myp), (cx, cy)]);
    }
    let ring_a = (150.0 * alpha_mult * (1.0 - prog * 0.5)) as i64;
    if ring_a > 6 {
        p.set_pen(round_cap_pen(silk(SILK_STICKY, ring_a), 1.0));
        p.set_brush(Brush::None);
        for ring in [0.45, 0.8] {
            let r = h.leash * (1.2 + ring * 1.6);
            let steps = 14;
            let pts: Vec<Point> = (0..=steps)
                .map(|s| {
                    let aa = (s as f64 / steps as f64) * TAU;
                    (cx + aa.cos() * r, cy + aa.sin() * r * 0.92)
                })
                .collect();
            p.draw_polyline(&pts);
        }
    }
    p.set_pen(Pen::NONE);
    p.set_brush(silk(SILK_BRIGHT, (210.0 * alpha_mult) as i64));
    p.draw_ellipse_at(cx, cy, 3.1, 3.1);
    p.set_brush(silk(SILK_BRIGHT, (90.0 * alpha_mult) as i64));
    p.draw_ellipse_at(cx, cy, 5.6, 5.6);
}

/// The live capture: `TrapCapture` or `ShoveCapture`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CaptureDraw {
    /// `TrapCapture` (`kind = "trap"`).
    Trap { hold: StickyHoldDraw },
    /// `ShoveCapture` (`kind = "wall"`).
    Wall {
        /// `"shove"` or `"pin"`.
        phase: String,
        trail: Vec<Point>,
        cur: Point,
        wall: Point,
        hold: Option<StickyHoldDraw>,
    },
}

impl CaptureDraw {
    pub fn bbox(&self) -> Clip {
        match self {
            CaptureDraw::Trap { hold } => hold.bbox(),
            CaptureDraw::Wall { trail, wall, hold, .. } => {
                if let Some(h) = hold {
                    return h.bbox();
                }
                let pts = trail.iter().chain(std::iter::once(wall));
                let b = pts.fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| {
                    (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
                });
                let pad = 18.0;
                (b.0 - pad, b.1 - pad, b.2 + pad, b.3 + pad)
            }
        }
    }
}

/// `TrapCapture.draw` / `ShoveCapture.draw`.
pub fn draw_capture(p: &mut Canvas, cap: &CaptureDraw) {
    match cap {
        CaptureDraw::Trap { hold } => draw_sticky_hold(p, hold, 1.0),
        CaptureDraw::Wall { phase, trail, cur, hold, .. } => {
            if phase == "shove" && trail.len() >= 2 {
                let n = trail.len();
                for i in 1..n {
                    let f = i as f64 / n as f64;
                    p.set_pen(round_cap_pen(silk(SILK, (150.0 * f) as i64), 1.0 + 1.6 * f));
                    p.draw_line(trail[i - 1].0, trail[i - 1].1, trail[i].0, trail[i].1);
                }
                p.set_pen(Pen::NONE);
                p.set_brush(silk(SILK_BRIGHT, 210));
                p.draw_ellipse_at(cur.0, cur.1, 3.0, 3.0);
            }
            if let Some(h) = hold {
                draw_sticky_hold(p, h, 1.0);
            }
        }
    }
}

/// `_Projectile`: the cursor glob in flight.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct MouseShotDraw {
    pub trail: Vec<Point>,
    pub pos: Point,
}

impl MouseShotDraw {
    pub fn bbox(&self) -> Clip {
        let b = self.trail.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| {
            (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
        });
        let pad = 10.0;
        (b.0 - pad, b.1 - pad, b.2 + pad, b.3 + pad)
    }
}

/// `_Projectile.draw(painter)`.
pub fn draw_mouse_shot(p: &mut Canvas, shot: &MouseShotDraw) {
    let n = shot.trail.len();
    for i in 1..n {
        let f = i as f64 / n as f64;
        p.set_pen(round_cap_pen(silk(SILK, (120.0 * f) as i64), 0.8 + 1.4 * f));
        let (a, b) = (shot.trail[i - 1], shot.trail[i]);
        p.draw_line(a.0, a.1, b.0, b.1);
    }
    p.set_pen(Pen::NONE);
    p.set_brush(silk(SILK_BRIGHT, 220));
    p.draw_ellipse_at(shot.pos.0, shot.pos.1, 2.8, 2.8);
    p.set_brush(silk(SILK_BRIGHT, 80));
    p.draw_ellipse_at(shot.pos.0, shot.pos.1, 5.0, 5.0);
}

/// `_SpentSplat`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct SpentSplatDraw {
    pub at: Point,
    pub t: f64,
    pub spokes: Vec<Point>,
}

impl SpentSplatDraw {
    pub fn bbox(&self) -> Clip {
        (self.at.0 - 26.0, self.at.1 - 26.0, self.at.0 + 26.0, self.at.1 + 26.0)
    }
}

/// `_SpentSplat.draw(painter)`.
pub fn draw_spent(p: &mut Canvas, s: &SpentSplatDraw) {
    let fade = clamp(1.0 - s.t / SPENT_FADE, 0.0, 1.0);
    let a = (150.0 * fade) as i64;
    if a <= 4 {
        return;
    }
    p.set_pen(round_cap_pen(silk(SILK, a), 1.0));
    for off in &s.spokes {
        let k = 1.0 + (1.0 - fade) * 0.4;
        p.draw_line(s.at.0, s.at.1, s.at.0 + off.0 * k, s.at.1 + off.1 * k);
    }
}

/// `MouseWebWorld`: the spent splats, the live capture and the glob.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct MouseWebDraw {
    pub spent: Vec<SpentSplatDraw>,
    pub capture: Option<CaptureDraw>,
    pub projectile: Option<MouseShotDraw>,
}

/// `MouseWebWorld.render(painter, clip)`.
pub fn render_mouse_webs(p: &mut Canvas, world: &MouseWebDraw, clip: Option<Clip>) {
    let visible = |b: Clip| match clip {
        None => true,
        Some(c) => !(b.2 < c.0 || b.0 > c.2 || b.3 < c.1 || b.1 > c.3),
    };
    for s in &world.spent {
        if visible(s.bbox()) {
            draw_spent(p, s);
        }
    }
    if let Some(cap) = &world.capture
        && visible(cap.bbox())
    {
        draw_capture(p, cap);
    }
    if let Some(proj) = &world.projectile
        && visible(proj.bbox())
    {
        draw_mouse_shot(p, proj);
    }
}
