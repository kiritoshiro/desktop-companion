//! Port of `creature/kinematics/legs.py`: where a leg attaches, where its foot
//! wants to be, external displacement, and the held-leg springs.

use crate::core::{Candidates, FrameCtx, Kinematics, MotionState};
use crate::pymath::hypot;
use dc_core::PyRandom;
use dc_core::math::clamp;
use std::f64::consts::TAU;

impl Kinematics {
    /// `_visual_foot_for_render(leg)`: the visible foot location, without
    /// making planted feet slide with the body.
    pub fn visual_foot_for_render(&self, i: usize, ctx: &FrameCtx) -> (f64, f64) {
        let lively = self.uses_lively_gait(ctx);
        let leg = &self.legs[i];
        if self.rig.spider_gait.is_some() {
            if !leg.stepping && !leg.pending_step && ctx.held_release_timer <= 0.0 {
                return (leg.foot_x, leg.foot_y);
            }
            return self.limit_world_point_to_leg_reach(i, leg.foot_x, leg.foot_y, true);
        }
        if lively {
            let (mut vx, mut vy) = self.clamp_to_sector(i, leg.foot_x, leg.foot_y);
            let (ax, ay) = self.leg_attach(i);
            let d = &self.rig.legs[i];
            let cap = (d.reach_or() * 1.02).min((d.upper_len_or() + d.lower_len_or()) * 1.05) * self.size;
            let (ddx, ddy) = (vx - ax, vy - ay);
            let dl = hypot(ddx, ddy);
            if dl > cap && dl > 1e-5 {
                let scale = cap / dl;
                vx = ax + ddx * scale;
                vy = ay + ddy * scale;
            }
            return (vx, vy);
        }
        let (sx, sy) = self.constrain_leg_point(i, leg.foot_x, leg.foot_y);
        let (sx, sy) = self.limit_world_point_to_leg_reach(i, sx, sy, true);
        let a = self.leg_alignment_metrics(i, leg.foot_x, leg.foot_y);
        let r = self.leg_reach_metrics(i, leg.foot_x, leg.foot_y, true);
        let blend = if a.severe_wrong || r.very_far {
            0.88
        } else if r.too_far {
            0.58
        } else if a.wrong_side {
            0.42
        } else {
            return (leg.foot_x, leg.foot_y);
        };
        let vx = leg.foot_x + (sx - leg.foot_x) * blend;
        let vy = leg.foot_y + (sy - leg.foot_y) * blend;
        self.limit_world_point_to_leg_reach(i, vx, vy, true)
    }

    /// `_leg_attach(leg)`: the leg root in world space (with the small body sway).
    #[inline]
    pub fn leg_attach(&self, i: usize) -> (f64, f64) {
        let d = &self.rig.legs[i];
        let [fx, fy, rx, ry] = self.basis();
        let sign = d.side_sign;
        if d.has_attach() {
            let af = d.attach_forward_or() * self.size;
            let a_side = d.attach_side_or() * self.size * sign;
            let sway = self.body_sway * sign * 0.35;
            return (self.x + fx * af + rx * (a_side + sway), self.y + fy * af + ry * (a_side + sway));
        }
        let ang = self.heading + d.attach_angle.unwrap_or(0.0).to_radians();
        let radius = self.size * 0.25;
        (self.x + ang.cos() * radius, self.y + ang.sin() * radius)
    }

