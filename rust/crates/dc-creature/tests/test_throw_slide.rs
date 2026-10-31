//! Port of `tests/test_throw_slide.py`: a thrown spider slides to a stop
//! rather than running on under its own power.
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature.

mod common;

use dc_creature::Creature;

const DT: f64 = 1.0 / 60.0;
const SCREEN: (f64, f64) = (2400.0, 1400.0);
/// Far enough that the cursor cannot provoke a reaction.
const CURSOR: (f64, f64) = (-100000.0, -100000.0);

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// `Creature(model, personality, *SCREEN, index=, progression_id=)` placed at
/// `(x, y)` with its legs initialised there.
fn build(personality: &str, seed: i64, index: i64, progression_id: &str, x: f64, y: f64) -> Creature {
    let mut c =
        common::spider_with(common::load_model("tarantula"), common::load_personality(personality), seed, |cfg| {
            cfg.screen_w = SCREEN.0;
            cfg.screen_h = SCREEN.1;
            cfg.index = index;
            cfg.id = seed as u64 * 10 + index as u64 + 1;
            cfg.progression_id = Some(progression_id.to_string());
        });
    c.kin.x = x;
    c.kin.y = y;
    c.initialize_legs();
    c
}

/// Reproduce a release with hand velocity, the way the manager does.
fn throw(spider: &mut Creature, vx: f64, vy: f64, cursor: (f64, f64)) {
    spider.dragging = true;
    spider.drag_vel_x = vx;
    spider.drag_vel_y = vy;
    spider.release_drag(cursor.0, cursor.1);
}

/// Per-frame (x, y, inertia_timer) samples.
fn simulate(seconds: f64, spider: &mut Creature) -> Vec<(f64, f64, f64)> {
    let steps = (seconds / DT) as usize;
    let mut samples = Vec::with_capacity(steps);
    for _ in 0..steps {
        common::step(spider, DT, CURSOR.0, CURSOR.1, &[]);
        samples.push((spider.kin.x, spider.kin.y, spider.kin.inertia_timer));
    }
    samples
}

#[test]
fn test_slide_decelerates() {
    let mut spider = build("mellow", 11, 0, "throw:0", 400.0, 700.0);
    throw(&mut spider, 900.0, 0.0, CURSOR);
    assert!(spider.kin.inertia_timer > 0.0, "a hard throw produced no inertia at all");
    let samples = simulate(3.0, &mut spider);

    let sliding: Vec<_> = samples.iter().filter(|s| s.2 > 0.0).copied().collect();
    assert!(sliding.len() > 8, "the slide lasted {} frames", sliding.len());
    let steps: Vec<f64> =
        (1..sliding.len()).map(|i| dist((sliding[i].0, sliding[i].1), (sliding[i - 1].0, sliding[i - 1].1))).collect();
    let third = (steps.len() / 3).max(1);
    let first_third = steps[..steps.len() / 3].iter().sum::<f64>() / third as f64;
    let last_third = steps[steps.len() - steps.len() / 3..].iter().sum::<f64>() / third as f64;
    assert!(last_third < first_third * 0.5, "the slide did not decelerate: {first_third:.2} -> {last_third:.2}");

    let dx = sliding.last().unwrap().0 - sliding[0].0;
    let dy = sliding.last().unwrap().1 - sliding[0].1;
    assert!(dx > 60.0, "the throw did not carry the spider forward: dx={dx:.1}");
    assert!(dy.abs() < dx.abs(), "the spider went sideways more than forward: dx={dx:.1} dy={dy:.1}");
}

