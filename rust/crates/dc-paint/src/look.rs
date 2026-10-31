//! A model's `appearance` block, read once into typed values.
//!
//! The Python renderers call `self._appearance(key, default)` about a hundred
//! times per spider per frame. Every read below keeps the Python default, the
//! Python conversion (`float()`, `int()`, `bool()`) and the Python clamp at
//! the point of use. Where Python would raise on a malformed value, the
//! default is used instead.
//!
//! Defaults that differ between the procedural and the sprite-rig renderer
//! (`abdomen_scale`, `cephalothorax_scale`, `cephalothorax_offset_x`,
//! `eye_scale`, `eye_count`, `eye_layout`) are kept as `Option`s and resolved
//! by each renderer.

use dc_kinematics::LegChainConfig;
use dc_kinematics::pymath::{py_float, py_int, py_str, py_truthy};
use serde_json::Value;

fn f(obj: &Value, key: &str, default: f64) -> f64 {
    match obj.get(key) {
        None => default,
        Some(v) => py_float(v).unwrap_or(default),
    }
}

fn opt_f(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(py_float)
}

fn i(obj: &Value, key: &str, default: i64) -> i64 {
    match obj.get(key) {
        None => default,
        Some(v) => py_int(v).unwrap_or(default),
    }
}

fn b(obj: &Value, key: &str, default: bool) -> bool {
    obj.get(key).map(py_truthy).unwrap_or(default)
}

fn s(obj: &Value, key: &str, default: &str) -> String {
    obj.get(key).map(py_str).unwrap_or_else(|| default.to_string())
}

fn triple(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() < 3 {
        return None;
    }
    Some([py_float(&a[0])?, py_float(&a[1])?, py_float(&a[2])?])
}

fn list(v: Option<&Value>) -> Option<Vec<f64>> {
    let a = v?.as_array()?;
    a.iter().map(py_float).collect()
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    dc_core::math::clamp(v, lo, hi)
}

/// `appearance.pedicel` (the waist between abdomen and cephalothorax).
#[derive(Clone, Debug, PartialEq)]
pub struct PedicelLook {
    pub enabled: bool,
    pub offset_x: f64,
    /// `scale` when it has at least two entries.
    pub scale: Option<[f64; 2]>,
    pub color_key: String,
    pub outline_key: String,
    pub outline_width: f64,
}

/// `appearance.head` (the chelicerae lobe in front of the carapace).
#[derive(Clone, Debug, PartialEq)]
pub struct HeadLook {
    pub enabled: bool,
    pub offset_x: f64,
    pub scale: Option<[f64; 2]>,
    pub color_key: String,
    pub outline_key: String,
    pub outline_width: f64,
    pub highlight_key: String,
    pub highlight_alpha: i64,
    pub face_forward: f64,
    pub face_side: f64,
    pub fangs: bool,
}

/// `appearance.leg_shadow` (DC-83).
#[derive(Clone, Debug, PartialEq)]
pub struct LegShadowLook {
    pub knee_height: f64,
    pub root_height: f64,
    pub alpha: f64,
    pub width: f64,
}

/// `appearance.carapace_rim` (DC-83).
#[derive(Clone, Debug, PartialEq)]
pub struct RimLook {
    pub width: f64,
    pub color_key: String,
    pub alpha: i64,
}

/// `appearance.leg_connections`, already clamped (`_draw_leg_connections`).
#[derive(Clone, Debug, PartialEq)]
pub struct LegConnectionsLook {
    pub socket_radius: f64,
    pub socket_outline_width: f64,
    pub socket_core_scale: f64,
    pub coxa_length: f64,
    pub trochanter_length: f64,
    pub coxa_width: f64,
    pub trochanter_width: f64,
    pub joint_radius: f64,
    pub socket_color_key: String,
    pub core_color_key: String,
    pub accent_color_key: String,
}

/// The style of `appearance.antennae`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntennaStyle {
    /// The expressive insect-like feelers.
    Feelers,
    /// `front_leg` / `tarantula_front_legs`.
    FrontLegs,
    /// `tarantula_hand_palps`.
    HandPalps,
}

