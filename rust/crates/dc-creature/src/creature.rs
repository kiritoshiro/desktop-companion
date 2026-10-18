//! Port of `creature/core.py`: the `Creature` state and its construction.
//!
//! Python's `Creature` mixed eight classes together on one `self`. Here the
//! kinematic half lives in `kin: dc_kinematics::Kinematics` (pose, speeds,
//! targets, gait clocks, legs, roll, drift, throw inertia, catch reach) and
//! everything else is a plain field of [`Creature`], named after the Python
//! attribute without its leading underscore. Behaviour writes `kin` fields
//! directly, as the Python wrote `self.x` and friends.
//!
//! The methods are spread over this module (construction, small accessors),
//! `progression.rs` (stats, XP, equipment, damage), `manipulation.rs`
//! (drag/throw, hit test, size, screen, skills), `update.rs` (the per-frame
//! entry point), `labels.rs`, `snapshot.rs`, `expression.rs`,
//! `damage_numbers.rs`, `web_net.rs`, `regal.rs`, `arbiter.rs` and
//! `behaviour/*`.

use crate::bump;
use crate::damage_numbers::DamageNumber;
use crate::mood::Mood;
use crate::phase_scheduler::BehaviourPhaseScheduler;
use crate::pymath::{Dict, choice_sign, clamp, py_float, rand_range};
use crate::regal::RegalExpression;
use crate::state::State;
use crate::web_net::NetSpoke;
use crate::world::{CageRect, CreatureId, FlyId, PreyView, ShotKind, SocialRole, WebId};
use dc_content::OrderedMap;
use dc_content::personality::Personality;
use dc_content::personality_profiles::{PersonalityRef, behaviour_modules_for};
use dc_content::skills::{SkillSet, default_skills_for_personality};
use dc_core::PyRandom;
use dc_kinematics::{FrameCtx, GaitStyle, HeldInput, Kinematics, PlayfieldClamp, Rig};
use dc_state::progression::{ProgressionState, Stances};
use dc_state::teams::TeamProfiles;
use dc_world::Playfield;
use serde_json::Value;
use std::collections::HashSet;
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

/// `render_procedural.CURVED_LEGS`
pub const CURVED_LEGS: bool = true;

/// Behaviour modules a personality selects (`behaviour_modules_for`), as bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modules(u8);

impl Modules {
    pub const HUNTER: u8 = 1;
    pub const JUMPER: u8 = 2;
    pub const OBSERVER: u8 = 4;
    pub const NOPE: u8 = 8;
    pub const DRIFTER: u8 = 16;
    pub const WEBBER: u8 = 32;
    pub const WEB_SHOOTER: u8 = 64;

    pub fn from_personality(p: &Personality) -> Self {
        let mut bits = 0;
        for m in behaviour_modules_for(PersonalityRef::Def(p)) {
            bits |= match m {
                "hunter" => Self::HUNTER,
                "jumper" => Self::JUMPER,
                "observer" => Self::OBSERVER,
                "nope" => Self::NOPE,
                "drifter" => Self::DRIFTER,
                "webber" => Self::WEBBER,
                "web_shooter" => Self::WEB_SHOOTER,
                _ => 0,
            };
        }
        Modules(bits)
    }

    #[inline]
    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }
}

/// `drift_run_mode`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DriftRunMode {
    #[default]
    Circle,
    Corner,
}

/// `drift_run_phase`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DriftRunPhase {
    #[default]
    Charge,
    Slide,
}

/// What a jump, aim or coil hands over to on landing (`jump_after`,
/// `aim_after`, `coil_after`, `land_after`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum After {
    #[default]
    Idle,
    Land,
    Outcome,
    Strike,
    Nope,
    Play,
    Wander,
    Approach,
    Chase,
    /// Any other string: lands into `enter_idle`, as Python's `else` branch.
    Other,
}

impl After {
    pub fn parse(s: &str) -> After {
        match s {
            "idle" => After::Idle,
            "land" => After::Land,
            "outcome" => After::Outcome,
            "strike" => After::Strike,
            "nope" => After::Nope,
            "play" => After::Play,
            "wander" => After::Wander,
            "approach" => After::Approach,
            "chase" => After::Chase,
            _ => After::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            After::Idle => "idle",
            After::Land => "land",
            After::Outcome => "outcome",
            After::Strike => "strike",
            After::Nope => "nope",
            After::Play => "play",
            After::Wander => "wander",
            After::Approach => "approach",
            After::Chase => "chase",
            After::Other => "other",
        }
    }
}

/// `jump_kind`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum JumpKind {
    #[default]
    Hop,
    Escape,
    /// `"pounce"` and every other string (Python's `else` branch).
    Pounce,
}

impl JumpKind {
    pub fn parse(s: &str) -> JumpKind {
        match s {
            "hop" => JumpKind::Hop,
            "escape" => JumpKind::Escape,
            _ => JumpKind::Pounce,
        }
    }
}

/// `inspect_phase`, `cuddle_phase`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InspectPhase {
    #[default]
    Approach,
    Study,
    Orbit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CuddlePhase {
    #[default]
    Approach,
    Snuggle,
}

/// `range_mode`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RangeMode {
    #[default]
    Waggle,
    Nod,
}

