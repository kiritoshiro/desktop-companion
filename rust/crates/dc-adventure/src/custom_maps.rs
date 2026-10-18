//! Port of app/custom_maps.py
//!
//! Maps the player makes in the map editor. A custom map is one JSON file in
//! `<state>/maps/custom-*.json`. It says, in the same terms the built-in maps
//! use (map_layouts, campaign):
//!
//! - the map: title, one line, tier, which armour qualities random enemies
//!   wear, the companion it rewards, the spider cap, seconds between the
//!   Hatchery's reinforcements, and how well its enemies fight together;
//! - every building: kind, name, place, who holds it at the start, Hatchery
//!   reserves, healing supply, and its guards;
//! - enemies standing on their own, anywhere;
//! - the boss: name, kind, armour, strength.
//!
//! A spider (guard, enemy or boss) is a [`SpiderSpec`]. Python keeps maps as
//! dicts; here [`clean_map`] turns any JSON value into a typed [`CustomMap`],
//! and [`CustomMap::to_value`] writes the same keys in the same order.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::campaign::{MapInfo, MapKind, companion_by_id};
use crate::map_layouts::{BuildingKind, Guard, Placement, layout};
use crate::progression::{ARMOR_SETS, ARMOR_TIERS, MAX_ITEM_LEVEL, armor_by_id, armor_set, is_enemy_kind};
use crate::pyjson::{atomic_write, clamped_float, clamped_int, dumps_indent2, py_str, read_json, take_chars, truthy};

pub const PREFIX: &str = "custom-";
pub const ROLES: [&str; 4] = ["guard", "weaver", "hunter", "spitter"];
pub const BUILDING_KINDS: [BuildingKind; 9] = BuildingKind::ALL;

/// The default name of each building kind.
pub fn building_name(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Home => "Home burrow",
        BuildingKind::Food => "Food cache",
        BuildingKind::Silk => "Silk loom",
        BuildingKind::Hatchery => "Hatchery",
        BuildingKind::Nest => "Enemy nest",
        BuildingKind::Venom => "Venom den",
        BuildingKind::Lookout => "Lookout",
        BuildingKind::Amber => "Amber mine",
        BuildingKind::Nursery => "Nursery",
    }
}

/// Exactly one of each; a map without them cannot be won or has no start.
pub const REQUIRED: [BuildingKind; 3] = [BuildingKind::Home, BuildingKind::Hatchery, BuildingKind::Nest];

pub fn maps_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("maps")
}

pub fn is_custom(map_id: &str) -> bool {
    map_id.starts_with(PREFIX)
}

// -- specs ---------------------------------------------------------------------

/// What a spider wears: the map's rules, or exactly these pieces.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmorChoice {
    Auto,
    Pieces(Vec<String>),
}

impl ArmorChoice {
    pub fn to_value(&self) -> Value {
        match self {
            ArmorChoice::Auto => Value::String("auto".into()),
            ArmorChoice::Pieces(p) => json!(p),
        }
    }

    pub fn pieces(&self) -> Option<&[String]> {
        match self {
            ArmorChoice::Auto => None,
            ArmorChoice::Pieces(p) => Some(p),
        }
    }
}

/// A lone enemy's place on the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpecPlace {
    pub fx: f64,
    pub fy: f64,
    pub far: bool,
}

/// A guard, enemy or boss: its role (how it fights), its kind ("auto": the
/// map picks), a level above the map's, a health multiplier and its armour.
#[derive(Debug, Clone, PartialEq)]
pub struct SpiderSpec {
    pub role: String,
    pub kind: String,
    pub level_bonus: i64,
    pub hp: f64,
    pub armor: ArmorChoice,
    pub item_level: i64,
    /// Bosses only.
    pub name: Option<String>,
    /// Lone enemies only.
    pub place: Option<SpecPlace>,
}

impl Default for SpiderSpec {
    fn default() -> Self {
        spider_spec("guard")
    }
}

