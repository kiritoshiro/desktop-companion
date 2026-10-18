//! Compares dc-world against fixtures dumped from the Python modules by
//! `tools/dump_fixtures.py` (tactics plans, screen layouts, playfield clamps).

use dc_world::screen_layout::TUNNEL_REACH;
use dc_world::tactics::{self, Foe, Member, Role, Stance};
use dc_world::{Playfield, ScreenLayout, ScreenRect};
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn f(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

fn pair(v: &Value) -> Option<(f64, f64)> {
    v.as_array().map(|a| (f(&a[0]), f(&a[1])))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

fn close_pair(a: (f64, f64), b: (f64, f64)) -> bool {
    close(a.0, b.0) && close(a.1, b.1)
}

fn rects(v: &Value) -> Vec<ScreenRect> {
    v.as_array().unwrap().iter().map(|r| ScreenRect::new(f(&r[0]), f(&r[1]), f(&r[2]), f(&r[3]))).collect()
}

#[test]
fn tactics_plans_match_python() {
    let cases = load("tactics.json");
    for (n, case) in cases.as_array().unwrap().iter().enumerate() {
        let members: Vec<Member<String>> = case["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| Member {
                health: f(&m["health"]),
                role: Role::parse(m["role"].as_str().unwrap()).unwrap(),
                reach: f(&m["reach"]),
                anchor: pair(&m["anchor"]),
                ..Member::new(m["key"].as_str().unwrap().to_string(), f(&m["x"]), f(&m["y"]))
            })
            .collect();
        let foes: Vec<Foe<String>> = case["foes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| Foe {
                key: o["key"].as_str().unwrap().to_string(),
                x: f(&o["x"]),
                y: f(&o["y"]),
                health: f(&o["health"]),
                threat: f(&o["threat"]),
            })
            .collect();
        let plan = tactics::plan(
            &members,
            &foes,
            case["level"].as_i64().unwrap() as i32,
            Stance::parse(case["stance"].as_str().unwrap()).unwrap(),
            pair(&case["anchor"]),
            f(&case["notice"]),
        );
        assert_eq!(plan.doctrine.as_str(), case["doctrine"].as_str().unwrap(), "case {n}");
        assert_eq!(plan.focus.as_deref(), case["focus"].as_str(), "case {n}");
        let want = case["orders"].as_array().unwrap();
        assert_eq!(plan.orders.len(), want.len(), "case {n}");
        for ((key, order), w) in plan.orders.iter().zip(want) {
            assert_eq!(key, w["key"].as_str().unwrap(), "case {n}");
            assert_eq!(order.mode.as_str(), w["mode"].as_str().unwrap(), "case {n}");
            assert_eq!(order.target.as_deref(), w["target"].as_str(), "case {n}");
            assert_eq!(order.note, w["note"].as_str().unwrap(), "case {n}");
            match (order.point, pair(&w["point"])) {
                (None, None) => {}
                (Some(a), Some(b)) => assert!(close_pair(a, b), "case {n} {key}: {a:?} vs {b:?}"),
                other => panic!("case {n} {key}: point {other:?}"),
            }
        }
    }
}

#[test]
fn screen_layouts_match_python() {
    let cases = load("screen_layout.json");
    for (n, case) in cases.as_array().unwrap().iter().enumerate() {
        let layout = ScreenLayout::new(&rects(&case["rects"]), case["primary"].as_u64().unwrap() as usize).unwrap();
        assert_eq!(layout.playfield.simple(), case["simple"].as_bool().unwrap(), "case {n}");
        let names: Vec<String> = layout.screens.iter().map(|s| s.name()).collect();
        let want_names: Vec<&str> = case["names"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, want_names, "case {n}");
        let links = case["links"].as_array().unwrap();
        assert_eq!(layout.links.len(), links.len(), "case {n}");
        for (link, w) in layout.links.iter().zip(links) {
            assert_eq!(link.a as u64, w["a"].as_u64().unwrap(), "case {n}");
            assert_eq!(link.b as u64, w["b"].as_u64().unwrap(), "case {n}");
            assert_eq!(link.kind.as_str(), w["kind"].as_str().unwrap(), "case {n}");
            assert!(close_pair(link.a_point, pair(&w["a_point"]).unwrap()), "case {n}");
            assert!(close_pair(link.b_point, pair(&w["b_point"]).unwrap()), "case {n}");
        }
        for (key, want) in case["paths"].as_object().unwrap() {
            let (a, b) = key.split_once('-').unwrap();
            let steps = layout.path(a.parse().unwrap(), b.parse().unwrap());
            let got: Vec<usize> = steps.iter().map(|s| layout.links.iter().position(|l| l == s).unwrap()).collect();
            let want: Vec<usize> = want.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
            assert_eq!(got, want, "case {n} path {key}");
        }
        for (q_index, q) in case["queries"].as_array().unwrap().iter().enumerate() {
            let (x, y, tx, ty, margin) = (f(&q["x"]), f(&q["y"]), f(&q["tx"]), f(&q["ty"]), f(&q["margin"]));
            let at = format!("case {n} query {q_index}");
            assert_eq!(layout.screen_at(x, y).index as u64, q["screen_at"].as_u64().unwrap(), "{at}");
            assert!(close_pair(layout.route(x, y, tx, ty), pair(&q["route"]).unwrap()), "{at} route");
            assert!(close_pair(layout.clamp(x, y, margin), pair(&q["clamp"]).unwrap()), "{at} clamp");
            assert_eq!(layout.playfield.contains(x, y, margin), q["contains"].as_bool().unwrap(), "{at}");
            let hit = layout.tunnel_at(x, y, TUNNEL_REACH * 20.0);
            match (hit, q["tunnel_at"].as_array()) {
                (None, None) => {}
                (Some((link, far)), Some(w)) => {
                    let index = layout.links.iter().position(|l| *l == link).unwrap();
                    assert_eq!(index as u64, w[0].as_u64().unwrap(), "{at}");
                    assert!(close_pair(far, pair(&w[1]).unwrap()), "{at}");
                }
                other => panic!("{at}: tunnel_at {other:?}"),
            }
        }
    }
}

#[test]
fn playfield_clamps_match_python() {
    let cases = load("playfield.json");
    for (n, case) in cases.as_array().unwrap().iter().enumerate() {
        let field = Playfield::new(f(&case["width"]), f(&case["height"]), &rects(&case["rects"]));
        assert_eq!(field.simple(), case["simple"].as_bool().unwrap(), "case {n}");
        for p in case["points"].as_array().unwrap() {
            let (x, y, margin) = (f(&p["x"]), f(&p["y"]), f(&p["margin"]));
            assert_eq!(field.contains(x, y, margin), p["contains"].as_bool().unwrap(), "case {n} {x},{y}");
            assert!(close_pair(field.clamp(x, y, margin), pair(&p["clamp"]).unwrap()), "case {n} {x},{y}");
        }
    }
}
