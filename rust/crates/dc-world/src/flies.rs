//! Port of `world/flies.py`: tiny autonomous prey the spiders hunt.
//!
//! A [`Fly`] buzzes around, flees nearby spiders, and can blunder into a
//! finished web and get stuck. [`FlyWorld`] owns the flock, the nests
//! ([`FlySpawner`]), the remains of eaten flies ([`FlyRemains`]) and every
//! glob of silk in flight ([`Projectile`]: a [`WebShotProjectile`] thrown at a
//! fly or a foe spider, or an [`AimedSilk`] from the player controller).
//!
//! Python object references became ids: a fly's `stuck_web` is a [`WebId`],
//! its `hunters` and `_trapper` are [`CreatureId`]s, a shot's target is a
//! [`ShotTarget`]. Every fly draws from the world's one stream, passed in.
//!
//! The fear scan (each fly against every spider) uses a [`SpatialGrid`]
//! once there are enough spiders; candidates are visited in list order, so
//! the flee vector is summed exactly as in Python.
//!
//! Drawing is out of scope. Painter state: `Fly::{x, y, heading, length,
//! state, wing_phase, buzz_phase, struggle_phase, draw_jitter, web_pegs,
//! tether_from, web_splat, eat_t}`, `FlySpawner::{x, y, radius, pulse,
//! specks}`, `FlyRemains::{x, y, scale, t, parts}`, projectile `trail`/`pos`.
//! Python's renderer lazily rolls a trapped fly's splat pegs on its first
//! drawn frame (an rng draw inside `render`); call
//! [`FlyWorld::ensure_capture_pegs`] before painting to keep that draw.

use dc_core::PyRandom;
use dc_core::math::{angle_lerp, clamp, normalize_angle};
use serde_json::{Value, json};

use crate::aimed_silk::AimedSilk;
use crate::creature_view::{CreatureId, FlyId, WebId, WorldCreature, find_index};
use crate::playfield::Playfield;
use crate::pymath::{self, distance, hypot};
use crate::spatial::{GRID_MIN_ITEMS, SpatialGrid};
use crate::webs::{Web, WebWorld};
use crate::{Point, Xywh};

const PI: f64 = std::f64::consts::PI;
const TAU: f64 = std::f64::consts::TAU;

pub const FLY_BASE_LENGTH: f64 = 9.0;
pub const FLY_MIN_LENGTH: f64 = 5.5;
pub const FLY_MAX_LENGTH: f64 = 13.0;
pub const FLY_CRUISE_SPEED: (f64, f64) = (38.0, 72.0);
pub const FLY_PANIC_SPEED: (f64, f64) = (210.0, 300.0);
pub const FLY_EDGE_MARGIN: f64 = 16.0;
pub const FLY_EAT_DURATION: f64 = 0.42;

/// Silk colour shared with the cursor web-shot.
pub const SHOT_SILK: (u8, u8, u8) = (236, 240, 248);
pub const WEB_SHOT_SPEED: f64 = 1500.0;
pub const WEB_SHOT_HIT_RADIUS: f64 = 16.0;

/// `_seg_project`: closest point on segment AB to `(x, y)` and the distance.
pub fn seg_project(ax: f64, ay: f64, bx: f64, by: f64, x: f64, y: f64) -> (Point, f64) {
    let (dx, dy) = (bx - ax, by - ay);
    let d2 = dx * dx + dy * dy;
    if d2 <= 1e-9 {
        return ((ax, ay), distance(ax, ay, x, y));
    }
    let t = clamp(((x - ax) * dx + (y - ay) * dy) / d2, 0.0, 1.0);
    let (px, py) = (ax + dx * t, ay + dy * t);
    ((px, py), distance(px, py, x, y))
}

#[inline]
fn split_screens(playfield: Option<&Playfield>) -> Option<&Playfield> {
    playfield.filter(|p| !p.simple())
}

/// `Fly.state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyState {
    Flying,
    Trapped,
    Eaten,
    Gone,
}

impl FlyState {
    pub fn as_str(self) -> &'static str {
        match self {
            FlyState::Flying => "flying",
            FlyState::Trapped => "trapped",
            FlyState::Eaten => "eaten",
            FlyState::Gone => "gone",
        }
    }
}

/// `Fly.motion_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionMode {
    Walk,
    Pause,
    Turn,
}

impl MotionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MotionMode::Walk => "walk",
            MotionMode::Pause => "pause",
            MotionMode::Turn => "turn",
        }
    }
}

/// Spiders as a fly sees them for one frame: the slice, an optional grid
/// over their positions, and the largest panic radius any of them has.
pub struct FearField<'a, C> {
    pub spiders: &'a [C],
    pub grid: Option<&'a SpatialGrid>,
    pub reach: f64,
}

/// A single buzzing fly.
#[derive(Clone, Debug)]
pub struct Fly {
    pub id: FlyId,
    pub screen_w: f64,
    pub screen_h: f64,
    pub scale: f64,
    pub x: f64,
    pub y: f64,
    pub length: f64,
    /// "Radius-ish" size other code reads (equals `length`).
    pub size: f64,
    pub heading: f64,
    pub target_heading: f64,
    pub cruise: f64,
    pub panic: f64,
    pub speed: f64,
    pub vx: f64,
    pub vy: f64,
    pub turn_rate: f64,
    pub motion_mode: MotionMode,
    pub motion_timer: f64,
    pub wander_timer: f64,
    pub hover_timer: f64,
    pub panic_level: f64,
    pub wing_phase: f64,
    pub wing_rate: f64,
    pub buzz_phase: f64,
    pub buzz_rate: f64,
    pub state: FlyState,
    pub stuck_web: Option<WebId>,
    pub captured_by_web: bool,
    pub tether_from: Option<Point>,
    pub web_splat: f64,
    pub trapper: Option<CreatureId>,
    pub struggle_phase: f64,
    pub pluck_timer: f64,
    pub trapped_for: f64,
    pub free_progress: f64,
    pub web_check_timer: f64,
    pub draw_jitter: f64,
    /// Splat anchor offsets, relative to the fly.
    pub web_pegs: Vec<Point>,
    pub dragging: bool,
    drag_dx: f64,
    drag_dy: f64,
    drag_hist: Vec<Point>,
    pub eat_t: f64,
    /// Spiders locked onto this fly (a set; managed by the manager).
    pub hunters: Vec<CreatureId>,
    bbox_prev: Option<Xywh>,
}