impl SpiderSpec {
    /// The spec as Python's dict, keys in Python's order.
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("role".into(), json!(self.role));
        m.insert("kind".into(), json!(self.kind));
        m.insert("level_bonus".into(), json!(self.level_bonus));
        m.insert("hp".into(), json!(self.hp));
        m.insert("armor".into(), self.armor.to_value());
        m.insert("item_level".into(), json!(self.item_level));
        if let Some(name) = &self.name {
            m.insert("name".into(), json!(name));
        }
        if let Some(place) = &self.place {
            m.insert("fx".into(), json!(place.fx));
            m.insert("fy".into(), json!(place.fy));
            m.insert("far".into(), json!(place.far));
        }
        Value::Object(m)
    }
}

/// `spider_spec(role)`: kind "auto", no level bonus, normal health, the
/// map's armour at item level 1. Set other fields with struct update syntax.
pub fn spider_spec(role: &str) -> SpiderSpec {
    SpiderSpec {
        role: role.into(),
        kind: "auto".into(),
        level_bonus: 0,
        hp: 1.0,
        armor: ArmorChoice::Auto,
        item_level: 1,
        name: None,
        place: None,
    }
}

/// Python iterates whatever `x or []` is: a list's items, a string's
/// characters, a dict's keys. Anything else iterates as nothing here.
fn py_iter(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => s.chars().map(|c| Value::String(c.to_string())).collect(),
        Some(Value::Object(o)) => o.keys().map(|k| Value::String(k.clone())).collect(),
        _ => Vec::new(),
    }
}

fn get_or<'a>(raw: &'a Map<String, Value>, key: &str, default: &'a Value) -> &'a Value {
    raw.get(key).unwrap_or(default)
}

pub fn clean_spec(raw: &Value, boss: bool) -> SpiderSpec {
    let empty = Map::new();
    let raw = raw.as_object().unwrap_or(&empty);
    let mut spec = spider_spec("guard");
    let role = raw.get("role").and_then(Value::as_str).unwrap_or(if raw.contains_key("role") { "" } else { "guard" });
    spec.role = if boss {
        "guardian".into()
    } else if ROLES.contains(&role) {
        role.into()
    } else {
        "guard".into()
    };
    let kind = raw.get("kind").map_or(Some("auto"), Value::as_str);
    spec.kind = match kind {
        Some(k) if is_enemy_kind(k) => k.into(),
        _ => "auto".into(),
    };
    spec.level_bonus = clamped_int(get_or(raw, "level_bonus", &json!(0)), -5, 20);
    spec.hp = clamped_float(get_or(raw, "hp", &json!(1.0)), 0.2, 10.0);
    spec.armor = match raw.get("armor") {
        Some(Value::Array(items)) => {
            let mut pieces: Vec<String> = Vec::new();
            let mut slots: Vec<&str> = Vec::new();
            for item in items.iter().filter_map(Value::as_str) {
                if let Some(piece) = armor_by_id(item)
                    && !slots.contains(&piece.slot)
                {
                    pieces.push(item.into());
                    slots.push(piece.slot);
                }
            }
            ArmorChoice::Pieces(pieces)
        }
        _ => ArmorChoice::Auto,
    };
    spec.item_level = clamped_int(get_or(raw, "item_level", &json!(1)), 1, MAX_ITEM_LEVEL);
    if boss {
        let name = match raw.get("name") {
            Some(v) if truthy(v) => py_str(v),
            _ => "Guardian".into(),
        };
        spec.name = Some(take_chars(&name, 40));
    }
    spec
}

// -- maps ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CustomBuilding {
    pub kind: BuildingKind,
    pub name: String,
    pub fx: f64,
    pub fy: f64,
    pub far: bool,
    pub owned: bool,
    pub reserves: i64,
    pub supply: f64,
    pub guards: Vec<SpiderSpec>,
}

