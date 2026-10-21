//! Ports of the Python tests covering `desktop_bug.state` (the parts that do
//! not need a `Creature` or `CreatureManager`).
//!
//! Sources: test_progression.py, test_state_migration.py,
//! test_scene_persistence.py (state parts), test_reset_progress.py,
//! test_team_relations.py, test_team_identity.py (state parts).

use std::path::PathBuf;

use dc_core::PyRandom;
use dc_state::progression::{
    self as pr, MAX_LEVEL, ProgressionState, normalize_team_id, normalize_team_id_value, normalize_team_stances,
    relation_between, team_stance, xp_to_next_level,
};
use dc_state::runtime_state::{
    self as rs, RETAIN_LAUNCHES, RuntimeState, STATE_SCHEMA_VERSION, SavedCreature, entry_rank, evict, load_payload,
    migrate_creatures, next_launch, normalize_namespace, normalize_state_key, reset_saved_progress,
};
use dc_state::teams;
use serde_json::{Value, json};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dc-state-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn entry(level: i64, name: &str, last_seen: Option<i64>) -> Value {
    let mut data = json!({"name": name, "model": "tarantula", "personality": "mellow",
                          "progression": {"level": level, "xp": 0, "total_xp": level * 100}});
    if let Some(l) = last_seen {
        data["last_seen"] = json!(l);
    }
    data
}

fn team(id: &str) -> ProgressionState {
    ProgressionState { team_id: id.into(), ..Default::default() }
}

// ---------------------------------------------------------------------------
// test_progression.py (state parts)
// ---------------------------------------------------------------------------

#[test]
fn a_new_spider_starts_at_the_bottom() {
    let s = ProgressionState::default();
    assert_eq!((s.level, s.xp, s.total_xp, s.skill_points), (1, 0, 0, 0));
    assert_eq!(s.inventory, vec!["silk_carapace", "leg_guard_set"]);
    assert_eq!(s.team_id, "neutral");
}

#[test]
fn the_xp_curve_starts_at_100_and_rises() {
    assert_eq!(xp_to_next_level(1), 115);
    assert!((1..MAX_LEVEL).all(|l| xp_to_next_level(l + 1) > xp_to_next_level(l)));
    assert_eq!(xp_to_next_level(MAX_LEVEL + 5), xp_to_next_level(MAX_LEVEL));
    assert_eq!(xp_to_next_level(0), xp_to_next_level(1));
    let (size, speed) = pr::growth_multipliers(5);
    assert!(size > 1.0 && speed > size);
}

#[test]
fn talents_need_level_points_and_prerequisites() {
    let mut s = ProgressionState { level: 5, skill_points: 1, ..Default::default() };
    assert!(s.can_unlock("vitality"));
    assert!(!s.can_unlock("carapace_harden"), "needs vitality first");
    s.unlocked_abilities.push("vitality".into());
    assert!(!s.can_unlock("vitality"), "already unlocked");
    assert!(s.can_unlock("carapace_harden"));
    s.skill_points = 0;
    assert!(!s.can_unlock("carapace_harden"));
    assert!(!s.can_unlock("nonsense"));
}

#[test]
fn equipment_needs_ownership_and_fills_its_slot() {
    let mut s = ProgressionState::default();
    assert!(!s.equip("fluffy_mantle"), "not owned yet");
    assert!(s.add_item("fluffy_mantle"));
    assert!(!s.add_item("fluffy_mantle"), "already owned");
    assert!(!s.add_item("nonsense"));
    assert!(s.equip("fluffy_mantle"));
    assert_eq!(s.equipped.get("abdomen").map(String::as_str), Some("fluffy_mantle"));
    let worn = pr::equipped_items(&s);
    assert!(worn[0].armor > 0.0);
    assert!(s.unequip("abdomen"));
    assert!(!s.unequip("abdomen"));
}

