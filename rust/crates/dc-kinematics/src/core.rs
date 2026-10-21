//! Port of `creature/kinematics/core.py`: the state the kinematics mixin kept
//! on `Creature`, gathered into one struct, plus the lively gait and the
//! feeler probe.
//!
//! In Python the mixin read and wrote `self.*` on the creature. Here
//! [`Kinematics`] *owns* every attribute the kinematics package writes (body
//! pose, speeds, targets, gait clocks, legs, roll, drift and feeler state);
//! the creature crate keeps one `Kinematics` and reads or writes those public
//! fields directly where Python's behaviour code did (`kin.target_x = ...`).
//! What the kinematics only *read* from the rest of the creature arrives each
//! frame in a [`FrameCtx`].

use crate::config::{MAX_LEGS, Rig};
use crate::legstate::LegState;
use crate::pymath::{hypot, wrap_pi};
use dc_core::PyRandom;
use dc_core::math::{clamp, py_mod};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

/// The creature FSM states the kinematics distinguish. Every other state is
/// [`MotionState::Other`]; map with [`MotionState::from_name`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotionState {
    #[default]
    Idle,
    Wander,
    Observe,
    Inspect,
    Alert,
    Approach,
    Chase,
    Retreat,
    Dragged,
    Startled,
    DriftRun,
    JobPatrol,
    Jump,
    Land,
    Roll,
    Feed,
    WebWalk,
    WebApproach,
    Zoom,
    Play,
    Other,
}

impl MotionState {
    pub fn from_name(name: &str) -> Self {
        use MotionState::*;
        match name {
            "Idle" => Idle,
            "Wander" => Wander,
            "Observe" => Observe,
            "Inspect" => Inspect,
            "Alert" => Alert,
            "Approach" => Approach,
            "Chase" => Chase,
            "Retreat" => Retreat,
            "Dragged" => Dragged,
            "Startled" => Startled,
            "DriftRun" => DriftRun,
            "JobPatrol" => JobPatrol,
            "Jump" => Jump,
            "Land" => Land,
            "Roll" => Roll,
            "Feed" => Feed,
            "WebWalk" => WebWalk,
            "WebApproach" => WebApproach,
            "Zoom" => Zoom,
            "Play" => Play,
            _ => Other,
        }
    }

    /// `state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun")`
    #[inline]
    pub fn is_fast(self) -> bool {
        matches!(self, Self::Chase | Self::Retreat | Self::Dragged | Self::Startled | Self::DriftRun)
    }

    /// `state in ("Chase", "Retreat", "Dragged", "Startled")`
    #[inline]
    pub fn is_fast4(self) -> bool {
        matches!(self, Self::Chase | Self::Retreat | Self::Dragged | Self::Startled)
    }

    /// `state in ("Chase", "Retreat", "Startled")`
    #[inline]
    pub fn is_fast3(self) -> bool {
        matches!(self, Self::Chase | Self::Retreat | Self::Startled)
    }
}

/// `gait_style` / `gait_spell` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GaitStyle {
    #[default]
    Classic,
    Lively,
    Skitter,
}

impl GaitStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            GaitStyle::Classic => "classic",
            GaitStyle::Lively => "lively",
            GaitStyle::Skitter => "skitter",
        }
    }
}

/// Clamp to the habitable area of a split playfield (`Playfield.clamp`).
/// Pass `None` in [`FrameCtx::playfield`] when the playfield is `simple`.
/// Any `Fn(x, y, margin) -> (x, y)` closure implements it.
pub trait PlayfieldClamp {
    fn clamp_point(&self, x: f64, y: f64, margin: f64) -> (f64, f64);
}

impl<F: Fn(f64, f64, f64) -> (f64, f64)> PlayfieldClamp for F {
    #[inline]
    fn clamp_point(&self, x: f64, y: f64, margin: f64) -> (f64, f64) {
        self(x, y, margin)
    }
}

/// The carried-pose inputs the creature's drag code maintains
/// (`held_drag_response`, `held_drag_accel_*`, `held_pose_clock`, ...).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldInput {
    pub drag_response: f64,
    pub drag_accel_x: f64,
    pub drag_accel_y: f64,
    pub pose_clock: f64,
    pub drag_sway_x: f64,
    pub drag_sway_y: f64,
    /// `held_leg_relax` (Python default 1.0).
    pub leg_relax: f64,
    pub drag_vel_x: f64,
    pub drag_vel_y: f64,
}

