//! Port of `content/personality_profiles.py`: the compact temperament catalog.
//!
//! Personality means stable temperament and phase preference. Jobs live in
//! `world/jobs`, capabilities in [`crate::skills`]. Legacy JSON personalities
//! stay readable and are projected into the same temperament space.
//!
//! Python's functions accept either a personality dict or a bare id string;
//! here that is [`PersonalityRef`].

use std::collections::BTreeSet;

use serde_json::Value;

use crate::personality::{Personality, Temperament, clamp_trait, flag_enabled};
use crate::util::{JsonMap, OrderedMap, py_float, py_round_n};

/// A personality definition or a bare personality id.
#[derive(Clone, Copy, Debug)]
pub enum PersonalityRef<'a> {
    Def(&'a Personality),
    Id(&'a str),
}

impl<'a> From<&'a Personality> for PersonalityRef<'a> {
    fn from(p: &'a Personality) -> Self {
        PersonalityRef::Def(p)
    }
}
impl<'a> From<&'a str> for PersonalityRef<'a> {
    fn from(s: &'a str) -> Self {
        PersonalityRef::Id(s)
    }
}
impl<'a> From<&'a String> for PersonalityRef<'a> {
    fn from(s: &'a String) -> Self {
        PersonalityRef::Id(s.as_str())
    }
}

