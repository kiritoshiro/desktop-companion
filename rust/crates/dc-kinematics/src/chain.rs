//! Leg geometry from `creature/render_procedural.py`: the parts that are pure
//! kinematics rather than painting.
//!
//! * `_update_spider_joint_articulation` (called by the gait every substep),
//! * `_sprite_leg_chain_points` (the bounded four/five-segment chain solve),
//! * `_solve_knee` (the two-segment knee of unchained models),
//! * `_safe_sprite_leg_foot`, `_leg_lift_px`, `_leg_attach_drawn`,
//! * the rendered-foot poses `_leg_draw_points`, `_picked_up_leg_pose`,
//!   `_front_leg_feeler_pose` and `_combat_leg_pose`.
//!
//! The painter (`dc-paint`) calls these and draws what they return. The
//! Python memoised `_sprite_leg_chain_points` per drawn frame; the solve is a
//! pure function of its inputs, so the cache is left to the caller.

use crate::config::{LegChainConfig, MAX_CHAIN_POINTS};
use crate::core::{FrameCtx, Kinematics, MotionState};
use crate::pymath::{hypot, py_sum};
use dc_core::math::clamp;
use std::f64::consts::{PI, TAU};

/// The solved chain: root, joints, foot (`segment_count + 1` points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainPoints {
    len: usize,
    points: [(f64, f64); MAX_CHAIN_POINTS],
}

