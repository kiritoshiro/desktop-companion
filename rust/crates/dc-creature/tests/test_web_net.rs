//! Port of `tests/test_web_net.py`: a webbed spider shows the net and fights
//! it. The net's pixels are `dc-paint`'s; `PlayerController` is not ported,
//! so its `struggling` property is the creature's `player_struggling` flag.
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature.

mod common;

use dc_creature::Creature;
use dc_creature::constants::WEBBED_SECONDS;
use dc_creature::state::State;
use dc_creature::{ApproxLabelMetrics, kin_ctx};

const DT: f64 = 1.0 / 60.0;

/// `movement.advance_controller`: the production grounded update order.
fn advance_controller(c: &mut Creature, dt: f64) {
    kin_ctx!(c, |kin, ctx, rng| kin.update_spider_grounded_frame(dt, &ctx, rng));
    c.kin.spider_gait_frame_updated = false;
}

/// `_tarantula()`: `movement.build_creature` with the bold tarantula, settled.
fn tarantula() -> Creature {
    let mut c = common::spider_with(common::load_model("tarantula"), common::load_personality("bold"), 8, |cfg| {
        cfg.screen_w = 2400.0;
        cfg.screen_h = 1400.0;
        cfg.gait_style = "lively".into();
    });
    c.kin.x = 900.0;
    c.kin.y = 700.0;
    c.kin.heading = 0.0;
    c.kin.target_heading = 0.0;
    c.kin.target_x = 2100.0;
    c.kin.target_y = 700.0;
    c.kin.current_speed = 0.0;
    c.kin.speed = 80.0;
    c.kin.vel_x = 0.0;
    c.kin.vel_y = 0.0;
    c.state = State::Wander;
    c.initialize_legs();
    c.kin.speed = 0.0;
    (c.kin.target_x, c.kin.target_y) = (c.kin.x, c.kin.y);
    for _ in 0..30 {
        advance_controller(&mut c, DT);
    }
    c
}

fn seconds_to_free(c: &mut Creature, mut step: impl FnMut(&mut Creature)) -> f64 {
    c.web_pinned(None);
    let mut elapsed = 0.0;
    while c.webbed() && elapsed < 10.0 {
        step(c);
        elapsed += DT;
    }
    elapsed
}

#[test]
fn test_a_fresh_net_is_anchored_and_goes_when_free() {
    let mut c = tarantula();
    c.web_pinned(None);
    assert_eq!(c.web_anchors.len(), 9);
    let size = c.size();
    assert!(c.web_anchors.iter().all(|&(ax, ay)| (ax - c.kin.x).hypot(ay - c.kin.y) > size));
    c.webbed_timer = DT / 2.0;
    c.update_web_struggle(DT);
    c.webbed_timer = 0.0;
    c.update_web_struggle(DT);
    assert!(c.web_anchors.is_empty());
}

#[test]
fn test_a_computer_spider_struggles_free_sooner() {
    let mut c = tarantula();
    let freed = seconds_to_free(&mut c, |c| {
        c.webbed_timer = (c.webbed_timer - DT).max(0.0);
        c.update_web_struggle(DT);
    });
    assert!(freed < WEBBED_SECONDS * 0.8, "struggling should shorten the pin, took {freed:.2}s");
    assert!(freed > WEBBED_SECONDS * 0.5, "too easy: {freed:.2}s");
}

#[test]
fn test_the_player_frees_itself_faster_by_struggling() {
    let run = |struggling: bool| {
        let mut c = tarantula();
        // `PlayerController(spider)`; `player.held = set(hold)`.
        c.player_control = true;
        c.player_struggling = struggling;
        c.enter_idle();
        seconds_to_free(&mut c, |c| {
            common::step(c, DT, -9000.0, -9000.0, &[]);
        })
    };
    let waiting = run(false);
    let fighting = run(true);
    assert!((waiting - WEBBED_SECONDS).abs() <= 0.05, "not struggling takes the full pin: {waiting}");
    assert!(fighting < waiting * 0.8, "{fighting} {waiting}");
}

// not ported: test_a_webbed_spider_does_not_move_or_turn (drives PlayerController.update with held keys; dc-adventure has no controller over a dc-creature Creature)

#[test]
fn test_a_computer_spider_is_held_still_too() {
    let mut c = tarantula();
    c.web_pinned(None);
    c.kin.target_x = c.kin.x + 300.0;
    c.kin.target_y = c.kin.y + 200.0;
    c.kin.target_heading = c.kin.heading + 2.0;
    c.kin.speed = 120.0;
    let (x0, y0, h0) = (c.kin.x, c.kin.y, c.kin.heading);
    for _ in 0..60 {
        advance_controller(&mut c, DT);
    }
    assert!((c.kin.x - x0).hypot(c.kin.y - y0) < 1e-6);
    assert!((c.kin.heading - h0).abs() < 1e-6);
}

#[test]
fn test_the_net_is_a_web_that_tears_as_it_loosens() {
    // The logic half: the net's shape and the repaint box. The pixel
    // comparisons (the net shows, a loosening net tears) are dc-paint's.
    let mut c = tarantula();
    c.web_pinned(None);
    assert_eq!(c.web_net.len(), 9);
    assert!(c.web_net.iter().all(|s| s.rings.len() == 4));
    let bx = c.bounding_rect(false, &ApproxLabelMetrics);
    for &(ax, ay) in &c.web_anchors {
        assert!(bx.0 <= ax && ax <= bx.2 && bx.1 <= ay && ay <= bx.3, "repaint must cover the guy lines");
    }
    assert_eq!(c.web_strength(), 1.0);
    c.webbed_timer = WEBBED_SECONDS * 0.2;
    assert!((c.web_strength() - 0.2).abs() < 1e-12);
}
