//! Port of `creature/kinematics/movement.py`: moving the body itself (the
//! legacy, non-grounded path and a throw's inertia), and the skitter rhythm.

use crate::core::{FrameCtx, Kinematics, MotionState};
use crate::pymath::{hypot, wrap_pi};
use dc_core::PyRandom;
use dc_core::math::{angle_lerp, clamp, clamp_point};
use std::f64::consts::{PI, TAU};

impl Kinematics {
    /// `_skitter_motion_factor(dt, desired_speed, target_dist)`: the
    /// stop-go speed multiplier for the Skitter style.
    pub fn skitter_motion_factor(
        &mut self,
        dt: f64,
        desired_speed: f64,
        target_dist: f64,
        ctx: &FrameCtx,
        rng: &mut PyRandom,
    ) -> f64 {
        if !self.uses_skitter_gait(ctx) {
            return 1.0;
        }
        if desired_speed <= 5.0 || target_dist < 10.0f64.max(self.size * 0.70) {
            self.skitter_pause_timer = 0.0;
            self.skitter_burst_timer = self.skitter_burst_timer.min(rng.uniform(0.12, 0.24));
            return 1.0;
        }
        if ctx.state == MotionState::DriftRun && self.is_drift_sliding(ctx) {
            return 1.0;
        }

        let speed01 = clamp(desired_speed / 160.0, 0.0, 1.0);
        self.skitter_phase = dc_core::math::py_mod(self.skitter_phase + dt * (18.0 + speed01 * 16.0), TAU);

        if self.skitter_pause_timer > 0.0 {
            self.skitter_pause_timer = (self.skitter_pause_timer - dt).max(0.0);
            return 0.0;
        }

        self.skitter_burst_timer -= dt;
        if self.skitter_burst_timer <= 0.0 {
            let fast_state = ctx.state.is_fast3();
            self.skitter_pause_timer = if fast_state { rng.uniform(0.055, 0.135) } else { rng.uniform(0.085, 0.22) };
            self.skitter_burst_timer = if fast_state { rng.uniform(0.10, 0.24) } else { rng.uniform(0.16, 0.36) };
            self.skitter_burst_jitter = rng.uniform(0.94, 1.20);
            return 0.0;
        }

        let pulse = 0.90 + 0.16 * 0.0f64.max(self.skitter_phase.sin());
        let state_boost = if ctx.state.is_fast3() { 1.34 } else { 1.18 };
        state_boost * self.skitter_burst_jitter * pulse
    }

    /// `_aim_at_a_real_screen()`: pull this frame's destination onto a
    /// monitor before any step (DC-88).
    pub fn aim_at_a_real_screen(&mut self, ctx: &FrameCtx) {
        if let Some(pf) = ctx.playfield {
            (self.target_x, self.target_y) = pf.clamp_point(self.target_x, self.target_y, ctx.margin);
        }
    }

    /// `_startle_amount()`
    #[inline]
    pub fn startle_amount(ctx: &FrameCtx) -> f64 {
        if ctx.dragging {
            return 1.0;
        }
        clamp(ctx.startled_timer / 1.15, 0.0, 1.0)
    }

