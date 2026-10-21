//! Port of `creature/kinematics/legstate.py`: one leg's live state.
//!
//! The Python dataclass also carries the leg's `definition` dict; here the
//! definition lives in [`crate::config::Rig::legs`] at the same index, so the
//! hot state stays small and `Copy`.

use crate::config::MAX_JOINTS;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `leg.contact_state`: `"stance"` or `"swing"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContactState {
    #[default]
    Stance,
    Swing,
}

/// `leg.joint_bends`: a short list (0 to 4 entries) kept inline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointBends {
    len: u8,
    values: [f64; MAX_JOINTS],
}

impl JointBends {
    pub const EMPTY: JointBends = JointBends { len: 0, values: [0.0; MAX_JOINTS] };

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }
    #[inline]
    pub fn as_slice(&self) -> &[f64] {
        &self.values[..self.len as usize]
    }
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [f64] {
        &mut self.values[..self.len as usize]
    }
    /// Replace with `values` (at most [`MAX_JOINTS`] are kept).
    pub fn set(&mut self, values: &[f64]) {
        let n = values.len().min(MAX_JOINTS);
        self.values[..n].copy_from_slice(&values[..n]);
        self.len = n as u8;
    }
    /// Resize to `n` entries filled by `f(index)`.
    #[inline]
    pub fn fill_with(&mut self, n: usize, f: impl Fn(usize) -> f64) {
        let n = n.min(MAX_JOINTS);
        for (i, v) in self.values[..n].iter_mut().enumerate() {
            *v = f(i);
        }
        self.len = n as u8;
    }
}

impl Serialize for JointBends {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(s)
    }
}

impl<'de> Deserialize<'de> for JointBends {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: Vec<f64> = Vec::deserialize(d)?;
        let mut out = JointBends::EMPTY;
        out.set(&v);
        Ok(out)
    }
}

/// `LegState`: field names and defaults are the dataclass's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegState {
    pub foot_x: f64,
    pub foot_y: f64,
    pub step_start_x: f64,
    pub step_start_y: f64,
    pub step_target_x: f64,
    pub step_target_y: f64,
    pub step_control_x: f64,
    pub step_control_y: f64,
    pub pending_target_x: f64,
    pub pending_target_y: f64,
    pub pending_delay: f64,
    pub step_timer: f64,
    pub step_duration: f64,
    pub stepping: bool,
    pub pending_step: bool,
    pub lift: f64,
    /// Randomised per instance by the creature from its own generator.
    pub phase_seed: f64,
    pub twitch_clock: f64,
    pub gait_phase_offset: f64,
    pub step_cooldown: f64,
    // Grounded locomotion state.
    pub contact_state: ContactState,
    pub support_weight: f64,
    pub contact_age: f64,
    pub stroke_progress: f64,
    pub stance_base_f: f64,
    pub stance_base_s: f64,
    pub stance_stroke_f: f64,
    pub stance_stroke_s: f64,
    pub stance_turn: f64,
    pub last_step_phase: f64,
    pub last_step_emergency: bool,
    pub joint_phase: f64,
    pub joint_bends: JointBends,
    // Suspended/carry pose dynamics (screen-space spring offsets).
    pub held_spring_x: f64,
    pub held_spring_y: f64,
    pub held_spring_vx: f64,
    pub held_spring_vy: f64,
}

impl Default for LegState {
    fn default() -> Self {
        LegState {
            foot_x: 0.0,
            foot_y: 0.0,
            step_start_x: 0.0,
            step_start_y: 0.0,
            step_target_x: 0.0,
            step_target_y: 0.0,
            step_control_x: 0.0,
            step_control_y: 0.0,
            pending_target_x: 0.0,
            pending_target_y: 0.0,
            pending_delay: 0.0,
            step_timer: 0.0,
            step_duration: 0.24,
            stepping: false,
            pending_step: false,
            lift: 0.0,
            phase_seed: 0.0,
            twitch_clock: 0.0,
            gait_phase_offset: 0.0,
            step_cooldown: 0.0,
            contact_state: ContactState::Stance,
            support_weight: 1.0,
            contact_age: 0.0,
            stroke_progress: 0.0,
            stance_base_f: 0.0,
            stance_base_s: 0.0,
            stance_stroke_f: 0.0,
            stance_stroke_s: 0.0,
            stance_turn: 0.0,
            last_step_phase: 0.0,
            last_step_emergency: false,
            joint_phase: 0.0,
            joint_bends: JointBends::EMPTY,
            held_spring_x: 0.0,
            held_spring_y: 0.0,
            held_spring_vx: 0.0,
            held_spring_vy: 0.0,
        }
    }
}

impl LegState {
    /// `leg.stepping or leg.pending_step`
    #[inline]
    pub fn airborne(&self) -> bool {
        self.stepping || self.pending_step
    }
}
