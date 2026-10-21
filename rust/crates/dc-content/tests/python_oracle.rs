//! Compares the port against values dumped from the Python code
//! (`tools/dump_fixtures.py` -> `tests/fixtures/content_oracle.json`).

mod common;

use std::path::Path;

use common::{assert_json_eq, isolate, oracle, root};
use dc_content::discovery::{discover_models, discover_personalities, validate_model, validate_personality};
use dc_content::personality_profiles::{self as pp, PersonalityRef};
use dc_content::preset_io::{safe_preset_filename, validate_preset};
use dc_content::{body_plans, enemy_kinds, enemy_traits, palettes, skills};
use dc_core::PyRandom;
use serde_json::{Value, json};

fn strip_private(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.retain(|k, _| !k.starts_with('_') || k == "_canonical");
    }
    v
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn opt_strings(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
}

#[test]
fn body_plans_match() {
    let expected = &oracle()["body_plans"];
    for plan in body_plans::body_plans() {
        assert_json_eq(&format!("{} legs", plan.id), &json!(plan.legs_json), &expected[plan.id]["legs"]);
        assert_json_eq(&format!("{} typed legs", plan.id), &json!(plan.legs), &expected[plan.id]["legs"]);
        assert_json_eq(
            &format!("{} gait", plan.id),
            &serde_json::to_value(&plan.gait).unwrap(),
            &expected[plan.id]["gait"],
        );
    }
    assert_eq!(expected.as_object().unwrap().keys().collect::<Vec<_>>(), body_plans::BODY_PLAN_IDS.to_vec());
}

#[test]
fn palettes_match() {
    let o = &oracle()["palettes"];
    let named: Vec<Value> =
        palettes::named_palettes().iter().map(|p| json!({"id": p.id, "name": p.name, "colors": p.colors})).collect();
    assert_json_eq("named", &json!(named), &o["named"]);
    for case in o["from_hue"].as_array().unwrap() {
        let a = &case["args"];
        let got = palettes::palette_from_hue(a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap());
        assert_json_eq(&format!("from_hue {a}"), &json!(got), &case["colors"]);
    }
    for case in o["random"].as_array().unwrap() {
        let mut rng = PyRandom::from_int(case["seed"].as_i64().unwrap() as i128);
        assert_json_eq("random", &json!(palettes::random_palette(&mut rng)), &case["colors"]);
    }
    assert_json_eq("enemy", &json!(palettes::enemy_palette()), &o["enemy"]);
}

#[test]
fn enemy_kinds_match() {
    let o = oracle()["enemy_kinds"].as_array().unwrap();
    assert_eq!(o.len(), enemy_kinds::enemy_kinds().len());
    for (kind, expected) in enemy_kinds::enemy_kinds().iter().zip(o) {
        assert_eq!(kind.id, expected["id"]);
        assert_eq!(kind.model_id, expected["model_id"]);
        assert_eq!(kind.name, expected["name"]);
        assert_eq!(kind.boss, expected["boss"]);
        assert_eq!(kind.tier, expected["tier"].as_i64().unwrap());
        assert_eq!(kind.size_scale, expected["size_scale"].as_f64().unwrap());
        assert_eq!(json!(kind.tags), expected["tags"]);
        assert_json_eq(&format!("{} skins", kind.id), &json!(kind.skins), &expected["skins"]);
        let picks: Vec<_> = [0, 1, 2, 3, 17, 12345].iter().map(|s| enemy_kinds::pick_skin(kind, *s)).collect();
        assert_json_eq(&format!("{} picks", kind.id), &json!(picks), &expected["picks"]);
    }
    for (key, ids) in oracle()["kinds_for_tier"].as_object().unwrap() {
        let (tier, boss) = key.split_once(':').unwrap();
        let got: Vec<&str> =
            enemy_kinds::kinds_for_tier(tier.parse().unwrap(), boss == "True").iter().map(|k| k.id).collect();
        assert_eq!(json!(got), *ids, "{key}");
    }
}

fn traits_json(t: &enemy_traits::EnemyTraits) -> Value {
    let temper = t.temper();
    json!({
        "temperament": t.temperament, "abilities": t.abilities, "speed": t.speed, "health": t.health,
        "bite": t.bite, "shell": t.shell, "squad_role": t.squad_role, "style": t.style, "look": t.look,
        "leash": temper.leash, "notice": temper.notice, "chase_forever": temper.chase_forever, "give_up": temper.give_up,
    })
}

#[test]
fn enemy_traits_match() {
    let o = &oracle()["enemy_traits"];
    for (kind, expected) in o["kinds"].as_object().unwrap() {
        assert_json_eq(kind, &traits_json(enemy_traits::traits_for(Some(kind), "guard")), expected);
    }
    for (role, expected) in o["roles"].as_object().unwrap() {
        assert_json_eq(role, &traits_json(enemy_traits::traits_for(None, role)), expected);
    }
}

