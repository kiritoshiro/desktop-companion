//! Port of `state/runtime_state.py` (plus the file I/O of
//! `manager/persistence.py`): identity, migration and eviction rules for
//! `creatures.json`, and its atomic save.
//!
//! On disk (schema 3):
//! ```json
//! {"schema_version": 3, "launch": N,
//!  "creatures": {"<namespace>|<id>": {"name", "model", "personality",
//!                 "progression", "slot_id", "born", "last_seen"}},
//!  "bases": [...],
//!  "scene": {"cages": [...], "webs": [...], "nests": [...]}}
//! ```
//! The rules run on raw JSON like the Python (entries from older schemas are
//! arbitrary dicts). `bases` and the scene lists are owned by `dc-world` and
//! pass through as `serde_json::Value`. [`SavedCreature`] is the typed view
//! of one entry.

use std::path::{Path, PathBuf};

use dc_content::util::{JsonMap, casefold, is_py_int, py_int, py_str, python_json_dumps};
use dc_core::PyRandom;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::progression::ProgressionState;

/// Bump when the on-disk shape changes. Versions 1 and 2 migrate on load.
pub const STATE_SCHEMA_VERSION: i64 = 3;
/// An entry not seen for this many launches is dropped on the next save.
pub const RETAIN_LAUNCHES: i64 = 50;
pub const DEFAULT_NAMESPACE: &str = "default";
/// The scene lists, in file order.
pub const SCENE_KEYS: [&str; 3] = ["cages", "webs", "nests"];

/// `normalize_namespace`: the case-insensitive namespace for a preset stem.
pub fn normalize_namespace(value: &str) -> String {
    let text = casefold(value.trim());
    if text.is_empty() { DEFAULT_NAMESPACE.into() } else { text }
}

/// `normalize_state_key`: `<namespace>|<rest>` with the namespace folded;
/// `None` for keys with no namespace or an empty rest.
pub fn normalize_state_key(key: &str) -> Option<String> {
    let (namespace, rest) = key.split_once('|')?;
    if rest.is_empty() {
        return None;
    }
    Some(format!("{}|{rest}", normalize_namespace(namespace)))
}

/// `new_creature_id`: `<namespace>|<12 hex>`; a seeded `rng` mints
/// reproducible ids with `getrandbits(48)`.
pub fn new_creature_id(namespace: &str, rng: Option<&mut PyRandom>) -> String {
    let token = match rng {
        Some(rng) => rng.getrandbits(48),
        None => PyRandom::from_entropy().getrandbits(48),
    };
    format!("{}|{token:012x}", normalize_namespace(namespace))
}

fn split_member(key: &str) -> Option<(&str, &str)> {
    let rest = key.split_once('|').map(|(_, r)| r).unwrap_or(key);
    let (head, tail) = rest.rsplit_once(':')?;
    if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
        Some((head, tail))
    } else {
        None
    }
}

/// `slot_id_from_key`: the slot a version-2 key (`ns|slot:index`) belonged to.
pub fn slot_id_from_key(key: &str) -> String {
    match split_member(key) {
        Some((head, _)) => head.to_string(),
        None => key.split_once('|').map(|(_, r)| r).unwrap_or(key).to_string(),
    }
}

/// `member_index_from_key`.
pub fn member_index_from_key(key: &str) -> Option<i64> {
    split_member(key).and_then(|(_, tail)| tail.parse().ok())
}

/// `next_birth_ordinal`: one past the highest `born` across all entries.
pub fn next_birth_ordinal(states: &JsonMap) -> i64 {
    let mut highest = -1;
    for entry in states.values() {
        let Some(entry) = entry.as_object() else { continue };
        let born = match entry.get("born") {
            None => Some(-1),
            Some(v) => py_int(v),
        };
        if let Some(b) = born {
            highest = highest.max(b);
        }
    }
    highest + 1
}