    /// `_move_body(dt)`: the legacy body path (non-grounded states and
    /// models without a spider gait), including a throw's slide.
    pub fn move_body(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let margin = ctx.margin;
        let (screen_w, screen_h) = (ctx.screen_w, ctx.screen_h);
        let mut edge_push_x = 0.0;
        let mut edge_push_y = 0.0;
        let edge_zone = margin + 30.0;
        if self.x < edge_zone {
            edge_push_x += (edge_zone - self.x) / edge_zone;
        }
        if self.x > screen_w - edge_zone {
            edge_push_x -= (self.x - (screen_w - edge_zone)) / edge_zone;
        }
        if self.y < edge_zone {
            edge_push_y += (edge_zone - self.y) / edge_zone;
        }
        if self.y > screen_h - edge_zone {
            edge_push_y -= (self.y - (screen_h - edge_zone)) / edge_zone;
        }
        if edge_push_x != 0.0 || edge_push_y != 0.0 {
            self.target_x = clamp(self.target_x + edge_push_x * 140.0 * dt, margin, screen_w - margin);
            self.target_y = clamp(self.target_y + edge_push_y * 140.0 * dt, margin, screen_h - margin);
        }

        if self.inertia_timer > 0.0 {
            self.move_body_inertia(dt, ctx, rng);
            return;
        }

        if ctx.webbed_held && !ctx.dragging {
            self.current_speed = 0.0;
            self.vel_x = 0.0;
            self.vel_y = 0.0;
            return;
        }
        let dx = self.target_x - self.x;
        let dy = self.target_y - self.y;
        let target_dist = hypot(dx, dy);
        let move_heading = if target_dist > 2.0 { dy.atan2(dx) } else { self.heading };
        let strafe_observe = self.walks_while_facing_elsewhere(ctx);
        self.strafe_observe = strafe_observe;

        if target_dist > 2.0 && !strafe_observe {
            if self.legs.len() >= 8 {
                if let Some(cfg) = self.rig.spider_gait {
                    self.target_heading = self.spider_filtered_heading(move_heading, dt, &cfg);
                } else {
                    let mut filtered = self.spider_heading_filter;
                    let bearing_error = wrap_pi(move_heading - filtered);
                    if bearing_error.abs() > 0.05 {
                        filtered += bearing_error * (1.0 - (-dt * 10.0).exp());
                    }
                    self.spider_heading_filter = filtered;
                    self.target_heading = filtered;
                }
            } else {
                self.target_heading = move_heading;
            }
        }

        let p = &self.rig.personality;
        let sliding = self.is_drift_sliding(ctx);
        let mut turn_mult;
        if ctx.state == MotionState::DriftRun {
            turn_mult = if sliding { p.drift_slide_body_turn } else { p.drift_charge_body_turn };
        } else {
            turn_mult = if ctx.state.is_fast4() { 1.35 } else { 1.0 };
            if matches!(
                ctx.state,
                MotionState::Alert | MotionState::Approach | MotionState::Chase | MotionState::Observe
            ) {
                turn_mult *= p.turn_rate_multiplier;
            }
        }
        let mut max_turn_rate = (self.turn_rate * turn_mult).abs();
        if self.uses_lively_gait(ctx) || self.legs.len() >= 8 {
            max_turn_rate = max_turn_rate.min(7.0);
        }
        let angle_error = wrap_pi(self.target_heading - self.heading);
        let max_turn =
            self.smooth_turn_step(angle_error, dt, if angle_error.abs() > 1e-7 { max_turn_rate } else { 0.0 });
        let old_heading = self.heading;
        self.heading = angle_lerp(self.heading, self.target_heading, max_turn);
        let turn_delta = wrap_pi(self.heading - old_heading);
        self.turn_rehome_pressure =
            clamp(self.turn_rehome_pressure * 0.0f64.max(1.0 - dt * 2.6) + turn_delta.abs() * 3.0, 0.0, 1.4);

        let mut desired_speed = if !self.motion_paused { self.speed } else { 0.0 };
        if target_dist < 15.0 && !ctx.state.is_fast() {
            desired_speed *= target_dist / 15.0;
        }
        let p = &self.rig.personality;
        let alignment = if strafe_observe {
            p.observe_strafe_alignment
        } else {
            let angle_error = wrap_pi(self.target_heading - self.heading).abs();
            if ctx.state == MotionState::DriftRun {
                clamp(1.0 - angle_error / PI, 0.72, 1.0)
            } else {
                clamp(1.0 - angle_error / (PI * 0.75), 0.15, 1.0)
            }
        };
        desired_speed *= clamp(alignment, 0.15, 1.0);
        let skitter_factor = self.skitter_motion_factor(dt, desired_speed, target_dist, ctx, rng);
        desired_speed *= skitter_factor;
        let p = &self.rig.personality;
        let mut accel_mult = p.acceleration_multiplier;
        if self.uses_skitter_gait(ctx) {
            accel_mult *= if desired_speed > self.current_speed { 1.55 } else { 2.35 };
        }
        if ctx.state == MotionState::DriftRun && self.is_drift_sliding(ctx) {
            accel_mult *= p.drift_slide_accel_grip;
        }
        let accel = (if desired_speed > self.current_speed { 420.0 } else { 580.0 }) * accel_mult;
        if self.current_speed < desired_speed {
            self.current_speed = desired_speed.min(self.current_speed + accel * dt);
        } else {
            self.current_speed = desired_speed.max(self.current_speed - accel * dt);
        }

        let mv = target_dist.min(self.current_speed * dt);
        let drift_amount = if mv > 0.0 { self.drift_amount(dt, self.current_speed, false, ctx, rng) } else { 0.0 };
        if mv > 0.0 {
            let (mut move_x, mut move_y) = if strafe_observe && target_dist > 1e-4 {
                (dx / target_dist, dy / target_dist)
            } else {
                (self.heading.cos(), self.heading.sin())
            };
            if drift_amount != 0.0 {
                let side_x = -move_y;
                let side_y = move_x;
                let blended_x = move_x + side_x * drift_amount;
                let blended_y = move_y + side_y * drift_amount;
                let mag = 1e-5f64.max(hypot(blended_x, blended_y));
                move_x = blended_x / mag;
                move_y = blended_y / mag;
            }
            if ctx.state == MotionState::DriftRun && self.is_drift_sliding(ctx) {
                let prev_speed = hypot(self.vel_x, self.vel_y);
                if prev_speed > 2.0 {
                    let old_x = self.vel_x / prev_speed;
                    let old_y = self.vel_y / prev_speed;
                    let keep = clamp(self.rig.personality.drift_inertia_keep + drift_amount.abs() * 0.14, 0.0, 0.92);
                    move_x = old_x * keep + move_x * (1.0 - keep);
                    move_y = old_y * keep + move_y * (1.0 - keep);
                    let mag = 1e-5f64.max(hypot(move_x, move_y));
                    move_x /= mag;
                    move_y /= mag;
                }
            }
            (move_x, move_y) = self.guard_drifter_travel_direction(move_x, move_y, ctx);
            self.vel_x = move_x * self.current_speed;
            self.vel_y = move_y * self.current_speed;
            self.x += move_x * mv;
            self.y += move_y * mv;
        } else {
            self.vel_x *= 0.0f64.max(1.0 - dt * 8.0);
            self.vel_y *= 0.0f64.max(1.0 - dt * 8.0);
        }
        (self.x, self.y) = clamp_point(self.x, self.y, margin * 0.4, screen_w, screen_h);
        if let Some(pf) = ctx.playfield {
            (self.x, self.y) = pf.clamp_point(self.x, self.y, 8.0f64.max(self.size * 0.5));
        }

        let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
        let skitter = self.uses_skitter_gait(ctx);
        self.bob_phase +=
            dt * ((if skitter { 5.2 } else { 3.6 }) + self.current_speed * (if skitter { 0.135 } else { 0.095 }));
        self.breath_phase += dt * ((if skitter { 1.85 } else { 1.55 }) + 0.35 * speed01);
        self.body_bob = self.bob_phase.sin() * speed01 * (if skitter { 1.20 } else { 1.55 });
        self.body_sway = (self.bob_phase * (if skitter { 0.78 } else { 0.5 })).sin()
            * speed01
            * self.size
            * (if skitter { 0.022 } else { 0.030 })
            + turn_delta * self.size * 0.55
            + drift_amount * self.size * 0.26
            + self.drift.drift_lean * self.size * 0.62;
        self.abdomen_pulse = self.breath_phase.sin() * 0.045 + (self.bob_phase * 0.5).sin() * speed01 * 0.022;
        self.ceph_pulse = -(self.breath_phase + 0.85).sin() * 0.018;
    }

