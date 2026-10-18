//! Port of app/adventure_profile.py
//!
//! The Adventure hero, saved apart from the Companion colony, in
//! `adventure-hero.json` in the state folder (version 4):
//!
//! - `name`; `progression` (missing for a hero who has not played yet, who
//!   then starts at level 1); `missions` (victories, defeats, fastest win);
//! - `armoury`: the armour the player owns, shared by the hero and every
//!   companion -- which pieces, their levels, spare duplicates and amber;
//! - `companions`: each with its own name and progression;
//! - `two_player` and `player_two` (local co-op);
//! - `selected_map`, `all_screens`, `disabled_screens`, `admin`.
//!
//! Version 1 files held only `progression`, version 2 had no armoury or
//! companions; both still load. Python keeps the profile as a dict; here it
//! is [`AdventureProfile`], and unknown keys are dropped exactly as Python's
//! loader drops them.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::campaign::{DEFAULT_MAP, STARTING_COMPANIONS, companion_by_id, map_by_id};
use crate::progression::{
    Dict, MAX_ITEM_LEVEL, ProgressionState, armor_by_id, dict_get, dict_set, retain_equipped, state_at_level,
    to_ordered,
};
use crate::pyjson::{atomic_write, collapse_whitespace, dumps_indent2, py_float, py_int, py_str, read_json, truthy};

pub const PROFILE_VERSION: i64 = 4;
pub const PLAYER_TWO: &str = "player_two";
pub const HERO: &str = "hero";
pub const DEFAULT_HERO_NAME: &str = "Wayfarer";
pub const DEFAULT_PLAYER_TWO_NAME: &str = "Trailblazer";
pub const MAX_NAME_LENGTH: usize = 24;

pub fn profile_path(state_dir: &Path) -> PathBuf {
    state_dir.join("adventure-hero.json")
}

/// Whitespace collapsed, at most 24 characters, the default when empty.
pub fn clean_name(value: Option<&Value>) -> String {
    let raw = match value {
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    };
    let text: String = collapse_whitespace(&raw).chars().take(MAX_NAME_LENGTH).collect();
    if text.is_empty() { DEFAULT_HERO_NAME.into() } else { text }
}

fn clean_name_str(value: &str) -> String {
    clean_name(Some(&Value::String(value.into())))
}

/// Per mission: victories, defeats and the fastest win.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MissionRecord {
    pub victories: i64,
    pub defeats: i64,
    pub best_seconds: Option<f64>,
}

impl MissionRecord {
    pub fn to_value(&self) -> Value {
        json!({"victories": self.victories, "defeats": self.defeats, "best_seconds": self.best_seconds})
    }

    /// Python's `_clean_record`.
    pub fn from_value(record: &Value) -> Self {
        let empty = Map::new();
        let record = record.as_object().unwrap_or(&empty);
        let count = |key: &str| record.get(key).map_or(Some(0), py_int).map_or(0, |n| n.max(0));
        let best = match record.get("best_seconds") {
            None | Some(Value::Null) => None,
            Some(v) => py_float(v),
        };
        Self { victories: count("victories"), defeats: count("defeats"), best_seconds: best }
    }
}

/// The armour the player owns: one of each piece, its level, spares, amber.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Armoury {
    pub owned: Vec<String>,
    /// Item id -> level above 1.
    pub levels: Dict<i64>,
    /// Item id -> spare duplicates.
    pub spares: Dict<i64>,
    pub amber: i64,
}

impl Armoury {
    pub fn to_value(&self) -> Value {
        let map = |d: &Dict<i64>| {
            let mut m = Map::new();
            for (k, v) in d {
                m.insert(k.clone(), json!(v));
            }
            Value::Object(m)
        };
        json!({"owned": self.owned, "levels": map(&self.levels), "spares": map(&self.spares), "amber": self.amber})
    }
}

/// `fresh_armoury(owned)`: None owns what a new spider starts with.
pub fn fresh_armoury(owned: Option<Vec<String>>) -> Armoury {
    Armoury {
        owned: owned.unwrap_or_else(|| ProgressionState::default().inventory),
        levels: Vec::new(),
        spares: Vec::new(),
        amber: 0,
    }
}

