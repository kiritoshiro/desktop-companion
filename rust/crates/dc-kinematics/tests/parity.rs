//! Frame-by-frame replay of the Python traces dumped by
//! `tools/dump_fixtures.py`. Every snapshot field of every frame is compared
//! (the fixture stores what changed; the replay accumulates it), plus the
//! rendered leg geometry every few frames.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_kinematics::MotionState;
use serde_json::Value;

/// Relative tolerance. The port is bit-exact in practice; the fixture rounds
/// oracle outputs to 13 significant digits, so this is the floor.
const TOL: f64 = 1e-9;

struct Report {
    frames: usize,
    worst: f64,
}

fn replay(name: &str) -> Report {
    let fx = load(&format!("{name}.json"));
    let mut k = build(&fx);
    let mut expected = fx["init"].clone();
    let mut rng = PyRandom::from_int(fx["kin_seed"].as_i64().unwrap() as i128);
    let dt = fx["dt"].as_f64().unwrap();
    let mut ctx_state = CtxState::default();
    let mut worst = 0.0f64;
    let frames = fx["frames"].as_array().unwrap();
    for (index, frame) in frames.iter().enumerate() {
        let pre = &frame["pre"];
        if pre.as_object().is_some_and(|o| !o.is_empty()) {
            k.apply_json_overrides(pre).unwrap();
            merge(&mut expected, pre);
        }
        ctx_state.merge(&frame["ctx"]);
        let mut ctx = ctx_state.frame_ctx();
        let mut mid = frame.get("mid").and_then(Value::as_array).map(|v| v.iter()).into_iter().flatten();
        let ops = frame["ops"].as_array().unwrap();
        for (op_index, op) in ops.iter().enumerate() {
            let op = op.as_str().unwrap();
            match op {
                "production" => {
                    let _ = k.locomotion_frame(dt, &ctx, &mut rng);
                }
                "controller" => {
                    k.update_spider_grounded_frame(dt, &ctx, &mut rng);
                    k.spider_gait_frame_updated = false;
                }
                "locomotion" => k.update_spider_grounded_locomotion(dt, &ctx, &mut rng),
                "advance_steps" => k.advance_active_steps(dt, &ctx, &mut rng),
                "move_body" => k.move_body(dt, &ctx, &mut rng),
                "update_legs" => k.update_legs(dt, &ctx, &mut rng),
                "roll" => {
                    if k.update_roll(dt, &ctx) {
                        // Python's `enter_idle()` ran here (with the FSM stream).
                        let patch = mid.next().expect("enter_idle patch");
                        k.apply_json_overrides(patch).unwrap();
                        ctx.state = MotionState::Idle;
                        ctx_state.json.insert("state".into(), Value::from("Idle"));
                    }
                }
                "reconcile_roll" => {
                    let _ = k.reconcile_roll(&ctx);
                }
                "held" => k.update_held_leg_springs(dt, &ctx),
                "initialize_legs" => k.initialize_legs(&ctx, &mut rng),
                "panic_rehome" => k.panic_rehome_legs(0.8, &ctx, &mut rng),
                "gait_spell" => k.update_gait_spell(dt),
                "intent" => {
                    let (f, s, t) = k.spider_locomotion_intent(dt, &ctx, &mut rng);
                    let want = &frame["results"][op_index];
                    let got = Value::from(vec![f, s, t]);
                    if let Some(m) = compare("intent", want, &got, TOL, &mut worst) {
                        panic!("{name} frame {index}: {m}");
                    }
                }
                other => panic!("unknown op {other}"),
            }
        }
        merge(&mut expected, &frame["post"]);
        let actual = serde_json::to_value(&k).unwrap();
        if let Some(m) = compare("state", &expected, &actual, TOL, &mut worst) {
            panic!("{name} frame {index} ({ops:?}): {m}");
        }
        if let Some(probe) = frame.get("probe") {
            check_probe(name, index, &k, &ctx, probe, &mut worst);
        }
    }
    Report { frames: frames.len(), worst }
}

fn check_probe(
    name: &str,
    index: usize,
    k: &dc_kinematics::Kinematics,
    ctx: &dc_kinematics::FrameCtx,
    probe: &Value,
    worst: &mut f64,
) {
    let draw = draw_ctx(&probe["draw"]);
    for (i, want) in probe["legs"].as_array().unwrap().iter().enumerate() {
        let (ax, ay) = k.leg_attach(i);
        let (vx, vy) = k.visual_foot_for_render(i, ctx);
        let (dax, day, dfx, dfy) = k.leg_draw_points(i, ctx, &draw);
        let mut got = serde_json::json!({
            "attach": [ax, ay],
            "visual_foot": [vx, vy],
            "draw_points": [dax, day, dfx, dfy],
            "lift_px": k.leg_lift_px(i, ctx),
            "ideal": <[f64; 2]>::from(k.leg_ideal_foot(i, ctx)),
            "knee": <[f64; 2]>::from(k.solve_knee(i, ax, ay, vx, vy, ctx)),
        });
        if want.get("chain").is_some() {
            let chain = k.sprite_leg_chain_points(i, ax, ay, vx, vy, ctx).expect("chain config");
            got["chain"] = Value::from(chain.as_slice().iter().map(|p| vec![p.0, p.1]).collect::<Vec<_>>());
            let (sx, sy) = k.safe_sprite_leg_foot(i, dfx, dfy, true, ctx, &draw);
            got["safe_foot"] = Value::from(vec![sx, sy]);
        }
        if let Some(m) = compare(&format!("probe.legs[{i}]"), want, &got, TOL, worst) {
            panic!("{name} frame {index}: {m}");
        }
    }
}

macro_rules! parity {
    ($($test:ident => $name:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                let r = replay($name);
                println!("{}: {} frames, max |diff| {:e}", $name, r.frames, r.worst);
                assert!(r.frames > 50);
            }
        )*
    };
}

parity! {
    tarantula_walk_matches_python => "tarantula_walk",
    snowpuff_walk_matches_python => "snowpuff_walk",
    jumper_skitter_matches_python => "jumper_skitter",
    bug_classic_matches_python => "bug_classic",
    nogait_lively_matches_python => "nogait_lively",
    nogait_classic_matches_python => "nogait_classic",
    drifter_matches_python => "drifter",
    throw_matches_python => "throw",
    roll_matches_python => "roll",
    held_pose_matches_python => "held",
    heading_filter_matches_python => "heading_filter",
}