#[test]
fn item_levels_scale_stats() {
    let item = pr::armor_by_id("sun_aegis").unwrap();
    assert_eq!(pr::item_at_level(item, 1), *item);
    let top = pr::item_at_level(item, 5);
    assert!((top.armor - item.armor * 1.8).abs() < 1e-12);
    assert_eq!(pr::item_at_level(item, 99), top);
}

#[test]
fn a_full_set_grants_its_bonus() {
    let set = pr::armor_set("frost").unwrap();
    let mut s =
        ProgressionState { inventory: set.pieces().iter().map(|p| p.to_string()).collect(), ..Default::default() };
    for piece in set.pieces() {
        assert!(pr::set_bonus_effects(&s).is_empty());
        s.equip(piece);
    }
    assert_eq!(pr::set_pieces_worn(&s, "frost"), 5);
    assert_eq!(pr::set_bonus_effects(&s).get("speed"), Some(&0.06));
}

#[test]
fn a_team_decides_the_default_relation() {
    let me = team("pack");
    assert_eq!(relation_between(&me, &team("pack"), Some("smoke:1"), None), "friend");
    assert_eq!(relation_between(&me, &team("pack_b"), Some("smoke:1"), None), "neutral");
    assert_eq!(relation_between(&me, &team("rivals"), Some("smoke:1"), None), "foe");
    for over in ["friend", "foe"] {
        let mut me = team("pack");
        me.relation_overrides.insert("smoke:1", over.to_string());
        assert_eq!(relation_between(&me, &team("rivals"), Some("smoke:1"), None), over);
    }
}

#[test]
fn progression_survives_a_round_trip() {
    let mut s =
        ProgressionState { level: 7, xp: 30, total_xp: 900, skill_points: 2, pin_level: true, ..Default::default() };
    s.unlocked_abilities.push("vitality".into());
    s.add_item("sun_crown");
    s.equip("sun_crown");
    s.item_levels.insert("sun_crown", 3);
    s.relation_overrides.insert("x|1", "foe".to_string());
    s.team_id = "porch_guard".into();
    let back = ProgressionState::from_value(&s.to_value());
    assert_eq!(back, s);
    assert_eq!(back.to_value(), s.to_value());
    let via_serde: ProgressionState = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(via_serde, s);
}

// ---------------------------------------------------------------------------
// test_team_relations.py
// ---------------------------------------------------------------------------

#[test]
fn team_ids_fold() {
    assert_eq!(normalize_team_id("Rivals"), "rivals");
    assert_eq!(normalize_team_id("  PACK_A "), "pack_a");
    assert_eq!(normalize_team_id(""), "neutral");
    assert_eq!(normalize_team_id_value(&Value::Null), "neutral");
    assert_eq!(normalize_team_id("Porch Guard"), "porch_guard");
}

#[test]
fn default_stances() {
    assert_eq!(team_stance("pack_a", "pack_a", None).as_deref(), Some("friend"));
    assert_eq!(team_stance("pack_a", "pack_b", None), None);
    assert_eq!(team_stance("pack_a", "rivals", None).as_deref(), Some("foe"));
    assert_eq!(team_stance("rivals", "pack_a", None).as_deref(), Some("foe"));
    assert_eq!(team_stance("rivals", "rivals", None).as_deref(), Some("friend"));
    for (l, r) in [("neutral", "rivals"), ("rivals", "neutral"), ("neutral", "neutral")] {
        assert_eq!(team_stance(l, r, None), None);
    }
}

#[test]
fn declared_stances() {
    let stances = normalize_team_stances(Some(&json!({"pack_a": {"pack_b": "foe"}})));
    assert_eq!(stances.get("pack_a").unwrap().get("pack_b").unwrap(), "foe");
    assert_eq!(stances.get("pack_b").unwrap().get("pack_a").unwrap(), "foe");
    assert_eq!(team_stance("pack_b", "pack_a", Some(&stances)).as_deref(), Some("foe"));
    let friendly = normalize_team_stances(Some(&json!({"rivals": {"pack_a": "friend"}})));
    assert_eq!(team_stance("pack_a", "rivals", Some(&friendly)).as_deref(), Some("friend"));
    let mixed = normalize_team_stances(Some(&json!({" Pack_A ": {"RIVALS": "foe"}})));
    assert_eq!(team_stance("pack_a", "rivals", Some(&mixed)).as_deref(), Some("foe"));
    assert!(normalize_team_stances(None).is_empty());
    assert!(normalize_team_stances(Some(&json!({"pack_a": "nonsense"}))).is_empty());
    assert!(normalize_team_stances(Some(&json!({"pack_a": {"pack_b": "enemy"}}))).is_empty());
}