impl Fly {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: FlyId,
        x: f64,
        y: f64,
        screen_w: f64,
        screen_h: f64,
        heading: Option<f64>,
        scale: f64,
        rng: &mut PyRandom,
    ) -> Fly {
        let scale = clamp(scale, 0.5, 2.2);
        let jitter = rng.uniform(0.82, 1.22);
        let length = clamp(FLY_BASE_LENGTH * jitter * scale, FLY_MIN_LENGTH, FLY_MAX_LENGTH);
        let heading = heading.unwrap_or_else(|| rng.uniform(-PI, PI));
        let cruise = rng.uniform(FLY_CRUISE_SPEED.0, FLY_CRUISE_SPEED.1);
        let panic = rng.uniform(FLY_PANIC_SPEED.0, FLY_PANIC_SPEED.1);
        let speed = cruise;
        let turn_rate = rng.uniform(7.0, 12.0);
        let motion_mode = match rng.choice_index(4) {
            0 => MotionMode::Walk,
            3 => MotionMode::Turn,
            _ => MotionMode::Pause,
        };
        let motion_timer = match motion_mode {
            MotionMode::Walk => rng.uniform(0.22, 0.85),
            MotionMode::Turn => rng.uniform(0.08, 0.28),
            MotionMode::Pause => rng.uniform(0.18, 1.15),
        };
        let (speed, vx, vy) = if motion_mode != MotionMode::Walk {
            (0.0, 0.0, 0.0)
        } else {
            (speed, heading.cos() * speed, heading.sin() * speed)
        };
        let wander_timer = rng.uniform(0.12, 0.45);
        let wing_phase = rng.random() * TAU;
        let wing_rate = rng.uniform(46.0, 64.0);
        let buzz_phase = rng.random() * TAU;
        let buzz_rate = rng.uniform(18.0, 26.0);
        let struggle_phase = rng.random() * TAU;
        let pluck_timer = rng.uniform(0.12, 0.28);
        let web_check_timer = rng.uniform(0.0, 0.08);
        Fly {
            id,
            screen_w,
            screen_h,
            scale,
            x,
            y,
            length,
            size: length,
            heading,
            target_heading: heading,
            cruise,
            panic,
            speed,
            vx,
            vy,
            turn_rate,
            motion_mode,
            motion_timer,
            wander_timer,
            hover_timer: 0.0,
            panic_level: 0.0,
            wing_phase,
            wing_rate,
            buzz_phase,
            buzz_rate,
            state: FlyState::Flying,
            stuck_web: None,
            captured_by_web: false,
            tether_from: None,
            web_splat: 0.0,
            trapper: None,
            struggle_phase,
            pluck_timer,
            trapped_for: 0.0,
            free_progress: 0.0,
            web_check_timer,
            draw_jitter: 0.0,
            web_pegs: Vec::new(),
            dragging: false,
            drag_dx: 0.0,
            drag_dy: 0.0,
            drag_hist: Vec::new(),
            eat_t: 0.0,
            hunters: Vec::new(),
            bbox_prev: None,
        }
    }

    // -- status -----------------------------------------------------------
    /// A fly a spider can still chase and eat.
    pub fn alive(&self) -> bool {
        matches!(self.state, FlyState::Flying | FlyState::Trapped)
    }
    pub fn trapped(&self) -> bool {
        self.state == FlyState::Trapped
    }
    pub fn eaten(&self) -> bool {
        self.state == FlyState::Eaten
    }
    pub fn removable(&self) -> bool {
        self.state == FlyState::Gone
    }

    /// Whether a stalking hunter should advance this frame.
    pub fn is_moving(&self) -> bool {
        if self.state != FlyState::Flying || self.dragging {
            return false;
        }
        if self.panic_level > 0.08 {
            return true;
        }
        self.motion_mode == MotionMode::Walk && hypot(self.vx, self.vy) > 5.0
    }

    pub fn add_hunter(&mut self, id: CreatureId) {
        if !self.hunters.contains(&id) {
            self.hunters.push(id);
        }
    }

    pub fn remove_hunter(&mut self, id: CreatureId) {
        self.hunters.retain(|h| *h != id);
    }

    // -- capture ----------------------------------------------------------
    pub fn stick_to_web(&mut self, web: &mut Web, point: Point, rng: &mut PyRandom) {
        self.state = FlyState::Trapped;
        self.stuck_web = Some(web.id);
        self.captured_by_web = true;
        self.tether_from = None;
        self.web_splat = 0.0;
        self.x = point.0;
        self.y = point.1;
        self.vx = 0.0;
        self.vy = 0.0;
        self.speed = 0.0;
        self.trapped_for = 0.0;
        self.free_progress = 0.0;
        self.pluck_timer = rng.uniform(0.05, 0.14);
        web.pluck((self.x, self.y), 0.9);
    }

    /// Pinned in place by a trapper's thrown silk.
    pub fn pin_with_tether(&mut self, origin: Point, rng: &mut PyRandom) {
        self.state = FlyState::Trapped;
        self.stuck_web = None;
        self.captured_by_web = false;
        self.tether_from = Some(origin);
        self.web_splat = 0.0;
        self.vx = 0.0;
        self.vy = 0.0;
        self.speed = 0.0;
        self.trapped_for = 0.0;
        self.free_progress = 0.0;
        self.pluck_timer = rng.uniform(0.1, 0.2);
    }

    /// `_make_web_pegs`: fixed anchor points for a static web splat.
    pub fn make_web_pegs(&mut self, rng: &mut PyRandom) {
        self.web_pegs.clear();
        let l = self.length;
        let n = rng.randint(6, 8);
        for i in 0..n {
            let a = (i as f64 / n as f64) * TAU + rng.uniform(-0.28, 0.28);
            let r = l * rng.uniform(1.4, 2.6);
            self.web_pegs.push((a.cos() * r, a.sin() * r));
        }
    }

    /// Wrapped where it flew by a glob of silk shot from a spider.
    pub fn pin_with_web_shot(&mut self, _origin: Point, _kind: &str, rng: &mut PyRandom) {
        self.state = FlyState::Trapped;
        self.stuck_web = None;
        self.captured_by_web = false;
        self.tether_from = None;
        self.web_splat = 1.0;
        self.make_web_pegs(rng);
        self.vx = 0.0;
        self.vy = 0.0;
        self.speed = 0.0;
        self.trapped_for = 0.0;
        self.free_progress = -0.6;
        self.pluck_timer = rng.uniform(0.1, 0.2);
    }

    pub fn break_free(&mut self, rng: &mut PyRandom) {
        self.state = FlyState::Flying;
        self.stuck_web = None;
        self.captured_by_web = false;
        self.tether_from = None;
        self.web_splat = 0.0;
        self.trapper = None;
        self.panic_level = 1.0;
        self.motion_mode = MotionMode::Walk;
        self.motion_timer = rng.uniform(0.35, 0.8);
        self.speed = self.panic;
        self.heading = rng.uniform(-PI, PI);
        self.target_heading = self.heading;
        self.vx = self.heading.cos() * self.speed;
        self.vy = self.heading.sin() * self.speed;
    }

    pub fn begin_eaten(&mut self) {
        if self.state == FlyState::Eaten {
            return;
        }
        self.state = FlyState::Eaten;
        self.eat_t = 0.0;
        self.dragging = false;
        self.stuck_web = None;
        self.captured_by_web = false;
        self.tether_from = None;
        self.web_splat = 0.0;
    }

    // -- dragging ---------------------------------------------------------
    pub fn start_drag(&mut self, mx: f64, my: f64) {
        self.dragging = true;
        self.state = FlyState::Flying;
        self.stuck_web = None;
        self.captured_by_web = false;
        self.tether_from = None;
        self.web_splat = 0.0;
        self.trapper = None;
        self.panic_level = 0.0;
        self.drag_dx = self.x - mx;
        self.drag_dy = self.y - my;
        self.drag_hist.clear();
        self.drag_hist.push((mx, my));
    }

    pub fn drag_to(&mut self, mx: f64, my: f64, playfield: Option<&Playfield>) {
        self.x = clamp(mx + self.drag_dx, FLY_EDGE_MARGIN, self.screen_w - FLY_EDGE_MARGIN);
        self.y = clamp(my + self.drag_dy, FLY_EDGE_MARGIN, self.screen_h - FLY_EDGE_MARGIN);
        if let Some(p) = split_screens(playfield) {
            (self.x, self.y) = p.clamp(self.x, self.y, FLY_EDGE_MARGIN);
        }
        self.drag_hist.push((mx, my));
        if self.drag_hist.len() > 5 {
            let extra = self.drag_hist.len() - 5;
            self.drag_hist.drain(..extra);
        }
    }

    pub fn release_drag(&mut self, _mx: f64, _my: f64, rng: &mut PyRandom) {
        self.dragging = false;
        self.state = FlyState::Flying;
        let (mut vx, mut vy) = (0.0, 0.0);
        if self.drag_hist.len() >= 2 {
            let (ax, ay) = self.drag_hist[0];
            let (bx, by) = *self.drag_hist.last().unwrap();
            vx = (bx - ax) * 6.0;
            vy = (by - ay) * 6.0;
        }
        let sp = hypot(vx, vy);
        if sp > 12.0 {
            self.heading = vy.atan2(vx);
            self.target_heading = self.heading;
            self.speed = clamp(sp, self.cruise, self.panic);
            self.panic_level = clamp(sp / self.panic, 0.0, 1.0);
        } else {
            self.motion_mode = MotionMode::Walk;
            self.motion_timer = rng.uniform(0.3, 0.8);
            self.speed = self.cruise;
        }
        self.vx = self.heading.cos() * self.speed;
        self.vy = self.heading.sin() * self.speed;
        self.drag_hist.clear();
    }

    // -- per-frame --------------------------------------------------------
    pub fn update<C: WorldCreature>(
        &mut self,
        dt: f64,
        fear: &FearField<'_, C>,
        scratch: &mut Vec<u32>,
        web_world: Option<&mut WebWorld>,
        rng: &mut PyRandom,
        playfield: Option<&Playfield>,
    ) {
        self.wing_phase += dt * self.wing_rate;
        self.buzz_phase += dt * self.buzz_rate;
        self.struggle_phase += dt * rng.uniform(11.0, 16.0);

        if self.dragging {
            self.panic_level = self.panic_level.max(0.4);
            self.draw_jitter = self.buzz_phase.sin() * self.length * 0.12;
            return;
        }
        match self.state {
            FlyState::Eaten => {
                self.eat_t += dt;
                if self.eat_t >= FLY_EAT_DURATION {
                    self.state = FlyState::Gone;
                }
            }
            FlyState::Trapped => self.update_trapped(dt, web_world, rng),
            _ => self.update_flying(dt, fear, scratch, web_world, rng, playfield),
        }
    }

    fn update_trapped(&mut self, dt: f64, web_world: Option<&mut WebWorld>, rng: &mut PyRandom) {
        self.trapped_for += dt;
        if self.stuck_web.is_none() && self.tether_from.is_none() && self.web_splat <= 0.0 {
            self.break_free(rng);
            return;
        }
        if let Some(web_id) = self.stuck_web {
            if let Some(world) = web_world.as_deref()
                && !world.contains(web_id)
            {
                self.break_free(rng);
                return;
            }
            self.pluck_timer -= dt;
            if self.pluck_timer <= 0.0 {
                self.pluck_timer = rng.uniform(0.16, 0.34);
                let strength = rng.uniform(0.55, 1.0);
                if let Some(web) = web_world.and_then(|w| w.web_mut(web_id)) {
                    web.pluck((self.x, self.y), strength);
                }
            }
        }
        self.draw_jitter = self.struggle_phase.sin() * self.length * 0.16;
        let escape_rate = if self.captured_by_web {
            0.05
        } else if self.web_splat > 0.0 {
            0.08
        } else {
            0.14
        };
        self.free_progress += dt * escape_rate * rng.uniform(0.4, 1.2);
        if self.free_progress >= 1.0 {
            self.break_free(rng);
        }
    }

    fn feel_spider<C: WorldCreature>(&self, sp: &C, flee: &mut (f64, f64)) {
        if sp.dragging() || sp.desktop_fully_hidden() {
            return;
        }
        if sp.acts_as_hunter() && sp.hunting_prey() && sp.prey_fly() == Some(self.id) {
            return;
        }
        let (sx, sy) = (sp.x(), sp.y());
        let d = distance(self.x, self.y, sx, sy);
        let sp_size = sp.size();
        let mut panic_radius = sp_size * 5.5 + 110.0 + sp.current_speed() * 0.22 + sp.aim_intent() * 70.0;
        if sp.airborne() {
            panic_radius += sp_size * 3.0;
        }
        if d < panic_radius && d > 1e-3 {
            let mut w = 1.0 - d / panic_radius;
            w *= w;
            flee.0 += (self.x - sx) / d * w;
            flee.1 += (self.y - sy) / d * w;
        }
    }

    fn update_flying<C: WorldCreature>(
        &mut self,
        dt: f64,
        fear: &FearField<'_, C>,
        scratch: &mut Vec<u32>,
        web_world: Option<&mut WebWorld>,
        rng: &mut PyRandom,
        playfield: Option<&Playfield>,
    ) {
        // 1) Fear.
        let mut flee = (0.0, 0.0);
        match fear.grid {
            Some(grid) => {
                grid.query(self.x, self.y, fear.reach + 1.0, scratch);
                for &i in scratch.iter() {
                    self.feel_spider(&fear.spiders[i as usize], &mut flee);
                }
            }
            None => {
                for sp in fear.spiders {
                    self.feel_spider(sp, &mut flee);
                }
            }
        }
        let threat = hypot(flee.0, flee.1);
        if threat > 0.01 {
            self.panic_level = clamp(self.panic_level + dt * 4.0, 0.0, 1.0);
            self.target_heading = flee.1.atan2(flee.0);
            self.hover_timer = 0.0;
            self.wander_timer = rng.uniform(0.05, 0.16);
        } else {
            self.panic_level = clamp(self.panic_level - dt * 1.6, 0.0, 1.0);
            self.update_wander(dt, rng);
        }

        // 2) Edge avoidance.
        let edge = FLY_EDGE_MARGIN + self.length;
        if let Some(steer) = self.edge_steer(edge, playfield) {
            self.target_heading = blend_heading(self.target_heading, steer, 0.6);
        }

        // 3) Turn and pick a speed.
        let agility = self.turn_rate * (1.0 + self.panic_level * 0.8);
        self.heading = angle_lerp(self.heading, self.target_heading, agility * dt);
        let calm_speed = if self.panic_level > 0.03 || self.motion_mode == MotionMode::Walk {
            self.cruise
        } else if self.motion_mode == MotionMode::Turn {
            self.cruise * 0.06
        } else {
            0.0
        };
        let target_speed = calm_speed + (self.panic - calm_speed) * self.panic_level;
        let response = if target_speed <= 1.0 { 10.0 } else { 7.0 };
        self.speed += (target_speed - self.speed) * clamp(dt * response, 0.0, 1.0);
        if self.panic_level <= 0.03 && self.motion_mode == MotionMode::Pause && self.speed < 2.0 {
            self.speed = 0.0;
        }

        // 4) Integrate.
        let (fwd_x, fwd_y) = (self.heading.cos(), self.heading.sin());
        let moving01 = clamp(self.speed / 1.0f64.max(self.cruise), 0.0, 1.0);
        let buzz = self.buzz_phase.sin() * self.length * (0.08 + moving01 * 0.82 + self.panic_level * 0.6);
        let (perp_x, perp_y) = (-fwd_y, fwd_x);
        self.vx = fwd_x * self.speed + perp_x * buzz;
        self.vy = fwd_y * self.speed + perp_y * buzz;
        if self.speed <= 0.0 && self.panic_level <= 0.03 {
            self.vx = 0.0;
            self.vy = 0.0;
        }
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.clamp_to_screen(playfield);

        // 5) Blunder into a web?
        self.web_check_timer -= dt;
        if self.web_check_timer <= 0.0 {
            self.web_check_timer = rng.uniform(0.05, 0.1);
            if let Some(world) = web_world {
                self.maybe_get_stuck(dt, world, rng);
            }
        }
    }

    fn update_wander(&mut self, dt: f64, rng: &mut PyRandom) {
        self.motion_timer -= dt;
        self.wander_timer -= dt;
        match self.motion_mode {
            MotionMode::Walk => {
                if self.wander_timer <= 0.0 {
                    self.wander_timer = rng.uniform(0.18, 0.55);
                    self.target_heading = normalize_angle(self.target_heading + rng.uniform(-0.34, 0.34));
                }
                if self.motion_timer <= 0.0 {
                    let roll = rng.random();
                    if roll < 0.58 {
                        self.motion_mode = MotionMode::Pause;
                        self.motion_timer = rng.uniform(0.22, 1.35);
                    } else {
                        self.motion_mode = MotionMode::Turn;
                        self.motion_timer = rng.uniform(0.09, 0.34);
                        let sign = if rng.choice_index(2) == 0 { -1.0 } else { 1.0 };
                        self.target_heading = normalize_angle(self.heading + sign * rng.uniform(0.45, 2.35));
                    }
                }
            }
            MotionMode::Turn => {
                if self.motion_timer <= 0.0 {
                    if rng.random() < 0.45 {
                        self.motion_mode = MotionMode::Pause;
                        self.motion_timer = rng.uniform(0.12, 0.7);
                    } else {
                        self.motion_mode = MotionMode::Walk;
                        self.motion_timer = rng.uniform(0.25, 1.05);
                        self.wander_timer = rng.uniform(0.16, 0.5);
                    }
                }
            }
            MotionMode::Pause => {
                if self.motion_timer <= 0.0 {
                    if rng.random() < 0.62 {
                        self.motion_mode = MotionMode::Turn;
                        self.motion_timer = rng.uniform(0.08, 0.32);
                        let sign = if rng.choice_index(2) == 0 { -1.0 } else { 1.0 };
                        self.target_heading = normalize_angle(self.heading + sign * rng.uniform(0.35, 2.6));
                    } else {
                        self.motion_mode = MotionMode::Walk;
                        self.motion_timer = rng.uniform(0.24, 1.15);
                        self.wander_timer = rng.uniform(0.14, 0.45);
                        self.target_heading = normalize_angle(self.heading + rng.uniform(-0.42, 0.42));
                    }
                }
            }
        }
    }

    fn edge_steer(&self, edge: f64, playfield: Option<&Playfield>) -> Option<f64> {
        let mut push_x = 0.0;
        let mut push_y = 0.0;
        if self.x < edge {
            push_x += 1.0;
        } else if self.x > self.screen_w - edge {
            push_x -= 1.0;
        }
        if self.y < edge {
            push_y += 1.0;
        } else if self.y > self.screen_h - edge {
            push_y -= 1.0;
        }
        if let Some(p) = split_screens(playfield) {
            if push_x == 0.0 {
                if !p.contains(self.x + edge, self.y, 0.0) {
                    push_x -= 1.0;
                } else if !p.contains(self.x - edge, self.y, 0.0) {
                    push_x += 1.0;
                }
            }
            if push_y == 0.0 {
                if !p.contains(self.x, self.y + edge, 0.0) {
                    push_y -= 1.0;
                } else if !p.contains(self.x, self.y - edge, 0.0) {
                    push_y += 1.0;
                }
            }
        }
        if push_x == 0.0 && push_y == 0.0 {
            return None;
        }
        Some(f64::atan2(push_y, push_x))
    }

    fn clamp_to_screen(&mut self, playfield: Option<&Playfield>) {
        let m = FLY_EDGE_MARGIN;
        if self.x < m {
            self.x = m;
            self.heading = self.reflect_x();
        } else if self.x > self.screen_w - m {
            self.x = self.screen_w - m;
            self.heading = self.reflect_x();
        }
        if self.y < m {
            self.y = m;
            self.heading = self.reflect_y();
        } else if self.y > self.screen_h - m {
            self.y = self.screen_h - m;
            self.heading = self.reflect_y();
        }
        if let Some(p) = split_screens(playfield)
            && !p.contains(self.x, self.y, m)
        {
            let (cx, cy) = p.clamp(self.x, self.y, m);
            if (cx - self.x).abs() > 1e-6 {
                self.heading = self.reflect_x();
            }
            if (cy - self.y).abs() > 1e-6 {
                self.heading = self.reflect_y();
            }
            self.x = cx;
            self.y = cy;
        }
    }

    fn reflect_x(&mut self) -> f64 {
        let h = normalize_angle(PI - self.heading);
        self.target_heading = h;
        h
    }

    fn reflect_y(&mut self) -> f64 {
        let h = normalize_angle(-self.heading);
        self.target_heading = h;
        h
    }

    fn maybe_get_stuck(&mut self, dt: f64, world: &mut WebWorld, rng: &mut PyRandom) {
        if world.webs.is_empty() {
            return;
        }
        let base_chance = if self.panic_level < 0.3 { 0.55 } else { 0.22 };
        let reach = self.length * 1.3 + 5.0;
        for web in world.webs.iter_mut() {
            if !web.is_complete() {
                continue;
            }
            let Some(snap) = self.web_capture_point(web, reach) else {
                continue;
            };
            if rng.random() < clamp(base_chance * (self.web_check_timer + dt) * 12.0, 0.02, 0.85) {
                self.stick_to_web(web, snap, rng);
                return;
            }
        }
    }

    /// Nearest point on the web's silk within `radius`, or `None`.
    pub fn web_capture_point(&self, web: &Web, radius: f64) -> Option<Point> {
        let (x0, y0, x1, y1) = web.bbox();
        if self.x < x0 - radius || self.x > x1 + radius || self.y < y0 - radius || self.y > y1 + radius {
            return None;
        }
        let mut best_pt = None;
        let mut best_d = radius;
        for &(si, segi, _len) in web.tearable() {
            let Some(s) = web.strands.get(si) else {
                continue;
            };
            if segi + 1 >= s.points.len() {
                continue;
            }
            let (ax, ay) = s.points[segi];
            let (bx, by) = s.points[segi + 1];
            let (pt, d) = seg_project(ax, ay, bx, by, self.x, self.y);
            if d < best_d {
                best_d = d;
                best_pt = Some(pt);
            }
        }
        best_pt
    }

    /// Repaint footprint `(x, y, w, h)`.
    pub fn footprint(&self) -> Xywh {
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

/// `Fly._blend_heading`.
pub fn blend_heading(a: f64, b: f64, t: f64) -> f64 {
    let delta = normalize_angle(b - a);
    normalize_angle(a + delta * clamp(t, 0.0, 1.0))
}

// ======================================================================
// Projectiles
// ======================================================================

/// What a thrown glob is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotTarget {
    Fly(FlyId),
    Creature(CreatureId),
}

