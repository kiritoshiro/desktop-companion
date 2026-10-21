//! Rust ports of the Python locomotion tests: `tests/movement.py` with
//! `test_spider_movement.py` and `test_tarantula.py` (walk limits, causality,
//! heading filter, quick turn, roll recovery, pivot), `test_tarantula_stride.py`,
//! `test_turn_movement.py` (the kinematic half), `test_drifter.py`,
//! `test_throw_slide.py` and the gait-phase half of
//! `test_gait_phases_and_growth.py`.
//!
//! Creatures start from the exact state `build_creature` produced in Python
//! (each fixture's `init`); the walks then use their own seeded stream, so
//! these check the properties the Python tests check rather than a trace.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_kinematics::legstate::ContactState;
use dc_kinematics::pymath::{hypot, wrap_pi};
use dc_kinematics::{FrameCtx, GaitStyle, Kinematics, MotionState, Rig};
use serde_json::Value;
use std::f64::consts::{FRAC_PI_2, PI};

const DT: f64 = 1.0 / 60.0;

/// `build_creature(model, personality)` as the fixture recorded it, plus its
/// first-frame context. `personality` replaces the fixture's personality
/// when given (a file under `personalities/`).
fn creature(fixture: &str, personality: Option<&str>) -> (Kinematics, FrameCtx<'static>) {
    let mut fx = load(&format!("{fixture}.json"));
    if let Some(p) = personality {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../personalities")
            .join(format!("{p}.json"));
        fx["personality"] = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    }
    let k = build(&fx);
    let mut ctx = CtxState::default();
    ctx.merge(&fx["frames"][0]["ctx"]);
    (k, ctx.frame_ctx())
}

fn advance_controller(k: &mut Kinematics, ctx: &FrameCtx, rng: &mut PyRandom) {
    k.update_spider_grounded_frame(DT, ctx, rng);
    k.spider_gait_frame_updated = false;
}

#[derive(Debug, Default)]
struct Walk {
    starts: usize,
    outside: usize,
    max_airborne: usize,
    planted_displacements: usize,
    max_chain_stretch: f64,
    max_segment_ratio: f64,
    max_pose_jump: f64,
    max_heading_jump: f64,
    min_supports: usize,
    max_joint_bend_range: f64,
    max_joint_motion_spread: f64,
}

/// `tests/movement.py::run_walk`.
fn run_walk(
    base: &Kinematics,
    ctx: &FrameCtx,
    seconds: f64,
    speed: f64,
    turn_rate: f64,
    dt: f64,
    seed: i128,
) -> (Kinematics, Walk) {
    let mut k = base.clone();
    let mut rng = PyRandom::from_int(seed);
    let cfg = k.rig.spider_gait.unwrap();
    k.speed = speed;
    if turn_rate.abs() > 1e-6 {
        k.target_x = k.x;
        k.target_y = k.y;
        k.turn_rate = turn_rate.abs();
        k.target_heading = FRAC_PI_2.copysign(turn_rate);
    }
    let n = k.legs.len();
    let mut w = Walk { min_supports: n, ..Default::default() };
    let mut previous_bends: Option<Vec<Vec<f64>>> = None;
    let frames = ((seconds / dt).round() as usize).max(1);
    for _ in 0..frames {
        if turn_rate.abs() > 1e-6 {
            k.target_x = k.x;
            k.target_y = k.y;
            k.target_heading = FRAC_PI_2.copysign(turn_rate);
        }
        let old_pose = (k.x, k.y, k.heading);
        let old_contacts: Vec<Option<(f64, f64)>> = k
            .legs
            .iter()
            .map(|l| (l.contact_state == ContactState::Stance && !l.airborne()).then_some((l.foot_x, l.foot_y)))
            .collect();
        let was_stepping: Vec<bool> = k.legs.iter().map(|l| l.airborne()).collect();

        k.update_spider_grounded_frame(dt, ctx, &mut rng);
        k.spider_gait_frame_updated = false;

        w.max_pose_jump = w.max_pose_jump.max(hypot(k.x - old_pose.0, k.y - old_pose.1));
        w.max_heading_jump = w.max_heading_jump.max(wrap_pi(k.heading - old_pose.2).abs());
        w.min_supports = w
            .min_supports
            .min(k.legs.iter().filter(|l| l.contact_state == ContactState::Stance && !l.stepping).count());
        w.max_airborne = w.max_airborne.max(k.legs.iter().filter(|l| l.airborne()).count());
        for i in 0..n {
            let leg = k.legs[i];
            if let Some((ox, oy)) = old_contacts[i]
                && !was_stepping[i]
                && leg.contact_state == ContactState::Stance
                && !leg.stepping
            {
                w.planted_displacements += usize::from(hypot(leg.foot_x - ox, leg.foot_y - oy) > 1e-6);
                let (vx, vy) = k.visual_foot_for_render(i, ctx);
                w.planted_displacements += usize::from(hypot(vx - leg.foot_x, vy - leg.foot_y) > 1e-6);
            }
            if !was_stepping[i] && leg.stepping {
                w.starts += 1;
                let window = k.spider_phase_window(i, leg.last_step_phase, &cfg);
                if !window.in_window && !leg.last_step_emergency {
                    w.outside += 1;
                }
            }
            if let Some(chain) = k.rig.leg_chain.clone() {
                let (ax, ay) = k.leg_attach(i);
                let (fx, fy) = k.visual_foot_for_render(i, ctx);
                let pts = k.sprite_leg_chain_points(i, ax, ay, fx, fy, ctx).unwrap();
                let p = pts.as_slice();
                let direct = hypot(p[p.len() - 1].0 - p[0].0, p[p.len() - 1].1 - p[0].1);
                let path: f64 = (0..p.len() - 1).map(|j| hypot(p[j + 1].0 - p[j].0, p[j + 1].1 - p[j].1)).sum();
                if direct > 1e-6 {
                    w.max_chain_stretch = w.max_chain_stretch.max(path / direct);
                }
                let d = &k.rig.legs[i];
                let upper = (k.size * 0.22).max(d.upper_len_or() * k.size);
                let lower = (k.size * 0.22).max(d.lower_len_or() * k.size);
                let total: f64 = chain.segment_lengths.iter().sum::<f64>().max(1e-4);
                for (s, weight) in chain.segment_lengths.iter().enumerate() {
                    let limit = (upper + lower) * chain.max_stretch * weight / total;
                    let length = hypot(p[s + 1].0 - p[s].0, p[s + 1].1 - p[s].1);
                    w.max_segment_ratio = w.max_segment_ratio.max(length / limit.max(1e-4));
                }
            }
        }
        let bends: Vec<Vec<f64>> = k.legs.iter().map(|l| l.joint_bends.as_slice().to_vec()).collect();
        for b in &bends {
            if !b.is_empty() {
                let max = b.iter().cloned().fold(f64::MIN, f64::max);
                let min = b.iter().cloned().fold(f64::MAX, f64::min);
                w.max_joint_bend_range = w.max_joint_bend_range.max(max - min);
            }
        }
        if let Some(prev) = &previous_bends {
            for (before, after) in prev.iter().zip(&bends) {
                let deltas: Vec<f64> = before.iter().zip(after).map(|(p, c)| (c - p).abs()).collect();
                if !deltas.is_empty() {
                    let max = deltas.iter().cloned().fold(f64::MIN, f64::max);
                    let min = deltas.iter().cloned().fold(f64::MAX, f64::min);
                    w.max_joint_motion_spread = w.max_joint_motion_spread.max(max - min);
                }
            }
        }
        previous_bends = Some(bends);
    }
    (k, w)
}

