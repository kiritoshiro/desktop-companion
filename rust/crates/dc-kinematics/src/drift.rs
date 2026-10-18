//! The drift (low-grip slide) helpers from `creature/behaviour/temperament.py`
//! that the body movement and the gait consume every frame:
//! `_drift_state_multiplier`, `_is_drift_sliding`, `_drift_amount`,
//! `_prime_drift` and `_guard_drifter_travel_direction`.
//!
//! They live here, not in `dc-creature`, because `_move_body` and the
//! stepping code call them in the middle of a kinematic update and their
//! state ([`crate::core::DriftState`]) is read by both.

use crate::constants::smootherstep;
use crate::core::{FrameCtx, Kinematics, MotionState};
use crate::pymath::hypot;
use dc_core::PyRandom;
use dc_core::math::{clamp, rand_range};

impl Kinematics {
    /// `_drift_state_multiplier(inertia=...)`
    pub fn drift_state_multiplier(&self, inertia: bool, ctx: &FrameCtx) -> f64 {
        use MotionState::*;
        if !ctx.acts_as_drifter {
            return 0.0;
        }
        if inertia {
            return 1.18;
        }
        match ctx.state {
            Approach => 0.58,
            Chase => 0.76,
            Retreat => 0.82,
            Dragged => 0.82,
            Startled => 0.92,
            Wander => 0.34,
            Zoom => 0.82,
            DriftRun => 1.14,
            Play => 0.62,
            _ => 0.0,
        }
    }

    /// `_is_drift_sliding()`
    #[inline]
    pub fn is_drift_sliding(&self, ctx: &FrameCtx) -> bool {
        if !ctx.acts_as_drifter {
            return false;
        }
        if ctx.state == MotionState::DriftRun && ctx.drift_run_sliding {
            return true;
        }
        self.drift.last_drift_amount.abs() > 0.16 && self.drift.drift_momentum > self.rig.personality.drift_breakaway
    }

    /// `_prime_drift(boost, direction)`
    pub fn prime_drift(&mut self, boost: f64, direction: Option<f64>, ctx: &FrameCtx, rng: &mut PyRandom) {
        if !ctx.acts_as_drifter {
            return;
        }
        if let Some(d) = direction {
            self.drift.drift_dir = if d >= 0.0 { 1.0 } else { -1.0 };
        }
        self.drift.drift_boost = self.drift.drift_boost.max(clamp(boost, 0.0, 1.6));
        let switch = rand_range(self.rig.personality.drift_switch_time, 1.15, 2.4, rng);
        self.drift.drift_flip_timer = self.drift.drift_flip_timer.max(switch);
        self.drift.wiggle_burst = self.drift.wiggle_burst.max(0.16 + self.drift.drift_boost * 0.10);
    }

