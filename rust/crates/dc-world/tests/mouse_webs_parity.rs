//! `mouse_webs.rs` against `tools/dump_world_fixtures.py`: traps, a wall
//! shove, a miss and a tracking shot against a recorded pointer, with and
//! without pointer control.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::MouseWebWorld;
use serde_json::{Value, json};

fn xy4(r: (f64, f64, f64, f64)) -> Value {
    json!([r.0, r.1, r.2, r.3])
}

#[test]
fn cursor_silk_matches_python() {
    let data = load("mouse_webs.json");
    for case in data.as_array().unwrap() {
        let seed = case["seed"].as_u64().unwrap();
        let can_control = case["can_control"].as_bool().unwrap();
        let mut world =
            MouseWebWorld::new(1200.0, 800.0, can_control, PyRandom::from_str_seed(&format!("{seed}:mouse-webs")));
        let events: Vec<&Value> = case["events"].as_array().unwrap().iter().collect();
        for (fr, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let ctx = format!("seed {seed} frame {fr}");
            for ev in events.iter().filter(|e| e[0].as_u64() == Some(fr as u64)) {
                let ok = world.shoot(
                    pair(&ev[2]).unwrap(),
                    pair(&ev[3]).unwrap(),
                    ev[1].as_str().unwrap(),
                    f(&ev[4]),
                    pair(&ev[5]).unwrap(),
                );
                assert_eq!(ok, ev[6].as_bool().unwrap(), "{ctx}: shoot");
            }
            if fr == 1060 {
                world.enabled = false;
            }
            if fr == 1100 {
                world.enabled = true;
            }
            let (mx, my) = (f(&row[0]), f(&row[1]));
            let desired = world.update(DT, mx, my);
            assert_pair(desired, &row[2], &format!("{ctx}: desired pointer"));
            assert_eq!(world.active_kind(), row[3].as_str(), "{ctx}: active kind");
            assert_pair(world.projectile.as_ref().map(|p| p.pos), &row[4], &format!("{ctx}: glob"));
            match (&world.capture, row[5].as_array()) {
                (None, None) => {}
                (Some(c), Some(_)) => assert_json(&xy4(c.bbox()), &row[5], &format!("{ctx}: capture bbox")),
                (c, _) => panic!("{ctx}: capture {c:?} vs {}", row[5]),
            }
            let hold = world.capture.as_ref().and_then(|c| c.hold());
            match (hold, row[6].as_array()) {
                (None, None) => {}
                (Some(h), Some(w)) => {
                    assert_close(h.struggle, &w[0], &format!("{ctx}: struggle"));
                    assert_close(h.hold_t, &w[1], &format!("{ctx}: hold_t"));
                    assert_close(h.wobble, &w[2], &format!("{ctx}: wobble"));
                    assert_pair(Some(h.strain), &w[3], &format!("{ctx}: strain"));
                }
                (h, _) => panic!("{ctx}: hold {h:?} vs {}", row[6]),
            }
            assert_eq!(world.spent.len() as u64, row[7].as_u64().unwrap(), "{ctx}: spent splats");
            let dirty: Vec<Value> = world.dirty_rects().into_iter().map(xy4).collect();
            assert_json(&Value::Array(dirty), &row[8], &format!("{ctx}: dirty rects"));
        }
        assert_close(world.rng.random(), &case["next"], &format!("seed {seed}: stream position"));
    }
}