/// What the kinematics read, but do not own, each frame.
#[derive(Clone, Copy, Default)]
pub struct FrameCtx<'a> {
    pub state: MotionState,
    /// `self.fleeing` (DC-50 retreat).
    pub fleeing: bool,
    /// `self._acts_as_observer()`
    pub acts_as_observer: bool,
    /// `self._acts_as_drifter()`
    pub acts_as_drifter: bool,
    /// `getattr(self, "reverse_walk", False)`
    pub reverse_walk: bool,
    /// `getattr(self, "job_facing", None) is not None`
    pub has_job_facing: bool,
    pub airborne: bool,
    pub dragging: bool,
    pub webbed_held: bool,
    pub startled_timer: f64,
    /// `getattr(self, "drift_run_phase", "charge") == "slide"`
    pub drift_run_sliding: bool,
    pub held_release_timer: f64,
    pub focus_strength: f64,
    pub focus_x: f64,
    pub focus_y: f64,
    pub held: HeldInput,
    pub screen_w: f64,
    pub screen_h: f64,
    /// `self.margin` (50.0 on every creature).
    pub margin: f64,
    pub playfield: Option<&'a dyn PlayfieldClamp>,
}

impl std::fmt::Debug for FrameCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCtx")
            .field("state", &self.state)
            .field("dragging", &self.dragging)
            .field("airborne", &self.airborne)
            .field("playfield", &self.playfield.is_some())
            .finish_non_exhaustive()
    }
}

impl FrameCtx<'_> {
    /// A grounded, idle context on a `screen_w` x `screen_h` overlay.
    pub fn new(screen_w: f64, screen_h: f64) -> Self {
        FrameCtx {
            screen_w,
            screen_h,
            margin: 50.0,
            held: HeldInput { leg_relax: 1.0, ..Default::default() },
            ..Default::default()
        }
    }
}

/// Drift (low-grip slide) state, ported with `behaviour/temperament.py`'s
/// drift helpers because the body movement consumes it every frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DriftState {
    pub last_drift_amount: f64,
    pub drift_momentum: f64,
    pub drift_lean: f64,
    pub drift_phase: f64,
    pub drift_boost: f64,
    pub drift_flip_timer: f64,
    pub drift_dir: f64,
    pub drift_slide_timer: f64,
    /// Shared with the expression code; drift only ever raises it.
    pub wiggle_burst: f64,
}

impl Default for DriftState {
    fn default() -> Self {
        DriftState {
            last_drift_amount: 0.0,
            drift_momentum: 0.0,
            drift_lean: 0.0,
            drift_phase: 0.0,
            drift_boost: 0.0,
            drift_flip_timer: 0.0,
            drift_dir: 1.0,
            drift_slide_timer: 0.0,
            wiggle_burst: 0.0,
        }
    }
}

/// A leg's angular wedge, `(lo, hi, rmin_mult, rmax_mult)` in the Python.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    pub lo: f64,
    pub hi: f64,
    pub rmin_m: f64,
    pub rmax_m: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReachCache {
    pub size: f64,
    pub values: [f64; 2],
}

impl Default for ReachCache {
    fn default() -> Self {
        ReachCache { size: f64::NAN, values: [0.0; 2] }
    }
}

