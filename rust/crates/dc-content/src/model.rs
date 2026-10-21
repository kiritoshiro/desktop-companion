//! Typed `models/<id>/model.json`.
//!
//! Required keys (`id`, `display_name`, `base_size`, `default_personality`,
//! `colors`) are plain fields. Everything optional is an `Option`, because the
//! Python readers apply *different* defaults to the same key at different call
//! sites (`abdomen_scale` defaults to `[0.96, 0.86]` in `core.py`,
//! `[0.92, 0.84]` in `render_procedural.py` and `[1.08, 0.92]` in
//! `render_sprite.py`); a consumer writes `.unwrap_or(<its default>)`.
//! Where every reader agrees, an accessor method carries that default.
//! Unknown keys at every level are kept in `extra` and round-trip.

use std::sync::LazyLock;

use dc_core::Color;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::optional_fields;
use crate::util::{ColorMap, JsonMap, de};

/// One leg of a rig, as written in a model or a body plan.
///
/// The eight keys `validate_model` requires are plain fields; the rest are
/// optional with accessors carrying the default every Python reader uses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LegDef {
    pub name: String,
    pub side: String,
    #[serde(deserialize_with = "de::i64_lenient")]
    pub gait_group: i64,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub phase_offset: Option<f64>,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub attach_angle: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub rest_angle: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub reach: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub upper_len: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub lower_len: f64,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub attach_forward: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub attach_side: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub rest_forward: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub rest_side: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub rest_jitter: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub coxa_len: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub stride_forward: Option<f64>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl LegDef {
    /// `d.get("attach_forward", 0.0)` (geometry, gait, legs, renderer).
    pub fn attach_forward(&self) -> f64 {
        self.attach_forward.unwrap_or(0.0)
    }
    /// `d.get("attach_side", 0.30)`.
    pub fn attach_side(&self) -> f64 {
        self.attach_side.unwrap_or(0.30)
    }
    /// `d.get("rest_forward", 0.0)`.
    pub fn rest_forward(&self) -> f64 {
        self.rest_forward.unwrap_or(0.0)
    }
    /// `d.get("rest_side", 1.0)`.
    pub fn rest_side(&self) -> f64 {
        self.rest_side.unwrap_or(1.0)
    }
    /// `d.get("rest_jitter", 0.0)`.
    pub fn rest_jitter(&self) -> f64 {
        self.rest_jitter.unwrap_or(0.0)
    }
    /// `d.get("stride_forward", 0.34)`.
    pub fn stride_forward(&self) -> f64 {
        self.stride_forward.unwrap_or(0.34)
    }
    /// `definition.get("coxa_len", 0.20)`.
    pub fn coxa_len(&self) -> f64 {
        self.coxa_len.unwrap_or(0.20)
    }
    pub fn is_left(&self) -> bool {
        self.side == "left"
    }
}

optional_fields! {
    /// `appearance.spider_gait`: the anatomical gait controller's settings.
    pub struct SpiderGait {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        profile: String,
        #[serde(deserialize_with = "de::opt_i64")] max_airborne: i64,
        #[serde(deserialize_with = "de::opt_f64")] cycle_hz: f64,
        #[serde(deserialize_with = "de::opt_f64")] swing_fraction: f64,
        #[serde(deserialize_with = "de::opt_f64")] swing_height: f64,
        #[serde(deserialize_with = "de::opt_f64")] stride_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] front_stride_bias: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] max_body_turn_rate: f64,
        #[serde(deserialize_with = "de::opt_f64")] stance_deadband: f64,
        #[serde(deserialize_with = "de::opt_f64")] front_phase_offset: f64,
        #[serde(deserialize_with = "de::opt_f64")] heading_deadband: f64,
        #[serde(deserialize_with = "de::opt_f64")] heading_smoothing: f64,
        #[serde(deserialize_with = "de::opt_f64")] max_cycle_hz: f64,
        #[serde(deserialize_with = "de::opt_f64")] min_swing_time: f64,
        #[serde(deserialize_with = "de::opt_f64")] refractory_fraction: f64,
        #[serde(deserialize_with = "de::opt_f64")] speed_cycle_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] step_lookahead: f64,
        #[serde(deserialize_with = "de::opt_f64")] step_trigger: f64,
        #[serde(deserialize_with = "de::opt_f64")] stride_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] stride_speed_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] support_blend_time: f64,
        #[serde(deserialize_with = "de::opt_f64")] support_stroke_limit: f64,
        #[serde(deserialize_with = "de::opt_f64")] support_turn_limit: f64,
        #[serde(deserialize_with = "de::opt_f64")] swing_arc_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] swing_arc_outward: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_cycle_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_error_drive: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_pose_fraction: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_radial_gain: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_radial_limit: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_speed_floor: f64,
        #[serde(deserialize_with = "de::opt_f64")] turn_step_pressure: f64,
    }
}

