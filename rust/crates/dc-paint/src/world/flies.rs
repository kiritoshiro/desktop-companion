//! Port of the drawing half of `world/flies.py`: `Fly.render` (with
//! `_render_capture_silk`, `_render_body`, `_render_eaten`),
//! `WebShotProjectile.render`, `FlyRemains.render`, `FlySpawner.render`,
//! the footprints and `FlyWorld.render`.

use super::{Clip, round_cap_pen};
use dc_core::Color;
use dc_core::math::clamp;
use dc_render::{Brush, Canvas, Pen};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

type Point = (f64, f64);

pub const FLY_BASE_LENGTH: f64 = 9.0;
pub const FLY_EAT_DURATION: f64 = 0.42;
/// `_SHOT_SILK`.
pub const SHOT_SILK: (u8, u8, u8) = (236, 240, 248);
/// `FlyRemains.LIFETIME`.
pub const REMAINS_LIFETIME: f64 = 3.6;

fn a8(v: f64) -> u8 {
    (v as i64).clamp(0, 255) as u8
}

/// `Fly`: what `Fly.render` reads.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FlyDraw {
    pub x: f64,
    pub y: f64,
    pub length: f64,
    pub heading: f64,
    /// `"flying"`, `"trapped"`, `"eaten"` or `"gone"`.
    pub state: String,
    pub struggle_phase: f64,
    pub wing_phase: f64,
    pub eat_t: f64,
    /// `_draw_jitter`.
    pub draw_jitter: f64,
    /// `_web_pegs`. Python generates them on the first trapped frame from the
    /// fly's own stream (`_make_web_pegs`); the simulation must have done
    /// so before painting, since painting draws no random numbers.
    pub web_pegs: Vec<Point>,
    pub tether_from: Option<Point>,
}

impl Default for FlyDraw {
    fn default() -> Self {
        FlyDraw {
            x: 0.0,
            y: 0.0,
            length: FLY_BASE_LENGTH,
            heading: 0.0,
            state: "flying".into(),
            struggle_phase: 0.0,
            wing_phase: 0.0,
            eat_t: 0.0,
            draw_jitter: 0.0,
            web_pegs: Vec::new(),
            tether_from: None,
        }
    }
}

impl FlyDraw {
    /// `footprint()` as `(x, y, w, h)`.
    pub fn footprint(&self) -> (f64, f64, f64, f64) {
        let pad = self.length * 2.9 + 12.0;
        if let Some((tx, ty)) = self.tether_from {
            let x0 = self.x.min(tx) - pad;
            let y0 = self.y.min(ty) - pad;
            let x1 = self.x.max(tx) + pad;
            let y1 = self.y.max(ty) + pad;
            return (x0, y0, x1 - x0, y1 - y0);
        }
        (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)
    }
}

/// `Fly.render(painter)`.
pub fn render_fly(p: &mut Canvas, fly: &FlyDraw) {
    if fly.state == "gone" {
        return;
    }
    if fly.state == "eaten" {
        render_eaten(p, fly);
        return;
    }
    let trapped = fly.state == "trapped";
    let struggle = if trapped { fly.struggle_phase.sin() * 0.5 } else { 0.0 };
    p.save();
    p.translate(fly.x, fly.y);
    if trapped {
        render_capture_silk(p, fly);
    }
    let jitter = fly.draw_jitter;
    let body_angle = fly.heading + struggle * 0.4;
    p.save();
    if jitter != 0.0 {
        p.translate((fly.struggle_phase * 1.3).cos() * jitter, fly.struggle_phase.sin() * jitter);
    }
    p.rotate(body_angle.to_degrees());
    render_body(p, fly.length, fly.wing_phase);
    p.restore();
    p.restore();
}

/// `_render_capture_silk(painter)`.
fn render_capture_silk(p: &mut Canvas, fly: &FlyDraw) {
    let wob = fly.struggle_phase.sin() * fly.length * 0.05;
    let (cx, cy) = (wob * 0.4, wob);
    p.set_pen(round_cap_pen(Color::rgba(240, 242, 248, 150), 1.0));
    p.set_brush(Brush::None);
    for &(px, py) in &fly.web_pegs {
        let bx = (px + cx) * 0.5 + (cx - px) * 0.12;
        let by = (py + cy) * 0.5 + (cy - py) * 0.12;
        let mut path = dc_render::Path::new();
        path.move_to(px, py);
        path.quad_to(bx, by, cx, cy);
        p.draw_path(&path);
    }
    p.set_pen(round_cap_pen(Color::rgba(235, 240, 250, 90), 1.0));
    let r = fly.length * 1.5;
    let steps = 14;
    let pts: Vec<Point> = (0..=steps)
        .map(|s| {
            let a = (s as f64 / steps as f64) * TAU;
            (cx + a.cos() * r, cy + a.sin() * r * 0.92)
        })
        .collect();
    p.draw_polyline(&pts);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(245, 247, 252, 210));
    p.draw_ellipse_at(cx, cy, 2.6, 2.6);
    p.set_brush(Color::rgba(245, 247, 252, 80));
    p.draw_ellipse_at(cx, cy, 4.8, 4.8);
}