/// The creature's kinematic state. Field names are the Python attribute
/// names without their leading underscore.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Kinematics {
    // --- body pose and locomotion intent -------------------------------
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub target_heading: f64,
    pub target_x: f64,
    pub target_y: f64,
    pub current_speed: f64,
    /// Desired speed, set by behaviour.
    pub speed: f64,
    pub vel_x: f64,
    pub vel_y: f64,
    pub turn_rate: f64,
    /// Body size in pixels; growth writes it directly.
    pub size: f64,
    pub motion_paused: bool,
    // --- body animation ------------------------------------------------
    pub body_bob: f64,
    pub body_sway: f64,
    pub bob_phase: f64,
    pub breath_phase: f64,
    pub abdomen_pulse: f64,
    pub ceph_pulse: f64,
    pub turn_rehome_pressure: f64,
    pub strafe_observe: bool,
    // --- a throw -------------------------------------------------------
    pub inertia_timer: f64,
    pub inertia_vx: f64,
    pub inertia_vy: f64,
    // --- feeler probe and catch reach ----------------------------------
    pub catch_point: (f64, f64),
    pub catch_blend: f64,
    pub inspect_intent: f64,
    pub feeler_clock: f64,
    pub feeler_pulse: f64,
    pub feeler_pulse_dur: f64,
    pub feeler_pulse_t: f64,
    // --- gait ----------------------------------------------------------
    pub gait_style: GaitStyle,
    pub gait_spell: Option<GaitStyle>,
    pub gait_spell_timer: f64,
    pub lively_gait_phase: f64,
    pub skitter_burst_timer: f64,
    pub skitter_pause_timer: f64,
    pub skitter_phase: f64,
    pub skitter_burst_jitter: f64,
    pub prev_heading_gait: f64,
    pub spider_heading_filter: f64,
    pub spider_locomotion_active: bool,
    pub spider_gait_frame_updated: bool,
    pub turn_speed_smooth: f64,
    pub idle_twitch_timer: f64,
    pub spider_solver_dt: f64,
    pub spider_turn_speed: f64,
    // --- roll ----------------------------------------------------------
    pub rolling: bool,
    pub roll_spin: f64,
    pub roll_tuck: f64,
    pub roll_progress: f64,
    pub roll_eased: f64,
    pub roll_duration: f64,
    pub roll_dir: f64,
    pub roll_total: f64,
    pub roll_distance: f64,
    pub roll_drift_dir: f64,
    // --- drift ---------------------------------------------------------
    #[serde(flatten)]
    pub drift: DriftState,
    // --- legs ----------------------------------------------------------
    pub legs: Vec<LegState>,
    // --- static config and caches (not part of the snapshot) -----------
    #[serde(skip)]
    pub rig: Rig,
    /// The dedicated DC-63 gait-spell stream (`_gait_rng`).
    #[serde(skip)]
    pub gait_rng: PyRandom,
    #[serde(skip)]
    pub(crate) sectors: Vec<Option<Sector>>,
    /// `sorted({gait_group}) or [0]`
    #[serde(skip)]
    pub gait_groups: Vec<i64>,
    #[serde(skip, default = "fresh_basis_cache")]
    pub(crate) basis_cache: Cell<(f64, [f64; 4])>,
    #[serde(skip)]
    pub(crate) reach_cache: Vec<Cell<ReachCache>>,
}

impl Default for Kinematics {
    fn default() -> Self {
        Kinematics::new(Rig::default(), GaitStyle::Classic, PyRandom::from_int(0), 0.0, 0.0, 0.0, 28.0)
    }
}

fn fresh_basis_cache() -> Cell<(f64, [f64; 4])> {
    Cell::new((f64::NAN, [1.0, 0.0, 0.0, 1.0]))
}

/// Fixed-capacity, allocation-free candidate list for one scheduling pass.
#[derive(Clone, Copy)]
pub(crate) struct Candidates<T: Copy> {
    len: usize,
    items: [(f64, T); MAX_LEGS],
}

impl<T: Copy + Default> Candidates<T> {
    #[inline]
    pub fn new() -> Self {
        Candidates { len: 0, items: [(0.0, T::default()); MAX_LEGS] }
    }
    #[inline]
    pub fn push(&mut self, key: f64, item: T) {
        if self.len < MAX_LEGS {
            self.items[self.len] = (key, item);
            self.len += 1;
        }
    }
    /// `list.sort(reverse=True, key=...)`: descending, stable for equal keys.
    #[inline]
    pub fn sort_desc(&mut self) {
        for i in 1..self.len {
            let cur = self.items[i];
            let mut j = i;
            while j > 0 && self.items[j - 1].0 < cur.0 {
                self.items[j] = self.items[j - 1];
                j -= 1;
            }
            self.items[j] = cur;
        }
    }
    #[inline]
    pub fn as_slice(&self) -> &[(f64, T)] {
        &self.items[..self.len]
    }
}