/// `appearance.antennae`, with every list already resolved the way
/// `_draw_antennae` resolves it.
#[derive(Clone, Debug, PartialEq)]
pub struct AntennaLook {
    pub enabled: bool,
    pub style: AntennaStyle,
    pub segments: usize,
    pub length: f64,
    pub base_forward: f64,
    pub base_side: f64,
    pub thickness: f64,
    pub color_key: String,
    pub tip_color_key: String,
    // Leg-like styles only.
    pub lengths: Vec<f64>,
    pub angle_profile: Vec<f64>,
    pub joint_scale: f64,
    pub proximal_rise: f64,
    pub screen_lift_scale: f64,
    pub leg_thickness: f64,
    pub hairy: bool,
    pub hair_scale: f64,
    pub width_profile: Vec<f64>,
    pub segment_color_keys: Vec<String>,
    pub joint_color_key: String,
    pub claw_color_key: String,
    pub min_drive_length: f64,
    pub max_drive_length: f64,
    pub max_extension: f64,
    /// `tip_nail_length` clamped (multiplied by the span at draw time).
    pub tip_nail_length: f64,
    pub tip_curl: f64,
}

impl AntennaLook {
    fn from_json(raw: Option<&Value>) -> (Self, bool) {
        let empty = Value::Object(Default::default());
        let is_dict = raw.is_some_and(Value::is_object);
        let cfg = if is_dict { raw.unwrap() } else { &empty };
        let enabled = !matches!(cfg.get("enabled"), Some(Value::Bool(false)));
        let style_name = s(cfg, "style", "").trim().to_lowercase();
        let style = match style_name.as_str() {
            "tarantula_hand_palps" => AntennaStyle::HandPalps,
            "front_leg" | "tarantula_front_legs" => AntennaStyle::FrontLegs,
            _ => AntennaStyle::Feelers,
        };
        let segments = i(cfg, "segments", 5).max(3) as usize;
        let color_key = s(cfg, "color_key", "legs");
        let tip_color_key = s(cfg, "tip_color_key", "highlight");

        let default_lengths = [0.24, 0.28, 0.22, 0.15, 0.11];
        let mut lengths: Vec<f64> = match cfg.get("segment_lengths") {
            None => default_lengths.iter().take(segments).copied().collect(),
            Some(v) if v.is_array() => {
                let a = v.as_array().unwrap();
                a.iter().take(segments).map(py_float).collect::<Option<Vec<_>>>().unwrap_or_default()
            }
            Some(_) => default_lengths.iter().take(segments).copied().collect(),
        };
        if lengths.len() != segments || lengths.iter().any(|v| *v <= 0.0 || !v.is_finite()) {
            lengths = vec![1.0; segments];
        }
        let default_angles = [0.90, 0.56, 0.22, -0.13, -0.46];
        let mut angle_profile: Vec<f64> = match cfg.get("rest_angles") {
            Some(v) if v.is_array() => v
                .as_array()
                .unwrap()
                .iter()
                .take(segments)
                .map(py_float)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
            _ => default_angles.iter().take(segments).copied().collect(),
        };
        if angle_profile.len() != segments || angle_profile.iter().any(|v| !v.is_finite()) {
            angle_profile = default_angles.iter().take(segments).copied().collect();
        }
        while angle_profile.len() < segments {
            let last = *angle_profile.last().unwrap();
            angle_profile.push(last * 0.72);
        }
        let default_widths = [1.20, 1.08, 0.92, 0.72, 0.48];
        let mut width_profile: Vec<f64> = match cfg.get("segment_widths") {
            Some(v) if v.is_array() => v
                .as_array()
                .unwrap()
                .iter()
                .take(segments)
                .map(py_float)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
            _ => default_widths.iter().take(segments).copied().collect(),
        };
        if width_profile.len() != segments || width_profile.iter().any(|v| *v <= 0.0 || !v.is_finite()) {
            width_profile = default_widths.iter().take(segments).copied().collect();
        }
        while width_profile.len() < segments {
            let last = *width_profile.last().unwrap();
            width_profile.push((last * 0.72).max(0.30));
        }
        let mut segment_color_keys: Vec<String> = match cfg.get("segment_color_keys") {
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| {
                    let t = py_str(v).trim().to_string();
                    if t.is_empty() { color_key.clone() } else { t }
                })
                .collect(),
            _ => vec![color_key.clone(); segments],
        };
        if segment_color_keys.len() != segments {
            segment_color_keys = vec![color_key.clone(); segments];
        }
        let joint_default = if segments > 1 { segment_color_keys[1].clone() } else { color_key.clone() };
        let mut joint_color_key = s(cfg, "joint_color_key", &joint_default).trim().to_string();
        if joint_color_key.is_empty() {
            joint_color_key = color_key.clone();
        }
        let claw_default = cfg.get("tip_color_key").map(py_str).unwrap_or_else(|| "highlight".to_string());
        let mut claw_color_key = s(cfg, "claw_color_key", &claw_default).trim().to_string();
        if claw_color_key.is_empty() {
            claw_color_key = "highlight".to_string();
        }
        let min_drive_length = clamp(f(cfg, "min_drive_length", 0.78), 0.55, 1.0);
        let max_drive_length = clamp(f(cfg, "max_drive_length", 1.08), min_drive_length, 1.20);
        let look = AntennaLook {
            enabled,
            style,
            segments,
            length: f(cfg, "length", 1.15),
            base_forward: f(cfg, "base_forward", 0.34),
            base_side: f(cfg, "base_side", 0.26),
            thickness: f(cfg, "thickness", 0.06),
            color_key,
            tip_color_key,
            lengths,
            angle_profile,
            joint_scale: clamp(f(cfg, "joint_scale", 0.95), 0.45, 1.60),
            proximal_rise: clamp(f(cfg, "proximal_rise", 0.12), 0.0, 0.28),
            screen_lift_scale: clamp(f(cfg, "screen_lift_scale", 0.22), 0.0, 0.45),
            leg_thickness: clamp(f(cfg, "leg_thickness", 1.0), 0.65, 1.60),
            hairy: b(cfg, "hairy", false),
            hair_scale: clamp(f(cfg, "hair_scale", 0.16), 0.0, 0.50),
            width_profile,
            segment_color_keys,
            joint_color_key,
            claw_color_key,
            min_drive_length,
            max_drive_length,
            max_extension: clamp(f(cfg, "max_extension", 0.06), 0.0, 0.12),
            tip_nail_length: clamp(f(cfg, "tip_nail_length", 0.065), 0.025, 0.12),
            tip_curl: clamp(f(cfg, "tip_curl", 0.72), 0.30, 1.10),
        };
        // `custom_hand_palps` in `_render_procedural` additionally needs the
        // block to be a dict.
        (look, is_dict)
    }
}