/// `_render_body(painter)`.
fn render_body(p: &mut Canvas, l: f64, wing_phase: f64) {
    let beat = wing_phase.sin() * 0.5 + 0.5;
    let wing_alpha = (70.0 + beat * 70.0) as i64;
    let wing_spread = 0.55 + beat * 0.5;
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(205, 215, 230, wing_alpha as u8));
    for side in [-1.0, 1.0] {
        p.save();
        p.translate(-l * 0.05, side * l * 0.18);
        p.rotate(side * (18.0 + wing_spread * 22.0));
        p.draw_ellipse_at(-l * 0.45, 0.0, l * 0.62, l * 0.30);
        p.restore();
    }
    p.set_brush(Color::rgba(34, 36, 42, 255));
    p.draw_ellipse_at(-l * 0.42, 0.0, l * 0.55, l * 0.40);
    p.set_brush(Color::rgba(60, 64, 74, 180));
    p.draw_ellipse_at(-l * 0.30, -l * 0.06, l * 0.30, l * 0.16);
    p.set_brush(Color::rgba(46, 52, 50, 255));
    p.draw_ellipse_at(l * 0.12, 0.0, l * 0.42, l * 0.34);
    p.set_brush(Color::rgba(78, 96, 88, 150));
    p.draw_ellipse_at(l * 0.18, -l * 0.05, l * 0.20, l * 0.12);
    p.set_brush(Color::rgba(28, 28, 32, 255));
    p.draw_ellipse_at(l * 0.52, 0.0, l * 0.24, l * 0.24);
    p.set_brush(Color::rgba(150, 40, 34, 235));
    for side in [-1.0, 1.0] {
        p.draw_ellipse_at(l * 0.56, side * l * 0.12, l * 0.13, l * 0.15);
    }
    p.set_pen(round_cap_pen(Color::rgba(20, 20, 24, 220), (l * 0.07).max(0.8)));
    p.set_brush(Brush::None);
    for i in 0..3 {
        let fi = i as f64;
        let ox = l * (0.18 - fi * 0.20);
        for side in [-1.0, 1.0] {
            p.draw_line(ox, side * l * 0.18, ox - l * 0.18, side * (l * 0.42 + fi * 1.0));
        }
    }
}

/// `_render_eaten(painter)`.
fn render_eaten(p: &mut Canvas, fly: &FlyDraw) {
    let t = clamp(fly.eat_t / FLY_EAT_DURATION, 0.0, 1.0);
    p.save();
    p.translate(fly.x, fly.y);
    let alpha = (200.0 * (1.0 - t)) as i64;
    if alpha <= 2 {
        p.restore();
        return;
    }
    p.set_pen(round_cap_pen(Color::rgba(40, 40, 46, alpha as u8), (fly.length * 0.10).max(0.8)));
    let spread = fly.length * (1.0 + t * 2.6);
    for i in 0..6 {
        let ang = (i as f64 / 6.0) * TAU + fly.struggle_phase * 0.2;
        let x = ang.cos() * spread;
        let y = ang.sin() * spread;
        p.draw_line(x * 0.4, y * 0.4, x, y);
    }
    p.restore();
}

/// `WebShotProjectile`: the trail and the glob's position.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WebShotDraw {
    pub trail: Vec<Point>,
    pub pos: Point,
}

impl WebShotDraw {
    pub fn footprint(&self) -> (f64, f64, f64, f64) {
        let (x0, x1) = self.trail.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (y0, y1) = self.trail.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        let pad = 12.0;
        (x0 - pad, y0 - pad, (x1 - x0) + pad * 2.0, (y1 - y0) + pad * 2.0)
    }
}

/// `WebShotProjectile.render(painter)`.
pub fn render_web_shot(p: &mut Canvas, shot: &WebShotDraw) {
    let n = shot.trail.len();
    let (r, g, b) = SHOT_SILK;
    for i in 1..n {
        let f = i as f64 / n as f64;
        p.set_pen(round_cap_pen(Color::rgba(r, g, b, a8(130.0 * f)), 0.8 + 1.5 * f));
        let (a, c) = (shot.trail[i - 1], shot.trail[i]);
        p.draw_line(a.0, a.1, c.0, c.1);
    }
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(r, g, b, 230));
    p.draw_ellipse_at(shot.pos.0, shot.pos.1, 3.0, 3.0);
    p.set_brush(Color::rgba(r, g, b, 80));
    p.draw_ellipse_at(shot.pos.0, shot.pos.1, 5.4, 5.4);
}

/// One part of `FlyRemains.parts`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct RemainsPart {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub len: f64,
    /// `"leg"`, `"wing"` or `"crumb"`.
    pub kind: String,
}

/// `FlyRemains`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FlyRemainsDraw {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub t: f64,
    pub parts: Vec<RemainsPart>,
}

impl Default for FlyRemainsDraw {
    fn default() -> Self {
        FlyRemainsDraw { x: 0.0, y: 0.0, scale: 1.0, t: 0.0, parts: Vec::new() }
    }
}

