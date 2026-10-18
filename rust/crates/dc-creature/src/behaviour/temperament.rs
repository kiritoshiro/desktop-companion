//! Port of `creature/behaviour/temperament.py`: which behaviour modules a
//! spider acts on, and the movement each temperament flavours. The drift
//! helpers live in `dc_kinematics::drift` (the body movement needs them
//! mid-update); `prime_drift` is wrapped in `creature.rs`.

use crate::constants::smootherstep;
use crate::creature::{Creature, Modules};
use crate::pymath::{angle_to, clamp, clamp_point, distance, hypot, pair_of, py_int, rand_range};
use crate::state::State;
use crate::world::{CreatureId, Population};
use serde_json::Value;
use std::f64::consts::PI;

impl Creature {
    #[inline]
    pub fn has_behaviour_module(&self, bit: u8) -> bool {
        self.modules.has(bit)
    }

    pub fn acts_as_hunter(&self) -> bool {
        (self.job_id == "hunter" || self.modules.has(Modules::HUNTER)) && self.has_skill("chase")
    }

    pub fn acts_as_jumper(&self) -> bool {
        self.modules.has(Modules::JUMPER) && self.has_skill("jump")
    }

    pub fn acts_as_observer(&self) -> bool {
        (self.job_id == "scout" || self.modules.has(Modules::OBSERVER)) && self.has_skill("observe")
    }

    pub fn acts_as_nope(&self) -> bool {
        self.modules.has(Modules::NOPE) && self.has_skill("run_away")
    }

    pub fn acts_as_drifter(&self) -> bool {
        self.modules.has(Modules::DRIFTER) && self.has_skill("drift")
    }

    pub fn acts_as_webber(&self) -> bool {
        (self.job_id == "webber" || self.modules.has(Modules::WEBBER)) && self.has_skill("weave_web")
    }

    pub fn acts_as_web_shooter(&self) -> bool {
        self.modules.has(Modules::WEB_SHOOTER) && (self.has_skill("shoot_web") || self.has_skill("wall_web"))
    }

    /// `_cursor_speed()`
    pub fn cursor_speed(&self) -> f64 {
        hypot(self.prev_cursor_vx, self.prev_cursor_vy)
    }

    /// `_cursor_is_still_for_observe()`
    pub fn cursor_is_still_for_observe(&self) -> bool {
        if self.hunting_prey
            && let Some(prey) = self.prey
        {
            if prey.trapped {
                return false;
            }
            return !prey.is_moving;
        }
        self.cursor_speed() <= self.pf("observe_cursor_speed", 24.0)
    }

    /// `_hunter_approach_speed(dist, reaction)`
    pub fn hunter_approach_speed(&self, dist_to_cursor: f64, reaction: f64) -> f64 {
        let (far_speed, near_speed) = match self.p().get("hunt_approach_speed") {
            None => (70.0, 185.0),
            Some(v) => match pair_of(Some(v)) {
                Some([a, b]) => (a, b),
                None => (70.0, 185.0),
            },
        };
        let closeness = 1.0 - clamp(dist_to_cursor / 1.0f64.max(reaction), 0.0, 1.0);
        (far_speed + (near_speed - far_speed) * smootherstep(closeness)) * self.speed_mult()
    }

    /// `_observer_radius_bounds()`
    pub fn observer_radius_bounds(&self) -> (f64, f64) {
        let size = self.kin.size;
        let (mut lo, mut hi) = match pair_of(self.p().get("observe_radius_mult")) {
            Some([a, b]) => (a * size, b * size),
            None => (size * 4.2, size * 6.8),
        };
        lo = (size * 2.6).max(lo);
        hi = (lo + size * 0.9).max(hi);
        (lo, hi)
    }

    /// `_observer_anchor(mx, my)`: (x, y, mate) to watch, or `None`.
    pub fn observer_anchor(&mut self, mx: f64, my: f64, pop: &Population) -> Option<(f64, f64, Option<CreatureId>)> {
        if !self.acts_as_observer() {
            return None;
        }
        let reaction = self.pf("reaction_radius", 360.0);
        let cursor_range = reaction * self.pf("observe_cursor_range_mult", 1.18);
        let dist_to_cursor = distance(self.kin.x, self.kin.y, mx, my);
        let social_range = (self.kin.size * 12.0).max(reaction * self.pf("observe_social_range_mult", 0.95));
        let social_pref = clamp(self.pf("observe_social_preference", 0.66), 0.0, 1.0);
        if let Some(mate) = self.find_social_target(social_range, pop) {
            let m = pop.get(mate).unwrap();
            let dist_to_mate = distance(self.kin.x, self.kin.y, m.x, m.y);
            if dist_to_cursor > cursor_range || dist_to_mate <= dist_to_cursor * 1.12 || self.rng.random() < social_pref {
                return Some((m.x, m.y, Some(mate)));
            }
        }
        if dist_to_cursor < cursor_range {
            return Some((mx, my, None));
        }
        None
    }