/// `label_style`: "full", "quiet" or "hero".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LabelStyle {
    #[default]
    Full,
    Quiet,
    Hero,
}

/// Construction arguments (`Creature.__init__`).
#[derive(Clone, Debug)]
pub struct CreatureConfig {
    /// Stable identity the manager uses for this creature.
    pub id: CreatureId,
    /// The resolved model dict (after `resolve_body_plan`).
    pub model: Arc<Value>,
    /// The personality dict, as Python passes it.
    pub personality: Arc<Value>,
    pub screen_w: f64,
    pub screen_h: f64,
    pub index: i64,
    pub size_scale: f64,
    /// `None` = `default_skills_for_personality(personality)`.
    pub skills: Option<Vec<String>>,
    pub gait_style: String,
    pub color_overrides: Option<Value>,
    pub progression_state: Option<Value>,
    pub progression_id: Option<String>,
    pub job_id: String,
    /// `None`: an unseeded creature (Python drew from the module-level
    /// `random`; here from an entropy-seeded stream).
    pub seed: Option<i64>,
}

impl CreatureConfig {
    pub fn new(model: Value, personality: Value, screen_w: f64, screen_h: f64) -> Self {
        CreatureConfig {
            id: 0,
            model: Arc::new(model),
            personality: Arc::new(personality),
            screen_w,
            screen_h,
            index: 0,
            size_scale: 1.0,
            skills: None,
            gait_style: "classic".into(),
            color_overrides: None,
            progression_state: None,
            progression_id: None,
            job_id: "none".into(),
            seed: None,
        }
    }
}

/// `Playfield` as the kinematics' clamp (only when it is split).
#[derive(Clone)]
pub struct PfClamp(pub Arc<Playfield>);

impl PlayfieldClamp for PfClamp {
    fn clamp_point(&self, x: f64, y: f64, margin: f64) -> (f64, f64) {
        self.0.clamp(x, y, margin)
    }
}

/// A typed `Personality` for the content functions, built from a dict that
/// may lack the keys a personality *file* must have.
pub fn typed_personality(value: &Value) -> Personality {
    let mut map = value.as_object().cloned().unwrap_or_default();
    let id = map.get("id").map(crate::pymath::py_str).unwrap_or_default();
    let defaults: [(&str, Value); 6] = [
        ("id", Value::from(id.clone())),
        ("display_name", Value::from(id)),
        ("speed_multiplier", Value::from(1.0)),
        ("reaction_radius", Value::from(360.0)),
        ("boldness", Value::from(0.5)),
        ("wander_frequency", Value::from(0.5)),
    ];
    for (k, v) in defaults {
        let bad = match map.get(k) {
            None => true,
            Some(existing) => match k {
                "id" | "display_name" => !existing.is_string(),
                _ => py_float(existing).is_none(),
            },
        };
        if bad {
            map.insert(k.to_string(), v);
        }
    }
    let mut full = Value::Object(map.clone());
    if let Ok(p) = Personality::from_value(full.clone()) {
        return p;
    }
    // Some composition key has the wrong shape: drop the typed ones and keep
    // the flat tunables.
    for k in [
        "temperament",
        "movement_profile",
        "phase_scores",
        "phase_duration_multiplier",
        "skills",
        "behaviours",
        "abilities",
        "ability_bundles",
        "include_common_abilities",
        "behaviour_modules",
        "ability_ids",
        "mood",
        "description",
        "threat_radius",
        "threat_cursor_speed",
        "_canonical",
        "_path",
    ] {
        map.remove(k);
    }
    full = Value::Object(map);
    Personality::from_value(full).expect("a sanitised personality always parses")
}

/// One live desktop creature.
#[derive(Clone, Debug)]
pub struct Creature {
    // --- identity and data ----------------------------------------------
    pub id: CreatureId,
    pub model: Arc<Value>,
    pub personality: Arc<Value>,
    /// Typed copy of `personality` for the content functions.
    pub personality_def: Arc<Personality>,
    /// `_behaviour_modules()`, recomputed with the personality.
    pub modules: Modules,
    pub screen_w: f64,
    pub screen_h: f64,
    pub margin: f64,
    pub index: i64,
    pub progression_id: String,
    pub seeded: bool,
    /// `self.rng`
    pub rng: PyRandom,
    /// The scheduler's private stream of an unseeded creature; a seeded one
    /// shares `rng` (Python passed `rng=self.rng`).
    pub scheduler_rng: Option<PyRandom>,
    pub skills: SkillSet,
    pub phase_scheduler: BehaviourPhaseScheduler,
    pub progression: ProgressionState,
    pub job_id: String,
    pub job_mode: String,
    pub job_target: Option<(f64, f64)>,
    pub job_alert_target: Option<CreatureId>,
    pub job_base_id: Option<String>,
    pub job_facing: Option<f64>,
    pub job_busy: bool,
    pub team_stances: Option<Arc<Stances>>,
    pub team_profiles: Option<Arc<TeamProfiles>>,
    pub size_scale: f64,
    pub progression_base_size: f64,

    // --- kinematics -----------------------------------------------------
    pub kin: Kinematics,
    pub active_gait_index: i64,
    pub stepping_group: Option<i64>,

