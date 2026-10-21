//! Port of `creature/behaviour/actions.py`: entering a deliberate action
//! (inspect, observe, cuddle, aim, coil, jump, land, catch, feed, play,
//! zoomies, drift run, roll) and the social helpers.

use crate::bump;
use crate::constants::smootherstep;
use crate::creature::{After, Creature, CuddlePhase, DriftRunMode, DriftRunPhase, InspectPhase, JumpKind, RangeMode};
use crate::phase_scheduler::Phase;
use crate::pymath::{angle_to, choice_sign, clamp, clamp_point, distance, hypot, pair_of, py_float, rand_range};
use crate::state::State;
use crate::world::{CreatureId, Population, SocialRole};
use std::f64::consts::{PI, TAU};

impl Creature {
    /// `enter_inspect(tx, ty, target)`
    pub fn enter_inspect(&mut self, tx: f64, ty: f64, target: Option<CreatureId>) {
        if !self.phase_allowed(Phase::Inspect) || !self.has_skill("inspect") {
            self.enter_idle();
            return;
        }
        self.state = State::Inspect;
        self.kin.motion_paused = false;
        self.social_target = target;
        self.set_focus(tx, ty, 1.0);
        self.kin.target_x = tx;
        self.kin.target_y = ty;
        self.kin.speed = 46.0 * self.speed_mult();
        self.state_timer = self.rng.uniform(2.2, 4.8);
        self.inspect_phase = InspectPhase::Approach;
        self.inspect_clock = 0.0;
        self.orbit_dir = choice_sign(&mut self.rng);
        bump!(self.mood, curiosity = 0.22, arousal = 0.06);
    }

    /// `enter_observe(tx, ty, target)`
    pub fn enter_observe(&mut self, tx: f64, ty: f64, target: Option<CreatureId>) {
        if !self.phase_allowed(Phase::Observe) || !self.has_skill("observe") {
            if self.phase_allowed(Phase::Inspect) && self.has_skill("inspect") {
                self.enter_inspect(tx, ty, target);
            } else {
                self.enter_idle();
            }
            return;
        }
        self.state = State::Observe;
        self.kin.motion_paused = false;
        self.social_target = target;
        self.set_focus(tx, ty, 1.0);
        self.kin.target_x = tx;
        self.kin.target_y = ty;
        self.observe_dir = choice_sign(&mut self.rng);
        self.observe_clock = 0.0;
        let (lo, hi) = self.observer_radius_bounds();
        let current = distance(self.kin.x, self.kin.y, tx, ty);
        let backoff = self.kin.size * self.pf("observe_backoff_mult", 1.6);
        self.observe_radius = Some(clamp((current + backoff).max(lo), lo, hi));
        self.observe_vertical_scale = clamp(self.pf("observe_vertical_scale", 0.58), 0.35, 1.0);
        self.observe_horizontal_scale = 1.0f64.max(self.pf("observe_horizontal_scale", 1.18));
        self.kin.speed = self.pf("observe_speed", 44.0) * self.speed_mult();
        self.state_timer = rand_range(self.personality.get("observe_orbit_time"), 2.8, 6.4, &mut self.rng);
        bump!(self.mood, curiosity = 0.18, arousal = 0.04);
    }

    /// `enter_cuddle(tx, ty, target)`
    pub fn enter_cuddle(&mut self, tx: f64, ty: f64, target: Option<CreatureId>) {
        if !self.phase_allowed(Phase::Cuddle) || !self.has_skill("cuddle") {
            self.enter_idle();
            return;
        }
        self.state = State::Cuddle;
        self.kin.motion_paused = false;
        self.social_target = target;
        self.set_focus(tx, ty, 1.0);
        self.kin.target_x = tx;
        self.kin.target_y = ty;
        self.kin.speed = 40.0 * self.speed_mult();
        self.state_timer = self.rng.uniform(3.0, 6.5);
        self.cuddle_phase = CuddlePhase::Approach;
        self.cuddle_clock = 0.0;
        self.boop_timer = self.rng.uniform(0.4, 0.9);
        bump!(self.mood, affection = 0.18, valence = 0.06);
    }