impl ChainPoints {
    #[inline]
    pub fn as_slice(&self) -> &[(f64, f64)] {
        &self.points[..self.len]
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Render-only inputs for [`Kinematics::leg_draw_points`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawCtx {
    /// `combat_body_offset()`: how far the drawn body is shifted by a bite.
    pub combat_body_offset: (f64, f64),
    /// `combat_stance` (0..1).
    pub combat_stance: f64,
    /// `strike_phase()` as `(push, lift)`.
    pub strike_push: f64,
    pub strike_lift: f64,
    /// `strike_face` unit vector.
    pub strike_face: (f64, f64),
    pub combat_face_x: f64,
    pub combat_face_y: f64,
    /// `_feed_paw` clock (used in the Feed state).
    pub feed_paw: f64,
    pub jump_z: f64,
    pub jump_peak: f64,
}

impl Kinematics {
    /// `_update_spider_joint_articulation(dt, config)`: each interior joint's
    /// own smooth flex rhythm. A no-op without a leg chain.
    pub fn update_spider_joint_articulation(&mut self, dt: f64) {
        let Some(chain) = self.rig.leg_chain.as_ref() else {
            return;
        };
        let joint_count = chain.joint_count();
        let mut profile = [0.0; 4];
        let mut offsets = [0.0; 4];
        profile[..joint_count].copy_from_slice(&chain.bend_profile[..joint_count]);
        offsets[..joint_count].copy_from_slice(&chain.joint_phase_offsets[..joint_count]);
        let blend = 1.0 - (-dt * 12.0).exp();
        let gait_phase = self.lively_gait_phase;
        for leg in self.legs.iter_mut() {
            if leg.joint_bends.len() != joint_count {
                leg.joint_bends.fill_with(joint_count, |index| 0.88 + 0.08 * index as f64);
            }
            let swing = if leg.stepping {
                let step_t = clamp(leg.step_timer / 0.001f64.max(leg.step_duration), 0.0, 1.0);
                (PI * step_t).sin()
            } else {
                0.0
            };
            let gait_angle = gait_phase * TAU + leg.joint_phase;
            let stepping = leg.stepping;
            let bends = leg.joint_bends.as_mut_slice();
            for index in 0..joint_count {
                let joint_angle = gait_angle + offsets[index];
                let fi = index as f64;
                let joint_wave =
                    (joint_angle * (1.0 + fi * 0.13)).sin() * 0.72 + (joint_angle * 1.37 + fi * 0.81).sin() * 0.28;
                let mut target = 0.86 + 0.11 * joint_wave + 0.08 * profile[index];
                if stepping {
                    let mid = if index == joint_count / 2 { 1.0 } else { 0.0 };
                    target += swing * (0.16 + 0.07 * mid);
                }
                let target = clamp(target, 0.55, 1.28);
                bends[index] += (target - bends[index]) * blend;
            }
        }
    }

    /// `_leg_attach_drawn(leg)`: the root as drawn, moved with the combat
    /// body offset.
    #[inline]
    pub fn leg_attach_drawn(&self, i: usize, draw: &DrawCtx) -> (f64, f64) {
        let (ax, ay) = self.leg_attach(i);
        (ax + draw.combat_body_offset.0, ay + draw.combat_body_offset.1)
    }

    /// `_safe_sprite_leg_foot(leg, x, y, chain_config)`: keep rendered sprite
    /// feet inside the reach envelope.
    pub fn safe_sprite_leg_foot(
        &self,
        i: usize,
        x: f64,
        y: f64,
        use_chain: bool,
        ctx: &FrameCtx,
        draw: &DrawCtx,
    ) -> (f64, f64) {
        let leg = &self.legs[i];
        if self.rig.spider_gait.is_some() && !leg.stepping && !leg.pending_step && ctx.held_release_timer <= 0.0 {
            return (x, y);
        }
        let (mut sx, mut sy) = self.limit_world_point_to_leg_reach(i, x, y, true);
        let chain = match (use_chain, self.rig.leg_chain.as_ref()) {
            (true, Some(c)) => c,
            _ => return (sx, sy),
        };
        let d = &self.rig.legs[i];
        let segment_reach = (d.upper_len_or() + d.lower_len_or()) * self.size;
        let max_reach = self.leg_max_reach(i, true).min(segment_reach * chain.max_stretch);
        let (ax, ay) = self.leg_attach_drawn(i, draw);
        let (dx, dy) = (sx - ax, sy - ay);
        let distance = hypot(dx, dy);
        if distance > max_reach && distance > 1e-5 {
            let scale = max_reach / distance;
            sx = ax + dx * scale;
            sy = ay + dy * scale;
        }
        (sx, sy)
    }

    /// `_sprite_leg_chain_points(leg, ax, ay, fx, fy, chain_config)` with the
    /// rig's own chain. `None` for a model without `leg_chain`.
    pub fn sprite_leg_chain_points(
        &self,
        i: usize,
        ax: f64,
        ay: f64,
        fx: f64,
        fy: f64,
        ctx: &FrameCtx,
    ) -> Option<ChainPoints> {
        let chain = self.rig.leg_chain.as_ref()?;
        Some(self.chain_points_with(i, ax, ay, fx, fy, chain, ctx))
    }

    /// The chain solve against an explicit chain configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn chain_points_with(
        &self,
        i: usize,
        ax: f64,
        ay: f64,
        fx: f64,
        fy: f64,
        chain: &LegChainConfig,
        ctx: &FrameCtx,
    ) -> ChainPoints {
        let leg = &self.legs[i];
        let d = &self.rig.legs[i];
        let size = self.size;
        let (a_f, a_s) = self.world_to_body_local(ax, ay);
        let (f_f, f_s) = self.world_to_body_local(fx, fy);
        let direct = 1e-4f64.max(hypot(f_f - a_f, f_s - a_s));
        let leg_curve = chain.leg_curve;
        let seg_scales: &[f64] = if leg_curve > 0.0 { &chain.segment_lengths } else { &chain.segment_scales };
        let total = 1e-4f64.max(py_sum(seg_scales.iter().copied()));
        let n_frac = seg_scales.len() - 1;
        let mut fractions = [0.0; 4];
        let mut running = 0.0;
        for (k, scale) in seg_scales[..n_frac].iter().enumerate() {
            running += scale / total;
            fractions[k] = running;
        }

        let side = d.side_sign;
        let front = self.leg_front_factor(i);
        let mut bend_base = size * chain.bend * (0.78 + 0.34 * leg.lift);
        let mut forward_bias = size * 0.075 * front * (0.72 + 0.28 * leg.lift);
        let profiles = &chain.bend_profile;
        let directions = &chain.bend_directions;
        let mut catch_strength = 0.0;
        if !ctx.dragging && self.catch_blend > 0.001 && front > 0.15 {
            catch_strength = clamp(self.catch_blend * (front - 0.15) / 0.85, 0.0, 1.0);
        }
        const CATCH_BEND_PROFILE: [f64; 4] = [0.55, 1.00, 0.72, 0.34];
        let (held_response, carry_wave_amount, carry_wave_phase) = if ctx.dragging {
            let held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0);
            let held_response = clamp(ctx.held.drag_response, 0.0, 1.0);
            bend_base *= 3.40 + held_response * 0.90 + held_speed01 * 0.45;
            forward_bias *= 0.35;
            // `chain_config.get("carry_joint_wave", 0.42)` on the sanitised
            // config, which never has the key.
            let carry_wave_amount = clamp(0.42, 0.0, 0.65);
            let mut carry_wave_phase = ctx.held.pose_clock * 3.0;
            carry_wave_phase += leg.phase_seed * 0.38;
            (held_response, carry_wave_amount, carry_wave_phase)
        } else {
            (0.0, 0.0, 0.0)
        };
        let (mut curve_f, mut curve_s) = (0.0, 0.0);
        if leg_curve > 0.0 {
            let rest_f = d.rest_forward.unwrap_or(front);
            let rest_side = d.rest_side_or().abs();
            let rest_bearing = rest_side.atan2(rest_f).to_degrees();
            let lateral = (90.0 - rest_bearing).abs() / 90.0;
            let mut swing = (leg_curve * (0.15 + 0.85 * lateral)).to_radians();
            if rest_bearing > 90.0 {
                swing = -swing;
            }
            let d_f = f_f - a_f;
            let d_out = (f_s - a_s) * side;
            let (cos_w, sin_w) = (swing.cos(), swing.sin());
            curve_f = 0.5 * (d_f * cos_w - d_out * sin_w - d_f);
            curve_s = 0.5 * (d_f * sin_w + d_out * cos_w - d_out) * side;
        }
        let mut joint_bends = [0.0; 4];
        if leg.joint_bends.len() != profiles.len() {
            for (index, jb) in joint_bends.iter_mut().enumerate().take(profiles.len()) {
                *jb = 0.90 + 0.06 * index as f64;
            }
        } else {
            joint_bends[..profiles.len()].copy_from_slice(leg.joint_bends.as_slice());
        }

        let upper = (size * 0.22).max(d.upper_len_or() * size);
        let lower = (size * 0.22).max(d.lower_len_or() * size);
        let length_weights = &chain.segment_lengths;
        let length_total = 1e-4f64.max(py_sum(length_weights.iter().copied()));
        let n_seg = length_weights.len();
        let mut segment_limits = [0.0; 5];
        for (k, w) in length_weights.iter().enumerate() {
            segment_limits[k] = (upper + lower) * chain.max_stretch * w / length_total;
        }
        let segment_limits = &segment_limits[..n_seg];

        let project = |point: (f64, f64), other: (f64, f64), limit: f64| -> (f64, f64) {
            let dx = point.0 - other.0;
            let dy = point.1 - other.1;
            let distance = hypot(dx, dy);
            if distance <= limit || distance < 1e-5 {
                return point;
            }
            (other.0 + dx * limit / distance, other.1 + dy * limit / distance)
        };

        let build = |bend_scale: f64| -> (ChainPoints, f64) {
            let mut pts = [(0.0, 0.0); MAX_CHAIN_POINTS];
            let n_pts = n_frac + 2;
            pts[0] = (ax, ay);
            for index in 0..n_frac {
                let fraction = fractions[index];
                let fi = index as f64;
                let mut bend = bend_base * profiles[index] * directions[index] * joint_bends[index] * bend_scale;
                if ctx.dragging {
                    let joint_wave = (carry_wave_phase + fi * 0.74).sin();
                    let joint_gain = 0.55 + 0.14 * index.min(2) as f64;
                    bend *= 1.0 + carry_wave_amount * joint_gain * (0.24 + 0.76 * held_response) * joint_wave;
                }
                if catch_strength > 0.0 {
                    bend += size * 0.075 * catch_strength * CATCH_BEND_PROFILE[index];
                }
                let arch = 2.0 * fraction * (1.0 - fraction) * bend_scale;
                let local_f = a_f + (f_f - a_f) * fraction + forward_bias * profiles[index] + curve_f * arch;
                let local_s = a_s + (f_s - a_s) * fraction + side * bend + curve_s * arch;
                let (px, mut py) = self.body_local_to_world(local_f, local_s);
                if ctx.dragging {
                    py += size * (0.105 + held_response * 0.075) * (PI * fraction).sin();
                    py += size
                        * carry_wave_amount
                        * (0.012 + held_response * 0.028)
                        * (carry_wave_phase + fi * 0.86).sin();
                }
                pts[index + 1] = (px, py);
            }
            pts[n_pts - 1] = (fx, fy);
            // Max-length chain: alternating projections keep both contacts.
            let target = (fx, fy);
            for _ in 0..10 {
                pts[0] = (ax, ay);
                for (index, &limit) in segment_limits.iter().enumerate() {
                    pts[index + 1] = project(pts[index + 1], pts[index], limit);
                }
                pts[n_pts - 1] = target;
                for index in (0..segment_limits.len()).rev() {
                    pts[index] = project(pts[index], pts[index + 1], segment_limits[index]);
                }
            }
            pts[0] = (ax, ay);
            pts[n_pts - 1] = target;
            let path_len = py_sum((0..n_pts - 1).map(|k| hypot(pts[k + 1].0 - pts[k].0, pts[k + 1].1 - pts[k].1)));
            (ChainPoints { len: n_pts, points: pts }, path_len)
        };

        let (mut points, path_len) = build(1.0);
        let path_ratio = if ctx.dragging { 1.56 } else { 1.28 };
        let path_budget = ((upper + lower) * chain.max_stretch).min(direct * path_ratio);
        if path_len > path_budget {
            let extra = 1e-4f64.max(path_len - direct);
            let bend_scale = clamp((path_budget - direct) / extra, 0.0, 1.0);
            points = build(bend_scale).0;
        }
        points
    }

    /// `_solve_knee(ax, ay, fx, fy, leg)`: the stylised two-segment knee.
    pub fn solve_knee(&self, i: usize, ax: f64, ay: f64, fx: f64, fy: f64, ctx: &FrameCtx) -> (f64, f64) {
        let d = &self.rig.legs[i];
        let leg = &self.legs[i];
        let size = self.size;
        let sign = d.side_sign;
        let (mut fx, mut fy) = (fx, fy);
        if self.rig.spider_gait.is_none() || leg.stepping || leg.pending_step {
            (fx, fy) = self.limit_world_point_to_leg_reach(i, fx, fy, true);
        }
        let (a_f, a_s) = self.world_to_body_local(ax, ay);
        let (f_f, f_s) = self.world_to_body_local(fx, fy);

        if ctx.dragging {
            let phase = leg.phase_seed;
            let held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0);
            let held_response = clamp(ctx.held.drag_response, 0.0, 1.0);
            let relax = 0.105 + held_response * 0.035 + 0.018 * (0.5 + 0.5 * phase.sin()) + held_speed01 * 0.020;
            let knee_f = a_f + (f_f - a_f) * (0.44 - held_speed01 * 0.035);
            let knee_s = a_s + (f_s - a_s) * 0.48 + sign * size * relax;
            let (kx, ky) = self.body_local_to_world(knee_f, knee_s);
            return (kx, ky + size * (0.040 + held_speed01 * 0.045));
        }

        let rest_f = d.rest_forward_or() * size;
        let rest_s = (d.rest_side_or() * size).abs();
        let attach_s = (d.attach_side_or() * size).abs();
        let l1 = d.upper_len.unwrap_or(0.8) * size;

        let mut outward_side = a_s.abs().max(f_s.abs()).max(attach_s + size * 0.12).max(rest_s * 0.60);
        outward_side += l1 * (0.10 + 0.08 * leg.lift);
        let knee_s = sign * outward_side;

        let lane_pull = clamp(rest_f - a_f, -size * 0.92, size * 0.92);
        let mut knee_f = (a_f + f_f) * 0.50 + lane_pull * 0.38;
        if rest_f > size * 0.45 {
            knee_f = knee_f.max(a_f + size * 0.20);
        } else if rest_f < -size * 0.45 {
            knee_f = knee_f.min(a_f - size * 0.20);
        }
        let forward_min = a_f.min(rest_f).min(f_f) - size * 0.56;
        let forward_max = a_f.max(rest_f).max(f_f) + size * 0.56;
        knee_f = clamp(knee_f, forward_min, forward_max);
        self.body_local_to_world(knee_f, knee_s)
    }

