//! Port of `tests/test_regal.py`: the regal jumper is selectable, expressive,
//! and yields to gameplay. Drawing (`_render_regal`) is `dc-paint`'s.

mod common;

use dc_creature::Creature;
use dc_creature::creature::{After, JumpKind};
use dc_creature::regal::{RegalExpression, RegalGesture, RegalTarget};
use dc_creature::state::State;
use dc_creature::world::Population;
use std::f64::consts::PI;

/// `regal(seed)`: `Creature(*load_pair("regal_fluff", "curious"), 800, 600, seed=seed)`, idle at (400, 300).
fn regal(seed: i64) -> Creature {
    let mut c =
        common::spider_with(common::load_model("regal_fluff"), common::load_personality("curious"), seed, |cfg| {
            cfg.screen_w = 800.0;
            cfg.screen_h = 600.0;
        });
    c.kin.x = 400.0;
    c.kin.y = 300.0;
    c.kin.heading = -PI / 2.0;
    c.state = State::Idle;
    c.kin.current_speed = 0.0;
    c.startled_timer = 0.0;
    c.initialize_legs();
    c
}

fn r(c: &Creature) -> RegalExpression {
    c.regal.expect("a regal creature has a regal expression")
}

const MOUSE_GESTURES: [RegalGesture; 3] = [RegalGesture::Question, RegalGesture::Peek, RegalGesture::Reach];

/// `seen.add(g)` on a list (the gesture enum is not `Hash`).
fn add(seen: &mut Vec<RegalGesture>, g: RegalGesture) {
    if !seen.contains(&g) {
        seen.push(g);
    }
}

fn any_mouse(seen: &[RegalGesture]) -> bool {
    seen.iter().any(|g| MOUSE_GESTURES.contains(g))
}

#[test]
fn test_selectable_in_jumper_and_all_categories() {
    // Only the discovery half: the dropdown and preview icon are `dc-ui`'s.
    let (models, warnings) = dc_content::discovery::discover_models(Some(&common::root()));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(models.get("regal_fluff").expect("regal_fluff is discovered").legs.len(), 8);
}

#[test]
fn test_mouse_attention_and_bounded_pose() {
    let mut c = regal(8);
    c.regal = Some(RegalExpression { cooldown: 0.0, ..Default::default() });
    let pop = Population::new();
    let mut seen = Vec::new();
    for _ in 0..900 {
        c.update_regal(0.04, 460.0, 200.0, &pop);
        let r = r(&c);
        add(&mut seen, r.gesture);
        assert!((0.0..=1.0).contains(&r.left) && (0.0..=1.0).contains(&r.right));
        assert!(r.yaw.abs() <= 0.65 && r.tilt.abs() <= 0.2);
    }
    assert!(seen.contains(&RegalGesture::Rest) && any_mouse(&seen), "{seen:?}");
    assert_eq!(r(&c).target, RegalTarget::Mouse);
    assert_eq!((c.kin.x, c.kin.y), (400.0, 300.0), "expressions must not move navigation contacts");
}

#[test]
fn test_social_attention_respects_visibility_and_relationship() {
    let mut c = regal(8);
    let mut other = regal(9);
    other.kin.x += 70.0;
    c.neighbors = vec![other.id];
    c.regal = Some(RegalExpression { cooldown: 0.0, ..Default::default() });
    let mut pop = Population::from_views([other.view()]);
    c.update_regal(0.04, 900.0, 900.0, &pop);
    assert_eq!(r(&c).target, RegalTarget::Spider);
    assert!(matches!(r(&c).gesture, RegalGesture::Greet | RegalGesture::Dance | RegalGesture::Question));

    other.desktop_fully_hidden = true;
    pop.set(other.view());
    c.update_regal(0.04, 900.0, 900.0, &pop);
    assert_eq!(r(&c).target, RegalTarget::None);

    other.desktop_fully_hidden = false;
    pop.set(other.view());
    // `c.relation_to = lambda _: "foe"`: a per-pair override.
    c.progression.relation_overrides.insert(other.progression_id.clone(), "foe".to_string());
    c.update_regal(0.04, 900.0, 900.0, &pop);
    assert_eq!(r(&c).target, RegalTarget::None);

    c.progression.relation_overrides.insert(other.progression_id.clone(), "friend".to_string());
    c.allow_social = false;
    c.update_regal(0.04, 900.0, 900.0, &pop);
    assert_eq!(r(&c).target, RegalTarget::None);
}

