//! Static per-creature configuration: the leg rig, the optional spider gait
//! and leg chain blocks from `appearance`, and the personality values the
//! locomotion reads.
//!
//! The builders take `serde_json::Value` so `dc-creature` can hand over the
//! model's `legs` array and `appearance` object unchanged (whatever
//! `dc-content` parsed). Field names are the JSON keys. Every default and
//! every clamp is the one in the Python:
//!
//! * [`SpiderGaitConfig::from_json`] is `GaitConfigMixin._build_spider_gait_config`
//!   (`creature/kinematics/gait.py`).
//! * [`LegChainConfig::from_json`] is `_sprite_leg_chain_config`
//!   (`creature/render_procedural.py`), the chain geometry this crate solves.
//! * [`LegDef::from_json`] keeps which keys are *present*, because the
//!   Python branches on `"rest_forward" in d` and uses different defaults for
//!   the same key in different methods.

use crate::pymath::{get_float, py_float, py_float_list, py_int, py_str, py_truthy};
use dc_core::math::clamp;
use serde_json::Value;

/// Upper bound on legs per creature. Every shipped model has eight; the
/// scheduler keeps its per-frame candidate lists in fixed arrays of this size.
pub const MAX_LEGS: usize = 16;
/// Joints in the longest supported leg chain (`segment_count` 5).
pub const MAX_JOINTS: usize = 4;
/// Points in the longest supported leg chain (root, four joints, foot).
pub const MAX_CHAIN_POINTS: usize = 6;

/// One entry of the model's `legs` array.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegDef {
    pub name: String,
    /// The raw `side` value as `str(...)`; `"right"` when absent.
    pub side: String,
    /// `-1` when `side` starts with `l` (case-insensitive), else `+1`.
    pub side_sign: f64,
    /// First character of the lower-cased side, for the sector builder.
    pub side_initial: Option<char>,
    pub gait_group: i64,
    pub phase_offset: Option<f64>,
    pub attach_angle: Option<f64>,
    pub rest_angle: Option<f64>,
    pub reach: Option<f64>,
    pub upper_len: Option<f64>,
    pub lower_len: Option<f64>,
    pub attach_forward: Option<f64>,
    pub attach_side: Option<f64>,
    pub rest_forward: Option<f64>,
    pub rest_side: Option<f64>,
    pub rest_jitter: Option<f64>,
    pub stride_forward: Option<f64>,
    pub coxa_len: Option<f64>,
}

impl LegDef {
    /// Build from one JSON leg object. Non-numeric values are treated as
    /// absent (the Python would raise on them at first use).
    pub fn from_json(value: &Value) -> Self {
        let num = |key: &str| value.get(key).and_then(py_float);
        let side = value.get("side").map(py_str).unwrap_or_else(|| "right".to_string());
        let lower = side.to_lowercase();
        let side_sign = if lower.starts_with('l') { -1.0 } else { 1.0 };
        LegDef {
            name: value.get("name").map(py_str).unwrap_or_default(),
            side_initial: lower.chars().next(),
            side,
            side_sign,
            gait_group: value.get("gait_group").and_then(py_int).unwrap_or(0),
            phase_offset: num("phase_offset"),
            attach_angle: num("attach_angle"),
            rest_angle: num("rest_angle"),
            reach: num("reach"),
            upper_len: num("upper_len"),
            lower_len: num("lower_len"),
            attach_forward: num("attach_forward"),
            attach_side: num("attach_side"),
            rest_forward: num("rest_forward"),
            rest_side: num("rest_side"),
            rest_jitter: num("rest_jitter"),
            stride_forward: num("stride_forward"),
            coxa_len: num("coxa_len"),
        }
    }

    /// `"rest_forward" in d or "rest_side" in d`
    #[inline]
    pub fn has_rest(&self) -> bool {
        self.rest_forward.is_some() || self.rest_side.is_some()
    }

    /// `"attach_forward" in d or "attach_side" in d`
    #[inline]
    pub fn has_attach(&self) -> bool {
        self.attach_forward.is_some() || self.attach_side.is_some()
    }

