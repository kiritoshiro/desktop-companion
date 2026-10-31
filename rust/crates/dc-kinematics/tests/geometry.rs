//! Leg geometry: the chain-solve parity against hand-set Python stances, and
//! Rust ports of `test_leg_symmetry.py`, the geometry of `test_curved_legs.py`
//! and `test_leg_detail.py`, `test_held_pose.py`, plus the CPython numeric
//! oracle (`math.hypot`, `sum`).

mod common;

use common::*;
use dc_core::PyRandom;
use dc_kinematics::detail::{hair_detail, hair_detail_for, hair_pen_width, joint_node_shows};
use dc_kinematics::pymath::{hypot, py_sum};
use dc_kinematics::{FrameCtx, GaitStyle, Kinematics};
use std::collections::HashMap;

const HEADINGS: [f64; 7] = [0.0, 30.0, 45.0, 90.0, 135.0, 180.0, 270.0];
const PAIRS: [&str; 4] = ["front", "mid_front", "mid_rear", "rear"];

#[test]
fn hypot_and_sum_are_cpython_exact() {
    let fx = load("hypot.json");
    let mut mismatches = 0;
    for row in fx["hypot"].as_array().unwrap() {
        let (a, b, want) = (row[0].as_f64().unwrap(), row[1].as_f64().unwrap(), row[2].as_f64().unwrap());
        if hypot(a, b).to_bits() != want.to_bits() {
            mismatches += 1;
        }
    }
    assert_eq!(mismatches, 0, "math.hypot mismatches");
    for row in fx["sum"].as_array().unwrap() {
        let values: Vec<f64> = row[0].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(py_sum(values.iter().copied()).to_bits(), row[1].as_f64().unwrap().to_bits(), "{values:?}");
    }
}

/// `test_leg_symmetry.py::_standing`: every foot on its own rest pose.
fn standing(curved: bool, heading_deg: f64, scale: f64) -> Kinematics {
    let rig = stance_rig(curved);
    let size = 28.0 * scale.clamp(0.45, 2.25);
    let mut k =
        Kinematics::new(rig, GaitStyle::Lively, PyRandom::from_int(1), 800.0, 450.0, heading_deg.to_radians(), size);
    let [fx, fy, rx, ry] = k.basis();
    for i in 0..k.legs.len() {
        let d = k.rig.legs[i].clone();
        let rf = d.rest_forward.unwrap() * k.size;
        let rs = d.rest_side.unwrap() * k.size * d.side_sign;
        k.legs[i].foot_x = k.x + fx * rf + rx * rs;
        k.legs[i].foot_y = k.y + fy * rf + ry * rs;
        k.legs[i].lift = 0.0;
    }
    k
}

fn chain(k: &Kinematics, i: usize) -> Vec<(f64, f64)> {
    let ctx = FrameCtx::new(1600.0, 900.0);
    let (ax, ay) = k.leg_attach(i);
    let leg = k.legs[i];
    k.sprite_leg_chain_points(i, ax, ay, leg.foot_x, leg.foot_y, &ctx).unwrap().as_slice().to_vec()
}

