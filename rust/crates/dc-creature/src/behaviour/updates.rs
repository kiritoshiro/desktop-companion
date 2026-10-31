//! Port of `creature/behaviour/updates.py`: the per-frame update of each
//! committed action.

use crate::bump;
use crate::creature::{After, Creature, CuddlePhase, InspectPhase, JumpKind, RangeMode};
use crate::mood::MoodBump;
use crate::pymath::{angle_to, choice_sign, clamp, clamp_point, distance, hypot, smoothstep};
use crate::state::State;
use crate::world::{Command, CreatureId, Env, Population, SocialRole};
use std::f64::consts::PI;

impl Creature {
    /// The social target's view when it is still a usable partner:
    /// present, not being carried and still among the neighbours.
    fn live_target(&self, pop: &Population) -> Option<(CreatureId, f64, f64)> {
        let id = self.social_target?;
        let t = pop.get(id)?;
        if t.dragging || !self.neighbors.contains(&id) {
            return None;
        }
        Some((id, t.x, t.y))
    }

    /// `_update_observe(dt, mx, my)`
    pub fn update_observe(&mut self, dt: f64, mx: f64, my: f64, pop: &Population) {
        let mut target = None;
        if self.social_target.is_some() {
            match self.live_target(pop) {
                Some(t) => target = Some(t),
                None => self.social_target = None,
            }
        }
        let (fx, fy) = match target {
            Some((_, x, y)) => (x, y),
            None => (mx, my),
        };
        self.set_focus(fx, fy, 1.0);
        let reaction = self.pf("reaction_radius", 360.0);
        let cursor_range = reaction * self.pf("observe_cursor_range_mult", 1.18);
        let (x, y) = (self.kin.x, self.kin.y);
        if target.is_none() && distance(x, y, fx, fy) > cursor_range * 1.4 {
            self.enter_idle();
            return;
        }
        let (lo, hi) = self.observer_radius_bounds();
        let mut desired = clamp(self.observe_radius.unwrap_or((lo + hi) * 0.5), lo, hi);
        let current = distance(x, y, fx, fy);
        if current < lo * 0.72 || current > hi * 1.35 {
            desired = clamp(current, lo, hi);
            self.observe_radius = Some(desired);
        }
        let mut away_x = x - fx;
        let mut away_y = y - fy;
        let mut away_len = hypot(away_x, away_y);
        if away_len <= 1e-4 {
            let fallback = self.kin.heading + PI;
            away_x = fallback.cos();
            away_y = fallback.sin();
            away_len = 1.0;
        }
        away_x /= away_len;
        away_y /= away_len;
        self.observe_clock += dt;
        (self.kin.target_x, self.kin.target_y) =
            clamp_point(fx + away_x * desired, fy + away_y * desired, self.margin, self.screen_w, self.screen_h);
        self.kin.target_heading = angle_to(x, y, fx, fy);
        self.kin.speed = self.pf("observe_speed", 44.0) * self.speed_mult();

        if self.decision_timer <= 0.0 && self.rng.random() < 0.18 {
            self.decision_timer = self.rng.uniform(0.45, 0.9);
            let jitter = self.rng.uniform(-self.kin.size * 0.8, self.kin.size * 0.8);
            self.observe_radius = Some(clamp(desired + jitter, lo, hi));
        }
        if self.state_timer <= 0.0 {
            let linger = clamp(self.pf("observe_linger_chance", 0.78), 0.0, 1.0);
            let next_anchor = self.observer_anchor(mx, my, pop);
            if let Some((ax, ay, next_target)) = next_anchor
                && self.rng.random() < linger
            {
                self.enter_observe(ax, ay, next_target);
                if self.rng.random() < 0.45 {
                    self.observe_dir *= -1.0;
                }
                return;
            }
            self.social_target = None;
            if current < reaction * 0.8 && self.rng.random() < 0.35 {
                self.enter_alert(fx, fy);
            } else {
                self.enter_idle();
            }
        }
    }

