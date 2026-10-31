//! Port of `creature/kinematics/stepping.py`: scheduling, starting and
//! advancing a footstep, and the classic reactive gait (`_update_legs`).

use crate::constants::smootherstep;
use crate::core::{Candidates, FrameCtx, IndexSet, Kinematics, MotionState};
use crate::legstate::ContactState;
use crate::pymath::{hypot, py_pow, wrap_pi};
use dc_core::PyRandom;
use dc_core::math::clamp;
use std::f64::consts::{PI, TAU};

impl Kinematics {
    /// `_schedule_step(leg, target_x, target_y, delay, force_fast)`
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_step(
        &mut self,
        i: usize,
        target_x: f64,
        target_y: f64,
        delay: f64,
        force_fast: bool,
        ctx: &FrameCtx,
        rng: &mut PyRandom,
    ) {
        if self.legs[i].stepping || self.legs[i].pending_step {
            return;
        }
        let (px, py) = self.constrain_leg_point(i, target_x, target_y);
        let leg = &mut self.legs[i];
        leg.pending_step = true;
        leg.pending_delay = delay.max(0.0);
        leg.pending_target_x = px;
        leg.pending_target_y = py;
        if delay <= 0.001 {
            self.begin_step(i, force_fast, ctx, rng);
        }
    }

    /// `_begin_step(leg, force_fast)`: lift the foot and plan its swing arc.
    pub fn begin_step(&mut self, i: usize, force_fast: bool, ctx: &FrameCtx, rng: &mut PyRandom) {
        {
            let leg = &mut self.legs[i];
            leg.pending_step = false;
            leg.stepping = true;
            leg.contact_state = ContactState::Swing;
            leg.support_weight = 0.0;
            leg.contact_age = 0.0;
            leg.stroke_progress = 0.0;
            leg.step_timer = 0.0;
        }
        let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
        let spider_gait = self.rig.spider_gait;
        let lively = self.uses_lively_gait(ctx);
        let skitter = self.uses_skitter_gait(ctx);
        let mut duration = match spider_gait {
            Some(cfg) if lively => self.spider_step_duration(
                &cfg,
                clamp(self.current_speed / 150.0, 0.0, 1.0),
                self.spider_turn_speed,
                force_fast,
            ),
            _ => 0.30 - 0.13 * speed01,
        };
        let sliding = self.is_drift_sliding(ctx);
        if spider_gait.is_none() {
            if force_fast
                || (ctx.state.is_fast() && !sliding)
                || (self.drift.last_drift_amount.abs() > 0.22 && !sliding)
            {
                duration *= 0.72;
            }
            if skitter && !sliding {
                duration *= if force_fast { 0.58 } else { 0.66 };
            }
            if sliding {
                duration *= self.rig.personality.drift_slide_step_slowdown;
            }
            let min_step = if skitter && !sliding { 0.052 } else { 0.095 };
            let max_step = if skitter && !sliding { 0.26 } else { 0.40 };
            duration = clamp(duration, min_step, max_step);
        }
        self.legs[i].step_duration = duration;

        let (fx0, fy0) = (self.legs[i].foot_x, self.legs[i].foot_y);
        if self.leg_alignment_metrics(i, fx0, fy0).severe_wrong {
            let (cx, cy) = self.constrain_leg_point(i, fx0, fy0);
            self.legs[i].foot_x = cx;
            self.legs[i].foot_y = cy;
        }
        {
            let leg = &mut self.legs[i];
            leg.step_start_x = leg.foot_x;
            leg.step_start_y = leg.foot_y;
        }
        let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
        let (mut target_f, mut target_s) =
            self.world_to_body_local(self.legs[i].pending_target_x, self.legs[i].pending_target_y);
        let side = self.side_sign(i);
        let spider_lively = spider_gait.is_some() && lively;
        if spider_lively {
            let seed = self.legs[i].phase_seed;
            target_f += seed.sin() * 0.018 * self.size;
            target_s += side * (seed * 1.37).cos() * 0.012 * self.size;
        } else {
            target_f += rng.uniform(-0.060, 0.095) * self.size * (0.7 + speed01);
            target_s += side * rng.uniform(-0.040, 0.055) * self.size * (0.6 + speed01 * 0.5);
        }
        let (tx, ty) = self.body_local_to_world(target_f, target_s);
        let (sx, sy) = self.constrain_leg_point(i, tx, ty);
        self.legs[i].step_target_x = sx;
        self.legs[i].step_target_y = sy;

        let leg = self.legs[i];
        let mid_x = (leg.step_start_x + leg.step_target_x) * 0.5;
        let mid_y = (leg.step_start_y + leg.step_target_y) * 0.5;
        let path_x = leg.step_target_x - leg.step_start_x;
        let path_y = leg.step_target_y - leg.step_start_y;
        let path_len = 1.0f64.max(hypot(path_x, path_y));
        if let Some(cfg) = spider_gait
            && lively
        {
            let [fx, fy, rx, ry] = self.basis();
            let front_factor = clamp(self.rig.legs[i].rest_forward_or(), -1.0, 1.0);
            let forward_arc = path_len * cfg.swing_arc_forward * (0.82 + front_factor.max(0.0) * 0.18);
            let outward_arc = path_len * cfg.swing_arc_outward;
            let leg = &mut self.legs[i];
            leg.step_control_x = mid_x + fx * forward_arc + rx * side * outward_arc;
            leg.step_control_y = mid_y + fy * forward_arc + ry * side * outward_arc;
            return;
        }
        let mut nx = -path_y / path_len;
        let mut ny = path_x / path_len;
        let [_, _, rx, ry] = self.basis();
        let (out_x, out_y) = (rx * side, ry * side);
        if nx * out_x + ny * out_y < 0.0 {
            nx = -nx;
            ny = -ny;
        }
        let mut arc = clamp(path_len * rng.uniform(0.22, 0.36), self.size * 0.12, self.size * 0.56);
        if skitter {
            arc *= 0.78;
        }
        let leg = &mut self.legs[i];
        leg.step_control_x = mid_x + nx * arc;
        leg.step_control_y = mid_y + ny * arc;
    }