/// A named spider with its own progression (a companion, or player two).
#[derive(Debug, Clone, PartialEq)]
pub struct SpiderEntry {
    pub name: String,
    pub progression: Option<ProgressionState>,
}

impl SpiderEntry {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), progression: None }
    }

    pub fn to_value(&self) -> Value {
        json!({"name": self.name, "progression": self.progression.as_ref().map(ProgressionState::to_value)})
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdventureProfile {
    pub version: i64,
    pub name: String,
    pub progression: Option<ProgressionState>,
    pub missions: Dict<MissionRecord>,
    pub armoury: Armoury,
    pub two_player: bool,
    pub player_two: SpiderEntry,
    pub companions: Dict<SpiderEntry>,
    pub selected_map: String,
    pub all_screens: bool,
    pub disabled_screens: Vec<String>,
    pub admin: bool,
}

impl Default for AdventureProfile {
    fn default() -> Self {
        fresh_profile()
    }
}

fn starting_companions() -> Dict<SpiderEntry> {
    STARTING_COMPANIONS
        .iter()
        .map(|cid| (cid.to_string(), SpiderEntry::new(companion_by_id(cid).expect("a known companion").name)))
        .collect()
}

pub fn fresh_profile() -> AdventureProfile {
    AdventureProfile {
        version: PROFILE_VERSION,
        name: DEFAULT_HERO_NAME.into(),
        progression: None,
        missions: Vec::new(),
        armoury: fresh_armoury(None),
        two_player: false,
        player_two: SpiderEntry::new(DEFAULT_PLAYER_TWO_NAME),
        companions: starting_companions(),
        selected_map: DEFAULT_MAP.into(),
        all_screens: true,
        disabled_screens: Vec::new(),
        admin: false,
    }
}

/// `_count(value, low, high)`.
fn count(value: &Value, low: i64, high: Option<i64>) -> i64 {
    let Some(number) = py_int(value).map(|n| n.max(low)) else {
        return low;
    };
    high.map_or(number, |h| number.min(h))
}

fn string_keyed(value: Option<&Value>) -> Vec<(String, Value)> {
    match value {
        Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Vec::new(),
    }
}

/// The armoury from a file; from an older file, built from the hero's
/// inventory and item levels.
fn clean_armoury(raw: Option<&Value>, progression: Option<&Value>) -> Armoury {
    let progression = progression.and_then(Value::as_object);
    let (mut armoury, levels_src, spares_src, amber_src): (Armoury, Option<Value>, Option<Value>, Option<Value>) =
        match raw.and_then(Value::as_object) {
            None => {
                let owned = progression.and_then(|p| p.get("inventory")).and_then(Value::as_array).map(|items| {
                    items.iter().map(|v| v.as_str().map(str::to_string).unwrap_or_default()).collect::<Vec<_>>()
                });
                let levels = progression.and_then(|p| p.get("item_levels")).filter(|v| v.is_object()).cloned();
                (fresh_armoury(owned), levels, None, None)
            }
            Some(raw) => {
                let owned = match raw.get("owned") {
                    Some(Value::Array(items)) => items.iter().map(py_str_or_marker).collect(),
                    Some(Value::String(s)) => s.chars().map(|c| c.to_string()).collect(),
                    Some(Value::Object(o)) => o.keys().cloned().collect(),
                    _ => Vec::new(),
                };
                (
                    fresh_armoury(Some(owned)),
                    raw.get("levels").cloned(),
                    raw.get("spares").cloned(),
                    raw.get("amber").cloned(),
                )
            }
        };
    // Unique, known pieces, first seen first.
    let mut owned: Vec<String> = Vec::new();
    for item in &armoury.owned {
        if armor_by_id(item).is_some() && !owned.contains(item) {
            owned.push(item.clone());
        }
    }
    armoury.owned = owned;
    armoury.levels = Vec::new();
    for (k, v) in string_keyed(levels_src.as_ref()) {
        let level = count(&v, 1, Some(MAX_ITEM_LEVEL));
        if armoury.owned.contains(&k) && level > 1 {
            dict_set(&mut armoury.levels, &k, level);
        }
    }
    armoury.spares = Vec::new();
    for (k, v) in string_keyed(spares_src.as_ref()) {
        let n = count(&v, 0, None);
        if armor_by_id(&k).is_some() && n > 0 {
            dict_set(&mut armoury.spares, &k, n);
        }
    }
    armoury.amber = amber_src.as_ref().map_or(0, |v| count(v, 0, None));
    armoury
}

/// A non-string item can never be an armour id; keep a marker that fails
/// the lookup.
fn py_str_or_marker(v: &Value) -> String {
    v.as_str().map(str::to_string).unwrap_or_default()
}

fn clean_companions(raw: Option<&Value>) -> Dict<SpiderEntry> {
    let mut companions = starting_companions();
    for (cid, entry) in string_keyed(raw) {
        let (Some(info), Some(entry)) = (companion_by_id(&cid), entry.as_object()) else { continue };
        let name = match entry.get("name") {
            Some(v) if truthy(v) => clean_name(Some(v)),
            _ => clean_name_str(info.name),
        };
        let progression = entry.get("progression").filter(|p| p.is_object()).map(ProgressionState::from_value);
        dict_set(&mut companions, &cid, SpiderEntry { name, progression });
    }
    companions
}

/// The profile at `path`; a fresh one when the file is missing or broken.
pub fn load_profile(path: &Path) -> AdventureProfile {
    let mut profile = fresh_profile();
    let Some(raw) = read_json(path) else {
        return profile;
    };
    let Some(raw) = raw.as_object() else {
        return profile;
    };
    profile.name = clean_name(raw.get("name"));
    profile.progression = raw.get("progression").filter(|p| p.is_object()).map(ProgressionState::from_value);
    profile.armoury = clean_armoury(raw.get("armoury"), raw.get("progression"));
    profile.companions = clean_companions(raw.get("companions"));
    let selected = match raw.get("selected_map") {
        Some(v) if truthy(v) => py_str(v),
        _ => DEFAULT_MAP.into(),
    };
    profile.selected_map =
        if map_by_id(&selected).is_some() || selected.starts_with("custom-") { selected } else { DEFAULT_MAP.into() };
    profile.all_screens = raw.get("all_screens").is_none_or(truthy);
    profile.disabled_screens = match raw.get("disabled_screens") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) => s.chars().map(|c| c.to_string()).collect(),
        Some(Value::Object(o)) => o.keys().cloned().collect(),
        _ => Vec::new(),
    };
    profile.admin = raw.get("admin").is_some_and(truthy);
    profile.two_player = raw.get("two_player").is_some_and(truthy);
    if let Some(second) = raw.get(PLAYER_TWO).and_then(Value::as_object) {
        profile.player_two.name = match second.get("name") {
            Some(v) if truthy(v) => clean_name(Some(v)),
            _ => clean_name_str(DEFAULT_PLAYER_TWO_NAME),
        };
        if let Some(p) = second.get("progression").filter(|p| p.is_object()) {
            profile.player_two.progression = Some(ProgressionState::from_value(p));
        }
    }
    for (mission_id, record) in string_keyed(raw.get("missions")) {
        if record.is_object() {
            dict_set(&mut profile.missions, &mission_id, MissionRecord::from_value(&record));
        }
    }
    profile
}