#[test]
fn stance_chains_match_python() {
    let fx = load("stance_chains.json");
    let mut worst = 0.0;
    for case in fx["cases"].as_array().unwrap() {
        let curved = case["curved"].as_bool().unwrap();
        let mut k = standing(curved, case["heading_deg"].as_f64().unwrap(), case["scale"].as_f64().unwrap());
        assert_eq!(k.size, case["size"].as_f64().unwrap());
        k.heading = case["heading"].as_f64().unwrap();
        k.body_sway = case["body_sway"].as_f64().unwrap();
        for (i, leg) in case["legs"].as_array().unwrap().iter().enumerate() {
            let (fx, fy) = pt(&leg["foot"]);
            k.legs[i].foot_x = fx;
            k.legs[i].foot_y = fy;
            let bends: Vec<f64> = leg["joint_bends"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            k.legs[i].joint_bends.set(&bends);
            k.legs[i].phase_seed = leg["phase_seed"].as_f64().unwrap();
        }
        for (i, leg) in case["legs"].as_array().unwrap().iter().enumerate() {
            let got = serde_json::to_value(chain(&k, i).iter().map(|p| [p.0, p.1]).collect::<Vec<_>>()).unwrap();
            if let Some(m) = compare("chain", &leg["chain"], &got, 1e-12, &mut worst) {
                panic!("curved {curved} heading {} leg {i}: {m}", case["heading_deg"]);
            }
        }
    }
    println!("stance chains: max |diff| {worst:e}");
}

/// `_outboard`: how far the first joint sits outside its socket, in body widths.
fn outboard(k: &Kinematics, i: usize) -> f64 {
    let p = chain(k, i);
    let (_, root_s) = k.world_to_body_local(p[0].0, p[0].1);
    let (_, joint_s) = k.world_to_body_local(p[1].0, p[1].1);
    (joint_s - root_s) * k.side_sign(i) / k.size
}

fn by_name<T>(k: &Kinematics, f: impl Fn(usize) -> T) -> HashMap<String, T> {
    (0..k.legs.len()).map(|i| (k.rig.legs[i].name.clone(), f(i))).collect()
}

#[test]
fn no_leg_folds_its_first_joint_under_the_shell_and_flanks_match() {
    for heading in HEADINGS {
        let k = standing(true, heading, 3.0);
        let bows = by_name(&k, |i| outboard(&k, i));
        for (name, bow) in &bows {
            assert!(*bow >= 0.0, "{name} at {heading} folds inboard");
            // DC-69: a knee fold, not a splay.
            assert!(0.05 < *bow && *bow < 0.40, "{name} at {heading} bows {bow}");
        }
        for stem in PAIRS {
            let (l, r) = (bows[&format!("{stem}_left")], bows[&format!("{stem}_right")]);
            assert!((l - r).abs() < 0.01, "{stem} at {heading}: {l} vs {r}");
        }
    }
}

#[test]
fn the_pose_does_not_know_which_way_it_is_walking() {
    let pose = |heading: f64| {
        let k = standing(true, heading, 3.0);
        let mut out = Vec::new();
        for i in 0..k.legs.len() {
            for (px, py) in chain(&k, i) {
                let (f, s) = k.world_to_body_local(px, py);
                out.push((f / k.size, s / k.size));
            }
        }
        out
    };
    let reference = pose(HEADINGS[0]);
    for heading in HEADINGS {
        for (a, b) in reference.iter().zip(pose(heading)) {
            assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01, "heading {heading}");
        }
    }
    let k = standing(true, 0.0, 3.0);
    let min_bow = (0..k.legs.len()).map(|i| outboard(&k, i)).fold(f64::MAX, f64::min);
    assert!(min_bow > 0.05, "zero bow would be a starfish");
}

/// `test_curved_legs.py::_shape`: (femur bearing, distal bearing, knee fraction).
fn shape(curved: bool, heading: f64) -> HashMap<String, (f64, f64, f64)> {
    let k = standing(curved, heading, 1.0);
    by_name(&k, |i| {
        let side = k.side_sign(i);
        let local: Vec<(f64, f64)> = chain(&k, i).iter().map(|p| k.world_to_body_local(p.0, p.1)).collect();
        let bearing = |p: (f64, f64), q: (f64, f64)| ((q.1 - p.1) * side).atan2(q.0 - p.0).to_degrees();
        let lengths: Vec<f64> = local.windows(2).map(|w| hypot(w[1].0 - w[0].0, w[1].1 - w[0].1)).collect();
        let total: f64 = lengths.iter().sum();
        (bearing(local[0], local[1]), bearing(local[2], local[local.len() - 1]), (lengths[0] + lengths[1]) / total)
    })
}