    /// `_advance_active_steps(dt)`: integrate in-flight swings and cooldowns.
    pub fn advance_active_steps(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        for i in 0..self.legs.len() {
            self.legs[i].step_cooldown = (self.legs[i].step_cooldown - dt).max(0.0);

            if self.legs[i].pending_step {
                self.legs[i].pending_delay -= dt;
                if self.legs[i].pending_delay <= 0.0 {
                    let sliding = self.is_drift_sliding(ctx);
                    let force_fast = (ctx.state.is_fast() || self.drift.last_drift_amount.abs() > 0.22) && !sliding;
                    self.begin_step(i, force_fast, ctx, rng);
                }
            }

            if self.legs[i].stepping {
                let leg = &mut self.legs[i];
                leg.step_timer += dt;
                let t = clamp(leg.step_timer / 0.001f64.max(leg.step_duration), 0.0, 1.0);
                let s = smootherstep(t);
                let a_x = leg.step_start_x + (leg.step_control_x - leg.step_start_x) * s;
                let a_y = leg.step_start_y + (leg.step_control_y - leg.step_start_y) * s;
                let b_x = leg.step_control_x + (leg.step_target_x - leg.step_control_x) * s;
                let b_y = leg.step_control_y + (leg.step_target_y - leg.step_control_y) * s;
                leg.foot_x = a_x + (b_x - a_x) * s;
                leg.foot_y = a_y + (b_y - a_y) * s;
                leg.lift = py_pow((PI * t).sin(), 0.85);
                if t >= 1.0 {
                    leg.stepping = false;
                    leg.lift = 0.0;
                    leg.foot_x = leg.step_target_x;
                    leg.foot_y = leg.step_target_y;
                    let (fx, fy) = (leg.foot_x, leg.foot_y);
                    if self.rig.spider_gait.is_some() {
                        let (local_f, local_s) = self.world_to_body_local(fx, fy);
                        let leg = &mut self.legs[i];
                        leg.contact_state = ContactState::Stance;
                        leg.support_weight = 0.12;
                        leg.contact_age = 0.0;
                        leg.stroke_progress = 0.0;
                        leg.stance_base_f = local_f;
                        leg.stance_base_s = local_s;
                        leg.stance_stroke_f = 0.0;
                        leg.stance_stroke_s = 0.0;
                        leg.stance_turn = 0.0;
                    }
                    let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
                    let lively = self.uses_lively_gait(ctx);
                    if let Some(cfg) = self.rig.spider_gait
                        && lively
                    {
                        let leg = &mut self.legs[i];
                        leg.step_cooldown = leg.step_duration * cfg.refractory_fraction;
                    } else if self.uses_skitter_gait(ctx) {
                        self.legs[i].step_cooldown = rng.uniform(0.012, 0.045) * (1.08 - speed01 * 0.36);
                    } else {
                        self.legs[i].step_cooldown = rng.uniform(0.035, 0.095) * (1.15 - speed01 * 0.45);
                    }
                }
            }
        }
    }