/// Small set of leg indices (`used = set()` in the Python).
#[derive(Clone, Copy, Default)]
pub(crate) struct IndexSet {
    bits: u32,
}

impl IndexSet {
    #[inline]
    pub fn insert(&mut self, i: usize) {
        self.bits |= 1 << i;
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }
    /// `any(abs(i - j) in (1, 2) for j in used)`
    #[inline]
    pub fn has_neighbour_of(&self, i: usize) -> bool {
        let mut b = self.bits;
        while b != 0 {
            let j = b.trailing_zeros() as usize;
            let d = i.abs_diff(j);
            if d == 1 || d == 2 {
                return true;
            }
            b &= b - 1;
        }
        false
    }
}

impl Kinematics {
    /// A creature's kinematics before any random draws: the Python
    /// constructor's assignments, `_compute_leg_sectors()`, and the one draw
    /// from the gait-spell stream (`gait_spell_timer`).
    ///
    /// The creature constructor must then, in its own draw order:
    /// [`Kinematics::seed_legs`] (line 321 of `creature/core.py`), draw its
    /// `active_gait_index`, call [`Kinematics::seed_gait_clocks`] (lines
    /// 344-369), set `turn_rate`, `bob_phase`, `breath_phase` and the drift
    /// fields it draws earlier, and finally [`Kinematics::initialize_legs`].
    pub fn new(
        rig: Rig,
        gait_style: GaitStyle,
        mut gait_rng: PyRandom,
        x: f64,
        y: f64,
        heading: f64,
        size: f64,
    ) -> Self {
        let n = rig.legs.len();
        let mut groups: Vec<i64> = rig.legs.iter().map(|l| l.gait_group).collect();
        groups.sort_unstable();
        groups.dedup();
        if groups.is_empty() {
            groups.push(0);
        }
        let gait_spell_timer = gait_rng.uniform(crate::constants::GAIT_SPELL_GAP.0, crate::constants::GAIT_SPELL_GAP.1);
        let mut k = Kinematics {
            x,
            y,
            heading,
            target_heading: heading,
            target_x: x,
            target_y: y,
            current_speed: 0.0,
            speed: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            turn_rate: 5.0,
            size,
            motion_paused: false,
            body_bob: 0.0,
            body_sway: 0.0,
            bob_phase: 0.0,
            breath_phase: 0.0,
            abdomen_pulse: 0.0,
            ceph_pulse: 0.0,
            turn_rehome_pressure: 0.0,
            strafe_observe: false,
            inertia_timer: 0.0,
            inertia_vx: 0.0,
            inertia_vy: 0.0,
            catch_point: (x, y),
            catch_blend: 0.0,
            inspect_intent: 0.0,
            feeler_clock: 1.0,
            feeler_pulse: 0.0,
            feeler_pulse_dur: 0.0,
            feeler_pulse_t: 0.0,
            gait_style,
            gait_spell: None,
            gait_spell_timer,
            lively_gait_phase: 0.0,
            skitter_burst_timer: 0.2,
            skitter_pause_timer: 0.0,
            skitter_phase: 0.0,
            skitter_burst_jitter: 1.0,
            prev_heading_gait: heading,
            spider_heading_filter: heading,
            spider_locomotion_active: false,
            spider_gait_frame_updated: false,
            turn_speed_smooth: 0.0,
            idle_twitch_timer: 1.0,
            spider_solver_dt: 0.016,
            spider_turn_speed: 0.0,
            rolling: false,
            roll_spin: 0.0,
            roll_tuck: 0.0,
            roll_progress: 0.0,
            roll_eased: 0.0,
            roll_duration: 0.0,
            roll_dir: 1.0,
            roll_total: 0.0,
            roll_distance: 0.0,
            roll_drift_dir: 0.0,
            drift: DriftState::default(),
            legs: vec![LegState::default(); n],
            rig,
            gait_rng,
            sectors: vec![None; n],
            gait_groups: groups,
            basis_cache: fresh_basis_cache(),
            reach_cache: vec![Cell::new(ReachCache::default()); n],
        };
        k.compute_leg_sectors();
        k
    }