/// `entry_rank`: (level, total_xp), so a key collision keeps the more
/// advanced spider. Older entries kept progression at the top level.
pub fn entry_rank(entry: &Value) -> (i64, i64) {
    let Some(obj) = entry.as_object() else { return (0, 0) };
    let progression = obj.get("progression").and_then(Value::as_object).unwrap_or(obj);
    let level = progression.get("level").map(|v| py_int(v).unwrap_or(1)).unwrap_or(1);
    let total_xp = progression.get("total_xp").map(|v| py_int(v).unwrap_or(0)).unwrap_or(0);
    (level, total_xp)
}

/// `next_launch`: the launch counter for this session.
pub fn next_launch(data: Option<&Value>) -> i64 {
    let Some(obj) = data.and_then(Value::as_object) else { return 1 };
    match obj.get("launch") {
        None => 1,
        Some(v) => py_int(v).map(|l| l.max(0) + 1).unwrap_or(1),
    }
}

/// `migrate_creatures`: canonicalise keys (higher rank wins a collision),
/// default `last_seen`, recover `slot_id` and `born` from version-2 keys.
/// The input is not mutated.
pub fn migrate_creatures(raw: Option<&Value>, launch: i64) -> JsonMap {
    let mut merged = JsonMap::new();
    let Some(raw) = raw.and_then(Value::as_object) else { return merged };
    for (key, entry) in raw {
        let Some(canonical) = normalize_state_key(key) else { continue };
        if !entry.is_object() {
            continue;
        }
        if let Some(existing) = merged.get(&canonical)
            && entry_rank(existing) >= entry_rank(entry)
        {
            continue;
        }
        merged.insert(canonical, entry.clone());
    }
    for (key, entry) in merged.iter_mut() {
        let entry = entry.as_object_mut().expect("only objects were merged");
        if !entry.get("last_seen").is_some_and(is_py_int) {
            entry.insert("last_seen".into(), json!(launch));
        }
        let slot = entry.get("slot_id").map(py_str).unwrap_or_default();
        if slot.trim().is_empty() {
            entry.insert("slot_id".into(), json!(slot_id_from_key(key)));
        }
        if !entry.get("born").is_some_and(is_py_int) {
            entry.insert("born".into(), json!(member_index_from_key(key).unwrap_or(0)));
        }
    }
    merged
}

/// `saved_slot_members`: saved ids for one preset slot, oldest first.
pub fn saved_slot_members(states: &JsonMap, namespace: &str, slot_id: &str) -> Vec<String> {
    let prefix = format!("{}|", normalize_namespace(namespace));
    let mut found: Vec<(i64, String)> = Vec::new();
    for (key, entry) in states {
        let Some(entry) = entry.as_object() else { continue };
        if !key.starts_with(&prefix) {
            continue;
        }
        if entry.get("slot_id").map(py_str).unwrap_or_default() != slot_id {
            continue;
        }
        let born = entry.get("born").map(|v| py_int(v).unwrap_or(0)).unwrap_or(0);
        found.push((born, key.clone()));
    }
    found.sort();
    found.into_iter().map(|(_, k)| k).collect()
}

/// `slot_member_ids`: `count` stable ids for a slot, saved spiders first,
/// the rest minted (skipping any collision).
pub fn slot_member_ids(
    states: &JsonMap,
    namespace: &str,
    slot_id: &str,
    count: i64,
    mut mint: impl FnMut() -> String,
) -> Vec<String> {
    let wanted = count.max(0) as usize;
    let mut ids: Vec<String> = saved_slot_members(states, namespace, slot_id).into_iter().take(wanted).collect();
    while ids.len() < wanted {
        let candidate = mint();
        if ids.contains(&candidate) {
            continue;
        }
        ids.push(candidate);
    }
    ids
}

/// `load_payload`: `(creatures, bases)` from any supported payload.
pub fn load_payload(data: Option<&Value>, launch: i64) -> (JsonMap, Vec<Value>) {
    let Some(obj) = data.and_then(Value::as_object) else { return (JsonMap::new(), Vec::new()) };
    let bases = obj.get("bases").and_then(Value::as_array).cloned().unwrap_or_default();
    (migrate_creatures(obj.get("creatures"), launch), bases)
}