#[test]
fn a_pair_override_wins() {
    let mut left = team("pack_a");
    let right = team("rivals");
    let stances = normalize_team_stances(Some(&json!({"pack_a": {"rivals": "foe"}})));
    assert_eq!(relation_between(&left, &right, Some("right-key"), Some(&stances)), "foe");
    left.relation_overrides.insert("right-key", "friend".to_string());
    assert_eq!(relation_between(&left, &right, Some("right-key"), Some(&stances)), "friend");
    let plain = team("pack_b");
    assert_eq!(relation_between(&team("pack_a"), &plain, None, None), "neutral");
    assert_eq!(relation_between(&team("pack_b"), &plain, None, None), "friend");
}

// ---------------------------------------------------------------------------
// test_team_identity.py (state parts)
// ---------------------------------------------------------------------------

#[test]
fn profiles_from_a_preset_block() {
    let profiles = teams::normalize_teams::<&str>(
        Some(
            &json!({"porch_guard": {"name": "Porch guard", "color": "#3fa9d9"}, "shed": "Shed crew", "neutral": {"name": "Nope"}}),
        ),
        &[],
    );
    assert_eq!(profiles.keys().collect::<Vec<_>>(), ["porch_guard", "shed"]);
    assert_eq!(profiles.get("porch_guard").unwrap().color, dc_core::Color::rgb(63, 169, 217));
    assert_eq!(profiles.get("shed").unwrap().name, "Shed crew");
    assert_eq!(profiles.get("shed").unwrap().color, teams::default_color("shed"));
    let profiles = teams::normalize_teams(None, &["pack_a", "neutral", "attic_watch"]);
    assert_eq!(profiles.get("pack_a").unwrap().name, "Pack A");
    assert_eq!(profiles.get("attic_watch").unwrap().name, "Attic Watch");
}

#[test]
fn ids_are_case_folded_and_colours_differ() {
    let profiles =
        teams::normalize_teams(Some(&json!({"Porch Guard": {"name": "Porch guard"}})), &["PORCH_guard", "porch guard"]);
    assert_eq!(profiles.keys().collect::<Vec<_>>(), ["porch_guard"]);
    assert_eq!(teams::team_label("RIVALS", None), teams::team_label("rivals", None));
    assert_eq!(teams::color_to_hex(dc_core::Color::rgb(63, 169, 217)), "#3fa9d9");
    let invented =
        ["attic_watch", "porch_guard", "shed_crew", "garden_patrol", "roof", "hearth", "wanderers", "night_shift"];
    let profiles = teams::normalize_teams(None, &invented);
    for (i, l) in invented.iter().enumerate() {
        for r in &invented[i + 1..] {
            let gap = teams::color_distance(profiles.get(l).unwrap().color, profiles.get(r).unwrap().color);
            assert!(gap >= teams::MIN_COLOR_SEPARATION, "{l} and {r}: {gap}");
        }
    }
    let mut reversed = invented;
    reversed.reverse();
    assert_eq!(teams::normalize_teams(None, &reversed).get("roof").unwrap().color, profiles.get("roof").unwrap().color);
    let chosen = teams::normalize_teams::<&str>(
        Some(&json!({"a_team": {"color": "#3fa9d9"}, "b_team": {"color": "#3fa9d9"}})),
        &[],
    );
    assert_eq!(chosen.get("a_team").unwrap().color, chosen.get("b_team").unwrap().color);
}

