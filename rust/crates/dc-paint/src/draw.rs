//! The painter's input structs: plain snapshots of what the Python renderers
//! read off `self`. Field names are the Python attribute names (without the
//! leading underscore), so `dc-creature` / `dc-manager` fill them field by
//! field. All of them deserialize from JSON with the same names, which is
//! how the comparison fixtures are loaded.

use crate::color::Palette;
use serde::{Deserialize, Serialize};

/// `creature/mood.py` `Mood` (only the four live channels are read).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Mood {
    pub valence: f64,
    pub arousal: f64,
    pub affection: f64,
    pub curiosity: f64,
}

impl Default for Mood {
    fn default() -> Self {
        Mood { valence: 0.18, arousal: 0.40, affection: 0.32, curiosity: 0.52 }
    }
}

impl Mood {
    pub fn happy(&self) -> f64 {
        dc_core::math::clamp(self.valence, 0.0, 1.0)
    }
    pub fn sad(&self) -> f64 {
        dc_core::math::clamp(-self.valence, 0.0, 1.0)
    }
    pub fn sleepy(&self) -> f64 {
        dc_core::math::clamp((0.5 - self.arousal) * 2.0, 0.0, 1.0)
    }
}

/// `creature/regal.py` `RegalExpression`: the drawn part of a gesture. The
/// caller passes `RegalExpression::default()` when `_regal_blocked()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct RegalExpression {
    pub yaw: f64,
    pub pitch: f64,
    pub tilt: f64,
    pub left: f64,
    pub right: f64,
    pub bounce: f64,
    pub palps: f64,
}

/// `{item.slot: item for item in equipped_items(progression)}`: the id of the
/// armour piece worn in each slot.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Worn {
    pub abdomen: Option<String>,
    pub carapace: Option<String>,
    pub head: Option<String>,
    pub legs: Option<String>,
    pub pedipalps: Option<String>,
}

impl Worn {
    pub fn is_empty(&self) -> bool {
        self.abdomen.is_none()
            && self.carapace.is_none()
            && self.head.is_none()
            && self.legs.is_none()
            && self.pedipalps.is_none()
    }

    /// Build from `(slot, item_id)` pairs (`equipped_items`).
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut w = Worn::default();
        for (slot, id) in pairs {
            let id = Some(id.to_string());
            match slot {
                "abdomen" => w.abdomen = id,
                "carapace" => w.carapace = id,
                "head" => w.head = id,
                "legs" => w.legs = id,
                "pedipalps" => w.pedipalps = id,
                _ => {}
            }
        }
        w
    }
}

/// One leg as the renderer draws it this frame. The geometry half of the
/// Python leg loops (`_leg_draw_points`, `_safe_sprite_leg_foot`,
/// `_solve_knee`, `_leg_lift_px`, the jump offset and the chain solve) is
/// already applied; [`crate::resolve`] produces these from `dc-kinematics`.
///
/// Which fields are meaningful depends on the render mode:
/// procedural uses `ax..tarsus_y`, `chain_points` and `connection_points`;
/// the sprite rig uses `ax, ay, kx, ky, tarsus_*, foot_*` and
/// `chain_points`; the regal jumper uses `regal_points`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct LegDraw {
    /// `leg.foot_x` / `leg.foot_y`: the planted contact (labels, damage
    /// numbers and the bounding box read it).
    pub leg_foot_x: f64,
    pub leg_foot_y: f64,
    /// `_leg_attach(leg)` (bounding box only).
    pub attach_x: f64,
    pub attach_y: f64,
    /// `leg.lift` (widths and sprite segment scale).
    pub lift: f64,
    /// `_leg_draw_points(leg)` as returned: `(ax, ay, foot_x, foot_y)`
    /// (sprite and regal equipment accents).
    pub draw_points: [f64; 4],
    // The leg pass, after every adjustment.
    pub ax: f64,
    pub ay: f64,
    pub kx: f64,
    pub ky: f64,
    pub coxa_x: f64,
    pub coxa_y: f64,
    pub tarsus_x: f64,
    pub tarsus_y: f64,
    pub foot_x: f64,
    pub foot_y: f64,
    /// The solved multi-joint chain (procedural segmented legs, sprite rigs
    /// with `leg_chain`).
    pub chain_points: Option<Vec<(f64, f64)>>,
    /// `_draw_leg_connections`' own chain solve (jump-raised, no lift).
    pub connection_points: Option<Vec<(f64, f64)>>,
    /// The regal jumper's four-point leg (attach, knee, ankle, foot).
    pub regal_points: Option<[(f64, f64); 4]>,
}