    /// `leg.phase_seed = rng.random() * tau; leg.gait_phase_offset = rng.uniform(-0.42, 0.42)` per leg.
    pub fn seed_legs(&mut self, rng: &mut PyRandom) {
        for leg in &mut self.legs {
            leg.phase_seed = rng.random() * std::f64::consts::TAU;
            leg.gait_phase_offset = rng.uniform(-0.42, 0.42);
        }
    }

    /// The constructor's gait clock draws, in order: `_lively_gait_phase`,
    /// `_skitter_burst_timer`, `_skitter_phase`, `_skitter_burst_jitter`,
    /// `_feeler_clock`, `idle_twitch_timer`. Also resets the heading filters
    /// to the current heading, as the constructor does.
    pub fn seed_gait_clocks(&mut self, rng: &mut PyRandom) {
        self.lively_gait_phase = rng.random();
        self.skitter_burst_timer = rng.uniform(0.14, 0.34);
        self.skitter_pause_timer = 0.0;
        self.skitter_phase = rng.random() * std::f64::consts::TAU;
        self.skitter_burst_jitter = rng.uniform(0.94, 1.16);
        self.prev_heading_gait = self.heading;
        self.spider_heading_filter = self.heading;
        self.feeler_clock = rng.uniform(0.4, 1.4);
        self.idle_twitch_timer = rng.uniform(0.6, 1.8);
    }

    /// Replace the snapshot fields from a JSON object of `name: value`
    /// overrides (the serialised field names; legs as a list of partial leg
    /// objects). Static config, the gait stream and the sectors are kept;
    /// caches are rebuilt. Used by the parity fixtures and save/restore.
    pub fn apply_json_overrides(&mut self, patch: &serde_json::Value) -> Result<(), serde_json::Error> {
        let mut value = serde_json::to_value(&*self)?;
        merge_json(&mut value, patch);
        let mut next: Kinematics = serde_json::from_value(value)?;
        next.rig = std::mem::take(&mut self.rig);
        next.gait_rng = self.gait_rng.clone();
        next.sectors = std::mem::take(&mut self.sectors);
        next.gait_groups = std::mem::take(&mut self.gait_groups);
        next.reach_cache = vec![Cell::new(ReachCache::default()); next.legs.len()];
        next.basis_cache = fresh_basis_cache();
        next.sectors.resize(next.legs.len(), None);
        *self = next;
        Ok(())
    }

    #[inline]
    pub fn spider_gait(&self) -> Option<&crate::config::SpiderGaitConfig> {
        self.rig.spider_gait.as_ref()
    }

    /// `_walks_while_facing_elsewhere`: an Observer strafing, the player
    /// backing up, or a guard on station holding its facing.
    pub fn walks_while_facing_elsewhere(&self, ctx: &FrameCtx) -> bool {
        if ctx.state == MotionState::Observe && ctx.acts_as_observer {
            return true;
        }
        if ctx.reverse_walk {
            return true;
        }
        ctx.state == MotionState::JobPatrol && ctx.has_job_facing
    }

    /// `_update_legs_lively`: the visible alternating-tetrapod gait.
    pub fn update_legs_lively(&mut self, dt: f64, ctx: &FrameCtx, rng: &mut PyRandom) {
        if self.rig.spider_gait.is_some() {
            if self.spider_gait_frame_updated {
                self.spider_gait_frame_updated = false;
                return;
            }
            self.update_spider_gait(dt, ctx, rng);
            return;
        }
        if ctx.airborne {
            return;
        }
        self.advance_active_steps(dt, ctx, rng);

        let dheading = wrap_pi(self.heading - self.prev_heading_gait);
        self.prev_heading_gait = self.heading;
        let turn_speed = dheading.abs() / dt.max(1e-3);
        let turn_err = wrap_pi(self.target_heading - self.heading).abs();

        let moving = self.current_speed > 4.0;
        let turning = turn_speed > 0.30 || turn_err > 0.12;
        let skitter = self.uses_skitter_gait(ctx);
        let spider_gait = self.rig.spider_gait.as_ref().map(|g| (g.max_airborne, g.swing_fraction));
        let hold_still = (self.current_speed < (if skitter { 4.5 } else { 3.0 })) && !turning && !ctx.dragging;
        let speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0);
        let sliding = self.is_drift_sliding(ctx);
        let fast_state = ctx.state.is_fast();