/// How a model is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Procedural,
    SpriteRig,
    /// `species_profile` `regal_fluff` / `regal_scout` (`_is_regal`).
    Regal,
}

/// Everything the painters read from one model, parsed once.
#[derive(Clone, Debug)]
pub struct CreatureLook {
    pub model_id: String,
    pub render_mode: RenderMode,
    /// `species_profile == "regal_scout"` (`_regal_face_forward`).
    pub regal_face_forward: bool,
    /// `model["_folder"]`: where `assets/*.png` live for a sprite rig.
    pub folder: Option<String>,
    pub leg_chain: Option<LegChainConfig>,

    pub leg_thickness: f64,
    pub fluffiness: f64,
    pub segmented_legs: bool,
    pub show_joint_nodes: bool,
    pub joint_node_scale: f64,
    pub coxa_thickness_scale: f64,
    pub leg_shadow: Option<LegShadowLook>,
    /// `appearance["fluff_color"]` when it is an RGB triple.
    pub fluff_color: Option<[f64; 3]>,
    pub legs_over_body: bool,
    pub highlight_alpha: i64,
    pub abdomen_scale: Option<Vec<f64>>,
    pub cephalothorax_scale: Option<Vec<f64>>,
    pub abdomen_offset_x: f64,
    pub cephalothorax_offset_x: Option<f64>,
    pub pedicel: PedicelLook,
    pub head: HeadLook,
    pub pedipalp_scale: f64,
    pub eye_scale: Option<f64>,
    pub eye_count: Option<i64>,
    pub eye_layout: Option<String>,
    pub enemy_shell: String,
    pub carapace_rim: Option<RimLook>,
    /// `appearance["stripe_color"] is not None`, and its value as a triple.
    pub stripe_color_set: bool,
    pub stripe_color: Option<[f64; 3]>,
    pub stripe_alpha: i64,
    pub stripe_count: i64,
    pub enemy_crown: bool,
    pub enemy_marking: String,
    pub antennae: AntennaLook,
    /// The antennae block is a dict (`custom_hand_palps` needs it).
    pub antennae_is_dict: bool,
    pub cute_blush: f64,
    pub leg_connections: Option<LegConnectionsLook>,
    // Sprite rig.
    pub shadow_scale: f64,
    pub leg_segment_thickness: Option<f64>,
    pub leg_tip_thickness: Option<f64>,
    pub foot_bulb: Option<f64>,
    /// `roll_tuck_strength` (read by the kinematics, kept for completeness).
    pub roll_tuck_strength: f64,
}

