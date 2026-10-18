//! `jobs.rs` against `tools/dump_world_fixtures.py`: a scripted colony with
//! every job (two teams, lone spiders, a web tender with a web world), food,
//! damage, a torn web and a moved base; plus mounds, dirt patches, tints and
//! `BaseSite.from_dict`.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::jobs::{
    BaseSite, BaseWorld, FLY_CATCH_RESOURCE_AMOUNT, MAX_BUILD_PROGRESS, blend, patch_recipe, plan_mounds,
};
use dc_world::pymath::hypot;
use dc_world::webs::{TEAR_RADIUS, WebState, WebWorld, plan_orb};
use serde_json::{Value, json};

fn flags(c: &mut Bug, f: i64) {
    let i = c.index;
    c.job_busy = i == 3 && (700..760).contains(&f);
    c.dragging = i == 1 && (1000..1030).contains(&f);
    c.airborne = i == 2 && (1100..1120).contains(&f);
    c.hunting = (i == 5 && (400..500).contains(&f)) || (i == 7 && (1300..1400).contains(&f));
    c.feeding =
        (i == 5 && ((200..230).contains(&f) || (900..930).contains(&f))) || (i == 7 && (1500..1520).contains(&f));
}

fn walk(c: &mut Bug, f: i64) {
    if c.index == 6 {
        let t = f as f64 * DT;
        c.x = tri(1200.0, -170.0, 60.0, 1540.0, t);
        c.y = tri(700.0, -90.0, 60.0, 840.0, t);
        return;
    }
    let Some((tx, ty)) = c.target else {
        return;
    };
    if c.dragging {
        return;
    }
    let (dx, dy) = (tx - c.x, ty - c.y);
    let d = hypot(dx, dy);
    let step = 150.0 * DT;
    if d <= step {
        c.x = tx;
        c.y = ty;
    } else {
        c.x += dx / d * step;
        c.y += dy / d * step;
    }
}

#[test]
fn a_scripted_colony_matches_python() {
    let data = load("jobs.json");
    let col = &data["colony"];
    let mut rng = PyRandom::from_u64(4);
    let mut world = BaseWorld::new(1600.0, 900.0, &[]);
    let mut webs = WebWorld::new(1600.0, 900.0, PyRandom::from_str_seed("9:webs"));
    let mut bugs: Vec<Bug> = col["colony"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut b = Bug::new(i as i64, c[0].as_str().unwrap(), c[1].as_str().unwrap(), f(&c[2]), f(&c[3]));
            b.level = f(&c[4]);
            b.hp = c[5].as_f64();
            b
        })
        .collect();
    world.ensure_site(&bugs[0]).resources = 30.0;
    let plan = plan_orb((420.0, 430.0), 80.0, 1600.0, 900.0, &mut PyRandom::from_u64(31));
    let mut near = webs.make_web("orb", plan, "test_near", 20.0);
    near.state = WebState::Complete;
    near.built = near.strands.len();
    let hub = near.hub;
    for k in -5..=5 {
        near.damage_near(hub.0 + k as f64 * 8.0, hub.1 + 2.0, TEAR_RADIUS);
    }
    webs.webs.push(near);

    let events: std::collections::HashMap<i64, Vec<&str>> = col["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e[0].as_i64().unwrap(), e[1].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()))
        .collect();
    for (fr, row) in col["rows"].as_array().unwrap().iter().enumerate() {
        let fr = fr as i64;
        let ctx = format!("frame {fr}");
        for b in bugs.iter_mut() {
            flags(b, fr);
        }
        for ev in events.get(&fr).cloned().unwrap_or_default() {
            match ev {
                "food" => {
                    world.credit_team_food(Some("pack_a"), FLY_CATCH_RESOURCE_AMOUNT);
                    world.credit_team_food(Some("rivals"), 2.5);
                }
                "damage" => world.base_mut("team:pack_a").unwrap().integrity = 40.0,
                "tear" => {
                    let web = webs.webs.iter_mut().find(|w| w.is_complete()).expect("a finished web");
                    let (hx, hy) = web.hub;
                    for k in -6..=6 {
                        web.damage_near(hx + k as f64 * 8.0, hy + 2.0, TEAR_RADIUS);
                    }
                }
                "move" => assert!(world.move_base("team:rivals", 1400.0, 820.0)),
                other => panic!("unknown event {other}"),
            }
        }
        world.update(DT, &mut bugs, Some(&mut webs), &mut rng);
        webs.update(DT, None);
        for (b, w) in bugs.iter().zip(row.as_array().unwrap()) {
            let bctx = format!("{ctx} creature {} ({})", b.index, b.job);
            assert_eq!(b.mode.as_str(), w[0].as_str().unwrap(), "{bctx}: mode");
            assert_pair(b.target, &w[1], &format!("{bctx}: target"));
            assert_eq!(b.alert.map(|a| a.0), w[2].as_u64(), "{bctx}: alert target");
            assert_eq!(b.base_id.as_deref(), w[3].as_str(), "{bctx}: base");
            match (b.facing, w[4].as_f64()) {
                (None, None) => {}
                (Some(g), Some(_)) => assert_close(g, &w[4], &format!("{bctx}: facing")),
                (g, want) => panic!("{bctx}: facing {g:?} vs {want:?}"),
            }
        }
        if let Some(cp) = col["checkpoints"].get(fr.to_string()) {
            assert_json(&Value::Array(world.to_dict()), &cp["bases"], &format!("{ctx}: bases"));
            for (b, w) in bugs.iter().zip(cp["hp"].as_array().unwrap()) {
                assert_eq!(b.hp.is_some(), !w.is_null(), "{ctx}: hp");
                if let Some(hp) = b.hp {
                    assert_close(hp, w, &format!("{ctx}: hp of {}", b.index));
                }
            }
            for (b, w) in bugs.iter().zip(cp["pos"].as_array().unwrap()) {
                assert_pair(Some((b.x, b.y)), w, &format!("{ctx}: position of {}", b.index));
            }
            let want_webs = cp["webs"].as_array().unwrap();
            assert_eq!(webs.webs.len(), want_webs.len(), "{ctx}: webs");
            for (web, w) in webs.webs.iter().zip(want_webs) {
                assert_eq!(web.built as u64, w[0].as_u64().unwrap(), "{ctx}: web built");
                assert_close(web.active_t, &w[1], &format!("{ctx}: web active_t"));
                assert_eq!(web.state_str(), w[2].as_str().unwrap(), "{ctx}: web state");
                assert_eq!(web.cut.len() as u64, w[3].as_u64().unwrap(), "{ctx}: web torn");
                assert_pair(Some(web.hub), &w[4], &format!("{ctx}: web hub"));
            }
            for (b, w) in bugs.iter().zip(cp["duty"].as_array().unwrap()) {
                match (world.duty_state(b), w.as_array()) {
                    (None, None) => {}
                    (Some(d), Some(w)) => {
                        assert_eq!(d.on_duty, w[0].as_bool().unwrap(), "{ctx}: on duty {}", b.index);
                        assert_close(d.remaining, &w[1], &format!("{ctx}: shift left {}", b.index));
                    }
                    (d, _) => panic!("{ctx}: duty of {}: {d:?} vs {w}", b.index),
                }
            }
        }
        for b in bugs.iter_mut() {
            walk(b, fr);
        }
    }
    assert_close(rng.random(), &col["next"], "manager stream position");
    assert_close(webs.rng.random(), &col["webs_next"], "web stream position");
    for (b, w) in bugs.iter_mut().zip(col["creature_next"].as_array().unwrap()) {
        assert_close(b.rng.random(), w, &format!("creature {} stream", b.index));
    }
}

