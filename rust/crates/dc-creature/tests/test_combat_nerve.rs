//! Port of `tests/test_combat_nerve.py` (DC-50): fights that last, silk that
//! holds, and spiders with nerve.
//!
//! Who is in danger is decided by `CreatureManager._update_nerve`; what to do
//! about it is the creature's own (`_flee_from_danger`). The ported tests
//! cover the creature's half: the manager's part (keeping `flee_timer` and
//! `flee_from` fresh while the spider is cornered, as its branch 3 does) is
//! done by hand, and the foe is a fixed `CreatureView`.
//!
//! not ported: test_a_fight_survives_the_foe_stepping_out_of_engage_range
//! (foe selection is CreatureManager's).
//! not ported: test_a_foe_that_truly_leaves_is_dropped (CreatureManager).
//! not ported: test_a_hunter_is_allowed_to_pick_a_fight
//! (`CreatureManager._may_pick_a_fight`).
//! not ported: test_a_badly_hurt_spider_runs (the flee decision is
//! CreatureManager's `_update_nerve`).
//! not ported: test_a_healthy_spider_does_not_run (CreatureManager).
//! not ported: test_being_outnumbered_is_enough_on_its_own (CreatureManager).
//! not ported: test_an_even_match_is_not_outnumbered (CreatureManager).
//! not ported: test_it_stops_running_once_patched_up (calling off a retreat is
//! CreatureManager's).

mod common;

use dc_creature::constants::{FLEE_HEALTH_FRACTION, RALLY_HEALTH_FRACTION};
use dc_creature::world::{BaseLookup, CreatureView, Env, Population};
use dc_creature::Creature;
use dc_state::progression::normalize_team_stances;
use serde_json::json;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;
const SCREEN: (f64, f64) = (1400.0, 900.0);
const AWAY: (f64, f64) = (-9000.0, -9000.0);

/// `base_world.bases` reduced to what a creature reads: `team:<id>` -> site.
struct Bases(Vec<(String, (f64, f64))>);

impl BaseLookup for Bases {
    fn base_point(&self, key: &str) -> Option<(f64, f64)> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, p)| *p)
    }
}

fn spider(seed: i64, team: &str, x: f64, y: f64) -> Creature {
    let mut c = common::spider_with(common::load_model("tarantula"), common::load_personality("bold"), seed, |cfg| {
        cfg.screen_w = SCREEN.0;
        cfg.screen_h = SCREEN.1;
    });
    c.set_team(team);
    c.team_stances = Some(Arc::new(normalize_team_stances(Some(&json!({"pack_a": {"pack_b": "foe"}})))));
    c.kin.x = x;
    c.kin.y = y;
    c.initialize_legs();
    c
}

/// `(me, them)`: one bold tarantula per hostile team, `apart` pixels apart.
fn scene(apart: f64) -> (Creature, Creature) {
    let mut me = spider(6, "pack_a", 700.0 - apart / 2.0, 450.0);
    let them = spider(7, "pack_b", 700.0 + apart / 2.0, 450.0);
    me.set_neighbors(vec![them.id]);
    (me, them)
}

/// One frame of `me` with `them` in the population and optional bases.
fn step(me: &mut Creature, them: &CreatureView, bases: Option<&Bases>) {
    let mut pop = Population::from_views([them.clone()]);
    pop.set(me.view());
    let mut commands = Vec::new();
    let mut env = Env::bare(&pop, &mut commands);
    env.bases = bases.map(|b| b as &dyn BaseLookup);
    let (sw, sh) = (me.screen_w, me.screen_h);
    me.update(DT, AWAY.0, AWAY.1, sw, sh, &mut env);
}

/// What `_update_nerve` keeps doing for a cornered, hurt spider.
fn keep_frightened(me: &mut Creature, them: &Creature) {
    me.flee_timer = me.flee_timer.max(0.6);
    me.flee_from = Some(them.id);
    me.foe = None;
    me.hp = me.hp.min(me.max_hp * 0.2);
}

fn gap(a: &Creature, b: &Creature) -> f64 {
    (a.kin.x - b.kin.x).hypot(a.kin.y - b.kin.y)
}

// ---------------------------------------------------------------------- silk

#[test]
fn test_fresh_silk_holds_a_spider_still() {
    let (ours, mut victim) = scene(120.0);
    victim.web_pinned(Some(ours.id));
    assert!(victim.webbed_held());
    assert_eq!(victim.speed_mult(), 0.0);

    let (start_x, start_y) = (victim.kin.x, victim.kin.y);
    let ov = ours.view();
    for _ in 0..30 {
        step(&mut victim, &ov, None);
        if !victim.webbed_held() {
            break;
        }
    }
    let moved = (victim.kin.x - start_x).hypot(victim.kin.y - start_y);
    assert!(moved < 6.0, "a held spider walked {moved:.1}px");
}

// --------------------------------------------------------------------- nerve

#[test]
fn test_running_opens_the_distance() {
    // Creature-side: the threat holds still, so this measures only the run.
    let (mut me, them) = scene(90.0);
    let tv = them.view();
    me.hp = me.max_hp * 0.2;
    let start = gap(&me, &them);
    let mut best = start;
    for _ in 0..(4 * 60) {
        keep_frightened(&mut me, &them);
        step(&mut me, &tv, None);
        assert!(!me.dead);
        best = best.max(gap(&me, &them));
    }
    assert!(best > start * 1.5, "{start} -> {best}");
}

#[test]
fn test_the_rally_point_is_higher_than_the_flee_point() {
    const { assert!(RALLY_HEALTH_FRACTION > FLEE_HEALTH_FRACTION + 0.2) };
}

#[test]
fn test_a_hurt_spider_runs_towards_its_own_base() {
    // Built where the test places them, so the legs start under the body.
    let mut me = spider(6, "pack_a", 700.0, 450.0);
    let them = spider(7, "pack_b", 900.0, 450.0);
    me.set_neighbors(vec![them.id]);
    let site = (200.0, 450.0);
    let bases = Bases(vec![("team:pack_a".into(), site)]);
    let tv = them.view();

    me.hp = me.max_hp * 0.2;
    let before = (me.kin.x - site.0).hypot(me.kin.y - site.1);
    for _ in 0..(3 * 60) {
        keep_frightened(&mut me, &them);
        step(&mut me, &tv, Some(&bases));
    }
    let after = (me.kin.x - site.0).hypot(me.kin.y - site.1);
    assert!(after < before * 0.7, "{before} -> {after}");
}

#[test]
fn test_a_spider_with_no_base_still_runs_away() {
    let (mut me, them) = scene(90.0);
    let tv = them.view();
    let bases = Bases(Vec::new());
    me.hp = me.max_hp * 0.2;
    let start = gap(&me, &them);
    let mut best = start;
    for _ in 0..(3 * 60) {
        keep_frightened(&mut me, &them);
        step(&mut me, &tv, Some(&bases));
        best = best.max(gap(&me, &them));
    }
    assert!(best > start * 1.4, "{start} -> {best}");
}
