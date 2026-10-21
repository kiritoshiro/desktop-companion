//! Port of `world/mouse_webs.py`: sticky silk the spiders shoot at the real
//! cursor.
//!
//! Pure geometry: [`MouseWebWorld::update`] returns the *desired*
//! overlay-local cursor position each frame (or `None` when the pointer is
//! free) and the engine applies it. A failed hold reads as an escape, and
//! every capture has a hard maximum hold time.
//!
//! Drawing is out of scope. Painter state: [`StickyHold`] (`anchor`, `strain`,
//! `wobble`, `pegs`, [`StickyHold::progress`]), [`ShoveCapture`] (`phase`,
//! `trail`, `cur`, `hold`), [`MouseShot`] (`trail`, `pos`) and
//! [`SpentSplat`] (`at`, `t`, `spokes`).

use dc_core::PyRandom;
use dc_core::math::{clamp, lerp};

use crate::pymath::{distance, hypot};
use crate::{Bbox, Point, Xywh};

const TAU: f64 = std::f64::consts::TAU;

pub const SHOT_SPEED: f64 = 1350.0;
pub const SHOT_HOMING: f64 = 7.0;
pub const SHOT_HIT_RADIUS: f64 = 26.0;
pub const SHOT_MAX_TRAVEL_MULT: f64 = 2.4;
pub const SHOT_MIN_TRAVEL: f64 = 60.0;
pub const TRAP_LEASH: f64 = 17.0;
pub const TRAP_RETRACT: f64 = 0.55;
pub const TRAP_DEADZONE: f64 = 3.0;
pub const TRAP_STRUGGLE_CAP: f64 = 27.0;
pub const TRAP_STRUGGLE_DECAY: f64 = 170.0;
pub const TRAP_ESCAPE: f64 = 460.0;
pub const TRAP_MAX_HOLD: f64 = 6.0;
pub const SHOVE_DURATION: f64 = 0.45;
pub const WALL_MARGIN: f64 = 8.0;
pub const WALL_ESCAPE: f64 = 320.0;
pub const WALL_MAX_HOLD: f64 = 4.0;
pub const NO_CONTROL_VISUAL: f64 = 0.55;
pub const SPENT_FADE: f64 = 0.45;

pub const SILK: (u8, u8, u8) = (224, 233, 245);
pub const SILK_BRIGHT: (u8, u8, u8) = (240, 247, 255);
pub const SILK_STICKY: (u8, u8, u8) = (238, 245, 255);

fn unit(dx: f64, dy: f64) -> Point {
    let m = hypot(dx, dy);
    if m <= 1e-9 {
        return (1.0, 0.0);
    }
    (dx / m, dy / m)
}

fn clamp_vec(dx: f64, dy: f64, max_len: f64) -> Point {
    let m = hypot(dx, dy);
    if m <= max_len || m <= 1e-9 {
        return (dx, dy);
    }
    let s = max_len / m;
    (dx * s, dy * s)
}

