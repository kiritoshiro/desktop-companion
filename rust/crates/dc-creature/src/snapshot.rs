//! The painter-facing state of one creature for one frame.
//!
//! `Creature.render` read everything straight off `self`. Here
//! [`Creature::render_snapshot`] copies what the painters read into a
//! [`RenderSnapshot`] whose field names (and serde names) are those of
//! `dc_paint::CreatureDraw`, so the manager fills a `CreatureDraw` field by
//! field (or through serde). The legs are not copied: their geometry comes
//! from the creature's [`dc_kinematics::Kinematics`] (`creature.kin`) through
//! the `dc_kinematics::chain` helpers, which `dc-paint` already calls.

use crate::creature::{Creature, LabelStyle};
use crate::regal::RegalExpression;
use crate::web_net::NetSpoke;
use serde::{Deserialize, Serialize};

/// `dc_paint::Mood`
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MoodSnapshot {
    pub valence: f64,
    pub arousal: f64,
    pub affection: f64,
    pub curiosity: f64,
}

/// `dc_paint::LabelDraw`: the name label's inputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelSnapshot {
    pub name: String,
    pub display_name: String,
    pub level: i64,
    /// `"full"`, `"quiet"` or `"hero"`.
    pub label_style: String,
    pub level_label_pinned: bool,
    pub health_label_pinned: bool,
    pub xp_label_pinned: bool,
    pub stamina_label_pinned: bool,
    pub hovered: bool,
    pub team_id: String,
    pub team_color: (u8, u8, u8),
    pub hp: f64,
    pub max_hp: f64,
    pub energy: f64,
    pub max_energy: f64,
    pub xp: f64,
}

/// The colour inputs (`dc_paint::Palette`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaletteSnapshot {
    /// `Creature.colors` in model order, overrides applied.
    pub colors: Vec<(String, [u8; 3])>,
    /// The keys of `Creature.color_overrides`.
    pub color_overrides: Vec<String>,
    pub camouflage_color: Option<[u8; 3]>,
    pub camouflage_strength: f64,
    /// `personality["camouflage_color_blend"]`, clamped to 0..1.
    pub camouflage_color_blend: f64,
}

/// Everything `Creature.render` reads, minus the leg geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderSnapshot {
    // --- pose -----------------------------------------------------------
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
    pub combat_body_offset: (f64, f64),
    pub state: String,
    /// `_spider_gait_config() is not None`.
    pub spider_gait: bool,
    pub dragging: bool,
    pub dead: bool,
    pub current_speed: f64,
    // --- expression -----------------------------------------------------
    pub startled_timer: f64,
    pub startle_highlight_suppression: f64,
    pub aim_intent: f64,
    pub inspect_intent: f64,
    pub cuddle_intent: f64,
    pub catch_blend: f64,
    pub expression_blink: f64,
    pub look_fwd: f64,
    pub look_side: f64,
    pub mood: MoodSnapshot,
    pub antenna_phase: [f64; 2],
    pub antenna_segment_angles: Vec<Vec<f64>>,
    pub antenna_joint_lifts: Vec<Vec<f64>>,
    pub antenna_extension: Vec<f64>,
    pub focus_x: f64,
    pub focus_y: f64,
    pub antenna_hand_targets: Vec<(f64, f64)>,
    /// What `_render_regal` draws: the live gesture, or a resting one while
    /// `_regal_blocked()`.
    pub regal: RegalExpression,
    // --- colour ---------------------------------------------------------
    pub palette: PaletteSnapshot,
    pub camouflage_opacity_drop: f64,
    /// `progression.equipped`: anatomy slot -> armour id.
    pub worn: Vec<(String, String)>,
    // --- web net --------------------------------------------------------
    pub webbed: bool,
    pub web_net: Vec<NetSpoke>,
    pub web_anchors: Vec<(f64, f64)>,
    pub struggle: f64,
    pub struggle_clock: f64,
    pub webbed_timer: f64,
    // --- label and numbers ---------------------------------------------
    pub label: LabelSnapshot,
    /// `[amount, age, nudge]` per floating number.
    pub damage_numbers: Vec<(f64, f64, f64)>,
    /// `damage_team` or `progression.team_id` (`damage_color`).
    pub damage_team: Option<String>,
}