    #[inline]
    pub fn reach_or(&self) -> f64 {
        self.reach.unwrap_or(1.8)
    }
    #[inline]
    pub fn upper_len_or(&self) -> f64 {
        self.upper_len.unwrap_or(0.85)
    }
    #[inline]
    pub fn lower_len_or(&self) -> f64 {
        self.lower_len.unwrap_or(1.05)
    }
    #[inline]
    pub fn attach_forward_or(&self) -> f64 {
        self.attach_forward.unwrap_or(0.0)
    }
    #[inline]
    pub fn attach_side_or(&self) -> f64 {
        self.attach_side.unwrap_or(0.30)
    }
    #[inline]
    pub fn rest_forward_or(&self) -> f64 {
        self.rest_forward.unwrap_or(0.0)
    }
    #[inline]
    pub fn rest_side_or(&self) -> f64 {
        self.rest_side.unwrap_or(1.0)
    }
}

/// `spider_gait.profile`. Only `chosen_one` and `tarantula` change behaviour
/// (a metachronal wave instead of tetrapod groups); the rest are labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaitProfile {
    #[default]
    Tetrapod,
    Bug,
    Segmented,
    Jumper,
    ChosenOne,
    Tarantula,
    Other,
}

impl GaitProfile {
    pub fn from_name(name: &str) -> Self {
        match name {
            "tetrapod" => Self::Tetrapod,
            "bug" => Self::Bug,
            "segmented" => Self::Segmented,
            "jumper" => Self::Jumper,
            "chosen_one" => Self::ChosenOne,
            "tarantula" => Self::Tarantula,
            _ => Self::Other,
        }
    }
}

/// The sanitised `appearance.spider_gait` block. `Copy`, so the hot path can
/// hold it by value while it mutates the creature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpiderGaitConfig {
    pub profile: GaitProfile,
    /// `profile in ("chosen_one", "tarantula")`
    pub metachronal: bool,
    pub max_airborne: usize,
    pub swing_arc_outward: f64,
    pub swing_arc_forward: f64,
    pub swing_height: f64,
    pub front_stride_bias: f64,
    pub stride_gain: f64,
    pub swing_fraction: f64,
    pub cycle_hz: f64,
    pub speed_cycle_gain: f64,
    pub stride_length: f64,
    pub stride_speed_gain: f64,
    pub max_cycle_hz: f64,
    pub min_swing_time: f64,
    pub step_lookahead: f64,
    pub step_trigger: f64,
    pub stance_deadband: f64,
    pub front_phase_offset: f64,
    pub refractory_fraction: f64,
    pub support_stroke_limit: f64,
    pub support_turn_limit: f64,
    pub support_blend_time: f64,
    pub max_body_turn_rate: f64,
    pub turn_gain: f64,
    pub turn_radial_gain: f64,
    pub turn_radial_limit: f64,
    pub turn_step_pressure: f64,
    pub turn_cycle_gain: f64,
    pub turn_error_drive: f64,
    pub turn_speed_floor: f64,
    pub turn_pose_fraction: f64,
    pub heading_smoothing: f64,
    pub heading_deadband: f64,
}