#[test]
fn stances_round_trip_minimally() {
    let declared = normalize_team_stances(Some(&json!({"pack_a": {"rivals": "foe"}})));
    assert_eq!(
        serde_json::to_value(&declared).unwrap(),
        json!({"pack_a": {"rivals": "foe"}, "rivals": {"pack_a": "foe"}})
    );
    let minimal = teams::minimal_stances(Some(&declared));
    assert_eq!(serde_json::to_value(&minimal).unwrap(), json!({"pack_a": {"rivals": "foe"}}));
    assert_eq!(pr::normalize_team_stances_typed(&minimal), declared);
    let peace = normalize_team_stances(Some(&json!({"pack_a": {"rivals": "neutral"}})));
    assert_eq!(
        serde_json::to_value(teams::minimal_stances(Some(&peace))).unwrap(),
        json!({"pack_a": {"rivals": "neutral"}})
    );
    assert_eq!(team_stance("pack_a", "rivals", Some(&peace)).as_deref(), Some("neutral"));
    assert_eq!(team_stance("pack_a", "rivals", Some(&Default::default())).as_deref(), Some("foe"));
}

#[test]
fn the_interface_says_what_hostility_does() {
    let note = teams::HOSTILITY_NOTE.to_lowercase();
    assert!(!note.contains("no combat") && !note.contains("nothing takes damage"));
    assert!(note.contains("fight") && note.contains("guard") && note.contains("intercept"));
    assert!(note.contains("dies") || note.contains("death"));
    let sentence = teams::describe_stance("pack_a", "rivals", "foe", None);
    assert!(sentence.to_lowercase().contains("damage") && sentence.to_lowercase().contains("dies"));
    assert!(sentence.contains("Pack A") && sentence.contains("Rivals"));
    assert!(teams::describe_stance("a", "b", "friend", None).ends_with("company."));
}

// ---------------------------------------------------------------------------
// test_state_migration.py
// ---------------------------------------------------------------------------

#[test]
fn keys() {
    assert_eq!(normalize_namespace("Default"), "default");
    assert_eq!(normalize_namespace("  COLONY  "), "colony");
    assert_eq!(normalize_namespace(""), "default");
    assert_eq!(normalize_state_key("Default|slot-0:0").as_deref(), Some("default|slot-0:0"));
    assert_eq!(normalize_state_key("Default|slot-3FE0:1").as_deref(), Some("default|slot-3FE0:1"));
    assert_eq!(normalize_state_key("runtime:0"), None);
    assert_eq!(normalize_state_key(""), None);
    assert_eq!(normalize_state_key("default|"), None);
    assert!(entry_rank(&entry(5, "", None)) > entry_rank(&entry(2, "", None)));
    assert_eq!(entry_rank(&json!({})), (1, 0));
    assert_eq!(entry_rank(&json!("nonsense")), (0, 0));
    assert_eq!(entry_rank(&json!({"level": 4, "total_xp": 400})), (4, 400));
}

#[test]
fn migration() {
    let raw = json!({
        "Default|slot-0:0": entry(5, "Webster", None),
        "default|slot-0:0": entry(2, "", None),
        "runtime:0": entry(9, "orphan", None),
        "colony|slot-1:0": entry(3, "", None),
    });
    let migrated = migrate_creatures(Some(&raw), 7);
    let mut keys: Vec<&String> = migrated.keys().collect();
    keys.sort();
    assert_eq!(keys, ["colony|slot-1:0", "default|slot-0:0"]);
    assert_eq!(migrated["default|slot-0:0"]["progression"]["level"], 5);
    assert_eq!(migrated["default|slot-0:0"]["name"], "Webster");
    assert!(migrated.values().all(|e| e["last_seen"] == 7));
    assert!(raw["Default|slot-0:0"].get("last_seen").is_none(), "the source was mutated");
    let flipped = migrate_creatures(
        Some(&json!({"default|slot-0:0": entry(2, "", None), "Default|slot-0:0": entry(5, "Webster", None)})),
        7,
    );
    assert_eq!(flipped["default|slot-0:0"]["progression"]["level"], 5);
    assert_eq!(migrate_creatures(Some(&Value::Object(migrated.clone())), 8), migrated, "not idempotent");
    let kept = migrate_creatures(Some(&json!({"default|a:0": entry(1, "", Some(3))})), 9);
    assert_eq!(kept["default|a:0"]["last_seen"], 3);
    assert!(migrate_creatures(None, 1).is_empty());
    assert!(migrate_creatures(Some(&json!({"default|a:0": "nonsense"})), 1).is_empty());
    // Version-2 keys give up their slot and birth order.
    assert_eq!(migrated["colony|slot-1:0"]["slot_id"], "slot-1");
    assert_eq!(migrated["colony|slot-1:0"]["born"], 0);
}

