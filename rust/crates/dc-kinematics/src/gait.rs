//! Port of `creature/kinematics/gait.py`: gait style pipeline, spider gait
//! configuration access, and the grounded, support-driven body solver.

use crate::config::{MAX_LEGS, SpiderGaitConfig};
use crate::constants::{GAIT_SPELL_GAP, GAIT_SPELL_LENGTH, gait_by_state};
use crate::core::{FrameCtx, GaitStyle, Kinematics, MotionState};
use crate::legstate::ContactState;
use crate::pymath::{hypot, wrap_pi};
use dc_core::PyRandom;
use dc_core::math::{clamp, clamp_point, py_round};
use std::f64::consts::PI;

/// `substeps = max(1, int(round(duration * 120.0)))`, `step = min(1/120, duration)`
/// for `duration = max(0.0, dt)`: the fixed 120 Hz mechanics rate.
#[inline]
pub fn fixed_substeps(dt: f64) -> (usize, f64) {
    let duration = dt.max(0.0);
    let substeps = (py_round(duration * 120.0) as i64).max(1) as usize;
    let step = (1.0f64 / 120.0).min(duration);
    (substeps, step)
}

/// One support leg's proposed stroke in a [`SupportPose`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SupportRecord {
    pub leg: usize,
    pub q_f: f64,
    pub q_s: f64,
    pub stroke_f: f64,
    pub stroke_s: f64,
    pub stance_turn: f64,
    pub weight: f64,
}

/// `_spider_support_pose` result: the solved rigid body pose and the stroke
/// each supporting foot would take to get there.
#[derive(Clone, Copy, Debug)]
pub struct SupportPose {
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    len: usize,
    records: [SupportRecord; MAX_LEGS],
}

impl SupportPose {
    pub fn records(&self) -> &[SupportRecord] {
        &self.records[..self.len]
    }
}

impl Kinematics {
    /// `effective_gait_style()`: temperament, then a spell, then the activity.
    #[inline]
    pub fn effective_gait_style(&self, ctx: &FrameCtx) -> GaitStyle {
        let baseline = self.gait_style;
        if baseline == GaitStyle::Classic {
            return baseline;
        }
        if ctx.fleeing {
            return GaitStyle::Lively;
        }
        if let Some(activity) = gait_by_state(ctx.state) {
            return activity;
        }
        self.gait_spell.unwrap_or(baseline)
    }

    /// `_update_gait_spell(dt)`: roll the occasional change of step (DC-63)
    /// from the dedicated gait stream.
    pub fn update_gait_spell(&mut self, dt: f64) {
        if self.gait_style == GaitStyle::Classic {
            return;
        }
        self.gait_spell_timer -= dt.max(0.0);
        if self.gait_spell_timer > 0.0 {
            return;
        }
        if self.gait_spell.is_none() {
            let other = if self.gait_style == GaitStyle::Lively { GaitStyle::Skitter } else { GaitStyle::Lively };
            self.gait_spell = Some(other);
            self.gait_spell_timer = self.gait_rng.uniform(GAIT_SPELL_LENGTH.0, GAIT_SPELL_LENGTH.1);
        } else {
            self.gait_spell = None;
            self.gait_spell_timer = self.gait_rng.uniform(GAIT_SPELL_GAP.0, GAIT_SPELL_GAP.1);
        }
    }

    #[inline]
    pub fn uses_lively_gait(&self, ctx: &FrameCtx) -> bool {
        matches!(self.effective_gait_style(ctx), GaitStyle::Lively | GaitStyle::Skitter)
    }

    #[inline]
    pub fn uses_skitter_gait(&self, ctx: &FrameCtx) -> bool {
        self.effective_gait_style(ctx) == GaitStyle::Skitter
    }

    /// `_spider_reanchor_stance(leg)`: a fresh world-space stance.
    pub fn spider_reanchor_stance(&mut self, i: usize) {
        let (local_f, local_s) = self.world_to_body_local(self.legs[i].foot_x, self.legs[i].foot_y);
        let leg = &mut self.legs[i];
        leg.contact_state = ContactState::Stance;
        leg.support_weight = 1.0;
        leg.contact_age = 0.0;
        leg.stroke_progress = 0.0;
        leg.stance_base_f = local_f;
        leg.stance_base_s = local_s;
        leg.stance_stroke_f = 0.0;
        leg.stance_stroke_s = 0.0;
        leg.stance_turn = 0.0;
    }

