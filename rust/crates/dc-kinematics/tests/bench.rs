//! Criterion-free micro benchmark: the per-frame kinematic update cost per
//! spider, for a colony of walking tarantulas at 60 fps.
//!
//! ```text
//! CARGO_TARGET_DIR=target-dc-kinematics cargo test --release -p dc-kinematics \
//!     --test bench -- --ignored --nocapture
//! ```

mod common;

use common::*;
use dc_core::PyRandom;
use std::time::Instant;

const DT: f64 = 1.0 / 60.0;

fn colony(fixture: &str, n: usize) -> (Vec<dc_kinematics::Kinematics>, dc_kinematics::FrameCtx<'static>) {
    let fx = load(fixture);
    let base = build(&fx);
    let mut ctx = CtxState::default();
    ctx.merge(&fx["frames"][0]["ctx"]);
    let ctx = ctx.frame_ctx();
    let mut spiders = Vec::with_capacity(n);
    for i in 0..n {
        let mut k = base.clone();
        let dx = (i % 16) as f64 * 3.0;
        let dy = (i / 16) as f64 * 3.0;
        k.x += dx;
        k.y += dy;
        for leg in k.legs.iter_mut() {
            leg.foot_x += dx;
            leg.foot_y += dy;
        }
        k.speed = 60.0 + (i % 7) as f64 * 20.0;
        spiders.push(k);
    }
    (spiders, ctx)
}

fn run(label: &str, fixture: &str, spiders_n: usize, frames: usize, with_chain: bool) {
    let (mut spiders, ctx) = colony(fixture, spiders_n);
    let mut rng = PyRandom::from_int(1);
    // Warm up, and steer them around so turning and stepping both happen.
    let mut sink = 0.0;
    let step = |spiders: &mut Vec<dc_kinematics::Kinematics>, frame: usize, rng: &mut PyRandom| {
        let mut acc = 0.0;
        for (i, k) in spiders.iter_mut().enumerate() {
            let a = ((frame / 90 + i) % 8) as f64 * 0.785;
            k.target_x = k.x + a.cos() * 400.0;
            k.target_y = k.y + a.sin() * 400.0;
            k.locomotion_frame(DT, &ctx, rng);
            if with_chain {
                for leg in 0..k.legs.len() {
                    let (ax, ay) = k.leg_attach(leg);
                    let (fx, fy) = k.visual_foot_for_render(leg, &ctx);
                    if let Some(p) = k.sprite_leg_chain_points(leg, ax, ay, fx, fy, &ctx) {
                        acc += p.as_slice()[1].0;
                    }
                }
            }
            // Keep the colony on screen.
            if k.x < 200.0 || k.x > 2200.0 || k.y < 200.0 || k.y > 1200.0 {
                k.target_x = 1200.0;
                k.target_y = 700.0;
            }
        }
        acc
    };
    for frame in 0..60 {
        sink += step(&mut spiders, frame, &mut rng);
    }
    let start = Instant::now();
    for frame in 60..60 + frames {
        sink += step(&mut spiders, frame, &mut rng);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let per = elapsed / (frames * spiders_n) as f64;
    println!(
        "{label}: {spiders_n} spiders x {frames} frames: {:.2} us per spider per frame, {:.3} ms per colony frame (sink {})",
        per * 1e6,
        elapsed / frames as f64 * 1e3,
        sink.is_finite()
    );
}

#[test]
#[ignore]
fn bench_update_cost_per_spider() {
    run("tarantula locomotion", "tarantula_walk.json", 256, 600, false);
    run("tarantula locomotion + 8 chain solves", "tarantula_walk.json", 256, 600, true);
    run("bug classic locomotion", "bug_classic.json", 256, 600, false);
}