impl<'a> PersonalityRef<'a> {
    /// `str(personality.get("id", "")).strip().lower()` / `str(pid).strip().lower()`.
    pub fn pid(&self) -> String {
        match self {
            PersonalityRef::Def(p) => p.id.trim().to_lowercase(),
            PersonalityRef::Id(s) => s.trim().to_lowercase(),
        }
    }
    pub fn def(&self) -> Option<&'a Personality> {
        match self {
            PersonalityRef::Def(p) => Some(p),
            PersonalityRef::Id(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Temperament
// ---------------------------------------------------------------------------

/// `TEMPERAMENT_TRAITS`: (id, label, description).
pub const TEMPERAMENT_TRAITS: [(&str, &str, &str); 6] = [
    ("energy", "Energy", "How often the spider chooses active movement."),
    ("curiosity", "Curiosity", "How strongly it investigates new targets."),
    ("boldness", "Boldness", "How readily it approaches risk or a target."),
    ("sociability", "Sociability", "How often it seeks contact with friends."),
    ("patience", "Patience", "How long it watches or holds a calm phase."),
    ("caution", "Caution", "How readily it retreats from sudden threats."),
];

pub const TEMPERAMENT_TRAIT_IDS: [&str; 6] = ["energy", "curiosity", "boldness", "sociability", "patience", "caution"];

/// One entry of `COMPACT_TEMPERAMENTS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompactTemperament {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub traits: Temperament,
}

pub const COMPACT_TEMPERAMENTS: [CompactTemperament; 6] = [
    CompactTemperament {
        id: "balanced",
        display_name: "Balanced",
        description: "A steady mix of roaming, curiosity, and caution.",
        traits: Temperament::new(5.0, 5.0, 5.0, 5.0, 5.0, 5.0),
    },
    CompactTemperament {
        id: "playful",
        display_name: "Playful",
        description: "Energetic, social, and prone to cheerful movement phases.",
        traits: Temperament::new(8.0, 6.0, 6.0, 8.0, 3.0, 3.0),
    },
    CompactTemperament {
        id: "curious",
        display_name: "Curious",
        description: "Investigates objects and pauses to study them.",
        traits: Temperament::new(6.0, 10.0, 5.0, 5.0, 7.0, 4.0),
    },
    CompactTemperament {
        id: "bold",
        display_name: "Bold",
        description: "Confident and quick to approach or pursue.",
        traits: Temperament::new(8.0, 6.0, 10.0, 4.0, 2.0, 1.0),
    },
    CompactTemperament {
        id: "cautious",
        display_name: "Cautious",
        description: "Patient and observant, with a strong threat response.",
        traits: Temperament::new(4.0, 5.0, 2.0, 3.0, 8.0, 10.0),
    },
    CompactTemperament {
        id: "social",
        display_name: "Social",
        description: "Friendly and connection-seeking without being frantic.",
        traits: Temperament::new(5.0, 6.0, 4.0, 10.0, 7.0, 4.0),
    },
];

pub const COMPACT_TEMPERAMENT_IDS: [&str; 6] = ["balanced", "playful", "curious", "bold", "cautious", "social"];

pub fn compact_temperament(id: &str) -> Option<&'static CompactTemperament> {
    COMPACT_TEMPERAMENTS.iter().find(|t| t.id == id)
}

/// The compact projection of old specialist ids used by `temperament_for`.
const LEGACY_TEMPERAMENT: [(&str, &str); 21] = [
    ("bashful", "cautious"),
    ("shy", "cautious"),
    ("skittish", "cautious"),
    ("nope", "cautious"),
    ("mellow", "balanced"),
    ("sleepy", "balanced"),
    ("camouflage", "cautious"),
    ("grumpy", "bold"),
    ("clingy", "social"),
    ("cuddly", "social"),
    ("explorer", "curious"),
    ("observer", "curious"),
    ("jumper", "playful"),
    ("playful", "playful"),
    ("zoomy", "playful"),
    ("drifter", "playful"),
    ("hunter", "bold"),
    ("trapper", "bold"),
    ("webber", "curious"),
    ("weaver", "curious"),
    ("webslinger", "bold"),
];

/// `temperament_for`: the six 0..10 temperament values for any personality.
pub fn temperament_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> Temperament {
    let p = personality.into();
    let pid = p.pid();
    if let Some(def) = p.def() {
        if let Some(t) = def.temperament {
            return t;
        }
        if let Some(raw) = def.legacy_traits() {
            return Temperament::from_raw(raw);
        }
    }
    if let Some(t) = compact_temperament(&pid) {
        return t.traits;
    }
    let profile = LEGACY_TEMPERAMENT.iter().find(|(k, _)| *k == pid).map(|(_, v)| *v).unwrap_or("balanced");
    compact_temperament(profile).expect("legacy map targets a compact temperament").traits
}

/// `canonical_personality_definitions`: the six menu temperaments as full
/// personalities, in catalog order.
pub fn canonical_personality_definitions() -> OrderedMap<Personality> {
    let mut out = OrderedMap::new();
    for def in COMPACT_TEMPERAMENTS.iter() {
        let t = def.traits;
        let energy = t.energy;
        let caution = t.caution;
        let pid = def.id;
        let mood = if pid == "playful" || pid == "curious" {
            pid
        } else if pid == "balanced" || pid == "cautious" {
            "calm"
        } else {
            "cuddly"
        };
        let movement = if t.curiosity >= 8.0 {
            "curious"
        } else if t.boldness >= 8.0 {
            "bold"
        } else {
            "gentle"
        };
        let mut tunables = JsonMap::new();
        tunables.insert(
            "idle_time".into(),
            serde_json::json!([f64::max(0.35, 2.8 - energy * 0.18), f64::max(1.0, 5.2 - energy * 0.25)]),
        );
        tunables.insert("move_time".into(), serde_json::json!([0.65, 1.2 + energy * 0.15]));
        let personality = Personality {
            id: pid.to_string(),
            display_name: def.display_name.to_string(),
            description: Some(def.description.to_string()),
            mood: Some(mood.to_string()),
            speed_multiplier: 0.82 + energy * 0.035,
            reaction_radius: 260.0 + t.curiosity * 34.0,
            boldness: t.boldness / 10.0,
            wander_frequency: 0.18 + energy * 0.045,
            threat_radius: Some(95.0 + caution * 10.0),
            threat_cursor_speed: Some(950.0 + caution * 75.0),
            temperament: Some(t),
            movement_profile: Some(movement.to_string()),
            phase_scores: None,
            phase_duration_multiplier: Some(0.78 + t.patience * 0.05),
            skills: None,
            behaviours: None,
            abilities: None,
            ability_bundles: None,
            include_common_abilities: Some(true),
            behaviour_modules: None,
            ability_ids: None,
            canonical: true,
            path: None,
            tunables,
        };
        out.insert(pid, personality);
    }
    out
}

/// `selectable_personality_ids`: the compact menu, plus `include_id` when it
/// names a personality that exists (or when no catalog is given).
pub fn selectable_personality_ids(
    personalities: Option<&OrderedMap<Personality>>,
    include_id: Option<&str>,
) -> Vec<String> {
    let mut values: Vec<String> = COMPACT_TEMPERAMENT_IDS.iter().map(|s| s.to_string()).collect();
    let legacy_id = include_id.unwrap_or("").trim().to_lowercase();
    if !legacy_id.is_empty() && !values.contains(&legacy_id) {
        let known = match personalities {
            None => true,
            Some(p) if p.is_empty() => true,
            Some(p) => p.contains_key(&legacy_id),
        };
        if known {
            values.push(legacy_id);
        }
    }
    values
}

// ---------------------------------------------------------------------------
// Behaviour modules
// ---------------------------------------------------------------------------

pub const BEHAVIOUR_MODULE_IDS: [&str; 7] =
    ["hunter", "jumper", "observer", "nope", "drifter", "webber", "web_shooter"];

const LEGACY_MODULE_FLAGS: [(&str, &str); 9] = [
    ("mouse_hunter", "hunter"),
    ("constant_small_hops", "jumper"),
    ("horizontal_orbit_observer", "observer"),
    ("nope_escape", "nope"),
    ("drifter", "drifter"),
    ("drift_movement", "drifter"),
    ("web_weaver", "webber"),
    ("webber", "webber"),
    ("web_shooter", "web_shooter"),
];

const LEGACY_MODULE_IDS: [(&str, &str); 9] = [
    ("hunter", "hunter"),
    ("jumper", "jumper"),
    ("observer", "observer"),
    ("nope", "nope"),
    ("drifter", "drifter"),
    ("webber", "webber"),
    ("weaver", "webber"),
    ("trapper", "web_shooter"),
    ("webslinger", "web_shooter"),
];

fn lookup<'t>(table: &'t [(&'t str, &'t str)], key: &str) -> Option<&'t str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn module_id(id: &str) -> Option<&'static str> {
    BEHAVIOUR_MODULE_IDS.iter().copied().find(|m| *m == id)
}