impl CreatureLook {
    /// Build from the resolved model JSON (as `Creature.model` holds it:
    /// `appearance`, `render_mode`, `_folder`, `id`). `curved_legs` is
    /// `render_procedural.CURVED_LEGS`.
    pub fn from_model_json(model: &Value, curved_legs: bool) -> Self {
        let empty = Value::Object(Default::default());
        let app = model.get("appearance").filter(|a| a.is_object()).unwrap_or(&empty);
        let species = app.get("species_profile").map(py_str);
        let regal = matches!(species.as_deref(), Some("regal_fluff") | Some("regal_scout"));
        let mode = model.get("render_mode").map(py_str).unwrap_or_else(|| "procedural".into()).to_lowercase();
        let render_mode = if regal {
            RenderMode::Regal
        } else if mode == "sprite_rig" {
            RenderMode::SpriteRig
        } else {
            RenderMode::Procedural
        };

        let leg_shadow = match app.get("leg_shadow") {
            Some(cfg) if cfg.is_object() && b(cfg, "enabled", true) => Some(LegShadowLook {
                knee_height: f(cfg, "knee_height", 0.30),
                root_height: f(cfg, "root_height", 0.35),
                alpha: f(cfg, "alpha", 46.0),
                width: f(cfg, "width", 1.1),
            }),
            _ => None,
        };

        let pedicel_raw = app.get("pedicel");
        let pedicel = {
            let cfg = pedicel_raw.filter(|v| v.is_object()).unwrap_or(&empty);
            // `self._appearance("pedicel", {})`: a missing block is an empty
            // dict, which *is* enabled. A non-dict value disables it.
            let enabled = pedicel_raw.is_none_or(Value::is_object) && b(cfg, "enabled", true);
            let scale = match cfg.get("scale") {
                None => Some([0.20, 0.12]),
                Some(v) => list(Some(v)).filter(|l| l.len() >= 2).map(|l| [l[0], l[1]]),
            };
            PedicelLook {
                enabled,
                offset_x: f(cfg, "offset_x", -0.04),
                scale,
                color_key: s(cfg, "color_key", "body"),
                outline_key: s(cfg, "outline_key", "legs"),
                outline_width: f(cfg, "outline_width", 0.025),
            }
        };
        let head_raw = app.get("head");
        let head = {
            let cfg = head_raw.filter(|v| v.is_object()).unwrap_or(&empty);
            let enabled = head_raw.is_none_or(Value::is_object) && b(cfg, "enabled", true);
            let scale = match cfg.get("scale") {
                None => Some([0.22, 0.20]),
                Some(v) => list(Some(v)).filter(|l| l.len() >= 2).map(|l| [l[0], l[1]]),
            };
            HeadLook {
                enabled,
                offset_x: f(cfg, "offset_x", 0.49),
                scale,
                color_key: s(cfg, "color_key", "body"),
                outline_key: s(cfg, "outline_key", "legs"),
                outline_width: f(cfg, "outline_width", 0.028),
                highlight_key: s(cfg, "highlight_key", "highlight"),
                highlight_alpha: i(cfg, "highlight_alpha", 115),
                face_forward: f(cfg, "face_forward", 0.10),
                face_side: f(cfg, "face_side", 0.0),
                fangs: b(cfg, "fangs", true),
            }
        };
        let carapace_rim = match app.get("carapace_rim") {
            Some(cfg) if cfg.is_object() && b(cfg, "enabled", true) => Some(RimLook {
                width: f(cfg, "width", 0.12),
                color_key: s(cfg, "color_key", "highlight"),
                alpha: i(cfg, "alpha", 235),
            }),
            _ => None,
        };
        let leg_connections = match app.get("leg_connections") {
            Some(cfg) if cfg.is_object() && matches!(cfg.get("enabled"), Some(Value::Bool(true))) => {
                Some(LegConnectionsLook {
                    socket_radius: clamp(f(cfg, "socket_radius", 0.09), 0.04, 0.18),
                    socket_outline_width: clamp(f(cfg, "socket_outline_width", 0.035), 0.01, 0.08),
                    socket_core_scale: clamp(f(cfg, "socket_core_scale", 0.66), 0.35, 0.90),
                    coxa_length: clamp(f(cfg, "coxa_length", 0.18), 0.06, 0.32),
                    trochanter_length: clamp(f(cfg, "trochanter_length", 0.12), 0.04, 0.24),
                    coxa_width: clamp(f(cfg, "coxa_width", 0.10), 0.04, 0.18),
                    trochanter_width: clamp(f(cfg, "trochanter_width", 0.075), 0.03, 0.14),
                    joint_radius: clamp(f(cfg, "joint_radius", 0.06), 0.03, 0.12),
                    socket_color_key: s(cfg, "socket_color_key", "body"),
                    core_color_key: s(cfg, "core_color_key", "legs"),
                    accent_color_key: s(cfg, "accent_color_key", "highlight"),
                })
            }
            _ => None,
        };
        let (antennae, antennae_is_dict) = AntennaLook::from_json(app.get("antennae"));
        let stripe_raw = app.get("stripe_color").filter(|v| !v.is_null());

        CreatureLook {
            model_id: model.get("id").map(py_str).unwrap_or_default(),
            render_mode,
            regal_face_forward: species.as_deref() == Some("regal_scout"),
            folder: model.get("_folder").and_then(Value::as_str).map(str::to_string),
            leg_chain: LegChainConfig::from_json(app.get("leg_chain"), curved_legs),
            leg_thickness: f(app, "leg_thickness", 1.0),
            fluffiness: clamp(f(app, "fluffiness", 0.0), 0.0, 1.0),
            segmented_legs: b(app, "segmented_legs", false),
            show_joint_nodes: b(app, "show_joint_nodes", false),
            joint_node_scale: f(app, "joint_node_scale", 1.0),
            coxa_thickness_scale: f(app, "coxa_thickness_scale", 1.0),
            leg_shadow,
            fluff_color: app.get("fluff_color").and_then(triple),
            legs_over_body: b(app, "legs_over_body", false),
            highlight_alpha: i(app, "highlight_alpha", 145),
            abdomen_scale: list(app.get("abdomen_scale")).filter(|l| l.len() >= 2),
            cephalothorax_scale: list(app.get("cephalothorax_scale")).filter(|l| l.len() >= 2),
            abdomen_offset_x: f(app, "abdomen_offset_x", -0.18),
            cephalothorax_offset_x: opt_f(app, "cephalothorax_offset_x"),
            pedicel,
            head,
            pedipalp_scale: f(app, "pedipalp_scale", 1.0),
            eye_scale: opt_f(app, "eye_scale"),
            eye_count: app.get("eye_count").and_then(py_int),
            eye_layout: app.get("eye_layout").map(py_str),
            enemy_shell: s(app, "enemy_shell", ""),
            carapace_rim,
            stripe_color_set: stripe_raw.is_some(),
            stripe_color: stripe_raw.and_then(triple),
            stripe_alpha: i(app, "stripe_alpha", 90),
            stripe_count: i(app, "stripe_count", 2).max(1),
            enemy_crown: b(app, "enemy_crown", false),
            enemy_marking: app.get("enemy_marking").filter(|v| py_truthy(v)).map(py_str).unwrap_or_default(),
            antennae,
            antennae_is_dict,
            cute_blush: f(app, "cute_blush", 0.0),
            leg_connections,
            shadow_scale: f(app, "shadow_scale", 1.6),
            leg_segment_thickness: opt_f(app, "leg_segment_thickness"),
            leg_tip_thickness: opt_f(app, "leg_tip_thickness"),
            foot_bulb: opt_f(app, "foot_bulb"),
            roll_tuck_strength: f(app, "roll_tuck_strength", 0.46),
        }
    }

    /// `_load_sprite_assets` folder, if any.
    pub fn assets_dir(&self) -> Option<std::path::PathBuf> {
        self.folder.as_ref().map(|f| std::path::Path::new(f).join("assets"))
    }
}