impl CustomBuilding {
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind.id(),
            "name": self.name,
            "fx": self.fx,
            "fy": self.fy,
            "far": self.far,
            "owned": self.owned,
            "reserves": self.reserves,
            "supply": self.supply,
            "guards": self.guards.iter().map(SpiderSpec::to_value).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomMap {
    pub id: String,
    pub title: String,
    pub blurb: String,
    pub tier: i64,
    pub enemy_tiers: Vec<String>,
    pub reward_companion: Option<String>,
    pub cap: i64,
    pub wave_every: f64,
    /// How well its enemies fight together (world/tactics): -1 by the tier.
    pub tactics: i64,
    pub buildings: Vec<CustomBuilding>,
    /// Lone enemies; each has a `place`.
    pub enemies: Vec<SpiderSpec>,
    pub boss: SpiderSpec,
}

impl CustomMap {
    /// The map as Python's dict, keys in Python's order.
    pub fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "title": self.title,
            "blurb": self.blurb,
            "tier": self.tier,
            "enemy_tiers": self.enemy_tiers,
            "reward_companion": self.reward_companion,
            "cap": self.cap,
            "wave_every": self.wave_every,
            "tactics": self.tactics,
            "buildings": self.buildings.iter().map(CustomBuilding::to_value).collect::<Vec<_>>(),
            "enemies": self.enemies.iter().map(SpiderSpec::to_value).collect::<Vec<_>>(),
            "boss": self.boss.to_value(),
        })
    }
}

/// A playable starting point: the first map's buildings and guards.
pub fn new_map(title: &str) -> CustomMap {
    let mut buildings = Vec::new();
    for place in layout("territory").expect("the first map has a layout") {
        if place.far {
            continue;
        }
        let guards: Vec<Value> = place.guards.iter().map(|g| spider_spec(g.role()).to_value()).collect();
        buildings.push(json!({
            "kind": place.kind.id(), "name": place.name, "fx": place.fx, "fy": place.fy,
            "far": false, "owned": place.owned, "reserves": place.reserves,
            "supply": 180.0, "guards": guards,
        }));
    }
    let forager: Vec<&str> = armor_set("forager").map(|s| s.pieces()).unwrap_or_default();
    let boss = SpiderSpec {
        name: Some("Guardian".into()),
        armor: ArmorChoice::Pieces(forager.iter().map(|s| s.to_string()).collect()),
        item_level: 2,
        ..spider_spec("guardian")
    };
    clean_map(&json!({
        "id": "", "title": title, "blurb": "A map of your own.", "tier": 1,
        "enemy_tiers": ["common", "uncommon"], "buildings": buildings, "enemies": [],
        "boss": boss.to_value(),
    }))
}

