//! Port of `creature/kinematics/spider_gait.py`: the eight-legged gait cycle
//! (groups, phase windows, timing) and its grounded scheduler.

use crate::config::SpiderGaitConfig;
use crate::core::{Candidates, FrameCtx, IndexSet, Kinematics};
use crate::gait::fixed_substeps;
use crate::pymath::{hypot, wrap_pi};
use dc_core::PyRandom;
use dc_core::math::{clamp, py_mod};

/// `_spider_phase_window` result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseWindow {
    pub in_window: bool,
    pub relative: f64,
    pub group: usize,
}

#[derive(Clone, Copy, Default)]
struct GaitCandidate {
    leg: usize,
    target: (f64, f64),
    emergency: bool,
}

impl Kinematics {
    /// `_spider_group_index(leg)`: the model gait group mapped to one of the
    /// two tetrapod phases.
    pub fn spider_group_index(&self, i: usize) -> usize {
        let group = self.rig.legs[i].gait_group;
        if self.gait_groups.len() < 2 {
            return 0;
        }
        match self.gait_groups.iter().position(|&g| g == group) {
            Some(idx) => idx % 2,
            None => group.rem_euclid(2) as usize,
        }
    }

    /// `_leg_front_factor(leg)`: ~1 front pair, ~0 mid, ~-1 rear.
    #[inline]
    pub fn leg_front_factor(&self, i: usize) -> f64 {
        let d = &self.rig.legs[i];
        match d.rest_forward {
            Some(rf) => clamp(rf, -1.0, 1.0),
            None => d.attach_angle.unwrap_or(0.0).to_radians().cos(),
        }
    }

    /// `_spider_cycle_hz(config, speed01, turn_speed)`: the one cadence used
    /// by both phase windows and foot swings.
    pub fn spider_cycle_hz(&self, config: &SpiderGaitConfig, speed01: f64, turn_speed: f64) -> f64 {
        let stride = config.stride_length * self.size;
        let mut hz = if stride > 1.0 {
            config.cycle_hz.max(self.current_speed / stride)
        } else {
            config.cycle_hz + speed01 * config.speed_cycle_gain
        };
        if turn_speed > 0.30 {
            hz += turn_speed.min(5.5) * config.turn_cycle_gain;
        }
        clamp(hz, 1.20, config.max_cycle_hz)
    }

    /// `_spider_step_duration(config, speed01, turn_speed, force_fast)`
    pub fn spider_step_duration(
        &self,
        config: &SpiderGaitConfig,
        speed01: f64,
        turn_speed: f64,
        force_fast: bool,
    ) -> f64 {
        let hz = self.spider_cycle_hz(config, speed01, turn_speed);
        let mut duration = config.swing_fraction / 0.1f64.max(hz);
        if force_fast {
            duration *= 0.86;
        }
        clamp(duration, config.min_swing_time, 0.18)
    }

    /// `_spider_phase_window(leg, phase, config)`: whether a leg is in its
    /// group's launch window.
    pub fn spider_phase_window(&self, i: usize, phase: f64, config: &SpiderGaitConfig) -> PhaseWindow {
        let group = self.spider_group_index(i);
        let front = self.leg_front_factor(i);
        let start = if config.metachronal {
            let phase_offset = self.rig.legs[i].phase_offset.unwrap_or(0.5 * group as f64);
            phase_offset - front * config.front_phase_offset
        } else {
            (0.5 * group as f64) - front * config.front_phase_offset
        };
        let relative = py_mod(phase - start, 1.0);
        PhaseWindow { in_window: relative < config.swing_fraction, relative, group }
    }