/// A glob of silk a spider flings at a fly or a foe: aimed once, leading the
/// target, then flown straight (or homing with Silk tracking).
#[derive(Clone, Debug)]
pub struct WebShotProjectile {
    pub shooter: CreatureId,
    pub target: ShotTarget,
    pub kind: String,
    pub homing: f64,
    pub origin: Point,
    pub pos: Point,
    pub vel: Point,
    pub travelled: f64,
    pub max_travel: f64,
    /// Last up to 9 positions, oldest first.
    pub trail: Vec<Point>,
    pub done: bool,
    pub hit: bool,
}

/// What the projectile needs to know about its shooter at launch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShooterPose {
    pub id: CreatureId,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub size: f64,
}

impl ShooterPose {
    pub fn of<C: WorldCreature>(c: &C) -> ShooterPose {
        ShooterPose { id: c.id(), x: c.x(), y: c.y(), heading: c.heading(), size: c.size() }
    }
}

fn or_one(v: f64) -> f64 {
    if v == 0.0 { 1.0 } else { v }
}

impl WebShotProjectile {
    /// `WebShotProjectile.__init__`: aim at a target at `(tx, ty)` moving at
    /// `(tvx, tvy)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shooter: ShooterPose,
        target: ShotTarget,
        tx: f64,
        ty: f64,
        tvx: f64,
        tvy: f64,
        kind: &str,
        homing: f64,
    ) -> WebShotProjectile {
        let (fx, fy) = (shooter.heading.cos(), shooter.heading.sin());
        let ox = shooter.x + fx * shooter.size * 0.6;
        let oy = shooter.y + fy * shooter.size * 0.6;
        let mut travel = or_one(hypot(tx - ox, ty - oy)) / WEB_SHOT_SPEED;
        let (mut aim_x, mut aim_y) = (tx, ty);
        for _ in 0..4 {
            aim_x = tx + tvx * travel;
            aim_y = ty + tvy * travel;
            travel = or_one(hypot(aim_x - ox, aim_y - oy)) / WEB_SHOT_SPEED;
        }
        let (ux, uy) = (aim_x - ox, aim_y - oy);
        let d = or_one(hypot(ux, uy));
        WebShotProjectile {
            shooter: shooter.id,
            target,
            kind: kind.to_string(),
            homing: homing.max(0.0),
            origin: (ox, oy),
            pos: (ox, oy),
            vel: (ux / d * WEB_SHOT_SPEED, uy / d * WEB_SHOT_SPEED),
            travelled: 0.0,
            max_travel: 160.0f64.max(d * 1.8),
            trail: vec![(ox, oy)],
            done: false,
            hit: false,
        }
    }

    /// Advance; resolves the target through `flies` / `creatures`.
    pub fn update<C: WorldCreature>(&mut self, dt: f64, flies: &mut [Fly], creatures: &mut [C], rng: &mut PyRandom) {
        // Look the target up; a missing one is gone (Python's would read as
        // dead/eaten by then).
        let (tx, ty, size) = match self.target {
            ShotTarget::Fly(id) => match flies.iter().find(|f| f.id == id) {
                Some(f) if !(!f.alive() || f.eaten() || f.dragging || f.trapped()) => (f.x, f.y, f.size),
                _ => {
                    self.done = true;
                    return;
                }
            },
            ShotTarget::Creature(id) => match find_index(creatures, id).map(|i| &creatures[i]) {
                Some(c) if !(c.dead() || c.dragging() || c.webbed()) => (c.x(), c.y(), c.size()),
                _ => {
                    self.done = true;
                    return;
                }
            },
        };
        let (vx, vy) = if self.homing > 0.0 {
            let (dx, dy) = (tx - self.pos.0, ty - self.pos.1);
            let dd = or_one(hypot(dx, dy));
            let vx = self.vel.0 + dx / dd * WEB_SHOT_SPEED * 5.0 * self.homing * dt;
            let vy = self.vel.1 + dy / dd * WEB_SHOT_SPEED * 5.0 * self.homing * dt;
            let sp = or_one(hypot(vx, vy));
            (vx / sp * WEB_SHOT_SPEED, vy / sp * WEB_SHOT_SPEED)
        } else {
            self.vel
        };
        self.vel = (vx, vy);
        let (nx, ny) = (self.pos.0 + vx * dt, self.pos.1 + vy * dt);
        self.travelled += hypot(nx - self.pos.0, ny - self.pos.1);
        self.pos = (nx, ny);
        push_trail(&mut self.trail, self.pos, 9);
        let hit_radius = WEB_SHOT_HIT_RADIUS.max(size * 0.55);
        if distance(nx, ny, tx, ty) <= hit_radius {
            match self.target {
                ShotTarget::Fly(id) => {
                    if let Some(f) = flies.iter_mut().find(|f| f.id == id) {
                        pin_fly_with_shot(f, self.origin, &self.kind, self.shooter, rng);
                    }
                }
                ShotTarget::Creature(id) => {
                    if let Some(i) = find_index(creatures, id) {
                        creatures[i].web_pinned(&self.kind, self.shooter);
                    }
                }
            }
            self.hit = true;
            self.done = true;
        } else if self.travelled >= self.max_travel {
            self.done = true;
        }
    }
}

