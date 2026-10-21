//! Typed `presets/*.json`: `name`, `slots[]`, `settings`.
//!
//! Validation (`validate_preset`) runs on the raw JSON in
//! [`crate::preset_io`], exactly like the Python, so a hand-written preset
//! gets the same messages. These structs are what the app and the settings
//! window then work with. Unknown keys round-trip at every level.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::util::{ColorMap, JsonMap, OrderedMap, de};

/// The label switches and their defaults (`LABEL_SWITCH_DEFAULTS`).
pub const LABEL_SWITCH_DEFAULTS: [(&str, bool); 5] = [
    ("always_show_names", true),
    ("always_show_levels", true),
    ("always_show_health", true),
    ("always_show_xp", false),
    ("always_show_stamina", true),
];

/// One row of a preset: which model, which temperament, how many.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetSlot {
    pub model: String,
    pub personality: String,
    #[serde(deserialize_with = "de::i64_lenient")]
    pub count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_random: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// Legacy full skill list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// Chosen capabilities; absent means "the personality decides".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abilities: Option<Vec<String>>,
    /// Per-slot palette overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<ColorMap>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl PresetSlot {
    pub fn new(model: impl Into<String>, personality: impl Into<String>, count: i64) -> Self {
        Self {
            model: model.into(),
            personality: personality.into(),
            count,
            count_random: None,
            slot_id: None,
            team: None,
            team_id: None,
            job: None,
            skills: None,
            abilities: None,
            colors: None,
            extra: JsonMap::new(),
        }
    }

    /// `slot.get("team_id", slot.get("team", "neutral"))` as the manager reads it.
    pub fn team(&self) -> &str {
        self.team_id.as_deref().or(self.team.as_deref()).unwrap_or("neutral")
    }

    /// `slot.get("job", "none")`.
    pub fn job(&self) -> &str {
        self.job.as_deref().unwrap_or("none")
    }

    /// `slot.get("count_random", False)`.
    pub fn count_random(&self) -> bool {
        self.count_random.unwrap_or(false)
    }
}

/// `settings.flies`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FliesSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub min_interval: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub max_interval: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_i64")]
    pub max_flies: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawner: Option<bool>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

/// One entry of `settings.teams`: an object, or a bare string naming the team.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TeamEntry {
    Name(String),
    Profile(TeamEntryProfile),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamEntryProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `#rrggbb`, `rrggbb`, `#rgb` or `[r, g, b]` (see `dc_state::teams::parse_color`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Value>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl TeamEntry {
    /// The entry as the raw JSON `dc_state::teams::normalize_teams` reads.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a team entry always serialises")
    }
}

/// `preset["settings"]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetSettings {
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub size_scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interferable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_play: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gait_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_icons_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_mouse_capture: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_show_names: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_show_levels: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_show_health: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_show_xp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_show_stamina: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flies: Option<FliesSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<OrderedMap<TeamEntry>>,
    /// `{team: {team: "friend" | "neutral" | "foe"}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_relations: Option<OrderedMap<OrderedMap<String>>>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl PresetSettings {
    /// A label switch's value, or its `LABEL_SWITCH_DEFAULTS` default.
    pub fn label_switch(&self, key: &str) -> bool {
        let stored = match key {
            "always_show_names" => self.always_show_names,
            "always_show_levels" => self.always_show_levels,
            "always_show_health" => self.always_show_health,
            "always_show_xp" => self.always_show_xp,
            "always_show_stamina" => self.always_show_stamina,
            _ => None,
        };
        stored
            .unwrap_or_else(|| LABEL_SWITCH_DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(false))
    }

    /// `settings.get("size_scale", 1.0)`.
    pub fn size_scale(&self) -> f64 {
        self.size_scale.unwrap_or(1.0)
    }

    /// `settings.get("interferable", True)`.
    pub fn interferable(&self) -> bool {
        self.interferable.unwrap_or(true)
    }

    /// The `teams` block as raw JSON (for `dc_state::teams::normalize_teams`).
    pub fn teams_value(&self) -> Option<Value> {
        self.teams.as_ref().map(|t| serde_json::to_value(t).expect("teams serialise"))
    }

    /// The `team_relations` block as raw JSON (for `normalize_team_stances`).
    pub fn team_relations_value(&self) -> Option<Value> {
        self.team_relations.as_ref().map(|t| serde_json::to_value(t).expect("relations serialise"))
    }
}

/// One preset file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub slots: Vec<PresetSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<PresetSettings>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl Preset {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), slots: Vec::new(), settings: Some(PresetSettings::default()), extra: JsonMap::new() }
    }

    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a preset always serialises")
    }

    /// `data.get("settings", {})`.
    pub fn settings(&self) -> PresetSettings {
        self.settings.clone().unwrap_or_default()
    }
}