    /// `_spider_reanchor_contacts()`: re-establish contacts after an
    /// externally controlled movement mode.
    pub fn spider_reanchor_contacts(&mut self) {
        self.spider_heading_filter = self.heading;
        for i in 0..self.legs.len() {
            if self.legs[i].stepping {
                self.legs[i].contact_state = ContactState::Swing;
                self.legs[i].support_weight = 0.0;
                continue;
            }
            self.legs[i].pending_step = false;
            self.legs[i].pending_delay = 0.0;
            self.spider_reanchor_stance(i);
        }
    }

    /// `_spider_leg_attach_at_pose(leg, x, y, heading)`: the rigid root, no sway.
    #[inline]
    pub fn spider_leg_attach_at_pose(&self, i: usize, x: f64, y: f64, heading: f64) -> (f64, f64) {
        let d = &self.rig.legs[i];
        let sign = d.side_sign;
        let af = d.attach_forward_or() * self.size;
        let a_side = d.attach_side_or() * self.size * sign;
        let (fx, fy) = (heading.cos(), heading.sin());
        let (rx, ry) = (-heading.sin(), heading.cos());
        (x + fx * af + rx * a_side, y + fy * af + ry * a_side)
    }

    /// `_spider_rotate_local(forward, side, angle)`
    #[inline]
    pub fn spider_rotate_local(forward: f64, side: f64, angle: f64) -> (f64, f64) {
        let (ca, sa) = (angle.cos(), angle.sin());
        (forward * ca - side * sa, forward * sa + side * ca)
    }

    /// `_spider_turn_radial_adjustment`: inside legs contract, outside legs extend.
    pub fn spider_turn_radial_adjustment(
        &self,
        i: usize,
        forward: f64,
        side: f64,
        turn_delta: f64,
        config: &SpiderGaitConfig,
    ) -> f64 {
        if turn_delta.abs() <= 1e-7 {
            return 0.0;
        }
        let radius = hypot(forward, side);
        if radius <= 1e-5 {
            return 0.0;
        }
        let side_sign = self.side_sign(i);
        let lateral_leverage = clamp(side.abs() / radius, 0.35, 1.0);
        let signed_change = -turn_delta * side_sign;
        let gain = config.turn_radial_gain;
        let limit = config.turn_radial_limit * self.size;
        let change = signed_change * self.size * gain * (0.72 + 0.28 * lateral_leverage);
        clamp(change, -limit, limit)
    }

    /// `_spider_turn_rehome_allowance`: the rotation-only part of a planted
    /// foot's target error that does not justify a swing.
    pub fn spider_turn_rehome_allowance(&self, i: usize, config: &SpiderGaitConfig) -> f64 {
        let leg = &self.legs[i];
        if leg.contact_state != ContactState::Stance || leg.stepping || leg.pending_step {
            return 0.0;
        }
        let (local_f, local_s) = self.world_to_body_local(leg.foot_x, leg.foot_y);
        let radius = hypot(local_f, local_s);
        let turn_limit = 0.05f64.max(config.support_turn_limit);
        let accumulated = clamp(leg.stance_turn.abs(), 0.0, turn_limit);
        radius * accumulated * 0.92
    }