// ------------------------------------------------- tests/movement.py checks

fn causality(base: &Kinematics, ctx: &FrameCtx) {
    let mut rng = PyRandom::from_int(19);
    let mut frozen = base.clone();
    frozen.target_x = frozen.x;
    frozen.target_y = frozen.y;
    frozen.speed = 0.0;
    frozen.current_speed = 0.0;
    let (h0, p0) = (frozen.heading, (frozen.x, frozen.y));
    for _ in 0..30 {
        frozen.update_spider_grounded_locomotion(DT, ctx, &mut rng);
    }
    assert!(hypot(frozen.x - p0.0, frozen.y - p0.1) < 1e-6);
    assert!(wrap_pi(frozen.heading - h0).abs() < 1e-6);

    let mut swing = base.clone();
    swing.target_x = swing.x;
    swing.target_y = swing.y;
    swing.speed = 0.0;
    swing.current_speed = 0.0;
    let (fx, fy) = (swing.legs[0].foot_x, swing.legs[0].foot_y);
    swing.schedule_step(0, fx + 5.0, fy, 0.0, false, ctx, &mut rng);
    let before = (swing.x, swing.y, swing.heading);
    for _ in 0..8 {
        swing.update_spider_grounded_locomotion(DT, ctx, &mut rng);
        swing.advance_active_steps(DT, ctx, &mut rng);
    }
    assert!(hypot(swing.x - before.0, swing.y - before.1) < 1e-6);
    assert!(wrap_pi(swing.heading - before.2).abs() < 1e-6);
}

fn heading_filter(base: &Kinematics, ctx: &FrameCtx) {
    let mut rng = PyRandom::from_int(19);
    let mut k = base.clone();
    let ctx = FrameCtx { state: MotionState::Chase, ..*ctx };
    k.speed = 0.0;
    k.current_speed = 0.0;
    k.spider_heading_filter = k.heading;
    let mut previous = k.target_heading;
    let mut max_target_step = 0.0f64;
    let mut values = Vec::new();
    for frame in 0..180 {
        let raw: f64 = if frame % 2 == 1 { 0.78 } else { -0.78 };
        k.target_x = k.x + raw.cos() * 500.0;
        k.target_y = k.y + raw.sin() * 500.0;
        k.spider_locomotion_intent(DT, &ctx, &mut rng);
        max_target_step = max_target_step.max(wrap_pi(k.target_heading - previous).abs());
        values.push(k.target_heading);
        previous = k.target_heading;
    }
    let settled = values[values.len() - 60..].iter().map(|v| v.abs()).fold(0.0, f64::max);
    assert!(max_target_step < 0.20, "{max_target_step}");
    assert!(settled < 0.13, "{settled}");
    k.target_x = k.x + 0.78f64.cos() * 500.0;
    k.target_y = k.y + 0.78f64.sin() * 500.0;
    for _ in 0..30 {
        k.spider_locomotion_intent(DT, &ctx, &mut rng);
    }
    let sustained = wrap_pi(k.target_heading - 0.78).abs();
    assert!(sustained < 0.14, "{sustained}");
}