    /// `_update_inspect(dt, mx, my)`
    pub fn update_inspect(&mut self, dt: f64, mx: f64, my: f64, pop: &Population) {
        let target = self.social_target;
        if target.is_some() {
            match self.live_target(pop) {
                Some((_, tx, ty)) => self.set_focus(tx, ty, 1.0),
                None => {
                    self.social_target = None;
                    self.enter_idle();
                    return;
                }
            }
        } else {
            self.set_focus(mx, my, 1.0);
        }
        let (fx, fy) = (self.focus_x, self.focus_y);
        let (x, y) = (self.kin.x, self.kin.y);
        let d = distance(x, y, fx, fy);
        let personal = self.kin.size * 3.1;
        self.inspect_clock += dt;
        self.kin.inspect_intent = (self.kin.inspect_intent + dt * 2.2).min(1.0);

        match self.inspect_phase {
            InspectPhase::Approach => {
                self.kin.target_heading = angle_to(x, y, fx, fy);
                self.kin.target_x = fx;
                self.kin.target_y = fy;
                self.kin.speed = 46.0 * self.speed_mult();
                if d <= personal {
                    self.inspect_phase = InspectPhase::Study;
                    self.inspect_clock = 0.0;
                    self.kin.motion_paused = true;
                }
            }
            InspectPhase::Study => {
                self.kin.speed = 0.0;
                self.kin.motion_paused = true;
                self.kin.target_heading = angle_to(x, y, fx, fy);
                self.raise_wiggle_burst(0.25);
                if self.inspect_clock > self.rng.uniform(0.7, 1.3) {
                    self.inspect_clock = 0.0;
                    let roll = self.rng.random();
                    if roll < 0.4 {
                        self.inspect_phase = InspectPhase::Orbit;
                        self.orbit_clock = 0.0;
                        self.orbit_dir = choice_sign(&mut self.rng);
                    } else if roll < 0.62 {
                        self.head_tilt = self.rng.uniform(-0.5, 0.5);
                    } else if roll < 0.78 {
                        self.set_wiggle_burst(0.6);
                    }
                }
                if d > personal * 2.4 {
                    self.inspect_phase = InspectPhase::Approach;
                    self.kin.motion_paused = false;
                }
            }
            InspectPhase::Orbit => {
                self.kin.motion_paused = false;
                self.orbit_clock += dt;
                let ang = angle_to(fx, fy, x, y) + self.orbit_dir * dt * 1.3;
                (self.kin.target_x, self.kin.target_y) = clamp_point(
                    fx + ang.cos() * personal,
                    fy + ang.sin() * personal,
                    self.margin,
                    self.screen_w,
                    self.screen_h,
                );
                self.kin.target_heading = angle_to(x, y, fx, fy);
                self.kin.speed = 40.0 * self.speed_mult();
                if self.orbit_clock > self.rng.uniform(0.8, 1.6) {
                    self.inspect_phase = InspectPhase::Study;
                    self.inspect_clock = 0.0;
                }
            }
        }

        if self.state_timer <= 0.0 {
            let m = self.mood;
            let roll = self.rng.random();
            if m.affection > 0.6 && roll < 0.35 {
                self.enter_cuddle(fx, fy, target);
            } else if m.valence > 0.45 && m.arousal > 0.5 && roll < 0.6 {
                match target {
                    Some(t) => self.enter_play(t, None, pop),
                    None => self.enter_aim(fx, fy, None, After::Outcome, (0.4, 0.8), 0.1, pop),
                }
            } else {
                bump!(self.mood, curiosity = -0.2);
                self.social_target = None;
                self.enter_idle();
            }
        }
    }