optional_fields! {
    /// `appearance.leg_chain`: 4- or 5-segment articulated legs.
    pub struct LegChain {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        #[serde(deserialize_with = "de::opt_i64")] segment_count: i64,
        extra_segment_asset: String,
        #[serde(deserialize_with = "de::opt_vec_f64")] segment_scales: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] segment_lengths: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] width_scales: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] joint_scales: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] bend_profile: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] joint_phase_offsets: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] bend_directions: Vec<f64>,
        segment_color_keys: Vec<String>,
        joint_color_key: String,
        tip_color_key: String,
        #[serde(deserialize_with = "de::opt_bool")] hairy: bool,
        #[serde(deserialize_with = "de::opt_f64")] hair_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] carry_bounce: f64,
        #[serde(deserialize_with = "de::opt_f64")] carry_joint_wave: f64,
        #[serde(deserialize_with = "de::opt_f64")] carry_spring: f64,
        #[serde(deserialize_with = "de::opt_f64")] elevated_arc: f64,
        #[serde(deserialize_with = "de::opt_f64")] proximal_lift: f64,
        #[serde(deserialize_with = "de::opt_f64")] knuckle_bend: f64,
        #[serde(deserialize_with = "de::opt_f64")] knuckle_split: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_curve: f64,
        #[serde(deserialize_with = "de::opt_f64")] max_stretch: f64,
    }
}

impl LegChain {
    /// `bool(chain.get("enabled", False))`.
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
    /// `int(chain.get("segment_count", 4))`.
    pub fn segment_count(&self) -> i64 {
        self.segment_count.unwrap_or(4)
    }
    /// `chain.get("extra_segment_asset", "leg_knuckle")`.
    pub fn extra_segment_asset(&self) -> &str {
        self.extra_segment_asset.as_deref().unwrap_or("leg_knuckle")
    }
}

optional_fields! {
    /// `appearance.antennae`: antennae, or articulated pedipalp "hands".
    pub struct Antennae {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        style: String,
        control_mode: String,
        #[serde(deserialize_with = "de::opt_i64")] segments: i64,
        #[serde(deserialize_with = "de::opt_f64")] length: f64,
        #[serde(deserialize_with = "de::opt_f64")] thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] base_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] base_side: f64,
        color_key: String,
        claw_color_key: String,
        joint_color_key: String,
        tip_color_key: String,
        segment_color_keys: Vec<String>,
        #[serde(deserialize_with = "de::opt_vec_f64")] segment_lengths: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] segment_widths: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] rest_angles: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] joint_angle_limits: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] joint_steering: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] joint_steering_limits: Vec<f64>,
        #[serde(deserialize_with = "de::opt_vec_f64")] free_range_of_motion: Vec<f64>,
        #[serde(deserialize_with = "de::opt_bool")] hairy: bool,
        #[serde(deserialize_with = "de::opt_f64")] hair_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] joint_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] proximal_rise: f64,
        #[serde(deserialize_with = "de::opt_f64")] screen_lift_scale: f64,
        #[serde(deserialize_with = "de::opt_bool")] symmetric_rest: bool,
        #[serde(deserialize_with = "de::opt_f64")] tip_curl: f64,
        #[serde(deserialize_with = "de::opt_f64")] tip_nail_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_min_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_max_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_rest_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_min_lateral: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_max_lateral: f64,
        #[serde(deserialize_with = "de::opt_f64")] hand_rest_lateral: f64,
        #[serde(deserialize_with = "de::opt_f64")] knuckle_lift_delay: f64,
        #[serde(deserialize_with = "de::opt_f64")] knuckle_wave_delay: f64,
        #[serde(deserialize_with = "de::opt_f64")] max_drive_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] min_drive_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] max_extension: f64,
        #[serde(deserialize_with = "de::opt_f64")] serial_follow: f64,
        #[serde(deserialize_with = "de::opt_f64")] rub_forward_amount: f64,
        #[serde(deserialize_with = "de::opt_f64")] rub_frequency: f64,
        #[serde(deserialize_with = "de::opt_f64")] rub_joint_angle: f64,
        #[serde(deserialize_with = "de::opt_f64")] rub_lateral_amount: f64,
        #[serde(deserialize_with = "de::opt_f64")] rub_lift_amount: f64,
    }
}