impl FlyRemainsDraw {
    pub fn footprint(&self) -> (f64, f64, f64, f64) {
        let pad = FLY_BASE_LENGTH * self.scale * 2.2 + 8.0;
        (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)
    }
}

/// `FlyRemains.render(painter)`.
pub fn render_remains(p: &mut Canvas, rem: &FlyRemainsDraw) {
    let fade = clamp(1.0 - rem.t / REMAINS_LIFETIME, 0.0, 1.0);
    if fade <= 0.02 {
        return;
    }
    let alpha = a8(190.0 * fade);
    p.save();
    p.translate(rem.x, rem.y);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(30, 30, 36, a8(50.0 * fade)));
    p.draw_ellipse_at(0.0, 0.0, FLY_BASE_LENGTH * rem.scale * 1.2, FLY_BASE_LENGTH * rem.scale * 0.7);
    for part in &rem.parts {
        p.save();
        p.translate(part.x, part.y);
        p.rotate(part.rot.to_degrees());
        match part.kind.as_str() {
            "wing" => {
                p.set_pen(Pen::NONE);
                p.set_brush(Color::rgba(205, 215, 230, a8(110.0 * fade)));
                p.draw_ellipse_at(0.0, 0.0, part.len * 0.5, part.len * 0.26);
            }
            "crumb" => {
                p.set_pen(Pen::NONE);
                p.set_brush(Color::rgba(36, 38, 44, alpha));
                p.draw_ellipse_at(0.0, 0.0, part.len * 0.3, part.len * 0.24);
            }
            _ => {
                p.set_pen(round_cap_pen(Color::rgba(22, 22, 26, alpha), (FLY_BASE_LENGTH * rem.scale * 0.08).max(0.8)));
                p.draw_line(-part.len * 0.5, 0.0, part.len * 0.5, part.len * 0.18);
            }
        }
        p.restore();
    }
    p.restore();
}

/// `FlySpawner`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FlySpawnerDraw {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub pulse: f64,
    /// `[angle, radius_frac, speed]` per speck.
    pub specks: Vec<[f64; 3]>,
}

impl Default for FlySpawnerDraw {
    fn default() -> Self {
        FlySpawnerDraw { x: 0.0, y: 0.0, radius: 22.0, pulse: 0.0, specks: Vec::new() }
    }
}

impl FlySpawnerDraw {
    pub fn footprint(&self) -> (f64, f64, f64, f64) {
        let pad = self.radius + 12.0;
        (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)
    }
}

/// `FlySpawner.render(painter)`.
pub fn render_spawner(p: &mut Canvas, sp: &FlySpawnerDraw) {
    p.save();
    p.translate(sp.x, sp.y);
    let r = sp.radius;
    let breath = 1.0 + sp.pulse.sin() * 0.04;
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(0, 0, 0, 45));
    p.draw_ellipse_at(0.0, r * 0.55, r * 1.05, r * 0.5);
    p.set_brush(Color::rgba(58, 44, 38, 235));
    p.draw_ellipse_at(0.0, 0.0, r * breath, r * 0.92 * breath);
    p.set_brush(Color::rgba(78, 60, 48, 220));
    p.draw_ellipse_at(-r * 0.22, -r * 0.16, r * 0.55, r * 0.42);
    p.set_brush(Color::rgba(96, 78, 58, 150));
    p.draw_ellipse_at(r * 0.18, r * 0.05, r * 0.4, r * 0.3);
    p.set_brush(Color::rgba(16, 12, 12, 240));
    p.draw_ellipse_at(0.0, -r * 0.02, r * 0.42, r * 0.36);
    p.set_brush(Color::rgba(20, 20, 24, 220));
    for &[ang, rr, _sp] in &sp.specks {
        let sx = ang.cos() * r * rr;
        let sy = ang.sin() * r * rr * 0.85;
        p.draw_ellipse_at(sx, sy, r * 0.07, r * 0.07);
    }
    p.restore();
}

/// `FlyWorld`: everything its `render` draws.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FlyWorldDraw {
    pub spawners: Vec<FlySpawnerDraw>,
    pub remains: Vec<FlyRemainsDraw>,
    pub flies: Vec<FlyDraw>,
    pub projectiles: Vec<WebShotDraw>,
}

/// `FlyWorld.render(painter, clip)`: nests, remains, flies, then silk globs.
pub fn render_flies(p: &mut Canvas, world: &FlyWorldDraw, clip: Option<Clip>) {
    let visible = |fp: (f64, f64, f64, f64)| match clip {
        None => true,
        Some(c) => !(fp.0 + fp.2 < c.0 || fp.0 > c.2 || fp.1 + fp.3 < c.1 || fp.1 > c.3),
    };
    for sp in &world.spawners {
        if visible(sp.footprint()) {
            render_spawner(p, sp);
        }
    }
    for rem in &world.remains {
        if visible(rem.footprint()) {
            render_remains(p, rem);
        }
    }
    for fly in &world.flies {
        if visible(fly.footprint()) {
            render_fly(p, fly);
        }
    }
    for proj in &world.projectiles {
        if visible(proj.footprint()) {
            render_web_shot(p, proj);
        }
    }
}