impl SpiderGaitConfig {
    /// `_build_spider_gait_config` on `appearance.spider_gait`. `None` when the
    /// block is missing, not an object, not enabled, or holds a value
    /// `float()` rejects.
    pub fn from_json(raw: Option<&Value>) -> Option<Self> {
        let raw = raw?;
        if !raw.is_object() {
            return None;
        }
        if !raw.get("enabled").is_some_and(py_truthy) {
            return None;
        }
        let f = |key: &str, default: f64, lo: f64, hi: f64| -> Option<f64> {
            Some(clamp(get_float(raw, key, default)?, lo, hi))
        };
        let profile = raw.get("profile").map(py_str).unwrap_or_else(|| "tetrapod".to_string()).trim().to_lowercase();
        let max_airborne = clamp(get_float(raw, "max_airborne", 3.0)?, 2.0, 3.0) as usize;
        Some(SpiderGaitConfig {
            metachronal: profile == "chosen_one" || profile == "tarantula",
            profile: GaitProfile::from_name(&profile),
            max_airborne,
            swing_arc_outward: f("swing_arc_outward", 0.12, 0.0, 0.30)?,
            swing_arc_forward: f("swing_arc_forward", 0.24, 0.0, 0.50)?,
            swing_height: f("swing_height", 0.56, 0.35, 0.85)?,
            front_stride_bias: f("front_stride_bias", 0.16, 0.0, 0.35)?,
            stride_gain: f("stride_gain", 1.0, 0.75, 1.80)?,
            swing_fraction: f("swing_fraction", 0.29, 0.20, 0.40)?,
            cycle_hz: f("cycle_hz", 2.10, 1.20, 4.50)?,
            speed_cycle_gain: f("speed_cycle_gain", 1.45, 0.0, 3.0)?,
            stride_length: f("stride_length", 0.0, 0.0, 2.0)?,
            stride_speed_gain: f("stride_speed_gain", 1.0, 0.0, 1.0)?,
            max_cycle_hz: f("max_cycle_hz", 4.50, 1.20, 12.0)?,
            min_swing_time: f("min_swing_time", 0.075, 0.04, 0.075)?,
            step_lookahead: f("step_lookahead", 0.72, 0.25, 1.20)?,
            step_trigger: f("step_trigger", 0.18, 0.12, 0.34)?,
            stance_deadband: f("stance_deadband", 0.36, 0.18, 0.80)?,
            front_phase_offset: f("front_phase_offset", 0.035, 0.0, 0.10)?,
            refractory_fraction: f("refractory_fraction", 0.16, 0.04, 0.35)?,
            support_stroke_limit: f("support_stroke_limit", 0.62, 0.35, 1.10)?,
            support_turn_limit: f("support_turn_limit", 0.58, 0.25, 1.20)?,
            support_blend_time: f("support_blend_time", 0.10, 0.04, 0.30)?,
            max_body_turn_rate: f("max_body_turn_rate", 2.60, 0.80, 8.00)?,
            turn_gain: f("turn_gain", 1.0, 0.80, 3.00)?,
            turn_radial_gain: f("turn_radial_gain", 0.34, 0.0, 0.80)?,
            turn_radial_limit: f("turn_radial_limit", 0.28, 0.08, 0.55)?,
            turn_step_pressure: f("turn_step_pressure", 0.62, 0.45, 0.88)?,
            turn_cycle_gain: f("turn_cycle_gain", 0.26, 0.08, 0.80)?,
            turn_error_drive: f("turn_error_drive", 1.25, 0.40, 2.40)?,
            turn_speed_floor: f("turn_speed_floor", 0.15, 0.10, 0.60)?,
            turn_pose_fraction: f("turn_pose_fraction", 0.78, 0.55, 1.0)?,
            heading_smoothing: f("heading_smoothing", 7.0, 3.0, 18.0)?,
            heading_deadband: f("heading_deadband", 0.045, 0.015, 0.16)?,
        })
    }
}

/// `appearance.leg_chain`, sanitised as `_sprite_leg_chain_config` does.
///
/// Note the Python quirk this preserves: `carry_bounce`, `carry_spring` and
/// `carry_joint_wave` are read from the *sanitised* dictionary, which never
/// contains them, so the held-pose code always uses their defaults (0.18,
/// 1.0, 0.42) whatever the model says. They are therefore not stored here.
#[derive(Clone, Debug, PartialEq)]
pub struct LegChainConfig {
    pub segment_count: usize,
    pub asset_name: String,
    pub split: f64,
    pub bend: f64,
    pub max_stretch: f64,
    pub elevated_arc: f64,
    pub proximal_lift: f64,
    pub hairy: bool,
    pub hair_scale: f64,
    pub leg_curve: f64,
    pub segment_scales: Vec<f64>,
    pub width_scales: Vec<f64>,
    pub segment_color_keys: Vec<String>,
    pub joint_color_key: String,
    pub tip_color_key: String,
    pub segment_lengths: Vec<f64>,
    pub joint_scales: Vec<f64>,
    pub bend_profile: Vec<f64>,
    pub joint_phase_offsets: Vec<f64>,
    pub bend_directions: Vec<f64>,
}

