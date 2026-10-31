//! `cage.rs` and `carcass.rs` against `tools/dump_world_fixtures.py`.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::cage::{Cage, Corner};
use dc_world::carcass::{Carcass, DamageNumber, eaters_near, note_damage};
use serde_json::{Value, json};

fn xy4(r: (f64, f64, f64, f64)) -> Value {
    json!([r.0, r.1, r.2, r.3])
}

#[test]
fn cages_match_python() {
    let data = load("cage.json");
    for (n, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let ctx = format!("cage {n}");
        let s = &case["start"];
        let mut cage = Cage::new(f(&s[0]), f(&s[1]), f(&s[2]), f(&s[3]));
        assert_json(&xy4(cage.rect()), s, &ctx);
        let inset = f(&case["inset"]);
        let (px, py) = pair(&case["p"]).unwrap();
        let (cx, cy) = pair(&case["corner_probe"]).unwrap();
        assert_json(&xy4(cage.interior(inset)), &case["interior"], &format!("{ctx}: interior"));
        assert_pair(Some(cage.clamp_center(px, py, inset)), &case["clamp"], &format!("{ctx}: clamp"));
        assert_eq!(cage.contains_point(px, py), case["contains"].as_bool().unwrap(), "{ctx}: contains");
        assert_eq!(cage.on_border(px, py), case["on_border"].as_bool().unwrap(), "{ctx}: on_border");
        assert_eq!(cage.corner_at(cx, cy).map(Corner::as_str), case["corner_at"].as_str(), "{ctx}: corner_at");
        assert_json(&xy4(cage.outer_bbox()), &case["outer"], &format!("{ctx}: outer"));
        assert_json(&xy4(cage.footprint_rect(6.0)), &case["footprint"], &format!("{ctx}: footprint"));
        let bands: Vec<Value> = cage.border_rects().into_iter().map(xy4).collect();
        assert_json(&Value::Array(bands), &case["bands"], &format!("{ctx}: bands"));
        let (rx, ry) = pair(&case["resize_to"]).unwrap();
        cage.resize_corner(Corner::parse(case["corner"].as_str().unwrap()).unwrap(), rx, ry);
        assert_json(&xy4(cage.rect()), &case["resized"], &format!("{ctx}: resized"));
        let (sw, sh) = pair(&case["screen"]).unwrap();
        cage.clamp_to_screen(sw, sh, 4.0);
        assert_json(&xy4(cage.rect()), &case["clamped"], &format!("{ctx}: clamped"));
    }
    for case in data["from_dict"].as_array().unwrap() {
        let got = Cage::from_dict(&case[0]).map(|c| c.to_dict());
        match (got, &case[1]) {
            (None, Value::Null) => {}
            (Some(g), w) if !w.is_null() => assert_json(&g, w, &format!("from_dict {}", case[0])),
            (g, w) => panic!("from_dict {}: got {g:?}, want {w}", case[0]),
        }
    }
}

#[test]
fn carcasses_match_python() {
    let data = load("carcass.json");
    let mut script = PyRandom::from_u64(12);
    for case in data.as_array().unwrap() {
        let k = case["k"].as_i64().unwrap();
        let kf = k as f64;
        let mut c = Carcass::new(
            100.0 + kf * 50.0,
            200.0,
            6.0 + kf * 9.0,
            if k == 2 { "" } else { "pack_a" },
            Some((120, 80, 60)),
        );
        c.damage_numbers = vec![DamageNumber { amount: 3.0 + kf, age: 0.0, nudge: 0.0 }];
        c.damage_team = Some(if k % 2 == 1 { "rivals" } else { "pack_a" }.to_string());
        assert_close(c.size, &case["size"], "size");
        assert_eq!(c.team_id, case["team"].as_str().unwrap());
        let legs: Vec<Value> = c.legs.iter().map(|l| json!([l.0, l.1, l.2])).collect();
        assert_json(&Value::Array(legs), &case["legs"], "legs");
        assert_close(c.damage_top(), &case["damage_top"], "damage top");
        let col = c.damage_color();
        assert_eq!(json!([col.0, col.1, col.2]), case["color"], "colour");

        let mut bodies = [Bug::new(0, "none", "a", c.x + 5.0, c.y), Bug::new(1, "none", "a", c.x + 500.0, c.y)];
        let rows = case["rows"].as_array().unwrap();
        let mut next = rows.iter();
        let mut done_at = None;
        for fr in 0..1800i64 {
            bodies[0].x = c.x + if (fr / (60 + k * 30)) % 2 == 1 { 5.0 } else { 400.0 };
            bodies[1].dragging = fr % 2 == 0;
            if fr % 97 == 3 {
                let amount = *script.choice(&[0.0, 0.5, 2.0, 7.0]);
                note_damage(&mut c.damage_numbers, amount);
            }
            let eaters = eaters_near(&c, &bodies);
            let done = c.update(DT, eaters);
            if fr % 10 == 0 || done {
                let w = next.next().expect("row");
                let ctx = format!("carcass {k} frame {fr}");
                assert_eq!(w[0].as_i64().unwrap(), fr, "{ctx}");
                assert_eq!(eaters as u64, w[1].as_u64().unwrap(), "{ctx}: eaters");
                assert_close(c.t, &w[2], &format!("{ctx}: t"));
                assert_close(c.settle_t, &w[3], &format!("{ctx}: settle"));
                assert_close(c.spent(), &w[4], &format!("{ctx}: spent"));
                assert_eq!(c.being_eaten, w[5].as_bool().unwrap(), "{ctx}: being eaten");
                assert_eq!(done, w[6].as_bool().unwrap(), "{ctx}: done");
                assert_json(&xy4(c.footprint()), &w[7], &format!("{ctx}: footprint"));
                let nums: Vec<Value> = c.damage_numbers.iter().map(|n| json!([n.amount, n.age, n.nudge])).collect();
                assert_json(&Value::Array(nums), &w[8], &format!("{ctx}: numbers"));
            }
            if done {
                done_at = Some(fr);
                break;
            }
        }
        assert_eq!(done_at, case["done_at"].as_i64(), "carcass {k}: eaten at");
        assert!(next.next().is_none());
    }
}