#[test]
fn launch_and_eviction() {
    assert_eq!(next_launch(None), 1);
    assert_eq!(next_launch(Some(&json!({}))), 1);
    assert_eq!(next_launch(Some(&json!({"launch": 4}))), 5);
    assert_eq!(next_launch(Some(&json!({"launch": "nonsense"}))), 1);
    assert_eq!(next_launch(Some(&json!({"launch": -3}))), 1);
    let states = json!({
        "default|fresh:0": entry(1, "", Some(100)),
        "default|edge:0": entry(1, "", Some(100 - RETAIN_LAUNCHES)),
        "default|stale:0": entry(1, "", Some(100 - RETAIN_LAUNCHES - 1)),
    });
    let kept = evict(states.as_object().unwrap(), 100, RETAIN_LAUNCHES);
    assert!(kept.contains_key("default|fresh:0"));
    assert!(kept.contains_key("default|edge:0"), "eviction boundary is off by one");
    assert!(!kept.contains_key("default|stale:0"));
    assert!(!evict(json!({"default|x:0": entry(1, "", None)}).as_object().unwrap(), 100, RETAIN_LAUNCHES).is_empty());
    let (states, bases) =
        load_payload(Some(&json!({"creatures": {"A|s:0": entry(1, "", None)}, "bases": [{"id": "b"}]})), 3);
    assert_eq!(states.keys().collect::<Vec<_>>(), ["a|s:0"]);
    assert_eq!(bases, vec![json!({"id": "b"})]);
    assert_eq!(load_payload(None, 1), (Default::default(), vec![]));
    assert_eq!(
        load_payload(Some(&json!({"creatures": "nonsense", "bases": "nonsense"})), 1),
        (Default::default(), vec![])
    );
}

/// The file half of `test_manager_round_trip`: a schema-1 file migrates,
/// saves as schema 3, and reloads on the next launch unchanged.
#[test]
fn a_state_file_round_trips_across_launches() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("creatures.json");
    std::fs::write(
        &path,
        json!({"schema_version": 1, "creatures": {"Default|slot-0:0": entry(5, "Webster", None),
               "default|slot-0:0": entry(2, "", None), "runtime:0": entry(9, "orphan", None)}, "bases": []})
        .to_string(),
    )
    .unwrap();
    let mut state = RuntimeState::load(&path);
    assert_eq!(state.creatures.keys().collect::<Vec<_>>(), ["default|slot-0:0"]);
    assert_eq!(state.launch, 1);
    state.save(&path).unwrap();
    assert!(!path.with_extension("tmp").exists());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["schema_version"], STATE_SCHEMA_VERSION);
    assert_eq!(saved["launch"], 1);
    assert_eq!(saved["creatures"]["default|slot-0:0"]["progression"]["level"], 5);
    assert_eq!(saved["scene"], json!({"cages": [], "webs": [], "nests": []}));
    let again = RuntimeState::load(&path);
    assert_eq!(again.launch, 2);
    assert_eq!(again.creatures, state.creatures);
    let typed = SavedCreature::from_value(&again.creatures["default|slot-0:0"]).unwrap();
    assert_eq!((typed.name.as_str(), typed.progression.level), ("Webster", 5));
}