impl LegChainConfig {
    /// `_sprite_leg_chain_config()`. `curved_legs` is the module flag
    /// `render_procedural.CURVED_LEGS` (true in production).
    pub fn from_json(raw: Option<&Value>, curved_legs: bool) -> Option<Self> {
        let raw = raw?;
        if !raw.is_object() || !raw.get("enabled").is_some_and(py_truthy) {
            return None;
        }
        let segment_count = match raw.get("segment_count") {
            None => 4,
            Some(v) => py_int(v)?,
        };
        if segment_count != 4 && segment_count != 5 {
            return None;
        }
        let n = segment_count as usize;
        let asset_name =
            raw.get("extra_segment_asset").map(py_str).unwrap_or_else(|| "leg_knuckle".to_string()).trim().to_string();
        if asset_name.is_empty() {
            return None;
        }
        let split = clamp(get_float(raw, "knuckle_split", 0.46)?, 0.28, 0.66);
        let bend = clamp(get_float(raw, "knuckle_bend", 0.12)?, 0.0, 0.24);
        let max_stretch = clamp(get_float(raw, "max_stretch", 1.05)?, 1.0, 1.12);
        let elevated_arc = clamp(get_float(raw, "elevated_arc", 0.0)?, 0.0, 0.45);
        let proximal_lift = clamp(get_float(raw, "proximal_lift", 1.0)?, 1.0, 2.20);
        let hairy = raw.get("hairy").is_some_and(py_truthy);
        let hair_scale = clamp(get_float(raw, "hair_scale", 0.18)?, 0.0, 0.50);
        let leg_curve = if curved_legs { clamp(get_float(raw, "leg_curve", 0.0)?, 0.0, 60.0) } else { 0.0 };
        let default_scales: Vec<f64> = [1.0, 0.84, 0.64, 0.52, 0.42][..n].to_vec();
        let default_lengths: Vec<f64> = [1.0, 1.05, 0.78, 0.56, 0.44][..n].to_vec();
        let default_joints: Vec<f64> = [0.95, 0.72, 0.60, 0.48][..n - 1].to_vec();
        let default_profile: Vec<f64> = [1.0, 0.64, 0.28, 0.18][..n - 1].to_vec();
        let default_phase_offsets: Vec<f64> = [0.0, 0.32, -0.22, 0.44][..n - 1].to_vec();
        let default_bend_directions: Vec<f64> = [1.0, 1.0, 0.72, 0.42][..n - 1].to_vec();
        let list = |key: &str, default: &Vec<f64>| -> Option<Vec<f64>> {
            match raw.get(key) {
                None => Some(default.clone()),
                Some(v) => py_float_list(v),
            }
        };
        let segment_scales = list("segment_scales", &default_scales)?;
        let width_scales = list("width_scales", &segment_scales)?;
        let segment_color_keys: Vec<String> = match raw.get("segment_color_keys") {
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| {
                    let s = py_str(v).trim().to_string();
                    if s.is_empty() { "legs".to_string() } else { s }
                })
                .collect(),
            _ => vec!["legs".to_string(); n],
        };
        let segment_lengths = list("segment_lengths", &default_lengths)?;
        let joint_scales = list("joint_scales", &default_joints)?;
        let bend_profile = list("bend_profile", &default_profile)?;
        let tau = std::f64::consts::TAU;
        let joint_phase_offsets: Vec<f64> =
            list("joint_phase_offsets", &default_phase_offsets)?.into_iter().map(|v| clamp(v, -tau, tau)).collect();
        let bend_directions: Vec<f64> =
            list("bend_directions", &default_bend_directions)?.into_iter().map(|v| clamp(v, -1.4, 1.4)).collect();
        let joint_count = n - 1;
        if segment_scales.len() != n
            || width_scales.len() != n
            || segment_color_keys.len() != n
            || segment_lengths.len() != n
            || joint_scales.len() != joint_count
            || bend_profile.len() != joint_count
            || joint_phase_offsets.len() != joint_count
            || bend_directions.len() != joint_count
        {
            return None;
        }
        let positive = segment_scales
            .iter()
            .chain(&width_scales)
            .chain(&segment_lengths)
            .chain(&joint_scales)
            .chain(&bend_profile)
            .all(|v| *v > 0.0 && v.is_finite());
        if !positive {
            return None;
        }
        if !joint_phase_offsets.iter().chain(&bend_directions).all(|v| v.is_finite()) {
            return None;
        }
        let key = |name: &str| {
            let s = raw.get(name).map(py_str).unwrap_or_else(|| "legs".to_string()).trim().to_string();
            if s.is_empty() { "legs".to_string() } else { s }
        };
        Some(LegChainConfig {
            segment_count: n,
            asset_name,
            split,
            bend,
            max_stretch,
            elevated_arc,
            proximal_lift,
            hairy,
            hair_scale,
            leg_curve,
            segment_scales,
            width_scales,
            segment_color_keys,
            joint_color_key: key("joint_color_key"),
            tip_color_key: key("tip_color_key"),
            segment_lengths,
            joint_scales,
            bend_profile,
            joint_phase_offsets,
            bend_directions,
        })
    }

    /// `segment_count - 1`
    #[inline]
    pub fn joint_count(&self) -> usize {
        self.segment_count - 1
    }
}