fn quick_turn(base: &Kinematics, ctx: &FrameCtx) {
    let mut rng = PyRandom::from_int(19);
    let mut k = base.clone();
    k.speed = 0.0;
    k.current_speed = 0.0;
    k.target_heading = 0.0;
    let (mut max_step, mut reversals, mut previous_step) = (0.0f64, 0, 0.0);
    for frame in 0..90 {
        let angle: f64 = if frame < 8 { 0.0 } else { FRAC_PI_2 };
        k.target_x = k.x + angle.cos() * 500.0;
        k.target_y = k.y + angle.sin() * 500.0;
        k.target_heading = angle;
        let old = k.heading;
        k.update_spider_grounded_frame(DT, ctx, &mut rng);
        let step = wrap_pi(k.heading - old);
        max_step = max_step.max(step.abs());
        if step.abs() > 1e-5 && previous_step * step < -1e-5 {
            reversals += 1;
        }
        previous_step = step;
    }
    let final_error = wrap_pi(k.heading - FRAC_PI_2).abs();
    assert!(max_step < 0.12, "{max_step}");
    assert_eq!(reversals, 0);
    assert!(final_error < 0.08, "{final_error}");
}

/// `run_roll_recovery_check`, with `enter_roll` (FSM) reduced to the fields
/// it hands the kinematics.
fn roll_recovery(base: &Kinematics, ctx: &FrameCtx) {
    let mut rng = PyRandom::from_int(19);
    let mut k = base.clone();
    k.current_speed = 130.0;
    k.vel_x = 130.0;
    // enter_roll(direction=0.0)
    k.motion_paused = true;
    k.speed = 0.0;
    k.current_speed = 0.0;
    k.vel_x = 0.0;
    k.vel_y = 0.0;
    k.rolling = true;
    k.roll_dir = -1.0;
    k.roll_total = std::f64::consts::TAU;
    k.roll_duration = 0.9;
    k.roll_progress = 0.0;
    k.roll_eased = 0.0;
    k.roll_distance = k.size * 1.4;
    k.roll_drift_dir = 0.0;
    let roll_ctx = FrameCtx { state: MotionState::Roll, ..*ctx };
    let mut finished = false;
    for _ in 0..180 {
        if k.update_roll(DT, &roll_ctx) {
            finished = true;
            break;
        }
        k.update_legs(DT, &roll_ctx, &mut rng);
        assert!(k.roll_tuck >= 0.0 && k.roll_tuck <= 1.0);
    }
    assert!(finished);
    assert_eq!(k.roll_spin, 0.0);
    assert_eq!(k.roll_tuck, 0.0);
    assert_eq!(k.current_speed, 0.0);
    assert!(k.vel_x == 0.0 && k.vel_y == 0.0);
    assert!(!k.rolling && !k.motion_paused);
    assert!(!k.legs.iter().any(|l| l.airborne()));
    for i in 0..k.legs.len() {
        let (fx, fy) = (k.legs[i].foot_x, k.legs[i].foot_y);
        assert!(!k.leg_reach_metrics(i, fx, fy, false).very_far);
    }
    // A roll cut short by something else is reconciled the same way.
    let mut cut = base.clone();
    cut.rolling = true;
    cut.roll_spin = 4.0;
    cut.roll_tuck = 0.7;
    assert!(cut.reconcile_roll(ctx));
    assert_eq!(cut.roll_spin, 0.0);
    assert!(!cut.reconcile_roll(ctx));
}

// ------------------------------------------------- test_spider_movement.py

fn snowpuff() -> (Kinematics, FrameCtx<'static>) {
    creature("snowpuff_walk", None)
}

#[test]
fn snowpuff_motion_is_leg_first() {
    let (k, ctx) = snowpuff();
    causality(&k, &ctx);
}

#[test]
fn snowpuff_heading_filter_settles() {
    let (k, ctx) = snowpuff();
    heading_filter(&k, &ctx);
}

#[test]
fn snowpuff_legs_replant_after_a_roll() {
    let (k, ctx) = snowpuff();
    roll_recovery(&k, &ctx);
}

#[test]
fn snowpuff_quick_turn_stays_bounded() {
    let (k, ctx) = snowpuff();
    quick_turn(&k, &ctx);
}