    /// `_maybe_jumper_hop(dt, after, toward)`
    pub fn maybe_jumper_hop(&mut self, dt: f64, after: crate::creature::After, toward: (f64, f64)) -> bool {
        if !self.has_skill("jump") || !self.acts_as_jumper() || self.airborne || self.kin.motion_paused {
            return false;
        }
        if self.kin.speed <= 1.0 && self.kin.current_speed <= 8.0 {
            return false;
        }
        self.hop_timer -= dt;
        if self.hop_timer > 0.0 {
            return false;
        }
        self.hop_timer = rand_range(self.personality.get("hop_interval"), 0.18, 0.42, &mut self.rng);
        let power = rand_range(self.personality.get("hop_power"), 0.34, 0.58, &mut self.rng);
        self.enter_coil(after, power, Some(toward));
        true
    }

    /// `_cursor_triggers_nope_escape(mx, my, cvx, cvy)`
    pub fn cursor_triggers_nope_escape(&self, mx: f64, my: f64, cursor_vx: f64, cursor_vy: f64) -> bool {
        if !self.acts_as_nope() || self.dragging || self.airborne || self.nope_cooldown > 0.0 {
            return false;
        }
        if matches!(self.state, State::Jump | State::Land | State::Retreat | State::Startled | State::Dragged) {
            return false;
        }
        let dist_to_cursor = distance(self.kin.x, self.kin.y, mx, my);
        let trigger_radius = match self.p().get("nope_trigger_radius") {
            Some(v) => crate::pymath::py_float(v).unwrap_or(360.0),
            None => self.pf("reaction_radius", 360.0),
        };
        if dist_to_cursor > trigger_radius {
            return false;
        }
        let near_radius = self.pf("nope_near_radius", trigger_radius * 0.62);
        if dist_to_cursor <= near_radius {
            return true;
        }
        let cursor_speed = hypot(cursor_vx, cursor_vy);
        let speed_threshold = self.pf("nope_cursor_speed", 45.0);
        if cursor_speed < speed_threshold {
            return false;
        }
        let toward_x = self.kin.x - mx;
        let toward_y = self.kin.y - my;
        let toward_len = 1.0f64.max(hypot(toward_x, toward_y));
        let approach_score = (cursor_vx * toward_x + cursor_vy * toward_y) / (1.0f64.max(cursor_speed) * toward_len);
        approach_score > self.pf("nope_toward_score", 0.12)
    }

    /// `enter_nope_escape(mx, my, continuing=...)`: the backwards zigzag hop chain.
    pub fn enter_nope_escape(&mut self, mx: f64, my: f64, continuing: bool) {
        if !self.phase_allowed_id("run_away") || !self.has_skill("run_away") {
            self.enter_alert(mx, my);
            return;
        }
        if !self.phase_allowed_id("jump") || !self.has_skill("jump") {
            self.enter_retreat(mx, my);
            return;
        }
        if !continuing {
            match self.p().get("nope_jump_count") {
                Some(Value::Array(items)) if items.len() >= 2 => {
                    let a = py_int(&items[0]).unwrap_or(3);
                    let b = py_int(&items[1]).unwrap_or(5);
                    self.nope_repeats = self.rng.randint(a, b);
                }
                None => self.nope_repeats = self.rng.randint(3, 5),
                Some(Value::Null) => self.nope_repeats = 4,
                Some(v) => self.nope_repeats = py_int(v).unwrap_or(4),
            }
            self.nope_zigzag_dir = crate::pymath::choice_sign(&mut self.rng);
            crate::bump!(self.mood, arousal = 0.6, valence = -0.22, curiosity = -0.12);
        } else {
            self.nope_repeats = (self.nope_repeats - 1).max(0);
        }
        let (x, y, size) = (self.kin.x, self.kin.y, self.kin.size);
        let mut away = (y - my).atan2(x - mx);
        if distance(x, y, mx, my) < 1.0 {
            away = self.kin.heading + PI;
        }
        self.nope_zigzag_dir *= -1.0;
        let jump_dist = rand_range(self.personality.get("nope_jump_distance"), size * 4.5, size * 8.0, &mut self.rng);
        let zigzag = rand_range(self.personality.get("nope_zigzag_distance"), size * 2.2, size * 4.6, &mut self.rng);
        let perp = away + PI * 0.5;
        let target_x = x + away.cos() * jump_dist + perp.cos() * zigzag * self.nope_zigzag_dir;
        let target_y = y + away.sin() * jump_dist + perp.sin() * zigzag * self.nope_zigzag_dir;
        let (target_x, target_y) = clamp_point(target_x, target_y, self.margin, self.screen_w, self.screen_h);
        let face_cursor = angle_to(x, y, mx, my);
        self.kin.heading = face_cursor;
        self.kin.target_heading = face_cursor;
        self.social_target = None;
        self.raise_wiggle_burst(0.85);
        self.launch_jump(target_x, target_y, crate::creature::JumpKind::Escape, crate::creature::After::Nope, None, None, 1.0);
    }
}
