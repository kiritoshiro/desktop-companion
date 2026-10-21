//! Port of `tests/test_perception_snapshot.py`: behaviour reads the world
//! through a `Perception` snapshot (DC-17, C1).
//!
//! The Rust `Perception` passes the creature's position (not the creature)
//! to the web world, and `Creature` keeps no `perception` attribute: one is
//! built from the frame's `Env` when a decision needs it.
//!
//! not ported: test_behaviour_no_longer_makes_this_direct_world_query (5 params; greps Python source text).
//! not ported: test_behaviour_still_reads_perception_for_its_web_decisions (greps Python source text).

mod common;

use dc_creature::Creature;
use dc_creature::perception::Perception;
use dc_creature::world::{
    Command, CreatureView, Env, FlyId, FlyWorldApi, Population, PreyView, Shooter, ShotKind, WebId, WebWorldApi,
};
use std::cell::RefCell;

const REPAIRABLE: WebId = 101;
const ADOPTABLE: WebId = 202;
const WALKABLE: WebId = 303;

/// `FakeWebWorld`: answers every query and records what it was asked.
struct FakeWebWorld {
    repairable_calls: RefCell<Vec<(f64, f64, f64)>>,
    adoptable_calls: RefCell<Vec<(f64, f64, f64)>>,
    walkable_calls: RefCell<Vec<(f64, f64, f64)>>,
    intact_count: usize,
}

impl FakeWebWorld {
    fn new() -> Self {
        FakeWebWorld {
            repairable_calls: RefCell::new(Vec::new()),
            adoptable_calls: RefCell::new(Vec::new()),
            walkable_calls: RefCell::new(Vec::new()),
            intact_count: 3,
        }
    }
}

impl WebWorldApi for FakeWebWorld {
    fn find_repairable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.repairable_calls.borrow_mut().push((x, y, max_dist));
        Some(REPAIRABLE)
    }
    fn find_adoptable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.adoptable_calls.borrow_mut().push((x, y, max_dist));
        Some(ADOPTABLE)
    }
    fn find_walkable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.walkable_calls.borrow_mut().push((x, y, max_dist));
        Some(WALKABLE)
    }
    fn intact_complete_count(&self) -> usize {
        self.intact_count
    }
    fn claim_site(&mut self, _builder: u64, _size: f64, _prefer_corner: bool) -> Option<WebId> {
        None
    }
    fn adopt(&mut self, _web: WebId, _builder: u64) -> bool {
        false
    }
    fn abandon(&mut self, _web: WebId) {}
    fn claim_repair(&mut self, _web: WebId, _repairer: u64) -> bool {
        false
    }
    fn release_repair(&mut self, _web: WebId) {}
    fn contains(&self, _web: WebId) -> bool {
        false
    }
    fn is_complete(&self, _web: WebId) -> bool {
        false
    }
    fn is_damaged(&self, _web: WebId) -> bool {
        false
    }
    fn damaged_centroid(&self, _web: WebId) -> Option<(f64, f64)> {
        None
    }
    fn reach_inset(&self, _web: WebId) -> f64 {
        0.0
    }
    fn hub(&self, _web: WebId) -> (f64, f64) {
        (0.0, 0.0)
    }
    fn working_point(&self, _web: WebId, _screen_w: f64, _screen_h: f64) -> ((f64, f64), bool) {
        ((0.0, 0.0), false)
    }
    fn advance(&mut self, _web: WebId, _dt: f64, _weave_speed: f64, _spider: (f64, f64), _lead_max: f64) {}
    fn repair_near(&mut self, _web: WebId, _x: f64, _y: f64, _dt: f64) {}
    fn walkable_point(&mut self, _web: WebId, _screen_w: f64, _screen_h: f64) -> (f64, f64) {
        (0.0, 0.0)
    }
    fn pluck(&mut self, _web: WebId, _at: (f64, f64), _strength: f64) {}
}

/// Stands in for Python's `fly_world=object()`: present, with no flies.
struct FakeFlyWorld;

impl FlyWorldApi for FakeFlyWorld {
    fn fly(&self, _id: FlyId) -> Option<PreyView> {
        None
    }
    fn launch_web_shot(&mut self, _shooter: &Shooter, _fly: FlyId, _kind: ShotKind) -> bool {
        false
    }
    fn launch_web_shot_at_creature(&mut self, _shooter: &Shooter, _target: &CreatureView, _kind: ShotKind) -> bool {
        false
    }
}