    // --- FSM ------------------------------------------------------------
    pub state: State,
    /// `player_control is not None`: a `PlayerController` drives it.
    pub player_control: bool,
    /// `getattr(player_control, "struggling", False)`
    pub player_struggling: bool,
    pub chooses_own_skills: bool,
    pub reverse_walk: bool,
    pub state_timer: f64,
    pub decision_timer: f64,
    pub chase_timer: f64,
    pub hop_timer: f64,
    pub nope_repeats: i64,
    pub nope_zigzag_dir: f64,
    pub nope_cooldown: f64,

    // --- direct manipulation -------------------------------------------
    pub dragging: bool,
    pub grab_offset_x: f64,
    pub grab_offset_y: f64,
    pub drag_vel_x: f64,
    pub drag_vel_y: f64,
    pub held_leg_relax: f64,
    pub held_pose_clock: f64,
    pub held_drag_response: f64,
    pub held_drag_sway_x: f64,
    pub held_drag_sway_y: f64,
    pub held_prev_drag_vx: f64,
    pub held_prev_drag_vy: f64,
    pub held_drag_accel_x: f64,
    pub held_drag_accel_y: f64,
    pub held_release_timer: f64,
    pub throw_recovery: f64,
    pub startled_timer: f64,
    pub startle_highlight_suppression: f64,
    pub startle_phase: f64,

    // --- drift runs -----------------------------------------------------
    pub drift_run_cooldown: f64,
    pub drift_run_mode: DriftRunMode,
    pub drift_run_center_x: f64,
    pub drift_run_center_y: f64,
    pub drift_run_radius: f64,
    pub drift_run_angle: f64,
    pub drift_run_dir: f64,
    pub drift_run_turn_rate: f64,
    pub drift_run_speed: f64,
    pub drift_run_corner_index: usize,
    pub drift_run_phase: DriftRunPhase,
    pub drift_run_phase_timer: f64,

    // --- cursor ---------------------------------------------------------
    pub prev_mx: Option<f64>,
    pub prev_my: Option<f64>,
    pub prev_cursor_vx: f64,
    pub prev_cursor_vy: f64,

    // --- colours and labels --------------------------------------------
    pub colors: OrderedMap<[u8; 3]>,
    pub color_overrides: OrderedMap<[u8; 3]>,
    pub force_show_level: bool,
    pub force_show_health: bool,
    pub force_show_xp: bool,
    pub force_show_stamina: bool,
    pub label_style: LabelStyle,
    pub name: String,
    pub hovered: bool,

    // --- nerve ----------------------------------------------------------
    pub flee_timer: f64,
    pub flee_from: Option<CreatureId>,
    pub escaped_timer: f64,
    pub recovering: bool,
    pub engagement_timer: f64,

    // --- mood, expression, posture -------------------------------------
    pub mood_mode: String,
    pub mood: Mood,
    pub expression_blink: f64,
    pub blink_timer: f64,
    pub head_tilt: f64,
    pub look_fwd: f64,
    pub look_side: f64,
    pub body_wiggle: f64,
    pub abdomen_wag: f64,
    pub crouch: f64,
    pub rear: f64,
    pub cursor_pressure: f64,
    pub combat_stance: f64,
    pub lunge: f64,
    pub strike_clock: Option<f64>,
    pub strike_face: (f64, f64),
    pub combat_face_x: f64,
    pub combat_face_y: f64,
    pub squash: f64,
    pub wiggle_phase: f64,
    pub wiggle_amp: f64,

    // --- jumping --------------------------------------------------------
    pub jump_z: f64,
    pub airborne: bool,
    pub jump_t: f64,
    pub jump_duration: f64,
    pub jump_peak: f64,
    pub jump_from: (f64, f64),
    pub jump_to: (f64, f64),
    pub jump_after: After,
    pub jump_kind: JumpKind,
    pub land_recover: f64,
    pub land_after: After,

    // --- focus and antennae --------------------------------------------
    pub focus_x: f64,
    pub focus_y: f64,
    pub focus_strength: f64,
    pub antenna_phase: [f64; 2],
    pub antenna_segment_angles: [Vec<f64>; 2],
    pub antenna_joint_lifts: [Vec<f64>; 2],
    pub antenna_extension: [f64; 2],
    pub antenna_hand_targets: [(f64, f64); 2],
    pub antenna_hand_grips: [f64; 2],
    pub aim_intent: f64,
    pub cuddle_intent: f64,

    // --- social ---------------------------------------------------------
    /// `self.neighbors`, as ids (set by the manager, includes this creature).
    pub neighbors: Vec<CreatureId>,
    pub social_target: Option<CreatureId>,
    pub social_role: SocialRole,
    pub social_cooldown: f64,
    pub allow_social: bool,

    // --- containment and desktop awareness -----------------------------
    pub cage: Option<CageRect>,
    pub playfield: Option<Arc<Playfield>>,
    pub desktop_visibility_alpha: f64,
    pub desktop_hidden_timer: f64,
    pub desktop_fully_hidden: bool,
    pub desktop_occluding_keys: HashSet<String>,
    pub desktop_seek_timer: f64,
    pub desktop_hide_surface_key: Option<String>,
    pub desktop_hide_timer: f64,
    pub desktop_hide_kind: Option<String>,
    pub desktop_spawn_grace: f64,
    pub desktop_surface_keys_seen_outside: HashSet<String>,
    pub folder_portal_cooldown: f64,
    pub folder_portal_dwell: f64,
    pub folder_portal_threshold: f64,
    pub camouflage_color: Option<[u8; 3]>,
    pub camouflage_strength: f64,
    pub camouflage_idle_timer: f64,
    pub camouflage_visible_timer: f64,