#[test]
fn front_legs_curve_forward_and_rear_legs_curve_back() {
    let s = shape(true, 0.0);
    for stem in ["front", "mid_front"] {
        let (femur, distal, _) = s[&format!("{stem}_right")];
        assert!(distal < femur - 5.0, "{stem}: {femur} {distal}");
    }
    for stem in ["mid_rear", "rear"] {
        let (femur, distal, _) = s[&format!("{stem}_right")];
        assert!(distal > femur + 5.0, "{stem}: {femur} {distal}");
    }
    let turn: HashMap<&str, f64> = PAIRS
        .iter()
        .map(|stem| {
            let (f, d, _) = s[&format!("{stem}_right")];
            (*stem, (d - f).abs())
        })
        .collect();
    assert!(turn["front"].min(turn["rear"]) > turn["mid_front"].max(turn["mid_rear"]), "{turn:?}");
    assert!(25.0 < turn["front"] && turn["front"] < 60.0 && 25.0 < turn["rear"] && turn["rear"] < 60.0, "{turn:?}");
    assert!(s["rear_right"].1 < 179.0);
    assert!(s["mid_rear_right"].0 - s["mid_front_right"].0 > 15.0);
    for stem in PAIRS {
        let (l, r) = (s[&format!("{stem}_left")], s[&format!("{stem}_right")]);
        assert!((l.0 - r.0).abs() < 0.01 && (l.1 - r.1).abs() < 0.01 && (l.2 - r.2).abs() < 0.01, "{stem}");
    }
}