/// `Creature(load_model("tarantula"), load_personality("mellow"), 1600, 900)`, seeded.
fn build_creature() -> Creature {
    common::spider("tarantula", "mellow", 1)
}

fn env_with<'a>(
    pop: &'a Population,
    commands: &'a mut Vec<Command>,
    webs: Option<&'a mut dyn WebWorldApi>,
    flies: Option<&'a mut dyn FlyWorldApi>,
) -> Env<'a> {
    let mut env = Env::bare(pop, commands);
    env.webs = webs;
    env.flies = flies;
    env
}

#[test]
fn test_perception_delegates_to_the_web_world_lazily() {
    let creature = build_creature();
    let mut world = FakeWebWorld::new();
    let pop = Population::from_views([creature.view()]);
    let mut commands = Vec::new();
    {
        let env = env_with(&pop, &mut commands, Some(&mut world), None);
        let perception = Perception::new(&creature, &env);
        assert!(perception.has_web_world);
        assert_eq!(perception.repairable_web(250.0), Some(REPAIRABLE));
        assert_eq!(perception.adoptable_web(300.0), Some(ADOPTABLE));
        assert_eq!(perception.walkable_web(400.0), Some(WALKABLE));
        assert_eq!(perception.intact_web_count(), 3);
        assert!(!perception.can_shoot_prey_web());
    }
    // The creature (its position) and the caller's own max_dist reach the
    // world unchanged.
    let (x, y) = (creature.kin.x, creature.kin.y);
    assert_eq!(*world.repairable_calls.borrow(), vec![(x, y, 250.0)]);
    assert_eq!(*world.adoptable_calls.borrow(), vec![(x, y, 300.0)]);
    assert_eq!(*world.walkable_calls.borrow(), vec![(x, y, 400.0)]);
}

#[test]
fn test_perception_with_no_world_answers_safely_instead_of_scanning() {
    let creature = build_creature();
    let pop = Population::from_views([creature.view()]);
    let mut commands = Vec::new();
    let env = Env::bare(&pop, &mut commands);
    let perception = Perception::new(&creature, &env);
    assert!(!perception.has_web_world);
    assert_eq!(perception.repairable_web(1e9), None);
    assert_eq!(perception.adoptable_web(1e9), None);
    assert_eq!(perception.walkable_web(1e9), None);
    assert_eq!(perception.intact_web_count(), 0);
    assert!(!perception.can_shoot_prey_web());
}

#[test]
fn test_can_shoot_prey_web_reflects_the_fly_world() {
    let creature = build_creature();
    let pop = Population::from_views([creature.view()]);
    let mut commands = Vec::new();
    {
        let env = Env::bare(&pop, &mut commands);
        assert!(!Perception::new(&creature, &env).can_shoot_prey_web());
    }
    let mut flies = FakeFlyWorld;
    let env = env_with(&pop, &mut commands, None, Some(&mut flies));
    assert!(Perception::new(&creature, &env).can_shoot_prey_web());
}

/// Python's version also checked `creature.perception` (None before the
/// first update, a new object each tick); the port keeps no such attribute,
/// so this checks what the snapshot sees across two updates instead: none
/// of the worlds on a bare frame, then the worlds wired into the next one.
#[test]
fn test_creature_update_builds_a_fresh_perception_every_tick() {
    let mut creature = build_creature();
    let dt = 1.0 / 60.0;
    let mut commands = Vec::new();
    {
        let pop = Population::from_views([creature.view()]);
        let mut env = Env::bare(&pop, &mut commands);
        let (sw, sh) = (creature.screen_w, creature.screen_h);
        creature.update(dt, 0.0, 0.0, sw, sh, &mut env);
        let first = Perception::new(&creature, &env);
        assert!(!first.has_web_world);
    }

    let mut world = FakeWebWorld::new();
    let mut flies = FakeFlyWorld;
    let pop = Population::from_views([creature.view()]);
    let mut env = env_with(&pop, &mut commands, Some(&mut world), Some(&mut flies));
    let (sw, sh) = (creature.screen_w, creature.screen_h);
    creature.update(dt, 0.0, 0.0, sw, sh, &mut env);
    let second = Perception::new(&creature, &env);
    assert!(second.has_web_world);
    assert!(second.can_shoot_prey_web());
    assert_eq!(second.intact_web_count(), 3);
}