        let mut max_air: usize = if skitter { 3 } else { 2 };
        if moving && speed01 >= (if skitter { 0.34 } else { 0.5 }) {
            max_air = if skitter { 4 } else { 3 };
        }
        if fast_state || ctx.dragging {
            max_air = if skitter { 5 } else { 4 };
        }
        if turning {
            max_air = max_air.max(if skitter { 4 } else { 3 });
        }
        if let Some((cap, _)) = spider_gait {
            max_air = max_air.min(cap);
        }
        let mut swinging_now = self.legs.iter().filter(|l| l.airborne()).count();

        // Emergency repair.
        let mut broken: Candidates<(usize, f64, f64)> = Candidates::new();
        for i in 0..self.legs.len() {
            let (fx, fy) = (self.legs[i].foot_x, self.legs[i].foot_y);
            let a = self.leg_alignment_metrics(i, fx, fy);
            let r = self.leg_reach_metrics(i, fx, fy, false);
            let is_broken = a.severe_wrong || (if hold_still { r.very_far } else { r.too_far });
            let leg = &self.legs[i];
            if is_broken && !leg.stepping && !leg.pending_step && leg.step_cooldown <= 0.0 {
                let (ix, iy) = self.leg_ideal_foot(i, ctx);
                let dist = hypot(fx - ix, fy - iy) + (if r.very_far { self.size * 0.6 } else { 0.0 });
                broken.push(dist, (i, ix, iy));
            }
        }
        broken.sort_desc();
        let take = max_air.saturating_sub(swinging_now);
        for &(_, (i, ix, iy)) in broken.as_slice().iter().take(take) {
            self.schedule_step(i, ix, iy, 0.0, true, ctx, rng);
            self.legs[i].step_cooldown = if skitter { rng.uniform(0.010, 0.032) } else { rng.uniform(0.02, 0.05) };
            swinging_now += 1;
        }

        self.update_feelers(dt, moving, turning, ctx, rng);

        if hold_still {
            return;
        }

        let mut cadence_hz = if skitter { 2.85 + speed01 * 6.2 } else { 1.05 + speed01 * 3.0 };
        if fast_state {
            cadence_hz += if skitter { 2.4 } else { 1.2 };
        }
        if turning {
            cadence_hz = cadence_hz
                .max((if skitter { 1.65 } else { 0.9 }) + turn_speed.min(5.5) * (if skitter { 0.86 } else { 0.55 }));
        }
        self.lively_gait_phase = py_mod(self.lively_gait_phase + dt * cadence_hz, 1.0);

        let swing_frac = match spider_gait {
            Some((_, frac)) => frac,
            None => {
                if skitter {
                    0.31
                } else {
                    0.40
                }
            }
        };
        let group_count = self.gait_groups.len().max(1);
        let mut replant_thresh = self.size
            * (if skitter {
                if moving || sliding { 0.095 } else { 0.080 }
            } else if moving || sliding {
                0.16
            } else {
                0.12
            });
        if fast_state {
            replant_thresh *= if skitter { 0.72 } else { 0.8 };
        }
        if turning {
            replant_thresh = replant_thresh.min(self.size * (if skitter { 0.065 } else { 0.10 }));
        }

        let mut candidates: Candidates<(usize, f64, f64)> = Candidates::new();
        for i in 0..self.legs.len() {
            let leg = self.legs[i];
            if leg.stepping || leg.pending_step || leg.step_cooldown > 0.0 {
                continue;
            }
            let group = self.rig.legs[i].gait_group;
            let g = if group_count >= 2 { group.rem_euclid(2) } else { 0 };
            let window_pos = py_mod(self.lively_gait_phase - 0.5 * g as f64, 1.0);
            let in_window = window_pos < swing_frac;
            let (ix, iy) = self.leg_ideal_foot(i, ctx);
            let dist = hypot(leg.foot_x - ix, leg.foot_y - iy);
            let a = self.leg_alignment_metrics(i, leg.foot_x, leg.foot_y);
            let r = self.leg_reach_metrics(i, leg.foot_x, leg.foot_y, false);
            let urgent = a.wrong_side || r.too_far;
            if !(urgent || dist > replant_thresh) {
                continue;
            }
            let mut score = dist / replant_thresh.max(1.0);
            if urgent {
                score += 3.0 + (if a.severe_wrong || r.very_far { 1.5 } else { 0.0 });
            } else if in_window {
                score += if skitter { 1.8 } else { 1.4 };
            } else {
                if score < (if skitter { 1.08 } else { 1.25 }) {
                    continue;
                }
                score *= if skitter { 0.74 } else { 0.6 };
            }
            candidates.push(score, (i, ix, iy));
        }