pub(crate) fn push_trail(trail: &mut Vec<Point>, p: Point, keep: usize) {
    trail.push(p);
    if trail.len() > keep {
        let extra = trail.len() - keep;
        trail.drain(..extra);
    }
}

/// `_pin_fly_with_shot`.
pub fn pin_fly_with_shot(fly: &mut Fly, origin: Point, kind: &str, shooter: CreatureId, rng: &mut PyRandom) {
    fly.pin_with_web_shot(origin, kind, rng);
    fly.trapper = Some(shooter);
}

/// `(x, y, w, h)` around a trail, padded 12 (`WebShotProjectile.footprint`).
pub fn trail_footprint(trail: &[Point]) -> Xywh {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in trail {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let pad = 12.0;
    (x0 - pad, y0 - pad, (x1 - x0) + pad * 2.0, (y1 - y0) + pad * 2.0)
}

/// Silk in flight, owned by the fly world.
#[derive(Clone, Debug)]
pub enum Projectile {
    Shot(WebShotProjectile),
    Aimed(AimedSilk),
}

impl Projectile {
    pub fn done(&self) -> bool {
        match self {
            Projectile::Shot(p) => p.done,
            Projectile::Aimed(p) => p.done,
        }
    }

    pub fn trail(&self) -> &[Point] {
        match self {
            Projectile::Shot(p) => &p.trail,
            Projectile::Aimed(p) => &p.trail,
        }
    }

    pub fn pos(&self) -> Point {
        match self {
            Projectile::Shot(p) => p.pos,
            Projectile::Aimed(p) => p.pos,
        }
    }

    pub fn footprint(&self) -> Xywh {
        trail_footprint(self.trail())
    }

    pub fn update<C: WorldCreature>(&mut self, dt: f64, flies: &mut [Fly], creatures: &mut [C], rng: &mut PyRandom) {
        match self {
            Projectile::Shot(p) => p.update(dt, flies, creatures, rng),
            Projectile::Aimed(p) => p.update(dt, creatures, flies, rng),
        }
    }
}

// ======================================================================
// Remains and nests
// ======================================================================

/// `FlyRemains.parts[*]["kind"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Leg,
    Wing,
    Crumb,
}

