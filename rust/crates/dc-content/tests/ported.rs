//! Ports of the Python tests covering `desktop_bug.content`, plus checks that
//! every shipped data file loads and round-trips.
//!
//! Sources: test_body_plans.py, test_personality_catalog.py,
//! test_model_variations.py, test_behaviour_modules.py (data parts),
//! test_trapping_is_reachable.py, test_enemy_kinds.py (data parts),
//! test_preset_path.py, test_preset_save_location.py, test_state_location.py.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{assert_json_eq, isolate, root};
use dc_content::body_plans::{self, BODY_PLAN_IDS, body_plan_gait, body_plan_legs, resolve_body_plan};
use dc_content::discovery::{
    self, StateLocation, discover_models, discover_personalities, discover_presets, is_shipped_preset,
    resolve_preset_path, run_data_checks, shipped_presets_dirs, user_presets_dir,
};
use dc_content::personality_profiles::{
    self as pp, BEHAVIOUR_MODULE_IDS, BEHAVIOUR_PHASE_IDS, COMPACT_TEMPERAMENT_IDS, TEMPERAMENT_TRAIT_IDS,
    behaviour_modules_for, is_movement_profile,
};
use dc_content::preset_io::{load_preset, load_preset_value, save_preset_value};
use dc_content::skills::{
    COMMON_ABILITY_IDS, COMMON_BEHAVIOUR_IDS, ability_skill_ids, default_ability_ids, default_skills_for_personality,
    skills_with_selected_abilities,
};
use dc_content::{ModelDef, Personality, Preset, enemy_kinds, jobs};
use serde_json::{Value, json};

