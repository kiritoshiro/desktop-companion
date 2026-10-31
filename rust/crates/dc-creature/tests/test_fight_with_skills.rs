//! Port of `tests/test_fight_with_skills.py` (DC-45): spiders fight with the
//! skills they have, and silk pins what it lands on.
//!
//! The Python duel is a `CreatureManager`; where the manager's only part is
//! publishing the foe, it is set by hand here and the other spider is a fixed
//! `CreatureView` in the population.
//!
//! not ported: test_a_spider_is_given_the_foe_it_is_fighting (foe selection is
//! CreatureManager's).
//! not ported: test_a_distant_foe_is_not_engaged (CreatureManager).
//! not ported: test_with_conflict_off_nobody_is_given_a_foe (CreatureManager).
//! not ported: test_a_web_shooter_pins_its_foe_before_closing (the hit needs the
//! fly world's projectiles and CreatureManager).
//! not ported: test_a_foe_can_be_webbed_with_mouse_trapping_turned_off
//! (CreatureManager / mouse web world).
//! not ported: test_a_pinned_foe_takes_a_harder_hit (`_trade_blow` is
//! CreatureManager's).
//! not ported: test_a_dead_foe_is_dropped_as_a_target (CreatureManager).
//! not ported: test_hunting_a_fly_still_wins_over_fighting (`_creature_focus` is
//! CreatureManager's).

mod common;

use dc_creature::constants::{WEBBED_HOLD_SECONDS, WEBBED_SECONDS, WEBBED_SPEED_MULT};
use dc_creature::world::{CreatureView, Env, FlyId, FlyWorldApi, Population, PreyView, Shooter, ShotKind};
use dc_creature::{Creature, State};

const DT: f64 = 1.0 / 60.0;
const SCREEN: (f64, f64) = (1200.0, 800.0);
const AWAY: (f64, f64) = (-5000.0, -5000.0);

fn spider(personality: &str, seed: i64, team: &str, x: f64, y: f64) -> Creature {
    let mut c =
        common::spider_with(common::load_model("tarantula"), common::load_personality(personality), seed, |cfg| {
            cfg.screen_w = SCREEN.0;
            cfg.screen_h = SCREEN.1;
        });
    c.set_team(team);
    c.kin.x = x;
    c.kin.y = y;
    c.initialize_legs();
    c
}

/// One spider of each hostile team, `apart` pixels apart, the left one
/// fighting the right one.
fn duel(left_personality: &str, right_personality: &str, apart: f64) -> (Creature, Creature) {
    let mut left = spider(left_personality, 8, "pack_a", 600.0 - apart / 2.0, 400.0);
    let right = spider(right_personality, 9, "pack_b", 600.0 + apart / 2.0, 400.0);
    left.foe = Some(right.id);
    (left, right)
}

/// A fly world with no flies that counts the silk thrown through it.
#[derive(Default)]
struct CountingFlies {
    shots: usize,
}

impl FlyWorldApi for CountingFlies {
    fn fly(&self, _id: FlyId) -> Option<PreyView> {
        None
    }
    fn launch_web_shot(&mut self, _shooter: &Shooter, _fly: FlyId, _kind: ShotKind) -> bool {
        self.shots += 1;
        true
    }
    fn launch_web_shot_at_creature(&mut self, _shooter: &Shooter, _target: &CreatureView, _kind: ShotKind) -> bool {
        self.shots += 1;
        true
    }
}

/// `CreatureManager._creature_focus`: a spider with a live foe is updated
/// with the foe's position in place of the cursor (no flies here).
fn focus(c: &Creature, others: &[CreatureView]) -> (f64, f64) {
    match c.foe.and_then(|id| others.iter().find(|o| o.id == id)) {
        Some(foe) if !foe.dead => (foe.x, foe.y),
        _ => AWAY,
    }
}