    // --- gameplay stats -------------------------------------------------
    pub max_hp: f64,
    pub hp: f64,
    pub max_energy: f64,
    pub energy: f64,
    pub armor: f64,
    pub damage: f64,
    pub dead: bool,
    pub last_attacker: Option<CreatureId>,
    pub webbed_timer: f64,
    pub webbed_by: Option<CreatureId>,
    pub web_anchors: Vec<(f64, f64)>,
    pub web_net: Vec<NetSpoke>,
    pub struggle: f64,
    pub struggle_clock: f64,
    pub damage_numbers: Vec<DamageNumber>,
    /// Remains keep the side of the spider they were (`damage_team`).
    pub damage_team: Option<String>,
    pub foe: Option<CreatureId>,
    pub hurt_flash: f64,
    pub attack_cooldown: f64,
    pub combat_base_hp: f64,
    pub combat_base_energy: f64,
    pub combat_base_armor: f64,
    pub combat_base_damage: f64,
    pub progression_size_multiplier: f64,
    pub progression_speed_multiplier: f64,
    /// A mission enemy's (health, bite, shell) multipliers (`stat_scale`).
    pub stat_scale: Option<(f64, f64, f64)>,
    pub bbox: Option<(f64, f64, f64, f64)>,
    pub bbox_prev: Option<(f64, f64, f64, f64)>,

    // --- webs -----------------------------------------------------------
    pub weaving_web: Option<WebId>,
    pub repairing_web: Option<WebId>,
    pub web_target: Option<WebId>,
    pub weave_drawing: bool,
    pub web_pluck_count: i64,
    pub web_pluck_timer: f64,
    pub web_wiggle: f64,
    pub weave_cooldown: f64,
    pub web_walk_cooldown: f64,
    pub weave_speed: f64,
    pub web_walk_point: Option<(f64, f64)>,
    pub web_shot_kind: ShotKind,
    pub web_shot_cooldown: f64,

    // --- hunting --------------------------------------------------------
    pub hunting_prey: bool,
    /// `self._prey`: the manager refreshes this view every frame.
    pub prey: Option<PreyView>,
    pub prey_recheck: f64,
    pub feed_cooldown: f64,
    pub pounce_cooldown: f64,
    pub trap_shot_cooldown: f64,
    pub feed_frenzy: f64,
    pub feed_anchor: (f64, f64),
    pub feed_paw: f64,
    pub web_shot_prey: Option<FlyId>,
    pub web_shot_foe: Option<CreatureId>,

    // --- per-state scratch (set by the enter_* methods) -----------------
    pub inspect_phase: InspectPhase,
    pub inspect_clock: f64,
    pub orbit_dir: f64,
    pub orbit_clock: f64,
    pub observe_dir: f64,
    pub observe_clock: f64,
    /// `getattr(self, "observe_radius", (lo + hi) * 0.5)`
    pub observe_radius: Option<f64>,
    pub observe_vertical_scale: f64,
    pub observe_horizontal_scale: f64,
    pub cuddle_phase: CuddlePhase,
    pub cuddle_clock: f64,
    pub boop_timer: f64,
    pub aim_after: After,
    pub aim_abort_chance: f64,
    pub range_clock: f64,
    pub range_mode: RangeMode,
    pub range_switch: f64,
    pub coil_after: After,
    pub coil_power: f64,
    pub coil_toward: Option<(f64, f64)>,
    pub catch_resolved: bool,
    pub play_clock: f64,
    pub play_lookback: f64,
    pub zoom_repeats: i64,

    // --- regal jumper ---------------------------------------------------
    pub regal: Option<RegalExpression>,
}