        candidates.sort_desc();
        let mut slots = max_air.saturating_sub(swinging_now);
        let mut used = IndexSet::default();
        for &(_, (i, ix, iy)) in candidates.as_slice() {
            if slots == 0 {
                break;
            }
            if used.has_neighbour_of(i) {
                continue;
            }
            let (delay, cooldown) = if skitter {
                let d = rng.uniform(0.0, if fast_state { 0.010 } else { 0.020 });
                (d, rng.uniform(0.012, 0.044))
            } else {
                let d = rng.uniform(0.0, if fast_state { 0.02 } else { 0.05 });
                (d, rng.uniform(0.04, 0.10))
            };
            self.schedule_step(i, ix, iy, delay, fast_state || skitter, ctx, rng);
            self.legs[i].step_cooldown = cooldown;
            used.insert(i);
            slots -= 1;
        }

        if !used.is_empty() {
            self.turn_rehome_pressure = (self.turn_rehome_pressure - 0.18).max(0.0);
        }
    }

    /// `_update_feelers`: reach a front leg out to tap the focus and wave the
    /// palps, by raising floors on `catch_blend` and `inspect_intent`.
    pub fn update_feelers(&mut self, dt: f64, _moving: bool, _turning: bool, ctx: &FrameCtx, rng: &mut PyRandom) {
        use MotionState::*;
        let off = |k: &mut Kinematics| {
            k.feeler_pulse = 0.0;
            k.feeler_pulse_t = 0.0;
        };
        if self.rig.tarantula_hand_palps {
            off(self);
            return;
        }
        if !matches!(ctx.state, Idle | Wander | Observe | Inspect | Alert | Approach) {
            off(self);
            return;
        }
        if self.current_speed > 70.0 {
            off(self);
            return;
        }
        let focus = clamp(ctx.focus_strength, 0.0, 1.0);
        let attentive = matches!(ctx.state, Observe | Inspect);
        if focus < 0.25 && !attentive {
            off(self);
            return;
        }
        let d = hypot(ctx.focus_x - self.x, ctx.focus_y - self.y);
        if d > self.size * 7.0 && !attentive {
            off(self);
            return;
        }

        if self.feeler_pulse_t > 0.0 {
            self.feeler_pulse_t = (self.feeler_pulse_t - dt).max(0.0);
            let phase = 1.0 - (self.feeler_pulse_t / self.feeler_pulse_dur.max(1e-3));
            self.feeler_pulse = (std::f64::consts::PI * clamp(phase, 0.0, 1.0)).sin();
        } else {
            self.feeler_pulse = 0.0;
            self.feeler_clock -= dt;
            if self.feeler_clock <= 0.0 {
                self.feeler_pulse_dur = rng.uniform(0.45, 0.85);
                self.feeler_pulse_t = self.feeler_pulse_dur;
                self.feeler_clock = rng.uniform(0.5, 1.5);
            }
        }

        if self.feeler_pulse > 0.001 {
            self.catch_point = (ctx.focus_x, ctx.focus_y);
            self.catch_blend = self.catch_blend.max(self.feeler_pulse * 0.5);
            self.inspect_intent = self.inspect_intent.max(self.feeler_pulse * 0.9);
        }
    }
}

/// Recursive JSON merge used by [`Kinematics::apply_json_overrides`]: objects
/// merge key by key, arrays of objects merge element by element, anything
/// else replaces.
pub fn merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                match t.get_mut(k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        t.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (Value::Array(t), Value::Array(p)) if p.iter().all(Value::is_object) && t.iter().all(Value::is_object) => {
            for (i, v) in p.iter().enumerate() {
                if i < t.len() {
                    merge_json(&mut t[i], v);
                } else {
                    t.push(v.clone());
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}
