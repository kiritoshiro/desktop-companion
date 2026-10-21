//! Port of `tests/test_fight_choreography.py` (DC-59): a fight that looks
//! like a fight -- the stance, the facing, and the strike/recoil pose.
//!
//! The Python fixture is a `CreatureManager` brawl; the manager's only part
//! in the ported tests is publishing the foe, so here the foe id is set by
//! hand and the other spider is a fixed `CreatureView` in the population.
//!
//! not ported: test_a_fight_is_not_one_clump_of_bodies (needs CreatureManager:
//! both spiders updated together, contact combat).
//! not ported: test_two_spiders_fighting_do_not_cuddle (needs CreatureManager's
//! combat loop).
//! not ported: test_a_spider_walks_out_again_when_it_ends_up_too_close (needs
//! CreatureManager's combat loop).
//! not ported: test_a_blow_still_lands_at_the_distance_they_stand_at
//! (`CONTACT_REACH` is a dc-manager constant).
//! not ported: test_they_do_still_reach_each_other (blows are traded by
//! CreatureManager).

mod common;

use dc_creature::Creature;
use dc_creature::constants::{LUNGE_REACH, RECOIL_REACH};
use dc_creature::world::CreatureView;

const DT: f64 = 1.0 / 60.0;
const SCREEN: (f64, f64) = (420.0, 300.0);
const AWAY: (f64, f64) = (-9000.0, -9000.0);

fn spider(seed: i64, team: &str, x: f64, y: f64) -> Creature {
    let mut c = common::spider_with(common::load_model("tarantula"), common::load_personality("bold"), seed, |cfg| {
        cfg.screen_w = SCREEN.0;
        cfg.screen_h = SCREEN.1;
    });
    c.set_team(team);
    c.kin.x = x;
    c.kin.y = y;
    c.initialize_legs();
    // Unkillable, so the whole run is a fight rather than a funeral.
    c.max_hp = 100000.0;
    c.hp = 100000.0;
    c
}

/// `(left, right)`: hunters at (150, 165), rivals at (270, 165), with the
/// right one published as the left one's foe.
fn brawl() -> (Creature, Creature) {
    let mut left = spider(6, "hunters", 150.0, 165.0);
    let right = spider(7, "rivals", 270.0, 165.0);
    left.foe = Some(right.id);
    (left, right)
}

/// One frame for the left spider. As `CreatureManager._creature_focus` does,
/// a spider with a live foe is updated with the foe's position in place of
/// the cursor.
fn tick(left: &mut Creature, right: &CreatureView) {
    let (mx, my) = if left.foe == Some(right.id) && !right.dead { (right.x, right.y) } else { AWAY };
    common::step(left, DT, mx, my, std::slice::from_ref(right));
    left.hp = left.max_hp;
}

// ---------------------------------------------------------------- the stance

#[test]
fn test_a_spider_with_a_foe_squares_up() {
    let (mut left, right) = brawl();
    let rv = right.view();
    assert_eq!(left.combat_stance, 0.0);
    for _ in 0..60 {
        tick(&mut left, &rv);
    }
    assert!(left.combat_stance > 0.5, "stance {}", left.combat_stance);
    assert!(left.rear > 0.4, "squaring up did not rear the body");
}

#[test]
fn test_the_stance_drops_when_the_fight_does() {
    let (mut left, mut right) = brawl();
    for _ in 0..60 {
        tick(&mut left, &right.view());
    }
    assert!(left.combat_stance > 0.5);
    right.dead = true;
    left.foe = None;
    for _ in 0..120 {
        tick(&mut left, &right.view());
    }
    assert!(left.combat_stance < 0.05, "stance {}", left.combat_stance);
}

#[test]
fn test_a_fleeing_spider_does_not_hold_a_threat_pose() {
    let (mut left, right) = brawl();
    let rv = right.view();
    for _ in 0..60 {
        tick(&mut left, &rv);
    }
    left.flee_timer = 5.0;
    left.flee_from = Some(right.id);
    for _ in 0..60 {
        tick(&mut left, &rv);
        left.flee_timer = 5.0;
    }
    assert!(left.combat_stance < 0.5, "stance {}", left.combat_stance);
}