impl Creature {
    /// `Creature.__init__`: every random draw in the Python order.
    pub fn new(cfg: CreatureConfig) -> Creature {
        let personality = cfg.personality.clone();
        let p = Dict::of(&personality);
        let personality_def = Arc::new(typed_personality(&personality));
        let screen_w = (cfg.screen_w.trunc()).max(200.0);
        let screen_h = (cfg.screen_h.trunc()).max(200.0);
        let margin = 50.0;
        let progression_id = cfg.progression_id.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("runtime:{}", cfg.index));
        let seeded = cfg.seed.is_some();
        let mut rng = match cfg.seed {
            Some(seed) => PyRandom::from_str_seed(&format!("{seed}:{progression_id}")),
            None => PyRandom::from_entropy(),
        };
        let skills = match &cfg.skills {
            Some(list) => SkillSet::new(Some(list.as_slice())),
            None => {
                let ids = default_skills_for_personality(Some(PersonalityRef::Def(&personality_def)));
                SkillSet::new(Some(ids.as_slice()))
            }
        };
        let mut scheduler_rng = if seeded { None } else { Some(PyRandom::from_entropy()) };
        let phase_scheduler = {
            let r = scheduler_rng.as_mut().unwrap_or(&mut rng);
            BehaviourPhaseScheduler::new(&personality_def, &skills.ids(), r)
        };
        let progression = match &cfg.progression_state {
            Some(v) => ProgressionState::from_value(v),
            None => ProgressionState::default(),
        };
        let job_id = dc_content::jobs::normalize_job_id(Some(&cfg.job_id)).to_string();
        let size_scale = clamp(cfg.size_scale, 0.45, 2.25);
        // Keep the old random draw in the seeded stream (core.py:173).
        rng.random();
        let base_size = 28.0;
        let size = base_size * size_scale;
        let x = rng.uniform(margin, screen_w - margin);
        let y = rng.uniform(margin, screen_h - margin);
        let heading = rng.uniform(-PI, PI);
        let turn_rate = rng.uniform(4.0, 6.0);
        let state_timer = rand_range(p.get("idle_time"), 1.0, 3.0, &mut rng);
        let decision_timer = rng.uniform(0.2, 0.5);
        let hop_timer = rand_range(p.get("hop_interval"), 0.25, 0.65, &mut rng);
        let nope_zigzag_dir = choice_sign(&mut rng);
        let startle_phase = rng.random() * TAU;
        let drift_phase = rng.random() * TAU;
        let drift_dir = choice_sign(&mut rng);
        let drift_flip_timer = rand_range(p.get("drift_switch_time"), 0.55, 1.35, &mut rng);
        let drift_run_cooldown = rand_range(p.get("drift_run_interval"), 4.5, 10.5, &mut rng);
        let drift_run_radius = (size * 2.8).max(70.0);
        let drift_run_dir = choice_sign(&mut rng);
        let bob_phase = rng.random() * TAU;
        let breath_phase = rng.random() * TAU;

        let mut colors = OrderedMap::new();
        if let Some(Value::Object(raw)) = cfg.model.get("colors") {
            for (key, value) in raw {
                if let Some(rgb) = rgb_of(value, false) {
                    colors.insert(key.clone(), rgb);
                }
            }
        }
        let mut color_overrides = OrderedMap::new();
        if let Some(Value::Object(raw)) = &cfg.color_overrides {
            for (key, value) in raw {
                if let Some(rgb) = rgb_of(value, true) {
                    color_overrides.insert(key.clone(), rgb);
                    colors.insert(key.clone(), rgb);
                }
            }
        }

        let rig = Rig::from_model_json(&cfg.model, &personality, CURVED_LEGS);
        let gait_style = dc_kinematics::constants::normalize_gait_style(Some(&cfg.gait_style));
        let gait_rng = PyRandom::from_str_seed(&format!("{progression_id}:gait-spell"));
        let mut kin = Kinematics::new(rig, gait_style, gait_rng, x, y, heading, size);
        kin.turn_rate = turn_rate;
        kin.bob_phase = bob_phase;
        kin.breath_phase = breath_phase;
        kin.drift.drift_phase = drift_phase;
        kin.drift.drift_dir = drift_dir;
        kin.drift.drift_flip_timer = drift_flip_timer;
        kin.seed_legs(&mut rng);
        let group_count = kin.gait_groups.len() as i64;
        let active_gait_index = rng.randrange(group_count);
        kin.seed_gait_clocks(&mut rng);

        let mood_mode = p.get("mood").map(crate::pymath::py_str).unwrap_or_else(|| "auto".into()).to_lowercase();
        let mut mood = Mood::default();
        let baseline = if mood_mode != "auto" { mood_mode.clone() } else { p.str_or("id", "auto") };
        mood.set_baseline_named(&baseline);
        let (bv, ba, baf, bc) =
            (rng.uniform(-0.12, 0.12), rng.uniform(-0.10, 0.10), rng.uniform(-0.10, 0.10), rng.uniform(-0.10, 0.10));
        bump!(mood, valence = bv, arousal = ba, affection = baf, curiosity = bc);
        let blink_timer = rng.uniform(1.5, 5.0);
        let wiggle_phase = rng.random() * TAU;
        let antenna_phase = [rng.random() * TAU, rng.random() * TAU];
        let social_cooldown = rng.uniform(1.5, 5.0);
        let desktop_seek_timer = rand_range(p.get("desktop_hide_seek_interval"), 28.0, 64.0, &mut rng);
        let folder_portal_cooldown = rand_range(p.get("folder_portal_cooldown"), 24.0, 55.0, &mut rng);
        let folder_portal_threshold = rand_range(p.get("folder_portal_dwell"), 0.85, 1.39, &mut rng);
        let camouflage_visible_timer = rand_range(p.get("camouflage_initial_visible_time"), 3.5, 6.0, &mut rng);

        let combat = Dict::of(&cfg.model).dict("combat");
        let combat_number = |key: &str, default: f64, minimum: f64| -> f64 {
            match combat.get(key) {
                None => minimum.max(default),
                Some(v) => match py_float(v) {
                    Some(f) => minimum.max(f),
                    None => default,
                },
            }
        };