/// `behaviour_modules_for`: the behaviour-module ids a personality selects.
pub fn behaviour_modules_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> BTreeSet<&'static str> {
    let p = personality.into();
    let Some(def) = p.def() else {
        return lookup(&LEGACY_MODULE_IDS, &p.pid()).and_then(module_id).into_iter().collect();
    };
    if let Some(explicit) = &def.behaviour_modules {
        return explicit.iter().filter_map(|item| module_id(&item.trim().to_lowercase())).collect();
    }
    let mut modules = BTreeSet::new();
    if let Some(m) = lookup(&LEGACY_MODULE_IDS, &p.pid()).and_then(module_id) {
        modules.insert(m);
    }
    for (flag, module) in LEGACY_MODULE_FLAGS {
        if def.flag(flag) {
            modules.insert(module_id(module).expect("flag targets a module"));
        }
    }
    modules
}

// ---------------------------------------------------------------------------
// Movement profiles and phases
// ---------------------------------------------------------------------------

/// `MOVEMENT_PROFILES`: (id, description).
pub const MOVEMENT_PROFILES: [(&str, &str); 11] = [
    ("gentle", "slow, cautious, low-pressure roaming"),
    ("social", "friendly approach and close-range interaction"),
    ("curious", "exploration, inspection, and measured pursuit"),
    ("bold", "confident pursuit with stronger acceleration"),
    ("hunter", "persistent cursor/prey pursuit"),
    ("observer", "orbiting and deliberate watching"),
    ("jumper", "playful hop and pounce movement"),
    ("escape", "rapid threat avoidance and evasive movement"),
    ("drift", "momentum-based sliding and wide turns"),
    ("camouflage", "quiet movement with desktop hiding"),
    ("webber", "calm movement around web construction"),
];