impl AdventureProfile {
    /// What `save_profile` writes, keys in Python's order.
    pub fn to_value(&self) -> Value {
        let mut missions = Map::new();
        for (k, r) in &self.missions {
            missions.insert(k.clone(), r.to_value());
        }
        let mut companions = Map::new();
        for (k, c) in &self.companions {
            companions.insert(k.clone(), c.to_value());
        }
        let selected = if self.selected_map.is_empty() { DEFAULT_MAP.to_string() } else { self.selected_map.clone() };
        json!({
            "version": PROFILE_VERSION,
            "two_player": self.two_player,
            "player_two": self.player_two.to_value(),
            "name": clean_name_str(&self.name),
            "progression": self.progression.as_ref().map(ProgressionState::to_value),
            "missions": missions,
            "armoury": self.armoury.to_value(),
            "companions": companions,
            "selected_map": selected,
            "all_screens": self.all_screens,
            "disabled_screens": self.disabled_screens,
            "admin": self.admin,
        })
    }
}

/// Save atomically (`.tmp` then rename); false on an OS error.
pub fn save_profile(profile: &AdventureProfile, path: &Path) -> bool {
    atomic_write(path, &dumps_indent2(&profile.to_value())).is_ok()
}

/// Give a spider's progression the shared armoury: what is owned and at what
/// level. Pieces it wore that are no longer owned come off.
fn dress(mut state: ProgressionState, profile: &AdventureProfile) -> ProgressionState {
    let armoury = &profile.armoury;
    state.inventory = armoury.owned.clone();
    state.item_levels = to_ordered(&armoury.levels);
    retain_equipped(&mut state, |item| armoury.owned.iter().any(|o| o == item));
    state
}