/// Ground covered after friction has finished. The cursor stays at the point
/// of release, where a hand actually leaves it.
fn distance_after_slide(seed: i64, seconds: f64, speed: f64) -> f64 {
    let cursor = (400.0, 700.0);
    let mut spider = build("mellow", seed, 0, "throw:1", 400.0, 700.0);
    throw(&mut spider, speed, 0.0, cursor);
    let mut travelled = 0.0;
    let mut previous: Option<(f64, f64)> = None;
    for _ in 0..(seconds / DT) as usize {
        common::step(&mut spider, DT, cursor.0, cursor.1, &[]);
        if spider.kin.inertia_timer <= 0.0 {
            let here = (spider.kin.x, spider.kin.y);
            if let Some(p) = previous {
                travelled += dist(p, here);
            }
            previous = Some(here);
        }
    }
    travelled
}

const SEEDS: [i64; 5] = [7, 11, 23, 42, 99];

#[test]
fn test_does_not_run_on() {
    let travelled: Vec<f64> = SEEDS.iter().map(|&s| distance_after_slide(s, 6.0, 900.0)).collect();
    let worst = travelled.iter().cloned().fold(0.0, f64::max);
    assert!(
        worst < 400.0,
        "after the slide the spider covered up to {worst:.0}px (per seed: {travelled:?}); that is far enough \
         to suggest it is walking somewhere rather than sliding to a stop"
    );
}

#[test]
fn test_no_projected_target() {
    for seed in SEEDS {
        let mut spider = build("mellow", seed, 0, "throw:5", 400.0, 700.0);
        throw(&mut spider, 900.0, 0.0, (400.0, 700.0));
        let aimed = dist((spider.kin.x, spider.kin.y), (spider.kin.target_x, spider.kin.target_y));
        assert!(aimed < 1.0, "seed {seed}: a throw aimed the spider {aimed:.0}px further along the throw heading");
        assert!(spider.throw_recovery > 0.0, "seed {seed}: a throw scheduled no recovery beat");
    }
}

#[test]
fn test_recovery_beat_is_held() {
    let mut spider = build("mellow", 3, 0, "throw:2", 400.0, 700.0);
    throw(&mut spider, 850.0, 0.0, CURSOR);
    assert!(spider.throw_recovery > 0.0, "a throw scheduled no recovery beat");

    let mut held: Option<(f64, f64)> = None;
    for _ in 0..(3.0 / DT) as usize {
        common::step(&mut spider, DT, CURSOR.0, CURSOR.1, &[]);
        if spider.kin.inertia_timer <= 0.0 && spider.throw_recovery > 0.0 && held.is_none() {
            held = Some((spider.kin.x, spider.kin.y));
        }
        if spider.kin.inertia_timer <= 0.0 && spider.throw_recovery <= 0.0 {
            break;
        }
    }
    let held = held.expect("the recovery beat was never entered");
    let settled = dist(held, (spider.kin.x, spider.kin.y));
    assert!(settled < 12.0, "the spider drifted {settled:.1}px during the recovery beat");
}

#[test]
fn test_gentle_drop_still_scurries() {
    // Placed still, with no measurable hand movement at all.
    let mut spider = build("skittish", 5, 0, "throw:3", 1200.0, 700.0);
    throw(&mut spider, 0.5, 0.0, CURSOR);
    assert_eq!(spider.kin.inertia_timer, 0.0, "a still release should not create inertia");
    assert_eq!(spider.throw_recovery, 0.0, "a still release should not schedule a recovery beat");
    assert!(
        dist((spider.kin.x, spider.kin.y), (spider.kin.target_x, spider.kin.target_y)) > 40.0,
        "a dropped spider was given nowhere to scurry to"
    );

    // A nudge: enough for a little inertia, not enough to be a throw.
    let mut nudged = build("skittish", 5, 1, "throw:4", 1200.0, 700.0);
    throw(&mut nudged, 30.0, 0.0, CURSOR);
    assert!(nudged.kin.inertia_timer > 0.0, "a nudge should carry some inertia");
    assert_eq!(nudged.throw_recovery, 0.0, "a nudge is not a throw and needs no recovery beat");
    assert!(
        dist((nudged.kin.x, nudged.kin.y), (nudged.kin.target_x, nudged.kin.target_y)) > 40.0,
        "a nudged spider was given nowhere to scurry to"
    );
}