    /// `_leg_ideal_foot(leg)`: where the foot wants to be this frame.
    pub fn leg_ideal_foot(&self, i: usize, ctx: &FrameCtx) -> (f64, f64) {
        let d = &self.rig.legs[i];
        let [fx, fy, rx, ry] = self.basis();
        let sign = d.side_sign;
        let size = self.size;
        let lively = self.uses_lively_gait(ctx);
        if d.has_rest() {
            let mut rf = d.rest_forward_or() * size;
            let mut rs = d.rest_side_or() * size * sign;
            let mut stride = d.stride_forward.unwrap_or(0.34) * size;
            let jitter = d.rest_jitter.unwrap_or(0.0) * size;
            let speed01 = clamp(self.current_speed / 145.0, 0.0, 1.0);
            let spider_gait = self.rig.spider_gait.as_ref();
            if let Some(g) = spider_gait
                && lively
            {
                stride *= g.stride_gain * g.stride_speed_gain;
            }
            if ctx.state == MotionState::Observe && ctx.acts_as_observer && self.current_speed > 2.0 {
                let inv_speed = 1.0 / 1e-4f64.max(hypot(self.vel_x, self.vel_y));
                let move_f = (self.vel_x * fx + self.vel_y * fy) * inv_speed;
                let move_s = (self.vel_x * rx + self.vel_y * ry) * inv_speed;
                let p = &self.rig.personality;
                rf += stride * speed01 * move_f * p.observe_forward_stride_mult;
                rs += stride * speed01 * move_s * p.observe_lateral_stride_mult;
            } else {
                rf += stride * speed01 * (if !ctx.state.is_fast4() { 1.05 } else { 1.35 });
            }
            if let Some(g) = spider_gait
                && lively
            {
                let front_factor = clamp(d.rest_forward_or(), -1.0, 1.0);
                rf += stride * speed01 * front_factor * g.front_stride_bias;
            }
            let seed = self.legs[i].phase_seed;
            let micro_x = (self.breath_phase * 0.9 + seed).sin() * jitter;
            let micro_y = (self.breath_phase * 0.7 + seed * 1.13).cos() * jitter;
            let mut wx = self.x + fx * rf + rx * rs + micro_x;
            let mut wy = self.y + fy * rf + ry * rs + micro_y;
            if lively {
                (wx, wy) = self.clamp_to_sector(i, wx, wy);
            }
            return self.constrain_leg_point(i, wx, wy);
        }
        let angle = self.heading + d.rest_angle.unwrap_or(0.0).to_radians();
        let reach = d.reach_or() * size;
        let mut wx = self.x + angle.cos() * reach;
        let mut wy = self.y + angle.sin() * reach;
        if lively {
            (wx, wy) = self.clamp_to_sector(i, wx, wy);
        }
        self.constrain_leg_point(i, wx, wy)
    }

    /// `_initialize_legs()`: plant every foot near its ideal pose.
    /// Draws, per leg: two `uniform(-2, 2)` jitters and the twitch clock.
    pub fn initialize_legs(&mut self, ctx: &FrameCtx, rng: &mut PyRandom) {
        for i in 0..self.legs.len() {
            let (fx, fy) = self.leg_ideal_foot(i, ctx);
            let jx = fx + rng.uniform(-2.0, 2.0);
            let jy = fy + rng.uniform(-2.0, 2.0);
            let (px, py) = self.constrain_leg_point(i, jx, jy);
            let phase_offset = self.rig.legs[i].phase_offset.unwrap_or(0.0);
            let leg = &mut self.legs[i];
            leg.foot_x = px;
            leg.foot_y = py;
            leg.step_target_x = px;
            leg.step_target_y = py;
            leg.twitch_clock = rng.uniform(0.0, 1.0);
            leg.joint_phase = phase_offset * TAU + leg.phase_seed * 0.11;
            leg.joint_bends.clear();
            self.spider_reanchor_stance(i);
        }
    }

    /// `_translate_leg_world_points(dx, dy, factor)`: carry footfall arcs
    /// part of the way with an external body displacement.
    pub fn translate_leg_world_points(&mut self, dx: f64, dy: f64, factor: f64) {
        if dx.abs() < 1e-5 && dy.abs() < 1e-5 {
            return;
        }
        let tx = dx * clamp(factor, 0.0, 1.0);
        let ty = dy * clamp(factor, 0.0, 1.0);
        let blend = if factor >= 0.50 { 0.55 } else { 0.78 };
        for i in 0..self.legs.len() {
            let leg = &mut self.legs[i];
            leg.foot_x += tx;
            leg.foot_y += ty;
            leg.step_start_x += tx;
            leg.step_start_y += ty;
            leg.step_target_x += tx;
            leg.step_target_y += ty;
            leg.step_control_x += tx;
            leg.step_control_y += ty;
            leg.pending_target_x += tx;
            leg.pending_target_y += ty;
            self.soft_limit_leg_state(i, blend);
        }
    }