#[test]
fn snowpuff_walk_stays_inside_its_limits() {
    let (k, ctx) = snowpuff();
    let cfg = k.rig.spider_gait.unwrap();
    let (_, r) = run_walk(&k, &ctx, 8.0, 80.0, 0.0, DT, 19);
    assert!(r.starts > 0);
    assert!(r.max_airborne <= cfg.max_airborne, "{r:?}");
    assert_eq!(r.planted_displacements, 0);
    assert_eq!(r.outside, 0, "{r:?}");
    assert!(r.max_chain_stretch <= 1.35, "{r:?}");
    assert!(r.max_segment_ratio <= 1.001, "{r:?}");
    assert!(r.min_supports >= k.legs.len() - cfg.max_airborne);
    // The body never teleports.
    assert!(r.max_pose_jump < 12.0f64.max(80.0 * 0.05));
    assert!(r.max_heading_jump < 0.16);
}

#[test]
fn snowpuff_turns_when_asked() {
    let (k, ctx) = snowpuff();
    let (out, _) = run_walk(&k, &ctx, 8.0, 80.0, 0.6, DT, 19);
    let final_turn = wrap_pi(out.heading).abs();
    assert!(final_turn > 0.50, "{final_turn}");
}

#[test]
fn snowpuff_walk_agrees_across_frame_rates() {
    let (k, ctx) = snowpuff();
    let (base, _) = run_walk(&k, &ctx, 8.0, 80.0, 0.0, DT, 19);
    for dt in [1.0 / 30.0, 1.0 / 120.0] {
        let (other, _) = run_walk(&k, &ctx, 8.0, 80.0, 0.0, dt, 19);
        assert!(hypot(other.x - base.x, other.y - base.y) < 32.0);
        assert!(wrap_pi(other.heading - base.heading).abs() < 0.35);
    }
}

// ----------------------------------------------------------- test_tarantula.py

fn tarantula() -> (Kinematics, FrameCtx<'static>) {
    creature("tarantula_walk", None)
}

#[test]
fn tarantula_causality_and_filter() {
    let (k, ctx) = tarantula();
    causality(&k, &ctx);
    let (hunter, hctx) = creature("tarantula_walk", Some("hunter"));
    heading_filter(&hunter, &hctx);
}

#[test]
fn tarantula_walks_stay_inside_their_limits() {
    let (k, ctx) = tarantula();
    let cfg = k.rig.spider_gait.unwrap();
    for (speed, turn_rate) in [(45.0, 0.0), (100.0, 0.0), (170.0, 0.0), (100.0, 1.1), (170.0, -1.1)] {
        let (out, r) = run_walk(&k, &ctx, 5.0, speed, turn_rate, DT, 19);
        let label = format!("speed {speed} turn {turn_rate}: {r:?}");
        assert!(r.starts > 0, "{label}");
        assert_eq!(r.outside, 0, "{label}");
        assert!(r.max_airborne <= cfg.max_airborne, "{label}");
        assert!(r.min_supports >= 5, "{label}");
        assert_eq!(r.planted_displacements, 0, "{label}");
        assert!(r.max_chain_stretch <= 1.30, "{label}");
        assert!(r.max_segment_ratio <= 1.001, "{label}");
        assert!(r.max_joint_bend_range > 0.08, "{label}");
        assert!(r.max_joint_motion_spread > 1e-5, "{label}");
        assert!(r.max_heading_jump < 0.12, "{label}");
        if turn_rate == 0.0 && speed >= 100.0 {
            let travelled = hypot(out.x - 900.0, out.y - 700.0);
            assert!(travelled > if speed < 150.0 { 300.0 } else { 650.0 }, "{label}: {travelled}");
        }
        if turn_rate != 0.0 {
            assert!(wrap_pi(out.heading).abs() > 0.50, "{label}");
        }
    }
}

#[test]
fn tarantula_pivots_promptly() {
    let (mut k, ctx) = tarantula();
    let cfg = k.rig.spider_gait.unwrap();
    let mut rng = PyRandom::from_int(19);
    k.speed = 0.0;
    k.turn_rate = 5.0;
    k.target_heading = FRAC_PI_2;
    let (mut early, mut launches, mut t45) = (0.0, 0, None);
    for frame in 0..96 {
        k.target_x = k.x;
        k.target_y = k.y;
        k.target_heading = FRAC_PI_2;
        let before = k.legs.iter().filter(|l| l.airborne()).count();
        advance_controller(&mut k, &ctx, &mut rng);
        let after = k.legs.iter().filter(|l| l.airborne()).count();
        launches += after.saturating_sub(before);
        if t45.is_none() && k.heading >= PI * 0.25 - 0.01 {
            t45 = Some((frame + 1) as f64 / 60.0);
        }
        if early == 0.0 && k.heading >= 1.25 {
            early = k.heading;
        }
    }
    assert!(early >= 1.25, "{early}");
    assert!(wrap_pi(k.heading).abs() > 1.40);
    assert!(t45.is_some_and(|t| t <= 0.75), "{t45:?}");
    assert!(launches <= 14, "{launches}");
    // Positive turns contract the positive-side (inside) leg.
    let s = k.size;
    let left = k.spider_turn_radial_adjustment(0, s, s, 0.10, &cfg);
    let right = k.spider_turn_radial_adjustment(1, s, s, 0.10, &cfg);
    assert!(left > 0.0 && right < 0.0, "{left} {right}");
}