fn raw_models() -> Vec<(PathBuf, Value)> {
    let mut out: Vec<(PathBuf, Value)> = std::fs::read_dir(root().join("models"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("model.json"))
        .filter(|p| p.is_file())
        .map(|p| {
            let v = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            (p, v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn json_files(dir: &str) -> Vec<(PathBuf, Value)> {
    let mut out: Vec<(PathBuf, Value)> = std::fs::read_dir(root().join(dir))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            let v = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            (p, v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn personality(extra: Value) -> Personality {
    let mut base = json!({"id": "custom", "display_name": "Custom", "speed_multiplier": 1,
                          "reaction_radius": 1, "boldness": 1, "wander_frequency": 1});
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    Personality::from_value(base).unwrap()
}

// ---------------------------------------------------------------------------
// Every shipped file
// ---------------------------------------------------------------------------

#[test]
fn every_model_file_loads_typed_and_round_trips() {
    let models = raw_models();
    assert_eq!(models.len(), 62);
    for (path, raw) in models {
        let model = ModelDef::from_value(raw.clone()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_json_eq(&path.display().to_string(), &model.to_value(), &raw);
        for key in ["id", "display_name", "base_size", "default_personality", "colors", "body_plan"] {
            assert!(raw.get(key).is_some(), "{} lacks {key}", path.display());
        }
    }
}

#[test]
fn every_personality_file_loads_typed_and_round_trips() {
    let files = json_files("personalities");
    assert_eq!(files.len(), 21);
    for (path, raw) in files {
        let p = Personality::from_value(raw.clone()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_json_eq(&path.display().to_string(), &p.to_value(), &raw);
    }
}

#[test]
fn every_preset_file_loads_typed_and_round_trips() {
    let files = json_files("presets");
    assert_eq!(files.len(), 5);
    for (path, raw) in files {
        let preset = load_preset(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_json_eq(&path.display().to_string(), &preset.to_value(), &raw);
        assert!(!preset.slots.is_empty());
    }
}

#[test]
fn unknown_keys_survive_a_round_trip() {
    let raw = json!({"id": "m", "display_name": "M", "base_size": 20, "default_personality": "mellow",
        "colors": {"body": [1, 2, 3], "glow": [4, 5, 6, 7]}, "future_key": {"x": 1},
        "appearance": {"brand_new": true, "leg_chain": {"enabled": true, "mystery": [1]}, "spider_gait": {"new_knob": 2}}});
    let model = ModelDef::from_value(raw.clone()).unwrap();
    // Known numeric fields are f64, so `20` comes back as `20.0`; equal under Python `==`.
    assert_json_eq("model", &model.to_value(), &raw);
    let preset = json!({"name": "A", "slots": [{"model": "m", "personality": "p", "count": 1, "x": 1}],
        "settings": {"new_setting": "y", "flies": {"buzz": true}, "teams": {"a": "Ants", "b": {"name": "B", "z": 0}}},
        "author": "me"});
    assert_eq!(Preset::from_value(preset.clone()).unwrap().to_value(), preset);
}

#[test]
fn the_data_checks_pass() {
    assert_eq!(run_data_checks(&root()), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// test_body_plans.py
// ---------------------------------------------------------------------------

const KEEPS_OWN_RIG: [&str; 5] =
    ["plush_snow_hybrid_2", "knuckle_skitter_stalker", "chosen_one", "regal_fluff", "regal_scout"];

#[test]
fn every_model_names_a_known_plan() {
    for (path, data) in raw_models() {
        let plan = data["body_plan"].as_str().unwrap();
        assert!(BODY_PLAN_IDS.contains(&plan), "{}: {plan}", path.display());
    }
}

#[test]
fn almost_no_model_still_carries_its_own_rig() {
    let models = raw_models();
    let own: BTreeSet<String> = models
        .iter()
        .filter(|(_, d)| d.get("legs").is_some_and(|l| l.as_array().is_some_and(|a| !a.is_empty())))
        .map(|(_, d)| d["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(own, KEEPS_OWN_RIG.iter().map(|s| s.to_string()).collect());
    let rigs: BTreeSet<String> = models.iter().filter_map(|(_, d)| d.get("legs")).map(|l| l.to_string()).collect();
    assert!(BODY_PLAN_IDS.len() + rigs.len() <= 10);
}

#[test]
fn a_model_that_states_a_rig_or_gait_keeps_it() {
    let custom = json!([{"name": "only", "side": "left", "gait_group": 0, "attach_angle": -11,
                         "rest_angle": -11, "reach": 2.0, "upper_len": 1.0, "lower_len": 1.0}]);
    let mut data = json!({"body_plan": "bug", "legs": custom.clone()});
    resolve_body_plan(&mut data);
    assert_eq!(data["legs"], custom);
    let stated = json!({"enabled": true, "profile": "mine", "cycle_hz": 1.11});
    let mut data = json!({"body_plan": "bug", "appearance": {"spider_gait": stated.clone()}});
    resolve_body_plan(&mut data);
    assert_eq!(data["appearance"]["spider_gait"], stated);
}

#[test]
fn each_plan_walks_differently() {
    let cadences: Vec<f64> = BODY_PLAN_IDS.iter().map(|p| body_plan_gait(p).unwrap().cycle_hz.unwrap()).collect();
    assert!(BODY_PLAN_IDS.iter().all(|p| body_plan_gait(p).unwrap().enabled == Some(true)));
    let unique: BTreeSet<u64> = cadences.iter().map(|c| c.to_bits()).collect();
    assert_eq!(unique.len(), cadences.len());
    let hz = |p: &str| body_plan_gait(p).unwrap().cycle_hz.unwrap();
    assert!(hz("tarantula") < hz("bug") && hz("bug") < hz("jumper"));
}

#[test]
fn a_plan_hands_out_copies() {
    let mut first = body_plan_legs("bug");
    first[0].reach = 99.0;
    assert_ne!(body_plan_legs("bug")[0].reach, 99.0);
    assert_ne!(body_plans::body_plan("bug").unwrap().legs[0].reach, 99.0);
}

#[test]
fn an_unknown_plan_is_left_alone() {
    let mut data = json!({"body_plan": "nonsense", "legs": [{"name": "a"}]});
    resolve_body_plan(&mut data);
    assert_eq!(data["legs"], json!([{"name": "a"}]));
    assert!(body_plan_legs("nonsense").is_empty());
    assert!(body_plan_gait("nonsense").is_none());
}

#[test]
fn every_shipped_model_loads_with_eight_legs() {
    isolate();
    let (models, warnings) = discover_models(Some(&root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(models.len(), 51 + enemy_kinds::enemy_kinds().len());
    for (id, model) in models.iter() {
        assert_eq!(model.legs.len(), 8, "{id}");
        assert!(model.spider_gait().is_some(), "{id} has no gait after resolution");
        assert!(model.path.is_some() && model.folder.is_some());
    }
}

#[test]
fn the_classification_metric_is_scale_free() {
    isolate();
    let (models, _) = discover_models(Some(&root()));
    let mut spreads: Vec<Vec<f64>> = vec![Vec::new(); 10];
    for model in models.values() {
        for leg in &model.legs {
            let values = [
                leg.attach_angle,
                leg.rest_angle,
                leg.reach,
                leg.upper_len,
                leg.lower_len,
                leg.attach_forward.unwrap_or(0.0),
                leg.attach_side.unwrap_or(0.0),
                leg.rest_forward.unwrap_or(0.0),
                leg.rest_side.unwrap_or(0.0),
                leg.coxa_len.unwrap_or(0.0),
            ];
            for (i, v) in values.iter().enumerate() {
                spreads[i].push(*v);
            }
        }
    }
    let sd: Vec<f64> = spreads
        .iter()
        .map(|v| {
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        })
        .collect();
    let widest = sd.iter().cloned().fold(0.0, f64::max);
    let narrowest = sd.iter().cloned().filter(|v| *v > 0.0).fold(f64::INFINITY, f64::min);
    assert!(widest / narrowest > 100.0, "{sd:?}");
}

// ---------------------------------------------------------------------------
// test_model_variations.py
// ---------------------------------------------------------------------------

#[test]
fn the_variations_differ() {
    isolate();
    let (models, warnings) = discover_models(Some(&root()));
    assert!(warnings.is_empty());
    for (id, fluffy) in [
        ("mini_marble", false),
        ("velvet_cloud", true),
        ("giant_copper", true),
        ("sunset_fuzzball", true),
        ("blue_jewel", false),
    ] {
        let model = models.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(model.render_mode.as_deref(), Some("procedural"), "{id}");
        assert_eq!(model.legs.len(), 8);
        let a = model.appearance();
        assert_eq!(a.segmented_legs, Some(true), "{id}");
        assert_eq!(a.leg_connections.as_ref().and_then(|c| c.enabled), Some(true), "{id}");
        let fluffiness = a.fluffiness.unwrap_or(0.0);
        if fluffy { assert!(fluffiness >= 0.75, "{id}") } else { assert!(fluffiness <= 0.40, "{id}") }
    }
    let size = |id: &str| models.get(id).unwrap().base_size;
    assert!(size("mini_marble") < size("blue_jewel") && size("blue_jewel") < size("giant_copper"));
}

// ---------------------------------------------------------------------------
// test_personality_catalog.py / test_behaviour_modules.py
// ---------------------------------------------------------------------------

fn personalities() -> dc_content::OrderedMap<Personality> {
    isolate();
    let (found, warnings) = discover_personalities(Some(&root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    found
}

#[test]
fn the_catalog_composes() {
    let found = personalities();
    assert!(found.len() >= 21);
    assert!(COMPACT_TEMPERAMENT_IDS.iter().all(|id| found.contains_key(id)));
    let abilities: BTreeSet<&str> = ability_skill_ids().into_iter().collect();
    assert_eq!(abilities, ["drift", "weave_web", "web_walk", "shoot_web", "wall_web"].into_iter().collect());
    assert_eq!(COMMON_ABILITY_IDS, ["web_walk"]);
    assert!(!COMMON_BEHAVIOUR_IDS.contains(&"web_walk"));
    for (path, raw) in json_files("personalities") {
        assert!(raw.get("skills").is_none(), "{}", path.display());
        let composed = found.get(raw["id"].as_str().unwrap()).unwrap();
        assert!(is_movement_profile(composed.movement_profile.as_deref().unwrap()));
        let scores = composed.phase_scores.as_ref().unwrap();
        assert_eq!(scores.keys().collect::<Vec<_>>(), BEHAVIOUR_PHASE_IDS);
        assert!(scores.values().all(|v| (0.0..=10.0).contains(&v.as_f64().unwrap())));
    }
    for id in COMPACT_TEMPERAMENT_IDS {
        let t = found.get(id).unwrap().temperament.unwrap();
        assert_eq!(t.iter().into_iter().map(|(k, _)| k).collect::<Vec<_>>(), TEMPERAMENT_TRAIT_IDS);
    }
}

#[test]
fn a_hunter_chases_and_nothing_else() {
    let found = personalities();
    let scores = pp::phase_scores_for(found.get("hunter").unwrap());
    assert_eq!(scores.get("chase"), Some(&10.0));
    assert!(*scores.get("prepare_jump_attack").unwrap() >= 7.0);
    for phase in ["jump", "roll", "zoomies", "cuddle", "social_play"] {
        assert_eq!(scores.get(phase), Some(&0.0), "{phase}");
    }
    let hunter = found.get("hunter").unwrap();
    let normal = default_skills_for_personality(Some(hunter.into()));
    assert!(COMMON_BEHAVIOUR_IDS.iter().all(|b| normal.iter().any(|s| s == b)));
    assert!(normal.iter().any(|s| s == "web_walk"));
    let chosen = skills_with_selected_abilities::<&str>(Some(hunter.into()), Some(&[]));
    assert!(chosen.iter().all(|s| !ability_skill_ids().contains(&s.as_str())));
}

#[test]
fn personalities_bring_their_own_abilities() {
    let found = personalities();
    for (id, expected) in [
        ("drifter", vec!["drift", "web_walk"]),
        ("trapper", vec!["shoot_web", "wall_web", "web_walk"]),
        ("webber", vec!["weave_web", "web_walk"]),
    ] {
        let skills = default_skills_for_personality(Some(found.get(id).unwrap().into()));
        assert!(expected.iter().all(|e| skills.iter().any(|s| s == e)), "{id}: {skills:?}");
    }
    let nope: BTreeSet<String> =
        default_skills_for_personality(Some(found.get("nope").unwrap().into())).into_iter().collect();
    assert_eq!(nope, ["wander", "jump", "run_away"].iter().map(|s| s.to_string()).collect());
}

#[test]
fn shipped_personalities_select_their_modules() {
    let found = personalities();
    for (id, expected) in [
        ("hunter", vec!["hunter"]),
        ("trapper", vec!["hunter", "web_shooter"]),
        ("jumper", vec!["jumper"]),
        ("observer", vec!["observer"]),
        ("nope", vec!["nope"]),
        ("drifter", vec!["drifter"]),
        ("webber", vec!["webber"]),
        ("balanced", vec![]),
        ("bold", vec![]),
        ("curious", vec![]),
        ("playful", vec![]),
    ] {
        let got: Vec<&str> = behaviour_modules_for(found.get(id).unwrap()).into_iter().collect();
        let mut want = expected.clone();
        want.sort();
        assert_eq!(got, want, "{id}");
    }
    let explicit = personality(json!({"id": "hunter", "mouse_hunter": true, "behaviour_modules": ["drifter"]}));
    assert_eq!(behaviour_modules_for(&explicit).into_iter().collect::<Vec<_>>(), ["drifter"]);
    let unknown = personality(json!({"id": "balanced", "behaviour_modules": ["hunter", "not-a-module"]}));
    assert_eq!(behaviour_modules_for(&unknown).into_iter().collect::<Vec<_>>(), ["hunter"]);
    let legacy = personality(json!({"id": "custom", "web_weaver": true}));
    assert_eq!(behaviour_modules_for(&legacy).into_iter().collect::<Vec<_>>(), ["webber"]);
    let mut all = BTreeSet::new();
    for (_, raw) in json_files("personalities") {
        all.extend(behaviour_modules_for(&Personality::from_value(raw).unwrap()));
    }
    assert_eq!(all, BEHAVIOUR_MODULE_IDS.into_iter().collect());
}

#[test]
fn the_menu_offers_the_six_and_keeps_legacy_in_use() {
    let found = personalities();
    let selectable = pp::selectable_personality_ids(Some(&found), None);
    assert_eq!(selectable, COMPACT_TEMPERAMENT_IDS.to_vec());
    for legacy in ["hunter", "mellow", "nope", "trapper"] {
        assert!(!selectable.iter().any(|s| s == legacy));
        assert!(pp::selectable_personality_ids(Some(&found), Some(legacy)).iter().any(|s| s == legacy));
    }
    assert!(
        !pp::selectable_personality_ids(Some(&found), Some("not_a_personality"))
            .iter()
            .any(|s| s == "not_a_personality")
    );
}

// ---------------------------------------------------------------------------
// test_trapping_is_reachable.py
// ---------------------------------------------------------------------------

#[test]
fn silk_is_reachable_through_a_job() {
    let silk = ["shoot_web", "wall_web"];
    let job_abilities = |id: &str| jobs::job_by_id(id).unwrap().ability_ids.to_vec();
    assert!(jobs::JOB_IDS.iter().any(|j| job_abilities(j).iter().any(|a| silk.contains(a))));
    assert!(job_abilities("webber").contains(&"weave_web") && job_abilities("webber").contains(&"shoot_web"));
    assert!(job_abilities("guard").contains(&"shoot_web"));
    let mut via_job = 0;
    for (_, raw) in json_files("personalities") {
        let p = Personality::from_value(raw).unwrap();
        let base = default_ability_ids(Some((&p).into()));
        let base_has = base.iter().any(|a| silk.contains(&a.as_str()));
        for job in jobs::JOB_IDS {
            let combined_has = base_has || job_abilities(job).iter().any(|a| silk.contains(a));
            if combined_has && !base_has {
                via_job += 1;
            }
        }
        if p.id == "trapper" {
            assert!(base_has, "the Trapper temperament no longer grants silk");
        }
    }
    assert!(via_job > 0);
}

// ---------------------------------------------------------------------------
// test_enemy_kinds.py (data parts)
// ---------------------------------------------------------------------------

#[test]
fn every_enemy_kind_has_a_model_and_a_plan() {
    isolate();
    let (models, _) = discover_models(Some(&root()));
    for kind in enemy_kinds::enemy_kinds() {
        let model = models.get(kind.model_id).unwrap_or_else(|| panic!("{} has no model", kind.id));
        assert!(enemy_kinds::is_enemy_model(kind.model_id));
        assert_eq!(kind.skins.len(), 3);
        assert!(BODY_PLAN_IDS.contains(&model.body_plan.as_deref().unwrap()));
        let a = enemy_kinds::pick_skin(kind, 7);
        assert_eq!(a, enemy_kinds::pick_skin(kind, 7), "pick_skin is not stable");
    }
    assert!(!enemy_kinds::is_enemy_model("tarantula"));
}

// ---------------------------------------------------------------------------
// test_preset_path.py / test_preset_save_location.py
// ---------------------------------------------------------------------------

fn small_preset(name: &str, model: &str) -> Value {
    json!({"name": name, "slots": [{"model": model, "personality": "mellow", "count": 1, "slot_id": "slot-0"}], "settings": {}})
}

#[test]
fn a_relative_preset_path_resolves_in_a_source_build() {
    isolate();
    let _presets = common::preset_lock();
    let resolved = resolve_preset_path(Path::new("presets/colony.json"));
    assert!(resolved.is_file(), "{}", resolved.display());
    assert_eq!(
        discovery::resolve_path(&resolved),
        discovery::resolve_path(&root().join("presets").join("colony.json"))
    );
    let absolute = root().join("presets").join("default.json");
    assert_eq!(resolve_preset_path(&absolute), absolute);
    assert!(resolve_preset_path(Path::new("presets/default.json")).is_file());
    // A bare name means a preset.
    assert!(resolve_preset_path(Path::new("tarantula.json")).is_file());
    let missing = resolve_preset_path(Path::new("presets/no_such_preset.json"));
    assert!(!missing.exists());
    assert_eq!(missing.file_name().unwrap(), "no_such_preset.json");
}

#[test]
fn shipped_presets_are_detected_case_insensitively() {
    let state = isolate();
    let shipped = root().join("presets");
    let dirs: Vec<String> = shipped_presets_dirs().iter().map(|d| d.to_string_lossy().to_lowercase()).collect();
    assert!(dirs.contains(&shipped.to_string_lossy().to_lowercase()), "{dirs:?}");
    assert!(is_shipped_preset(&shipped.join("default.json")));
    assert!(is_shipped_preset(&shipped.join("Default.json")));
    assert!(is_shipped_preset(&shipped.join("colony.json")));
    assert!(!is_shipped_preset(&user_presets_dir().join("Default.json")));
    assert!(!is_shipped_preset(&std::env::temp_dir().join("elsewhere").join("default.json")));
    assert!(user_presets_dir().starts_with(state));
}

#[test]
fn saving_never_touches_a_shipped_preset_and_user_copies_shadow() {
    isolate();
    let _presets = common::preset_lock();
    let shipped = root().join("presets");
    let before = std::fs::read(shipped.join("default.json")).unwrap();
    let saved = save_preset_value(&small_preset("Default", "tarantula"), None).unwrap();
    assert_eq!(saved.parent().unwrap(), user_presets_dir());
    assert_eq!(saved.file_name().unwrap(), "Default.json");
    assert_eq!(std::fs::read(shipped.join("default.json")).unwrap(), before);
    assert_eq!(load_preset_value(&saved).unwrap()["name"], "Default");
    // Written like Python's json.dump(indent=2) + newline.
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.starts_with("{\n  \"name\": \"Default\",\n  \"slots\": [\n    {\n"), "{text}");
    assert!(text.ends_with("}\n"));
    std::fs::remove_file(&saved).unwrap();

    let colony_before = std::fs::read(shipped.join("colony.json")).unwrap();
    for target in [shipped.join("colony.json"), shipped.join("Colony.json")] {
        let err = save_preset_value(&small_preset("Colony", "tarantula"), Some(&target)).unwrap_err();
        assert!(err.to_string().contains("ships with the application"), "{err}");
    }
    assert_eq!(std::fs::read(shipped.join("colony.json")).unwrap(), colony_before);

    let saved = save_preset_value(&small_preset("Colony", "spider"), None).unwrap();
    let resolved = resolve_preset_path(Path::new("presets/Colony.json"));
    assert_eq!(discovery::resolve_path(&resolved), discovery::resolve_path(&saved));
    assert_eq!(load_preset(&resolved).unwrap().slots[0].model, "spider");
    let listed = discover_presets(None);
    let colonies: Vec<&PathBuf> =
        listed.iter().filter(|p| p.file_stem().unwrap().to_string_lossy().to_lowercase() == "colony").collect();
    assert_eq!(colonies.len(), 1);
    assert_eq!(discovery::resolve_path(colonies[0]), discovery::resolve_path(&saved));
    std::fs::remove_file(&saved).unwrap();
}

#[test]
fn shipped_presets_name_real_models() {
    let models: BTreeSet<String> = raw_models()
        .iter()
        .map(|(p, _)| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for (path, _) in json_files("presets") {
        let preset = load_preset(&path).unwrap();
        for slot in &preset.slots {
            assert!(models.contains(&slot.model), "{} names {}", path.display(), slot.model);
        }
    }
}

// ---------------------------------------------------------------------------
// test_state_location.py
// ---------------------------------------------------------------------------

struct FakeInstall {
    base: PathBuf,
    loc: StateLocation,
}

impl FakeInstall {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dc15-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("install");
        std::fs::create_dir_all(&root).unwrap();
        let local = base.join("AppData").join("Local");
        let loc = StateLocation {
            override_dir: None,
            app_root: root,
            local_app_data: Some(local.to_string_lossy().into()),
            home: base.clone(),
        };
        Self { base, loc }
    }
    fn root(&self) -> &Path {
        &self.loc.app_root
    }
}

impl Drop for FakeInstall {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[test]
fn state_defaults_to_per_user_and_portable_opts_out() {
    let fake = FakeInstall::new("default");
    assert_eq!(fake.loc.state_dir(), fake.loc.default_state_root().join("state"));
    assert!(fake.loc.state_dir().to_string_lossy().contains("AppData"));
    std::fs::write(fake.root().join("portable.txt"), "").unwrap();
    assert!(fake.loc.is_portable_install());
    assert_eq!(fake.loc.state_dir(), fake.root().join("state"));
    let mut overridden = fake.loc.clone();
    overridden.override_dir = Some(fake.root().join("elsewhere"));
    assert_eq!(overridden.state_dir(), fake.root().join("elsewhere"));
    let no_local = StateLocation { local_app_data: Some("  ".into()), ..fake.loc.clone() };
    assert_eq!(no_local.default_state_root(), fake.base.join("AppData").join("Local").join("DesktopBugCompanion"));
}

#[test]
fn a_legacy_state_folder_migrates_once_by_copy() {
    let fake = FakeInstall::new("migrate");
    let legacy = fake.root().join("state");
    std::fs::create_dir_all(&legacy).unwrap();
    let payload = r#"{"schema_version": 2, "creatures": {"default|slot-0:0": {"name": "Legacy"}}}"#;
    std::fs::write(legacy.join("creatures.json"), payload).unwrap();
    fake.loc.migrate_legacy_state_dir();
    let new_dir = fake.loc.state_dir();
    assert_ne!(new_dir, legacy);
    assert_eq!(std::fs::read_to_string(new_dir.join("creatures.json")).unwrap(), payload);
    assert!(legacy.join("creatures.json").exists());
    std::fs::write(new_dir.join("creatures.json"), "kept").unwrap();
    std::fs::write(legacy.join("creatures.json"), "changed").unwrap();
    fake.loc.migrate_legacy_state_dir();
    assert_eq!(std::fs::read_to_string(new_dir.join("creatures.json")).unwrap(), "kept");
}

#[test]
fn migration_is_a_no_op_without_legacy_or_when_portable() {
    let fake = FakeInstall::new("noop");
    fake.loc.migrate_legacy_state_dir();
    assert!(!fake.loc.state_dir().exists());
    std::fs::write(fake.root().join("portable.txt"), "").unwrap();
    let legacy = fake.root().join("state");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("creatures.json"), "{}").unwrap();
    fake.loc.migrate_legacy_state_dir();
    assert_eq!(fake.loc.state_dir(), legacy);
    assert_eq!(std::fs::read_dir(&legacy).unwrap().count(), 1);
}

#[test]
fn writability_is_probed() {
    let fake = FakeInstall::new("writable");
    assert!(fake.loc.state_dir_is_writable());
    assert!(fake.loc.state_dir().is_dir());
    let blocked = FakeInstall::new("blocked");
    let blocker = blocked.loc.default_state_root();
    std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
    std::fs::write(&blocker, "").unwrap();
    assert!(!blocked.loc.state_dir_is_writable());
}