fn ease_out(t: f64) -> f64 {
    let t = clamp(t, 0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

fn xywh(b: Bbox) -> Xywh {
    (b.0, b.1, b.2 - b.0, b.3 - b.1)
}

fn trail_bbox(trail: &[Point], extra: Option<Point>, pad: f64) -> Bbox {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in trail.iter().chain(extra.iter()) {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x0 - pad, y0 - pad, x1 + pad, y1 + pad)
}

// ======================================================================
// A sticky hold at a fixed anchor
// ======================================================================

/// Pins the pointer near `anchor` until the user wiggles free.
#[derive(Clone, Debug)]
pub struct StickyHold {
    pub anchor: Point,
    pub leash: f64,
    pub escape: f64,
    pub max_hold: f64,
    pub struggle: f64,
    pub hold_t: f64,
    pub forced_prev: Option<Point>,
    pub strain: Point,
    pub wobble: f64,
    pub wob_phase: f64,
    /// `(offset from anchor, brightness)` of each radiating anchor strand.
    pub pegs: Vec<(Point, f64)>,
}

impl StickyHold {
    pub fn new(anchor: Point, leash: f64, escape: f64, max_hold: f64, pegs: usize, rng: &mut PyRandom) -> StickyHold {
        let wob_phase = rng.uniform(0.0, TAU);
        let mut list = Vec::with_capacity(pegs);
        for i in 0..pegs {
            let a = (i as f64 / pegs as f64) * TAU + rng.uniform(-0.18, 0.18);
            let r = leash * rng.uniform(2.4, 4.2);
            let w = rng.uniform(0.8, 1.0);
            list.push(((a.cos() * r, a.sin() * r), w));
        }
        StickyHold {
            anchor,
            leash,
            escape,
            max_hold,
            struggle: 0.0,
            hold_t: 0.0,
            forced_prev: None,
            strain: anchor,
            wobble: 0.0,
            wob_phase,
            pegs: list,
        }
    }

    pub fn progress(&self) -> f64 {
        clamp(self.struggle / 1.0f64.max(self.escape), 0.0, 1.0)
    }

    /// `(desired pointer, done)`.
    pub fn update(&mut self, dt: f64, mx: f64, my: f64, can_control: bool) -> (Option<Point>, bool) {
        self.hold_t += dt;
        self.wob_phase += dt * (14.0 + 22.0 * self.progress());
        if !can_control {
            self.strain = (mx, my);
            self.wobble = 2.0 * self.wob_phase.sin();
            return (None, self.hold_t >= NO_CONTROL_VISUAL);
        }
        let prev = *self.forced_prev.get_or_insert((mx, my));
        let user_move = distance(mx, my, prev.0, prev.1);
        self.struggle += clamp(user_move - TRAP_DEADZONE, 0.0, TRAP_STRUGGLE_CAP);
        self.struggle = 0.0f64.max(self.struggle - TRAP_STRUGGLE_DECAY * dt);
        let (sdx, sdy) = clamp_vec(mx - self.anchor.0, my - self.anchor.1, self.leash * 1.8);
        self.strain = (self.anchor.0 + sdx, self.anchor.1 + sdy);
        self.wobble = (1.5 + 4.0 * self.progress()) * self.wob_phase.sin() + 6.0f64.min(user_move * 0.25);
        if self.struggle >= self.escape || self.hold_t >= self.max_hold {
            return (None, true);
        }
        let (hdx, hdy) = clamp_vec(mx - self.anchor.0, my - self.anchor.1, self.leash);
        let desired = (self.anchor.0 + hdx * TRAP_RETRACT, self.anchor.1 + hdy * TRAP_RETRACT);
        self.forced_prev = Some(desired);
        (Some(desired), false)
    }

    pub fn bbox(&self) -> Bbox {
        let (ax, ay) = self.anchor;
        let reach = self.leash * 4.6 + 14.0;
        (ax - reach, ay - reach, ax + reach, ay + reach)
    }
}

// ======================================================================
// Captures
// ======================================================================

/// Slam the pointer to the nearest wall along the firing line, then pin it.
#[derive(Clone, Debug)]
pub struct ShoveCapture {
    pub start: Point,
    pub screen_w: f64,
    pub screen_h: f64,
    pub wall: Point,
    /// `"shove"` then `"pin"`; true while shoving.
    pub shoving: bool,
    pub t: f64,
    pub cur: Point,
    /// Last up to 10 shove positions.
    pub trail: Vec<Point>,
    pub hold: Option<StickyHold>,
}

fn closer(best: Option<f64>, t: f64) -> Option<f64> {
    if t <= 1.0 {
        return best;
    }
    Some(match best {
        None => t,
        Some(b) => b.min(t),
    })
}

/// `_nearest_wall`.
pub fn nearest_wall(p: Point, sw: f64, sh: f64) -> Point {
    let (x, y) = p;
    let d = [x, sw - x, y, sh - y];
    let mut side = 0;
    for i in 1..4 {
        if d[i] < d[side] {
            side = i;
        }
    }
    match side {
        0 => (WALL_MARGIN, clamp(y, WALL_MARGIN, sh - WALL_MARGIN)),
        1 => (sw - WALL_MARGIN, clamp(y, WALL_MARGIN, sh - WALL_MARGIN)),
        2 => (clamp(x, WALL_MARGIN, sw - WALL_MARGIN), WALL_MARGIN),
        _ => (clamp(x, WALL_MARGIN, sw - WALL_MARGIN), sh - WALL_MARGIN),
    }
}

/// `_snap_to_edge`.
pub fn snap_to_edge(p: Point, sw: f64, sh: f64) -> Point {
    let (x, y) = p;
    let d = [x - WALL_MARGIN, sw - WALL_MARGIN - x, y - WALL_MARGIN, sh - WALL_MARGIN - y];
    let mut side = 0;
    for i in 1..4 {
        if d[i] < d[side] {
            side = i;
        }
    }
    match side {
        0 => (WALL_MARGIN, y),
        1 => (sw - WALL_MARGIN, y),
        2 => (x, WALL_MARGIN),
        _ => (x, sh - WALL_MARGIN),
    }
}

impl ShoveCapture {
    pub fn new(hit: Point, fire_dir: Point, screen_w: f64, screen_h: f64) -> ShoveCapture {
        ShoveCapture {
            start: hit,
            screen_w,
            screen_h,
            wall: Self::wall_point(hit, fire_dir, screen_w, screen_h),
            shoving: true,
            t: 0.0,
            cur: hit,
            trail: vec![hit],
            hold: None,
        }
    }

    /// `_wall_point`.
    pub fn wall_point(start: Point, fire_dir: Point, sw: f64, sh: f64) -> Point {
        let (dx, dy) = unit(fire_dir.0, fire_dir.1);
        let (sx, sy) = start;
        let mut best = None;
        if dx > 1e-6 {
            best = closer(best, (sw - WALL_MARGIN - sx) / dx);
        } else if dx < -1e-6 {
            best = closer(best, (WALL_MARGIN - sx) / dx);
        }
        if dy > 1e-6 {
            best = closer(best, (sh - WALL_MARGIN - sy) / dy);
        } else if dy < -1e-6 {
            best = closer(best, (WALL_MARGIN - sy) / dy);
        }
        match best {
            Some(b) if b > 1.0 => {
                let wx = clamp(sx + dx * b, WALL_MARGIN, sw - WALL_MARGIN);
                let wy = clamp(sy + dy * b, WALL_MARGIN, sh - WALL_MARGIN);
                snap_to_edge((wx, wy), sw, sh)
            }
            _ => nearest_wall(start, sw, sh),
        }
    }

    pub fn update(
        &mut self,
        dt: f64,
        mx: f64,
        my: f64,
        can_control: bool,
        rng: &mut PyRandom,
    ) -> (Option<Point>, bool) {
        if self.shoving {
            self.t += dt;
            let f = ease_out(self.t / SHOVE_DURATION);
            self.cur = (lerp(self.start.0, self.wall.0, f), lerp(self.start.1, self.wall.1, f));
            self.trail.push(self.cur);
            if self.trail.len() > 10 {
                let extra = self.trail.len() - 10;
                self.trail.drain(..extra);
            }
            if !can_control {
                return (None, self.t >= SHOVE_DURATION.min(NO_CONTROL_VISUAL));
            }
            if self.t >= SHOVE_DURATION {
                self.shoving = false;
                self.hold = Some(StickyHold::new(self.wall, TRAP_LEASH * 0.9, WALL_ESCAPE, WALL_MAX_HOLD, 9, rng));
            }
            return (Some(if self.t >= SHOVE_DURATION { self.wall } else { self.cur }), false);
        }
        self.hold.as_mut().expect("pinned shove has a hold").update(dt, mx, my, can_control)
    }

    pub fn bbox(&self) -> Bbox {
        if let Some(h) = &self.hold {
            return h.bbox();
        }
        trail_bbox(&self.trail, Some(self.wall), 18.0)
    }

    pub fn release_point(&self) -> Point {
        match &self.hold {
            Some(h) => h.strain,
            None => self.cur,
        }
    }
}

/// A live capture of the pointer: `TrapCapture` or `ShoveCapture`.
#[derive(Clone, Debug)]
pub enum Capture {
    /// Pin the pointer where the glob hit it; wiggle to escape.
    Trap(StickyHold),
    /// Slam it to the wall, then pin it there.
    Wall(ShoveCapture),
}

impl Capture {
    pub fn kind(&self) -> &'static str {
        match self {
            Capture::Trap(_) => "trap",
            Capture::Wall(_) => "wall",
        }
    }

    pub fn update(
        &mut self,
        dt: f64,
        mx: f64,
        my: f64,
        can_control: bool,
        rng: &mut PyRandom,
    ) -> (Option<Point>, bool) {
        match self {
            Capture::Trap(h) => h.update(dt, mx, my, can_control),
            Capture::Wall(s) => s.update(dt, mx, my, can_control, rng),
        }
    }

    pub fn bbox(&self) -> Bbox {
        match self {
            Capture::Trap(h) => h.bbox(),
            Capture::Wall(s) => s.bbox(),
        }
    }

    pub fn release_point(&self) -> Point {
        match self {
            Capture::Trap(h) => h.strain,
            Capture::Wall(s) => s.release_point(),
        }
    }

    /// The hold, once there is one.
    pub fn hold(&self) -> Option<&StickyHold> {
        match self {
            Capture::Trap(h) => Some(h),
            Capture::Wall(s) => s.hold.as_ref(),
        }
    }
}

// ======================================================================
// Projectile
// ======================================================================

/// `_Projectile.update` result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotOutcome {
    Fly,
    Hit,
    Miss,
}

