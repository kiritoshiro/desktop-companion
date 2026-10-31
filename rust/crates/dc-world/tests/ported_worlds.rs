//! Ports of the world parts of `test_world_determinism.py`, the fly-glob part
//! of `test_straight_shot.py`, the remains part of `test_damage_numbers.py`
//! and the persistence rules `test_scene_persistence.py` relies on.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::carcass::{
    ALLY_DAMAGE_COLOR, DAMAGE_NUMBER_SECONDS, DamageNumber, ENEMY_DAMAGE_COLOR, carcass_for, note_damage,
};
use dc_world::creature_view::{CreatureId, FlyId};
use dc_world::flies::{Fly, FlyState, ShooterPose, ShotTarget, WebShotProjectile};
use dc_world::webs::WebWorld;
use dc_world::{Cage, Point};
use serde_json::{Value, json};

// ------------------------------------------------------- world determinism

type WebSummary = Vec<(String, (f64, f64), usize, (f64, f64))>;

fn web_world(seed: u64) -> WebSummary {
    let mut world = WebWorld::new(1200.0, 800.0, PyRandom::from_u64(seed));
    let mut picker = PyRandom::from_u64(seed + 500);
    for i in 0..6 {
        world.claim_site(CreatureId(i), 24.0, i % 2 == 0, None, Some(&mut picker));
    }
    let ids: Vec<_> = world.webs.iter().map(|w| w.id).collect();
    ids.into_iter()
        .map(|id| {
            let p = world.walkable_point(id).unwrap();
            let w = world.web(id).unwrap();
            (w.pattern.clone(), w.hub, w.strands.len(), p)
        })
        .collect()
}

#[test]
fn a_seeded_web_world_replays() {
    assert_eq!(web_world(5), web_world(5));
    assert!(!web_world(5).is_empty(), "no webs were built; the test proves nothing");
}

#[test]
fn two_seeds_give_two_different_webs() {
    assert_ne!(web_world(5), web_world(6));
}

#[test]
fn where_a_spider_steps_onto_a_web_is_replayable() {
    let steps = || -> Vec<Point> {
        let mut world = WebWorld::new(1200.0, 800.0, PyRandom::from_u64(3));
        let id = world.claim_site(CreatureId(1), 24.0, true, None, Some(&mut PyRandom::from_u64(9))).unwrap();
        (0..12).map(|_| world.walkable_point(id).unwrap()).collect()
    };
    assert_eq!(steps(), steps());
}

// ---------------------------------------------------------- the fly glob

const SHOT_DT: f64 = 1.0 / 120.0;

fn pose() -> ShooterPose {
    ShooterPose { id: CreatureId(1), x: 100.0, y: 400.0, heading: 0.0, size: 12.0 }
}

fn still_fly(vx: f64, vy: f64) -> Fly {
    let mut fly = Fly::new(FlyId(7), 900.0, 400.0, 1920.0, 1080.0, Some(0.0), 1.0, &mut PyRandom::from_u64(1));
    (fly.vx, fly.vy) = (vx, vy);
    fly.state = FlyState::Flying;
    fly
}

#[test]
fn the_fly_glob_is_thrown_not_guided() {
    let led = WebShotProjectile::new(pose(), ShotTarget::Fly(FlyId(7)), 900.0, 400.0, 0.0, 900.0, "trap", 0.0);
    assert!(led.vel.1.atan2(led.vel.0) > 0.01, "the fly glob was not led toward where the fly is going");
    let straight = WebShotProjectile::new(pose(), ShotTarget::Fly(FlyId(7)), 900.0, 400.0, 0.0, 0.0, "trap", 0.0);
    assert!(straight.vel.1.abs() < 1e-6, "a shot at a hovering fly was aimed off-axis");
    assert_eq!(straight.homing, 0.0);

    let closest = |homing: f64| -> f64 {
        let mut flies = vec![still_fly(0.0, 0.0)];
        let mut shot =
            WebShotProjectile::new(pose(), ShotTarget::Fly(FlyId(7)), 900.0, 400.0, 0.0, 0.0, "trap", homing);
        let mut nobody: Vec<Bug> = Vec::new();
        let mut rng = PyRandom::from_u64(2);
        let mut nearest = f64::INFINITY;
        for step in 0..400 {
            if step as f64 * SHOT_DT > 0.03 {
                flies[0].vy = 520.0;
                flies[0].y += flies[0].vy * SHOT_DT;
            }
            shot.update(SHOT_DT, &mut flies, &mut nobody, &mut rng);
            nearest = nearest.min(dc_world::pymath::hypot(shot.pos.0 - flies[0].x, shot.pos.1 - flies[0].y));
            if shot.done {
                break;
            }
        }
        nearest
    };
    let (straight_near, tracking_near) = (closest(0.0), closest(1.0));
    assert!(tracking_near < straight_near * 0.7, "{tracking_near} vs {straight_near}");
}

// ------------------------------------------------------- remains' numbers

#[test]
fn the_killing_blow_shows_on_the_remains() {
    let mut spider = Bug::new(0, "none", "pack_a", 450.0, 350.0);
    spider.size = 18.0;
    let mut numbers = Vec::new();
    note_damage(&mut numbers, 40.0);
    let remains = carcass_for(&spider, None, &numbers);
    assert_eq!(remains.damage_numbers[0], DamageNumber { amount: 40.0, age: 0.0, nudge: 0.0 });
    let plain = carcass_for(&spider, None, &[]);
    assert!(remains.footprint().3 > plain.footprint().3, "its repaint area covers the number");
    let mut remains = remains;
    remains.update(DAMAGE_NUMBER_SECONDS, 0);
    assert!(remains.damage_numbers.is_empty());

    let mut enemy = Bug::new(1, "none", "rivals", 0.0, 0.0);
    enemy.size = 18.0;
    assert_eq!(carcass_for(&enemy, None, &[]).damage_color(), ENEMY_DAMAGE_COLOR);
    assert_eq!(carcass_for(&spider, None, &[]).damage_color(), ALLY_DAMAGE_COLOR);
    assert_ne!(ENEMY_DAMAGE_COLOR, ALLY_DAMAGE_COLOR);
}

// ---------------------------------------------------- scene persistence

#[test]
fn a_cage_round_trips_and_a_web_that_no_longer_fits_is_dropped() {
    let cage = Cage::new(40.0, 50.0, 300.0, 200.0);
    assert_eq!(Cage::from_dict(&cage.to_dict()), Some(cage));
    assert_eq!(Cage::from_dict(&json!({"x": 1})), None);

    let mut world = WebWorld::new(1600.0, 900.0, PyRandom::from_u64(1));
    world.claim_site(CreatureId(1), 30.0, true, None, Some(&mut PyRandom::from_u64(2))).unwrap();
    let saved = Value::Array(world.to_dict());
    let mut tiny = WebWorld::new(10.0, 10.0, PyRandom::from_u64(1));
    let far = world.webs[0].bbox();
    let fits = far.0 <= 10.0 && far.1 <= 10.0;
    assert_eq!(tiny.restore(&saved), usize::from(fits));
    let mut same = WebWorld::new(1600.0, 900.0, PyRandom::from_u64(1));
    assert_eq!(same.restore(&saved), 1);
    assert_eq!(Value::Array(same.to_dict()), saved);
}
