//! Typed `personalities/*.json` (and the built-in temperament catalog).
//!
//! The four keys `validate_personality` requires plus the composition keys
//! (`temperament`, `phase_scores`, ability lists, ...) are typed. The many
//! flat tunables (`drift_slip`, `idle_time` as a `[min, max]` pair, ...) stay
//! in [`Personality::tunables`] and are read with the accessors below, because
//! Python reads each with its own call-site default
//! (`personality.get("drift_countersteer", 0.42)` in one place, `0.34` in
//! another). Unknown keys round-trip.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::util::{JsonMap, OrderedMap, de, py_float, py_int, py_truthy};

/// The six 0..10 temperament values (`TEMPERAMENT_TRAITS`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Temperament {
    pub energy: f64,
    pub curiosity: f64,
    pub boldness: f64,
    pub sociability: f64,
    pub patience: f64,
    pub caution: f64,
}

impl Default for Temperament {
    fn default() -> Self {
        Self::uniform(5.0)
    }
}

impl Temperament {
    pub const fn new(
        energy: f64,
        curiosity: f64,
        boldness: f64,
        sociability: f64,
        patience: f64,
        caution: f64,
    ) -> Self {
        Self { energy, curiosity, boldness, sociability, patience, caution }
    }
    pub const fn uniform(v: f64) -> Self {
        Self::new(v, v, v, v, v, v)
    }
    /// The value for a trait id, in `TEMPERAMENT_TRAIT_IDS` spelling.
    pub fn get(&self, trait_id: &str) -> Option<f64> {
        Some(match trait_id {
            "energy" => self.energy,
            "curiosity" => self.curiosity,
            "boldness" => self.boldness,
            "sociability" => self.sociability,
            "patience" => self.patience,
            "caution" => self.caution,
            _ => return None,
        })
    }
    pub fn set(&mut self, trait_id: &str, value: f64) -> bool {
        let slot = match trait_id {
            "energy" => &mut self.energy,
            "curiosity" => &mut self.curiosity,
            "boldness" => &mut self.boldness,
            "sociability" => &mut self.sociability,
            "patience" => &mut self.patience,
            "caution" => &mut self.caution,
            _ => return false,
        };
        *slot = value;
        true
    }
    /// `(trait_id, value)` in `TEMPERAMENT_TRAIT_IDS` order.
    pub fn iter(&self) -> [(&'static str, f64); 6] {
        [
            ("energy", self.energy),
            ("curiosity", self.curiosity),
            ("boldness", self.boldness),
            ("sociability", self.sociability),
            ("patience", self.patience),
            ("caution", self.caution),
        ]
    }

    /// `{key: _clamp_trait(raw.get(key, 5.0)) for key in TEMPERAMENT_TRAIT_IDS}`.
    pub fn from_raw(raw: &JsonMap) -> Self {
        let mut t = Temperament::default();
        for (key, _) in Temperament::default().iter() {
            t.set(key, clamp_trait(raw.get(key)));
        }
        t
    }
}

/// `_clamp_trait`: `max(0, min(10, float(value)))`, 5.0 when unusable.
pub fn clamp_trait(value: Option<&Value>) -> f64 {
    match value {
        None => 5.0,
        Some(v) => match py_float(v) {
            Some(f) if !f.is_nan() => f.clamp(0.0, 10.0),
            // max(0.0, min(10.0, nan)) is 10.0 in Python.
            Some(_) => 10.0,
            None => 5.0,
        },
    }
}

impl<'de> Deserialize<'de> for Temperament {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = JsonMap::deserialize(d)?;
        Ok(Temperament::from_raw(&raw))
    }
}

/// A personality definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Personality {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub speed_multiplier: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub reaction_radius: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub boldness: f64,
    #[serde(deserialize_with = "de::f64_lenient")]
    pub wander_frequency: f64,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub threat_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub threat_cursor_speed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperament: Option<Temperament>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_profile: Option<String>,
    /// As written (overrides), or the full computed deck after annotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_scores: Option<OrderedMap<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de::opt_f64")]
    pub phase_duration_multiplier: Option<f64>,
    /// Legacy explicit skill list; `null` means "the full default set".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behaviours: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abilities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ability_bundles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_common_abilities: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behaviour_modules: Option<Vec<String>>,
    /// Added by `annotate_personality`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ability_ids: Option<Vec<String>>,
    /// Built-in temperament catalog entry (`_canonical`).
    #[serde(rename = "_canonical", default, skip_serializing_if = "std::ops::Not::not")]
    pub canonical: bool,
    /// Set by discovery: the file it came from.
    #[serde(rename = "_path", default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Every flat tunable and legacy flag (`drift_slip`, `idle_time`, `mouse_hunter`, ...).
    #[serde(flatten)]
    pub tunables: JsonMap,
}

impl Personality {
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a personality always serialises")
    }

    /// `personality.get(key)` for a key that lives in the flat tunables.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.tunables.get(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.tunables.contains_key(key)
    }

    /// `float(personality.get(key, default))`, with the default also used when
    /// the stored value is not a number.
    pub fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.tunables.get(key).and_then(py_float).unwrap_or(default)
    }

    /// `float(personality.get(key, default) or 0.0)`-style read that keeps
    /// `None` when the key is absent.
    pub fn f64_opt(&self, key: &str) -> Option<f64> {
        self.tunables.get(key).and_then(py_float)
    }

    /// `int(personality.get(key, default))`.
    pub fn i64_or(&self, key: &str, default: i64) -> i64 {
        self.tunables.get(key).and_then(py_int).unwrap_or(default)
    }

    /// `bool(personality.get(key, default))`.
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.tunables.get(key).map(py_truthy).unwrap_or(default)
    }

    /// A `[min, max]` tunable (`idle_time`, `hop_power`, ...), or `None` when
    /// absent or not a two-number list. Feed it to `dc_core::math::rand_range`.
    pub fn range(&self, key: &str) -> Option<[f64; 2]> {
        match self.tunables.get(key) {
            Some(Value::Array(items)) if items.len() >= 2 => Some([py_float(&items[0])?, py_float(&items[1])?]),
            _ => None,
        }
    }

    /// [`Personality::range`] with a default pair.
    pub fn range_or(&self, key: &str, default: [f64; 2]) -> [f64; 2] {
        self.range(key).unwrap_or(default)
    }

    /// `_flag_enabled`: strings `"1" / "true" / "yes" / "on"` count, otherwise
    /// Python truthiness.
    pub fn flag(&self, key: &str) -> bool {
        flag_enabled(self.tunables.get(key))
    }

    /// The legacy `traits` alias of `temperament`, if present as an object.
    pub fn legacy_traits(&self) -> Option<&JsonMap> {
        self.tunables.get("traits").and_then(Value::as_object)
    }

    /// `personality.get("include_common_abilities", True)`.
    pub fn include_common_abilities(&self) -> bool {
        self.include_common_abilities.unwrap_or(true)
    }

    /// `personality.get("mood", "auto")`.
    pub fn mood(&self) -> &str {
        self.mood.as_deref().unwrap_or("auto")
    }
}

/// `_flag_enabled` on a raw value.
pub fn flag_enabled(value: Option<&Value>) -> bool {
    match value {
        None => false,
        Some(Value::String(s)) => matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Some(v) => py_truthy(v),
    }
}
