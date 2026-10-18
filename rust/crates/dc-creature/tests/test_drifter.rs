//! Port of `tests/test_drifter.py`: Snowpuff-2's Drifter motion and leg
//! envelope.

mod common;

use dc_creature::Creature;
use dc_creature::creature::DriftRunMode;

const SEED: i64 = 7;

/// The Python fixture seeded the global `random` and built an unseeded
/// creature; here the creature is seeded instead.
fn drifter() -> Creature {
    let model = common::load_model("plush_snow_hybrid_2");
    let personality = common::load_personality("drifter");
    let mut spider = common::spider_with(model, personality, SEED, |cfg| {
        cfg.screen_w = 2400.0;
        cfg.screen_h = 1400.0;
        cfg.gait_style = "skitter".into();
    });
    spider.kin.x = 1200.0;
    spider.kin.y = 700.0;
    spider.kin.heading = 0.0;
    spider.kin.target_heading = 0.0;
    spider.initialize_legs();
    spider
}

#[test]
fn test_every_socket_sits_on_the_lateral_edge() {
    let spider = drifter();
    assert!(!spider.kin.rig.legs.is_empty());
    for def in &spider.kin.rig.legs {
        assert!(def.attach_side.unwrap_or(0.0) >= 0.50, "{:?}", def.attach_side);
    }
}

#[test]
fn test_the_neutral_stance_stays_long() {
    let spider = drifter();
    let min_stance_radius = spider
        .kin
        .legs
        .iter()
        .map(|leg| (leg.foot_x - spider.kin.x).hypot(leg.foot_y - spider.kin.y))
        .fold(f64::INFINITY, f64::min);
    assert!(min_stance_radius >= spider.kin.size * 1.35, "{min_stance_radius}");
}

#[test]
fn test_a_drifter_never_moonwalks() {
    let mut spider = drifter();
    spider.enter_drift_run(Some(DriftRunMode::Circle));
    assert_eq!(spider.state, dc_creature::State::DriftRun);
    let mut minimum_forward_component: f64 = 1.0;
    for _ in 0..480 {
        let dt = 1.0 / 60.0;
        spider.update_drift_run(dt);
        dc_creature::kin_ctx!(spider, |kin, ctx, rng| kin.move_body(dt, &ctx, rng));
        spider.update_legs(dt);
        let speed = spider.kin.vel_x.hypot(spider.kin.vel_y);
        if speed > 35.0 {
            let [fx, fy, _, _] = spider.kin.basis();
            minimum_forward_component =
                minimum_forward_component.min((spider.kin.vel_x * fx + spider.kin.vel_y * fy) / speed);
        }
    }
    assert!(minimum_forward_component >= 78.0f64.to_radians().cos() - 1e-6, "{minimum_forward_component}");
}