/// One disassembled bit of an eaten fly, relative to the remains.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemainsPart {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub len: f64,
    pub kind: PartKind,
}

/// The leftover bits of a devoured fly, fading over a few seconds.
#[derive(Clone, Debug)]
pub struct FlyRemains {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub t: f64,
    pub parts: Vec<RemainsPart>,
}

impl FlyRemains {
    pub const LIFETIME: f64 = 3.6;

    pub fn new(x: f64, y: f64, scale: f64, rng: &mut PyRandom) -> FlyRemains {
        let scale = clamp(scale, 0.5, 2.2);
        let l = FLY_BASE_LENGTH * scale;
        let n = rng.randint(4, 6);
        let mut parts = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let ang = rng.uniform(0.0, TAU);
            let dist = rng.uniform(l * 0.4, l * 1.6);
            let kind = match rng.choice_index(4) {
                0 | 1 => PartKind::Leg,
                2 => PartKind::Wing,
                _ => PartKind::Crumb,
            };
            let rot = rng.uniform(0.0, TAU);
            let len = l * rng.uniform(0.4, 0.9);
            parts.push(RemainsPart { x: ang.cos() * dist, y: ang.sin() * dist, rot, len, kind });
        }
        FlyRemains { x, y, scale, t: 0.0, parts }
    }

    /// Advance; true once faded.
    pub fn update(&mut self, dt: f64) -> bool {
        self.t += dt;
        self.t >= Self::LIFETIME
    }

    pub fn footprint(&self) -> Xywh {
        let pad = FLY_BASE_LENGTH * self.scale * 2.2 + 8.0;
        (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)
    }

    /// `clamp(1 - t / LIFETIME, 0, 1)`.
    pub fn fade(&self) -> f64 {
        clamp(1.0 - self.t / Self::LIFETIME, 0.0, 1.0)
    }
}

/// A movable nest the flies crawl out of.
#[derive(Clone, Debug)]
pub struct FlySpawner {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub radius: f64,
    pub pulse: f64,
    pub dragging: bool,
    drag_dx: f64,
    drag_dy: f64,
    /// `[angle, radius fraction, speed]` per crawling speck.
    pub specks: Vec<[f64; 3]>,
}

impl FlySpawner {
    pub fn new(x: f64, y: f64, scale: f64, rng: &mut PyRandom) -> FlySpawner {
        let scale = clamp(scale, 0.5, 2.2);
        let pulse = rng.random() * TAU;
        let specks = (0..5).map(|_| [rng.uniform(0.0, TAU), rng.uniform(0.3, 0.85), rng.uniform(0.4, 1.1)]).collect();
        FlySpawner { x, y, scale, radius: 22.0 * scale, pulse, dragging: false, drag_dx: 0.0, drag_dy: 0.0, specks }
    }