    /// `_spider_support_pose`: fit a rigid body pose to the fixed feet after
    /// one proposed stroke. `None` when fewer than three feet support the body
    /// or any planted chain would leave its envelope.
    pub fn spider_support_pose(
        &self,
        scale: f64,
        move_f: f64,
        move_s: f64,
        turn_delta: f64,
        config: &SpiderGaitConfig,
    ) -> Option<SupportPose> {
        let mut records = [SupportRecord::default(); MAX_LEGS];
        let mut n = 0usize;
        let supports = self
            .legs
            .iter()
            .filter(|l| l.contact_state == ContactState::Stance && !l.stepping && l.support_weight > 0.01)
            .count();
        if supports < 3 {
            return None;
        }

        let turn_fraction = clamp(config.turn_pose_fraction, 0.55, 1.0);
        let effective_turn_delta = turn_delta * turn_fraction;
        let mut total_weight = 0.0;
        let (mut q_f_sum, mut q_s_sum, mut p_x_sum, mut p_y_sum) = (0.0, 0.0, 0.0, 0.0);
        for (i, leg) in self.legs.iter().enumerate() {
            if !(leg.contact_state == ContactState::Stance && !leg.stepping && leg.support_weight > 0.01) {
                continue;
            }
            let stroke_f = leg.stance_stroke_f - move_f * scale;
            let stroke_s = leg.stance_stroke_s - move_s * scale;
            let stance_turn = leg.stance_turn - effective_turn_delta * scale;
            let (base_f, base_s) = Self::spider_rotate_local(leg.stance_base_f, leg.stance_base_s, stance_turn);
            let mut q_f = base_f + stroke_f;
            let mut q_s = base_s + stroke_s;
            let radial_change =
                self.spider_turn_radial_adjustment(i, base_f, base_s, effective_turn_delta * scale, config);
            if radial_change != 0.0 {
                let base_radius = 1e-5f64.max(hypot(base_f, base_s));
                q_f += base_f / base_radius * radial_change;
                q_s += base_s / base_radius * radial_change;
            }
            let weight = 0.01f64.max(leg.support_weight);
            records[n] = SupportRecord { leg: i, q_f, q_s, stroke_f, stroke_s, stance_turn, weight };
            n += 1;
            total_weight += weight;
            q_f_sum += q_f * weight;
            q_s_sum += q_s * weight;
            p_x_sum += leg.foot_x * weight;
            p_y_sum += leg.foot_y * weight;
        }

        let q_f_c = q_f_sum / total_weight;
        let q_s_c = q_s_sum / total_weight;
        let p_x_c = p_x_sum / total_weight;
        let p_y_c = p_y_sum / total_weight;
        let (mut dot, mut cross) = (0.0, 0.0);
        for r in &records[..n] {
            let leg = &self.legs[r.leg];
            let px = leg.foot_x - p_x_c;
            let py = leg.foot_y - p_y_c;
            let qx = r.q_f - q_f_c;
            let qy = r.q_s - q_s_c;
            dot += r.weight * (qx * px + qy * py);
            cross += r.weight * (qx * py - qy * px);
        }
        let mut solved_heading = if dot.abs() + cross.abs() > 1e-7 { cross.atan2(dot) } else { self.heading };
        let solver_dt = 0.001f64.max(self.spider_solver_dt);
        let mut max_heading_step = config.max_body_turn_rate * solver_dt;
        if effective_turn_delta.abs() > 1e-7 {
            max_heading_step = max_heading_step.min(effective_turn_delta.abs());
        }
        let mut heading_step = wrap_pi(solved_heading - self.heading);
        heading_step = clamp(heading_step, -max_heading_step, max_heading_step);
        let target_error = wrap_pi(self.target_heading - self.heading);
        if target_error.abs() <= 1e-4 || heading_step * target_error <= 0.0 {
            heading_step = 0.0;
        } else {
            heading_step = heading_step.abs().min(target_error.abs()).copysign(target_error);
        }
        solved_heading = self.heading + heading_step;
        let (fx, fy) = (solved_heading.cos(), solved_heading.sin());
        let (rx, ry) = (-solved_heading.sin(), solved_heading.cos());
        let solved_x = p_x_c - (fx * q_f_c + rx * q_s_c);
        let solved_y = p_y_c - (fy * q_f_c + ry * q_s_c);

        let size = self.size;
        for r in &records[..n] {
            let i = r.leg;
            let leg = &self.legs[i];
            if hypot(r.stroke_f, r.stroke_s) > config.support_stroke_limit * size + 1e-4 {
                return None;
            }
            if r.stance_turn.abs() > config.support_turn_limit + 1e-4 {
                return None;
            }
            let (attach_x, attach_y) = self.spider_leg_attach_at_pose(i, solved_x, solved_y, solved_heading);
            let reach = hypot(leg.foot_x - attach_x, leg.foot_y - attach_y);
            if reach > self.leg_max_reach(i, false) * 0.98 {
                return None;
            }
            let lf_dx = leg.foot_x - solved_x;
            let lf_dy = leg.foot_y - solved_y;
            let local_f = lf_dx * fx + lf_dy * fy;
            let local_s = lf_dx * rx + lf_dy * ry;
            let d = &self.rig.legs[i];
            let side = d.side_sign;
            let min_side = (d.attach_side_or() * size * 0.72).max(d.rest_side_or() * size * 0.30);
            let mut side_floor = min_side;
            if r.stance_turn.abs() > 1e-4 || effective_turn_delta.abs() > 1e-4 {
                side_floor = (size * 0.05).max(min_side * 0.20);
            }
            if side * local_s < side_floor {
                return None;
            }
            if local_f.abs() > size * 2.45 {
                return None;
            }
        }
        Some(SupportPose { x: solved_x, y: solved_y, heading: solved_heading, len: n, records })
    }

    /// `_spider_grounded_mode_allowed()`: whether the stance-driven
    /// locomotion owns the body this frame.
    pub fn spider_grounded_mode_allowed(&self, ctx: &FrameCtx) -> bool {
        use MotionState::*;
        if self.rig.spider_gait.is_none() || ctx.airborne || ctx.dragging {
            return false;
        }
        if self.inertia_timer > 0.0 {
            return false;
        }
        !matches!(ctx.state, Jump | Land | Roll | DriftRun | Dragged)
    }