#[test]
fn mounds_patches_tints_and_saved_bases_match_python() {
    let data = load("jobs.json");
    let p = &data["parts"];
    for m in p["mounds"].as_array().unwrap() {
        let plan = plan_mounds(m["id"].as_str().unwrap(), f(&m["radius"]));
        let got: Vec<Value> = plan.iter().map(|q| json!([q.0, q.1, q.2, q.3])).collect();
        assert_json(&Value::Array(got), &m["plan"], &format!("mounds of {}", m["id"]));
    }
    for s in p["sites"].as_array().unwrap() {
        let mut site = BaseSite::new("demo|1", "o", "hunters", 400.0, 300.0);
        site.build_progress = MAX_BUILD_PROGRESS * f(&s["completion"]);
        site.level = 2;
        let got: Vec<Value> = site.mounds().iter().map(|m| json!([m.x, m.y, m.size, m.aspect, m.built])).collect();
        assert_json(&Value::Array(got), &s["mounds"], &format!("site at {}", s["completion"]));
    }
    for (key, r) in p["recipes"].as_object().unwrap() {
        let got = patch_recipe(key);
        let pts = |v: &[(f64, f64)]| Value::Array(v.iter().map(|q| json!([q.0, q.1])).collect());
        assert_json(&pts(&got.body), &r["body"], &format!("{key} body"));
        assert_json(&pts(&got.crown), &r["crown"], &format!("{key} crown"));
        assert_json(&json!([got.crown_offset.0, got.crown_offset.1]), &r["crown_offset"], key);
        let clods: Vec<Value> = got.clods.iter().map(|c| json!([c.0, c.1, c.2, c.3])).collect();
        assert_json(&Value::Array(clods), &r["clods"], &format!("{key} clods"));
    }
    for b in p["blends"].as_array().unwrap() {
        let rgb =
            |v: &Value| (v[0].as_u64().unwrap() as u8, v[1].as_u64().unwrap() as u8, v[2].as_u64().unwrap() as u8);
        let got = blend(rgb(&b[0]), rgb(&b[1]), f(&b[2]));
        assert_eq!(got, rgb(&b[3]), "blend {b}");
    }
    for case in p["from_dict"].as_array().unwrap() {
        let got = BaseSite::from_dict(&case[0]).map(|s| s.to_dict());
        match (got, &case[1]) {
            (None, Value::Null) => {}
            (Some(g), w) if !w.is_null() => assert_json(&g, w, &format!("from_dict {}", case[0])),
            (g, w) => panic!("from_dict {}: got {g:?}, want {w}", case[0]),
        }
    }
}
