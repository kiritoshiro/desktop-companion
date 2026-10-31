//! `webs.rs` against `tools/dump_world_fixtures.py`: the four planners, site
//! claims, a build / tear / mend trace and the state-file round trip.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::creature_view::CreatureId;
use dc_world::webs::{
    DEFAULT_LEAD_MAX, REPAIR_RATE, Strand, WebWorld, plan_corner_orb, plan_funnel, plan_orb, plan_tangle,
};
use serde_json::Value;

#[track_caller]
fn assert_strands(got: &[Strand], want: &Value, ctx: &str) {
    let want = want.as_array().unwrap();
    assert_eq!(got.len(), want.len(), "{ctx}: strand count");
    for (i, (s, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(s.kind.as_str(), w[0].as_str().unwrap(), "{ctx}: strand {i} kind");
        assert_eq!(s.draw, w[1].as_bool().unwrap(), "{ctx}: strand {i} draw");
        assert_eq!(s.temporary, w[2].as_bool().unwrap(), "{ctx}: strand {i} temporary");
        let pts = w[3].as_array().unwrap();
        assert_eq!(s.points.len(), pts.len(), "{ctx}: strand {i} points");
        for (j, (p, q)) in s.points.iter().zip(pts).enumerate() {
            assert_pair(Some(*p), q, &format!("{ctx}: strand {i} point {j}"));
        }
    }
}

#[test]
fn planners_match_python() {
    let data = load("webs.json");
    for case in data["plans"].as_array().unwrap() {
        let pattern = case["pattern"].as_str().unwrap();
        let seed = case["seed"].as_u64().unwrap();
        let ctx = format!("{pattern}/{seed}");
        let mut rng = PyRandom::from_u64(seed);
        let vertex = pair(&case["vertex"]).unwrap();
        let din = pair(&case["din"]).unwrap();
        let center = pair(&case["center"]).unwrap();
        let span = f(&case["span"]);
        let (strands, hub, wob) = match pattern {
            "corner_orb" => plan_corner_orb(vertex, din, span, 38.0, 1600.0, 900.0, &mut rng),
            "orb" => plan_orb(center, span, 1600.0, 900.0, &mut rng),
            "funnel" => plan_funnel(vertex, din, span, 1600.0, 900.0, &mut rng),
            _ => plan_tangle(vertex, din, span, 1600.0, 900.0, &mut rng),
        };
        assert_strands(&strands, &case["strands"], &ctx);
        assert_pair(Some(hub), &case["hub"], &format!("{ctx}: hub"));
        assert_pair(Some(wob), &case["wob"], &format!("{ctx}: wob"));
        assert_close(rng.random(), &case["next"], &format!("{ctx}: stream position"));
    }
}

#[test]
fn site_claims_match_python() {
    let data = load("webs.json");
    for case in data["claims"].as_array().unwrap() {
        let seed = case["seed"].as_u64().unwrap();
        let mut world = WebWorld::new(1600.0, 900.0, PyRandom::from_str_seed(&format!("{seed}:webs")));
        let mut picker = PyRandom::from_u64(seed + 500);
        for (i, ev) in case["events"].as_array().unwrap().iter().enumerate() {
            let weights: Option<Vec<(&str, f64)>> =
                ev["weights"].as_array().map(|ws| ws.iter().map(|w| (w[0].as_str().unwrap(), f(&w[1]))).collect());
            let got = world.claim_site(
                CreatureId(i as u64),
                f(&ev["size"]),
                ev["prefer"].as_bool().unwrap(),
                weights.as_deref(),
                Some(&mut picker),
            );
            assert_eq!(got.is_some(), ev["claimed"].as_bool().unwrap(), "seed {seed} claim {i}");
        }
        assert_json(&Value::Array(world.to_dict()), &case["webs"], &format!("seed {seed} webs"));
        let ids: Vec<_> = world.webs.iter().map(|w| w.id).collect();
        let mut walk = Vec::new();
        for id in ids {
            for _ in 0..3 {
                walk.push(world.walkable_point(id).unwrap());
            }
        }
        for (k, (p, w)) in walk.iter().zip(case["walk"].as_array().unwrap()).enumerate() {
            assert_pair(Some(*p), w, &format!("seed {seed} walk {k}"));
        }
        assert_close(world.rng.random(), &case["next"], &format!("seed {seed}: world stream"));
        assert_close(picker.random(), &case["picker_next"], &format!("seed {seed}: picker stream"));
    }
}

#[test]
fn a_build_tear_and_mend_trace_matches_python() {
    let data = load("webs.json");
    let trace = &data["trace"];
    let mut world = WebWorld::new(1600.0, 900.0, PyRandom::from_str_seed("7:webs"));
    let mut picker = PyRandom::from_u64(77);
    for (n, (size, prefer)) in [(18.0, true), (24.0, false), (30.0, true)].into_iter().enumerate() {
        world.claim_site(CreatureId(n as u64), size, prefer, None, Some(&mut picker)).unwrap();
    }
    let speeds: Vec<f64> = trace["speeds"].as_array().unwrap().iter().map(f).collect();
    let mend_from = trace["mend_from"].as_u64().unwrap() as usize;
    let frames = trace["frames"].as_array().unwrap();
    for (fr, row) in frames.iter().enumerate() {
        let ctx = format!("frame {fr}");
        let cursor = pair(&row["cursor"]);
        for (i, web) in world.webs.iter_mut().enumerate() {
            if !web.is_complete() {
                let (tip, _drawing) = web.working_point();
                let lag = if (fr / 40) % 5 == i { 30.0 } else { 0.0 };
                web.advance(DT, speeds[i], (tip.0 + lag, tip.1), DEFAULT_LEAD_MAX);
            }
        }
        if fr >= mend_from {
            for web in world.webs.iter_mut() {
                if web.is_damaged() {
                    let hub = web.hub;
                    web.repair_near(hub.0 + 7.0, hub.1 - 5.0, DT, REPAIR_RATE, 1e9);
                }
            }
        }
        world.update(DT, cursor);
        let want = row["webs"].as_array().unwrap();
        assert_eq!(world.webs.len(), want.len(), "{ctx}: web count");
        for (w, web) in want.iter().zip(&world.webs) {
            let w = w.as_array().unwrap();
            let mending = fr >= mend_from && web.is_damaged();
            assert_eq!(web.built as u64, w[0].as_u64().unwrap(), "{ctx}: built");
            assert_close(web.active_t, &w[1], &format!("{ctx}: active_t"));
            if w.len() == 4 {
                assert_close(web.bounce_amp, &w[2], &format!("{ctx}: bounce_amp"));
                assert_eq!(web.cut.len() as u64, w[3].as_u64().unwrap(), "{ctx}: torn segments");
            } else {
                assert_eq!(web.state_str(), w[2].as_str().unwrap(), "{ctx}: state");
                assert_close(web.bounce_amp, &w[3], &format!("{ctx}: bounce_amp"));
                assert_close(web.bounce_phase, &w[4], &format!("{ctx}: bounce_phase"));
                assert_eq!(web.cut.len() as u64, w[5].as_u64().unwrap(), "{ctx}: torn segments");
                // Which of two equally near torn segments is mended first is
                // set-order in Python (see webs.rs); the count always agrees.
                if !mending {
                    assert_close(web.intact_fraction(), &w[6], &format!("{ctx}: intact"));
                }
                assert_eq!(web.is_animating(), w[7].as_bool().unwrap(), "{ctx}: animating");
                assert_close(web.progress(), &w[8], &format!("{ctx}: progress"));
                assert_close(web.temp_alpha(), &w[9], &format!("{ctx}: temp_alpha"));
            }
        }
        assert_eq!(world.dirty_rects().len() as u64, row["dirty"].as_u64().unwrap(), "{ctx}: dirty rects");
        if let Some(cp) = trace["checkpoints"].get(fr.to_string()) {
            assert_json(&Value::Array(world.to_dict()), &cp["webs"], &format!("{ctx}: checkpoint"));
            for (web, c) in world.webs.iter().zip(cp["centroids"].as_array().unwrap()) {
                assert_pair(web.damaged_centroid(), c, &format!("{ctx}: centroid"));
            }
        }
    }
    assert_json(&Value::Array(world.to_dict()), &trace["checkpoints"]["end"]["webs"], "end");
    let n = world.webs.len();
    for (k, w) in trace["walk"].as_array().unwrap().iter().enumerate() {
        let id = world.webs[k % n].id;
        assert_pair(world.walkable_point(id), w, &format!("walk {k}"));
    }
    assert_close(world.rng.random(), &trace["next"], "world stream");
}

#[test]
fn the_state_file_round_trip_matches_python() {
    let data = load("webs.json");
    let p = &data["persistence"];
    let entries = p["entries"].as_array().unwrap();
    let mut other = WebWorld::new(1600.0, 900.0, PyRandom::from_u64(0));
    assert_eq!(other.restore(&p["entries"]) as u64, p["count"].as_u64().unwrap());
    assert_json(&Value::Array(other.to_dict()), &p["restored"], "restored");

    let saved = Value::Array(vec![entries[0].clone(), entries[entries.len() - 1].clone()]);
    let mut small = WebWorld::new(1600.0, 900.0, PyRandom::from_u64(0));
    small.restore(&saved);
    small.set_screen(300.0, 200.0);
    assert_json(&Value::Array(small.to_dict()), &p["after_shrink"], "after set_screen");
    assert_eq!(other.restore(&Value::from("nope")), 0);
}
