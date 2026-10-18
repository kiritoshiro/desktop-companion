//! Compares dc-adventure against fixtures dumped from the Python modules by
//! `tools/dump_fixtures.py`.

use dc_adventure::adventure_profile::{load_profile, profile_path};
use dc_adventure::campaign::{enemy_loadout, map_by_id, roll_drop};
use dc_adventure::controls::ControlSettings;
use dc_adventure::custom_maps::{clean_map, map_info, new_map, problems};
use dc_adventure::encounters::{EncounterDirector, OutpostSite, profile};
use dc_core::PyRandom;
use dc_world::{ScreenLayout, ScreenRect};
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

/// JSON equality with numbers compared as f64 (Python writes 1 and 1.0 alike
/// where Rust keeps them apart) and objects compared in key order.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-9 * y.abs().max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|((ka, va), (kb, vb))| ka == kb && same(va, vb))
        }
        _ => a == b,
    }
}

fn assert_same(got: &Value, want: &Value, what: &str) {
    assert!(same(got, want), "{what}\n got: {got}\nwant: {want}");
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn enemy_loadouts_and_drops_draw_like_python() {
    for case in load("campaign.json").as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let mut rng = PyRandom::from_int(seed as i128);
        for (n, d) in case["draws"].as_array().unwrap().iter().enumerate() {
            let info = map_by_id(d["map"].as_str().unwrap()).unwrap();
            let role = d["role"].as_str().unwrap();
            let (worn, level) = enemy_loadout(info, role, &mut rng);
            let at = format!("seed {seed} draw {n} {} {role}", info.id);
            assert_eq!(worn, strings(&d["worn"]), "{at}");
            assert_eq!(level, d["level"].as_i64().unwrap(), "{at}");
            assert_eq!(roll_drop(&worn, role == "guardian", &mut rng), strings(&d["drop"]), "{at}");
            assert_eq!(rng.random(), d["after"].as_f64().unwrap(), "{at}: the draw order matches");
        }
    }
}

struct Site {
    owned: bool,
    reserves: i64,
    warning: f64,
}

impl OutpostSite for Site {
    fn owned(&self) -> bool {
        self.owned
    }
    fn reserves(&self) -> i64 {
        self.reserves
    }
    fn set_warning(&mut self, warning: f64) {
        self.warning = warning;
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn encounter_plans_and_waves_match_python() {
    for (n, case) in load("encounters.json").as_array().unwrap().iter().enumerate() {
        let rects: Vec<ScreenRect> = case["rects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let f = |i: usize| r[i].as_f64().unwrap();
                ScreenRect::new(f(0), f(1), f(2), f(3))
            })
            .collect();
        let layout = ScreenLayout::new(&rects, case["primary"].as_u64().unwrap() as usize).unwrap();
        let kind = case["kind"].as_str().unwrap();
        let mut director =
            EncounterDirector::new(profile(kind).unwrap().clone(), layout, case["tier"].as_i64().unwrap());
        let avoid: Vec<(f64, f64)> = case["avoid"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
            .collect();
        let plans = director.outposts(&avoid, 240.0);
        let want = case["plans"].as_array().unwrap();
        assert_eq!(plans.len(), want.len(), "case {n}");
        for (p, w) in plans.iter().zip(want) {
            assert_eq!(p.screen as u64, w["screen"].as_u64().unwrap(), "case {n}");
            assert!(close(p.x, w["x"].as_f64().unwrap()) && close(p.y, w["y"].as_f64().unwrap()), "case {n}");
            assert_eq!(p.kind, w["kind"].as_str().unwrap());
            assert_eq!(p.name, w["name"].as_str().unwrap());
            assert_eq!(p.defenders, strings(&w["defenders"]), "case {n}");
            assert_eq!(p.reserves, w["reserves"].as_i64().unwrap());
        }
        let mut sites: Vec<Site> =
            (0..plans.len()).map(|i| Site { owned: false, reserves: 2 + i as i64, warning: 0.0 }).collect();
        let mut trace = Vec::new();
        for step in 0..900 {
            if step == 500 && !sites.is_empty() {
                sites[0].owned = true;
            }
            let got = director.update(0.1, &mut sites);
            for (index, _) in &got {
                if step % 7 == 0 {
                    director.retry(*index);
                } else {
                    sites[*index].reserves -= 1;
                }
            }
            if !got.is_empty() {
                trace.push((step, got, sites.iter().map(|s| s.warning).collect::<Vec<_>>()));
            }
        }
        let want = case["trace"].as_array().unwrap();
        assert_eq!(trace.len(), want.len(), "case {n} waves");
        for ((step, got, warnings), w) in trace.iter().zip(want) {
            assert_eq!(*step as u64, w[0].as_u64().unwrap(), "case {n}");
            let want_got: Vec<(usize, String)> = w[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| (g[0].as_u64().unwrap() as usize, g[1].as_str().unwrap().to_string()))
                .collect();
            assert_eq!(*got, want_got, "case {n} step {step}");
            let want_warn: Vec<f64> = w[2].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            assert!(warnings.iter().zip(&want_warn).all(|(a, b)| close(*a, *b)), "case {n} step {step}");
        }
    }
}

#[test]
fn custom_maps_clean_like_python() {
    for (n, case) in load("custom_maps.json").as_array().unwrap().iter().enumerate() {
        let data = if case["raw"].is_null() { new_map("Fresh") } else { clean_map(&case["raw"]) };
        assert_same(&data.to_value(), &case["clean"], &format!("case {n} clean"));
        assert_eq!(problems(&data), strings(&case["problems"]), "case {n}");
        if let Some(w) = case["info"].as_object() {
            let info = map_info(&data);
            assert_eq!(info.title, w["title"].as_str().unwrap());
            assert_eq!(info.blurb, w["blurb"].as_str().unwrap());
            assert_eq!(info.guardian, w["guardian"].as_str().unwrap(), "case {n}");
            assert_eq!(info.guardian_set, w["guardian_set"].as_str().unwrap(), "case {n}");
            assert_eq!(info.tier, w["tier"].as_i64().unwrap());
            assert_eq!(info.enemy_tiers, strings(&w["enemy_tiers"]));
            assert_eq!(info.reward_companion.as_deref(), w["reward_companion"].as_str());
        }
    }
}

#[test]
fn controls_load_like_python() {
    for (n, case) in load("controls.json").as_array().unwrap().iter().enumerate() {
        let got = ControlSettings::from_dict(&case["raw"]).to_dict();
        assert_same(&got, &case["out"], &format!("case {n}"));
    }
}

#[test]
fn profiles_load_and_save_like_python() {
    let dir = std::env::temp_dir().join(format!("dc-profile-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = profile_path(&dir);
    for (n, case) in load("profiles.json").as_array().unwrap().iter().enumerate() {
        std::fs::write(&path, serde_json::to_string(&case["raw"]).unwrap()).unwrap();
        let got = load_profile(&path).to_value();
        assert_same(&got, &case["saved"], &format!("case {n}"));
    }
}