#[test]
fn tarantula_first_joint_lifts_outboard_and_chain_has_six_points() {
    let (k, ctx) = tarantula();
    for i in 0..k.legs.len() {
        let (ax, ay) = k.leg_attach(i);
        let (fx, fy) = k.visual_foot_for_render(i, &ctx);
        let pts = k.sprite_leg_chain_points(i, ax, ay, fx, fy, &ctx).unwrap();
        assert_eq!(pts.len(), 6);
        let (_, root_s) = k.world_to_body_local(pts.as_slice()[0].0, pts.as_slice()[0].1);
        let (_, joint_s) = k.world_to_body_local(pts.as_slice()[1].0, pts.as_slice()[1].1);
        let side = k.side_sign(i);
        assert!(joint_s * side >= root_s * side - 0.25, "leg {i}");
    }
}

#[test]
fn tarantula_carried_legs_hang_below_and_spread() {
    let (mut k, ctx) = tarantula();
    let mut ctx = FrameCtx { dragging: true, ..ctx };
    ctx.held.leg_relax = 1.0;
    k.body_sway = 0.0;
    let draw = dc_kinematics::DrawCtx::default();
    let (ax, ay) = k.leg_attach(0);
    let (_, lifted_y) = k.picked_up_leg_pose(0, ax, ay, ax, ay, &ctx);
    assert!(lifted_y > k.y + k.size * 0.50);
    let (nfx, nfy) = k.visual_foot_for_render(0, &ctx);
    k.breath_phase = 0.0;
    let a = k.picked_up_leg_pose(0, ax, ay, nfx, nfy, &ctx);
    k.breath_phase = 9.0;
    let b = k.picked_up_leg_pose(0, ax, ay, nfx, nfy, &ctx);
    assert_eq!(a, b, "the held pose must not tremble with breathing");
    let mut xs = Vec::new();
    for i in 0..k.legs.len() {
        let (_, _, hx, hy) = k.leg_draw_points(i, &ctx, &draw);
        assert!(hy > k.y + k.size * 0.50, "leg {i}");
        assert!((hx - k.x).abs() > k.size * 0.15, "leg {i}");
        xs.push(hx);
        let (lax, lay) = k.leg_attach(i);
        assert!(hypot(hx - lax, hy - lay) < k.size * 1.55, "leg {i}");
    }
    let spread = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    assert!(spread > k.size * 1.60, "{spread}");
    let slow = k.picked_up_leg_pose(0, ax, ay, nfx, nfy, &ctx);
    ctx.held.drag_vel_x = 520.0;
    k.current_speed = 260.0;
    let fast = k.picked_up_leg_pose(0, ax, ay, nfx, nfy, &ctx);
    assert!(fast.0 < slow.0);
    assert!(fast.1 > slow.1 + k.size * 0.02);
    // A held spider's walking swing is frozen.
    k.legs[0].stepping = true;
    k.legs[0].lift = 1.0;
    let mut rng = PyRandom::from_int(1);
    k.update_legs(DT, &ctx, &mut rng);
    assert!(k.legs[0].stepping && k.legs[0].lift == 1.0);
}

#[test]
fn tarantula_released_feet_stay_inside_the_chain_span() {
    let (k, ctx) = tarantula();
    let ctx = FrameCtx { held_release_timer: 0.24, ..ctx };
    let draw = dc_kinematics::DrawCtx::default();
    let chain = k.rig.leg_chain.clone().unwrap();
    let mut far = k.clone();
    // Stretch every planted foot well out of reach, as a fast drag leaves them.
    for leg in far.legs.iter_mut() {
        leg.foot_x += 3.0 * far.size;
    }
    for i in 0..far.legs.len() {
        let (ax, ay) = far.leg_attach(i);
        let (_, _, fx, fy) = far.leg_draw_points(i, &ctx, &draw);
        let (fx, fy) = far.safe_sprite_leg_foot(i, fx, fy, true, &ctx, &draw);
        let d = &far.rig.legs[i];
        let span = (d.upper_len_or() + d.lower_len_or()) * far.size * chain.max_stretch;
        assert!(hypot(fx - ax, fy - ay) <= span + 1e-4, "leg {i}");
    }
}

// ----------------------------------------------------- test_tarantula_stride.py