    /// `_update_legs(dt)`: the per-frame leg update. Dispatches to the lively
    /// gait (and through it the spider gait), or runs the classic reactive
    /// gait with leg-level coordination.
    pub fn update_legs(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        if ctx.dragging {
            return;
        }
        if ctx.state == MotionState::Roll {
            return;
        }
        if self.uses_lively_gait(ctx) {
            self.update_legs_lively(dt, ctx, rng);
            return;
        }
        if ctx.airborne {
            return;
        }
        self.advance_active_steps(dt, ctx, rng);

        let size = self.size;
        let moving = self.current_speed > 4.0;
        let hold_still = self.current_speed < 3.0 && !ctx.airborne;
        let mut threshold = size * (if moving { 0.36 } else { 0.21 });
        let drifting_fast = self.drift.last_drift_amount.abs() > 0.18;
        let sliding = self.is_drift_sliding(ctx);
        let observing = ctx.state == MotionState::Observe && ctx.acts_as_observer;
        if sliding {
            threshold *= self.rig.personality.drift_slide_rehome_slop;
        } else if ctx.state.is_fast() || drifting_fast {
            threshold *= 0.72;
        }
        if observing {
            threshold *= 0.78;
        }

        let speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0);
        let mut max_swinging: i64 = if !moving {
            1
        } else if speed01 < 0.55 {
            2
        } else {
            3
        };
        if sliding {
            max_swinging = max_swinging.min(self.rig.personality.drift_slide_max_swinging);
        } else if ctx.state.is_fast() || drifting_fast {
            max_swinging =
                if matches!(ctx.state, MotionState::Dragged | MotionState::DriftRun) || drifting_fast { 4 } else { 3 };
        }
        if observing && moving {
            max_swinging = max_swinging.max(2);
        }
        let mut swinging_now = self.legs.iter().filter(|l| l.airborne()).count() as i64;

        let mut emergency_rehome = false;
        let mut emergency: Candidates<(usize, f64, f64)> = Candidates::new();
        for i in 0..self.legs.len() {
            let (fx, fy) = (self.legs[i].foot_x, self.legs[i].foot_y);
            let a = self.leg_alignment_metrics(i, fx, fy);
            let r = self.leg_reach_metrics(i, fx, fy, false);
            let broken = a.severe_wrong || (if hold_still { r.very_far } else { r.too_far });
            if broken {
                emergency_rehome = true;
                if !self.legs[i].airborne() {
                    let (ix, iy) = self.leg_ideal_foot(i, ctx);
                    let mut dist = hypot(fx - ix, fy - iy);
                    if r.very_far {
                        dist += size * 0.65;
                    }
                    emergency.push(dist, (i, ix, iy));
                }
            }
        }
        emergency.sort_desc();
        let emergency_slots = (max_swinging - swinging_now).max(0) as usize;
        for &(_, (i, ix, iy)) in emergency.as_slice().iter().take(emergency_slots) {
            self.schedule_step(i, ix, iy, 0.0, true, ctx, rng);
            self.legs[i].step_cooldown = rng.uniform(0.020, 0.055);
            swinging_now += 1;
        }

        if hold_still {
            return;
        }
        if swinging_now >= max_swinging {
            return;
        }