    /// The `inertia_timer > 0` branch of `_move_body`: a thrown spider slides.
    fn move_body_inertia(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let margin = ctx.margin;
        let (screen_w, screen_h) = (ctx.screen_w, ctx.screen_h);
        self.inertia_timer = (self.inertia_timer - dt).max(0.0);
        let throw_speed = hypot(self.inertia_vx, self.inertia_vy);
        let drift_amount = if throw_speed > 2.0 { self.drift_amount(dt, throw_speed, true, ctx, rng) } else { 0.0 };
        if throw_speed > 2.0 {
            self.target_heading = self.inertia_vy.atan2(self.inertia_vx);
            if drift_amount != 0.0 {
                self.target_heading -= drift_amount * self.rig.personality.drift_countersteer;
            }
        }
        let old_heading = self.heading;
        self.heading = angle_lerp(self.heading, self.target_heading, self.turn_rate * 1.9 * dt);
        let turn_delta = wrap_pi(self.heading - old_heading);
        let (old_x, old_y) = (self.x, self.y);
        let (mut drift_vx, mut drift_vy) = (0.0, 0.0);
        if drift_amount != 0.0 && throw_speed > 2.0 {
            let side_x = -self.inertia_vy / throw_speed;
            let side_y = self.inertia_vx / throw_speed;
            drift_vx = side_x * throw_speed * drift_amount * 0.60;
            drift_vy = side_y * throw_speed * drift_amount * 0.60;
        }
        self.x += (self.inertia_vx + drift_vx) * dt;
        self.y += (self.inertia_vy + drift_vy) * dt;
        let (mut cx, mut cy) = clamp_point(self.x, self.y, margin * 0.25, screen_w, screen_h);
        if let Some(pf) = ctx.playfield {
            (cx, cy) = pf.clamp_point(cx, cy, margin * 0.25);
        }
        if cx != self.x {
            self.inertia_vx *= -0.28;
        }
        if cy != self.y {
            self.inertia_vy *= -0.28;
        }
        self.x = cx;
        self.y = cy;
        self.target_x = clamp(self.x + self.inertia_vx * 0.22, margin, screen_w - margin);
        self.target_y = clamp(self.y + self.inertia_vy * 0.22, margin, screen_h - margin);
        self.translate_leg_world_points(self.x - old_x, self.y - old_y, 0.22);

        let decay = 0.0f64.max(1.0 - dt * (2.15 + clamp(throw_speed / 900.0, 0.0, 1.0) * 1.25));
        self.inertia_vx *= decay;
        self.inertia_vy *= decay;
        if throw_speed < 24.0 {
            self.inertia_timer = 0.0;
            self.inertia_vx = 0.0;
            self.inertia_vy = 0.0;
        }

        self.vel_x = self.inertia_vx + drift_vx;
        self.vel_y = self.inertia_vy + drift_vy;
        self.current_speed = clamp(hypot(self.vel_x, self.vel_y), 0.0, 260.0);
        let speed01 = clamp(self.current_speed / 220.0, 0.0, 1.0);
        let startle = Self::startle_amount(ctx);
        self.bob_phase += dt * (5.4 + self.current_speed * 0.055);
        self.breath_phase += dt * (2.2 + 1.2 * startle + 0.35 * speed01);
        self.body_bob = self.bob_phase.sin() * (0.6 + speed01 * 1.9);
        self.body_sway = (self.bob_phase * 0.9).sin() * self.size * (0.018 + startle * 0.020)
            + turn_delta * self.size * 0.55
            + drift_amount * self.size * 0.44;
        self.abdomen_pulse = (self.breath_phase * (1.0 + startle * 0.6)).sin() * (0.045 + 0.035 * startle);
        self.ceph_pulse = -(self.breath_phase + 0.85).sin() * (0.018 + 0.022 * startle);
    }
}
