//! Port of `tests/test_roll_and_health.py`: a spin ends cleanly however it
//! ends, and a health bar can be pinned.
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature.
//!
//! not ported: test_the_pin_is_remembered (a `ProgressionState` round trip,
//! dc-state's tests, not Creature).
//! not ported: test_the_bar_is_drawn_and_follows_the_health (QPainter pixels;
//! render belongs to dc-paint).

mod common;

use dc_creature::labels::ApproxLabelMetrics;
use dc_creature::{Creature, State};
use std::f64::consts::{PI, TAU};

const DT: f64 = 1.0 / 60.0;
const AWAY: (f64, f64) = (-100000.0, -100000.0);
const SEEDS: [i64; 7] = [1, 7, 11, 23, 42, 99, 123];

fn tick(c: &mut Creature) {
    common::step(c, DT, AWAY.0, AWAY.1, &[]);
}

fn build(seed: i64) -> Creature {
    let mut c =
        common::spider_with(common::load_model("tarantula"), common::load_personality("playful"), seed, |cfg| {
            cfg.progression_id = Some(format!("roll:{seed}"))
        });
    c.kin.x = 800.0;
    c.kin.y = 450.0;
    c.initialize_legs();
    for _ in 0..60 {
        tick(&mut c);
    }
    // A playful spider tumbles on its own; take the skill away and hand it
    // back only when the test wants it.
    c.set_skill_enabled("roll", false, None);
    for _ in 0..10 {
        tick(&mut c);
    }
    c
}

fn start_roll(c: &mut Creature) -> bool {
    c.set_skill_enabled("roll", true, None);
    c.enter_roll(None);
    c.state == State::Roll
}

fn worst_reach_ratio(c: &Creature) -> f64 {
    let mut worst: f64 = 0.0;
    for i in 0..c.kin.legs.len() {
        let (ax, ay) = c.kin.leg_attach(i);
        let reach = c.kin.leg_max_reach(i, true);
        let leg = &c.kin.legs[i];
        worst = worst.max((leg.foot_x - ax).hypot(leg.foot_y - ay) / reach.max(1e-6));
    }
    worst
}

fn assert_tidy(c: &Creature, label: &str) {
    assert!(!c.kin.rolling, "{label}: the spider still believes it is rolling");
    assert!(
        c.kin.roll_spin.abs() < 1e-9,
        "{label}: {:.0} degrees of rotation was left applied to the body",
        c.kin.roll_spin.abs().to_degrees()
    );
    assert!(c.kin.roll_tuck.abs() < 1e-9, "{label}: the legs were left tucked in");
    let ratio = worst_reach_ratio(c);
    assert!(ratio < 1.15, "{label}: a foot ended up at {ratio:.2} times its own reach");
}

#[test]
fn test_a_finished_spin_does_not_snap() {
    let mut worst_snap: f64 = 0.0;
    let mut rolled = 0;
    for seed in SEEDS {
        let mut c = build(seed);
        if !start_roll(&mut c) {
            continue;
        }
        rolled += 1;
        let turns = c.kin.roll_total / TAU;
        assert!(
            (turns - turns.round()).abs() < 1e-9,
            "a roll was planned as {turns:.2} turns; a fraction of a turn ends with the body jumping"
        );
        let mut last = 0.0;
        for _ in 0..400 {
            let previous = c.kin.roll_spin;
            tick(&mut c);
            if c.state != State::Roll {
                last = previous;
                break;
            }
        }
        // How far the drawn body moves on the frame the spin is dropped to zero.
        let snap = ((last.rem_euclid(TAU) + PI).rem_euclid(TAU) - PI).abs();
        worst_snap = worst_snap.max(snap);
        assert_tidy(&c, &format!("seed {seed} after a finished spin"));
    }
    assert!(rolled >= 5, "only {rolled} spiders rolled, so this proves little");
    assert!(
        worst_snap.to_degrees() < 1.0,
        "a finished spin left the body {:.0} degrees away from where it started",
        worst_snap.to_degrees()
    );
}

type Interrupt = fn(&mut Creature);

fn interrupt_and_settle(seed: i64, interrupt: Interrupt) -> Option<Creature> {
    let mut c = build(seed);
    if !start_roll(&mut c) {
        return None;
    }
    for frame in 0..400 {
        tick(&mut c);
        if frame == 18 {
            interrupt(&mut c);
            break;
        }
        if c.state != State::Roll {
            return None; // finished before it could be interrupted
        }
    }
    for _ in 0..20 {
        tick(&mut c);
    }
    Some(c)
}