    /// `_leg_lift_px(leg)`: the screen-space height of a swinging or
    /// reaching leg in the lively gait (0 for classic).
    pub fn leg_lift_px(&self, i: usize, ctx: &FrameCtx) -> f64 {
        if !self.uses_lively_gait(ctx) {
            return 0.0;
        }
        let lift_scale = match self.rig.spider_gait.as_ref() {
            Some(g) => g.swing_height,
            None => {
                if self.uses_skitter_gait(ctx) {
                    0.42
                } else {
                    0.55
                }
            }
        };
        let mut px = self.legs[i].lift * self.size * lift_scale;
        let front = self.leg_front_factor(i);
        if self.catch_blend > 0.01 && front > 0.15 {
            px += self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0) * self.size * 0.12;
        }
        px
    }

    /// `_front_leg_feeler_pose(...)`: the optional front-leg feeler bias.
    #[allow(clippy::too_many_arguments)]
    pub fn front_leg_feeler_pose(
        &self,
        i: usize,
        ax: f64,
        ay: f64,
        foot_x: f64,
        foot_y: f64,
        front: f64,
        ctx: &FrameCtx,
    ) -> (f64, f64) {
        let feeler = &self.rig.front_leg_feeler;
        if !feeler.enabled || front <= 0.18 {
            return (foot_x, foot_y);
        }
        let size = self.size;
        let strength = clamp((front - 0.18) / 0.82, 0.0, 1.0);
        let idle_lift = feeler.idle_lift * size * strength;
        let idle_forward = feeler.idle_forward * size * strength;
        let sway = feeler.sway * size * strength;

        let forward_dx = self.heading.cos() * idle_forward;
        let forward_dy = self.heading.sin() * idle_forward;
        let side_sign = self.side_sign(i);
        let side_dx = -self.heading.sin() * sway * side_sign;
        let side_dy = self.heading.cos() * sway * side_sign;

        let feel_clock =
            self.breath_phase * (if self.uses_skitter_gait(ctx) { 2.9 } else { 2.1 }) + self.legs[i].phase_seed * 0.9;
        let pulse = 0.5 + 0.5 * feel_clock.sin();
        let mut fx = foot_x + forward_dx * (0.60 + pulse * 0.40) + side_dx * (feel_clock * 1.8).sin();
        let mut fy = foot_y + forward_dy * (0.60 + pulse * 0.40) + side_dy * (feel_clock * 1.8).sin();
        fy -= idle_lift * (0.65 + pulse * 0.35);

        let max_span = size * (self.rig.legs[i].reach_or() + 0.45);
        let (ddx, ddy) = (fx - ax, fy - ay);
        let dlen = hypot(ddx, ddy);
        if dlen > max_span && dlen > 1e-4 {
            let scale = max_span / dlen;
            fx = ax + ddx * scale;
            fy = ay + ddy * scale;
        }
        (fx, fy)
    }

    /// `_picked_up_leg_pose(leg, ax, ay, foot_x, foot_y)`: each carried leg
    /// sags in its own lane under screen-down gravity.
    pub fn picked_up_leg_pose(
        &self,
        i: usize,
        ax: f64,
        ay: f64,
        foot_x: f64,
        foot_y: f64,
        ctx: &FrameCtx,
    ) -> (f64, f64) {
        let leg = &self.legs[i];
        let d = &self.rig.legs[i];
        let size = self.size;
        let h = &ctx.held;
        let phase = leg.phase_seed;
        let held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0);
        let held_response = clamp(h.drag_response, 0.0, 1.0);
        let held_clock = h.pose_clock;
        let settle = clamp(h.leg_relax, 0.0, 1.0);
        let sign = d.side_sign;
        let lane_f = d.rest_forward_or() * size;
        let lane_s = d.rest_side_or() * size * sign;
        let (lane_x, lane_y) = self.body_local_to_world(lane_f, lane_s);
        let history_blend = (1.0 - settle) * 0.22;
        let lane_source_x = lane_x + (foot_x - lane_x) * history_blend;
        let lane_source_y = lane_y + (foot_y - lane_y) * history_blend;
        let spread = 0.68 - held_response * 0.04 + 0.025 * (phase * 0.73 + held_clock * 0.34).sin();
        let mut fx = ax + (lane_source_x - ax) * spread;
        let mut fy = foot_y;

        let leg_phase = held_clock + phase * 0.24;
        fx += h.drag_sway_x * (0.72 + 0.10 * leg_phase.sin());
        fy += h.drag_sway_y * (0.72 + 0.10 * leg_phase.cos());

        let drag_speed = hypot(h.drag_vel_x, h.drag_vel_y);
        if drag_speed > 1.0 {
            let lag = size * (0.025 + held_speed01 * 0.16);
            fx -= (h.drag_vel_x / drag_speed) * lag;
            fy -= (h.drag_vel_y / drag_speed) * lag * 0.30;
        }

        let bottom_band = self.y + size * (0.52 + 0.022 * (phase * 1.13 + held_clock * 0.24).sin());
        let natural_drop = clamp(lane_source_y - ay, 0.0, size * 0.45);
        let gravity_floor = bottom_band + size * (0.10 + held_speed01 * 0.045) + natural_drop * 0.10;
        let bounce_amp = size * (0.012 + held_response * 0.052);
        let bounce = bounce_amp * (held_clock * 1.10 + phase * 0.31).sin();
        fy = fy.max(gravity_floor + bounce);
        fx += leg.held_spring_x;
        fy += leg.held_spring_y;
        (fx, fy)
    }

    /// `_combat_leg_pose(...)`: raise and spread the front legs of a spider
    /// squared up at a foe, or throw them with a bite.
    #[allow(clippy::too_many_arguments)]
    pub fn combat_leg_pose(&self, i: usize, foot_x: f64, foot_y: f64, front: f64, draw: &DrawCtx) -> (f64, f64) {
        let stance = draw.combat_stance;
        let (push, lift) = (draw.strike_push, draw.strike_lift);
        if (stance <= 0.01 && lift <= 0.01) || front <= 0.15 {
            return (foot_x, foot_y);
        }
        let size = self.size;
        let front_weight = clamp((front - 0.15) / 0.85, 0.0, 1.0);
        let side = self.side_sign(i);
        let (mut fx, mut fy) = (foot_x, foot_y);
        if lift > 0.01 {
            let (face_x, face_y) = draw.strike_face;
            let (lateral_x, lateral_y) = (-face_y, face_x);
            let spread = size * 0.30 * lift * front_weight;
            let reach = size * (0.15 * lift + 0.60 * push.max(0.0)) * front_weight;
            fx += face_x * reach + lateral_x * side * spread;
            fy += face_y * reach + lateral_y * side * spread;
            return (fx, fy);
        }
        let weight = stance * front_weight;
        let (face_x, face_y) = (draw.combat_face_x, draw.combat_face_y);
        let (lateral_x, lateral_y) = (-face_y, face_x);
        fx += face_x * size * 0.30 * weight;
        fy += face_y * size * 0.30 * weight;
        fx += lateral_x * side * size * 0.34 * weight;
        fy += lateral_y * side * size * 0.34 * weight;
        fy -= size * 0.52 * weight;
        (fx, fy)
    }

    /// `_leg_draw_points(leg)`: world-space `(ax, ay, foot_x, foot_y)` for
    /// rendering, with catch reach, feeding, feelers, combat stance, the
    /// airborne tuck, the roll tuck and the carried pose.
    pub fn leg_draw_points(&self, i: usize, ctx: &FrameCtx, draw: &DrawCtx) -> (f64, f64, f64, f64) {
        let size = self.size;
        let (ax, ay) = self.leg_attach_drawn(i, draw);
        let (mut foot_x, mut foot_y) = self.visual_foot_for_render(i, ctx);
        let front = self.leg_front_factor(i);
        if ctx.dragging {
            let (px, py) = self.picked_up_leg_pose(i, ax, ay, foot_x, foot_y, ctx);
            return (ax, ay, px, py);
        }
        if self.catch_blend > 0.001 && front > 0.15 {
            let (cx, cy) = self.catch_point;
            let reach = self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0);
            let (cx, cy) = self.constrain_leg_point(i, cx, cy);
            let mut rx = foot_x + (cx - foot_x) * 0.40 * reach;
            let mut ry = foot_y + (cy - foot_y) * 0.40 * reach;
            let max_span = size * (self.rig.legs[i].reach_or() + 0.4);
            let (ddx, ddy) = (rx - ax, ry - ay);
            let dlen = hypot(ddx, ddy);
            if dlen > max_span && dlen > 1e-4 {
                let scale = max_span / dlen;
                rx = ax + ddx * scale;
                ry = ay + ddy * scale;
            }
            foot_x = rx;
            foot_y = ry;
        }
        if ctx.state == MotionState::Feed && front > 0.2 {
            let paw = draw.feed_paw;
            let (cx, cy) = self.catch_point;
            let (dx, dy) = (cx - foot_x, cy - foot_y);
            let dl = hypot(dx, dy);
            let dl = if dl == 0.0 { 1.0 } else { dl };
            let (ux, uy) = (dx / dl, dy / dl);
            let amp = size * 0.11 * clamp((front - 0.2) / 0.8, 0.0, 1.0);
            let seed = self.legs[i].phase_seed;
            let osc = (paw * 22.0 + seed * 2.0).sin();
            foot_x += ux * amp * osc;
            foot_y += uy * amp * osc;
            let knead = (paw * 17.0 + seed).sin();
            foot_x += -uy * amp * 0.5 * knead;
            foot_y += ux * amp * 0.5 * knead;
        }
        (foot_x, foot_y) = self.front_leg_feeler_pose(i, ax, ay, foot_x, foot_y, front, ctx);
        (foot_x, foot_y) = self.combat_leg_pose(i, foot_x, foot_y, front, draw);
        if ctx.airborne && draw.jump_peak > 1e-3 {
            let tuck = clamp(draw.jump_z / draw.jump_peak, 0.0, 1.0) * 0.8;
            foot_x += (ax - foot_x) * tuck;
            foot_y += (ay - foot_y) * tuck;
        }
        if self.roll_tuck > 1e-3 {
            let tuck_strength = clamp(self.rig.roll_tuck_strength, 0.25, 0.62);
            let t = clamp(self.roll_tuck, 0.0, 1.0) * tuck_strength;
            foot_x += (ax - foot_x) * t;
            foot_y += (ay - foot_y) * t;
        }
        (ax, ay, foot_x, foot_y)
    }
}