    /// `enter_aim(tx, ty, target, after, ranging, abort_chance)`
    #[allow(clippy::too_many_arguments)]
    pub fn enter_aim(
        &mut self,
        tx: f64,
        ty: f64,
        target: Option<CreatureId>,
        after: After,
        ranging: (f64, f64),
        abort_chance: f64,
        pop: &Population,
    ) {
        if !self.phase_allowed(Phase::PrepareJumpAttack)
            || !self.has_skill("prepare_jump_attack")
            || !self.has_skill("jump")
        {
            if after == After::Play
                && let Some(t) = target
                && self.phase_allowed(Phase::SocialPlay)
                && self.has_skill("social_play")
            {
                self.enter_play(t, None, pop);
            } else if self.phase_allowed(Phase::Chase) && self.has_skill("chase") {
                self.enter_chase(tx, ty);
            } else if self.phase_allowed(Phase::Approach) && self.has_skill("approach") {
                self.enter_approach(tx, ty);
            } else {
                self.enter_alert(tx, ty);
            }
            return;
        }
        self.state = State::Aim;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.social_target = target;
        self.set_focus(tx, ty, 1.0);
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, tx, ty);
        // rand_range(ranging, ranging[0], ranging[1]) is one uniform either way.
        self.state_timer = self.rng.uniform(ranging.0, ranging.1);
        self.aim_after = after;
        self.aim_abort_chance = clamp(abort_chance, 0.0, 0.9);
        self.range_clock = 0.0;
        self.range_mode = RangeMode::Waggle;
        self.range_switch = self.rng.uniform(0.18, 0.34);
        bump!(self.mood, arousal = 0.16, curiosity = 0.05);
    }

    /// `enter_coil(after, power, toward)`: quick wind-up for a hop.
    pub fn enter_coil(&mut self, after: After, power: f64, toward: Option<(f64, f64)>) {
        if !self.phase_allowed(Phase::Jump) || !self.has_skill("jump") {
            match (after, toward) {
                (After::Wander, _) => self.enter_wander(),
                (After::Approach, Some((x, y))) if self.phase_allowed(Phase::Approach) => self.enter_approach(x, y),
                (After::Chase, Some((x, y))) if self.phase_allowed(Phase::Chase) => self.enter_chase(x, y),
                _ => self.enter_idle(),
            }
            return;
        }
        self.state = State::Coil;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.state_timer = self.rng.uniform(0.10, 0.20);
        self.coil_after = after;
        self.coil_power = clamp(power, 0.4, 1.6);
        self.coil_toward = toward;
        bump!(self.mood, arousal = 0.12, valence = 0.05);
    }

    /// `_launch_jump(tx, ty, kind, after, peak, duration, reach)`
    #[allow(clippy::too_many_arguments)]
    pub fn launch_jump(
        &mut self,
        tx: f64,
        ty: f64,
        kind: JumpKind,
        after: After,
        peak: Option<f64>,
        duration: Option<f64>,
        reach: f64,
    ) {
        if self.webbed_held() && !self.dragging {
            return;
        }
        if !self.has_skill("jump") {
            match after {
                After::Wander => self.enter_wander(),
                After::Approach => self.enter_approach(tx, ty),
                After::Chase => self.enter_chase(tx, ty),
                _ => self.enter_idle(),
            }
            return;
        }
        self.airborne = true;
        self.state = State::Jump;
        self.kin.motion_paused = false;
        self.crouch = 0.0;
        self.jump_kind = kind;
        self.jump_after = after;
        self.jump_t = 0.0;

        let (x, y, size, heading) = (self.kin.x, self.kin.y, self.kin.size, self.kin.heading);
        let dx = tx - x;
        let dy = ty - y;
        let dist = hypot(dx, dy);
        let (land_x, land_y);
        match kind {
            JumpKind::Hop => {
                let land_dist = dist.min(size * 0.7) * reach;
                let ang = if dist > 1.0 { dy.atan2(dx) } else { heading };
                land_x = x + ang.cos() * land_dist;
                land_y = y + ang.sin() * land_dist;
                let coil_power = self.coil_power;
                let hop_peak_mult = self.pf("hop_peak_multiplier", 1.0);
                let hop_duration_mult = self.pf("hop_duration_multiplier", 1.0);
                self.jump_peak = match peak {
                    Some(p) => p,
                    None => size * self.rng.uniform(0.85, 1.25) * coil_power * hop_peak_mult,
                };
                self.jump_duration = match duration {
                    Some(d) => d,
                    None => self.rng.uniform(0.36, 0.5) * hop_duration_mult,
                };
            }
            JumpKind::Escape => {
                let travel = clamp(dist, size * 1.8, size * 11.0) * reach;
                let ang = if dist > 1.0 { dy.atan2(dx) } else { heading + PI };
                land_x = x + ang.cos() * travel;
                land_y = y + ang.sin() * travel;
                let peak_raw = self.personality.get("nope_jump_peak_mult");
                let peak_mult = match peak_raw {
                    None => self.rng.uniform(1.4, 2.4),
                    Some(v) => match pair_of(Some(v)) {
                        Some([a, b]) => self.rng.uniform(a, b),
                        None if v.is_null() => 1.8,
                        None => py_float(v).unwrap_or(1.8),
                    },
                };
                self.jump_peak = peak.unwrap_or(size * peak_mult);
                self.jump_duration = match duration {
                    Some(d) => d,
                    None => rand_range(self.personality.get("nope_jump_duration"), 0.14, 0.22, &mut self.rng),
                };
            }
            JumpKind::Pounce => {
                let overshoot = size * 0.4;
                let travel = clamp(dist + overshoot, size * 1.2, size * 9.0) * reach;
                let ang = if dist > 1.0 { dy.atan2(dx) } else { heading };
                land_x = x + ang.cos() * travel;
                land_y = y + ang.sin() * travel;
                self.kin.target_heading = ang;
                self.kin.heading = ang;
                self.jump_peak = peak.unwrap_or_else(|| clamp(travel * 0.32, size * 0.9, size * 2.6));
                self.jump_duration = duration.unwrap_or_else(|| clamp(travel / 520.0 + 0.30, 0.32, 0.6));
            }
        }
        let (mut land_x, mut land_y) = clamp_point(land_x, land_y, self.margin * 0.5, self.screen_w, self.screen_h);
        if let Some(pf) = self.split_playfield() {
            (land_x, land_y) = pf.0.clamp(land_x, land_y, 8.0f64.max(size * 0.5));
        }
        self.jump_from = (self.kin.x, self.kin.y);
        self.jump_to = (land_x, land_y);
        self.kin.catch_point = (tx, ty);
        for leg in &mut self.kin.legs {
            leg.stepping = false;
            leg.pending_step = false;
        }
    }

    /// `enter_land(after)`
    pub fn enter_land(&mut self, after: After) {
        self.airborne = false;
        self.jump_z = 0.0;
        self.state = State::Land;
        self.kin.motion_paused = false;
        self.kin.speed = 0.0;
        self.squash = if self.kin.spider_gait().is_some() { 0.88 } else { 0.66 };
        self.land_recover = 0.22;
        if after == After::Nope {
            self.state_timer = rand_range(self.personality.get("nope_land_pause"), 0.02, 0.05, &mut self.rng);
        } else {
            self.state_timer = 0.16;
        }
        self.land_after = after;
        self.panic_rehome_legs(0.85);
        bump!(self.mood, arousal = 0.05);
    }

    /// `enter_catch(tx, ty, target)`
    pub fn enter_catch(&mut self, tx: f64, ty: f64, target: Option<CreatureId>) {
        self.state = State::Catch;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.social_target = target;
        self.set_focus(tx, ty, 1.0);
        self.kin.catch_point = (tx, ty);
        self.state_timer = self.rng.uniform(0.45, 0.72);
        self.catch_resolved = false;
        bump!(self.mood, arousal = 0.2);
    }

    /// `enter_feed(tx, ty)`: pounce-and-pin a caught fly, then a munch.
    pub fn enter_feed(&mut self, tx: f64, ty: f64) {
        self.airborne = false;
        self.jump_z = 0.0;
        self.state = State::Feed;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.social_target = None;
        self.weaving_web = None;
        self.repairing_web = None;
        self.web_target = None;
        self.kin.target_x = self.kin.x;
        self.kin.target_y = self.kin.y;
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, tx, ty);
        self.set_focus(tx, ty, 1.0);
        self.kin.catch_point = (tx, ty);
        self.kin.catch_blend = 1.0;
        self.crouch = (self.crouch + 0.55).min(1.0);
        self.raise_wiggle_burst(0.9);
        self.feed_frenzy = self.rng.uniform(0.5, 0.75);
        self.feed_anchor = (self.kin.x, self.kin.y);
        self.feed_paw = 0.0;
        self.state_timer = self.feed_frenzy + self.rng.uniform(0.4, 0.6);
        bump!(self.mood, valence = 0.45, arousal = 0.3, affection = 0.05, curiosity = 0.05);
    }

    /// `enter_play(target, role)`
    pub fn enter_play(&mut self, target: CreatureId, role: Option<SocialRole>, pop: &Population) {
        if !self.phase_allowed(Phase::SocialPlay) || !self.has_skill("social_play") {
            if self.phase_allowed(Phase::Inspect) && self.has_skill("inspect") {
                let (tx, ty) = pop.get(target).map(|t| (t.x, t.y)).unwrap_or((self.focus_x, self.focus_y));
                self.enter_inspect(tx, ty, Some(target));
            } else {
                self.enter_idle();
            }
            return;
        }
        self.state = State::Play;
        self.kin.motion_paused = false;
        self.social_target = Some(target);
        let role = role.unwrap_or_else(|| {
            let other_index = pop.get(target).map(|t| t.index).unwrap_or(self.index);
            if self.index <= other_index { SocialRole::Chase } else { SocialRole::Flee }
        });
        self.social_role = role;
        self.kin.speed = (if role == SocialRole::Chase { 92.0 } else { 104.0 }) * self.speed_mult();
        self.state_timer = self.rng.uniform(2.6, 5.5);
        self.play_clock = 0.0;
        self.play_lookback = self.rng.uniform(0.5, 1.1);
        self.prime_drift(0.55, None);
        bump!(self.mood, valence = 0.16, arousal = 0.2, curiosity = 0.08);
    }

    /// `enter_zoomies()`
    pub fn enter_zoomies(&mut self) {
        if !self.phase_allowed(Phase::Zoomies) || !self.has_skill("zoomies") {
            self.enter_idle();
            return;
        }
        self.state = State::Zoom;
        self.kin.motion_paused = false;
        let angle = self.rng.uniform(-PI, PI);
        let dist = self.rng.uniform(120.0, 260.0);
        let (tx, ty) = (self.kin.x + angle.cos() * dist, self.kin.y + angle.sin() * dist);
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, self.margin, self.screen_w, self.screen_h);
        self.kin.speed = 138.0 * self.speed_mult();
        self.state_timer = self.rng.uniform(0.35, 0.7);
        self.zoom_repeats = self.rng.randint(1, 3);
        self.prime_drift(1.0, None);
        bump!(self.mood, valence = 0.14, arousal = 0.24);
    }

    /// `_reset_drift_run_cooldown()`
    pub fn reset_drift_run_cooldown(&mut self) {
        self.drift_run_cooldown = rand_range(self.personality.get("drift_run_interval"), 4.5, 10.5, &mut self.rng);
    }

    /// `_should_start_drift_run(from_idle=...)`
    pub fn should_start_drift_run(&mut self, from_idle: bool) -> bool {
        if !self.acts_as_drifter() || self.airborne || self.dragging {
            return false;
        }
        if matches!(
            self.state,
            State::Dragged
                | State::Startled
                | State::Retreat
                | State::Chase
                | State::Jump
                | State::Land
                | State::Roll
                | State::Coil
                | State::Catch
        ) {
            return false;
        }
        if self.drift_run_cooldown > 0.0 {
            return false;
        }
        let (key, fallback) =
            if from_idle { ("drift_run_start_chance", 0.86) } else { ("drift_run_wander_chance", 0.34) };
        let chance = clamp(self.pf(key, fallback), 0.0, 1.0);
        self.rng.random() < chance
    }

    /// `enter_drift_run(mode)`: an autonomous Drifter burst.
    pub fn enter_drift_run(&mut self, mode: Option<DriftRunMode>) {
        if !self.acts_as_drifter() {
            self.enter_idle();
            return;
        }
        self.state = State::DriftRun;
        self.kin.motion_paused = false;
        self.social_target = None;
        self.drift_run_dir = choice_sign(&mut self.rng);
        self.kin.drift.drift_dir = self.drift_run_dir;
        let size = self.kin.size;
        self.state_timer = rand_range(self.personality.get("drift_run_time"), 2.2, 4.6, &mut self.rng);
        self.drift_run_turn_rate =
            rand_range(self.personality.get("drift_circle_turn_rate"), 0.95, 1.85, &mut self.rng);
        self.drift_run_radius = rand_range(
            self.personality.get("drift_circle_radius"),
            92.0f64.max(size * 4.0),
            230.0f64.max(size * 8.0),
            &mut self.rng,
        );
        self.drift_run_phase = DriftRunPhase::Charge;
        self.drift_run_phase_timer = rand_range(self.personality.get("drift_charge_time"), 0.55, 1.05, &mut self.rng);
        self.kin.drift.drift_momentum = self.kin.drift.drift_momentum.min(0.25);
        self.kin.drift.drift_boost = 0.0;
        self.kin.drift.last_drift_amount = 0.0;

        let (x, y) = (self.kin.x, self.kin.y);
        let mode = match mode {
            Some(m) => m,
            None => {
                let edge_band = self.margin + 120.0f64.max(self.drift_run_radius * 0.85);
                let near_edge =
                    x < edge_band || x > self.screen_w - edge_band || y < edge_band || y > self.screen_h - edge_band;
                let corner_chance =
                    clamp(self.pf("drift_corner_chance", 0.46) + if near_edge { 0.20 } else { 0.0 }, 0.0, 0.92);
                if self.rng.random() < corner_chance { DriftRunMode::Corner } else { DriftRunMode::Circle }
            }
        };
        self.drift_run_mode = mode;

        let radius = 35.0f64.max(self.drift_run_radius);
        if mode == DriftRunMode::Corner {
            let inset = self.margin + radius * 0.65;
            let corners = [
                (inset, inset),
                (self.screen_w - inset, inset),
                (self.screen_w - inset, self.screen_h - inset),
                (inset, self.screen_h - inset),
            ];
            let mut ranked: Vec<(usize, (f64, f64))> = corners.iter().copied().enumerate().collect();
            ranked.sort_by(|a, b| {
                distance(x, y, a.1.0, a.1.1)
                    .partial_cmp(&distance(x, y, b.1.0, b.1.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let (idx, (cx, cy)) =
                if ranked.len() > 1 && self.rng.random() < 0.32 { *self.rng.choice(&ranked[..2]) } else { ranked[0] };
            self.drift_run_corner_index = idx;
            self.drift_run_center_x = clamp(cx, self.margin, self.screen_w - self.margin);
            self.drift_run_center_y = clamp(cy, self.margin, self.screen_h - self.margin);
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, x, y);
            self.drift_run_turn_rate *= 0.86;
        } else {
            let radial = self.rng.uniform(-PI, PI);
            let cx = x - radial.cos() * radius;
            let cy = y - radial.sin() * radius;
            let low_x = (self.screen_w * 0.5).min(self.margin + radius);
            let high_x = low_x.max(self.screen_w - self.margin - radius);
            let low_y = (self.screen_h * 0.5).min(self.margin + radius);
            let high_y = low_y.max(self.screen_h - self.margin - radius);
            self.drift_run_center_x = clamp(cx, low_x, high_x);
            self.drift_run_center_y = clamp(cy, low_y, high_y);
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, x, y);
        }
        self.drift_run_speed = rand_range(self.personality.get("drift_run_speed"), 145.0, 205.0, &mut self.rng);
        self.kin.speed = self.drift_run_speed * self.speed_mult();
        self.raise_wiggle_burst(0.45);
        bump!(self.mood, valence = 0.10, arousal = 0.32, curiosity = 0.08);
        self.reset_drift_run_cooldown();
    }

    /// `_update_drift_run(dt, mx, my)`
    pub fn update_drift_run(&mut self, dt: f64) {
        self.kin.motion_paused = false;
        self.kin.speed = self.drift_run_speed * self.speed_mult();
        self.drift_run_phase_timer = (self.drift_run_phase_timer - dt).max(0.0);
        let mut phase = self.drift_run_phase;
        if phase == DriftRunPhase::Charge && (self.drift_run_phase_timer <= 0.0 || self.kin.drift.drift_momentum > 0.54)
        {
            self.drift_run_phase = DriftRunPhase::Slide;
            self.drift_run_phase_timer =
                rand_range(self.personality.get("drift_slide_time"), 0.95, 1.65, &mut self.rng);
            self.prime_drift(1.18, Some(self.drift_run_dir));
            self.raise_wiggle_burst(0.42);
            phase = DriftRunPhase::Slide;
        } else if phase == DriftRunPhase::Slide && self.drift_run_phase_timer <= 0.0 && self.state_timer > 0.45 {
            self.drift_run_phase = DriftRunPhase::Charge;
            self.drift_run_phase_timer =
                rand_range(self.personality.get("drift_charge_time"), 0.45, 0.95, &mut self.rng);
            phase = DriftRunPhase::Charge;
        }
        let slide01 = if phase == DriftRunPhase::Slide { smootherstep(self.kin.drift.drift_momentum) } else { 0.0 };
        let turn_grip = if phase == DriftRunPhase::Slide {
            self.pf("drift_slide_turn_grip", 0.34)
        } else {
            self.pf("drift_charge_turn_grip", 0.62)
        };
        self.drift_run_angle += self.drift_run_dir * self.drift_run_turn_rate * turn_grip * dt;

        let lead_base = self.pf("drift_run_lead", 0.52);
        let lead = self.drift_run_dir * (lead_base + slide01 * self.pf("drift_slide_lead_extra", 0.82));
        let wobble = 1.0 + (self.kin.drift.drift_phase * 0.48).sin() * self.pf("drift_run_radius_wobble", 0.055);
        let radius = 70.0f64.max(self.drift_run_radius * (1.0 + slide01 * 0.18) * wobble);
        let tx = self.drift_run_center_x + (self.drift_run_angle + lead).cos() * radius;
        let ty = self.drift_run_center_y + (self.drift_run_angle + lead).sin() * radius;
        let margin = if self.drift_run_mode == DriftRunMode::Corner {
            self.margin * if phase == DriftRunPhase::Slide { 0.18 } else { 0.45 }
        } else {
            self.margin
        };
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, margin, self.screen_w, self.screen_h);
        let tangent_heading = self.drift_run_angle + self.drift_run_dir * PI * 0.5;
        self.kin.target_heading = tangent_heading - self.drift_run_dir * slide01 * self.pf("drift_countersteer", 0.42);
        self.set_focus(self.kin.target_x, self.kin.target_y, 0.55);

        if self.decision_timer <= 0.0 {
            self.decision_timer = self.rng.uniform(0.42, 0.80);
            if phase == DriftRunPhase::Charge && self.rng.random() < self.pf("drift_run_flip_chance", 0.07) {
                self.drift_run_dir *= -1.0;
                self.kin.drift.drift_dir = self.drift_run_dir;
                self.raise_wiggle_burst(0.36);
            }
        }
        if self.state_timer <= 0.0 {
            let repeat_chance = self.pf("drift_run_chain_chance", 0.18);
            if self.rng.random() < repeat_chance {
                let next_mode = if self.drift_run_mode == DriftRunMode::Circle && self.rng.random() < 0.48 {
                    Some(DriftRunMode::Corner)
                } else {
                    None
                };
                self.enter_drift_run(next_mode);
            } else {
                self.enter_idle();
            }
        }
    }

    /// `enter_spring(after, power)`: a happy hop in place.
    pub fn enter_spring(&mut self, after: After, power: f64) {
        if !self.has_skill("jump") {
            self.enter_idle();
            return;
        }
        self.enter_coil(after, power, None);
    }

    /// `enter_roll(direction)`: a playful tumble.
    pub fn enter_roll(&mut self, direction: Option<f64>) {
        if !self.phase_allowed(Phase::Roll) || !self.has_skill("roll") {
            self.enter_idle();
            return;
        }
        self.state = State::Roll;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.current_speed = 0.0;
        self.kin.vel_x = 0.0;
        self.kin.vel_y = 0.0;
        self.kin.inertia_vx = 0.0;
        self.kin.inertia_vy = 0.0;
        self.kin.inertia_timer = 0.0;
        self.social_target = None;
        self.kin.roll_dir = choice_sign(&mut self.rng);
        let turns = self.rng.randint(1, 2) as f64;
        self.kin.roll_total = TAU * turns;
        self.kin.roll_duration = self.rng.uniform(0.7, 1.15);
        self.kin.roll_progress = 0.0;
        self.kin.roll_eased = 0.0;
        self.kin.roll_spin = 0.0;
        self.kin.roll_tuck = 0.0;
        self.kin.rolling = true;
        self.squash = 1.0;
        self.kin.roll_distance = self.kin.size * self.rng.uniform(1.6, 3.4);
        let direction = match direction {
            Some(d) => d,
            None => self.rng.uniform(-PI, PI),
        };
        self.kin.roll_drift_dir = direction;
        self.state_timer = self.kin.roll_duration + 0.1;
        bump!(self.mood, valence = 0.2, arousal = 0.16, curiosity = 0.05);
    }

    /// `_resolve_pounce_outcome()`
    pub fn resolve_pounce_outcome(&mut self, pop: &Population) {
        let (mut tx, mut ty) = self.kin.catch_point;
        let target = self.social_target;
        let mut target_gone = false;
        if let Some(id) = target {
            match pop.get(id) {
                Some(t) if !t.dragging && !t.dead && self.neighbors.contains(&id) => {
                    tx = t.x;
                    ty = t.y;
                }
                _ => target_gone = true,
            }
        }
        let d = distance(self.kin.x, self.kin.y, tx, ty);
        let in_range = d < self.kin.size * 3.2 && !target_gone;
        let boldness = clamp(self.pf("boldness", 0.5), 0.0, 1.0);
        let m = self.mood;
        let mut catch_w = 0.35 + m.arousal * 0.4 + m.curiosity * 0.25 + boldness * 0.2;
        let mut cuddle_w = 0.18 + m.affection * 0.95 + m.happy() * 0.3;
        let mut flee_w = 0.14 + m.sad() * 0.6 + (1.0 - boldness) * 0.5 + m.sleepy() * 0.2;
        if !in_range {
            catch_w *= 0.6;
            cuddle_w *= 0.3;
            flee_w += 0.25;
        }
        let total = catch_w + cuddle_w + flee_w;
        let roll = self.rng.random() * total;
        if roll < catch_w {
            self.enter_catch(tx, ty, target);
        } else if roll < catch_w + cuddle_w && in_range {
            bump!(self.mood, valence = 0.2, affection = 0.12);
            self.enter_cuddle(tx, ty, target);
        } else {
            bump!(self.mood, valence = -0.12, arousal = 0.22);
            self.enter_retreat(tx, ty);
        }
    }

    /// `_find_social_target(max_range)`: the nearest eligible neighbour.
    pub fn find_social_target(&self, max_range: f64, pop: &Population) -> Option<CreatureId> {
        if !self.allow_social || self.neighbors.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_d = max_range;
        for id in &self.neighbors {
            if *id == self.id {
                continue;
            }
            let Some(other) = pop.get(*id) else { continue };
            if other.dragging || other.airborne || other.desktop_fully_hidden {
                continue;
            }
            if matches!(other.state, State::Dragged | State::Startled) {
                continue;
            }
            let d = distance(self.kin.x, self.kin.y, other.x, other.y);
            if d < best_d {
                best_d = d;
                best = Some(*id);
            }
        }
        best
    }

    /// `_startle_amount()`
    pub fn startle_amount(&self) -> f64 {
        if self.dragging {
            return 1.0;
        }
        clamp(self.startled_timer / 1.15, 0.0, 1.0)
    }

    /// `_startle_highlight_active(startle)`
    pub fn startle_highlight_active(&self, startle: f64) -> bool {
        startle > 0.35 && !self.dragging && self.startle_highlight_suppression <= 0.0
    }

    /// `_eye_startle_amount(startle)`
    pub fn eye_startle_amount(&self, startle: f64) -> f64 {
        let cap = if self.dragging { 0.28 } else { 0.45 };
        clamp(startle, 0.0, 1.0) * cap
    }

    /// `register_camouflage_touch(visible_time)`
    pub fn register_camouflage_touch(&mut self, visible_time: Option<f64>) {
        let max_strength = match self.p().get("camouflage_strength") {
            Some(v) if crate::pymath::py_truthy(v) => py_float(v).unwrap_or(0.0),
            _ => 0.0,
        };
        if max_strength <= 0.0 {
            return;
        }
        let visible_time = match visible_time {
            Some(v) => v,
            None => rand_range(self.personality.get("camouflage_touch_visible_time"), 4.5, 8.0, &mut self.rng),
        };
        let visible_time = if visible_time.is_nan() { 5.5 } else { 0.4f64.max(visible_time) };
        self.camouflage_strength = 0.0;
        self.camouflage_color = None;
        self.camouflage_idle_timer = 0.0;
        self.camouflage_visible_timer = self.camouflage_visible_timer.max(visible_time);
    }
}