    pub fn hit_test(&self, x: f64, y: f64) -> bool {
        distance(self.x, self.y, x, y) <= self.radius + 4.0
    }

    pub fn start_drag(&mut self, mx: f64, my: f64) {
        self.dragging = true;
        self.drag_dx = self.x - mx;
        self.drag_dy = self.y - my;
    }

    pub fn drag_to(&mut self, mx: f64, my: f64, sw: f64, sh: f64) {
        self.x = clamp(mx + self.drag_dx, self.radius, sw - self.radius);
        self.y = clamp(my + self.drag_dy, self.radius, sh - self.radius);
    }

    pub fn release_drag(&mut self) {
        self.dragging = false;
    }

    pub fn clamp_to_screen(&mut self, sw: f64, sh: f64) {
        self.x = clamp(self.x, self.radius, self.radius.max(sw - self.radius));
        self.y = clamp(self.y, self.radius, self.radius.max(sh - self.radius));
    }

    /// A point at the nest opening with an outward-ish heading.
    pub fn emit_point(&self, rng: &mut PyRandom) -> (f64, f64, f64) {
        let ang = rng.uniform(0.0, TAU);
        let r = self.radius * 0.5;
        (self.x + ang.cos() * r, self.y + ang.sin() * r, ang)
    }

    pub fn update(&mut self, dt: f64) {
        self.pulse += dt * 2.4;
        for s in &mut self.specks {
            s[0] += dt * s[2] * 0.6;
        }
    }

    pub fn footprint(&self) -> Xywh {
        let pad = self.radius + 12.0;
        (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)
    }
}

// ======================================================================
// FlyWorld
// ======================================================================

/// `FlyWorld(...)` keyword arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlyWorldConfig {
    pub enabled: bool,
    pub min_interval: f64,
    pub max_interval: f64,
    pub max_flies: i64,
    pub scale: f64,
}

impl Default for FlyWorldConfig {
    fn default() -> Self {
        FlyWorldConfig { enabled: true, min_interval: 4.0, max_interval: 9.0, max_flies: 6, scale: 1.0 }
    }
}

/// `FlyWorld.configure(...)`: every field optional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlyConfigure {
    pub enabled: Option<bool>,
    pub min_interval: Option<f64>,
    pub max_interval: Option<f64>,
    pub max_flies: Option<i64>,
    pub scale: Option<f64>,
    pub use_spawner: Option<bool>,
}

/// Owns every fly: spawning on a timer, updating, and the silk in flight.
#[derive(Debug)]
pub struct FlyWorld {
    pub rng: PyRandom,
    pub screen_w: f64,
    pub screen_h: f64,
    /// The manager's playfield (the monitors). Python shared one object;
    /// the manager must hand over a fresh copy when the layout changes.
    pub playfield: Option<Playfield>,
    pub enabled: bool,
    pub min_interval: f64,
    pub max_interval: f64,
    pub max_flies: i64,
    pub scale: f64,
    pub flies: Vec<Fly>,
    pub spawners: Vec<FlySpawner>,
    pub projectiles: Vec<Projectile>,
    pub remains: Vec<FlyRemains>,
    pub use_spawner: bool,
    pub spawn_timer: f64,
    dirty: Vec<Xywh>,
    removed: Vec<Xywh>,
    next_fly_id: u64,
    grid: SpatialGrid,
    scratch: Vec<u32>,
}

impl FlyWorld {
    pub fn new(screen_w: f64, screen_h: f64, config: FlyWorldConfig, rng: PyRandom) -> FlyWorld {
        let mut world = FlyWorld {
            rng,
            screen_w,
            screen_h,
            playfield: None,
            enabled: config.enabled,
            min_interval: config.min_interval,
            max_interval: config.max_interval,
            max_flies: config.max_flies,
            scale: clamp(config.scale, 0.5, 2.2),
            flies: Vec::new(),
            spawners: Vec::new(),
            projectiles: Vec::new(),
            remains: Vec::new(),
            use_spawner: true,
            spawn_timer: 0.0,
            dirty: Vec::new(),
            removed: Vec::new(),
            next_fly_id: 1,
            grid: SpatialGrid::new(),
            scratch: Vec::new(),
        };
        world.spawn_timer = world.roll_interval();
        world
    }

    fn alloc_fly_id(&mut self) -> FlyId {
        let id = FlyId(self.next_fly_id);
        self.next_fly_id += 1;
        id
    }

    /// Make a fly with this world's stream and a fresh id (not added).
    pub fn make_fly(&mut self, x: f64, y: f64, heading: Option<f64>, scale: f64) -> Fly {
        let id = self.alloc_fly_id();
        Fly::new(id, x, y, self.screen_w, self.screen_h, heading, scale, &mut self.rng)
    }

    pub fn fly(&self, id: FlyId) -> Option<&Fly> {
        self.flies.iter().find(|f| f.id == id)
    }

    pub fn fly_mut(&mut self, id: FlyId) -> Option<&mut Fly> {
        self.flies.iter_mut().find(|f| f.id == id)
    }

    // -- configuration ------------------------------------------------------
    pub fn set_screen(&mut self, screen_w: f64, screen_h: f64) {
        self.screen_w = screen_w;
        self.screen_h = screen_h;
        for fly in &mut self.flies {
            fly.screen_w = screen_w;
            fly.screen_h = screen_h;
        }
        for sp in &mut self.spawners {
            sp.clamp_to_screen(screen_w, screen_h);
        }
    }

    pub fn configure(&mut self, c: FlyConfigure) {
        if let Some(e) = c.enabled {
            self.enabled = e;
        }
        if let Some(v) = c.min_interval {
            self.min_interval = 0.3f64.max(v);
        }
        if let Some(v) = c.max_interval {
            self.max_interval = 0.4f64.max(v);
        }
        if self.max_interval < self.min_interval {
            self.max_interval = self.min_interval;
        }
        if let Some(v) = c.max_flies {
            self.max_flies = v.max(0);
        }
        if let Some(v) = c.scale {
            self.scale = clamp(v, 0.5, 2.2);
            for sp in &mut self.spawners {
                sp.scale = self.scale;
                sp.radius = 22.0 * self.scale;
            }
        }
        if let Some(v) = c.use_spawner {
            self.use_spawner = v;
            if v {
                self.ensure_spawner();
            } else {
                self.clear_spawners();
            }
        }
        self.spawn_timer = self.spawn_timer.min(self.max_interval);
    }

    fn roll_interval(&mut self) -> f64 {
        let lo = 0.3f64.max(self.min_interval.min(self.max_interval));
        let hi = lo.max(self.min_interval.max(self.max_interval));
        self.rng.uniform(lo, hi)
    }

    // -- lifecycle ----------------------------------------------------------
    pub fn clear(&mut self) {
        for fly in &self.flies {
            self.removed.push(fly.footprint());
        }
        self.flies.clear();
        for proj in &self.projectiles {
            self.removed.push(proj.footprint());
        }
        self.projectiles.clear();
    }

    /// Drop predator bookkeeping (the spider roster is being rebuilt).
    pub fn clear_hunters(&mut self) {
        for fly in &mut self.flies {
            fly.hunters.clear();
            fly.trapper = None;
            if fly.state == FlyState::Trapped && fly.tether_from.is_some() && !fly.captured_by_web {
                fly.tether_from = None;
            }
        }
        for proj in &self.projectiles {
            self.removed.push(proj.footprint());
        }
        self.projectiles.clear();
    }

    pub fn count_alive(&self) -> i64 {
        self.flies.iter().filter(|f| f.alive()).count() as i64
    }

