//! Port of `tests/test_retreat_ends.py` (DC-64): a retreat has to end, and a
//! spider that is safe but hurt walks home to heal.
//!
//! Ending a retreat (`escaped_timer`, `_call_off_retreat`) and raising or
//! dropping `recovering` are `CreatureManager._update_nerve`'s; the ported
//! tests cover what the creature does with those flags. Where the manager
//! would keep `recovering` set (hurt, below the rally point, a base to go
//! to), it is set by hand.
//!
//! not ported: test_a_retreat_ends_even_when_nothing_ever_heals (the exit is
//! CreatureManager's escape check).
//! not ported: test_it_does_not_end_up_pinned_in_a_corner (CreatureManager).
//! not ported: test_getting_away_is_what_ends_it_not_getting_well
//! (CreatureManager).
//! not ported: test_it_keeps_running_while_the_foe_is_still_close
//! (CreatureManager).
//! not ported: test_one_frame_of_respite_is_not_an_escape (`escaped_timer` is
//! CreatureManager's).
//! not ported: test_it_stops_recovering_once_patched_up (CreatureManager drops
//! `recovering`).
//! not ported: test_every_named_team_still_has_somewhere_to_run_to
//! (CreatureManager re-founds team bases).

mod common;

use dc_creature::constants::{ESCAPED_RADIUS, ESCAPED_SECONDS, THREAT_SCAN_RADIUS};
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

/// `(me, them)` at the given points, `them` a neighbour and a foe.
fn scene(me_at: (f64, f64), them_at: (f64, f64)) -> (Creature, Creature) {
    let mut me = spider(6, "pack_a", me_at.0, me_at.1);
    let them = spider(7, "pack_b", them_at.0, them_at.1);
    me.set_neighbors(vec![them.id]);
    (me, them)
}

fn step(me: &mut Creature, them: &CreatureView, bases: Option<&Bases>) {
    let mut pop = Population::from_views([them.clone()]);
    pop.set(me.view());
    let mut commands = Vec::new();
    let mut env = Env::bare(&pop, &mut commands);
    env.bases = bases.map(|b| b as &dyn BaseLookup);
    let (sw, sh) = (me.screen_w, me.screen_h);
    me.update(DT, AWAY.0, AWAY.1, sw, sh, &mut env);
}

fn dist(c: &Creature, p: (f64, f64)) -> f64 {
    (c.kin.x - p.0).hypot(c.kin.y - p.1)
}

// ------------------------------------------------------- the reported bug

#[test]
fn test_the_thing_it_was_running_from_is_let_go() {
    // The creature's half: once nothing refreshes the retreat, its own
    // update drops `flee_from` when the timer lapses.
    let (mut me, them) = scene((640.0, 450.0), (760.0, 450.0));
    let tv = them.view();
    me.hp = me.max_hp * 0.2;
    me.flee_timer = 1.4;
    me.flee_from = Some(them.id);
    step(&mut me, &tv, None);
    assert!(me.fleeing());
    assert_eq!(me.flee_from, Some(them.id));
    for _ in 0..(2 * 60) {
        step(&mut me, &tv, None);
    }
    assert!(!me.fleeing());
    assert_eq!(me.flee_from, None);
}

// ------------------------------------------------------------- escaping

#[test]
fn test_the_escape_radius_is_wider_than_the_one_that_starts_a_retreat() {
    const { assert!(ESCAPED_RADIUS > THREAT_SCAN_RADIUS) };
    const { assert!(ESCAPED_SECONDS > 0.0) };
}

// ----------------------------------------------- the calm half: walking home

#[test]
fn test_once_safe_but_still_hurt_it_walks_home() {
    let (mut me, them) = scene((900.0, 620.0), (1300.0, 850.0));
    let site = (220.0, 200.0);
    let bases = Bases(vec![("team:pack_a".into(), site)]);
    let tv = them.view();
    me.hp = me.max_hp * 0.2;

    let before = dist(&me, site);
    for _ in 0..(12 * 60) {
        me.recovering = true;
        step(&mut me, &tv, Some(&bases));
        me.hp = me.hp.min(me.max_hp * 0.2);
        assert!(!me.dead);
    }
    let after = dist(&me, site);
    assert!(after < before * 0.6, "{before} -> {after}");
}

#[test]
fn test_it_will_not_limp_home_into_the_enemy() {
    let site = (700.0, 450.0);
    let (mut me, them) = scene((1200.0, 800.0), (site.0 + 40.0, site.1));
    let bases = Bases(vec![("team:pack_a".into(), site)]);
    let tv = them.view();
    me.hp = me.max_hp * 0.2;

    let before = dist(&me, site);
    for _ in 0..(8 * 60) {
        me.recovering = true;
        step(&mut me, &tv, Some(&bases));
        me.hp = me.hp.min(me.max_hp * 0.2);
        assert!(!me.dead);
    }
    let after = dist(&me, site);
    assert!(
        after > ESCAPED_RADIUS * 0.5,
        "limped to {after:.0}px of a base with a foe on it (from {before:.0})"
    );
}

#[test]
fn test_a_spider_with_nowhere_to_heal_simply_gets_on_with_things() {
    let (mut me, them) = scene((640.0, 450.0), (760.0, 450.0));
    me.set_team("neutral");
    // Even a site filed under "neutral" is no base to a spider with no team.
    let bases = Bases(vec![("team:neutral".into(), (100.0, 100.0)), ("team:pack_b".into(), (300.0, 300.0))]);
    let pop = Population::from_views([them.view(), me.view()]);
    let mut commands = Vec::new();
    let mut env = Env::bare(&pop, &mut commands);
    env.bases = Some(&bases);
    assert_eq!(me.own_base_point(&env), None, "neutral was given a base after all");

    me.hp = me.max_hp * 0.2;
    me.recovering = true;
    assert!(!me.walk_home_to_heal(&env));
    assert!(!me.recovering);
}