#[test]
fn test_every_way_out_of_a_spin_tidies_up() {
    let ways: [(&str, Interrupt); 4] = [
        ("startled", |c| c.enter_startled(900.0, 500.0)),
        ("grabbed", |c| c.start_drag(820.0, 470.0)),
        ("skill switched off", |c| c.set_skill_enabled("roll", false, None)),
        ("sent somewhere else", |c| c.enter_idle()),
    ];
    for (label, interrupt) in ways {
        let mut tested = 0;
        for seed in SEEDS {
            let Some(c) = interrupt_and_settle(seed, interrupt) else { continue };
            tested += 1;
            assert_tidy(&c, &format!("seed {seed}, {label} mid-spin"));
        }
        assert!(tested >= 5, "{label}: only {tested} spiders were interrupted");
    }
}

#[test]
fn test_a_dragged_spider_is_tidied_before_it_is_drawn() {
    for seed in SEEDS {
        let mut c = build(seed);
        if !start_roll(&mut c) {
            continue;
        }
        for _ in 0..40 {
            tick(&mut c);
            if c.state != State::Roll {
                break;
            }
            if c.kin.roll_spin.abs() > 0.5 {
                c.start_drag(820.0, 470.0);
                common::step(&mut c, DT, 820.0, 470.0, &[]);
                assert_tidy(&c, &format!("seed {seed}, the first frame of being carried"));
                return;
            }
        }
    }
    panic!("no spider stayed in a spin long enough to be grabbed");
}

#[test]
fn test_no_frame_ever_ends_mid_tidy() {
    let mut rolls_seen = 0;
    for seed in SEEDS {
        let mut c = build(seed);
        c.set_skill_enabled("roll", true, None);
        let mut was_rolling = false;
        for frame in 0..900 {
            tick(&mut c);
            if c.state == State::Roll {
                was_rolling = true;
                continue;
            }
            if was_rolling {
                rolls_seen += 1;
                was_rolling = false;
            }
            assert!(
                c.kin.roll_spin.abs() < 1e-9,
                "seed {seed}, frame {frame}: a frame would have been drawn with {:.0} degrees of leftover spin",
                c.kin.roll_spin.abs().to_degrees()
            );
            assert!(
                c.kin.roll_tuck.abs() < 1e-9,
                "seed {seed}, frame {frame}: a frame would have been drawn with the legs still tucked"
            );
        }
    }
    assert!(rolls_seen >= 3, "only {rolls_seen} tumbles happened on their own");
}

// ---------------------------------------------------------------------
// The pinned health bar
// ---------------------------------------------------------------------

#[test]
fn test_pinning_shows_the_label_on_its_own() {
    let mut c = build(5);
    assert!(!c.label_visible(false), "an unnamed spider showed a label");
    c.set_health_label_pinned(true);
    assert!(c.health_label_pinned());
    assert!(c.label_visible(false), "pinning the health bar did not bring the label on screen");
    c.set_health_label_pinned(false);
    assert!(!c.label_visible(false));
}

#[test]
fn test_the_bar_is_inside_the_repaint_footprint() {
    // Python measured text with QFontMetrics; the approximate metrics stand
    // in, since the bar's height is what is being checked.
    let mut c = build(11);
    c.hovered = true;
    c.set_name("Boris");
    let plain = c.bounding_rect(true, &ApproxLabelMetrics);
    c.set_health_label_pinned(true);
    let pinned = c.bounding_rect(true, &ApproxLabelMetrics);
    assert!(
        pinned.1 < plain.1 - 2.0,
        "the bounding box did not grow upward for the bar: {:.1} -> {:.1}",
        plain.1,
        pinned.1
    );
}

#[test]
fn test_the_colour_says_what_the_number_says() {
    let mut c = build(13);
    assert_eq!(c.health_fraction(), 1.0);
    c.hp = c.max_hp * 0.5;
    assert!((c.health_fraction() - 0.5).abs() < 1e-6);
    c.hp = -50.0;
    assert_eq!(c.health_fraction(), 0.0, "health below zero should read as empty");

    let healthy = Creature::health_bar_color(1.0);
    let hurt = Creature::health_bar_color(0.45);
    let dying = Creature::health_bar_color(0.1);
    assert!(healthy[1] > healthy[0], "full health did not read as green");
    assert!(dying[0] > dying[1], "near-death did not read as red");
    assert!(
        hurt[0] != healthy[0] && hurt[0] != dying[0],
        "the middle of the range is not distinguishable from either end"
    );
}
