//! Compares the port against values dumped from the Python code
//! (`tools/dump_fixtures.py` -> `tests/fixtures/state_oracle.json`).

use std::sync::OnceLock;

use dc_content::util::{json_first_difference, python_json_dumps};
use dc_core::{Color, PyRandom};
use dc_state::progression::{self as pr, ProgressionState};
use dc_state::runtime_state as rs;
use dc_state::teams;
use serde_json::{Value, json};

fn oracle() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/state_oracle.json");
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    })
}

fn eq(label: &str, a: &Value, b: &Value) {
    if let Some(d) = json_first_difference(a, b) {
        panic!("{label}: {d}");
    }
}

fn rgb(c: Color) -> Value {
    json!([c.r, c.g, c.b])
}

fn color_of(v: &Value) -> Color {
    let a = v.as_array().unwrap();
    Color::rgb(a[0].as_u64().unwrap() as u8, a[1].as_u64().unwrap() as u8, a[2].as_u64().unwrap() as u8)
}

fn item_json(i: &pr::ArmorItem) -> Value {
    json!({"id": i.id, "name": i.name, "slot": i.slot, "description": i.description, "armor": i.armor,
           "max_hp": i.max_hp, "max_energy": i.max_energy, "damage": i.damage, "speed": i.speed,
           "size_min": i.size_min, "size_max": i.size_max, "set_id": i.set_id, "tier": i.tier})
}

fn effects(e: &[(&str, f64)]) -> Value {
    Value::Object(e.iter().map(|(k, v)| (k.to_string(), json!(v))).collect())
}

#[test]
fn curves_match() {
    let o = oracle();
    for (level, xp) in o["xp"].as_object().unwrap() {
        assert_eq!(pr::xp_to_next_level(level.parse().unwrap()), xp.as_i64().unwrap(), "level {level}");
    }
    for (level, g) in o["growth"].as_object().unwrap() {
        let (s, v) = pr::growth_multipliers(level.parse().unwrap());
        assert_eq!(json!([s, v]), *g, "level {level}");
    }
    for (level, k) in o["level_multiplier"].as_object().unwrap() {
        assert_eq!(pr::level_multiplier(level.parse().unwrap()), k.as_f64().unwrap());
    }
}