optional_fields! {
    /// `appearance.head` and `appearance.pedicel`: an extra body part drawn
    /// between the cephalothorax and abdomen (pedicel) or in front (head).
    pub struct BodyPart {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        color_key: String,
        outline_key: String,
        highlight_key: String,
        #[serde(deserialize_with = "de::opt_i64")] highlight_alpha: i64,
        #[serde(deserialize_with = "de::opt_f64")] outline_width: f64,
        #[serde(deserialize_with = "de::opt_f64")] offset_x: f64,
        #[serde(deserialize_with = "de::opt_vec_f64")] scale: Vec<f64>,
        #[serde(deserialize_with = "de::opt_f64")] face_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] face_side: f64,
        #[serde(deserialize_with = "de::opt_bool")] fangs: bool,
    }
}

optional_fields! {
    /// `appearance.leg_connections`: visible coxa/trochanter roots.
    pub struct LegConnections {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        accent_color_key: String,
        core_color_key: String,
        socket_color_key: String,
        #[serde(deserialize_with = "de::opt_f64")] coxa_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] coxa_width: f64,
        #[serde(deserialize_with = "de::opt_f64")] trochanter_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] trochanter_width: f64,
        #[serde(deserialize_with = "de::opt_f64")] joint_radius: f64,
        #[serde(deserialize_with = "de::opt_f64")] socket_radius: f64,
        #[serde(deserialize_with = "de::opt_f64")] socket_core_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] socket_outline_width: f64,
    }
}

optional_fields! {
    /// `appearance.leg_shadow`.
    pub struct LegShadow {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        #[serde(deserialize_with = "de::opt_i64")] alpha: i64,
        #[serde(deserialize_with = "de::opt_f64")] width: f64,
        #[serde(deserialize_with = "de::opt_f64")] root_height: f64,
        #[serde(deserialize_with = "de::opt_f64")] knee_height: f64,
    }
}

optional_fields! {
    /// `appearance.carapace_rim`.
    pub struct CarapaceRim {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        color_key: String,
        #[serde(deserialize_with = "de::opt_i64")] alpha: i64,
        #[serde(deserialize_with = "de::opt_f64")] width: f64,
    }
}

optional_fields! {
    /// The model's top-level `fur` block (sprite-rig fluff).
    pub struct Fur {
        #[serde(deserialize_with = "de::opt_bool")] enabled: bool,
        color: Color,
        #[serde(deserialize_with = "de::opt_i64")] alpha: i64,
        #[serde(deserialize_with = "de::opt_i64")] density: i64,
        #[serde(deserialize_with = "de::opt_f64")] length: f64,
        #[serde(deserialize_with = "de::opt_f64")] thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] sway: f64,
        #[serde(deserialize_with = "de::opt_f64")] abdomen_ratio: f64,
        #[serde(deserialize_with = "de::opt_bool")] leg_fuzz: bool,
        #[serde(deserialize_with = "de::opt_i64")] leg_density: i64,
        #[serde(deserialize_with = "de::opt_f64")] leg_length: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_thickness: f64,
    }
}

