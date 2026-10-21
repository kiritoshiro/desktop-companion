//! Port of `tests/test_held_pose.py`: a carried spider's legs react to how
//! fast the hand is moving (`held_speed01` in the chain solve).
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature.

mod common;

use dc_creature::Creature;

const FAST: f64 = 400.0;

fn carried() -> Creature {
    let mut c = common::spider_with(common::load_model("tarantula"), common::load_personality("mellow"), 3, |cfg| {
        cfg.screen_w = 1200.0;
        cfg.screen_h = 800.0;
    });
    c.kin.x = 600.0;
    c.kin.y = 400.0;
    c.kin.heading = 0.0;
    c.kin.target_heading = 0.0;
    c.initialize_legs();
    c.dragging = true;
    c
}

/// How far the drawn chains bow away from straight attach-to-foot lines.
/// The Rust solve is not memoised, so no cache needs clearing.
fn chain_bend(c: &Creature) -> f64 {
    let ctx = c.kctx(None);
    let mut total = 0.0;
    for i in 0..c.kin.legs.len() {
        let (ax, ay) = c.kin.leg_attach(i);
        let (fx, fy) = c.kin.visual_foot_for_render(i, &ctx);
        let points = c.kin.sprite_leg_chain_points(i, ax, ay, fx, fy, &ctx).expect("the tarantula has a leg chain");
        let span = (fx - ax).hypot(fy - ay);
        if span < 1e-6 {
            continue;
        }
        let pts = points.as_slice();
        for &(px, py) in &pts[1..pts.len() - 1] {
            total += ((fx - ax) * (ay - py) - (ax - px) * (fy - ay)).abs() / span;
        }
    }
    total
}

#[test]
fn test_a_still_hand_still_leaves_a_soft_knee() {
    let mut c = carried();
    c.kin.current_speed = 0.0;
    assert!(chain_bend(&c) > 0.0);
}

#[test]
fn test_a_faster_hand_bends_the_carried_legs_more() {
    let mut c = carried();
    c.kin.current_speed = 0.0;
    let still = chain_bend(&c);
    c.kin.current_speed = FAST;
    let fast = chain_bend(&c);
    assert!(fast > still, "{still} {fast}");
}

#[test]
fn test_the_extra_bend_is_a_nudge_and_not_a_fold() {
    let mut c = carried();
    c.kin.current_speed = 0.0;
    let still = chain_bend(&c);
    c.kin.current_speed = FAST;
    let fast = chain_bend(&c);
    assert!(fast <= still * 1.35, "{still} {fast}");
}