#[test]
fn catalogues_match() {
    let o = oracle();
    let items = o["items"].as_object().unwrap();
    assert_eq!(items.len(), pr::ARMOR_CATALOG.len());
    for item in pr::ARMOR_CATALOG.iter() {
        for (level, expected) in items[item.id].as_object().unwrap() {
            assert_eq!(item_json(&pr::item_at_level(item, level.parse().unwrap())), *expected, "{} @ {level}", item.id);
        }
    }
    let abilities: Vec<Value> = pr::ABILITY_TREE
        .iter()
        .map(|n| {
            json!({"id": n.id, "name": n.name, "level_required": n.level_required, "cost": n.cost,
                        "prerequisites": n.prerequisites, "effects": effects(n.effects)})
        })
        .collect();
    assert_eq!(json!(abilities), o["abilities"]);
    for set in pr::ARMOR_SETS.iter() {
        assert_eq!(json!({"pieces": set.pieces(), "effects": effects(set.effects)}), o["sets"][set.id], "{}", set.id);
    }
    for case in o["set_bonus"].as_array().unwrap() {
        let worn: Vec<String> = case["worn"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
        let mut state = ProgressionState { inventory: worn.clone(), ..Default::default() };
        for id in &worn {
            state.equip(id);
        }
        state.item_levels.insert(worn[0].clone(), 3);
        assert_eq!(serde_json::to_value(&state.equipped).unwrap(), case["equipped"]);
        eq("bonus", &serde_json::to_value(pr::set_bonus_effects(&state)).unwrap(), &case["bonus"]);
        let items: Vec<Value> = pr::equipped_items(&state).iter().map(item_json).collect();
        eq("equipped items", &json!(items), &case["items"]);
    }
}

#[test]
fn progression_from_dict_matches() {
    for case in oracle()["progression"].as_array().unwrap() {
        let state = ProgressionState::from_value(&case["input"]);
        assert_eq!(state.to_value(), case["output"], "{}", case["input"]);
        // And it survives a round trip.
        assert_eq!(ProgressionState::from_value(&state.to_value()), state);
    }
}

#[test]
fn team_identity_matches() {
    let o = oracle();
    for (name, d) in o["team_defaults"].as_object().unwrap() {
        assert_eq!(pr::normalize_team_id(name), d["id"].as_str().unwrap(), "{name}");
        assert_eq!(rgb(teams::default_color(name)), d["color"], "{name}");
        assert_eq!(teams::default_name(name), d["name"].as_str().unwrap(), "{name}");
    }
    for case in o["distinct"].as_array().unwrap() {
        let taken: Vec<Color> = case["taken"].as_array().unwrap().iter().map(color_of).collect();
        assert_eq!(rgb(teams::distinct_color(case["name"].as_str().unwrap(), &taken)), case["color"], "{case}");
    }
    for case in o["normalize_teams"].as_array().unwrap() {
        let used: Vec<&str> = case["used"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let raw = if case["raw"].is_null() { None } else { Some(&case["raw"]) };
        let profiles = teams::normalize_teams(raw, &used);
        let got: Vec<Value> = profiles.iter().map(|(k, p)| json!([k, p.name, rgb(p.color)])).collect();
        assert_eq!(json!(got), case["profiles"], "{}", case["raw"]);
    }
    let first = &o["normalize_teams"][0];
    let profiles = teams::normalize_teams::<&str>(Some(&first["raw"]), &[]);
    assert_eq!(teams::teams_payload(&profiles), o["teams_payload"]);
    for case in o["parse_color"].as_array().unwrap() {
        let got = teams::parse_color(Some(&case["input"])).map(rgb).unwrap_or(Value::Null);
        assert_eq!(got, case["output"], "{}", case["input"]);
    }
}

#[test]
fn stances_match() {
    let o = oracle();
    let profiles = teams::normalize_teams::<&str>(None, &["pack_a", "rivals", "hunters", "b", "x"]);
    for case in o["stances"].as_array().unwrap() {
        let stances = pr::normalize_team_stances(Some(&case["raw"]));
        assert_eq!(serde_json::to_value(&stances).unwrap(), case["normalized"], "{}", case["raw"]);
        assert_eq!(serde_json::to_value(teams::minimal_stances(Some(&stances))).unwrap(), case["minimal"]);
        let pairs: Vec<Value> =
            teams::stance_pairs(&profiles, Some(&stances)).into_iter().map(|(a, b, c)| json!([a, b, c])).collect();
        assert_eq!(json!(pairs), case["pairs"]);
    }
    let declared = pr::normalize_team_stances(Some(&o["stances"][1]["raw"]));
    for case in o["team_stance"].as_array().unwrap() {
        let (l, r) = (case["left"].as_str().unwrap(), case["right"].as_str().unwrap());
        assert_eq!(json!(pr::team_stance(l, r, None)), case["none"]);
        assert_eq!(json!(pr::team_stance(l, r, Some(&declared))), case["declared"]);
    }
}

#[test]
fn runtime_rules_match() {
    let o = oracle();
    let mut rng = PyRandom::from_int(1234);
    let ids: Vec<String> = (0..5).map(|_| rs::new_creature_id("Default", Some(&mut rng))).collect();
    assert_eq!(json!(ids), o["new_ids"]);
    for (v, ns) in o["namespaces"].as_object().unwrap() {
        assert_eq!(rs::normalize_namespace(v), ns.as_str().unwrap(), "{v}");
    }
    for (k, d) in o["keys"].as_object().unwrap() {
        assert_eq!(
            json!([rs::normalize_state_key(k), rs::slot_id_from_key(k), rs::member_index_from_key(k)]),
            *d,
            "{k}"
        );
    }
    let migrated = rs::migrate_creatures(Some(&o["migration"]["input"]), 7);
    assert_eq!(Value::Object(migrated.clone()), o["migration"]["output"]);
    assert_eq!(
        migrated.keys().collect::<Vec<_>>(),
        o["migration"]["output"].as_object().unwrap().keys().collect::<Vec<_>>()
    );
    let sm = &o["slot_members"];
    assert_eq!(json!(rs::saved_slot_members(&migrated, "DEFAULT", "slot-0")), sm["slot-0"]);
    assert_eq!(json!(rs::saved_slot_members(&migrated, "colony", "slot-1")), sm["slot-1"]);
    assert_eq!(json!(rs::next_birth_ordinal(&migrated)), sm["next_birth"]);
    let states = json!({"a|1": {"last_seen": 100}, "a|2": {"last_seen": 50}, "a|3": {"last_seen": 49},
                        "a|4": {}, "a|5": {"last_seen": "x"}, "a|6": 3});
    assert_eq!(Value::Object(rs::evict(states.as_object().unwrap(), 100, rs::RETAIN_LAUNCHES)), o["evict"]);
    let scene = json!({"cages": [{"x": 1e-05, "name": "ünï\n"}], "webs": null});
    let payload = rs::build_payload_raw_scene(&migrated, &[json!({"id": "b", "x": 1.5})], 7, Some(&scene));
    eq("payload", &payload, &o["payload"]);
    // The writer is byte-identical to json.dump(indent=2) + newline.
    let mut text = python_json_dumps(&payload, Some(2));
    text.push('\n');
    assert_eq!(text, o["payload_text"].as_str().unwrap());
}