    /// `_update_held_leg_springs(dt)`: damped per-leg carry response to the
    /// hand's motion. `carry_bounce`/`carry_spring` take their defaults (see
    /// [`crate::config::LegChainConfig`]).
    pub fn update_held_leg_springs(&mut self, dt: f64, ctx: &FrameCtx) {
        let bounce = clamp(0.18, 0.04, 0.36);
        let stiffness = clamp(1.0, 0.65, 1.50);
        let size = self.size;
        let speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0);
        let response = clamp(ctx.held.drag_response, 0.0, 1.0);
        let accel_scale = 1.0f64.max(size * 18.0);
        let accel_x = clamp(ctx.held.drag_accel_x / accel_scale, -1.0, 1.0);
        let accel_y = clamp(ctx.held.drag_accel_y / accel_scale, -1.0, 1.0);
        let acceleration_level = clamp(hypot(accel_x, accel_y), 0.0, 1.0);
        let motion_level = clamp(response.max(acceleration_level * 0.62), 0.0, 1.0);
        let held_clock = ctx.held.pose_clock;

        for (index, leg) in self.legs.iter_mut().enumerate() {
            let phase = leg.phase_seed;
            let wave_phase = held_clock * (3.0 + speed01 * 1.5) + phase * 0.42 + index as f64 * 0.57;
            let wave_x = wave_phase.sin() * bounce * (0.16 + response * 0.24);
            let wave_y = (wave_phase * 0.86 + 0.7).cos() * bounce * (0.20 + response * 0.32);
            let target_x = size * (-accel_x * (0.10 + response * 0.20) + wave_x * motion_level);
            let target_y = size * (-accel_y * (0.08 + response * 0.16) + wave_y * motion_level);
            let target_x = clamp(target_x, -size * 0.30, size * 0.30);
            let target_y = clamp(target_y, -size * 0.24, size * 0.24);

            let spring_k = (38.0 + speed01 * 16.0) * stiffness;
            let damping = 5.8 + response * 1.8;
            leg.held_spring_vx += (target_x - leg.held_spring_x) * spring_k * dt;
            leg.held_spring_vy += (target_y - leg.held_spring_y) * spring_k * dt;
            leg.held_spring_vx *= (-damping * dt).exp();
            leg.held_spring_vy *= (-damping * dt).exp();
            leg.held_spring_x += leg.held_spring_vx * dt;
            leg.held_spring_y += leg.held_spring_vy * dt;
            leg.held_spring_x = clamp(leg.held_spring_x, -size * 0.30, size * 0.30);
            leg.held_spring_y = clamp(leg.held_spring_y, -size * 0.25, size * 0.25);
        }
    }

    /// `_point_segment_distance(px, py, ax, ay, bx, by)`
    pub fn point_segment_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
        let abx = bx - ax;
        let aby = by - ay;
        let denom = abx * abx + aby * aby;
        if denom <= 1e-6 {
            return hypot(px - ax, py - ay);
        }
        let t = clamp(((px - ax) * abx + (py - ay) * aby) / denom, 0.0, 1.0);
        let cx = ax + abx * t;
        let cy = ay + aby * t;
        hypot(px - cx, py - cy)
    }

    /// `_panic_rehome_legs(intensity)`: a few quick staggered corrective steps.
    pub fn panic_rehome_legs(&mut self, intensity: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let intensity = clamp(intensity, 0.0, 1.0);
        let mut candidates: Candidates<(usize, f64, f64)> = Candidates::new();
        for i in 0..self.legs.len() {
            if self.legs[i].airborne() {
                continue;
            }
            let (ix, iy) = self.leg_ideal_foot(i, ctx);
            let dist = hypot(self.legs[i].foot_x - ix, self.legs[i].foot_y - iy);
            candidates.push(dist + rng.random() * self.size * 0.12, (i, ix, iy));
        }
        candidates.sort_desc();
        let take = 2usize.max((2.0 + intensity * 3.0) as usize);
        let list = candidates;
        for (rank, &(_, (i, ix, iy))) in list.as_slice().iter().take(take).enumerate() {
            let delay = rank as f64 * rng.uniform(0.012, 0.035);
            let side = self.side_sign(i);
            let ix = ix + rng.uniform(-0.10, 0.10) * self.size;
            let iy = iy + side * rng.uniform(0.02, 0.15) * self.size * intensity;
            self.schedule_step(i, ix, iy, delay, true, ctx, rng);
            self.legs[i].step_cooldown = rng.uniform(0.012, 0.040);
        }
    }
}