        let mut c = Creature {
            id: cfg.id,
            model: cfg.model.clone(),
            personality: personality.clone(),
            modules: Modules::from_personality(&personality_def),
            personality_def,
            screen_w,
            screen_h,
            margin,
            index: cfg.index,
            progression_id,
            seeded,
            rng,
            scheduler_rng,
            skills,
            phase_scheduler,
            progression,
            job_id,
            job_mode: "idle".into(),
            job_target: None,
            job_alert_target: None,
            job_base_id: None,
            job_facing: None,
            job_busy: false,
            team_stances: None,
            team_profiles: None,
            size_scale,
            progression_base_size: base_size,
            kin,
            active_gait_index,
            stepping_group: None,
            state: State::Idle,
            player_control: false,
            player_struggling: false,
            chooses_own_skills: false,
            reverse_walk: false,
            state_timer,
            decision_timer,
            chase_timer: 0.0,
            hop_timer,
            nope_repeats: 0,
            nope_zigzag_dir,
            nope_cooldown: 0.0,
            dragging: false,
            grab_offset_x: 0.0,
            grab_offset_y: 0.0,
            drag_vel_x: 0.0,
            drag_vel_y: 0.0,
            held_leg_relax: 0.0,
            held_pose_clock: 0.0,
            held_drag_response: 0.0,
            held_drag_sway_x: 0.0,
            held_drag_sway_y: 0.0,
            held_prev_drag_vx: 0.0,
            held_prev_drag_vy: 0.0,
            held_drag_accel_x: 0.0,
            held_drag_accel_y: 0.0,
            held_release_timer: 0.0,
            throw_recovery: 0.0,
            startled_timer: 0.0,
            startle_highlight_suppression: 0.0,
            startle_phase,
            drift_run_cooldown,
            drift_run_mode: DriftRunMode::Circle,
            drift_run_center_x: x,
            drift_run_center_y: y,
            drift_run_radius,
            drift_run_angle: heading,
            drift_run_dir,
            drift_run_turn_rate: 2.8,
            drift_run_speed: 150.0,
            drift_run_corner_index: 0,
            drift_run_phase: DriftRunPhase::Charge,
            drift_run_phase_timer: 0.0,
            prev_mx: None,
            prev_my: None,
            prev_cursor_vx: 0.0,
            prev_cursor_vy: 0.0,
            colors,
            color_overrides,
            force_show_level: false,
            force_show_health: false,
            force_show_xp: false,
            force_show_stamina: false,
            label_style: LabelStyle::Full,
            name: String::new(),
            hovered: false,
            flee_timer: 0.0,
            flee_from: None,
            escaped_timer: 0.0,
            recovering: false,
            engagement_timer: 0.0,
            mood_mode,
            mood,
            expression_blink: 0.0,
            blink_timer,
            head_tilt: 0.0,
            look_fwd: 1.0,
            look_side: 0.0,
            body_wiggle: 0.0,
            abdomen_wag: 0.0,
            crouch: 0.0,
            rear: 0.0,
            cursor_pressure: 0.0,
            combat_stance: 0.0,
            lunge: 0.0,
            strike_clock: None,
            strike_face: (1.0, 0.0),
            combat_face_x: 0.0,
            combat_face_y: 0.0,
            squash: 1.0,
            wiggle_phase,
            wiggle_amp: 0.0,
            jump_z: 0.0,
            airborne: false,
            jump_t: 0.0,
            jump_duration: 0.0,
            jump_peak: 0.0,
            jump_from: (x, y),
            jump_to: (x, y),
            jump_after: After::Land,
            jump_kind: JumpKind::Hop,
            land_recover: 0.0,
            land_after: After::Idle,
            focus_x: x,
            focus_y: y,
            focus_strength: 0.0,
            antenna_phase,
            antenna_segment_angles: [Vec::new(), Vec::new()],
            antenna_joint_lifts: [Vec::new(), Vec::new()],
            antenna_extension: [0.0, 0.0],
            antenna_hand_targets: [(0.0, 0.0), (0.0, 0.0)],
            antenna_hand_grips: [0.0, 0.0],
            aim_intent: 0.0,
            cuddle_intent: 0.0,
            neighbors: Vec::new(),
            social_target: None,
            social_role: SocialRole::None,
            social_cooldown,
            allow_social: true,
            cage: None,
            playfield: None,
            desktop_visibility_alpha: 1.0,
            desktop_hidden_timer: 0.0,
            desktop_fully_hidden: false,
            desktop_occluding_keys: HashSet::new(),
            desktop_seek_timer,
            desktop_hide_surface_key: None,
            desktop_hide_timer: 0.0,
            desktop_hide_kind: None,
            desktop_spawn_grace: 2.4,
            desktop_surface_keys_seen_outside: HashSet::new(),
            folder_portal_cooldown,
            folder_portal_dwell: 0.0,
            folder_portal_threshold,
            camouflage_color: None,
            camouflage_strength: 0.0,
            camouflage_idle_timer: 0.0,
            camouflage_visible_timer,
            max_hp: 100.0,
            hp: 100.0,
            max_energy: 100.0,
            energy: 100.0,
            armor: 0.0,
            damage: 8.0,
            dead: false,
            last_attacker: None,
            webbed_timer: 0.0,
            webbed_by: None,
            web_anchors: Vec::new(),
            web_net: Vec::new(),
            struggle: 0.0,
            struggle_clock: 0.0,
            damage_numbers: Vec::new(),
            damage_team: None,
            foe: None,
            hurt_flash: 0.0,
            attack_cooldown: 0.0,
            combat_base_hp: combat_number("base_hp", 100.0, 1.0),
            combat_base_energy: combat_number("base_energy", 100.0, 1.0),
            combat_base_armor: combat_number("base_armor", 0.0, 0.0),
            combat_base_damage: combat_number("base_damage", 8.0, 0.1),
            progression_size_multiplier: 1.0,
            progression_speed_multiplier: 1.0,
            stat_scale: None,
            bbox: None,
            bbox_prev: None,
            weaving_web: None,
            repairing_web: None,
            web_target: None,
            weave_drawing: false,
            web_pluck_count: 0,
            web_pluck_timer: 0.0,
            web_wiggle: 0.0,
            weave_cooldown: 0.0,
            web_walk_cooldown: 0.0,
            weave_speed: 0.0,
            web_walk_point: None,
            web_shot_kind: ShotKind::Trap,
            web_shot_cooldown: 0.0,
            hunting_prey: false,
            prey: None,
            prey_recheck: 0.0,
            feed_cooldown: 0.0,
            pounce_cooldown: 0.0,
            trap_shot_cooldown: 0.0,
            feed_frenzy: 0.0,
            feed_anchor: (x, y),
            feed_paw: 0.0,
            web_shot_prey: None,
            web_shot_foe: None,
            inspect_phase: InspectPhase::Approach,
            inspect_clock: 0.0,
            orbit_dir: 1.0,
            orbit_clock: 0.0,
            observe_dir: 1.0,
            observe_clock: 0.0,
            observe_radius: None,
            observe_vertical_scale: 0.58,
            observe_horizontal_scale: 1.18,
            cuddle_phase: CuddlePhase::Approach,
            cuddle_clock: 0.0,
            boop_timer: 0.0,
            aim_after: After::Outcome,
            aim_abort_chance: 0.16,
            range_clock: 0.0,
            range_mode: RangeMode::Waggle,
            range_switch: 0.2,
            coil_after: After::Idle,
            coil_power: 1.0,
            coil_toward: None,
            catch_resolved: false,
            play_clock: 0.0,
            play_lookback: 0.0,
            zoom_repeats: 0,
            regal: None,
        };
        c.apply_progression_stats(true, false);