    /// `_drift_amount(dt, speed, inertia=...)`: the signed sideways slip this
    /// frame, updating momentum, lean and the flip clock.
    pub fn drift_amount(&mut self, dt: f64, speed: f64, inertia: bool, ctx: &FrameCtx, rng: &mut PyRandom) -> f64 {
        let mult = self.drift_state_multiplier(inertia, ctx);
        let p = self.rig.personality.clone_drift();
        let dr = &mut self.drift;
        if mult <= 0.0 || (self.motion_paused && ctx.state != MotionState::Dragged) {
            dr.last_drift_amount = 0.0;
            dr.drift_momentum = (dr.drift_momentum - dt * 1.8).max(0.0);
            dr.drift_lean += (0.0 - dr.drift_lean) * (1.0 - (-dt * 7.0).exp());
            return 0.0;
        }

        let min_speed = p.min_speed;
        let full_speed = (min_speed + 1.0).max(p.full_speed);
        let speed01 = clamp((speed - min_speed) / (full_speed - min_speed), 0.0, 1.0);
        let mut target_momentum = smootherstep(speed01);
        if inertia {
            target_momentum = target_momentum.max(clamp(speed / 1.0f64.max(full_speed), 0.0, 1.0));
        }

        let build = p.momentum_build * (if inertia { 1.4 } else { 1.0 });
        let decay = p.momentum_decay;
        let mut momentum = dr.drift_momentum;
        if target_momentum > momentum {
            momentum = target_momentum.min(momentum + build * dt);
        } else {
            momentum = target_momentum.max(momentum - decay * dt);
        }
        dr.drift_momentum = clamp(momentum, 0.0, 1.0);

        let breakaway = p.breakaway;
        if dr.drift_momentum < breakaway {
            dr.drift_phase += dt * (1.2 + speed01 * 1.4);
            dr.drift_boost = (dr.drift_boost - dt * p.boost_decay).max(0.0);
            dr.last_drift_amount = 0.0;
            dr.drift_lean += (0.0 - dr.drift_lean) * (1.0 - (-dt * 5.0).exp());
            return 0.0;
        }

        let slide01 = smootherstep((dr.drift_momentum - breakaway) / 0.001f64.max(1.0 - breakaway));
        dr.drift_phase += dt * (p.wave_speed + speed01 * 1.9);
        dr.drift_flip_timer -= dt;
        if dr.drift_flip_timer <= 0.0 {
            if rng.random() < p.flip_chance {
                dr.drift_dir *= -1.0;
                dr.wiggle_burst = dr.wiggle_burst.max(0.34);
            }
            dr.drift_flip_timer = rand_range(p.switch_time, 1.15, 2.4, rng);
        }

        let base = if inertia { p.throw_slip } else { p.slip };
        let wobble = 0.86 + 0.14 * dr.drift_phase.sin();
        let boost = 1.0 + dr.drift_boost * 0.24;
        let amount = dr.drift_dir * base * mult * slide01 * wobble * boost;
        dr.drift_boost = (dr.drift_boost - dt * p.boost_decay).max(0.0);
        dr.last_drift_amount = clamp(amount, -0.82, 0.82);
        let lean_target = dr.last_drift_amount * (0.55 + 0.45 * slide01);
        dr.drift_lean += (lean_target - dr.drift_lean) * (1.0 - (-dt * 6.5).exp());
        if dr.last_drift_amount.abs() > 0.14 {
            dr.drift_slide_timer = 0.20;
        } else {
            dr.drift_slide_timer = (dr.drift_slide_timer - dt).max(0.0);
        }
        dr.last_drift_amount
    }

    /// `_guard_drifter_travel_direction(move_x, move_y)`: never travel more
    /// than 78 degrees behind the facing during a DriftRun.
    pub fn guard_drifter_travel_direction(&self, move_x: f64, move_y: f64, ctx: &FrameCtx) -> (f64, f64) {
        if ctx.state != MotionState::DriftRun || !ctx.acts_as_drifter {
            return (move_x, move_y);
        }
        let travel_length = hypot(move_x, move_y);
        if travel_length <= 1e-5 {
            return (move_x, move_y);
        }
        let [fx, fy, rx, ry] = self.basis();
        let forward = (move_x * fx + move_y * fy) / travel_length;
        let max_angle = 78.0f64.to_radians();
        if forward >= max_angle.cos() {
            return (move_x, move_y);
        }
        let side = (move_x * rx + move_y * ry) / travel_length;
        let side_sign = if side >= 0.0 { 1.0 } else { -1.0 };
        let corrected_angle = side_sign * max_angle;
        (
            fx * corrected_angle.cos() + rx * corrected_angle.sin(),
            fy * corrected_angle.cos() + ry * corrected_angle.sin(),
        )
    }
}

/// The drift subset of [`crate::config::MotionPersonality`], copied out so
/// `drift_amount` can hold it while mutating the drift state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DriftParams {
    pub min_speed: f64,
    pub full_speed: f64,
    pub momentum_build: f64,
    pub momentum_decay: f64,
    pub breakaway: f64,
    pub wave_speed: f64,
    pub flip_chance: f64,
    pub slip: f64,
    pub throw_slip: f64,
    pub boost_decay: f64,
    pub switch_time: Option<[f64; 2]>,
}

impl crate::config::MotionPersonality {
    pub(crate) fn clone_drift(&self) -> DriftParams {
        DriftParams {
            min_speed: self.drift_min_speed,
            full_speed: self.drift_full_speed,
            momentum_build: self.drift_momentum_build,
            momentum_decay: self.drift_momentum_decay,
            breakaway: self.drift_breakaway,
            wave_speed: self.drift_wave_speed,
            flip_chance: self.drift_flip_chance,
            slip: self.drift_slip,
            throw_slip: self.drift_throw_slip,
            boost_decay: self.drift_boost_decay,
            switch_time: self.drift_switch_time,
        }
    }
}