/// The saved scene: cages, webs and fly nests, each validated by its owner.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default)]
    pub cages: Vec<Value>,
    #[serde(default)]
    pub webs: Vec<Value>,
    #[serde(default)]
    pub nests: Vec<Value>,
}

impl Scene {
    pub fn to_value(&self) -> Value {
        json!({"cages": self.cages, "webs": self.webs, "nests": self.nests})
    }
    pub fn is_empty(&self) -> bool {
        self.cages.is_empty() && self.webs.is_empty() && self.nests.is_empty()
    }
}

/// `load_scene`: always the three lists (a non-list entry becomes empty).
pub fn load_scene(data: Option<&Value>) -> Scene {
    let scene = data.and_then(Value::as_object).and_then(|o| o.get("scene")).and_then(Value::as_object);
    let list = |k: &str| scene.and_then(|s| s.get(k)).and_then(Value::as_array).cloned().unwrap_or_default();
    Scene { cages: list("cages"), webs: list("webs"), nests: list("nests") }
}

/// `stamp_seen`: a copy of the entry marked present in this launch.
pub fn stamp_seen(entry: &JsonMap, launch: i64) -> JsonMap {
    let mut entry = entry.clone();
    entry.insert("last_seen".into(), json!(launch));
    entry
}

/// `evict`: drop entries not seen for more than `retain` launches.
pub fn evict(states: &JsonMap, launch: i64, retain: i64) -> JsonMap {
    let horizon = retain.max(0);
    let mut kept = JsonMap::new();
    for (key, entry) in states {
        let Some(obj) = entry.as_object() else { continue };
        let last_seen = obj.get("last_seen").map(|v| py_int(v).unwrap_or(launch)).unwrap_or(launch);
        if launch - last_seen <= horizon {
            kept.insert(key.clone(), entry.clone());
        }
    }
    kept
}

/// `build_payload`: the on-disk payload for a save.
pub fn build_payload(states: &JsonMap, bases: &[Value], launch: i64, scene: Option<&Scene>) -> Value {
    let scene = scene.cloned().unwrap_or_default();
    json!({
        "schema_version": STATE_SCHEMA_VERSION,
        "launch": launch,
        "creatures": Value::Object(states.clone()),
        "bases": bases,
        "scene": scene.to_value(),
    })
}

/// `build_payload` with a raw scene dict (`scene.get(key) or []` per list).
pub fn build_payload_raw_scene(states: &JsonMap, bases: &[Value], launch: i64, scene: Option<&Value>) -> Value {
    let raw = scene.and_then(Value::as_object);
    let list = |k: &str| raw.and_then(|s| s.get(k)).and_then(Value::as_array).cloned().unwrap_or_default();
    let scene = Scene { cages: list("cages"), webs: list("webs"), nests: list("nests") };
    build_payload(states, bases, launch, Some(&scene))
}

/// Write a payload like `json.dump(payload, indent=2)` + newline, through
/// `<name>.tmp` and a rename, creating the parent folder.
pub fn write_payload_atomic(path: &Path, payload: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = python_json_dumps(payload, Some(2));
    text.push('\n');
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// `reset_saved_progress(path)`: forget every spider and base, keep the
/// scene and the launch counter. True if the file was written (or absent).
pub fn reset_saved_progress(path: &Path) -> bool {
    let data = match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => json!({}),
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(json!({})),
    };
    let data = if data.is_object() { data } else { json!({}) };
    let launch = match data.get("launch") {
        Some(v) if dc_content::util::py_truthy(v) => py_int(v).filter(|l| *l != 0).unwrap_or(1),
        _ => 1,
    };
    let payload = build_payload_raw_scene(&JsonMap::new(), &[], launch, data.get("scene"));
    write_payload_atomic(path, &payload).is_ok()
}