/// The flying glob of silk (`_Projectile`), thrown rather than guided.
#[derive(Clone, Debug)]
pub struct MouseShot {
    /// `"trap"` or `"wall"`.
    pub kind: &'static str,
    pub pos: Point,
    pub origin: Point,
    pub homing: f64,
    pub max_travel: f64,
    pub travelled: f64,
    pub vel: Point,
    /// Last up to 8 positions.
    pub trail: Vec<Point>,
}

impl MouseShot {
    pub fn new(origin: Point, target: Point, kind: &'static str, homing: f64, lead: Point) -> MouseShot {
        let mut travel = distance(origin.0, origin.1, target.0, target.1) / SHOT_SPEED;
        let mut aim = target;
        for _ in 0..4 {
            aim = (target.0 + lead.0 * travel, target.1 + lead.1 * travel);
            travel = distance(origin.0, origin.1, aim.0, aim.1) / SHOT_SPEED;
        }
        let d = distance(origin.0, origin.1, aim.0, aim.1);
        let (ux, uy) = unit(aim.0 - origin.0, aim.1 - origin.1);
        MouseShot {
            kind,
            pos: origin,
            origin,
            homing: homing.max(0.0),
            max_travel: SHOT_MIN_TRAVEL.max(d * SHOT_MAX_TRAVEL_MULT),
            travelled: 0.0,
            vel: (ux * SHOT_SPEED, uy * SHOT_SPEED),
            trail: vec![origin],
        }
    }