fn stride_walk(base: &Kinematics, ctx: &FrameCtx, speed: f64) -> (f64, f64, f64, f64) {
    let mut k = base.clone();
    let mut rng = PyRandom::from_int(19);
    k.speed = speed;
    k.target_x = 20000.0;
    for _ in 0..90 {
        advance_controller(&mut k, ctx, &mut rng);
    }
    let x0 = k.x;
    let mut was: Vec<bool> = k.legs.iter().map(|l| l.airborne()).collect();
    let (mut starts, mut emergencies) = (0usize, 0usize);
    let frames = 360;
    for _ in 0..frames {
        advance_controller(&mut k, ctx, &mut rng);
        for (i, leg) in k.legs.iter().enumerate() {
            let now = leg.airborne();
            if now && !was[i] {
                starts += 1;
                emergencies += usize::from(leg.last_step_emergency);
            }
            was[i] = now;
        }
    }
    let seconds = frames as f64 * DT;
    let actual = (k.x - x0) / seconds;
    let frequency = starts as f64 / k.legs.len() as f64 / seconds;
    (actual, frequency, actual / frequency, emergencies as f64 / starts.max(1) as f64)
}

#[test]
fn tarantula_speed_comes_from_step_frequency_not_stride() {
    let (k, ctx) = tarantula();
    let speeds = [60.0, 100.0, 150.0, 200.0];
    let walks: Vec<_> = speeds.iter().map(|&s| stride_walk(&k, &ctx, s)).collect();
    for (w, asked) in walks.iter().zip(speeds) {
        assert!(w.0 > asked * 0.97, "asked {asked}, walked {}", w.0);
        assert!(w.3 <= 0.05, "emergencies {w:?}");
    }
    let strides: Vec<f64> = walks.iter().map(|w| w.2).collect();
    let (mn, mx) = (strides.iter().cloned().fold(f64::MAX, f64::min), strides.iter().cloned().fold(f64::MIN, f64::max));
    assert!(mx < mn * 1.10, "{strides:?}");
    assert!(walks[3].1 > walks[0].1 * 3.0, "{walks:?}");
    let cfg = k.rig.spider_gait.unwrap();
    assert!(cfg.stride_length > 0.0);
    assert_eq!(cfg.min_swing_time, 0.04);
}

// ------------------------------------------------ test_turn_movement.py (kinematic half)

/// `PlayerController._update_turn_movement` for the bold tarantula, then
/// `advance_pose`'s kinematic calls.
fn player_step(k: &mut Kinematics, ctx: &FrameCtx, rng: &mut PyRandom, forward: i32, turn: i32) {
    let backing = forward < 0;
    let ctx = FrameCtx { state: MotionState::Other, reverse_walk: backing, ..*ctx };
    k.motion_paused = forward == 0;
    k.speed = 115.0 * 1.15; // bold speed_multiplier
    if backing {
        k.speed *= 0.40;
    }
    let facing = k.heading + turn as f64 * 0.6;
    if forward > 0 {
        k.target_x = k.x + facing.cos() * 100.0;
        k.target_y = k.y + facing.sin() * 100.0;
    } else if backing {
        k.target_heading = facing;
        k.target_x = k.x - k.heading.cos() * 100.0;
        k.target_y = k.y - k.heading.sin() * 100.0;
    } else {
        k.target_x = k.x;
        k.target_y = k.y;
        if turn != 0 {
            k.target_heading = facing;
        }
    }
    k.locomotion_frame(DT, &ctx, rng);
}

fn player() -> (Kinematics, FrameCtx<'static>, PyRandom) {
    let (mut k, ctx) = creature("tarantula_walk", Some("bold"));
    let mut rng = PyRandom::from_int(11);
    k.target_x = k.x;
    k.target_y = k.y;
    for _ in 0..30 {
        player_step(&mut k, &ctx, &mut rng, 0, 0);
    }
    (k, ctx, rng)
}

fn hold(k: &mut Kinematics, ctx: &FrameCtx, rng: &mut PyRandom, forward: i32, turn: i32, seconds: f64) {
    for _ in 0..(seconds / DT).round() as usize {
        player_step(k, ctx, rng, forward, turn);
    }
}

#[test]
fn w_walks_forward_and_s_backs_up_slower() {
    let (mut k, ctx, mut rng) = player();
    let (x0, y0, h0) = (k.x, k.y, k.heading);
    hold(&mut k, &ctx, &mut rng, 1, 0, 1.5);
    let ahead = (k.x - x0) * h0.cos() + (k.y - y0) * h0.sin();
    assert!(ahead > 60.0, "{ahead}");
    assert!(wrap_pi(k.heading - h0).abs() < 0.1);

    let (mut back, ctx, mut rng) = player();
    let (x0, y0, h0) = (back.x, back.y, back.heading);
    hold(&mut back, &ctx, &mut rng, -1, 0, 2.0);
    let along = (back.x - x0) * h0.cos() + (back.y - y0) * h0.sin();
    assert!(along < -25.0, "{along}");
    assert!(wrap_pi(back.heading - h0).abs() < 0.15);
    let (mut fwd, ctx, mut rng) = player();
    let fx0 = fwd.x;
    hold(&mut fwd, &ctx, &mut rng, 1, 0, 2.0);
    assert!(along.abs() < (fwd.x - fx0) * 0.6);
}