/// The default state file: `state_dir()/creatures.json`.
pub fn default_state_path() -> PathBuf {
    dc_content::discovery::state_dir().join("creatures.json")
}

/// A loaded state file (`_load_progression_states` in the manager).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeState {
    /// This session's launch number (stored + 1).
    pub launch: i64,
    /// Migrated entries by canonical key.
    pub creatures: JsonMap,
    pub bases: Vec<Value>,
    pub scene: Scene,
}

impl RuntimeState {
    /// Read and migrate a state file; a missing or corrupt file is an empty one.
    pub fn load(path: &Path) -> Self {
        let data: Option<Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok());
        Self::from_payload(data.as_ref())
    }

    pub fn from_payload(data: Option<&Value>) -> Self {
        let launch = next_launch(data);
        let (creatures, bases) = load_payload(data, launch);
        Self { launch, creatures, bases, scene: load_scene(data) }
    }

    /// Record one spider as present this launch (the manager's save loop),
    /// keeping its birth ordinal or assigning `next_ordinal`.
    pub fn record(&mut self, key: &str, creature: &SavedCreature, next_ordinal: &mut i64) {
        let born = self.creatures.get(key).and_then(|p| p.get("born")).and_then(py_int);
        let born = born.unwrap_or_else(|| {
            let b = *next_ordinal;
            *next_ordinal += 1;
            b
        });
        let mut entry = creature.clone();
        entry.born = born;
        let Value::Object(map) = entry.to_value() else { unreachable!("an entry is an object") };
        self.creatures.insert(key.to_string(), Value::Object(stamp_seen(&map, self.launch)));
    }

    /// Evict stale entries and write atomically (the manager's
    /// `save_runtime_state`, minus collecting the live spiders).
    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let states = evict(&self.creatures, self.launch, RETAIN_LAUNCHES);
        let payload = build_payload(&states, &self.bases, self.launch, Some(&self.scene));
        write_payload_atomic(path, &payload)?;
        self.creatures = states;
        Ok(())
    }
}

/// Typed view of one saved creature entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCreature {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(default)]
    pub progression: ProgressionState,
    #[serde(default)]
    pub slot_id: String,
    #[serde(default, deserialize_with = "dc_content::util::de::i64_lenient")]
    pub born: i64,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "dc_content::util::de::opt_i64")]
    pub last_seen: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl SavedCreature {
    /// Lenient parse of a stored entry (an unusable entry gives `None`).
    pub fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let progression = obj.get("progression").map(ProgressionState::from_value).unwrap_or_default();
        let s = |k: &str| obj.get(k).filter(|v| !v.is_null()).map(py_str);
        let known = ["name", "model", "personality", "progression", "slot_id", "born", "last_seen"];
        Some(Self {
            name: s("name").unwrap_or_default(),
            model: s("model"),
            personality: s("personality"),
            progression,
            slot_id: s("slot_id").unwrap_or_default(),
            born: obj.get("born").and_then(py_int).unwrap_or(0),
            last_seen: obj.get("last_seen").and_then(py_int),
            extra: obj
                .iter()
                .filter(|(k, _)| !known.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
    }

    /// The entry as the manager writes it (`name, model, personality,
    /// progression, slot_id, born`, then `last_seen` if set).
    pub fn to_value(&self) -> Value {
        let mut m = JsonMap::new();
        m.insert("name".into(), json!(self.name));
        m.insert("model".into(), self.model.as_ref().map(|s| json!(s)).unwrap_or(Value::Null));
        m.insert("personality".into(), self.personality.as_ref().map(|s| json!(s)).unwrap_or(Value::Null));
        m.insert("progression".into(), self.progression.to_value());
        m.insert("slot_id".into(), json!(self.slot_id));
        m.insert("born".into(), json!(self.born));
        if let Some(l) = self.last_seen {
            m.insert("last_seen".into(), json!(l));
        }
        for (k, v) in &self.extra {
            m.insert(k.clone(), v.clone());
        }
        Value::Object(m)
    }
}