        let mut candidates: Candidates<(usize, f64, f64)> = Candidates::new();
        for i in 0..self.legs.len() {
            let leg = self.legs[i];
            if leg.stepping || leg.pending_step || leg.step_cooldown > 0.0 {
                continue;
            }
            let (ix, iy) = self.leg_ideal_foot(i, ctx);
            let mut dist = hypot(leg.foot_x - ix, leg.foot_y - iy);
            let a = self.leg_alignment_metrics(i, leg.foot_x, leg.foot_y);
            let r = self.leg_reach_metrics(i, leg.foot_x, leg.foot_y, false);
            if a.wrong_side {
                dist = dist.max(threshold + size * (if a.severe_wrong { 0.62 } else { 0.34 }));
            }
            if r.too_far {
                dist = dist.max(threshold + size * (if r.very_far { 0.74 } else { 0.42 }));
            }
            if self.turn_rehome_pressure > 0.40 {
                dist = dist.max((a.local_f - a.rest_f).abs() * 0.34 + threshold + size * 0.08);
            }

            let mut urgency = dist / threshold.max(1.0);
            // `urgency <= 1.0 and not (not moving and rng.random() < 0.012)`:
            // the draw happens only when the first test passes.
            if urgency <= 1.0 && !(!moving && rng.random() < 0.012) {
                continue;
            }

            let group = self.rig.legs[i].gait_group;
            let preferred_phase = group as f64 * PI + leg.gait_phase_offset;
            let phase = dc_core::math::py_mod(self.bob_phase + preferred_phase, TAU);
            let phase_gate = 0.55 + 0.45 * ((phase.cos() + 1.0) * 0.5);
            urgency *= phase_gate;

            for (j, other) in self.legs.iter().enumerate() {
                if !other.airborne() {
                    continue;
                }
                let same_segment_opposite = i.abs_diff(j) == 1 && i.min(j) % 2 == 0;
                let same_side_adjacent = i.abs_diff(j) == 2;
                let same_group = self.rig.legs[j].gait_group == group;
                if same_segment_opposite {
                    urgency *= 0.58;
                } else if same_side_adjacent {
                    urgency *= 0.62;
                } else if same_group {
                    urgency *= 0.82;
                }
            }

            urgency += rng.uniform(-0.10, 0.18);
            if urgency > 1.0 {
                candidates.push(urgency, (i, ix, iy));
            }
        }

        candidates.sort_desc();
        let slots = (max_swinging - swinging_now).max(0) as usize;
        let mut started = false;
        let mut used = IndexSet::default();
        let mut scheduled_count = 0usize;
        let fast_footwork = (ctx.state.is_fast() || drifting_fast) && !sliding;
        for &(_, (i, ix, iy)) in candidates.as_slice() {
            if scheduled_count >= slots {
                break;
            }
            if !emergency_rehome && used.has_neighbour_of(i) {
                continue;
            }
            let side_sign = self.side_sign(i);
            let turn_bias = clamp(wrap_pi(self.target_heading - self.heading), -1.0, 1.0);
            let (mut local_ix, local_iy) = self.world_to_body_local(ix, iy);
            local_ix += side_sign * turn_bias * size * 0.035;
            let (ix, iy) = self.body_local_to_world(local_ix, local_iy);
            let delay = if sliding {
                rng.uniform(0.030, 0.120)
            } else {
                rng.uniform(0.000, if fast_footwork { 0.040 } else { 0.070 })
            };
            self.schedule_step(i, ix, iy, delay, fast_footwork, ctx, rng);
            self.legs[i].step_cooldown = if sliding { rng.uniform(0.070, 0.160) } else { rng.uniform(0.025, 0.070) };
            used.insert(i);
            scheduled_count += 1;
            started = true;
        }

        self.idle_twitch_timer -= dt;
        if !started && self.current_speed < 2.0 && self.idle_twitch_timer <= 0.0 {
            let idle_count = self.legs.iter().filter(|l| !l.airborne()).count();
            if idle_count > 0 {
                let pick = rng.choice_index(idle_count);
                let i =
                    self.legs.iter().enumerate().filter(|(_, l)| !l.airborne()).nth(pick).map(|(i, _)| i).unwrap_or(0);
                let (ix, iy) = self.leg_ideal_foot(i, ctx);
                let tx = ix + rng.uniform(-3.2, 3.2);
                let ty = iy + rng.uniform(-3.2, 3.2);
                self.schedule_step(i, tx, ty, 0.0, false, ctx, rng);
                started = true;
            }
            self.idle_twitch_timer = rng.uniform(0.75, 2.0);
        }

        if started {
            self.turn_rehome_pressure = (self.turn_rehome_pressure - 0.18).max(0.0);
        }
    }
}