/// The saved hero, or a new one at level 1, dressed from the armoury.
pub fn hero_progression(profile: &AdventureProfile) -> ProgressionState {
    dress(profile.progression.clone().unwrap_or_default(), profile)
}

/// A companion's saved progression; a new one starts at the hero's level.
pub fn companion_progression(profile: &AdventureProfile, companion_id: &str) -> ProgressionState {
    let saved = dict_get(&profile.companions, companion_id).and_then(|e| e.progression.clone());
    let state = saved.unwrap_or_else(|| state_at_level(hero_progression(profile).level));
    dress(state, profile)
}

/// The progression of `who`: [`HERO`], [`PLAYER_TWO`] or a companion id.
pub fn spider_progression(profile: &AdventureProfile, who: &str) -> ProgressionState {
    if who == PLAYER_TWO {
        return dress(profile.player_two.progression.clone().unwrap_or_default(), profile);
    }
    if who == HERO { hero_progression(profile) } else { companion_progression(profile, who) }
}

/// Store `state` as the progression of `who`. An unknown companion is
/// added under its id as its name (Python raises `KeyError`).
pub fn store_progression(profile: &mut AdventureProfile, who: &str, state: ProgressionState) {
    if who == PLAYER_TWO {
        profile.player_two.progression = Some(state);
    } else if who == HERO {
        profile.progression = Some(state);
    } else {
        let name = companion_by_id(who).map_or(who, |c| c.name);
        if dict_get(&profile.companions, who).is_none() {
            profile.companions.push((who.to_string(), SpiderEntry::new(name)));
        }
        if let Some((_, entry)) = profile.companions.iter_mut().find(|(k, _)| k == who) {
            entry.progression = Some(state);
        }
    }
}

/// Add a companion, at the hero's level. False if it was already unlocked
/// (or is not a companion).
pub fn unlock_companion(profile: &mut AdventureProfile, companion_id: &str) -> bool {
    let Some(info) = companion_by_id(companion_id) else { return false };
    if dict_get(&profile.companions, companion_id).is_some() {
        return false;
    }
    profile.companions.push((companion_id.to_string(), SpiderEntry::new(info.name)));
    let state = companion_progression(profile, companion_id);
    store_progression(profile, companion_id, state);
    true
}

/// Python's `round(x, 1)`.
fn round1(x: f64) -> f64 {
    format!("{x:.1}").parse().unwrap_or(x)
}

pub fn record_result(profile: &mut AdventureProfile, mission_id: &str, won: bool, seconds: f64) -> MissionRecord {
    let mut record = mission_record(profile, mission_id);
    if won {
        record.victories += 1;
        if record.best_seconds.is_none_or(|best| seconds < best) {
            record.best_seconds = Some(round1(seconds));
        }
    } else {
        record.defeats += 1;
    }
    dict_set(&mut profile.missions, mission_id, record);
    record
}