#[test]
fn a_and_d_turn_on_the_spot_and_w_d_walks_an_arc() {
    let (mut k, ctx, mut rng) = player();
    let (x0, y0, h0) = (k.x, k.y, k.heading);
    hold(&mut k, &ctx, &mut rng, 0, -1, 1.0);
    let left = wrap_pi(k.heading - h0);
    assert!(left.abs() > 0.5 && left < 0.0, "{left}");
    assert!(hypot(k.x - x0, k.y - y0) < k.size * 1.5);
    let h1 = k.heading;
    hold(&mut k, &ctx, &mut rng, 0, 1, 1.0);
    assert!(wrap_pi(k.heading - h1) > 0.3);

    let (mut arc, ctx, mut rng) = player();
    let (x0, y0, h0) = (arc.x, arc.y, arc.heading);
    hold(&mut arc, &ctx, &mut rng, 1, 1, 1.5);
    assert!(wrap_pi(arc.heading - h0).abs() > 0.6);
    assert!(hypot(arc.x - x0, arc.y - y0) > 40.0);
}

#[test]
fn backing_up_keeps_its_feet_planted() {
    let (mut k, ctx, mut rng) = player();
    let mut was: Vec<bool> = k.legs.iter().map(|l| l.airborne()).collect();
    let (mut starts, mut emergencies, mut slides) = (0, 0, 0);
    for _ in 0..180 {
        let planted: Vec<Option<(f64, f64)>> = k
            .legs
            .iter()
            .map(|l| (l.contact_state == ContactState::Stance && !l.airborne()).then_some((l.foot_x, l.foot_y)))
            .collect();
        player_step(&mut k, &ctx, &mut rng, -1, 0);
        for (i, leg) in k.legs.iter().enumerate() {
            let now = leg.airborne();
            if now && !was[i] {
                starts += 1;
                emergencies += usize::from(leg.last_step_emergency);
            }
            if let Some((ox, oy)) = planted[i]
                && !now
                && !was[i]
                && leg.contact_state == ContactState::Stance
            {
                slides += usize::from(hypot(leg.foot_x - ox, leg.foot_y - oy) > 1e-6);
            }
            was[i] = now;
        }
    }
    assert!(starts > 20, "{starts}");
    assert_eq!(slides, 0);
    assert!(emergencies as f64 <= starts as f64 * 0.15, "{emergencies} of {starts}");
}

// ---------------------------------------------------------------- test_drifter.py

#[test]
fn drifter_sockets_sit_on_the_edge_and_stance_stays_long() {
    let fx = load("drifter.json");
    let rig = Rig::from_model_json(&fx["model"], &fx["personality"], true);
    for leg in &rig.legs {
        assert!(leg.attach_side.unwrap_or(0.0) >= 0.50);
    }
    let mut k = Kinematics::new(rig, GaitStyle::Skitter, PyRandom::from_int(7), 1200.0, 700.0, 0.0, 28.0);
    let mut ctx_state = CtxState::default();
    ctx_state.merge(&fx["frames"][0]["ctx"]);
    let mut ctx = ctx_state.frame_ctx();
    ctx.state = MotionState::Wander;
    k.size = fx["init"]["size"].as_f64().unwrap();
    let mut rng = PyRandom::from_int(7);
    k.seed_legs(&mut rng);
    k.initialize_legs(&ctx, &mut rng);
    let min_r = k.legs.iter().map(|l| hypot(l.foot_x - k.x, l.foot_y - k.y)).fold(f64::MAX, f64::min);
    assert!(min_r >= k.size * 1.35, "{min_r}");
}

/// Replays a fixture (its `pre` patches carry what the FSM decided) and calls
/// `check` after every frame.
fn replay_with(name: &str, mut check: impl FnMut(&Kinematics, &FrameCtx)) {
    let fx = load(&format!("{name}.json"));
    let mut k = build(&fx);
    let mut rng = PyRandom::from_int(fx["kin_seed"].as_i64().unwrap() as i128);
    let mut ctx_state = CtxState::default();
    for frame in fx["frames"].as_array().unwrap() {
        if frame["pre"].as_object().is_some_and(|o| !o.is_empty()) {
            k.apply_json_overrides(&frame["pre"]).unwrap();
        }
        ctx_state.merge(&frame["ctx"]);
        let ctx = ctx_state.frame_ctx();
        for op in frame["ops"].as_array().unwrap() {
            match op.as_str().unwrap() {
                "production" => {
                    k.locomotion_frame(DT, &ctx, &mut rng);
                }
                "move_body" => k.move_body(DT, &ctx, &mut rng),
                "update_legs" => k.update_legs(DT, &ctx, &mut rng),
                "initialize_legs" => k.initialize_legs(&ctx, &mut rng),
                "panic_rehome" => k.panic_rehome_legs(0.8, &ctx, &mut rng),
                other => panic!("{other}"),
            }
        }
        check(&k, &ctx);
    }
}

#[test]
fn a_drifter_never_moonwalks() {
    let mut minimum = 1.0f64;
    let mut slid = false;
    replay_with("drifter", |k, ctx| {
        slid |= ctx.drift_run_sliding;
        let speed = hypot(k.vel_x, k.vel_y);
        if speed > 35.0 {
            let [fx, fy, _, _] = k.basis();
            minimum = minimum.min((k.vel_x * fx + k.vel_y * fy) / speed);
        }
    });
    assert!(slid, "the run never reached its slide");
    assert!(minimum >= 78.0f64.to_radians().cos() - 1e-6, "{minimum}");
}

