//! Port of `creature/behaviour/core.py`: the basic `enter_*` states and
//! `_update_state`, the per-frame dispatch of the personality state machine.

use crate::bump;
use crate::creature::{After, Creature};
use crate::kin_ctx;
use crate::phase_scheduler::Phase;
use crate::pymath::{angle_to, choice_sign, clamp, clamp_point, distance, hypot, rand_range};
use crate::state::State;
use crate::world::{Env, SocialRole};
use std::f64::consts::PI;

impl Creature {
    /// `enter_idle()`
    pub fn enter_idle(&mut self) {
        self.state = State::Idle;
        self.kin.speed = 0.0;
        self.kin.motion_paused = false;
        self.state_timer = rand_range(self.personality.get("idle_time"), 1.0, 3.0, &mut self.rng);
    }

    /// `enter_alert(mx, my)`
    pub fn enter_alert(&mut self, mx: f64, my: f64) {
        self.state = State::Alert;
        self.kin.speed = 0.0;
        self.kin.motion_paused = false;
        self.kin.target_x = mx;
        self.kin.target_y = my;
        self.state_timer = rand_range(self.personality.get("alert_time"), 0.3, 0.9, &mut self.rng);
    }

    /// `enter_approach(mx, my)`
    pub fn enter_approach(&mut self, mx: f64, my: f64) {
        if !self.phase_allowed(Phase::Approach) || !self.has_skill("approach") {
            self.enter_alert(mx, my);
            return;
        }
        self.kin.target_x = mx;
        self.kin.target_y = my;
        self.state = State::Approach;
        self.kin.motion_paused = false;
        let reaction = self.pf("reaction_radius", 360.0);
        if self.acts_as_hunter() {
            self.kin.speed = self.hunter_approach_speed(distance(self.kin.x, self.kin.y, mx, my), reaction);
        } else {
            self.kin.speed = 52.0 * self.speed_mult();
        }
        self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3, &mut self.rng);
        self.prime_drift(0.35, None);
    }

    /// `enter_chase(mx, my)`
    pub fn enter_chase(&mut self, mx: f64, my: f64) {
        if !self.phase_allowed(Phase::Chase) || !self.has_skill("chase") {
            if self.phase_allowed(Phase::Approach) && self.has_skill("approach") {
                self.enter_approach(mx, my);
            } else {
                self.enter_alert(mx, my);
            }
            return;
        }
        self.state = State::Chase;
        self.kin.motion_paused = false;
        self.kin.target_x = mx + self.rng.uniform(-16.0, 16.0);
        self.kin.target_y = my + self.rng.uniform(-16.0, 16.0);
        let base_speed = if self.acts_as_hunter() { self.pf("hunt_chase_speed", 150.0) } else { 112.0 };
        self.kin.speed = base_speed * self.speed_mult();
        self.chase_timer = self.pf("chase_persistence", 2.5);
        if self.acts_as_hunter() {
            self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.06, 0.14, &mut self.rng);
        } else {
            self.state_timer = self.rng.uniform(0.12, 0.28);
        }
        self.prime_drift(0.85, None);
    }

    /// `enter_retreat(mx, my)`
    pub fn enter_retreat(&mut self, mx: f64, my: f64) {
        if !self.phase_allowed(Phase::RunAway) || !self.has_skill("run_away") {
            self.enter_alert(mx, my);
            return;
        }
        let angle_away = (self.kin.y - my).atan2(self.kin.x - mx);
        let distance_away = rand_range(self.personality.get("retreat_distance"), 220.0, 380.0, &mut self.rng);
        let tx = self.kin.x + angle_away.cos() * distance_away;
        let ty = self.kin.y + angle_away.sin() * distance_away;
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, self.margin, self.screen_w, self.screen_h);
        self.state = State::Retreat;
        self.kin.motion_paused = false;
        self.kin.speed = 148.0 * self.speed_mult();
        self.state_timer = rand_range(self.personality.get("retreat_time"), 0.6, 1.2, &mut self.rng);
        self.prime_drift(0.95, None);
    }

    /// `enter_startled(mx, my)`: brief startled mode after a grab or throw.
    pub fn enter_startled(&mut self, mx: f64, my: f64) {
        self.state = State::Startled;
        self.kin.motion_paused = false;
        self.startled_timer = self.startled_timer.max(1.15);
        let away = (self.kin.y - my).atan2(self.kin.x - mx);
        let throw_speed = hypot(self.kin.inertia_vx, self.kin.inertia_vy);
        let (heading, distance_out);
        if throw_speed > 60.0 {
            heading = self.kin.inertia_vy.atan2(self.kin.inertia_vx);
            distance_out = 0.0;
            self.throw_recovery = self.rng.uniform(0.34, 0.52);
        } else {
            heading = away;
            distance_out = self.rng.uniform(80.0, 160.0);
            self.throw_recovery = 0.0;
        }
        let tx = self.kin.x + heading.cos() * distance_out;
        let ty = self.kin.y + heading.sin() * distance_out;
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, self.margin, self.screen_w, self.screen_h);
        self.kin.target_heading = heading;
        self.kin.speed = 74.0 * self.speed_mult();
        self.state_timer = self.rng.uniform(0.65, 1.15);
        let throw_boost = clamp(hypot(self.kin.inertia_vx, self.kin.inertia_vy) / 580.0, 0.45, 1.45);
        self.prime_drift(throw_boost, None);
    }

    /// `enter_wander()`
    pub fn enter_wander(&mut self) {
        if !self.phase_allowed(Phase::Wander) || !self.has_skill("wander") {
            self.enter_idle();
            return;
        }
        self.state = State::Wander;
        self.kin.motion_paused = false;
        let angle = self.rng.uniform(-PI, PI);
        let dist = self.rng.uniform(120.0, 320.0);
        let tx = self.kin.x + angle.cos() * dist;
        let ty = self.kin.y + angle.sin() * dist;
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, self.margin, self.screen_w, self.screen_h);
        self.kin.speed = 38.0 * self.speed_mult();
        self.state_timer = self.rng.uniform(1.4, 3.2);
        self.prime_drift(0.25, None);
    }

    /// `self.enter_alert(mx, my) if cond else self.enter_idle()`
    #[inline]
    fn alert_or_idle(&mut self, cond: bool, mx: f64, my: f64) {
        if cond {
            self.enter_alert(mx, my);
        } else {
            self.enter_idle();
        }
    }

    /// `(target_x, target_y) = clamp_point(...)` with the creature's margin.
    #[inline]
    fn set_target_clamped(&mut self, tx: f64, ty: f64) {
        (self.kin.target_x, self.kin.target_y) = clamp_point(tx, ty, self.margin, self.screen_w, self.screen_h);
    }

    /// `_update_state(dt, mx, my)`
    pub fn update_state(&mut self, dt: f64, mx: f64, my: f64, env: &mut Env<'_>) {
        self.state_timer -= dt;
        self.decision_timer -= dt;
        if self.finish_expired_scheduled_phase() {
            return;
        }
        let dist_to_cursor = distance(self.kin.x, self.kin.y, mx, my);
        let boldness = clamp(self.pf("boldness", 0.5), 0.0, 1.0);
        let reaction = self.pf("reaction_radius", 360.0);
        let hunter = self.acts_as_hunter();
        let jumper = self.acts_as_jumper();
        let observer = self.acts_as_observer();
        let cursor_still = hunter && self.cursor_is_still_for_observe();
        let hunt_catch_distance = self.kin.size * self.pf("hunt_catch_distance_mult", 2.25);
        if self.fleeing() && self.flee_from_danger(env) {
            return;
        }
        if self.recovering && self.walk_home_to_heal(env) {
            return;
        }
        if self.update_job_state(dt) {
            return;
        }
        if self.acts_as_drifter() && self.state != State::DriftRun {
            self.drift_run_cooldown = (self.drift_run_cooldown - dt).max(0.0);
        }

        match self.state {
            State::Idle => {
                self.kin.speed = 0.0;
                if dist_to_cursor < reaction {
                    self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
                }
                if self.decision_timer <= 0.0 {
                    self.run_arbiter(mx, my, dist_to_cursor, true, env);
                }
            }
            State::Alert => {
                self.kin.speed = 0.0;
                self.kin.target_x = mx;
                self.kin.target_y = my;
                self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
                if hunter {
                    self.aim_intent = (self.aim_intent + dt * 2.2).min(1.0);
                    if cursor_still {
                        if self.state_timer <= 0.0 {
                            if self.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env) {
                                return;
                            }
                            self.state_timer =
                                rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42, &mut self.rng);
                        }
                        return;
                    }
                    if dist_to_cursor <= hunt_catch_distance * 1.5 {
                        if self.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env) {
                            return;
                        }
                        self.enter_chase(mx, my);
                        return;
                    }
                    if dist_to_cursor < reaction {
                        self.enter_approach(mx, my);
                        return;
                    }
                }
                if self.state_timer <= 0.0 {
                    self.run_arbiter(mx, my, dist_to_cursor, false, env);
                }
            }
            State::Approach => self.update_approach_state(
                dt,
                mx,
                my,
                dist_to_cursor,
                reaction,
                boldness,
                hunter,
                cursor_still,
                hunt_catch_distance,
                env,
            ),
            State::Chase => {
                self.update_chase_state(dt, mx, my, dist_to_cursor, reaction, hunter, cursor_still, hunt_catch_distance)
            }
            State::Retreat => {
                self.kin.speed = 148.0 * self.speed_mult();
                if self.state_timer <= 0.0
                    || distance(self.kin.x, self.kin.y, self.kin.target_x, self.kin.target_y) < 25.0
                {
                    self.alert_or_idle(dist_to_cursor < reaction * 0.8, mx, my);
                }
            }
            State::Startled => {
                if self.kin.inertia_timer > 0.0 {
                    self.kin.speed = 0.0;
                } else if self.throw_recovery > 0.0 {
                    self.throw_recovery = (self.throw_recovery - dt).max(0.0);
                    self.kin.speed = 0.0;
                    self.kin.motion_paused = true;
                    self.kin.target_x = self.kin.x;
                    self.kin.target_y = self.kin.y;
                } else {
                    self.kin.motion_paused = false;
                    self.kin.speed = 74.0 * self.speed_mult();
                    if dist_to_cursor < reaction * 0.85 && self.decision_timer <= 0.0 {
                        let away = (self.kin.y - my).atan2(self.kin.x - mx);
                        let tx = self.kin.x + away.cos() * self.rng.uniform(95.0, 180.0);
                        let ty = self.kin.y + away.sin() * self.rng.uniform(95.0, 180.0);
                        self.set_target_clamped(tx, ty);
                        self.kin.target_heading = away;
                        self.decision_timer = self.rng.uniform(0.25, 0.45);
                    }
                }
                if self.state_timer <= 0.0 && self.kin.inertia_timer <= 0.0 && self.throw_recovery <= 0.0 {
                    if self.has_skill("run_away") && dist_to_cursor < reaction * 0.65 {
                        self.enter_retreat(mx, my);
                    } else {
                        self.enter_idle();
                    }
                }
            }
            State::Wander => {
                self.kin.speed = if self.kin.motion_paused { 0.0 } else { 38.0 * self.speed_mult() };
                if jumper && !self.kin.motion_paused {
                    let toward = (self.kin.target_x, self.kin.target_y);
                    if self.maybe_jumper_hop(dt, After::Wander, toward) {
                        return;
                    }
                }
                if observer
                    && let Some((ax, ay, target)) = self.observer_anchor(mx, my, env.population)
                    && self.decision_timer <= 0.0
                {
                    self.enter_observe(ax, ay, target);
                    return;
                }
                if dist_to_cursor < reaction * 0.75 {
                    self.enter_alert(mx, my);
                    return;
                }
                if self.state_timer <= 0.0
                    || distance(self.kin.x, self.kin.y, self.kin.target_x, self.kin.target_y) < 22.0
                {
                    self.enter_idle();
                    return;
                }
                if self.decision_timer <= 0.0 {
                    self.decision_timer = self.rng.uniform(0.35, 0.8);
                    if self.should_start_drift_run(false) {
                        self.enter_drift_run(None);
                        return;
                    }
                    if self.rng.random() < 0.25 {
                        self.kin.motion_paused = !self.kin.motion_paused;
                    }
                }
            }
            State::DriftRun => self.update_drift_run(dt),
            State::Observe => self.update_observe(dt, mx, my, env.population),
            State::Inspect => self.update_inspect(dt, mx, my, env.population),
            State::Cuddle => self.update_cuddle(dt, mx, my, env.population),
            State::Aim => self.update_aim(dt, mx, my, env.population),
            State::Coil => {
                self.kin.speed = 0.0;
                if self.state_timer <= 0.0 {
                    let (tx, ty) = match self.coil_toward {
                        Some(t) => t,
                        None => (
                            self.kin.x + self.kin.heading.cos() * self.kin.size,
                            self.kin.y + self.kin.heading.sin() * self.kin.size,
                        ),
                    };
                    self.launch_jump(
                        tx,
                        ty,
                        crate::creature::JumpKind::Hop,
                        self.coil_after,
                        None,
                        None,
                        self.coil_power,
                    );
                }
            }
            State::Land => {
                self.kin.speed = 0.0;
                if self.state_timer <= 0.0 {
                    self.land_into(mx, my, env);
                }
            }
            State::Catch => self.update_catch(dt, env.population),
            State::Feed => {
                self.kin.motion_paused = true;
                self.kin.speed = 0.0;
                self.crouch = (self.crouch + dt * 1.5).min(1.0);
                self.kin.catch_blend = 1.0;
                self.feed_paw += dt;
                self.feed_frenzy -= dt;
                let anchor = self.feed_anchor;
                self.kin.x += (anchor.0 - self.kin.x) * clamp(dt * 14.0, 0.0, 1.0);
                self.kin.y += (anchor.1 - self.kin.y) * clamp(dt * 14.0, 0.0, 1.0);
                self.kin.target_x = self.kin.x;
                self.kin.target_y = self.kin.y;
                if self.feed_frenzy > 0.0 {
                    self.raise_wiggle_burst(0.4);
                    self.aim_intent = (self.aim_intent + dt * 4.0).min(1.0);
                }
                if self.state_timer <= 0.0 {
                    self.crouch = (self.crouch - 0.3).max(0.0);
                    self.kin.motion_paused = false;
                    self.enter_idle();
                }
            }
            State::Play => self.update_play(dt, env),
            State::Zoom => {
                if self.state_timer <= 0.0
                    || distance(self.kin.x, self.kin.y, self.kin.target_x, self.kin.target_y) < 26.0
                {
                    if self.zoom_repeats > 0 && self.rng.random() < 0.7 {
                        self.zoom_repeats -= 1;
                        if self.rng.random() < 0.3 {
                            let power = self.rng.uniform(0.7, 1.1);
                            self.enter_spring(After::Idle, power);
                        } else {
                            self.enter_zoomies();
                        }
                    } else {
                        self.enter_idle();
                    }
                }
            }
            State::Roll => self.update_roll(dt),
            State::WeaveApproach => self.update_weave_approach(env),
            State::Weave => self.update_weave(dt, env),
            State::WebApproach => self.update_web_approach(env),
            State::WebWalk => self.update_web_walk(dt, env),
            State::WebAim => self.update_web_aim(dt, mx, my, env),
            State::WebShot => self.update_web_shot(dt, mx, my),
            State::RepairApproach => self.update_repair_approach(env),
            State::Repair => self.update_repair(dt, env),
            _ => {}
        }
    }

    /// `_update_roll(dt, mx, my)`, with the side effects the kinematics
    /// reports back (`squash = 1.0`, `enter_idle()`).
    pub fn update_roll(&mut self, dt: f64) {
        let finished = kin_ctx!(self, |kin, ctx, _rng| kin.update_roll(dt, &ctx));
        if finished {
            self.squash = 1.0;
            self.enter_idle();
        }
    }

    /// The `Land` state's hand-over once the landing beat has run.
    fn land_into(&mut self, mx: f64, my: f64, env: &mut Env<'_>) {
        match self.land_after {
            After::Strike => self.enter_idle(),
            After::Outcome => self.resolve_pounce_outcome(env.population),
            After::Nope => {
                if self.nope_repeats > 0 && self.has_skill("jump") && self.has_skill("run_away") {
                    self.enter_nope_escape(mx, my, true);
                } else if self.has_skill("run_away") {
                    self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28, &mut self.rng);
                    self.enter_retreat(mx, my);
                } else {
                    self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28, &mut self.rng);
                    self.enter_idle();
                }
            }
            After::Play
                if self.social_target.is_some_and(|id| env.population.get(id).is_some_and(|t| !t.dragging)) =>
            {
                let target = self.social_target.unwrap();
                let role = match self.social_role {
                    SocialRole::Chase | SocialRole::Flee => Some(self.social_role),
                    _ => None,
                };
                self.enter_play(target, role, env.population);
            }
            After::Wander => self.enter_wander(),
            After::Approach => self.enter_approach(mx, my),
            After::Chase => self.enter_chase(mx, my),
            _ => self.enter_idle(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_approach_state(
        &mut self,
        dt: f64,
        mx: f64,
        my: f64,
        dist_to_cursor: f64,
        reaction: f64,
        boldness: f64,
        hunter: bool,
        cursor_still: bool,
        hunt_catch_distance: f64,
        env: &mut Env<'_>,
    ) {
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
        if hunter {
            self.kin.target_x = mx;
            self.kin.target_y = my;
            self.aim_intent = (self.aim_intent + dt * 1.8).min(1.0);
            if cursor_still {
                self.kin.motion_paused = true;
                self.kin.speed = 0.0;
                if self.state_timer <= 0.0 {
                    if self.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env) {
                        return;
                    }
                    self.state_timer =
                        rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42, &mut self.rng);
                }
                return;
            }
            if self.kin.motion_paused {
                self.kin.motion_paused = false;
                self.state_timer = self.state_timer.min(0.08);
            }
            if dist_to_cursor <= hunt_catch_distance * 1.2 {
                if self.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env) {
                    return;
                }
                self.enter_chase(mx, my);
                return;
            }
            if dist_to_cursor > reaction * 1.65 {
                self.alert_or_idle(dist_to_cursor < reaction * 2.0, mx, my);
                return;
            }
            self.kin.speed = self.hunter_approach_speed(dist_to_cursor, reaction);
            if self.state_timer <= 0.0 {
                if self.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env) {
                    return;
                }
                self.kin.motion_paused = true;
                self.kin.speed = 0.0;
                self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.08, 0.20, &mut self.rng);
            }
            return;
        }
        if self.has_skill("chase") && dist_to_cursor < 78.0 && self.rng.random() < 0.025 + boldness * 0.035 {
            self.enter_chase(mx, my);
            return;
        }
        if dist_to_cursor > reaction * 1.35 {
            self.enter_alert(mx, my);
            return;
        }
        if self.kin.motion_paused {
            self.kin.speed = 0.0;
            self.kin.target_x = mx;
            self.kin.target_y = my;
        } else {
            self.kin.speed = 52.0 * self.speed_mult();
            if self.maybe_jumper_hop(dt, After::Approach, (mx, my)) {
                return;
            }
        }
        if self.state_timer <= 0.0 {
            if self.kin.motion_paused {
                self.kin.motion_paused = false;
                self.kin.target_x = mx;
                self.kin.target_y = my;
                self.kin.speed = 52.0 * self.speed_mult();
                self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3, &mut self.rng);
            } else {
                self.kin.motion_paused = true;
                self.kin.speed = 0.0;
                self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.25, 0.7, &mut self.rng);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_chase_state(
        &mut self,
        dt: f64,
        mx: f64,
        my: f64,
        dist_to_cursor: f64,
        reaction: f64,
        hunter: bool,
        cursor_still: bool,
        hunt_catch_distance: f64,
    ) {
        self.chase_timer -= dt;
        if hunter {
            self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
            self.aim_intent = (self.aim_intent + dt * 2.6).min(1.0);
            if self.hunting_prey {
                self.kin.target_x = mx;
                self.kin.target_y = my;
                if cursor_still {
                    self.kin.motion_paused = true;
                    self.kin.speed = 0.0;
                    self.kin.current_speed = self.kin.current_speed.min(8.0);
                } else {
                    self.kin.motion_paused = false;
                    self.kin.speed = self.hunter_approach_speed(dist_to_cursor, reaction);
                }
                self.chase_timer = self.chase_timer.max(1.0);
                return;
            }
            if cursor_still {
                self.kin.motion_paused = true;
                self.kin.speed = 0.0;
                self.kin.target_x = mx;
                self.kin.target_y = my;
                if self.chase_timer <= 0.0 && dist_to_cursor > reaction * 1.2 {
                    self.enter_idle();
                }
                return;
            }
            self.kin.motion_paused = false;
            let base_speed = self.pf("hunt_chase_speed", 150.0) * self.speed_mult();
            if dist_to_cursor <= hunt_catch_distance && self.decision_timer <= 0.0 {
                bump!(self.mood, valence = 0.08, arousal = 0.18, curiosity = 0.08);
                self.raise_wiggle_burst(0.75);
                self.kin.catch_blend = 0.0;
                let sign = choice_sign(&mut self.rng);
                let spin = angle_to(mx, my, self.kin.x, self.kin.y) + sign * self.rng.uniform(1.35, 2.75);
                let orbit = self.rng.uniform(self.kin.size * 1.2, self.kin.size * 2.8);
                self.set_target_clamped(mx + spin.cos() * orbit, my + spin.sin() * orbit);
                self.kin.speed = base_speed * 1.18;
                self.chase_timer = self.chase_timer.max(self.pf("chase_persistence", 2.5));
                self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12, &mut self.rng);
                self.decision_timer = self.rng.uniform(0.10, 0.22);
                return;
            }
            if self.state_timer <= 0.0 {
                let lead = clamp(self.cursor_speed() / 80.0, 0.0, 18.0);
                let tx = mx + self.prev_cursor_vx * 0.035 + self.rng.uniform(-10.0 - lead, 10.0 + lead);
                let ty = my + self.prev_cursor_vy * 0.035 + self.rng.uniform(-10.0 - lead, 10.0 + lead);
                self.set_target_clamped(tx, ty);
                self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12, &mut self.rng);
            }
            self.kin.speed = base_speed;
            if self.chase_timer <= 0.0 || dist_to_cursor > reaction * 1.75 {
                self.alert_or_idle(dist_to_cursor < reaction, mx, my);
            }
            return;
        }
        if self.state_timer <= 0.0 {
            let tx = mx + self.rng.uniform(-18.0, 18.0);
            let ty = my + self.rng.uniform(-18.0, 18.0);
            self.set_target_clamped(tx, ty);
            self.state_timer = self.rng.uniform(0.10, 0.24);
        }
        self.kin.speed = 112.0 * self.speed_mult();
        if self.maybe_jumper_hop(dt, After::Chase, (mx, my)) {
            return;
        }
        if self.hunting_prey {
            self.kin.target_x = mx;
            self.kin.target_y = my;
            self.chase_timer = self.chase_timer.max(0.6);
        } else if self.chase_timer <= 0.0 || dist_to_cursor > reaction * 1.45 {
            self.alert_or_idle(dist_to_cursor < reaction, mx, my);
        }
    }
}
