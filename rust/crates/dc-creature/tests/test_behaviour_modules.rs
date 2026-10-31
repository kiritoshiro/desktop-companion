//! Port of `tests/test_behaviour_modules.py`: behaviour modules replace the
//! `_is_*_personality` id branches (DC-19, C5).
//!
//! not ported: test_no_is_personality_branches_remain (greps Python source text).

mod common;

use dc_content::OrderedMap;
use dc_content::discovery::discover_personalities;
use dc_content::personality::Personality;
use dc_content::personality_profiles::{
    BEHAVIOUR_MODULE_IDS, PersonalityRef, behaviour_modules_for, movement_profile_for, selectable_personality_ids,
};
use dc_creature::Creature;
use dc_creature::creature::{Modules, typed_personality};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn personalities() -> OrderedMap<Personality> {
    let (found, warnings) = discover_personalities(Some(&common::root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    found
}

fn set(ids: &[&'static str]) -> BTreeSet<&'static str> {
    ids.iter().copied().collect()
}

fn modules_of(value: Value) -> BTreeSet<&'static str> {
    let p = typed_personality(&value);
    behaviour_modules_for(PersonalityRef::Def(&p))
}

#[test]
fn test_shipped_personality_selects_its_module() {
    let found = personalities();
    let cases: [(&str, &[&'static str]); 11] = [
        ("hunter", &["hunter"]),
        ("trapper", &["hunter", "web_shooter"]),
        ("jumper", &["jumper"]),
        ("observer", &["observer"]),
        ("nope", &["nope"]),
        ("drifter", &["drifter"]),
        ("webber", &["webber"]),
        ("balanced", &[]),
        ("bold", &[]),
        ("curious", &[]),
        ("playful", &[]),
    ];
    for (id, expected) in cases {
        let p = found.get(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(behaviour_modules_for(PersonalityRef::Def(p)), set(expected), "{id}");
    }
}

#[test]
fn test_explicit_behaviour_modules_list_wins_over_legacy_flags() {
    let p = json!({"id": "hunter", "mouse_hunter": true, "behaviour_modules": ["drifter"]});
    assert_eq!(modules_of(p), set(&["drifter"]));
}

#[test]
fn test_explicit_list_ignores_unknown_module_ids() {
    let p = json!({"id": "balanced", "behaviour_modules": ["hunter", "not-a-module"]});
    assert_eq!(modules_of(p), set(&["hunter"]));
}

#[test]
fn test_an_unmigrated_personality_falls_back_to_legacy_flags() {
    let p = json!({"id": "custom", "web_weaver": true});
    assert_eq!(modules_of(p), set(&["webber"]));
}

#[test]
fn test_every_module_id_is_reachable() {
    let dir = common::root().join("personalities");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    let mut ids = BTreeSet::new();
    for path in paths {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        ids.extend(modules_of(value));
    }
    assert_eq!(ids, BEHAVIOUR_MODULE_IDS.iter().copied().collect::<BTreeSet<_>>());
}

const LEGACY_IDS: [&str; 7] = ["hunter", "jumper", "observer", "nope", "drifter", "webber", "trapper"];

#[test]
fn test_the_menu_offers_the_six_temperaments() {
    let found = personalities();
    let selectable = selectable_personality_ids(Some(&found), None);
    for compact_id in ["balanced", "playful", "curious", "bold", "cautious", "social"] {
        assert!(selectable.iter().any(|s| s == compact_id), "{compact_id}");
    }
    for legacy_id in LEGACY_IDS {
        assert!(!selectable.iter().any(|s| s == legacy_id), "{legacy_id}");
    }
}

#[test]
fn test_a_legacy_personality_already_in_use_is_still_offered() {
    let found = personalities();
    for legacy_id in LEGACY_IDS {
        let selectable = selectable_personality_ids(Some(&found), Some(legacy_id));
        assert!(selectable.iter().any(|s| s == legacy_id), "{legacy_id}");
    }
}

#[test]
fn test_a_personality_that_does_not_exist_is_not_offered() {
    let found = personalities();
    let selectable = selectable_personality_ids(Some(&found), Some("not_a_personality"));
    assert!(!selectable.iter().any(|s| s == "not_a_personality"));
}

#[test]
fn test_every_legacy_personality_still_loads_and_still_differs() {
    let found = personalities();
    for legacy_id in LEGACY_IDS {
        assert!(found.contains_key(legacy_id), "{legacy_id}");
    }
    let profiles: BTreeSet<&str> =
        LEGACY_IDS.iter().map(|id| movement_profile_for(PersonalityRef::Def(found.get(id).unwrap()))).collect();
    assert!(profiles.len() > 3, "{profiles:?}");
}

fn module_bit(module_id: &str) -> u8 {
    match module_id {
        "hunter" => Modules::HUNTER,
        "jumper" => Modules::JUMPER,
        "observer" => Modules::OBSERVER,
        "nope" => Modules::NOPE,
        "drifter" => Modules::DRIFTER,
        "webber" => Modules::WEBBER,
        "web_shooter" => Modules::WEB_SHOOTER,
        other => panic!("unknown module {other}"),
    }
}

/// `Creature(model, personality, 2400, 1400)`; seeded instead of unseeded.
fn creature(personality: Value) -> Creature {
    common::spider_with(common::load_model("spider"), personality, 1, |cfg| {
        cfg.screen_w = 2400.0;
        cfg.screen_h = 1400.0;
    })
}

#[test]
fn test_a_creature_actually_wires_its_module_through() {
    type ActsAs = fn(&Creature) -> bool;
    let cases: [(&str, ActsAs, &str); 7] = [
        ("hunter", Creature::acts_as_hunter, "hunter"),
        ("jumper", Creature::acts_as_jumper, "jumper"),
        ("observer", Creature::acts_as_observer, "observer"),
        ("nope", Creature::acts_as_nope, "nope"),
        ("drifter", Creature::acts_as_drifter, "drifter"),
        ("webber", Creature::acts_as_webber, "webber"),
        ("trapper", Creature::acts_as_web_shooter, "web_shooter"),
    ];
    for (personality_id, method, module_id) in cases {
        let c = creature(common::load_personality(personality_id));
        assert!(c.has_behaviour_module(module_bit(module_id)), "{personality_id}: {module_id}");
        assert!(method(&c), "{personality_id}");
    }
}

#[test]
fn test_a_balanced_creature_acts_as_no_specialist() {
    let found = personalities();
    let c = creature(found.get("balanced").unwrap().to_value());
    type ActsAs = fn(&Creature) -> bool;
    let methods: [(&str, ActsAs); 7] = [
        ("acts_as_hunter", Creature::acts_as_hunter),
        ("acts_as_jumper", Creature::acts_as_jumper),
        ("acts_as_observer", Creature::acts_as_observer),
        ("acts_as_nope", Creature::acts_as_nope),
        ("acts_as_drifter", Creature::acts_as_drifter),
        ("acts_as_webber", Creature::acts_as_webber),
        ("acts_as_web_shooter", Creature::acts_as_web_shooter),
    ];
    for (name, method) in methods {
        assert!(!method(&c), "{name}");
    }
}