    /// `_spider_predicted_target(leg, config, turn_speed)`: where the foot
    /// should land after its bounded swing.
    pub fn spider_predicted_target(
        &self,
        i: usize,
        config: &SpiderGaitConfig,
        turn_speed: f64,
        ctx: &FrameCtx,
    ) -> (f64, f64) {
        let speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0);
        let duration = self.spider_step_duration(config, speed01, turn_speed, false);
        let (mut target_x, mut target_y) = self.leg_ideal_foot(i, ctx);
        let (mut vx, mut vy) = (self.vel_x, self.vel_y);
        let mut velocity = hypot(vx, vy);
        if velocity < 1.0 && self.current_speed > 1.0 {
            let [fx, fy, _, _] = self.basis();
            vx = fx * self.current_speed;
            vy = fy * self.current_speed;
            velocity = self.current_speed;
        }
        if velocity > 1.0 {
            let lookahead = velocity * duration * config.step_lookahead;
            target_x += vx / velocity * lookahead;
            target_y += vy / velocity * lookahead;
        }
        self.constrain_leg_point(i, target_x, target_y)
    }

    /// `_update_spider_gait_step(dt, config)`: the grounded tetrapod scheduler.
    pub fn update_spider_gait_step(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        if let Some(config) = self.rig.spider_gait {
            self.update_spider_gait_step_with(dt, &config, ctx, rng);
        }
    }

    pub(crate) fn update_spider_gait_step_with(
        &mut self,
        dt: f64,
        config: &SpiderGaitConfig,
        ctx: &FrameCtx,
        rng: &mut PyRandom,
    ) {
        if ctx.airborne {
            return;
        }
        self.advance_active_steps(dt, ctx, rng);

        let dheading = wrap_pi(self.heading - self.prev_heading_gait);
        self.prev_heading_gait = self.heading;
        let turn_speed = dheading.abs() / dt.max(1e-3);
        self.spider_turn_speed = turn_speed;
        let turn_err = wrap_pi(self.target_heading - self.heading).abs();
        let moving = self.current_speed > 4.0;
        let turning = turn_speed > 0.30 || turn_err > 0.12;
        let turn_drive = if turning { turn_speed.max(turn_err * config.turn_error_drive) } else { 0.0 };
        let speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0);
        let fast_state = ctx.state.is_fast();
        let max_air = config.max_airborne;
        let swinging_now = self.legs.iter().filter(|l| l.airborne()).count();

        self.lively_gait_phase =
            py_mod(self.lively_gait_phase + dt * self.spider_cycle_hz(config, speed01, turn_drive), 1.0);

        let size = self.size;
        let mut candidates: Candidates<GaitCandidate> = Candidates::new();
        for i in 0..self.legs.len() {
            let leg = self.legs[i];
            if leg.stepping || leg.pending_step || leg.step_cooldown > 0.0 {
                continue;
            }
            let a0 = self.leg_alignment_metrics(i, leg.foot_x, leg.foot_y);
            let mut severe_wrong = a0.severe_wrong;
            let r0 = self.leg_reach_metrics(i, leg.foot_x, leg.foot_y, false);
            let stroke_pressure = leg.stroke_progress >= 0.80;
            let turn_pressure = leg.stance_turn.abs() >= config.support_turn_limit * config.turn_step_pressure;
            let side_sign0 = self.side_sign(i);
            let side_magnitude0 = side_sign0 * a0.local_s;
            let turn_side_pressure = turning
                && side_magnitude0 < (size * 0.10).max(a0.min_side * 0.72)
                && leg.stance_turn.abs() > config.support_turn_limit * 0.28;
            let emergency = severe_wrong || r0.very_far || stroke_pressure || turn_pressure || turn_side_pressure;
            let target = self.spider_predicted_target(i, config, turn_drive, ctx);
            let distance_to_target = hypot(leg.foot_x - target.0, leg.foot_y - target.1);
            let turn_allowance = if turning { self.spider_turn_rehome_allowance(i, config) } else { 0.0 };
            let step_distance = (distance_to_target - turn_allowance).max(0.0);
            let mut threshold = size * config.step_trigger.max(config.stance_deadband);
            if !moving && !turning {
                threshold *= 0.78;
            }
            if fast_state {
                threshold *= 0.82;
            }
            if turning {
                threshold *= 1.08;
            }
            let window = self.spider_phase_window(i, self.lively_gait_phase, config);
            let relative = window.relative;
            let in_window = relative < config.swing_fraction;
            let a = self.leg_alignment_metrics(i, leg.foot_x, leg.foot_y);
            let mut wrong_side = a.wrong_side;
            let too_far = self.leg_reach_metrics(i, leg.foot_x, leg.foot_y, false).too_far;
            let turn_side_safe = turning
                && !too_far
                && self.side_sign(i) * a.local_s >= size * 0.07
                && leg.stance_turn.abs() < config.support_turn_limit * 0.98;
            if turn_side_safe {
                wrong_side = false;
                if (a.local_f - self.rig.legs[i].rest_forward_or() * size).abs() < size * 1.40 {
                    severe_wrong = false;
                }
            }
            if turning
                && wrong_side
                && !severe_wrong
                && !too_far
                && leg.stance_turn.abs() < config.support_turn_limit * 0.98
            {
                wrong_side = false;
            }
            let needs_step = step_distance > threshold || wrong_side || too_far;
            if !emergency && (!needs_step || !in_window) {
                continue;
            }
            let mut score = step_distance / threshold.max(1.0);
            if emergency {
                score += 3.0;
            }
            score += (config.swing_fraction - relative).max(0.0) * 0.35;
            candidates.push(score, GaitCandidate { leg: i, target, emergency });
        }

        candidates.sort_desc();
        let mut slots = max_air.saturating_sub(swinging_now);
        let mut side_left = 0usize;
        let mut side_right = 0usize;
        for (i, leg) in self.legs.iter().enumerate() {
            if leg.airborne() {
                let s = self.rig.legs[i].side_sign;
                if s < 0.0 {
                    side_left += 1;
                } else if s > 0.0 {
                    side_right += 1;
                }
            }
        }
        let mut used = IndexSet::default();
        for &(_, c) in candidates.as_slice() {
            if slots == 0 {
                break;
            }
            let side = self.side_sign(c.leg);
            let count = if side < 0.0 { &mut side_left } else { &mut side_right };
            if *count >= 2 && !c.emergency {
                continue;
            }
            self.legs[c.leg].last_step_phase = self.lively_gait_phase;
            self.legs[c.leg].last_step_emergency = c.emergency;
            self.schedule_step(c.leg, c.target.0, c.target.1, 0.0, c.emergency || fast_state, ctx, rng);
            self.legs[c.leg].step_cooldown = 0.0;
            if side < 0.0 {
                side_left += 1;
            } else {
                side_right += 1;
            }
            used.insert(c.leg);
            slots -= 1;
        }

        self.update_spider_joint_articulation(dt);
        self.update_feelers(dt, moving, turning, ctx, rng);
        if !used.is_empty() {
            self.turn_rehome_pressure = (self.turn_rehome_pressure - 0.18).max(0.0);
        }
    }

    /// `_update_spider_gait(dt, config)`: the scheduler at the fixed 120 Hz rate.
    pub fn update_spider_gait(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        let Some(config) = self.rig.spider_gait else {
            return;
        };
        let (substeps, step) = fixed_substeps(dt);
        for _ in 0..substeps {
            self.update_spider_gait_step_with(step, &config, ctx, rng);
        }
    }
}