        let weave_bias = if c.acts_as_webber() { 4.0 } else { 1.0 };
        c.weave_cooldown = rand_range(c.personality.get("weave_cooldown"), 6.0, 14.0, &mut c.rng) / weave_bias;
        c.web_walk_cooldown = rand_range(c.personality.get("web_walk_cooldown"), 8.0, 20.0, &mut c.rng);
        c.weave_speed = rand_range(c.personality.get("weave_speed"), 250.0, 330.0, &mut c.rng);
        let shot_bias = if c.acts_as_web_shooter() { 5.0 } else { 1.0 };
        c.web_shot_cooldown = rand_range(c.personality.get("web_shot_cooldown"), 8.0, 18.0, &mut c.rng) / shot_bias;
        c.pounce_cooldown = c.rng.uniform(0.0, 0.7);
        c.trap_shot_cooldown = c.rng.uniform(0.4, 1.6);

        c.kin.compute_leg_sectors();
        c.initialize_legs();
        c
    }

    /// Replace the personality dict (the settings window can rewrite a live
    /// creature's personality) and recompute everything derived from it.
    /// The phase scheduler keeps its deck; Python rebuilt only on demand.
    pub fn set_personality(&mut self, personality: Value) {
        self.personality = Arc::new(personality);
        self.personality_def = Arc::new(typed_personality(&self.personality));
        self.modules = Modules::from_personality(&self.personality_def);
        let legs = self.kin.rig.legs.clone();
        let mut rig = Rig::from_model_json(&self.model, &self.personality, CURVED_LEGS);
        rig.legs = legs;
        self.kin.rig = rig;
    }

    // -----------------------------------------------------------------
    // Small helpers
    // -----------------------------------------------------------------

    /// `self.personality` as a dict.
    #[inline]
    pub fn p(&self) -> Dict<'_> {
        Dict::of(&self.personality)
    }

    /// `float(self.personality.get(key, default))`
    #[inline]
    pub fn pf(&self, key: &str, default: f64) -> f64 {
        self.p().f64_or(key, default)
    }

    /// `self.model.get("appearance", {})`
    #[inline]
    pub fn appearance(&self) -> Dict<'_> {
        Dict::of(&self.model).dict("appearance")
    }

    /// `(self.scheduler, the stream it draws from)`.
    #[inline]
    pub fn scheduler_parts(&mut self) -> (&mut BehaviourPhaseScheduler, &mut PyRandom) {
        let rng = match self.scheduler_rng.as_mut() {
            Some(r) => r,
            None => &mut self.rng,
        };
        (&mut self.phase_scheduler, rng)
    }

    /// `self._split_playfield()` as the kinematics' clamp.
    #[inline]
    pub fn split_playfield(&self) -> Option<PfClamp> {
        self.playfield.as_ref().filter(|p| !p.simple()).map(|p| PfClamp(p.clone()))
    }

    /// The kinematics' per-call view of this creature.
    pub fn kctx<'p>(&self, playfield: Option<&'p dyn PlayfieldClamp>) -> FrameCtx<'p> {
        FrameCtx {
            state: self.state.motion(),
            fleeing: self.fleeing(),
            acts_as_observer: self.acts_as_observer(),
            acts_as_drifter: self.acts_as_drifter(),
            reverse_walk: self.reverse_walk,
            has_job_facing: self.job_facing.is_some(),
            airborne: self.airborne,
            dragging: self.dragging,
            webbed_held: self.webbed_held(),
            startled_timer: self.startled_timer,
            drift_run_sliding: self.drift_run_phase == DriftRunPhase::Slide,
            held_release_timer: self.held_release_timer,
            focus_strength: self.focus_strength,
            focus_x: self.focus_x,
            focus_y: self.focus_y,
            held: HeldInput {
                drag_response: self.held_drag_response,
                drag_accel_x: self.held_drag_accel_x,
                drag_accel_y: self.held_drag_accel_y,
                pose_clock: self.held_pose_clock,
                drag_sway_x: self.held_drag_sway_x,
                drag_sway_y: self.held_drag_sway_y,
                leg_relax: self.held_leg_relax,
                drag_vel_x: self.drag_vel_x,
                drag_vel_y: self.drag_vel_y,
            },
            screen_w: self.screen_w,
            screen_h: self.screen_h,
            margin: self.margin,
            playfield,
        }
    }

    /// `_initialize_legs()`
    pub fn initialize_legs(&mut self) {
        let pf = self.split_playfield();
        let ctx = self.kctx(pf.as_ref().map(|p| p as &dyn PlayfieldClamp));
        self.kin.initialize_legs(&ctx, &mut self.rng);
    }

    /// `_prime_drift(boost, direction)`
    pub fn prime_drift(&mut self, boost: f64, direction: Option<f64>) {
        if !self.acts_as_drifter() {
            return;
        }
        let ctx = self.kctx(None);
        self.kin.prime_drift(boost, direction, &ctx, &mut self.rng);
    }

    /// `_panic_rehome_legs(intensity)`
    pub fn panic_rehome_legs(&mut self, intensity: f64) {
        let pf = self.split_playfield();
        let ctx = self.kctx(pf.as_ref().map(|p| p as &dyn PlayfieldClamp));
        self.kin.panic_rehome_legs(intensity, &ctx, &mut self.rng);
    }

    /// `_set_focus(x, y, strength)`
    #[inline]
    pub fn set_focus(&mut self, x: f64, y: f64, strength: f64) {
        self.focus_x = x;
        self.focus_y = y;
        self.focus_strength = clamp(strength, 0.0, 1.0);
    }

    /// `wiggle_burst` (shared with the kinematics' drift state).
    #[inline]
    pub fn wiggle_burst(&self) -> f64 {
        self.kin.drift.wiggle_burst
    }

    #[inline]
    pub fn set_wiggle_burst(&mut self, value: f64) {
        self.kin.drift.wiggle_burst = value;
    }

    /// `self.wiggle_burst = max(self.wiggle_burst, value)`
    #[inline]
    pub fn raise_wiggle_burst(&mut self, value: f64) {
        self.kin.drift.wiggle_burst = self.kin.drift.wiggle_burst.max(value);
    }

    /// `self.x` etc. read through to the kinematics.
    #[inline]
    pub fn x(&self) -> f64 {
        self.kin.x
    }
    #[inline]
    pub fn y(&self) -> f64 {
        self.kin.y
    }
    #[inline]
    pub fn heading(&self) -> f64 {
        self.kin.heading
    }
    #[inline]
    pub fn size(&self) -> f64 {
        self.kin.size
    }

    pub fn gait_style(&self) -> GaitStyle {
        self.kin.gait_style
    }

    /// The snapshot other creatures see of this one.
    pub fn view(&self) -> crate::world::CreatureView {
        crate::world::CreatureView {
            id: self.id,
            index: self.index,
            progression_id: Arc::from(self.progression_id.as_str()),
            team_id: Arc::from(self.progression.team_id.as_str()),
            x: self.kin.x,
            y: self.kin.y,
            size: self.kin.size,
            state: self.state,
            dead: self.dead,
            dragging: self.dragging,
            airborne: self.airborne,
            webbed: self.webbed(),
            desktop_fully_hidden: self.desktop_fully_hidden,
        }
    }
}