    // -- spawners -----------------------------------------------------------
    fn default_spawner_pos(&self) -> Point {
        (self.screen_w * 0.5, self.screen_h * 0.5)
    }

    pub fn ensure_spawner(&mut self) {
        if self.spawners.is_empty() {
            let (x, y) = self.default_spawner_pos();
            let sp = FlySpawner::new(x, y, self.scale, &mut self.rng);
            self.spawners.push(sp);
        }
    }

    /// Add a nest (at a random spot when `at` is `None`); returns its index.
    pub fn add_spawner(&mut self, at: Option<Point>) -> usize {
        let at = at.unwrap_or_else(|| {
            let x = self.rng.uniform(self.screen_w * 0.2, self.screen_w * 0.8);
            let y = self.rng.uniform(self.screen_h * 0.2, self.screen_h * 0.8);
            (x, y)
        });
        let mut sp = FlySpawner::new(at.0, at.1, self.scale, &mut self.rng);
        sp.clamp_to_screen(self.screen_w, self.screen_h);
        self.spawners.push(sp);
        self.use_spawner = true;
        self.spawners.len() - 1
    }

    pub fn clear_spawners(&mut self) {
        for sp in &self.spawners {
            self.removed.push(sp.footprint());
        }
        self.spawners.clear();
    }

    /// `spawners_to_dict`: for `Scene.nests`.
    pub fn spawners_to_dict(&self) -> Vec<Value> {
        self.spawners.iter().map(|sp| json!({"x": sp.x, "y": sp.y, "scale": sp.scale})).collect()
    }

    /// `restore_spawners`: replace the nests with saved ones.
    pub fn restore_spawners(&mut self, entries: &Value) -> usize {
        let Some(list) = entries.as_array() else {
            return 0;
        };
        self.clear_spawners();
        for entry in list {
            if !entry.is_object() {
                continue;
            }
            let x = entry.get("x").and_then(pymath::py_float);
            let y = entry.get("y").and_then(pymath::py_float);
            let scale = pymath::get_float(entry, "scale", self.scale);
            let (Some(x), Some(y), Some(scale)) = (x, y, scale) else {
                continue;
            };
            let mut sp = FlySpawner::new(x, y, scale, &mut self.rng);
            sp.clamp_to_screen(self.screen_w, self.screen_h);
            self.spawners.push(sp);
        }
        self.spawners.len()
    }

    pub fn reset_spawners(&mut self) {
        self.clear_spawners();
        self.use_spawner = true;
        self.ensure_spawner();
    }

    /// Index of the topmost nest under the point.
    pub fn hit_spawner_at(&self, x: f64, y: f64) -> Option<usize> {
        (0..self.spawners.len()).rev().find(|&i| self.spawners[i].hit_test(x, y))
    }

    /// Topmost live fly under the point.
    pub fn hit_fly_at(&self, x: f64, y: f64) -> Option<FlyId> {
        self.flies.iter().rev().find(|f| f.alive() && distance(f.x, f.y, x, y) <= f.length * 1.6 + 6.0).map(|f| f.id)
    }

    pub fn start_drag_fly(&mut self, id: FlyId, mx: f64, my: f64) {
        if let Some(f) = self.flies.iter_mut().find(|f| f.id == id) {
            f.start_drag(mx, my);
        }
    }

    pub fn drag_fly_to(&mut self, id: FlyId, mx: f64, my: f64) {
        let playfield = self.playfield.as_ref();
        if let Some(f) = self.flies.iter_mut().find(|f| f.id == id) {
            f.drag_to(mx, my, playfield);
        }
    }

    pub fn release_drag_fly(&mut self, id: FlyId, mx: f64, my: f64) {
        if let Some(f) = self.flies.iter_mut().find(|f| f.id == id) {
            f.release_drag(mx, my, &mut self.rng);
        }
    }

    /// Python's renderer rolls a trapped fly's splat pegs the first time it
    /// draws one. Call before painting, in draw order, to keep that draw.
    pub fn ensure_capture_pegs(&mut self, id: FlyId) {
        if let Some(f) = self.flies.iter_mut().find(|f| f.id == id)
            && f.state == FlyState::Trapped
            && f.web_pegs.is_empty()
        {
            f.make_web_pegs(&mut self.rng);
        }
    }

    // -- web shots / remains ------------------------------------------------
    pub fn launch_web_shot<C: WorldCreature>(&mut self, shooter: &C, fly: FlyId, kind: &str) -> bool {
        let Some(f) = self.fly(fly) else {
            return false;
        };
        if !f.alive() || f.trapped() || f.dragging {
            return false;
        }
        let shot = WebShotProjectile::new(
            ShooterPose::of(shooter),
            ShotTarget::Fly(fly),
            f.x,
            f.y,
            f.vx,
            f.vy,
            kind,
            shooter.web_homing(),
        );
        self.projectiles.push(Projectile::Shot(shot));
        true
    }

    /// Fling the same silk at a foe spider (DC-45).
    pub fn launch_web_shot_at_creature<C: WorldCreature>(&mut self, shooter: &C, target: &C, kind: &str) -> bool {
        if target.dead() || target.webbed() {
            return false;
        }
        let shot = WebShotProjectile::new(
            ShooterPose::of(shooter),
            ShotTarget::Creature(target.id()),
            target.x(),
            target.y(),
            0.0,
            0.0,
            kind,
            shooter.web_homing(),
        );
        self.projectiles.push(Projectile::Shot(shot));
        true
    }

    /// `manager.fly_world.projectiles.append(AimedSilk(...))` (adventure).
    pub fn launch_aimed_silk<C: WorldCreature>(&mut self, shooter: &C, angle: f64, reach: f64) {
        self.projectiles.push(Projectile::Aimed(AimedSilk::new(ShooterPose::of(shooter), angle, reach)));
    }

    pub fn add_remains(&mut self, at: Point, scale: f64) {
        let r = FlyRemains::new(at.0, at.1, scale, &mut self.rng);
        self.remains.push(r);
    }

    fn keep_on_screen(playfield: Option<&Playfield>, sp: &mut FlySpawner) {
        if let Some(p) = split_screens(playfield) {
            (sp.x, sp.y) = p.clamp(sp.x, sp.y, sp.radius);
        }
    }

    fn spawn_edge_point(&mut self) -> (f64, f64, f64) {
        let (mut x0, mut y0, mut w, mut h) = (0.0, 0.0, self.screen_w, self.screen_h);
        if let Some(p) = split_screens(self.playfield.as_ref())
            && !p.rects().is_empty()
        {
            let rect = p.rects()[self.rng.choice_index(p.rects().len())];
            (x0, y0, w, h) = (rect.x, rect.y, rect.w, rect.h);
        }
        let m = FLY_EDGE_MARGIN + 6.0;
        let rng = &mut self.rng;
        match rng.choice_index(4) {
            0 => {
                let x = x0 + rng.uniform(m, w - m);
                (x, y0 + m, rng.uniform(0.2, PI - 0.2))
            }
            1 => {
                let x = x0 + rng.uniform(m, w - m);
                (x, y0 + h - m, rng.uniform(-PI + 0.2, -0.2))
            }
            2 => {
                let y = y0 + rng.uniform(m, h - m);
                (x0 + m, y, rng.uniform(-1.2, 1.2))
            }
            _ => {
                let y = y0 + rng.uniform(m, h - m);
                (x0 + w - m, y, rng.uniform(PI - 1.2, PI + 1.2))
            }
        }
    }

    fn spawn_origin(&mut self) -> (f64, f64, f64) {
        if self.use_spawner && !self.spawners.is_empty() {
            let i = self.rng.choice_index(self.spawners.len());
            return self.spawners[i].emit_point(&mut self.rng);
        }
        self.spawn_edge_point()
    }