/// One spoke of the net laid over a webbed spider (`web_net.py`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct NetSpoke {
    pub angle: f64,
    pub rings: Vec<f64>,
    pub anchor: (f64, f64),
    pub tear: f64,
    pub ring_tears: Vec<f64>,
}

/// The name label's inputs (`creature/core.py` label methods).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct LabelDraw {
    /// `name` (empty when unnamed) and `display_name` (falls back to the
    /// model's name).
    pub name: String,
    pub display_name: String,
    pub level: i64,
    /// `"full"`, `"quiet"` or `"hero"`.
    pub label_style: String,
    pub level_label_pinned: bool,
    pub health_label_pinned: bool,
    pub xp_label_pinned: bool,
    pub stamina_label_pinned: bool,
    /// `_hovered`.
    pub hovered: bool,
    /// `progression.team_id` and `team_color(team_id, team_profiles)`.
    pub team_id: String,
    pub team_color: (u8, u8, u8),
    pub hp: f64,
    pub max_hp: f64,
    pub energy: f64,
    pub max_energy: f64,
    pub xp: f64,
}

impl Default for LabelDraw {
    fn default() -> Self {
        LabelDraw {
            name: String::new(),
            display_name: "Spider".into(),
            level: 1,
            label_style: "full".into(),
            level_label_pinned: false,
            health_label_pinned: false,
            xp_label_pinned: false,
            stamina_label_pinned: false,
            hovered: false,
            team_id: "neutral".into(),
            team_color: (255, 255, 255),
            hp: 1.0,
            max_hp: 1.0,
            energy: 1.0,
            max_energy: 1.0,
            xp: 0.0,
        }
    }
}

/// `[amount, age, nudge]`, one entry of `damage_numbers`.
pub type DamageNumber = (f64, f64, f64);

/// Everything `Creature.render` reads, for one spider and one frame.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CreatureDraw {
    // --- pose ------------------------------------------------------------
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub heading: f64,
    pub jump_z: f64,
    pub body_bob: f64,
    pub body_wiggle: f64,
    pub breath_phase: f64,
    pub startle_phase: f64,
    pub crouch: f64,
    pub squash: f64,
    pub roll_spin: f64,
    pub rear: f64,
    pub abdomen_pulse: f64,
    pub ceph_pulse: f64,
    pub abdomen_wag: f64,
    pub combat_stance: f64,
    /// `combat_body_offset()`.
    pub combat_body_offset: (f64, f64),
    /// `state`; only `"Roll"` changes the drawing.
    pub state: String,
    /// `_spider_gait_config() is not None` (suppresses `body_wiggle`).
    pub spider_gait: bool,
    pub dragging: bool,
    pub dead: bool,
    pub current_speed: f64,
    // --- expression ------------------------------------------------------
    pub startled_timer: f64,
    pub startle_highlight_suppression: f64,
    pub aim_intent: f64,
    pub inspect_intent: f64,
    pub cuddle_intent: f64,
    pub catch_blend: f64,
    pub expression_blink: f64,
    pub look_fwd: f64,
    pub look_side: f64,
    pub mood: Mood,
    pub antenna_phase: [f64; 2],
    pub antenna_segment_angles: Vec<Vec<f64>>,
    pub antenna_joint_lifts: Vec<Vec<f64>>,
    pub antenna_extension: Vec<f64>,
    /// `focus_x` / `focus_y` (`_antenna_aim_angle`).
    pub focus_x: f64,
    pub focus_y: f64,
    /// Sprite-rig pedipalp armour accents.
    pub antenna_hand_targets: Vec<(f64, f64)>,
    pub regal: RegalExpression,
    // --- colour ----------------------------------------------------------
    #[serde(flatten)]
    pub palette: Palette,
    /// `personality["camouflage_opacity_drop"]` (default 0.72).
    pub camouflage_opacity_drop: f64,
    // --- body parts ------------------------------------------------------
    pub worn: Worn,
    pub legs: Vec<LegDraw>,
    // --- web net ---------------------------------------------------------
    pub webbed: bool,
    pub web_net: Vec<NetSpoke>,
    pub web_anchors: Vec<(f64, f64)>,
    pub struggle: f64,
    pub struggle_clock: f64,
    pub webbed_timer: f64,
    // --- label and numbers ----------------------------------------------
    pub label: LabelDraw,
    pub damage_numbers: Vec<DamageNumber>,
    /// `damage_team` or `progression.team_id` (`damage_color`).
    pub damage_team: Option<String>,
}