    pub fn update(&mut self, dt: f64, mx: f64, my: f64) -> ShotOutcome {
        let (mut vx, mut vy) = self.vel;
        if self.homing > 0.0 {
            let desired = unit(mx - self.pos.0, my - self.pos.1);
            vx = self.vel.0 + desired.0 * SHOT_SPEED * SHOT_HOMING * self.homing * dt;
            vy = self.vel.1 + desired.1 * SHOT_SPEED * SHOT_HOMING * self.homing * dt;
            let sp = hypot(vx, vy);
            if sp > 1e-6 {
                vx = vx / sp * SHOT_SPEED;
                vy = vy / sp * SHOT_SPEED;
            }
        }
        self.vel = (vx, vy);
        let step = (vx * dt, vy * dt);
        self.pos = (self.pos.0 + step.0, self.pos.1 + step.1);
        self.travelled += hypot(step.0, step.1);
        self.trail.push(self.pos);
        if self.trail.len() > 8 {
            let extra = self.trail.len() - 8;
            self.trail.drain(..extra);
        }
        if distance(self.pos.0, self.pos.1, mx, my) <= SHOT_HIT_RADIUS {
            return ShotOutcome::Hit;
        }
        if self.travelled >= self.max_travel {
            return ShotOutcome::Miss;
        }
        ShotOutcome::Fly
    }

    pub fn bbox(&self) -> Bbox {
        trail_bbox(&self.trail, None, 10.0)
    }
}

/// A short-lived torn-silk splat left where a capture released.
#[derive(Clone, Debug)]
pub struct SpentSplat {
    pub at: Point,
    pub t: f64,
    pub spokes: Vec<Point>,
}

impl SpentSplat {
    pub fn new(at: Point, rng: &mut PyRandom) -> SpentSplat {
        let spokes = (0..7)
            .map(|_| {
                let a = rng.uniform(0.0, TAU);
                let r = rng.uniform(8.0, 22.0);
                (a.cos() * r, a.sin() * r)
            })
            .collect();
        SpentSplat { at, t: 0.0, spokes }
    }

    pub fn update(&mut self, dt: f64) -> bool {
        self.t += dt;
        self.t >= SPENT_FADE
    }

    pub fn bbox(&self) -> Bbox {
        (self.at.0 - 26.0, self.at.1 - 26.0, self.at.0 + 26.0, self.at.1 + 26.0)
    }
}