    /// Release a fly (unless at the cap and not forced).
    pub fn spawn(&mut self, force: bool) -> Option<FlyId> {
        if self.count_alive() >= self.max_flies && !force {
            return None;
        }
        let (x, y, heading) = self.spawn_origin();
        let mut fly = self.make_fly(x, y, Some(heading), self.scale);
        fly.motion_mode = MotionMode::Walk;
        fly.motion_timer = self.rng.uniform(0.28, 0.75);
        fly.speed = fly.cruise;
        fly.vx = heading.cos() * fly.speed;
        fly.vy = heading.sin() * fly.speed;
        let id = fly.id;
        self.flies.push(fly);
        Some(id)
    }

    /// The user asked for a fly right now.
    pub fn spawn_now(&mut self) -> &'static str {
        let cap = if self.max_flies > 0 { self.max_flies } else { 1 };
        if self.count_alive() >= cap {
            return "The screen already has plenty of flies.";
        }
        self.spawn(true);
        "Released a fly."
    }

    // -- per-frame ------------------------------------------------------------
    /// Advance projectiles only (the mission loop does this on its own).
    pub fn update_projectiles<C: WorldCreature>(&mut self, dt: f64, creatures: &mut [C]) {
        let mut projectiles = std::mem::take(&mut self.projectiles);
        for proj in &mut projectiles {
            proj.update(dt, &mut self.flies, creatures, &mut self.rng);
        }
        projectiles.retain(|p| !p.done());
        self.projectiles = projectiles;
    }

    /// `FlyWorld.update(dt, spiders, web_world)`.
    pub fn update<C: WorldCreature>(&mut self, dt: f64, spiders: &mut [C], mut web_world: Option<&mut WebWorld>) {
        if self.use_spawner {
            self.ensure_spawner();
        }
        if self.enabled && self.max_flies > 0 {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = self.roll_interval();
                self.spawn(false);
            }
        }

        self.dirty.clear();
        let playfield = self.playfield.as_ref();
        for sp in &mut self.spawners {
            if !sp.dragging {
                Self::keep_on_screen(playfield, sp);
            }
            sp.update(dt);
            self.dirty.push(sp.footprint());
        }

        // Silk in flight.
        let mut projectiles = std::mem::take(&mut self.projectiles);
        for proj in &mut projectiles {
            proj.update(dt, &mut self.flies, spiders, &mut self.rng);
            self.dirty.push(proj.footprint());
        }
        projectiles.retain(|p| !p.done());
        self.projectiles = projectiles;

        // Fading remains.
        let dirty = &mut self.dirty;
        self.remains.retain_mut(|rem| {
            dirty.push(rem.footprint());
            !rem.update(dt)
        });

        // The fear field.
        let spiders: &[C] = spiders;
        let mut reach = 0.0f64;
        for sp in spiders {
            let mut r = sp.size() * 5.5 + 110.0 + sp.current_speed() * 0.22 + sp.aim_intent() * 70.0;
            if sp.airborne() {
                r += sp.size() * 3.0;
            }
            if r.is_finite() {
                reach = reach.max(r);
            } else {
                reach = f64::INFINITY;
            }
        }
        let use_grid = spiders.len() >= GRID_MIN_ITEMS && reach.is_finite();
        if use_grid {
            self.grid.rebuild(reach.max(64.0), spiders.iter().map(|s| (s.x(), s.y())));
        }
        let fear = FearField { spiders, grid: if use_grid { Some(&self.grid) } else { None }, reach };

        let playfield = self.playfield.as_ref();
        let rng = &mut self.rng;
        let scratch = &mut self.scratch;
        let dirty = &mut self.dirty;
        let removed = &mut self.removed;
        self.flies.retain_mut(|fly| {
            fly.update(dt, &fear, scratch, web_world.as_deref_mut(), rng, playfield);
            if fly.removable() {
                removed.push(fly.footprint());
                return false;
            }
            let fp = fly.footprint();
            dirty.push(fp);
            if let Some(prev) = fly.bbox_prev {
                dirty.push(prev);
            }
            fly.bbox_prev = Some(fp);
            true
        });
        self.dirty.append(&mut self.removed);
    }

    pub fn dirty_rects(&self) -> &[Xywh] {
        &self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature_view::{JobIntent, Relation};

    struct Nobody;
    impl WorldCreature for Nobody {
        fn id(&self) -> CreatureId {
            CreatureId(0)
        }
        fn index(&self) -> i64 {
            0
        }
        fn progression_id(&self) -> &str {
            ""
        }
        fn team_id(&self) -> &str {
            "neutral"
        }
        fn job_id(&self) -> &str {
            "none"
        }
        fn x(&self) -> f64 {
            0.0
        }
        fn y(&self) -> f64 {
            0.0
        }
        fn heading(&self) -> f64 {
            0.0
        }
        fn size(&self) -> f64 {
            24.0
        }
        fn relation_to(&self, _o: &Self) -> Relation {
            Relation::Neutral
        }
        fn set_job_intent(&mut self, _i: &JobIntent<'_>) {}
        fn set_job_base_id(&mut self, _b: Option<&str>) {}
        fn rng(&mut self) -> &mut PyRandom {
            unreachable!()
        }
    }

    fn world(seed: u64, playfield: Option<Playfield>) -> FlyWorld {
        let config = FlyWorldConfig { enabled: true, min_interval: 0.2, max_interval: 0.5, max_flies: 8, scale: 1.0 };
        let mut w = FlyWorld::new(1300.0, 600.0, config, PyRandom::from_u64(seed));
        w.playfield = playfield;
        w.use_spawner = false;
        w
    }

    fn two_screens() -> Playfield {
        use crate::ScreenRect;
        Playfield::new(
            1300.0,
            600.0,
            &[ScreenRect::new(0.0, 0.0, 800.0, 600.0), ScreenRect::new(800.0, 0.0, 500.0, 360.0)],
        )
    }

    fn off_frames(w: &mut FlyWorld, field: &Playfield) -> usize {
        let mut off = 0;
        let mut nobody: Vec<Nobody> = Vec::new();
        for _ in 0..3600 {
            w.update(1.0 / 60.0, &mut nobody, None);
            off += w.flies.iter().filter(|f| f.alive() && !field.contains(f.x, f.y, 0.0)).count();
        }
        off
    }

    #[test]
    fn flies_stay_on_the_monitors() {
        let field = two_screens();
        assert_eq!(off_frames(&mut world(5, Some(field.clone())), &field), 0);
        assert!(off_frames(&mut world(5, None), &field) > 0);
    }

    #[test]
    fn a_fly_in_the_hole_comes_out_and_new_ones_arrive_on_a_monitor() {
        let field = two_screens();
        let mut w = world(5, Some(field.clone()));
        w.enabled = false;
        let id = w.spawn(true).unwrap();
        let f = w.fly_mut(id).unwrap();
        (f.x, f.y) = (1100.0, 500.0);
        w.update::<Nobody>(1.0 / 60.0, &mut [], None);
        let f = w.fly(id).unwrap();
        assert!(field.contains(f.x, f.y, 0.0));
        for _ in 0..200 {
            let id = w.spawn(true).unwrap();
            let f = w.fly(id).unwrap();
            assert!(field.contains(f.x, f.y, 0.0));
        }
        let nest = w.add_spawner(Some((1100.0, 500.0)));
        w.update::<Nobody>(1.0 / 60.0, &mut [], None);
        let sp = &w.spawners[nest];
        assert!(field.contains(sp.x, sp.y, 0.0));
    }

    #[test]
    fn a_seeded_fly_world_replays() {
        let run = |seed| {
            let mut w = world(seed, None);
            w.use_spawner = true;
            for _ in 0..1200 {
                w.update::<Nobody>(1.0 / 60.0, &mut [], None);
            }
            w.flies.iter().map(|f| (f.x, f.y, f.heading)).collect::<Vec<_>>()
        };
        assert_eq!(run(11), run(11));
        assert!(!run(11).is_empty());
        assert_ne!(run(11), run(12));
    }
}
