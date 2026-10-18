//! Port of `creature/kinematics/roll.py`: rolling, leaving it cleanly and
//! putting the feet back down.
//!
//! Two side effects of the Python live on the creature rather than here and
//! are reported instead: finishing a roll sets `squash = 1.0`, and a roll
//! that runs to completion calls `enter_idle()`. Both functions that can
//! finish a roll return `true` when the caller must do those.

use crate::core::{FrameCtx, Kinematics, MotionState};
use crate::pymath::py_pow;
use dc_core::math::{clamp, clamp_point};
use std::f64::consts::PI;

impl Kinematics {
    /// `_finish_roll()`. The caller must also set `squash = 1.0`.
    pub fn finish_roll(&mut self, ctx: &FrameCtx) {
        self.rolling = false;
        self.roll_spin = 0.0;
        self.roll_tuck = 0.0;
        self.roll_progress = 1.0;
        self.motion_paused = false;
        self.reset_roll_contacts(ctx);
        self.spider_locomotion_active = false;
    }

    /// `_reconcile_roll()`: tidy up after a tumble something cut short.
    /// Returns `true` when it finished a roll (set `squash = 1.0`).
    pub fn reconcile_roll(&mut self, ctx: &FrameCtx) -> bool {
        if self.rolling && ctx.state != MotionState::Roll {
            self.finish_roll(ctx);
            return true;
        }
        false
    }

    /// `_reset_roll_contacts()`: every foot back to a safe, bounded stance.
    pub fn reset_roll_contacts(&mut self, ctx: &FrameCtx) {
        for i in 0..self.legs.len() {
            let (fx, fy) = self.leg_ideal_foot(i, ctx);
            let (fx, fy) = self.constrain_leg_point(i, fx, fy);
            let leg = &mut self.legs[i];
            leg.foot_x = fx;
            leg.foot_y = fy;
            leg.step_start_x = fx;
            leg.step_start_y = fy;
            leg.step_target_x = fx;
            leg.step_target_y = fy;
            leg.step_control_x = fx;
            leg.step_control_y = fy;
            leg.pending_target_x = fx;
            leg.pending_target_y = fy;
            leg.pending_delay = 0.0;
            leg.step_timer = 0.0;
            leg.stepping = false;
            leg.pending_step = false;
            leg.lift = 0.0;
            leg.joint_bends.clear();
            leg.held_spring_x = 0.0;
            leg.held_spring_y = 0.0;
            leg.held_spring_vx = 0.0;
            leg.held_spring_vy = 0.0;
            self.spider_reanchor_stance(i);
        }
        self.spider_heading_filter = self.heading;
    }

    /// `_update_roll(dt, mx, my)`. Returns `true` when the roll completed:
    /// the caller must set `squash = 1.0` and call `enter_idle()`.
    pub fn update_roll(&mut self, dt: f64, ctx: &FrameCtx) -> bool {
        self.motion_paused = true;
        self.speed = 0.0;
        self.current_speed = 0.0;
        self.vel_x = 0.0;
        self.vel_y = 0.0;
        let prev_eased = self.roll_eased;
        self.roll_progress = 1.0f64.min(self.roll_progress + dt / 0.05f64.max(self.roll_duration));
        self.roll_eased = 1.0 - py_pow(1.0 - self.roll_progress, 2.0);
        let d_eased = self.roll_eased - prev_eased;
        self.roll_spin = self.roll_dir * self.roll_total * self.roll_eased;
        self.roll_tuck = py_pow((PI * clamp(self.roll_progress, 0.0, 1.0)).sin(), 0.7);

        let dist = self.roll_distance * d_eased;
        let ndx = self.roll_drift_dir.cos() * dist;
        let ndy = self.roll_drift_dir.sin() * dist;
        let (nx, ny) = clamp_point(self.x + ndx, self.y + ndy, ctx.margin, ctx.screen_w, ctx.screen_h);
        let (adx, ady) = (nx - self.x, ny - self.y);
        self.x = nx;
        self.y = ny;
        self.translate_leg_world_points(adx, ady, 1.0);
        self.target_x = self.x;
        self.target_y = self.y;
        self.target_heading = self.heading;

        if self.roll_progress >= 1.0 {
            self.finish_roll(ctx);
            return true;
        }
        false
    }
}