/// A colour triple from model or override JSON: `int(clamp(float(c), 0, 255))`.
fn rgb_of(value: &Value, exactly_three: bool) -> Option<[u8; 3]> {
    let items = value.as_array()?;
    if (exactly_three && items.len() != 3) || items.len() < 3 {
        return None;
    }
    let mut out = [0u8; 3];
    for i in 0..3 {
        let f = py_float(&items[i])?;
        out[i] = clamp(f, 0.0, 255.0) as u8;
    }
    Some(out)
}

/// Run kinematics code that needs a [`FrameCtx`] built from the creature:
/// `kin_ctx!(self, |kin, ctx, rng| kin.update_roll(dt, &ctx))`. The context
/// is a copy (it borrows only the split playfield), so `kin` and `rng` can
/// be borrowed mutably alongside it.
#[macro_export]
macro_rules! kin_ctx {
    ($c:expr, |$kin:ident, $ctx:ident, $rng:ident| $body:expr) => {{
        let __pf = $c.split_playfield();
        let $ctx = $c.kctx(__pf.as_ref().map(|p| p as &dyn ::dc_kinematics::PlayfieldClamp));
        let $kin = &mut $c.kin;
        #[allow(unused_variables)]
        let $rng = &mut $c.rng;
        $body
    }};
}