/// The front-leg feeler presentation from `appearance` (`_front_leg_feeler_pose`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontLegFeeler {
    pub enabled: bool,
    pub idle_lift: f64,
    pub idle_forward: f64,
    pub sway: f64,
}

/// Personality values read by the locomotion code, with the Python's defaults.
/// Built from the personality JSON object by [`MotionPersonality::from_json`].
#[derive(Clone, Debug, PartialEq)]
pub struct MotionPersonality {
    pub acceleration_multiplier: f64,
    pub turn_rate_multiplier: f64,
    pub observe_lateral_stride_mult: f64,
    pub observe_forward_stride_mult: f64,
    pub observe_strafe_alignment: f64,
    pub drift_countersteer: f64,
    pub drift_slide_body_turn: f64,
    pub drift_charge_body_turn: f64,
    pub drift_slide_accel_grip: f64,
    pub drift_inertia_keep: f64,
    pub drift_slide_step_slowdown: f64,
    pub drift_slide_rehome_slop: f64,
    /// `int(personality.get("drift_slide_max_swinging", 2))`
    pub drift_slide_max_swinging: i64,
    pub drift_min_speed: f64,
    pub drift_full_speed: f64,
    pub drift_momentum_build: f64,
    pub drift_momentum_decay: f64,
    pub drift_breakaway: f64,
    pub drift_wave_speed: f64,
    pub drift_flip_chance: f64,
    pub drift_slip: f64,
    /// `personality.get("drift_throw_slip", personality.get("drift_slip", 0.48))`
    pub drift_throw_slip: f64,
    pub drift_boost_decay: f64,
    /// `rand_range(personality.get("drift_switch_time"), ...)`: the pair when
    /// the value is a list of at least two numbers.
    pub drift_switch_time: Option<[f64; 2]>,
}

impl Default for MotionPersonality {
    fn default() -> Self {
        Self::from_json(&Value::Object(Default::default()))
    }
}