// ======================================================================
// MouseWebWorld
// ======================================================================

/// The single active cursor-silk effect plus fading spent splats.
#[derive(Debug)]
pub struct MouseWebWorld {
    pub rng: PyRandom,
    pub screen_w: f64,
    pub screen_h: f64,
    pub enabled: bool,
    pub can_control: bool,
    pub projectile: Option<MouseShot>,
    pub capture: Option<Capture>,
    pub spent: Vec<SpentSplat>,
    dirty: Vec<Xywh>,
    prev_active_bbox: Option<Bbox>,
}

impl MouseWebWorld {
    pub fn new(screen_w: f64, screen_h: f64, can_control: bool, rng: PyRandom) -> MouseWebWorld {
        MouseWebWorld {
            rng,
            screen_w,
            screen_h,
            enabled: true,
            can_control,
            projectile: None,
            capture: None,
            spent: Vec::new(),
            dirty: Vec::new(),
            prev_active_bbox: None,
        }
    }

    pub fn set_screen(&mut self, screen_w: f64, screen_h: f64) {
        self.screen_w = screen_w;
        self.screen_h = screen_h;
        self.clear();
    }

    pub fn clear(&mut self) {
        if let Some(b) = self.prev_active_bbox {
            self.dirty.push(xywh(b));
        }
        if let Some(c) = &self.capture {
            self.dirty.push(xywh(c.bbox()));
        }
        if let Some(p) = &self.projectile {
            self.dirty.push(xywh(p.bbox()));
        }
        self.projectile = None;
        self.capture = None;
        self.prev_active_bbox = None;
    }

    pub fn busy(&self) -> bool {
        self.projectile.is_some() || self.capture.is_some()
    }