impl Default for CreatureDraw {
    fn default() -> Self {
        CreatureDraw {
            x: 0.0,
            y: 0.0,
            size: 28.0,
            heading: 0.0,
            jump_z: 0.0,
            body_bob: 0.0,
            body_wiggle: 0.0,
            breath_phase: 0.0,
            startle_phase: 0.0,
            crouch: 0.0,
            squash: 1.0,
            roll_spin: 0.0,
            rear: 0.0,
            abdomen_pulse: 0.0,
            ceph_pulse: 0.0,
            abdomen_wag: 0.0,
            combat_stance: 0.0,
            combat_body_offset: (0.0, 0.0),
            state: "Idle".into(),
            spider_gait: false,
            dragging: false,
            dead: false,
            current_speed: 0.0,
            startled_timer: 0.0,
            startle_highlight_suppression: 0.0,
            aim_intent: 0.0,
            inspect_intent: 0.0,
            cuddle_intent: 0.0,
            catch_blend: 0.0,
            expression_blink: 0.0,
            look_fwd: 0.0,
            look_side: 0.0,
            mood: Mood::default(),
            antenna_phase: [0.0, 0.0],
            antenna_segment_angles: Vec::new(),
            antenna_joint_lifts: Vec::new(),
            antenna_extension: Vec::new(),
            focus_x: 0.0,
            focus_y: 0.0,
            antenna_hand_targets: Vec::new(),
            regal: RegalExpression::default(),
            palette: Palette::default(),
            camouflage_opacity_drop: 0.72,
            worn: Worn::default(),
            legs: Vec::new(),
            webbed: false,
            web_net: Vec::new(),
            web_anchors: Vec::new(),
            struggle: 0.0,
            struggle_clock: 0.0,
            webbed_timer: 0.0,
            label: LabelDraw::default(),
            damage_numbers: Vec::new(),
            damage_team: None,
        }
    }
}

impl CreatureDraw {
    /// `_startle_amount()`.
    pub fn startle_amount(&self) -> f64 {
        if self.dragging {
            return 1.0;
        }
        dc_core::math::clamp(self.startled_timer / 1.15, 0.0, 1.0)
    }

    /// `_startle_highlight_active(startle)`.
    pub fn startle_highlight_active(&self, startle: f64) -> bool {
        startle > 0.35 && !self.dragging && self.startle_highlight_suppression <= 0.0
    }

    /// `_eye_startle_amount(startle)`.
    pub fn eye_startle_amount(&self, startle: f64) -> f64 {
        let cap = if self.dragging { 0.28 } else { 0.45 };
        dc_core::math::clamp(startle, 0.0, 1.0) * cap
    }

    /// `_basis()`: `(fx, fy, rx, ry)`.
    pub fn basis(&self) -> [f64; 4] {
        let fx = self.heading.cos();
        let fy = self.heading.sin();
        [fx, fy, -fy, fx]
    }

    /// `_body_local_to_world(forward, side)`.
    pub fn body_local_to_world(&self, forward: f64, side: f64) -> (f64, f64) {
        let [fx, fy, rx, ry] = self.basis();
        (self.x + fx * forward + rx * side, self.y + fy * forward + ry * side)
    }

    /// `_world_to_body_local(x, y)`.
    pub fn world_to_body_local(&self, x: f64, y: f64) -> (f64, f64) {
        let [fx, fy, rx, ry] = self.basis();
        let dx = x - self.x;
        let dy = y - self.y;
        (dx * fx + dy * fy, dx * rx + dy * ry)
    }

    /// `_antenna_aim_angle()`.
    pub fn antenna_aim_angle(&self) -> f64 {
        let (f, s) = self.world_to_body_local(self.focus_x, self.focus_y);
        s.atan2(f)
    }

    /// The camouflage opacity `render` applies to the whole spider.
    pub fn camouflage_opacity(&self) -> f64 {
        let strength = dc_core::math::clamp(self.palette.camouflage_strength, 0.0, 1.0);
        dc_core::math::clamp(1.0 - strength * self.camouflage_opacity_drop, 0.12, 1.0)
    }
}
