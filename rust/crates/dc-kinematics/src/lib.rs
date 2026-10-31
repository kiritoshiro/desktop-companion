//! Leg IK, gait scheduling and grounded-locomotion body solving: the Rust
//! port of `desktop_bug/creature/kinematics/` (plus the kinematic leg
//! geometry of `creature/render_procedural.py`, `render_detail.py` and the
//! drift helpers of `behaviour/temperament.py`).
//!
//! Entry point: [`Kinematics`]. One per creature; it owns the body pose, the
//! legs and every gait clock. Each frame the creature builds a [`FrameCtx`]
//! and calls, in the Python order:
//!
//! ```text
//! kin.aim_at_a_real_screen(&ctx);
//! if kin.spider_grounded_mode_allowed(&ctx) { kin.update_spider_grounded_frame(dt, &ctx, rng) }
//! else { kin.spider_locomotion_active = false; kin.move_body(dt, &ctx, rng) }
//! if kin.reconcile_roll(&ctx) { squash = 1.0 }
//! kin.update_legs(dt, &ctx, rng);
//! ```
//!
//! [`Kinematics::locomotion_frame`] does exactly that sequence.

pub mod chain;
pub mod config;
pub mod constants;
pub mod core;
pub mod detail;
pub mod drift;
pub mod gait;
pub mod geometry;
pub mod legs;
pub mod legstate;
pub mod movement;
pub mod pymath;
pub mod roll;
pub mod spider_gait;
pub mod stepping;

pub use crate::chain::{ChainPoints, DrawCtx};
pub use crate::config::{GaitProfile, LegChainConfig, LegDef, MotionPersonality, Rig, SpiderGaitConfig};
pub use crate::core::{DriftState, FrameCtx, GaitStyle, HeldInput, Kinematics, MotionState, PlayfieldClamp};
pub use crate::legstate::{ContactState, JointBends, LegState};

use dc_core::PyRandom;

/// What [`Kinematics::locomotion_frame`] reports back to the creature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameOutcome {
    /// A roll was finished this frame: set `squash = 1.0`.
    pub roll_finished: bool,
}

impl Kinematics {
    /// The kinematic part of `Creature.update` for a grounded, non-dragged
    /// creature after its state machine has run: aim, body (grounded solve or
    /// legacy movement), roll reconciliation, legs.
    pub fn locomotion_frame(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) -> FrameOutcome {
        if !ctx.airborne {
            self.aim_at_a_real_screen(ctx);
            if self.spider_grounded_mode_allowed(ctx) {
                self.update_spider_grounded_frame(dt, ctx, rng);
            } else {
                if self.spider_locomotion_active {
                    self.spider_locomotion_active = false;
                }
                self.move_body(dt, ctx, rng);
            }
        }
        let roll_finished = self.reconcile_roll(ctx);
        self.update_legs(dt, ctx, rng);
        FrameOutcome { roll_finished }
    }
}