pub fn is_movement_profile(id: &str) -> bool {
    MOVEMENT_PROFILES.iter().any(|(k, _)| *k == id)
}

fn movement_id(id: &str) -> Option<&'static str> {
    MOVEMENT_PROFILES.iter().map(|(k, _)| *k).find(|k| *k == id)
}

pub const BEHAVIOUR_PHASE_IDS: [&str; 12] = [
    "wander",
    "jump",
    "roll",
    "zoomies",
    "approach",
    "chase",
    "observe",
    "prepare_jump_attack",
    "inspect",
    "cuddle",
    "social_play",
    "run_away",
];

pub fn is_behaviour_phase(id: &str) -> bool {
    BEHAVIOUR_PHASE_IDS.contains(&id)
}

/// `DEFAULT_PHASE_SCORES`: 3.0 for every phase.
pub fn default_phase_scores() -> OrderedMap<f64> {
    BEHAVIOUR_PHASE_IDS.iter().map(|p| (p.to_string(), 3.0)).collect()
}

/// `PHASE_SCORE_PROFILES[profile]` as `(phase, score)` pairs, in the Python
/// dict's order (which `dict.update` ignores, since every key already exists).
pub fn phase_score_profile(profile: &str) -> Option<&'static [(&'static str, f64)]> {
    Some(match profile {
        "gentle" => &[
            ("wander", 8.0),
            ("approach", 4.0),
            ("observe", 5.0),
            ("inspect", 5.0),
            ("cuddle", 4.0),
            ("social_play", 2.0),
            ("chase", 1.0),
            ("jump", 1.0),
            ("roll", 1.0),
            ("zoomies", 1.0),
            ("prepare_jump_attack", 1.0),
            ("run_away", 1.0),
        ],
        "social" => &[
            ("wander", 5.0),
            ("approach", 5.0),
            ("observe", 3.0),
            ("inspect", 6.0),
            ("cuddle", 9.0),
            ("social_play", 10.0),
            ("chase", 3.0),
            ("jump", 4.0),
            ("roll", 4.0),
            ("zoomies", 3.0),
            ("prepare_jump_attack", 2.0),
            ("run_away", 1.0),
        ],
        "curious" => &[
            ("wander", 5.0),
            ("approach", 7.0),
            ("observe", 7.0),
            ("inspect", 10.0),
            ("cuddle", 3.0),
            ("social_play", 4.0),
            ("chase", 3.0),
            ("jump", 2.0),
            ("roll", 2.0),
            ("zoomies", 2.0),
            ("prepare_jump_attack", 3.0),
            ("run_away", 1.0),
        ],
        "bold" => &[
            ("wander", 4.0),
            ("approach", 6.0),
            ("observe", 2.0),
            ("inspect", 3.0),
            ("cuddle", 2.0),
            ("social_play", 5.0),
            ("chase", 9.0),
            ("jump", 5.0),
            ("roll", 3.0),
            ("zoomies", 8.0),
            ("prepare_jump_attack", 8.0),
            ("run_away", 1.0),
        ],
        "hunter" => &[
            ("wander", 2.0),
            ("approach", 9.0),
            ("observe", 6.0),
            ("inspect", 3.0),
            ("cuddle", 0.0),
            ("social_play", 0.0),
            ("chase", 10.0),
            ("jump", 0.0),
            ("roll", 0.0),
            ("zoomies", 0.0),
            ("prepare_jump_attack", 8.0),
            ("run_away", 1.0),
        ],
        "observer" => &[
            ("wander", 4.0),
            ("approach", 4.0),
            ("observe", 10.0),
            ("inspect", 8.0),
            ("cuddle", 2.0),
            ("social_play", 3.0),
            ("chase", 1.0),
            ("jump", 1.0),
            ("roll", 1.0),
            ("zoomies", 1.0),
            ("prepare_jump_attack", 1.0),
            ("run_away", 1.0),
        ],
        "jumper" => &[
            ("wander", 4.0),
            ("approach", 4.0),
            ("observe", 2.0),
            ("inspect", 2.0),
            ("cuddle", 2.0),
            ("social_play", 5.0),
            ("chase", 5.0),
            ("jump", 10.0),
            ("roll", 6.0),
            ("zoomies", 8.0),
            ("prepare_jump_attack", 9.0),
            ("run_away", 2.0),
        ],
        "escape" => &[
            ("wander", 1.0),
            ("approach", 0.0),
            ("observe", 0.0),
            ("inspect", 0.0),
            ("cuddle", 0.0),
            ("social_play", 0.0),
            ("chase", 0.0),
            ("jump", 8.0),
            ("roll", 0.0),
            ("zoomies", 0.0),
            ("prepare_jump_attack", 7.0),
            ("run_away", 10.0),
        ],
        "drift" => &[
            ("wander", 5.0),
            ("approach", 3.0),
            ("observe", 2.0),
            ("inspect", 2.0),
            ("cuddle", 1.0),
            ("social_play", 2.0),
            ("chase", 4.0),
            ("jump", 3.0),
            ("roll", 6.0),
            ("zoomies", 9.0),
            ("prepare_jump_attack", 2.0),
            ("run_away", 2.0),
        ],
        "camouflage" => &[
            ("wander", 7.0),
            ("approach", 3.0),
            ("observe", 6.0),
            ("inspect", 5.0),
            ("cuddle", 1.0),
            ("social_play", 1.0),
            ("chase", 1.0),
            ("jump", 0.0),
            ("roll", 0.0),
            ("zoomies", 0.0),
            ("prepare_jump_attack", 0.0),
            ("run_away", 4.0),
        ],
        "webber" => &[
            ("wander", 6.0),
            ("approach", 4.0),
            ("observe", 5.0),
            ("inspect", 7.0),
            ("cuddle", 2.0),
            ("social_play", 2.0),
            ("chase", 1.0),
            ("jump", 1.0),
            ("roll", 1.0),
            ("zoomies", 1.0),
            ("prepare_jump_attack", 1.0),
            ("run_away", 1.0),
        ],
        _ => return None,
    })
}

