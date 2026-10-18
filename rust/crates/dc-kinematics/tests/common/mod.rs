//! Shared helpers for the dc-kinematics integration tests: fixture loading,
//! building a `Kinematics` from a fixture, and the JSON <-> context mapping.
#![allow(dead_code)]

use dc_core::PyRandom;
use dc_kinematics::core::merge_json;
use dc_kinematics::{DrawCtx, FrameCtx, GaitStyle, HeldInput, Kinematics, MotionState, Rig};
use serde_json::Value;
use std::path::PathBuf;

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

pub fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    serde_json::from_str(&text).unwrap()
}

pub fn gait_style(v: &Value) -> GaitStyle {
    match v.as_str().unwrap_or("classic") {
        "lively" => GaitStyle::Lively,
        "skitter" => GaitStyle::Skitter,
        _ => GaitStyle::Classic,
    }
}

/// A `Kinematics` in the fixture's initial state.
pub fn build(fx: &Value) -> Kinematics {
    let rig = Rig::from_model_json(&fx["model"], &fx["personality"], true);
    let gait_rng = PyRandom::from_str_seed(fx["gait_seed"].as_str().unwrap());
    let mut k = Kinematics::new(rig, gait_style(&fx["gait_style"]), gait_rng, 0.0, 0.0, 0.0, 28.0);
    let groups: Vec<i64> = fx["gait_groups"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as i64).collect();
    assert_eq!(k.gait_groups, groups, "gait groups");
    k.apply_json_overrides(&fx["init"]).unwrap();
    k
}

/// The running per-frame context, kept as JSON and merged with each delta.
#[derive(Default)]
pub struct CtxState {
    pub json: serde_json::Map<String, Value>,
}

impl CtxState {
    pub fn merge(&mut self, delta: &Value) {
        if let Some(obj) = delta.as_object() {
            for (k, v) in obj {
                self.json.insert(k.clone(), v.clone());
            }
        }
    }

    fn f(&self, key: &str) -> f64 {
        self.json.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    }
    fn b(&self, key: &str) -> bool {
        self.json.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn frame_ctx(&self) -> FrameCtx<'static> {
        FrameCtx {
            state: MotionState::from_name(self.json.get("state").and_then(Value::as_str).unwrap_or("Idle")),
            fleeing: self.b("fleeing"),
            acts_as_observer: self.b("acts_as_observer"),
            acts_as_drifter: self.b("acts_as_drifter"),
            reverse_walk: self.b("reverse_walk"),
            has_job_facing: self.b("has_job_facing"),
            airborne: self.b("airborne"),
            dragging: self.b("dragging"),
            webbed_held: self.b("webbed_held"),
            startled_timer: self.f("startled_timer"),
            drift_run_sliding: self.b("drift_run_sliding"),
            held_release_timer: self.f("held_release_timer"),
            focus_strength: self.f("focus_strength"),
            focus_x: self.f("focus_x"),
            focus_y: self.f("focus_y"),
            held: HeldInput {
                drag_response: self.f("held_drag_response"),
                drag_accel_x: self.f("held_drag_accel_x"),
                drag_accel_y: self.f("held_drag_accel_y"),
                pose_clock: self.f("held_pose_clock"),
                drag_sway_x: self.f("held_drag_sway_x"),
                drag_sway_y: self.f("held_drag_sway_y"),
                leg_relax: self.f("held_leg_relax"),
                drag_vel_x: self.f("drag_vel_x"),
                drag_vel_y: self.f("drag_vel_y"),
            },
            screen_w: self.f("screen_w"),
            screen_h: self.f("screen_h"),
            margin: self.f("margin"),
            playfield: None,
        }
    }
}

pub fn draw_ctx(v: &Value) -> DrawCtx {
    let pair = |key: &str| {
        let a = v[key].as_array().unwrap();
        (a[0].as_f64().unwrap(), a[1].as_f64().unwrap())
    };
    let f = |key: &str| v[key].as_f64().unwrap();
    DrawCtx {
        combat_body_offset: pair("combat_body_offset"),
        combat_stance: f("combat_stance"),
        strike_push: f("strike_push"),
        strike_lift: f("strike_lift"),
        strike_face: pair("strike_face"),
        combat_face_x: f("combat_face_x"),
        combat_face_y: f("combat_face_y"),
        feed_paw: f("feed_paw"),
        jump_z: f("jump_z"),
        jump_peak: f("jump_peak"),
    }
}

/// Compare `actual` against `expected`, recursing into every key `expected`
/// has. Numbers compare within `tol * max(1, |expected|)`. Returns the
/// largest numeric deviation seen and the first mismatch, if any.
pub fn compare(path: &str, expected: &Value, actual: &Value, tol: f64, worst: &mut f64) -> Option<String> {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (k, ev) in e {
                let Some(av) = a.get(k) else {
                    return Some(format!("{path}.{k}: missing in Rust snapshot"));
                };
                if let Some(m) = compare(&format!("{path}.{k}"), ev, av, tol, worst) {
                    return Some(m);
                }
            }
            None
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Some(format!("{path}: length {} vs {}", e.len(), a.len()));
            }
            for (i, (ev, av)) in e.iter().zip(a).enumerate() {
                if let Some(m) = compare(&format!("{path}[{i}]"), ev, av, tol, worst) {
                    return Some(m);
                }
            }
            None
        }
        (Value::Number(e), Value::Number(a)) => {
            let (e, a) = (e.as_f64().unwrap(), a.as_f64().unwrap());
            let dev = (e - a).abs();
            if dev > *worst {
                *worst = dev;
            }
            if dev > tol * e.abs().max(1.0) {
                return Some(format!("{path}: expected {e:?}, got {a:?} (diff {dev:e})"));
            }
            None
        }
        (e, a) => {
            if e == a {
                None
            } else {
                Some(format!("{path}: expected {e}, got {a}"))
            }
        }
    }
}

pub fn merge(target: &mut Value, patch: &Value) {
    merge_json(target, patch);
}

pub fn pt(v: &Value) -> (f64, f64) {
    let a = v.as_array().unwrap();
    (a[0].as_f64().unwrap(), a[1].as_f64().unwrap())
}

/// Load a model + personality pair for a Rust-native test from the stance
/// fixture (the tarantula and `balanced`).
pub fn stance_rig(curved: bool) -> Rig {
    let fx = load("stance_chains.json");
    Rig::from_model_json(&fx["model"], &fx["personality"], curved)
}