pub fn clean_map(raw: &Value) -> CustomMap {
    let empty = Map::new();
    let raw = raw.as_object().unwrap_or(&empty);
    let text = |key: &str| match raw.get(key) {
        Some(v) if truthy(v) => Some(py_str(v)),
        _ => None,
    };
    let title = text("title").unwrap_or_else(|| "My map".into());
    let title = title.trim();
    let title = take_chars(if title.is_empty() { "My map" } else { title }, 60);
    let mut enemy_tiers: Vec<String> = py_iter(raw.get("enemy_tiers"))
        .iter()
        .filter_map(Value::as_str)
        .filter(|t| ARMOR_TIERS.contains(t))
        .map(str::to_string)
        .collect();
    if enemy_tiers.is_empty() {
        enemy_tiers = vec!["common".into()];
    }
    let reward_companion = raw
        .get("reward_companion")
        .and_then(Value::as_str)
        .filter(|c| companion_by_id(c).is_some())
        .map(str::to_string);
    let mut data = CustomMap {
        id: text("id").unwrap_or_default(),
        title,
        blurb: take_chars(&text("blurb").unwrap_or_default(), 200),
        tier: clamped_int(get_or(raw, "tier", &json!(1)), 1, 6),
        enemy_tiers,
        reward_companion,
        cap: clamped_int(get_or(raw, "cap", &json!(12)), 4, 30),
        wave_every: clamped_float(get_or(raw, "wave_every", &json!(24.0)), 5.0, 300.0),
        tactics: clamped_int(get_or(raw, "tactics", &json!(-1)), -1, 3),
        buildings: Vec::new(),
        enemies: Vec::new(),
        boss: clean_spec(raw.get("boss").unwrap_or(&Value::Null), true),
    };
    for b in py_iter(raw.get("buildings")) {
        let Some(b) = b.as_object() else { continue };
        let Some(kind) = b.get("kind").and_then(Value::as_str).and_then(BuildingKind::from_id) else {
            continue;
        };
        let default_name = building_name(kind);
        let name = match b.get("name") {
            Some(v) if truthy(v) => py_str(v),
            _ => default_name.into(),
        };
        let name = name.trim();
        let hatchery = kind == BuildingKind::Hatchery;
        data.buildings.push(CustomBuilding {
            kind,
            name: take_chars(if name.is_empty() { default_name } else { name }, 40),
            fx: clamped_float(get_or(b, "fx", &json!(0.5)), 0.0, 1.0),
            fy: clamped_float(get_or(b, "fy", &json!(0.5)), 0.0, 1.0),
            far: b.get("far").is_some_and(truthy),
            owned: b.get("owned").map_or(kind == BuildingKind::Home, truthy),
            reserves: clamped_int(get_or(b, "reserves", &json!(if hatchery { 6 } else { 0 })), 0, 50),
            supply: clamped_float(get_or(b, "supply", &json!(180.0)), 0.0, 5000.0),
            guards: py_iter(b.get("guards")).iter().map(|g| clean_spec(g, false)).take(8).collect(),
        });
    }
    for e in py_iter(raw.get("enemies")) {
        let Some(obj) = e.as_object() else { continue };
        let mut spec = clean_spec(&e, false);
        spec.place = Some(SpecPlace {
            fx: clamped_float(get_or(obj, "fx", &json!(0.5)), 0.0, 1.0),
            fy: clamped_float(get_or(obj, "fy", &json!(0.5)), 0.0, 1.0),
            far: obj.get("far").is_some_and(truthy),
        });
        data.enemies.push(spec);
    }
    data.enemies.truncate(20);
    data
}

/// What stops a map being played, in words; empty when it is fine.
pub fn problems(data: &CustomMap) -> Vec<String> {
    let mut found = Vec::new();
    for kind in REQUIRED {
        let count = data.buildings.iter().filter(|b| b.kind == kind).count();
        if count == 0 {
            found.push(format!("Needs a {}.", building_name(kind)));
        } else if count > 1 {
            found.push(format!("Only one {} is allowed.", building_name(kind)));
        }
    }
    if let Some(home) = data.buildings.iter().find(|b| b.kind == BuildingKind::Home)
        && (home.far || !home.owned)
    {
        found.push("The Home burrow must be yours and on the main screen.".into());
    }
    if !data.buildings.iter().any(|b| !REQUIRED.contains(&b.kind)) {
        found.push("Needs at least one other building to capture first.".into());
    }
    found
}

/// The map's card: the set named on it is the one the boss wears most of.
pub fn map_info(data: &CustomMap) -> MapInfo {
    let boss = &data.boss;
    let armour: &[String] = boss.armor.pieces().unwrap_or(&[]);
    let mut guardian_set = "forager";
    if !armour.is_empty() {
        let mut best = 0;
        for set in &ARMOR_SETS {
            let pieces = set.pieces();
            let count = armour.iter().filter(|i| pieces.contains(&i.as_str())).count();
            if count > best {
                best = count;
                guardian_set = set.id;
            }
        }
    }
    MapInfo {
        id: data.id.clone(),
        title: data.title.clone(),
        blurb: if data.blurb.is_empty() { "A map of your own.".into() } else { data.blurb.clone() },
        tier: data.tier,
        unlock_after: None,
        enemy_tiers: data.enemy_tiers.clone(),
        guardian: boss.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "Guardian".into()),
        guardian_set: guardian_set.into(),
        reward_companion: data.reward_companion.clone(),
        kind: MapKind::Raid,
    }
}