    /// `_update_cuddle(dt, mx, my)`
    pub fn update_cuddle(&mut self, dt: f64, mx: f64, my: f64, pop: &Population) {
        if self.social_target.is_some() {
            match self.live_target(pop) {
                Some((_, tx, ty)) => self.set_focus(tx, ty, 1.0),
                None => {
                    self.social_target = None;
                    self.enter_idle();
                    return;
                }
            }
        } else {
            self.set_focus(mx, my, 1.0);
        }
        let (fx, fy) = (self.focus_x, self.focus_y);
        let (x, y) = (self.kin.x, self.kin.y);
        let d = distance(x, y, fx, fy);
        let snug = self.kin.size * 1.7;
        self.cuddle_clock += dt;
        self.cuddle_intent = (self.cuddle_intent + dt * 1.8).min(1.0);
        bump!(self.mood, affection = dt * 0.25, valence = dt * 0.22);

        if self.cuddle_phase == CuddlePhase::Approach {
            self.kin.target_heading = angle_to(x, y, fx, fy);
            self.kin.target_x = fx;
            self.kin.target_y = fy;
            self.kin.speed = 42.0 * self.speed_mult();
            if d <= snug {
                self.cuddle_phase = CuddlePhase::Snuggle;
                self.kin.motion_paused = true;
            }
        } else {
            self.kin.speed = 0.0;
            self.kin.motion_paused = true;
            self.kin.target_heading = angle_to(x, y, fx, fy);
            self.raise_wiggle_burst(0.2);
            self.boop_timer -= dt;
            if self.boop_timer <= 0.0 {
                self.boop_timer = self.rng.uniform(0.5, 1.0);
                self.set_wiggle_burst(0.5);
                self.rear = (self.rear + 0.4).min(1.0);
            }
            if d > snug * 2.6 {
                self.cuddle_phase = CuddlePhase::Approach;
                self.kin.motion_paused = false;
            }
        }
        if self.state_timer <= 0.0 {
            bump!(self.mood, valence = 0.25, affection = 0.1);
            self.social_target = None;
            self.enter_wander();
        }
    }