#[test]
fn test_a_squared_up_spider_faces_its_foe() {
    let (mut left, right) = brawl();
    let rv = right.view();
    for _ in 0..60 {
        tick(&mut left, &rv);
    }
    let (dx, dy) = (right.kin.x - left.kin.x, right.kin.y - left.kin.y);
    let mut span = dx.hypot(dy);
    if span == 0.0 {
        span = 1.0;
    }
    let dot = left.combat_face_x * dx / span + left.combat_face_y * dy / span;
    assert!(dot > 0.98, "{dot}");
}

// ------------------------------------------------------------ the blow itself

#[test]
fn test_a_landed_blow_throws_the_body_forward() {
    let (mut left, right) = brawl();
    let rv = right.view();
    tick(&mut left, &rv);
    left.strike_landed(right.kin.x, right.kin.y);
    let toward = (right.kin.x - left.kin.x, right.kin.y - left.kin.y);
    let mut toward_len = toward.0.hypot(toward.1);
    if toward_len == 0.0 {
        toward_len = 1.0;
    }
    let along = |c: &Creature| {
        let (ox, oy) = c.combat_body_offset();
        (ox * toward.0 + oy * toward.1) / toward_len
    };

    let first = along(&left);
    assert!(first < 0.0, "no wind-up: the body did not draw back first");
    let mut furthest = first;
    for _ in 0..30 {
        left.update_combat_pose(DT, Some(&rv));
        furthest = furthest.max(along(&left));
    }
    assert!(furthest >= LUNGE_REACH * left.kin.size, "{furthest} < {}", LUNGE_REACH * left.kin.size);
}

#[test]
fn test_a_blow_taken_throws_the_body_back() {
    let (mut left, right) = brawl();
    tick(&mut left, &right.view());
    left.blow_taken(right.kin.x, right.kin.y);
    assert!((left.lunge - -1.0).abs() < 1e-12);
    let (ox, oy) = left.combat_body_offset();
    let expected = RECOIL_REACH * left.kin.size;
    assert!((ox.hypot(oy) - expected).abs() <= expected * 1e-6, "{} vs {expected}", ox.hypot(oy));
    assert!(ox * (right.kin.x - left.kin.x) + oy * (right.kin.y - left.kin.y) < 0.0);
}

#[test]
fn test_the_lunge_decays_rather_than_sticking() {
    let (mut left, mut right) = brawl();
    tick(&mut left, &right.view());
    right.dead = true;
    left.foe = None;
    left.strike_landed(right.kin.x, right.kin.y);
    assert_ne!(left.combat_body_offset(), (0.0, 0.0));
    for _ in 0..90 {
        tick(&mut left, &right.view());
    }
    assert_eq!(left.combat_body_offset(), (0.0, 0.0));
}

#[test]
fn test_the_body_moves_and_the_feet_do_not() {
    let (mut left, right) = brawl();
    let rv = right.view();
    for _ in 0..30 {
        tick(&mut left, &rv);
    }
    let feet: Vec<(f64, f64)> = left.kin.legs.iter().map(|l| (l.foot_x, l.foot_y)).collect();
    left.strike_landed(right.kin.x, right.kin.y);
    let after: Vec<(f64, f64)> = left.kin.legs.iter().map(|l| (l.foot_x, l.foot_y)).collect();
    assert_eq!(after, feet);
    assert_ne!(left.combat_body_offset(), (0.0, 0.0));
}

#[test]
fn test_the_pose_changes_nothing_that_decides_a_fight() {
    let (mut left, right) = brawl();
    tick(&mut left, &right.view());
    let snapshot = |c: &Creature| (c.hp, c.damage, c.armor, c.attack_cooldown, c.kin.x, c.kin.y, c.kin.speed);
    let before = snapshot(&left);
    left.strike_landed(right.kin.x, right.kin.y);
    left.blow_taken(right.kin.x, right.kin.y);
    assert_eq!(snapshot(&left), before);
}