impl Creature {
    /// The painter-facing snapshot of this frame (see the module docs).
    pub fn render_snapshot(&self) -> RenderSnapshot {
        let p = self.p();
        let team_id =
            if self.progression.team_id.is_empty() { "neutral".to_string() } else { self.progression.team_id.clone() };
        let border = self.label_border_color();
        let regal = if self.regal_blocked() { RegalExpression::default() } else { self.regal.unwrap_or_default() };
        RenderSnapshot {
            x: self.kin.x,
            y: self.kin.y,
            size: self.kin.size,
            heading: self.kin.heading,
            jump_z: self.jump_z,
            body_bob: self.kin.body_bob,
            body_wiggle: self.body_wiggle,
            breath_phase: self.kin.breath_phase,
            startle_phase: self.startle_phase,
            crouch: self.crouch,
            squash: self.squash,
            roll_spin: self.kin.roll_spin,
            rear: self.rear,
            abdomen_pulse: self.kin.abdomen_pulse,
            ceph_pulse: self.kin.ceph_pulse,
            abdomen_wag: self.abdomen_wag,
            combat_stance: self.combat_stance,
            combat_body_offset: self.combat_body_offset(),
            state: self.state.as_str().to_string(),
            spider_gait: self.kin.rig.spider_gait.is_some(),
            dragging: self.dragging,
            dead: self.dead,
            current_speed: self.kin.current_speed,
            startled_timer: self.startled_timer,
            startle_highlight_suppression: self.startle_highlight_suppression,
            aim_intent: self.aim_intent,
            inspect_intent: self.kin.inspect_intent,
            cuddle_intent: self.cuddle_intent,
            catch_blend: self.kin.catch_blend,
            expression_blink: self.expression_blink,
            look_fwd: self.look_fwd,
            look_side: self.look_side,
            mood: MoodSnapshot {
                valence: self.mood.valence,
                arousal: self.mood.arousal,
                affection: self.mood.affection,
                curiosity: self.mood.curiosity,
            },
            antenna_phase: self.antenna_phase,
            antenna_segment_angles: self.antenna_segment_angles.to_vec(),
            antenna_joint_lifts: self.antenna_joint_lifts.to_vec(),
            antenna_extension: self.antenna_extension.to_vec(),
            focus_x: self.focus_x,
            focus_y: self.focus_y,
            antenna_hand_targets: self.antenna_hand_targets.to_vec(),
            regal,
            palette: PaletteSnapshot {
                colors: self.colors.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                color_overrides: self.color_overrides.keys().map(str::to_string).collect(),
                camouflage_color: self.camouflage_color,
                camouflage_strength: self.camouflage_strength,
                camouflage_color_blend: p.f64_or("camouflage_color_blend", 0.0).clamp(0.0, 1.0),
            },
            camouflage_opacity_drop: p.f64_or("camouflage_opacity_drop", 0.72),
            worn: self.progression.equipped.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            webbed: self.webbed(),
            web_net: self.web_net.clone(),
            web_anchors: self.web_anchors.clone(),
            struggle: self.struggle,
            struggle_clock: self.struggle_clock,
            webbed_timer: self.webbed_timer,
            label: LabelSnapshot {
                name: self.name.clone(),
                display_name: self.display_name(),
                level: self.level(),
                label_style: match self.label_style {
                    LabelStyle::Full => "full",
                    LabelStyle::Quiet => "quiet",
                    LabelStyle::Hero => "hero",
                }
                .to_string(),
                level_label_pinned: self.level_label_pinned(),
                health_label_pinned: self.health_label_pinned(),
                xp_label_pinned: self.xp_label_pinned(),
                stamina_label_pinned: self.stamina_label_pinned(),
                hovered: self.hovered,
                team_id: team_id.clone(),
                team_color: (border[0], border[1], border[2]),
                hp: self.hp,
                max_hp: self.max_hp,
                energy: self.energy,
                max_energy: self.max_energy,
                xp: self.xp() as f64,
            },
            damage_numbers: self.damage_numbers.iter().map(|d| (d.amount, d.age, d.nudge)).collect(),
            damage_team: Some(self.damage_team.clone().filter(|t| !t.is_empty()).unwrap_or(team_id)),
        }
    }
}