/// `PHASE_DURATION_MULTIPLIERS`.
pub const PHASE_DURATION_MULTIPLIERS: [(&str, f64); 11] = [
    ("gentle", 1.15),
    ("social", 1.00),
    ("curious", 1.10),
    ("bold", 0.82),
    ("hunter", 0.72),
    ("observer", 1.30),
    ("jumper", 0.62),
    ("escape", 0.48),
    ("drift", 0.70),
    ("camouflage", 1.25),
    ("webber", 1.18),
];

/// `ABILITY_BUNDLES`: bundle id -> skill ids.
pub const ABILITY_BUNDLES: [(&str, &[&str]); 4] = [
    ("web_walker", &["web_walk"]),
    ("web_weaver", &["weave_web"]),
    ("web_trapper", &["shoot_web", "wall_web"]),
    ("drifter", &["drift"]),
];

pub fn ability_bundle(id: &str) -> Option<&'static [&'static str]> {
    ABILITY_BUNDLES.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// `PERSONALITY_COMPONENTS`: id -> (movement profile, ability bundles).
pub const PERSONALITY_COMPONENTS: [(&str, &str, &[&str]); 23] = [
    ("bashful", "gentle", &[]),
    ("bold", "bold", &[]),
    ("camouflage", "camouflage", &[]),
    ("clingy", "social", &[]),
    ("cuddly", "social", &[]),
    ("curious", "curious", &[]),
    ("drifter", "drift", &["drifter"]),
    ("explorer", "curious", &[]),
    ("grumpy", "gentle", &[]),
    ("hunter", "hunter", &[]),
    ("jumper", "jumper", &[]),
    ("mellow", "gentle", &[]),
    ("nope", "escape", &[]),
    ("observer", "observer", &[]),
    ("playful", "jumper", &[]),
    ("shy", "gentle", &[]),
    ("skittish", "escape", &[]),
    ("sleepy", "gentle", &[]),
    ("trapper", "hunter", &["web_trapper"]),
    ("webber", "webber", &["web_weaver"]),
    ("weaver", "webber", &["web_weaver"]),
    ("webslinger", "hunter", &["web_trapper"]),
    ("zoomy", "bold", &[]),
];

fn component(pid: &str) -> Option<(&'static str, &'static [&'static str])> {
    PERSONALITY_COMPONENTS.iter().find(|(k, _, _)| *k == pid).map(|(_, m, a)| (*m, *a))
}

/// `ABILITY_FLAGS`: legacy flag -> bundle ids.
pub const ABILITY_FLAGS: [(&str, &[&str]); 5] = [
    ("web_weaver", &["web_weaver"]),
    ("webber", &["web_weaver"]),
    ("web_shooter", &["web_trapper"]),
    ("drifter", &["drifter"]),
    ("drift_movement", &["drifter"]),
];

/// `GAIT_BY_MOVEMENT`.
pub const GAIT_BY_MOVEMENT: [(&str, &str); 11] = [
    ("bold", "skitter"),
    ("escape", "skitter"),
    ("hunter", "skitter"),
    ("jumper", "skitter"),
    ("camouflage", "lively"),
    ("curious", "lively"),
    ("drift", "lively"),
    ("gentle", "lively"),
    ("observer", "lively"),
    ("social", "lively"),
    ("webber", "lively"),
];

/// `movement_profile_for`: the compact movement profile id.
pub fn movement_profile_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> &'static str {
    let p = personality.into();
    if let Some(def) = p.def() {
        let explicit = def.movement_profile.as_deref().unwrap_or("").trim().to_lowercase();
        if let Some(id) = movement_id(&explicit) {
            return id;
        }
    }
    component(&p.pid()).map(|(m, _)| m).unwrap_or("curious")
}

/// `personality_gait_style`: the walking animation this temperament uses.
pub fn personality_gait_style<'a>(personality: impl Into<PersonalityRef<'a>>) -> &'static str {
    let profile = movement_profile_for(personality);
    GAIT_BY_MOVEMENT.iter().find(|(k, _)| *k == profile).map(|(_, v)| *v).unwrap_or("lively")
}