/// `step` with a fly world attached (so silk *could* be fired).
fn step_with_flies(c: &mut Creature, others: &[CreatureView], flies: &mut CountingFlies) {
    let mut pop = Population::from_views(others.iter().cloned());
    pop.set(c.view());
    let mut commands = Vec::new();
    let mut env = Env::bare(&pop, &mut commands);
    env.flies = Some(flies);
    let (sw, sh) = (c.screen_w, c.screen_h);
    let (mx, my) = focus(c, others);
    c.update(DT, mx, my, sw, sh, &mut env);
}

#[test]
fn test_a_spider_closes_on_its_foe() {
    // Creature-side: the foe holds still, so this measures only the pursuit.
    let (mut left, right) = duel("hunter", "mellow", 150.0);
    let rv = right.view();
    let start = (left.kin.x - right.kin.x).hypot(left.kin.y - right.kin.y);
    let mut closest = start;
    for _ in 0..(6 * 60) {
        let (mx, my) = focus(&left, std::slice::from_ref(&rv));
        common::step(&mut left, DT, mx, my, std::slice::from_ref(&rv));
        if left.dead {
            break;
        }
        closest = closest.min((left.kin.x - right.kin.x).hypot(left.kin.y - right.kin.y));
    }
    assert!(closest < start * 0.6, "{start} -> {closest}");
}

#[test]
fn test_a_spider_with_no_silk_never_fires_any() {
    let (mut left, mut right) = duel("hunter", "mellow", 150.0);
    right.foe = Some(left.id);
    for c in [&mut left, &mut right] {
        c.skills.set_enabled("shoot_web", false);
        c.skills.set_enabled("wall_web", false);
        c.web_shot_cooldown = 0.0;
    }
    let mut flies = CountingFlies::default();
    for _ in 0..(8 * 60) {
        let rv = right.view();
        step_with_flies(&mut left, std::slice::from_ref(&rv), &mut flies);
        let lv = left.view();
        step_with_flies(&mut right, std::slice::from_ref(&lv), &mut flies);
        if left.dead || right.dead {
            break;
        }
        assert!(!right.webbed() && !left.webbed());
        for c in [&left, &right] {
            assert!(!matches!(c.state, State::WebAim | State::WebShot), "a spider with no silk aimed some");
        }
        assert_eq!(flies.shots, 0);
    }
}

#[test]
fn test_being_webbed_holds_a_spider_then_slows_it() {
    let mut spider = spider("mellow", 1, "neutral", 600.0, 400.0);
    let free = spider.speed_mult();
    spider.web_pinned(None);
    assert!(spider.webbed());
    assert!(spider.webbed_held(), "fresh silk should hold, not merely slow");
    assert_eq!(spider.speed_mult(), 0.0);

    assert_eq!(WEBBED_HOLD_SECONDS, WEBBED_SECONDS);
    spider.webbed_timer = 0.05;
    assert!(spider.webbed_held() && spider.speed_mult() == 0.0);
    spider.webbed_timer = 0.0;
    assert!(!spider.webbed());
    assert!((spider.speed_mult() - free).abs() <= free.abs() * 1e-6);
    const { assert!(0.0 < WEBBED_SPEED_MULT && WEBBED_SPEED_MULT < 1.0) };
}

#[test]
fn test_silk_wears_off() {
    let (left, mut right) = duel("hunter", "mellow", 90.0);
    right.web_pinned(Some(left.id));
    assert!(right.webbed());
    assert_eq!(right.webbed_by, Some(left.id));
    let lv = left.view();
    for _ in 0..((WEBBED_SECONDS + 1.0) * 60.0) as usize {
        common::step(&mut right, DT, AWAY.0, AWAY.1, std::slice::from_ref(&lv));
        assert!(!right.dead);
    }
    assert!(!right.webbed());
    assert_eq!(right.webbed_by, None);
}

#[test]
fn test_silk_cannot_pin_a_spider_that_is_already_dead() {
    let (left, mut right) = duel("hunter", "mellow", 90.0);
    right.die();
    right.web_pinned(Some(left.id));
    assert!(!right.webbed());
}