#[test]
fn personalities_match() {
    isolate();
    let (found, warnings) = discover_personalities(Some(&root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    let expected = oracle()["personalities"].as_object().unwrap();
    assert_eq!(found.keys().collect::<Vec<_>>(), expected.keys().map(String::as_str).collect::<Vec<_>>());
    for (id, p) in found.iter() {
        let got = strip_private(p.to_value());
        assert_json_eq(id, &got, &expected[id]);
        // Key order of the phase deck matters to the scheduler.
        let order: Vec<&str> = p.phase_scores.as_ref().unwrap().keys().collect();
        let want: Vec<&str> = expected[id]["phase_scores"].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(order, want, "{id} phase order");
    }
    let derived = oracle()["personality_derived"].as_object().unwrap();
    for (id, p) in found.iter() {
        let d = &derived[id];
        let r = Some(PersonalityRef::Def(p));
        assert_eq!(json!(pp::behaviour_modules_for(p)), d["modules"], "{id} modules");
        assert_eq!(pp::personality_gait_style(p), d["gait"], "{id} gait");
        assert_eq!(json!(skills::default_skills_for_personality(r)), d["default_skills"], "{id}");
        assert_eq!(json!(skills::default_ability_ids(r)), d["default_abilities"], "{id}");
        assert_eq!(json!(skills::skills_with_selected_abilities::<&str>(r, Some(&[]))), d["no_abilities"], "{id}");
        assert_eq!(
            json!(skills::skills_with_default_abilities(r, Some(&["shoot_web"]))),
            d["with_job_abilities"],
            "{id}"
        );
        assert_eq!(json!(pp::ability_bundle_ids_for(p)), d["bundles"], "{id}");
    }
    let sel = &oracle()["selectable"];
    assert_eq!(json!(pp::selectable_personality_ids(None, None)), sel["none"]);
    assert_eq!(json!(pp::selectable_personality_ids(Some(&found), Some("hunter"))), sel["hunter"]);
    assert_eq!(json!(pp::selectable_personality_ids(Some(&found), Some("nonsense"))), sel["missing"]);
    assert_eq!(json!(pp::selectable_personality_ids(None, Some("Mystery"))), sel["no_catalog"]);
}

#[test]
fn personality_ids_match() {
    for (id, d) in oracle()["by_id"].as_object().unwrap() {
        let id = id.as_str();
        assert_json_eq(&format!("{id} temperament"), &json!(pp::temperament_for(id)), &d["temperament"]);
        assert_eq!(pp::movement_profile_for(id), d["movement"], "{id}");
        assert_json_eq(&format!("{id} phases"), &json!(pp::phase_scores_for(id)), &d["phase_scores"]);
        assert_eq!(pp::phase_duration_multiplier_for(id), d["duration"].as_f64().unwrap(), "{id}");
        assert_eq!(json!(pp::ability_ids_for(id)), d["abilities"], "{id}");
        assert_eq!(json!(pp::behaviour_modules_for(id)), d["modules"], "{id}");
        assert_eq!(json!(skills::default_skills_for_personality(Some(id.into()))), d["default_skills"], "{id}");
    }
}

#[test]
fn models_match() {
    isolate();
    let (models, warnings) = discover_models(Some(&root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    let expected = oracle()["models"].as_object().unwrap();
    let mut got_ids: Vec<&str> = models.keys().collect();
    let mut want_ids: Vec<&str> = expected.keys().map(String::as_str).collect();
    got_ids.sort();
    want_ids.sort();
    assert_eq!(got_ids, want_ids);
    for (id, model) in models.iter() {
        assert_json_eq(id, &strip_private(model.to_value()), &expected[id]);
    }
}

#[test]
fn skill_summaries_match() {
    for case in oracle()["skill_summaries"].as_array().unwrap() {
        let input = opt_strings(&case["input"]);
        let input = input.as_deref();
        assert_eq!(skills::compact_skill_summary(input), case["skills"], "{input:?}");
        assert_eq!(skills::compact_ability_summary(input), case["abilities"], "{input:?}");
        assert_eq!(skills::normalize_skill_ids(input), strings(&case["normalized"]));
        assert_eq!(skills::normalize_ability_ids(input), strings(&case["normalized_abilities"]));
        assert_eq!(skills::unknown_skill_ids(input), strings(&case["unknown"]));
    }
}

#[test]
fn validation_messages_match() {
    for case in oracle()["model_validation"].as_array().unwrap() {
        let got = validate_model(&case["data"], Path::new("X")).err().unwrap_or_default();
        assert_eq!(got, case["message"].as_str().unwrap(), "{}", case["data"]);
    }
    for case in oracle()["personality_validation"].as_array().unwrap() {
        let got = validate_personality(&case["data"], Path::new("X")).err().unwrap_or_default();
        assert_eq!(got, case["message"].as_str().unwrap(), "{}", case["data"]);
    }
    for case in oracle()["preset_validation"].as_array().unwrap() {
        let got = validate_preset(&case["data"]).err().unwrap_or_default();
        assert_eq!(got, case["message"].as_str().unwrap(), "{}", case["data"]);
    }
    for (name, file) in oracle()["safe_filenames"].as_object().unwrap() {
        assert_eq!(safe_preset_filename(name), file.as_str().unwrap(), "{name}");
    }
}