/// `phase_scores_for`: clamped 0..10 occurrence scores, in `BEHAVIOUR_PHASE_IDS` order.
pub fn phase_scores_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> OrderedMap<f64> {
    let p = personality.into();
    let t = temperament_for(p);
    let pid = p.pid();
    let canonical = compact_temperament(&pid).is_some() || p.def().is_some_and(|d| d.canonical);
    let overrides = p.def().and_then(|d| d.phase_scores.as_ref());
    if canonical {
        let raw = [
            ("wander", 3.0 + t.energy * 0.35 + t.patience * 0.20),
            ("jump", t.energy * 0.52 + t.boldness * 0.18 - t.caution * 0.18),
            ("roll", t.energy * 0.28 + t.sociability * 0.18),
            ("zoomies", t.energy * 0.62 + t.boldness * 0.20 - t.patience * 0.22),
            ("approach", 2.0 + t.boldness * 0.48 + t.curiosity * 0.18 - t.caution * 0.22),
            ("chase", t.boldness * 0.58 + t.energy * 0.28 - t.caution * 0.24),
            ("observe", 2.0 + t.patience * 0.48 + t.curiosity * 0.28),
            ("prepare_jump_attack", t.boldness * 0.46 + t.energy * 0.22 - t.caution * 0.18),
            ("inspect", 2.0 + t.curiosity * 0.62 + t.patience * 0.16),
            ("cuddle", t.sociability * 0.62 + t.patience * 0.18 - t.caution * 0.12),
            ("social_play", t.sociability * 0.66 + t.energy * 0.22),
            ("run_away", t.caution * 0.78 - t.boldness * 0.20),
        ];
        let mut scores: OrderedMap<f64> =
            raw.iter().map(|(k, v)| (k.to_string(), py_round_n(*v, 2).clamp(0.0, 10.0))).collect();
        if let Some(overrides) = overrides {
            for (phase_id, value) in overrides.iter() {
                let phase_id = phase_id.trim().to_lowercase();
                if let Some(slot) = scores.get_mut(&phase_id) {
                    *slot = clamp_trait(Some(value));
                }
            }
        }
        return scores;
    }
    let profile = movement_profile_for(p);
    let mut scores = default_phase_scores();
    if let Some(table) = phase_score_profile(profile) {
        for (k, v) in table {
            scores.insert(*k, *v);
        }
    }
    if let Some(overrides) = overrides {
        for (phase_id, value) in overrides.iter() {
            let phase_id = phase_id.trim().to_lowercase();
            if let Some(slot) = scores.get_mut(&phase_id)
                && let Some(f) = py_float(value)
            {
                *slot = if f.is_nan() { 10.0 } else { f.clamp(0.0, 10.0) };
            }
        }
    }
    scores
}