/// The buildings as the mission reads them; a building placed on the far
/// screen falls back to the same spot on the main one.
pub fn placements(data: &CustomMap) -> Vec<Placement> {
    data.buildings
        .iter()
        .map(|b| Placement {
            kind: b.kind,
            name: b.name.clone(),
            fx: b.fx,
            fy: b.fy,
            guards: b.guards.iter().cloned().map(Guard::Spec).collect(),
            reserves: b.reserves,
            owned: b.owned,
            far: b.far,
            alt: Some((b.fx, b.fy)),
            supply: b.supply,
        })
        .collect()
}

// -- files -----------------------------------------------------------------------

fn map_path(map_id: &str, state_dir: &Path) -> PathBuf {
    maps_dir(state_dir).join(format!("{map_id}.json"))
}

/// Every saved map, by title (case-insensitive).
pub fn list_maps(state_dir: &Path) -> Vec<CustomMap> {
    let Ok(entries) = std::fs::read_dir(maps_dir(state_dir)) else {
        return Vec::new();
    };
    let mut stems: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(PREFIX) && n.ends_with(".json"))
        .map(|n| n.trim_end_matches(".json").to_string())
        .collect();
    stems.sort();
    let mut maps: Vec<CustomMap> = stems.iter().filter_map(|s| load_map(s, state_dir)).collect();
    maps.sort_by_key(|m| casefold(&m.title));
    maps
}

pub fn load_map(map_id: &str, state_dir: &Path) -> Option<CustomMap> {
    if !is_custom(map_id) {
        return None;
    }
    let raw = read_json(&map_path(map_id, state_dir))?;
    let mut data = clean_map(&raw);
    data.id = map_id.into();
    Some(data)
}

/// Write a map; a new one gets an id from its title. Returns the id.
pub fn save_map(data: &CustomMap, state_dir: &Path) -> io::Result<String> {
    let mut data = clean_map(&data.to_value());
    if !is_custom(&data.id) {
        let mut stem = slug(&data.title);
        if stem.is_empty() {
            stem = "map".into();
        }
        let mut map_id = format!("{PREFIX}{stem}");
        let mut n = 2;
        while map_path(&map_id, state_dir).exists() {
            map_id = format!("{PREFIX}{stem}-{n}");
            n += 1;
        }
        data.id = map_id;
    }
    atomic_write(&map_path(&data.id, state_dir), &dumps_indent2(&data.to_value()))?;
    Ok(data.id)
}

pub fn delete_map(map_id: &str, state_dir: &Path) -> bool {
    is_custom(map_id) && std::fs::remove_file(map_path(map_id, state_dir)).is_ok()
}