pub fn mission_record(profile: &AdventureProfile, mission_id: &str) -> MissionRecord {
    dict_get(&profile.missions, mission_id).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_adventure_hero.py's profile tests and the old-save
    //! migration of tests/test_armour_set.py.
    use super::*;

    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dc-profile-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        profile_path(&dir)
    }

    #[test]
    fn the_profile_round_trips_and_reads_the_first_format() {
        let path = scratch("roundtrip");
        let mut profile = fresh_profile();
        profile.name = "  Big   Spider  ".into();
        let mut state = state_at_level(4);
        state.skill_points = 2;
        profile.progression = Some(state);
        record_result(&mut profile, "territory", true, 93.46);
        assert!(save_profile(&profile, &path));
        let loaded = load_profile(&path);
        assert_eq!(loaded.name, "Big Spider");
        assert_eq!(hero_progression(&loaded).level, 4);
        assert_eq!(mission_record(&loaded, "territory").victories, 1);
        assert_eq!(mission_record(&loaded, "territory").best_seconds, Some(93.5));
        assert_eq!(load_profile(&path), loaded, "load is idempotent");

        // Version 1: only the progression.
        std::fs::write(
            &path,
            r#"{"progression": {"level": 3, "inventory": ["silk_carapace", "warden_crest"],
                                  "item_levels": {"warden_crest": 3}}}"#,
        )
        .unwrap();
        let old = load_profile(&path);
        assert_eq!(old.name, DEFAULT_HERO_NAME);
        assert_eq!(hero_progression(&old).level, 3);
        assert_eq!(old.armoury.owned, vec!["silk_carapace", "warden_crest"]);
        assert_eq!(old.armoury.levels, vec![("warden_crest".to_string(), 3)]);
        assert_eq!(old.companions.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), vec!["scout"]);
    }

    #[test]
    fn a_new_hero_starts_at_level_one_under_its_name() {
        let path = scratch("new");
        let profile = load_profile(&path);
        assert_eq!(profile, fresh_profile());
        assert_eq!(hero_progression(&profile).level, 1);
        assert_eq!(clean_name(Some(&json!(""))), DEFAULT_HERO_NAME);
        assert_eq!(clean_name(Some(&json!("x".repeat(40)))).len(), MAX_NAME_LENGTH);
        std::fs::write(&path, "[1, 2]").unwrap();
        assert_eq!(load_profile(&path), fresh_profile());
        std::fs::write(&path, "{broken").unwrap();
        assert_eq!(load_profile(&path), fresh_profile());
    }

    #[test]
    fn bad_values_are_cleaned_and_unknown_maps_fall_back() {
        let path = scratch("clean");
        std::fs::write(
            &path,
            r#"{"selected_map": "atlantis", "armoury": {"owned": ["sun_crown", "sun_crown", "nope"],
                "levels": {"sun_crown": 9, "nope": 3}, "spares": {"sun_crown": "2", "x": 1, "sun_aegis": 0},
                "amber": -4}, "missions": {"territory": {"victories": "x", "best_seconds": "12.5"}},
                "companions": {"hunter": {"name": "  Hunty "}, "dragon": {}}, "all_screens": 0,
                "player_two": {"name": ""}}"#,
        )
        .unwrap();
        let p = load_profile(&path);
        assert_eq!(p.selected_map, DEFAULT_MAP);
        assert_eq!(p.armoury.owned, vec!["sun_crown"]);
        assert_eq!(p.armoury.levels, vec![("sun_crown".to_string(), 5)]);
        assert_eq!(p.armoury.spares, vec![("sun_crown".to_string(), 2)]);
        assert_eq!(p.armoury.amber, 0);
        assert_eq!(
            mission_record(&p, "territory"),
            MissionRecord { victories: 0, defeats: 0, best_seconds: Some(12.5) }
        );
        assert_eq!(
            p.companions.iter().map(|(k, e)| (k.as_str(), e.name.as_str())).collect::<Vec<_>>(),
            vec![("scout", "Scout"), ("hunter", "Hunty")]
        );
        assert!(!p.all_screens);
        assert_eq!(p.player_two.name, DEFAULT_PLAYER_TWO_NAME);
    }

    #[test]
    fn companions_start_at_the_heros_level_and_unlock_once() {
        let mut profile = fresh_profile();
        profile.progression = Some(state_at_level(7));
        assert!(unlock_companion(&mut profile, "weaver"));
        assert!(!unlock_companion(&mut profile, "weaver"));
        assert!(!unlock_companion(&mut profile, "dragon"));
        assert_eq!(companion_progression(&profile, "weaver").level, 7);
        assert_eq!(spider_progression(&profile, PLAYER_TWO).level, 1);
        assert!(spider_progression(&profile, PLAYER_TWO).equipped.is_empty());
    }

    #[test]
    fn a_loss_counts_a_defeat_and_a_slower_win_keeps_the_best() {
        let mut profile = fresh_profile();
        record_result(&mut profile, "ember", true, 50.0);
        record_result(&mut profile, "ember", true, 80.0);
        let r = record_result(&mut profile, "ember", false, 10.0);
        assert_eq!(r, MissionRecord { victories: 2, defeats: 1, best_seconds: Some(50.0) });
    }
}