    pub fn active_kind(&self) -> Option<&'static str> {
        if let Some(p) = &self.projectile {
            return Some(p.kind);
        }
        self.capture.as_ref().map(Capture::kind)
    }

    /// Launch a glob from `origin` toward `target`. One at a time.
    pub fn shoot(&mut self, origin: Point, target: Point, kind: &str, homing: f64, lead: Point) -> bool {
        if !self.enabled || self.busy() {
            return false;
        }
        let kind = if kind.to_lowercase().starts_with('w') { "wall" } else { "trap" };
        self.projectile = Some(MouseShot::new(origin, target, kind, homing, lead));
        true
    }

    /// Advance; returns where the engine should put the pointer, if anywhere.
    pub fn update(&mut self, dt: f64, mx: f64, my: f64) -> Option<Point> {
        let mut desired = None;
        let mark = self.dirty.len();
        let dirty = &mut self.dirty;
        self.spent.retain_mut(|s| {
            dirty.push(xywh(s.bbox()));
            !s.update(dt)
        });

        if !self.enabled {
            if self.busy() {
                // `clear` appends to the world's list straight away, ahead of
                // this frame's own entries, as in Python.
                let frame: Vec<Xywh> = self.dirty.drain(mark..).collect();
                self.clear();
                self.dirty.extend(frame);
            }
            self.merge_active_dirty();
            return None;
        }

        if let Some(proj) = &mut self.projectile {
            match proj.update(dt, mx, my) {
                ShotOutcome::Hit => {
                    let hit = proj.pos;
                    self.capture = Some(if proj.kind == "wall" {
                        let fire = (hit.0 - proj.origin.0, hit.1 - proj.origin.1);
                        Capture::Wall(ShoveCapture::new(hit, fire, self.screen_w, self.screen_h))
                    } else {
                        Capture::Trap(StickyHold::new(
                            (mx, my),
                            TRAP_LEASH,
                            TRAP_ESCAPE,
                            TRAP_MAX_HOLD,
                            9,
                            &mut self.rng,
                        ))
                    });
                    self.projectile = None;
                }
                ShotOutcome::Miss => {
                    let at = proj.pos;
                    self.spent.push(SpentSplat::new(at, &mut self.rng));
                    self.projectile = None;
                }
                ShotOutcome::Fly => {}
            }
        }

        if let Some(cap) = &mut self.capture {
            let (want, done) = cap.update(dt, mx, my, self.can_control, &mut self.rng);
            desired = want;
            if done {
                let at = cap.release_point();
                self.spent.push(SpentSplat::new(at, &mut self.rng));
                self.capture = None;
                desired = None;
            }
        }
        self.merge_active_dirty();
        desired
    }

    fn merge_active_dirty(&mut self) {
        let cur = match (&self.projectile, &self.capture) {
            (Some(p), _) => Some(p.bbox()),
            (None, Some(c)) => Some(c.bbox()),
            _ => None,
        };
        if let Some(c) = cur {
            self.dirty.push(xywh(c));
        }
        if let Some(prev) = self.prev_active_bbox {
            self.dirty.push(xywh(prev));
        }
        self.prev_active_bbox = cur;
    }

    /// Repaint regions accumulated since the last call (drains them).
    pub fn dirty_rects(&mut self) -> Vec<Xywh> {
        std::mem::take(&mut self.dirty)
    }

    /// [`MouseWebWorld::dirty_rects`] into a reused buffer.
    pub fn drain_dirty_into(&mut self, out: &mut Vec<Xywh>) {
        out.append(&mut self.dirty);
    }

    pub fn is_animating(&self) -> bool {
        self.busy() || !self.spent.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 120.0;

    fn closest(mut p: MouseShot, pointer: impl Fn(f64) -> Point) -> (ShotOutcome, f64) {
        let mut nearest = f64::INFINITY;
        for step in 0..900 {
            let (mx, my) = pointer(step as f64 * DT);
            let r = p.update(DT, mx, my);
            nearest = nearest.min(distance(p.pos.0, p.pos.1, mx, my));
            if r != ShotOutcome::Fly {
                return (r, nearest);
            }
        }
        (ShotOutcome::Fly, nearest)
    }

    #[test]
    fn a_straight_shot_misses_a_dodge_and_tracking_follows() {
        let dodging = |t: f64| (900.0, 400.0 + if t < 0.03 { 0.0 } else { 500.0 * (t - 0.03) });
        let (out, straight) = closest(MouseShot::new((100.0, 400.0), (900.0, 400.0), "trap", 0.0, (0.0, 0.0)), dodging);
        assert_eq!(out, ShotOutcome::Miss);
        let (_o, tracking) = closest(MouseShot::new((100.0, 400.0), (900.0, 400.0), "trap", 1.0, (0.0, 0.0)), dodging);
        assert!(tracking < straight * 0.7);
    }

    #[test]
    fn still_targets_are_hit_and_lead_is_applied() {
        for homing in [0.0, 1.0] {
            let (o, _) =
                closest(MouseShot::new((100.0, 400.0), (900.0, 400.0), "trap", homing, (0.0, 0.0)), |_| (900.0, 400.0));
            assert_eq!(o, ShotOutcome::Hit);
        }
        let s = MouseShot::new((100.0, 400.0), (900.0, 400.0), "trap", 0.0, (0.0, 0.0));
        assert!(s.vel.1.abs() < 1e-6);
        let led = MouseShot::new((100.0, 400.0), (900.0, 400.0), "trap", 0.0, (0.0, 600.0));
        assert!(led.vel.1 > 0.0);
        let (o, _) = closest(led, |t| (900.0, 400.0 + 600.0 * t));
        assert_eq!(o, ShotOutcome::Hit);
    }

    #[test]
    fn world_defaults_to_straight_and_holds_are_seeded() {
        let mut w = MouseWebWorld::new(1920.0, 1080.0, false, PyRandom::from_u64(1));
        assert!(w.shoot((100.0, 400.0), (900.0, 400.0), "trap", 0.0, (0.0, 0.0)));
        assert_eq!(w.projectile.as_ref().unwrap().homing, 0.0);
        w.clear();
        assert!(w.shoot((100.0, 400.0), (900.0, 400.0), "trap", 1.0, (0.0, 0.0)));
        assert_eq!(w.projectile.as_ref().unwrap().homing, 1.0);

        let run = |seed| {
            let mut w = MouseWebWorld::new(1200.0, 800.0, true, PyRandom::from_u64(seed));
            w.shoot((100.0, 100.0), (600.0, 400.0), "trap", 0.0, (0.0, 0.0));
            for i in 0..720 {
                w.update(1.0 / 60.0, 600.0 + (i % 17) as f64, 400.0 + (i % 13) as f64);
                if let Some(h) = w.capture.as_ref().and_then(Capture::hold) {
                    return (h.wob_phase, h.pegs.clone());
                }
            }
            panic!("never caught");
        };
        assert_eq!(run(4), run(4));
        assert_ne!(run(4), run(7));
    }
}