#[test]
fn test_priority_states_cancel_displays() {
    type Setter = fn(&mut Creature);
    let cases: [(&str, Setter); 10] = [
        ("dragging", |c| c.dragging = true),
        ("webbed_timer", |c| c.webbed_timer = 3.0),
        ("dead", |c| c.dead = true),
        ("_hunting_prey", |c| c.hunting_prey = true),
        ("_foe", |c| c.foe = Some(999)),
        ("player_control", |c| c.player_control = true),
        ("_desktop_fully_hidden", |c| c.desktop_fully_hidden = true),
        ("state", |c| c.state = State::Retreat),
        ("current_speed", |c| c.kin.current_speed = 90.0),
        ("startled_timer", |c| c.startled_timer = 1.0),
    ];
    for (field, set) in cases {
        let mut c = regal(8);
        c.regal = Some(RegalExpression {
            gesture: RegalGesture::Dance,
            duration: 3.4,
            left: 0.8,
            right: 0.8,
            ..Default::default()
        });
        set(&mut c);
        c.update_regal(0.04, 450.0, 250.0, &Population::new());
        let r = r(&c);
        assert_eq!(r.gesture, RegalGesture::Rest, "{field}");
        assert!(r.left == 0.0 && r.right == 0.0, "{field}");
    }
}

// not ported: test_render_is_pure_and_poses_are_visibly_different (QPainter rendering; dc-paint owns drawing)

#[test]
fn test_other_species_do_not_gain_regal_state() {
    let mut c =
        common::spider_with(common::load_model("velvet_regal_jumper"), common::load_personality("curious"), 8, |cfg| {
            cfg.screen_w = 800.0;
            cfg.screen_h = 600.0;
        });
    let mut before = c.rng.clone();
    c.update_regal(0.04, 200.0, 200.0, &Population::new());
    assert!(c.regal.is_none());
    // `rng.getstate()` unchanged: both streams continue identically.
    for _ in 0..4 {
        assert_eq!(before.random().to_bits(), c.rng.random().to_bits());
    }
}

#[test]
fn test_normal_update_loop_reaches_mouse_expressions() {
    let mut c = regal(17);
    let mut seen = Vec::new();
    for _ in 0..750 {
        let (mx, my) = (c.kin.x + 85.0, c.kin.y - 95.0);
        common::step(&mut c, 0.04, mx, my, &[]);
        add(&mut seen, r(&c).gesture);
        assert!(c.kin.legs.iter().all(|leg| leg.foot_x.is_finite() && leg.foot_y.is_finite()));
    }
    assert!(any_mouse(&seen), "{seen:?}");
}

#[test]
fn test_jump_interrupts_and_resumes_after_landing() {
    let mut c = regal(8);
    c.regal = Some(RegalExpression {
        gesture: RegalGesture::Dance,
        duration: 3.4,
        left: 0.8,
        right: 0.8,
        ..Default::default()
    });
    let size = c.size();
    c.launch_jump(405.0, 300.0, JumpKind::Hop, After::Outcome, Some(size), Some(0.5), 1.0);
    c.update_regal(0.04, 450.0, 250.0, &Population::new());
    assert_eq!(r(&c).gesture, RegalGesture::Rest);
    for _ in 0..25 {
        c.update_jump(0.04);
    }
    c.state = State::Idle;
    c.startled_timer = 0.0;
    c.kin.current_speed = 0.0;
    c.regal.as_mut().unwrap().cooldown = 0.0;
    c.update_regal(0.04, 450.0, 250.0, &Population::new());
    assert!(MOUSE_GESTURES.contains(&r(&c).gesture), "{:?}", r(&c).gesture);
}

#[test]
fn test_dance_alternates_forelegs_then_settles() {
    let mut c = regal(8);
    c.regal = Some(RegalExpression { gesture: RegalGesture::Dance, duration: 3.4, ..Default::default() });
    let mut differences = Vec::new();
    for _ in 0..86 {
        c.update_regal(0.04, 450.0, 250.0, &Population::new());
        differences.push(r(&c).left - r(&c).right);
    }
    let min = differences.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = differences.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(min < -0.1 && max > 0.1, "{min} {max}");
    assert_eq!(r(&c).gesture, RegalGesture::Rest);
    assert!(r(&c).left == 0.0 && r(&c).right == 0.0);
}