    /// `_spider_filtered_heading(raw_heading, dt, config)`
    pub fn spider_filtered_heading(&mut self, raw_heading: f64, dt: f64, config: &SpiderGaitConfig) -> f64 {
        let mut current = self.spider_heading_filter;
        let error = wrap_pi(raw_heading - current);
        if error.abs() <= config.heading_deadband {
            return current;
        }
        let alpha = 1.0 - (-config.heading_smoothing * 0.0f64.max(dt.min(0.10))).exp();
        current += error * alpha;
        self.spider_heading_filter = current;
        current
    }

    /// `_smooth_turn_step(target_error, dt, requested_rate)`: an
    /// acceleration-limited angular step.
    pub fn smooth_turn_step(&mut self, target_error: f64, dt: f64, requested_rate: f64) -> f64 {
        let requested_rate = clamp(requested_rate.abs(), 0.0, 12.0);
        let response = 10.0;
        let alpha = 1.0 - (-dt.max(0.0) * response).exp();
        self.turn_speed_smooth += (requested_rate - self.turn_speed_smooth) * alpha;
        self.turn_speed_smooth = clamp(self.turn_speed_smooth, 0.0, 12.0);
        target_error.abs().min(self.turn_speed_smooth * dt.max(0.0))
    }

    /// `_spider_locomotion_intent(dt)`: behaviour state as a local stroke
    /// `(forward, side)` and a turn request.
    pub fn spider_locomotion_intent(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) -> (f64, f64, f64) {
        use MotionState::*;
        if ctx.webbed_held && !ctx.dragging {
            self.current_speed = 0.0;
            self.smooth_turn_step(0.0, dt, 0.0);
            return (0.0, 0.0, 0.0);
        }
        let dx = self.target_x - self.x;
        let dy = self.target_y - self.y;
        let target_dist = hypot(dx, dy);
        let strafe = self.walks_while_facing_elsewhere(ctx);
        self.strafe_observe = strafe;
        if target_dist > 2.0 && !strafe {
            let raw_heading = dy.atan2(dx);
            self.target_heading = match self.rig.spider_gait {
                Some(cfg) => self.spider_filtered_heading(raw_heading, dt, &cfg),
                None => raw_heading,
            };
        }

        let mut desired_speed = if !self.motion_paused { self.speed } else { 0.0 };
        if target_dist < 15.0 && !ctx.state.is_fast() {
            desired_speed *= target_dist / 15.0;
        }
        let angle_error = wrap_pi(self.target_heading - self.heading);
        let (alignment_floor, gait_turn) = match self.rig.spider_gait.as_ref() {
            Some(g) => (g.turn_speed_floor, Some((g.turn_gain, g.max_body_turn_rate))),
            None => (0.15, None),
        };
        let alignment = clamp(1.0 - angle_error.abs() / (PI * 0.75), alignment_floor, 1.0);
        desired_speed *= alignment;
        desired_speed *= self.skitter_motion_factor(dt, desired_speed, target_dist, ctx, rng);
        let accel_mult = self.rig.personality.acceleration_multiplier;
        let accel = (if desired_speed > self.current_speed { 420.0 } else { 580.0 }) * accel_mult;
        if self.current_speed < desired_speed {
            self.current_speed = desired_speed.min(self.current_speed + accel * dt);
        } else {
            self.current_speed = desired_speed.max(self.current_speed - accel * dt);
        }

        let (move_x, move_y) = if strafe && target_dist > 1e-4 {
            (dx / target_dist, dy / target_dist)
        } else {
            (self.heading.cos(), self.heading.sin())
        };
        let (req_vx, req_vy) = (move_x * self.current_speed, move_y * self.current_speed);
        let [fx, fy, rx, ry] = self.basis();
        let request_f = (req_vx * fx + req_vy * fy) * dt;
        let request_s = (req_vx * rx + req_vy * ry) * dt;
        let mut turn_mult = if ctx.state.is_fast3() { 1.35 } else { 1.0 };
        if matches!(ctx.state, Alert | Approach | Chase | Observe | Retreat | Startled) {
            turn_mult *= self.rig.personality.turn_rate_multiplier;
        }
        let requested_rate = match gait_turn {
            Some((turn_gain, max_rate)) => {
                turn_mult *= turn_gain;
                (self.turn_rate * turn_mult).abs().min(max_rate)
            }
            None => (self.turn_rate * turn_mult).abs(),
        };
        let requested_turn = if angle_error.abs() > 1e-7 {
            self.smooth_turn_step(angle_error, dt, requested_rate).copysign(angle_error)
        } else {
            self.smooth_turn_step(0.0, dt, 0.0);
            0.0
        };
        (request_f, request_s, requested_turn)
    }