#[test]
fn a_saved_creature_keeps_its_birth_order() {
    let mut state =
        RuntimeState::from_payload(Some(&json!({"launch": 3, "creatures": {"d|old": {"born": 4, "slot_id": "s"}}})));
    assert_eq!(state.launch, 4);
    let mut ordinal = rs::next_birth_ordinal(&state.creatures);
    assert_eq!(ordinal, 5);
    let spider = |name: &str| SavedCreature {
        name: name.into(),
        model: Some("tarantula".into()),
        personality: Some("mellow".into()),
        progression: ProgressionState::default(),
        slot_id: "s".into(),
        born: 0,
        last_seen: None,
        extra: Default::default(),
    };
    state.record("d|old", &spider("Old"), &mut ordinal);
    state.record("d|new", &spider("New"), &mut ordinal);
    assert_eq!(state.creatures["d|old"]["born"], 4);
    assert_eq!(state.creatures["d|new"]["born"], 5);
    assert_eq!(state.creatures["d|new"]["last_seen"], 4);
    assert_eq!(rs::saved_slot_members(&state.creatures, "D", "s"), ["d|old", "d|new"]);
}

// ---------------------------------------------------------------------------
// test_scene_persistence.py (state parts)
// ---------------------------------------------------------------------------

#[test]
fn slot_members_are_reused_oldest_first() {
    let states = migrate_creatures(
        Some(&json!({
            "scene|slot-0:0": {"name": "Elder", "last_seen": 4, "progression": {"level": 9, "total_xp": 900}},
            "scene|slot-0:1": {"name": "Younger", "last_seen": 4, "progression": {"level": 3, "total_xp": 120}},
        })),
        5,
    );
    let mut rng = PyRandom::from_int(3);
    let grown = rs::slot_member_ids(&states, "scene", "slot-0", 4, || rs::new_creature_id("scene", Some(&mut rng)));
    assert_eq!(&grown[..2], ["scene|slot-0:0", "scene|slot-0:1"]);
    assert_eq!(grown.len(), 4);
    assert!(grown[2].starts_with("scene|") && grown[2].len() == "scene|".len() + 12);
    let shrunk = rs::slot_member_ids(&states, "scene", "slot-0", 1, || unreachable!());
    assert_eq!(shrunk, ["scene|slot-0:0"]);
}

#[test]
fn a_corrupt_scene_is_handed_back_for_its_owner_to_judge() {
    let data = json!({"schema_version": 3, "launch": 2, "creatures": {}, "bases": [],
        "scene": {"cages": [{"x": "nonsense"}, null], "webs": "not a list", "nests": [{"x": 100.0, "y": 120.0}]}});
    let scene = rs::load_scene(Some(&data));
    assert_eq!(scene.cages, vec![json!({"x": "nonsense"}), Value::Null]);
    assert!(scene.webs.is_empty());
    assert_eq!(scene.nests.len(), 1);
    let empty = rs::load_scene(Some(&json!({"schema_version": 2})));
    assert!(empty.is_empty(), "a pre-scene file must read as an empty desktop");
}

// ---------------------------------------------------------------------------
// test_reset_progress.py
// ---------------------------------------------------------------------------

#[test]
fn clearing_the_file_keeps_the_scene() {
    let dir = temp_dir("reset");
    let path = dir.join("creatures.json");
    let data = json!({"schema_version": 3, "launch": 6,
        "creatures": {"d|a": entry(9, "Old", Some(6))}, "bases": [{"id": "b"}],
        "scene": {"cages": [{"x": 10, "y": 20, "w": 200, "h": 150}], "webs": [], "nests": []}});
    std::fs::write(&path, data.to_string()).unwrap();
    assert!(reset_saved_progress(&path));
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after["creatures"], json!({}));
    assert_eq!(after["bases"], json!([]));
    assert_eq!(after["scene"]["cages"], json!([{"x": 10, "y": 20, "w": 200, "h": 150}]));
    assert_eq!(after["launch"], data["launch"]);
    assert!(reset_saved_progress(&temp_dir("reset-none").join("creatures.json")), "no file is nothing to reset");
}