// ------------------------------------------------------------- test_throw_slide.py

#[test]
fn a_thrown_spider_slides_to_a_stop() {
    let mut samples = Vec::new();
    replay_with("throw", |k, _| samples.push((k.x, k.y, k.inertia_timer)));
    let sliding: Vec<_> = samples.iter().filter(|s| s.2 > 0.0).collect();
    assert!(sliding.len() > 8, "{}", sliding.len());
    let steps: Vec<f64> = sliding.windows(2).map(|w| hypot(w[1].0 - w[0].0, w[1].1 - w[0].1)).collect();
    let third = (steps.len() / 3).max(1);
    let first: f64 = steps[..third].iter().sum::<f64>() / third as f64;
    let last: f64 = steps[steps.len() - third..].iter().sum::<f64>() / third as f64;
    assert!(last < first * 0.5, "{first} -> {last}");
    let dx = sliding[sliding.len() - 1].0 - sliding[0].0;
    let dy = sliding[sliding.len() - 1].1 - sliding[0].1;
    assert!(dx > 60.0, "{dx}");
    assert!(dy.abs() < dx.abs());
}

// ------------------------------------------------ test_gait_phases_and_growth.py

#[test]
fn what_a_spider_is_doing_overrides_how_it_walks() {
    let (mut k, ctx) = tarantula();
    k.gait_style = GaitStyle::Lively;
    let at = |state| FrameCtx { state, ..ctx };
    assert_eq!(k.effective_gait_style(&at(MotionState::Inspect)), GaitStyle::Skitter);
    assert!(k.uses_skitter_gait(&at(MotionState::Inspect)));
    assert_eq!(k.effective_gait_style(&at(MotionState::Chase)), GaitStyle::Lively);
    assert!(!k.uses_skitter_gait(&at(MotionState::Chase)));
    assert!(k.uses_lively_gait(&at(MotionState::Chase)));
    // Wander keeps the temperament's own gait.
    for style in [GaitStyle::Lively, GaitStyle::Skitter] {
        k.gait_style = style;
        assert_eq!(k.effective_gait_style(&at(MotionState::Wander)), style);
    }
    // A frightened spider runs whatever state it is in.
    k.gait_style = GaitStyle::Skitter;
    assert_eq!(k.effective_gait_style(&at(MotionState::Inspect)), GaitStyle::Skitter);
    let fleeing = FrameCtx { fleeing: true, ..at(MotionState::Inspect) };
    assert_eq!(k.effective_gait_style(&fleeing), GaitStyle::Lively);
    // Classic opts out entirely.
    k.gait_style = GaitStyle::Classic;
    for (name, _) in dc_kinematics::constants::GAIT_BY_STATE.iter().chain(&[("Wander", "")]) {
        assert_eq!(k.effective_gait_style(&at(MotionState::from_name(name))), GaitStyle::Classic, "{name}");
    }
}

#[test]
fn a_gait_spell_comes_and_goes_on_its_own_stream() {
    let (mut k, _) = tarantula();
    k.gait_style = GaitStyle::Lively;
    k.gait_spell_timer = 0.01;
    k.update_gait_spell(DT);
    assert_eq!(k.gait_spell, Some(GaitStyle::Skitter));
    assert!((1.8..=4.5).contains(&k.gait_spell_timer));
    k.gait_spell_timer = 0.0;
    k.update_gait_spell(DT);
    assert_eq!(k.gait_spell, None);
    assert!((9.0..=26.0).contains(&k.gait_spell_timer));
    k.gait_style = GaitStyle::Classic;
    let t = k.gait_spell_timer;
    k.update_gait_spell(DT);
    assert_eq!(k.gait_spell_timer, t);
}

#[test]
fn a_grown_spider_keeps_walking_inside_its_reach() {
    let (k, ctx) = tarantula();
    let mut grown = k.clone();
    grown.size *= 1.4;
    let base_reach = k.leg_max_reach(0, false);
    let grown_reach = grown.leg_max_reach(0, false);
    assert!((grown_reach / base_reach - 1.4).abs() < 1e-9, "reach scales with size");
    let (_, r) = run_walk(&grown, &ctx, 3.0, 100.0, 0.0, DT, 5);
    assert!(r.starts > 0);
    assert_eq!(r.planted_displacements, 0);
    assert!(r.max_segment_ratio <= 1.001, "{r:?}");
}

#[test]
fn json_overrides_round_trip() {
    let (k, _) = tarantula();
    let snapshot = serde_json::to_value(&k).unwrap();
    let mut other = k.clone();
    other.x += 50.0;
    other.legs[2].foot_x = -1.0;
    other.apply_json_overrides(&snapshot).unwrap();
    assert_eq!(serde_json::to_value(&other).unwrap(), snapshot);
    let _: Value = snapshot;
}