    /// `_update_spider_grounded_locomotion_step(dt)`: drive the body pose
    /// from the planted feet.
    pub fn update_spider_grounded_locomotion_step(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let Some(config) = self.rig.spider_gait else {
            return;
        };
        self.spider_grounded_step_with(dt, &config, ctx, rng);
    }

    fn spider_grounded_step_with(&mut self, dt: f64, config: &SpiderGaitConfig, ctx: &FrameCtx, rng: &mut PyRandom) {
        if !self.spider_locomotion_active {
            self.spider_reanchor_contacts();
            self.spider_locomotion_active = true;
        }

        self.spider_solver_dt = dt;
        let (move_f, move_s, turn_delta) = self.spider_locomotion_intent(dt, ctx, rng);
        let blend_time = config.support_blend_time;
        let stroke_norm = 1.0f64.max(config.support_stroke_limit * self.size);
        for leg in self.legs.iter_mut() {
            if leg.contact_state == ContactState::Stance && !leg.stepping {
                leg.contact_age += dt;
                leg.support_weight = 1.0f64.min(leg.support_weight.max(leg.contact_age / blend_time));
                leg.stroke_progress = clamp(hypot(leg.stance_stroke_f, leg.stance_stroke_s) / stroke_norm, 0.0, 1.0);
            }
        }

        let (old_x, old_y, old_heading) = (self.x, self.y, self.heading);
        let mut chosen = None;
        for scale in [1.0, 0.78, 0.56, 0.34, 0.16, 0.0] {
            if let Some(p) = self.spider_support_pose(scale, move_f, move_s, turn_delta, config) {
                chosen = Some(p);
                break;
            }
        }
        if let Some(pose) = chosen {
            for r in pose.records() {
                let leg = &mut self.legs[r.leg];
                leg.stance_stroke_f = r.stroke_f;
                leg.stance_stroke_s = r.stroke_s;
                leg.stance_turn = r.stance_turn;
                leg.stroke_progress = clamp(hypot(r.stroke_f, r.stroke_s) / stroke_norm, 0.0, 1.0);
            }
            self.x = pose.x;
            self.y = pose.y;
            self.heading = pose.heading;
        }
        (self.x, self.y) = clamp_point(self.x, self.y, ctx.margin * 0.4, ctx.screen_w, ctx.screen_h);
        if let Some(pf) = ctx.playfield {
            (self.x, self.y) = pf.clamp_point(self.x, self.y, 8.0f64.max(self.size * 0.5));
        }
        self.vel_x = (self.x - old_x) / dt.max(1e-4);
        self.vel_y = (self.y - old_y) / dt.max(1e-4);
        let actual_turn = wrap_pi(self.heading - old_heading);
        self.turn_rehome_pressure =
            clamp(self.turn_rehome_pressure * 0.0f64.max(1.0 - dt * 2.6) + actual_turn.abs() * 3.0, 0.0, 1.4);

        let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
        self.body_bob = 0.0;
        self.body_sway = 0.0;
        self.bob_phase += dt * (3.6 + self.current_speed * 0.095);
        self.breath_phase += dt * (1.55 + 0.35 * speed01);
        self.abdomen_pulse = self.breath_phase.sin() * 0.045 + (self.bob_phase * 0.5).sin() * speed01 * 0.022;
        self.ceph_pulse = -(self.breath_phase + 0.85).sin() * 0.018;
    }

    /// `_update_spider_grounded_locomotion(dt)`: the body solve alone, at a
    /// fixed 120 Hz substep.
    pub fn update_spider_grounded_locomotion(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let Some(config) = self.rig.spider_gait else {
            return;
        };
        let (substeps, step) = fixed_substeps(dt);
        for _ in 0..substeps {
            self.spider_grounded_step_with(step, &config, ctx, rng);
        }
    }

    /// `_update_spider_grounded_frame(dt)`: the production grounded order,
    /// contacts/body solve then gait, at a fixed 120 Hz substep. Marks the
    /// frame so the following `update_legs` does not run the gait twice.
    pub fn update_spider_grounded_frame(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let Some(config) = self.rig.spider_gait else {
            return;
        };
        let (substeps, step) = fixed_substeps(dt);
        for _ in 0..substeps {
            self.spider_grounded_step_with(step, &config, ctx, rng);
            self.update_spider_gait_step_with(step, &config, ctx, rng);
        }
        self.spider_gait_frame_updated = true;
    }
}