/// Python's `str.casefold()` for the common cases.
fn casefold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// `re.sub(r"[^a-z0-9]+", "-", title.casefold()).strip("-")`.
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in casefold(title).chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Port of the data parts of tests/test_editor_and_admin.py.
    use super::*;
    use crate::campaign::{chosen_map, map_unlocked};

    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dc-maps-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    pub(crate) fn my_map() -> CustomMap {
        let mut data = new_map("Spider fort");
        data.tier = 2;
        data.cap = 16;
        data.wave_every = 10.0;
        let food = data.buildings.iter_mut().find(|b| b.kind == BuildingKind::Food).unwrap();
        food.supply = 999.0;
        food.guards = vec![SpiderSpec {
            kind: "thorn_weaver".into(),
            level_bonus: 3,
            hp: 2.0,
            armor: ArmorChoice::Pieces(vec!["frost_aegis".into()]),
            item_level: 4,
            ..spider_spec("weaver")
        }];
        data.buildings.push(CustomBuilding {
            kind: BuildingKind::Venom,
            name: "Poison well".into(),
            fx: 0.3,
            fy: 0.4,
            far: true,
            owned: false,
            reserves: 0,
            supply: 180.0,
            guards: vec![spider_spec("spitter")],
        });
        data.enemies.push(SpiderSpec {
            kind: "ash_wolf".into(),
            place: Some(SpecPlace { fx: 0.7, fy: 0.8, far: false }),
            ..spider_spec("hunter")
        });
        data.boss = SpiderSpec {
            name: Some("Fort lord".into()),
            kind: "thorn_crown".into(),
            armor: ArmorChoice::Pieces(armor_set("sun").unwrap().pieces().iter().map(|s| s.to_string()).collect()),
            item_level: 5,
            hp: 1.5,
            ..spider_spec("guardian")
        };
        data
    }

    #[test]
    fn a_new_map_is_playable_and_saves_and_loads_back() {
        let state = scratch("save");
        assert!(problems(&new_map("My map")).is_empty());
        let map_id = save_map(&my_map(), &state).unwrap();
        assert!(map_id.starts_with("custom-spider-fort"));
        let again = load_map(&map_id, &state).unwrap();
        assert_eq!(again.title, "Spider fort");
        assert_eq!(again.cap, 16);
        assert_eq!(again.boss.name.as_deref(), Some("Fort lord"));
        assert_eq!(again.boss.kind, "thorn_crown");
        assert_eq!(list_maps(&state).iter().map(|m| m.id.clone()).collect::<Vec<_>>(), vec![map_id.clone()]);
        assert_ne!(save_map(&my_map(), &state).unwrap(), map_id, "a second map of the same title gets its own file");
        let info = map_info(&again);
        assert_eq!(info.guardian_set, "sun");
        assert_eq!(info.guardian, "Fort lord");
        assert!(map_unlocked(&Vec::new(), &map_id, false, &state), "your own maps are always open");
        let venom = placements(&again).into_iter().find(|p| p.kind == BuildingKind::Venom).unwrap();
        assert!(venom.far && venom.alt == Some((0.3, 0.4)));
        assert_eq!(again.enemies[0].place, Some(SpecPlace { fx: 0.7, fy: 0.8, far: false }));
    }

    #[test]
    fn a_map_without_its_essentials_says_why() {
        let mut data = new_map("My map");
        data.buildings.retain(|b| b.kind != BuildingKind::Nest);
        let first = data.buildings[0].clone();
        data.buildings.push(first);
        let found = problems(&data).join(" ");
        assert!(found.contains("Enemy nest") && found.contains("one Home burrow"), "{found}");
    }

    #[test]
    fn bad_values_in_a_file_are_cleaned() {
        let data = clean_map(&json!({"title": "", "tier": 99, "cap": -3,
            "buildings": [{"kind": "castle"}, {"kind": "food", "fx": 5, "guards": [
                {"role": "wizard", "kind": "dragon", "armor": ["nope", "frost_aegis", "frost_aegis"]}]}]}));
        assert_eq!(data.title, "My map");
        assert_eq!(data.tier, 6);
        assert_eq!(data.cap, 4);
        assert_eq!(data.buildings.iter().map(|b| b.kind).collect::<Vec<_>>(), vec![BuildingKind::Food]);
        let guard = &data.buildings[0].guards[0];
        assert_eq!(guard.role, "guard");
        assert_eq!(guard.kind, "auto");
        assert_eq!(guard.armor, ArmorChoice::Pieces(vec!["frost_aegis".into()]));
        assert_eq!(data.buildings[0].fx, 1.0);
    }

    #[test]
    fn an_editor_map_can_choose_its_enemy_teamwork() {
        assert_eq!(clean_map(&json!({"title": "Mine", "tactics": 3})).tactics, 3);
        assert_eq!(clean_map(&json!({"title": "Mine"})).tactics, -1, "by the tier unless chosen");
    }

    #[test]
    fn a_deleted_map_falls_back_to_the_first() {
        let state = scratch("delete");
        let map_id = save_map(&my_map(), &state).unwrap();
        let profile = crate::adventure_profile::AdventureProfile { selected_map: map_id.clone(), ..Default::default() };
        assert_eq!(chosen_map(&profile, &state).id, map_id);
        assert!(delete_map(&map_id, &state));
        assert!(!delete_map(&map_id, &state));
        assert_eq!(chosen_map(&profile, &state).id, "territory");
    }

    #[test]
    fn slugs_follow_the_python_regex() {
        assert_eq!(slug("Spider fort"), "spider-fort");
        assert_eq!(slug("  --Hello, World!! 2 "), "hello-world-2");
        assert_eq!(slug("ÄÖ"), "");
    }
}