    /// `_update_aim(dt, mx, my)`
    pub fn update_aim(&mut self, dt: f64, mx: f64, my: f64, pop: &Population) {
        let target = self.social_target;
        match self.live_target(pop) {
            Some((_, tx, ty)) => self.set_focus(tx, ty, 1.0),
            None => {
                if target.is_some() {
                    self.social_target = None;
                }
                self.set_focus(mx, my, 1.0);
            }
        }
        let (fx, fy) = (self.focus_x, self.focus_y);
        self.kin.speed = 0.0;
        self.kin.motion_paused = true;
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, fx, fy);
        self.aim_intent = (self.aim_intent + dt * 3.0).min(1.0);
        self.crouch = (self.crouch + dt * 4.0).min(1.0);
        self.range_clock += dt;
        if self.range_clock >= self.range_switch {
            self.range_clock = 0.0;
            self.range_switch = self.rng.uniform(0.16, 0.32);
            self.range_mode = if self.range_mode == RangeMode::Waggle { RangeMode::Nod } else { RangeMode::Waggle };
            if self.range_mode == RangeMode::Waggle {
                self.raise_wiggle_burst(0.7);
            } else {
                self.rear = (self.rear + 0.5).min(1.0);
            }
        }
        if self.state_timer <= 0.0 {
            if self.rng.random() < self.aim_abort_chance {
                bump!(self.mood, arousal = -0.1);
                if self.aim_after == After::Play && target.is_some() {
                    self.enter_inspect(fx, fy, target);
                } else {
                    self.enter_idle();
                }
            } else {
                self.launch_jump(fx, fy, JumpKind::Pounce, self.aim_after, None, None, 1.0);
            }
        }
    }

    /// `_update_catch(dt, mx, my)`
    pub fn update_catch(&mut self, dt: f64, pop: &Population) {
        let target = self.social_target;
        let live = self.live_target(pop);
        if let Some((_, tx, ty)) = live {
            self.kin.catch_point = (tx, ty);
        }
        self.kin.catch_blend = (self.kin.catch_blend + dt * 6.0).min(1.0);
        self.kin.speed = 0.0;
        self.kin.motion_paused = true;
        let (cx, cy) = self.kin.catch_point;
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, cx, cy);
        if self.state_timer <= 0.0 && !self.catch_resolved {
            self.catch_resolved = true;
            let d = distance(self.kin.x, self.kin.y, cx, cy);
            let got_it = d < self.kin.size * 2.6 && (target.is_none() || live.is_some());
            if got_it && self.rng.random() < 0.6 {
                bump!(self.mood, valence = 0.28, affection = 0.18, arousal = 0.1);
                match target {
                    Some(t) => self.enter_play(t, Some(SocialRole::Chase), pop),
                    None => self.enter_cuddle(cx, cy, None),
                }
            } else {
                bump!(self.mood, valence = -0.1, arousal = 0.18);
                self.set_wiggle_burst(0.8);
                if self.rng.random() < 0.4 && self.mood.arousal > 0.5 {
                    self.enter_aim(cx, cy, target, After::Outcome, (0.3, 0.6), 0.1, pop);
                } else {
                    self.enter_idle();
                }
            }
        }
    }

    /// `_update_play(dt, mx, my)`
    pub fn update_play(&mut self, dt: f64, env: &mut Env<'_>) {
        let pop = env.population;
        let Some((target, tx, ty)) = self.live_target(pop) else {
            self.social_target = None;
            self.enter_idle();
            return;
        };
        self.set_focus(tx, ty, 0.8);
        self.play_clock += dt;
        let (x, y) = (self.kin.x, self.kin.y);
        let d = distance(x, y, tx, ty);
        let contact = self.kin.size * 1.9;

        if self.social_role == SocialRole::Chase {
            self.kin.motion_paused = false;
            let jx = tx + self.rng.uniform(-12.0, 12.0);
            let jy = ty + self.rng.uniform(-12.0, 12.0);
            (self.kin.target_x, self.kin.target_y) = clamp_point(jx, jy, self.margin, self.screen_w, self.screen_h);
            self.kin.speed = 92.0 * self.speed_mult();
            self.kin.target_heading = angle_to(x, y, tx, ty);
            if d <= contact {
                bump!(self.mood, valence = 0.22, arousal = 0.18);
                env.commands.push(Command::BumpMood {
                    target,
                    bump: MoodBump { valence: 0.2, arousal: 0.2, ..Default::default() },
                });
                let roll = self.rng.random();
                if roll < 0.45 {
                    self.social_role = SocialRole::Flee;
                    if pop.get(target).is_some_and(|t| t.state != State::Play) {
                        env.commands.push(Command::EnterPlay { target, partner: self.id, role: SocialRole::Chase });
                    }
                    self.enter_spring(After::Idle, 0.8);
                } else if roll < 0.7 && self.mood.affection > 0.4 {
                    self.enter_cuddle(tx, ty, Some(target));
                } else {
                    self.enter_spring(After::Idle, 1.0);
                }
                return;
            }
        } else {
            self.kin.motion_paused = false;
            let away = (y - ty).atan2(x - tx);
            self.play_lookback -= dt;
            if self.play_lookback <= 0.0 && d > contact * 2.5 {
                self.play_lookback = self.rng.uniform(0.6, 1.2);
                self.kin.speed = 0.0;
                self.kin.target_heading = angle_to(x, y, tx, ty);
                self.set_wiggle_burst(0.4);
            } else {
                (self.kin.target_x, self.kin.target_y) = clamp_point(
                    x + away.cos() * 90.0,
                    y + away.sin() * 90.0,
                    self.margin,
                    self.screen_w,
                    self.screen_h,
                );
                self.kin.speed = 104.0 * self.speed_mult();
                self.kin.target_heading = away;
            }
        }
        if self.state_timer <= 0.0 {
            bump!(self.mood, valence = 0.12);
            self.social_target = None;
            self.enter_idle();
        }
    }

    /// `_update_jump(dt)`
    pub fn update_jump(&mut self, dt: f64) {
        if self.jump_duration <= 0.0 {
            self.enter_land(self.jump_after);
            return;
        }
        self.jump_t = (self.jump_t + dt / self.jump_duration).min(1.0);
        let ease = smoothstep(self.jump_t);
        let (fx, fy) = self.jump_from;
        let (tx, ty) = self.jump_to;
        let new_x = fx + (tx - fx) * ease;
        let new_y = fy + (ty - fy) * ease;
        let dx = new_x - self.kin.x;
        let dy = new_y - self.kin.y;
        self.kin.x = new_x;
        self.kin.y = new_y;
        self.kin.translate_leg_world_points(dx, dy, 1.0);
        self.jump_z = self.jump_peak * 4.0 * self.jump_t * (1.0 - self.jump_t);
        let horiz_speed = hypot(dx, dy) / 1e-4f64.max(dt);
        self.kin.current_speed = clamp(horiz_speed, 0.0, 260.0);
        self.kin.vel_x = dx / 1e-4f64.max(dt);
        self.kin.vel_y = dy / 1e-4f64.max(dt);
        self.kin.bob_phase += dt * 6.0;
        self.kin.breath_phase += dt * 2.4;
        self.kin.body_bob = 0.0;
        self.kin.abdomen_pulse = (self.jump_t * PI).sin() * 0.08;
        self.kin.ceph_pulse = (self.jump_t * PI).sin() * 0.05;
        if self.jump_t >= 1.0 {
            self.enter_land(self.jump_after);
        }
    }
}