impl MotionPersonality {
    pub fn from_json(p: &Value) -> Self {
        let f = |key: &str, default: f64| get_float(p, key, default).unwrap_or(default);
        let drift_slip = f("drift_slip", 0.48);
        let drift_switch_time = match p.get("drift_switch_time") {
            Some(Value::Array(items)) if items.len() >= 2 => match (py_float(&items[0]), py_float(&items[1])) {
                (Some(a), Some(b)) => Some([a, b]),
                _ => None,
            },
            _ => None,
        };
        MotionPersonality {
            acceleration_multiplier: f("acceleration_multiplier", 1.0),
            turn_rate_multiplier: f("turn_rate_multiplier", 1.0),
            observe_lateral_stride_mult: f("observe_lateral_stride_mult", 1.25),
            observe_forward_stride_mult: f("observe_forward_stride_mult", 0.28),
            observe_strafe_alignment: f("observe_strafe_alignment", 1.0),
            drift_countersteer: f("drift_countersteer", 0.34),
            drift_slide_body_turn: f("drift_slide_body_turn", 0.42),
            drift_charge_body_turn: f("drift_charge_body_turn", 0.62),
            drift_slide_accel_grip: f("drift_slide_accel_grip", 0.46),
            drift_inertia_keep: f("drift_inertia_keep", 0.78),
            drift_slide_step_slowdown: f("drift_slide_step_slowdown", 1.42),
            drift_slide_rehome_slop: f("drift_slide_rehome_slop", 1.58),
            drift_slide_max_swinging: p.get("drift_slide_max_swinging").and_then(py_int).unwrap_or(2),
            drift_min_speed: f("drift_min_speed", 62.0),
            drift_full_speed: f("drift_full_speed", 185.0),
            drift_momentum_build: f("drift_momentum_build", 1.65),
            drift_momentum_decay: f("drift_momentum_decay", 1.15),
            drift_breakaway: f("drift_breakaway", 0.46),
            drift_wave_speed: f("drift_wave_speed", 2.4),
            drift_flip_chance: f("drift_flip_chance", 0.18),
            drift_slip,
            drift_throw_slip: f("drift_throw_slip", drift_slip),
            drift_boost_decay: f("drift_boost_decay", 1.15),
            drift_switch_time,
        }
    }
}

/// Everything static the kinematics need about one creature.
#[derive(Clone, Debug, Default)]
pub struct Rig {
    pub legs: Vec<LegDef>,
    pub spider_gait: Option<SpiderGaitConfig>,
    pub leg_chain: Option<LegChainConfig>,
    /// `appearance.antennae.style == "tarantula_hand_palps"` (lower-cased,
    /// trimmed): the feeler probe is suppressed for these pedipalps.
    pub tarantula_hand_palps: bool,
    pub front_leg_feeler: FrontLegFeeler,
    /// `appearance.roll_tuck_strength`, clamped to `[0.25, 0.62]` at use.
    pub roll_tuck_strength: f64,
    pub personality: MotionPersonality,
}

impl Rig {
    /// Build from a resolved model (`legs`, `appearance`) and a personality
    /// object. Legs past [`MAX_LEGS`] are dropped.
    pub fn from_model_json(model: &Value, personality: &Value, curved_legs: bool) -> Self {
        let empty = Value::Object(Default::default());
        let appearance = model.get("appearance").filter(|a| a.is_object()).unwrap_or(&empty);
        let legs: Vec<LegDef> = model
            .get("legs")
            .and_then(Value::as_array)
            .map(|items| items.iter().take(MAX_LEGS).map(LegDef::from_json).collect())
            .unwrap_or_default();
        let antennae = appearance.get("antennae");
        let tarantula_hand_palps = antennae
            .filter(|a| a.is_object())
            .and_then(|a| a.get("style"))
            .map(|s| py_str(s).trim().to_lowercase() == "tarantula_hand_palps")
            .unwrap_or(false);
        let app_f = |key: &str, default: f64| get_float(appearance, key, default).unwrap_or(default);
        Rig {
            legs,
            spider_gait: SpiderGaitConfig::from_json(appearance.get("spider_gait")),
            leg_chain: LegChainConfig::from_json(appearance.get("leg_chain"), curved_legs),
            tarantula_hand_palps,
            front_leg_feeler: FrontLegFeeler {
                enabled: appearance.get("front_leg_feeler_mode").is_some_and(py_truthy),
                idle_lift: app_f("front_leg_idle_lift", 0.0),
                idle_forward: app_f("front_leg_idle_forward", 0.0),
                sway: app_f("front_leg_feeler_sway", 0.0),
            },
            roll_tuck_strength: app_f("roll_tuck_strength", 0.46),
            personality: MotionPersonality::from_json(personality),
        }
    }
}