/// `phase_duration_multiplier_for`: overall phase pacing.
pub fn phase_duration_multiplier_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> f64 {
    let p = personality.into();
    let profile = movement_profile_for(p);
    let default = PHASE_DURATION_MULTIPLIERS.iter().find(|(k, _)| *k == profile).map(|(_, v)| *v).unwrap_or(1.0);
    match p.def().and_then(|d| d.phase_duration_multiplier) {
        Some(v) if !v.is_nan() => v.clamp(0.25, 2.5),
        Some(_) => 2.5,
        None => default,
    }
}

/// `ability_bundle_ids_for`: bundle ids selected by a personality.
pub fn ability_bundle_ids_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> Vec<String> {
    let p = personality.into();
    let Some(def) = p.def() else {
        return component(&p.pid()).map(|(_, a)| a.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    };
    let mut bundles: Vec<String> = Vec::new();
    if let Some(explicit) = &def.ability_bundles {
        bundles.extend(explicit.iter().map(|s| s.trim().to_lowercase()));
    }
    if let Some((_, abilities)) = component(&p.pid()) {
        bundles.extend(abilities.iter().map(|s| s.to_string()));
    }
    for (flag, aliases) in ABILITY_FLAGS {
        if flag_enabled(def.get(flag)) {
            bundles.extend(aliases.iter().map(|s| s.to_string()));
        }
    }
    dedupe(bundles.into_iter().filter(|b| ability_bundle(b).is_some()))
}

/// `ability_ids_for`: explicit ability ids plus bundle expansions, deduplicated.
pub fn ability_ids_for<'a>(personality: impl Into<PersonalityRef<'a>>) -> Vec<String> {
    let p = personality.into();
    let mut values: Vec<String> = Vec::new();
    if let Some(def) = p.def()
        && let Some(explicit) = &def.abilities
    {
        values.extend(explicit.iter().map(|s| s.trim().to_lowercase()));
    }
    for bundle in ability_bundle_ids_for(p) {
        if let Some(ids) = ability_bundle(&bundle) {
            values.extend(ids.iter().map(|s| s.to_string()));
        }
    }
    dedupe(values.into_iter().filter(|v| !v.is_empty()))
}

/// `list(dict.fromkeys(...))`.
pub(crate) fn dedupe(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// `annotate_personality`: fill in composition metadata without overwriting
/// anything the definition states.
pub fn annotate_personality(personality: &Personality) -> Personality {
    let mut data = personality.clone();
    if data.temperament.is_none() {
        data.temperament = Some(temperament_for(&data));
    }
    if data.movement_profile.is_none() {
        data.movement_profile = Some(movement_profile_for(&data).to_string());
    }
    if data.phase_scores.is_none() {
        let scores = phase_scores_for(&data);
        data.phase_scores = Some(scores.iter().map(|(k, v)| (k.to_string(), Value::from(*v))).collect());
    }
    if data.phase_duration_multiplier.is_none() {
        data.phase_duration_multiplier = Some(phase_duration_multiplier_for(&data));
    }
    if data.ability_ids.is_none() {
        data.ability_ids = Some(ability_ids_for(&data));
    }
    data
}