optional_fields! {
    /// `model["appearance"]`: everything about how the spider is drawn.
    pub struct Appearance {
        #[serde(deserialize_with = "de::opt_f64")] cephalothorax_offset_x: f64,
        #[serde(deserialize_with = "de::opt_vec_f64")] cephalothorax_scale: Vec<f64>,
        #[serde(deserialize_with = "de::opt_f64")] abdomen_offset_x: f64,
        #[serde(deserialize_with = "de::opt_vec_f64")] abdomen_scale: Vec<f64>,
        #[serde(deserialize_with = "de::opt_f64")] pedipalp_scale: f64,
        #[serde(deserialize_with = "de::opt_i64")] eye_count: i64,
        eye_layout: String,
        #[serde(deserialize_with = "de::opt_f64")] eye_scale: f64,
        #[serde(deserialize_with = "de::opt_i64")] highlight_alpha: i64,
        #[serde(deserialize_with = "de::opt_f64")] fluffiness: f64,
        fluff_color: Color,
        #[serde(deserialize_with = "de::opt_f64")] cute_blush: f64,
        #[serde(deserialize_with = "de::opt_f64")] foot_bulb: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_segment_thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] leg_tip_thickness: f64,
        #[serde(deserialize_with = "de::opt_f64")] coxa_thickness_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] joint_node_scale: f64,
        #[serde(deserialize_with = "de::opt_bool")] segmented_legs: bool,
        #[serde(deserialize_with = "de::opt_bool")] show_joint_nodes: bool,
        #[serde(deserialize_with = "de::opt_bool")] legs_over_body: bool,
        #[serde(deserialize_with = "de::opt_f64")] shadow_scale: f64,
        #[serde(deserialize_with = "de::opt_f64")] roll_tuck_strength: f64,
        #[serde(deserialize_with = "de::opt_i64")] stripe_count: i64,
        stripe_color: Color,
        #[serde(deserialize_with = "de::opt_i64")] stripe_alpha: i64,
        #[serde(deserialize_with = "de::opt_bool")] front_leg_feeler_mode: bool,
        #[serde(deserialize_with = "de::opt_f64")] front_leg_feeler_sway: f64,
        #[serde(deserialize_with = "de::opt_f64")] front_leg_idle_forward: f64,
        #[serde(deserialize_with = "de::opt_f64")] front_leg_idle_lift: f64,
        #[serde(deserialize_with = "de::opt_bool")] enemy_crown: bool,
        enemy_marking: String,
        enemy_shell: String,
        legendary_principle: String,
        softness_profile: String,
        species_profile: String,
        species_style: String,
        antennae: Antennae,
        carapace_rim: CarapaceRim,
        head: BodyPart,
        pedicel: BodyPart,
        leg_chain: LegChain,
        leg_connections: LegConnections,
        leg_shadow: LegShadow,
        spider_gait: SpiderGait,
    }
}

static EMPTY_APPEARANCE: LazyLock<Appearance> = LazyLock::new(Appearance::default);

/// One `models/<id>/model.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelDef {
    pub id: String,
    pub display_name: String,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub base_size: f64,
    pub default_personality: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_mode: Option<String>,
    pub colors: ColorMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fur: Option<Fur>,
    /// Empty in a raw file that names a body plan; filled by
    /// [`crate::body_plans::resolve_model_body_plan`] (discovery does this).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legs: Vec<LegDef>,
    /// Set by discovery: the file the model came from.
    #[serde(rename = "_path", default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Set by discovery: the model's folder (sprite assets live under it).
    #[serde(rename = "_folder", default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl ModelDef {
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a model always serialises")
    }

    /// `model.get("appearance", {})`.
    pub fn appearance(&self) -> &Appearance {
        self.appearance.as_ref().unwrap_or(&EMPTY_APPEARANCE)
    }

    /// `model["colors"].get(key)`.
    pub fn color(&self, key: &str) -> Option<Color> {
        self.colors.get(key).copied()
    }

    /// `colors.get(key, fallback)`.
    pub fn color_or(&self, key: &str, fallback: Color) -> Color {
        self.color(key).unwrap_or(fallback)
    }

    /// `render_mode`, defaulting to `"procedural"` as the creature does.
    pub fn render_mode(&self) -> &str {
        self.render_mode.as_deref().unwrap_or("procedural")
    }

    /// The model's own gait, or the plan's once resolved.
    pub fn spider_gait(&self) -> Option<&SpiderGait> {
        self.appearance.as_ref().and_then(|a| a.spider_gait.as_ref())
    }
}