#[test]
fn the_knee_sits_nearer_the_body_and_the_curve_is_heading_free() {
    let curved = shape(true, 0.0);
    for (name, (_, _, knee)) in &curved {
        assert!(*knee < 0.50, "{name}: {knee}");
    }
    let straight = shape(false, 0.0);
    let sum = |s: &HashMap<String, (f64, f64, f64)>| s.values().map(|v| v.2).sum::<f64>();
    assert!(sum(&curved) < sum(&straight));
    for stem in PAIRS {
        let (f, d, _) = straight[&format!("{stem}_right")];
        assert!((d - f).abs() < 12.0, "curve off: {stem} {f} {d}");
    }
    for heading in [30.0, 90.0, 135.0, 200.0, 270.0] {
        let turned = shape(true, heading);
        for (name, a) in &curved {
            let b = turned[name];
            assert!(
                (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && (a.2 - b.2).abs() < 0.01,
                "{name} {heading}"
            );
        }
    }
}

#[test]
fn only_the_tarantula_arcs_and_a_bad_leg_curve_disables_the_chain() {
    let rig = stance_rig(true);
    let chain = rig.leg_chain.as_ref().unwrap();
    assert!(chain.elevated_arc > 0.0);
    assert_eq!(chain.leg_curve, 35.0);
    assert_eq!(chain.segment_count, 5);
    // A string where a number belongs: Python's float("35") accepts it, the
    // validator (dc-content) is what rejects it; the config itself tolerates it.
    let mut model = load("stance_chains.json")["model"].clone();
    model["appearance"]["leg_chain"]["segment_count"] = serde_json::json!(6);
    let bad = dc_kinematics::Rig::from_model_json(&model, &serde_json::json!({}), true);
    assert!(bad.leg_chain.is_none());
}

// --------------------------------------------------------- test_leg_detail.py

#[test]
fn joint_nodes_and_hair_follow_the_drawn_width() {
    assert!(!joint_node_shows(1.66, &[3.42, 2.96], true));
    assert!(joint_node_shows(3.00, &[3.42, 2.96], true));
    assert!(joint_node_shows(2.2, &[9.0, 3.0], true));
    assert!(!joint_node_shows(2.2, &[9.0, 9.0], true));
    assert!(joint_node_shows(1.0, &[], true));
    assert!(joint_node_shows(0.1, &[9.0], false));

    let thin = hair_detail_for(1.6, 0.42);
    assert!(thin.merge_hair && !thin.separate_hair && thin.strokes_per_segment() == 1);
    let thick = hair_detail_for(5.8, 0.42);
    assert!(thick.separate_hair && !thick.merge_hair && thick.strokes_per_segment() == 2);
    let smooth = hair_detail(5.8, None, true);
    assert!(!smooth.separate_hair && !smooth.merge_hair && smooth.strokes_per_segment() == 1);
    assert!((hair_pen_width(3.0, 0.42) - 3.0 * (1.12 + 0.42 * 0.55)).abs() < 1e-12);
    assert!(hair_pen_width(3.0, 0.42) > 3.0);

    let size = 24.5;
    let resting = 1.4f64.max(size * 0.066);
    let level = hair_detail_for(resting, 0.42);
    assert!(level.merge_hair);
    for lift in [0.0, 0.5, 1.0] {
        for startle in [0.0, 1.0] {
            let live = 1.4f64.max(size * 0.066 * (1.0 + lift * 0.24)) * (1.0 + startle * 0.10);
            assert!(live >= resting);
            assert_eq!(hair_detail_for(live, 0.42), level);
        }
    }
}

#[test]
fn the_tarantulas_joints_never_protrude_at_any_size_it_reaches() {
    let rig = stance_rig(true);
    let chain = rig.leg_chain.as_ref().unwrap();
    // `leg_thickness` and `joint_node_scale` from the tarantula's appearance
    // (render values, not part of the kinematic rig).
    let (thickness, node_scale) = (1.9f64, 1.4f64);
    let mut seen = 0;
    for scale in [0.9f64, 1.5, 2.2, 3.0, 4.5] {
        let size = 28.0 * scale.clamp(0.45, 2.25);
        let base_width = 1.4f64.max(size * 0.066 * thickness);
        let node_r = 1.0f64.max(base_width * 0.24 * node_scale);
        let widths: Vec<f64> = chain.width_scales.iter().map(|w| base_width * w).collect();
        for (index, joint_scale) in chain.joint_scales.iter().enumerate() {
            let radius = node_r * joint_scale * if index == 1 { 1.12 } else { 0.96 };
            assert!(!joint_node_shows(radius, &widths[index..index + 2], true), "size {size} joint {index}");
            seen += 1;
        }
    }
    assert_eq!(seen, 20);
    let detail = hair_detail(1.4f64.max(28.0 * 0.066 * thickness), Some(chain), true);
    assert!(detail.merge_hair);
    assert_eq!(chain.width_scales.len() * detail.strokes_per_segment(), 5);
}

// ----------------------------------------------------------- test_held_pose.py

fn carried() -> (Kinematics, FrameCtx<'static>) {
    let rig = stance_rig(true);
    let mut k = Kinematics::new(rig, GaitStyle::Classic, PyRandom::from_int(3), 600.0, 400.0, 0.0, 28.0);
    let mut rng = PyRandom::from_int(3);
    let ctx = FrameCtx::new(1200.0, 800.0);
    k.seed_legs(&mut rng);
    k.initialize_legs(&ctx, &mut rng);
    (k, FrameCtx { dragging: true, ..ctx })
}

fn chain_bend(k: &Kinematics, ctx: &FrameCtx) -> f64 {
    let mut total = 0.0;
    for i in 0..k.legs.len() {
        let (ax, ay) = k.leg_attach(i);
        let (fx, fy) = k.visual_foot_for_render(i, ctx);
        let pts = k.sprite_leg_chain_points(i, ax, ay, fx, fy, ctx).unwrap();
        let span = hypot(fx - ax, fy - ay);
        if span < 1e-6 {
            continue;
        }
        let p = pts.as_slice();
        for &(px, py) in &p[1..p.len() - 1] {
            total += ((fx - ax) * (ay - py) - (ax - px) * (fy - ay)).abs() / span;
        }
    }
    total
}

#[test]
fn a_faster_hand_bends_the_carried_legs_a_little_more() {
    let (mut k, ctx) = carried();
    k.current_speed = 0.0;
    let still = chain_bend(&k, &ctx);
    assert!(still > 0.0);
    k.current_speed = 400.0;
    let fast = chain_bend(&k, &ctx);
    assert!(fast > still, "{still} {fast}");
    assert!(fast <= still * 1.35, "{still} {fast}");
}
