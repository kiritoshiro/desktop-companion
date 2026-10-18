//! Ports of the Python tests for `world/jobs.py` that need no manager:
//! `test_jobs.py`, the `BaseWorld` half of `test_jobs_economy.py`,
//! `test_bases_on_screen.py`, `test_base_mounds.py`, the patch and
//! move parts of `test_base_ground.py` and the job half of
//! `test_trapping_is_reachable.py`.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::creature_view::{CreatureId, JobMode};
use dc_world::jobs::*;
use dc_world::webs::{Web, WebState, WebWorld, plan_orb};
use dc_world::{Playfield, ScreenRect};
use serde_json::Value;

// ---------------------------------------------------------------- test_jobs

struct Colony {
    world: BaseWorld,
    rng: PyRandom,
    bugs: Vec<Bug>,
}

const SITE: &str = "team:pack_a";

impl Colony {
    /// The `colony` fixture: a builder and a guard of one team, and an
    /// intruder, after one frame.
    fn new() -> Colony {
        let bugs = vec![
            Bug::dummy("builder", "pack_a", 200.0, 200.0, 1),
            Bug::dummy("guard", "pack_a", 230.0, 200.0, 2),
            Bug::dummy("none", "rivals", 246.0, 200.0, 3),
        ];
        let mut world = BaseWorld::new(800.0, 600.0, &[]);
        world.ensure_site(&bugs[0]).resources = 1000.0;
        let mut c = Colony { world, rng: PyRandom::from_u64(4), bugs };
        c.step(None);
        c
    }

    fn step(&mut self, webs: Option<&mut WebWorld>) {
        self.world.update(DT, &mut self.bugs, webs, &mut self.rng);
    }

    fn site(&self) -> &BaseSite {
        self.world.base(SITE).unwrap()
    }

    fn site_mut(&mut self) -> &mut BaseSite {
        self.world.base_mut(SITE).unwrap()
    }

    fn builder(&self) -> &Bug {
        &self.bugs[0]
    }
    fn guard(&self) -> &Bug {
        &self.bugs[1]
    }
}

#[test]
fn the_jobs_are_the_six_a_spider_can_hold() {
    assert_eq!(JOB_IDS, ["none", "hunter", "builder", "guard", "scout", "webber"]);
    assert_eq!(normalize_job_id(Some("weaver")), "webber");
}

#[test]
fn a_builder_founds_a_base_and_a_guard_answers_the_intruder() {
    let c = Colony::new();
    assert_eq!(c.world.bases.len(), 2);
    let mut teams: Vec<&str> = c.world.bases.iter().map(|b| b.team_id.as_str()).collect();
    teams.sort();
    assert_eq!(teams, ["pack_a", "rivals"]);
    assert_eq!(c.builder().base_id.as_deref(), Some(SITE));
    assert_eq!(c.builder().mode, JobMode::Build);
    assert_eq!(c.guard().mode, JobMode::GuardAlert);
    assert!(c.site().alert > 0.0);
}

#[test]
fn a_world_survives_a_round_trip() {
    let c = Colony::new();
    let saved = c.world.to_dict();
    assert_eq!(BaseWorld::new(800.0, 600.0, &saved).to_dict(), saved);
}

#[test]
fn a_base_under_construction_reports_partial_completion() {
    let c = Colony::new();
    let done = c.site().completion();
    assert!(0.0 < done && done < 0.25, "{done}");
}

#[test]
fn work_is_a_shift_not_ownership() {
    let mut c = Colony::new();
    let mut modes = Vec::new();
    for _ in 0..(60.0 * (BUILD_DUTY_ON.1 + BUILD_DUTY_OFF.1 + 2.0)) as usize {
        c.step(None);
        modes.push(c.builder().mode);
    }
    assert!(modes.contains(&JobMode::Build), "builder never worked");
    assert!(modes.contains(&JobMode::Idle), "builder never came off duty");
    let idle = modes.iter().filter(|m| **m == JobMode::Idle).count() as f64;
    assert!(60.0 * BUILD_DUTY_OFF.0 <= idle, "{idle}");
}

#[test]
fn a_busy_builder_earns_its_base_nothing() {
    let mut c = Colony::new();
    c.bugs[0].job_busy = true;
    let before = c.site().build_progress;
    for _ in 0..600 {
        c.step(None);
        assert_ne!(c.builder().mode, JobMode::Build, "a busy builder still claimed build work");
    }
    assert_eq!(c.site().build_progress, before);
}

#[test]
fn a_finished_base_releases_its_builder() {
    let mut c = Colony::new();
    for _ in 0..60 * 600 {
        c.step(None);
        let s = c.site();
        if s.build_progress >= MAX_BUILD_PROGRESS && s.integrity >= s.max_integrity {
            break;
        }
    }
    assert_eq!(c.site().build_progress, MAX_BUILD_PROGRESS);
    assert_eq!(c.site().integrity, c.site().max_integrity);
    assert_eq!(c.site().completion(), 1.0);
    for _ in 0..(60.0 * (BUILD_DUTY_ON.1 + BUILD_DUTY_OFF.1 + 2.0)) as usize {
        c.step(None);
        assert_eq!(c.builder().mode, JobMode::Idle);
    }
    assert!(c.builder().target.is_none());

    // Serious damage cancels the break at once.
    let max = c.site().max_integrity;
    c.site_mut().integrity = max * 0.4;
    c.step(None);
    assert!(matches!(c.builder().mode, JobMode::Build | JobMode::BuildTravel), "{:?}", c.builder().mode);
    assert!(c.world.duty_state(c.builder()).unwrap().on_duty);
}

#[test]
fn the_patrol_ring_keeps_turning_while_the_guard_rests() {
    let mut c = Colony::new();
    (c.bugs[2].x, c.bugs[2].y) = (780.0, 580.0);
    let (mut modes, mut angles) = (Vec::new(), Vec::new());
    for _ in 0..(60.0 * (PATROL_DUTY_ON.1 + PATROL_DUTY_OFF.1 + 2.0)) as usize {
        c.step(None);
        modes.push(c.guard().mode);
        if c.guard().mode == JobMode::Idle {
            angles.push(c.site().patrol_angle);
        }
    }
    assert!(modes.contains(&JobMode::Patrol), "guard never patrolled");
    assert!(modes.contains(&JobMode::Idle), "guard never came off duty");
    angles.dedup();
    assert!(angles.len() > 1, "patrol ring stalled while the guard was off duty");
}

#[test]
fn an_intruder_ends_the_break_on_the_frame_it_appears() {
    let mut c = Colony::new();
    (c.bugs[2].x, c.bugs[2].y) = (780.0, 580.0);
    for _ in 0..(60.0 * (PATROL_DUTY_ON.1 + PATROL_DUTY_OFF.1 + 2.0)) as usize {
        c.step(None);
    }
    let (sx, sy) = (c.site().x, c.site().y);
    (c.bugs[2].x, c.bugs[2].y) = (sx + 20.0, sy);
    c.step(None);
    assert_eq!(c.guard().mode, JobMode::GuardAlert);
    assert!(c.world.duty_state(c.guard()).unwrap().on_duty);
}

#[test]
fn a_busy_guard_is_not_dragged_back_onto_patrol() {
    let mut c = Colony::new();
    let (sx, sy) = (c.site().x, c.site().y);
    (c.bugs[2].x, c.bugs[2].y) = (sx + 20.0, sy);
    c.bugs[1].job_busy = true;
    c.step(None);
    assert_eq!(c.guard().mode, JobMode::Idle);
}

fn with_scout(c: &mut Colony) -> usize {
    let (sx, sy) = (c.site().x, c.site().y);
    c.bugs.push(Bug::dummy("scout", "pack_a", sx, sy, 5));
    c.bugs.len() - 1
}

#[test]
fn a_scout_covers_sectors_and_reports_to_the_blackboard() {
    let mut c = Colony::new();
    let s = with_scout(&mut c);
    c.step(None);
    assert_eq!(c.bugs[s].mode, JobMode::ScoutTravel);
    let mut target = c.bugs[s].target.unwrap();
    let mut modes = Vec::new();
    for _ in 0..60 * 8 {
        (c.bugs[s].x, c.bugs[s].y) = target;
        c.step(None);
        modes.push(c.bugs[s].mode);
        target = c.bugs[s].target.unwrap_or(target);
    }
    assert!(modes.contains(&JobMode::ScoutReport));
    assert!(!c.site().points_of_interest.is_empty(), "scout never published anything");
}

#[test]
fn a_scout_reports_a_foe_it_finds_in_a_sector() {
    let mut c = Colony::new();
    let s = with_scout(&mut c);
    c.step(None);
    let mut target = c.bugs[s].target.unwrap();
    (c.bugs[2].x, c.bugs[2].y) = target;
    for _ in 0..60 * 8 {
        (c.bugs[s].x, c.bugs[s].y) = target;
        c.step(None);
        target = c.bugs[s].target.unwrap_or(target);
    }
    assert!(c.site().points_of_interest.iter().any(|p| p["kind"] == "foe"), "{:?}", c.site().points_of_interest);
}

#[test]
fn a_busy_scout_does_not_claim_a_sector() {
    let mut c = Colony::new();
    let s = with_scout(&mut c);
    c.bugs[s].job_busy = true;
    c.step(None);
    assert_eq!(c.bugs[s].mode, JobMode::Idle);
}

fn web_at(webs: &mut WebWorld, x: f64, y: f64) -> dc_world::WebId {
    let plan = plan_orb((x, y), 80.0, 800.0, 600.0, &mut PyRandom::from_u64(3));
    let mut web: Web = webs.make_web("orb", plan, &format!("test_{x}_{y}"), 20.0);
    web.state = WebState::Complete;
    web.built = web.strands.len();
    let id = web.id;
    webs.webs.push(web);
    id
}

fn with_webber(c: &mut Colony, x: f64, y: f64) -> usize {
    let mut b = Bug::dummy("webber", "pack_a", x, y, 6);
    b.size = 24.0; // `getattr(creature, "size", 24.0)` on the Python dummy
    c.bugs.push(b);
    c.bugs.len() - 1
}

#[test]
fn a_webber_travels_to_a_torn_web_and_mends_it() {
    let mut c = Colony::new();
    let mut webs = WebWorld::new(800.0, 600.0, PyRandom::from_u64(1));
    let (sx, sy) = (c.site().x, c.site().y);
    let id = web_at(&mut webs, sx + 60.0, sy);
    let web = webs.web_mut(id).unwrap();
    let (si, segi, _len) = web.tearable()[0];
    web.cut.insert((si, segi));
    assert!(web.is_damaged());

    let w = with_webber(&mut c, sx + 300.0, sy + 200.0);
    c.step(Some(&mut webs));
    assert_eq!(c.bugs[w].mode, JobMode::WebTravel);
    let mut target = c.bugs[w].target.unwrap();
    let mut modes = Vec::new();
    for _ in 0..60 * 10 {
        (c.bugs[w].x, c.bugs[w].y) = target;
        c.step(Some(&mut webs));
        modes.push(c.bugs[w].mode);
        target = c.bugs[w].target.unwrap_or(target);
    }
    assert!(modes.contains(&JobMode::WebRepair));
    assert!(!webs.web(id).unwrap().is_damaged(), "webber never finished mending the torn web");
}

#[test]
fn a_webber_tops_up_the_teams_web_count_when_nothing_is_torn() {
    let mut c = Colony::new();
    let mut webs = WebWorld::new(800.0, 600.0, PyRandom::from_u64(1));
    let (sx, sy) = (c.site().x, c.site().y);
    let w = with_webber(&mut c, sx, sy);
    c.step(Some(&mut webs));
    let mut target = c.bugs[w].target;
    let mut modes = Vec::new();
    for _ in 0..60 * 10 {
        if let Some(t) = target {
            (c.bugs[w].x, c.bugs[w].y) = t;
        }
        c.step(Some(&mut webs));
        modes.push(c.bugs[w].mode);
        target = c.bugs[w].target.or(target);
    }
    assert!(modes.contains(&JobMode::WebWeave));
    assert!(!webs.webs.is_empty());
}

#[test]
fn a_webber_with_no_web_world_stays_idle_rather_than_crashing() {
    let mut c = Colony::new();
    let (sx, sy) = (c.site().x, c.site().y);
    let w = with_webber(&mut c, sx, sy);
    c.step(None);
    assert_eq!(c.bugs[w].mode, JobMode::Idle);
}

fn with_hunter(c: &mut Colony, x: f64, y: f64) -> usize {
    c.bugs.push(Bug::dummy("hunter", "pack_a", x, y, 7));
    c.bugs.len() - 1
}

#[test]
fn a_hunter_patrols_close_to_home_when_not_hunting() {
    let mut c = Colony::new();
    let (sx, sy) = (c.site().x, c.site().y);
    let h = with_hunter(&mut c, sx, sy);
    let mut seen = false;
    for _ in 0..60 * 20 {
        c.step(None);
        seen |= c.bugs[h].mode == JobMode::HuntPatrol;
    }
    assert!(seen);
}

#[test]
fn a_hunting_spider_still_marks_its_job_on_duty() {
    let mut c = Colony::new();
    let (sx, sy) = (c.site().x, c.site().y);
    let h = with_hunter(&mut c, sx, sy);
    c.bugs[h].hunting = true;
    c.step(None);
    assert_eq!(c.bugs[h].mode, JobMode::Hunting);
}

#[test]
fn a_hunter_carries_a_catch_home_and_it_feeds_the_base() {
    let mut c = Colony::new();
    let (sx, sy) = (c.site().x, c.site().y);
    let h = with_hunter(&mut c, sx + 500.0, sy + 300.0);
    c.bugs[h].feeding = true;
    c.step(None);
    c.bugs[h].feeding = false;
    let mut seen = false;
    for _ in 0..60 * 20 {
        c.step(None);
        seen |= c.bugs[h].mode == JobMode::HuntReturn;
        if c.bugs[h].mode == JobMode::HuntReturn
            && let Some((tx, ty)) = c.bugs[h].target
        {
            c.bugs[h].x += (tx - c.bugs[h].x) * 0.5;
            c.bugs[h].y += (ty - c.bugs[h].y) * 0.5;
        }
    }
    assert!(seen);
    assert!(c.site().resources > 0.0, "a delivered catch never credited the base");
}

#[test]
fn a_hunter_with_no_team_base_still_gets_a_home_to_patrol() {
    let mut world = BaseWorld::new(800.0, 600.0, &[]);
    let mut rng = PyRandom::from_u64(0);
    let mut loner = vec![Bug::dummy("hunter", "neutral", 400.0, 300.0, 91)];
    let mut seen = false;
    for _ in 0..60 * 20 {
        world.update::<Bug>(DT, &mut loner, None, &mut rng);
        seen |= loner[0].mode == JobMode::HuntPatrol;
    }
    assert!(seen);
    assert!(loner[0].base_id.is_none());
    assert!(world.bases.is_empty(), "a neutral spider founded a base");
}

// --------------------------------------------------------- test_jobs_economy

fn lone_builder_world() -> (BaseWorld, PyRandom, Vec<Bug>) {
    (BaseWorld::new(800.0, 600.0, &[]), PyRandom::from_u64(4), vec![Bug::dummy("builder", "pack_a", 200.0, 200.0, 1)])
}

#[test]
fn build_progress_stalls_with_an_empty_larder() {
    let (mut world, mut rng, mut bugs) = lone_builder_world();
    for _ in 0..60 * 60 {
        world.update::<Bug>(DT, &mut bugs, None, &mut rng);
    }
    let site = &world.bases[0];
    assert_eq!(site.resources, 0.0);
    assert_eq!(site.build_progress, 0.0);
    assert_eq!(site.completion(), 0.0);
}

#[test]
fn build_progress_resumes_and_spends_resources_once_food_arrives() {
    let (mut world, mut rng, mut bugs) = lone_builder_world();
    for _ in 0..60 * 5 {
        world.update::<Bug>(DT, &mut bugs, None, &mut rng);
    }
    assert_eq!(world.bases[0].build_progress, 0.0);
    world.bases[0].resources = 50.0;
    for _ in 0..60 * 5 {
        world.update::<Bug>(DT, &mut bugs, None, &mut rng);
    }
    let site = &world.bases[0];
    assert!(0.0 < site.build_progress && site.build_progress <= 50.0, "{}", site.build_progress);
    let spent = 50.0 - site.resources;
    assert!((spent - site.build_progress * BUILD_RESOURCE_COST_PER_PROGRESS).abs() <= 0.01);
}

#[test]
fn a_base_cannot_outrun_its_larder_no_matter_how_long_it_waits() {
    let (mut world, mut rng, mut bugs) = lone_builder_world();
    for _ in 0..60 * 600 {
        world.update::<Bug>(DT, &mut bugs, None, &mut rng);
    }
    assert_eq!(world.bases[0].build_progress, 0.0);
    assert_eq!(world.bases[0].level, 0);
}

fn guarded(level: i64, foe_pad_level: f64) -> JobMode {
    let mut world = BaseWorld::new(800.0, 600.0, &[]);
    let mut rng = PyRandom::from_u64(1);
    let guard = Bug::dummy("guard", "pack_a", 200.0, 200.0, 2);
    let site = world.ensure_site(&guard);
    site.level = level;
    let pad = GUARD_ALERT_RADIUS_PAD + foe_pad_level * GUARD_ALERT_RADIUS_PER_LEVEL;
    let (sx, sy, r) = (site.x, site.y, site.radius());
    let foe = Bug::dummy("none", "rivals", sx + r + pad - 1.0, sy, 3);
    assert!(foe.x - sx > r + GUARD_ALERT_RADIUS_PAD);
    let mut bugs = vec![guard, foe];
    world.update(DT, &mut bugs, None, &mut rng);
    bugs[0].mode
}

#[test]
fn a_leveled_base_widens_its_guards_alert_radius() {
    assert_eq!(guarded(3, 3.0), JobMode::GuardAlert);
}

#[test]
fn an_unleveled_bases_guard_does_not_get_the_wider_ring() {
    assert_ne!(guarded(0, 3.0), JobMode::GuardAlert);
}

fn regen(level: i64, dragged: bool) -> f64 {
    let mut world = BaseWorld::new(800.0, 600.0, &[]);
    let mut rng = PyRandom::from_u64(1);
    let guard = Bug::dummy("guard", "pack_a", 200.0, 200.0, 2);
    let site = world.ensure_site(&guard);
    site.level = level;
    let (sx, sy) = (site.x, site.y);
    let mut healable = Bug::dummy("none", "pack_a", sx + 10.0, sy, 5);
    healable.hp = Some(10.0);
    healable.dragging = dragged;
    let mut bugs = vec![guard, healable];
    for _ in 0..60 {
        world.update(DT, &mut bugs, None, &mut rng);
    }
    bugs[1].hp.unwrap()
}

#[test]
fn creatures_near_a_leveled_base_slowly_regenerate() {
    let hp = regen(2, false);
    let expected = 100.0f64.min(10.0 + BASE_REGEN_HP_PER_LEVEL * 2.0);
    assert!((hp - expected).abs() <= 0.05, "{hp} vs {expected}");
    assert!(hp > 10.0);
}

#[test]
fn no_regen_at_base_level_zero() {
    assert_eq!(regen(0, false), 10.0);
}

#[test]
fn regen_does_not_touch_a_dragged_creature() {
    assert_eq!(regen(2, true), 10.0);
}

#[test]
fn credit_team_food_tops_up_the_matching_teams_base() {
    let mut world = BaseWorld::new(800.0, 600.0, &[]);
    let builder = Bug::dummy("builder", "pack_a", 200.0, 200.0, 1);
    world.ensure_site(&builder);
    world.credit_team_food(Some("pack_a"), FLY_CATCH_RESOURCE_AMOUNT);
    let site = &world.bases[0];
    assert!((site.larder - FLY_CATCH_RESOURCE_AMOUNT * GROWTH_FOOD_SHARE).abs() < 1e-12);
    assert!((site.resources - FLY_CATCH_RESOURCE_AMOUNT * (1.0 - GROWTH_FOOD_SHARE)).abs() < 1e-12);
    assert!((site.resources + site.larder - FLY_CATCH_RESOURCE_AMOUNT).abs() < 1e-12);
}

#[test]
fn credit_team_food_is_a_no_op_with_no_base_or_neutral_team() {
    let mut world = BaseWorld::new(800.0, 600.0, &[]);
    world.credit_team_food(Some("pack_a"), 10.0);
    world.credit_team_food(Some("neutral"), 10.0);
    world.credit_team_food(None, 10.0);
    assert!(world.bases.is_empty());
}

// ------------------------------------------------------- test_bases_on_screen

fn site(id: &str, owner: &str, team: &str, x: f64, y: f64, built: f64, level: i64) -> BaseSite {
    let mut s = BaseSite::new(id, owner, team, x, y);
    s.build_progress = built;
    s.level = level;
    s
}

fn screens_world(sites: &[BaseSite]) -> BaseWorld {
    let saved: Vec<Value> = sites.iter().map(BaseSite::to_dict).collect();
    let mut world = BaseWorld::new(6400.0, 2160.0, &saved);
    let rects = [ScreenRect::new(0.0, 718.0, 2560.0, 1440.0), ScreenRect::new(2560.0, 0.0, 3840.0, 2160.0)];
    world.playfield = Some(Playfield::new(6400.0, 2160.0, &rects));
    world
}

fn lone(key: &str, team: &str, x: f64, y: f64) -> Bug {
    let mut b = Bug::new(0, "none", team, x, y);
    b.pid = key.to_string();
    b
}

fn posts_on_screen(world: &BaseWorld, s: &BaseSite) -> bool {
    world.playfield.as_ref().unwrap().contains(s.x, s.y, s.radius() + GUARD_STANDOFF_PAD)
}

#[test]
fn a_base_in_the_dead_zone_is_moved_onto_a_monitor() {
    let mut world = screens_world(&[site("owner:a", "a", "neutral", 95.0, 241.0, 46.0, 0)]);
    world.keep_all_on_screen();
    let moved = world.base("owner:a").unwrap();
    assert!(world.playfield.as_ref().unwrap().contains(moved.x, moved.y, 0.0));
    assert!(posts_on_screen(&world, moved));
}

#[test]
fn a_base_at_a_monitor_edge_is_moved_clear_of_it() {
    let mut world = screens_world(&[site("team:rivals", "r", "rivals", 6358.0, 42.0, 120.0, 5)]);
    world.keep_all_on_screen();
    assert!(posts_on_screen(&world, world.base("team:rivals").unwrap()));
}

#[test]
fn a_base_already_well_on_screen_does_not_move() {
    let mut world = screens_world(&[site("owner:a", "a", "neutral", 4000.0, 1000.0, 50.0, 0)]);
    world.keep_all_on_screen();
    let s = world.base("owner:a").unwrap();
    assert_eq!((s.x, s.y), (4000.0, 1000.0));
}

fn chosen(world: &BaseWorld, spider: &Bug) -> String {
    world.bases[world.site_for_team(spider).unwrap()].id.clone()
}

#[test]
fn a_lone_spider_does_not_adopt_another_lone_spiders_empty_base() {
    let world = screens_world(&[
        site("owner:me", "me", "neutral", 3000.0, 1000.0, 30.0, 0),
        site("owner:them", "them", "neutral", 3010.0, 1000.0, 0.0, 0),
    ]);
    assert_eq!(chosen(&world, &lone("me", "neutral", 3010.0, 1000.0)), "owner:me");
}

#[test]
fn a_visible_base_beats_an_invisible_one() {
    let world = screens_world(&[
        site("owner:me", "me", "neutral", 3000.0, 1000.0, 0.0, 0),
        site("owner:gone", "gone", "neutral", 4500.0, 1500.0, 80.0, 0),
    ]);
    assert_eq!(chosen(&world, &lone("me", "neutral", 3000.0, 1000.0)), "owner:gone");
}

#[test]
fn a_team_prefers_its_built_base() {
    let world = screens_world(&[
        site("team:x", "a", "x", 3000.0, 1000.0, 0.0, 0),
        site("team:x2", "b", "x", 5000.0, 1500.0, 60.0, 0),
    ]);
    assert_eq!(chosen(&world, &lone("c", "x", 3000.0, 1000.0)), "team:x2");
}

#[test]
fn empty_bases_of_spiders_that_are_gone_are_dropped() {
    let mut world = screens_world(&[
        site("owner:gone-empty", "gone-empty", "neutral", 3000.0, 1000.0, 0.0, 0),
        site("owner:gone-built", "gone-built", "neutral", 3000.0, 1000.0, 40.0, 0),
        site("owner:alive", "alive", "neutral", 3000.0, 1000.0, 0.0, 0),
        site("team:x", "someone", "x", 3000.0, 1000.0, 0.0, 0),
    ]);
    world.prune_orphan_sites(&[lone("alive", "neutral", 3000.0, 1000.0)]);
    let mut ids: Vec<&str> = world.bases.iter().map(|b| b.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["owner:alive", "owner:gone-built", "team:x"]);
}

// ---------------------------------------------------------- test_base_mounds

fn pile(id: &str, completion: f64, level: i64) -> BaseSite {
    site(id, "owner", "hunters", 400.0, 300.0, MAX_BUILD_PROGRESS * completion, level)
}

fn r6(x: f64) -> f64 {
    dc_world::pymath::round_n(x, 6)
}

#[test]
fn a_finished_base_is_a_full_pile_and_an_unstarted_one_is_bare() {
    assert!(pile("demo|1", 0.0, 0).mounds().is_empty());
    assert_eq!(pile("demo|1", 1.0, 0).mounds().len(), MOUNDS_PER_BASE);
}

#[test]
fn mounds_are_finished_one_at_a_time() {
    let mounds = pile("demo|1", 0.5, 0).mounds();
    assert_eq!(mounds.len(), MOUNDS_PER_BASE / 2);
    assert!(mounds[..mounds.len() - 1].iter().all(|m| m.built == 1.0));
    let part: Vec<f64> = pile("demo|1", 0.54, 0).mounds().iter().map(|m| m.built).collect();
    assert_eq!(part.iter().filter(|b| **b == 1.0).count(), 6, "{part:?}");
    let last = *part.last().unwrap();
    assert!(0.0 < last && last < 1.0);
}

#[test]
fn progress_only_adds_mounds_and_never_moves_one() {
    let mut s = pile("demo|1", 0.25, 0);
    let early: Vec<(f64, f64)> = s.mounds().iter().map(|m| (r6(m.x), r6(m.y))).collect();
    s.build_progress = MAX_BUILD_PROGRESS;
    let later: Vec<(f64, f64)> = s.mounds().iter().map(|m| (r6(m.x), r6(m.y))).collect();
    assert_eq!(later[..early.len()], early[..]);
    assert!(later.len() > early.len());
}

#[test]
fn nothing_about_a_mound_rotates() {
    let mut s = pile("demo|1", 1.0, 0);
    let before = s.mounds();
    for step in 0..12 {
        s.patrol_angle = step as f64 * 0.5;
        assert_eq!(s.mounds(), before, "a mound moved when the patrol angle turned");
    }
}

#[test]
fn the_same_base_piles_its_earth_in_the_same_place_every_time() {
    let first = pile("team:hunters|base-7", 1.0, 0).mounds();
    assert_eq!(pile("team:hunters|base-7", 1.0, 0).mounds(), first);
    assert_ne!(pile("team:rivals|base-7", 1.0, 0).mounds(), first);
}

#[test]
fn a_level_up_enlarges_the_pile_instead_of_rearranging_it() {
    let (small, large) = (pile("demo|1", 1.0, 0), pile("demo|1", 1.0, 4));
    assert!(large.radius() > small.radius());
    let shape = |s: &BaseSite| -> Vec<(f64, f64)> {
        s.mounds().iter().map(|m| (r6((m.x - s.x) / s.radius()), r6((m.y - s.y) / s.radius()))).collect()
    };
    assert_eq!(shape(&small), shape(&large));
    let sizes = |s: &BaseSite| -> Vec<f64> { s.mounds().iter().map(|m| r6(m.size / s.radius())).collect() };
    assert_eq!(sizes(&small), sizes(&large));
}

#[test]
fn a_finished_pile_stays_inside_the_base_ring() {
    for level in 0..6 {
        let s = pile("demo|1", 1.0, level);
        for m in s.mounds() {
            let reach = ((m.x - s.x).powi(2) + (m.y - s.y).powi(2)).sqrt() + m.size * 1.0f64.max(m.aspect);
            assert!(reach <= s.radius(), "{level}: {reach} > {}", s.radius());
        }
    }
}

// ---------------------------------------------------------- test_base_ground

#[test]
fn a_patch_is_not_a_circle() {
    let outline = patch_recipe("demo|1:patch:10.00:20.00").body;
    assert_eq!(outline.len(), PATCH_POINTS);
    let radii: Vec<f64> = outline.iter().map(|&(x, y)| dc_world::pymath::hypot(x, y / 0.58)).collect();
    let max = radii.iter().cloned().fold(f64::MIN, f64::max);
    let min = radii.iter().cloned().fold(f64::MAX, f64::min);
    let spread = (max - min) / (radii.iter().sum::<f64>() / radii.len() as f64);
    assert!(spread > 0.2, "the outline is near-circular: {spread}");
}

#[test]
fn a_patch_is_closed_and_stays_near_its_centre() {
    let recipe = patch_recipe("demo|1:patch:10.00:20.00");
    for outline in [&recipe.body, &recipe.crown] {
        for &(x, y) in outline.iter() {
            let r = dc_world::pymath::hypot(x, y / 0.58);
            assert!((0.4..=1.6).contains(&r), "{x} {y}");
        }
    }
}

#[test]
fn the_same_base_digs_the_same_earth_in_the_next_process() {
    let key = "colony|7:patch:100.00:200.00";
    let mut cache = PatchCache::default();
    let first = cache.get(key).clone();
    cache.clear();
    assert_eq!(*cache.get(key), first);
    assert_ne!(patch_recipe("colony|7:patch:100.00:201.00"), first);
}

#[test]
fn two_mounds_of_one_base_are_not_the_same_shape() {
    let s = pile("shape|1", 1.0, 0);
    let mut shapes: Vec<String> = s
        .mounds()
        .iter()
        .map(|m| format!("{:?}", patch_recipe(&format!("{}:patch:{:.2}:{:.2}", s.id, m.x, m.y)).body))
        .collect();
    shapes.sort();
    shapes.dedup();
    assert_eq!(shapes.len(), s.mounds().len());
}

#[test]
fn the_team_tint_leaves_the_soil_brown() {
    for colour in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)] {
        let soil = blend((86, 62, 42), colour, 0.08);
        assert!(soil.0 > soil.1 && soil.0 > soil.2, "{colour:?} -> {soil:?}");
    }
}

#[test]
fn the_world_reports_whether_it_moved_anything() {
    let mut world = BaseWorld::new(1400.0, 900.0, &[]);
    assert!(!world.move_base("nobody", 10.0, 10.0));
    world.insert_base(site("here|1", "o", "hunters", 50.0, 50.0, 0.0, 0));
    assert!(world.move_base("here|1", 300.0, 300.0));
}

#[test]
fn a_base_carries_its_earth_and_larder_and_cannot_leave_the_desktop() {
    let mut world = BaseWorld::new(1400.0, 900.0, &[]);
    let mut s = site("team:hunters", "o", "hunters", 400.0, 400.0, MAX_BUILD_PROGRESS * 0.7, 2);
    s.resources = 9.0;
    world.insert_base(s);
    let offsets = |w: &BaseWorld| -> Vec<(f64, f64)> {
        let s = w.base("team:hunters").unwrap();
        s.mounds().iter().map(|m| (r6(m.x - s.x), r6(m.y - s.y))).collect()
    };
    let before = offsets(&world);
    assert!(world.move_base("team:hunters", 900.0, 650.0));
    let s = world.base("team:hunters").unwrap();
    assert_eq!((s.x, s.y), (900.0, 650.0));
    assert_eq!((s.level, s.resources), (2, 9.0));
    assert!((s.build_progress - MAX_BUILD_PROGRESS * 0.7).abs() < 1e-12);
    assert_eq!(offsets(&world), before);
    world.move_base("team:hunters", -500.0, 99999.0);
    let s = world.base("team:hunters").unwrap();
    assert!(0.0 < s.x && s.x < 1400.0 && 0.0 < s.y && s.y < 900.0);
}

// ------------------------------------------------- test_trapping_is_reachable

#[test]
fn jobs_grant_silk() {
    let silk = |job: &str| job_ability_ids(Some(job)).iter().any(|a| *a == "shoot_web" || *a == "wall_web");
    assert!(JOB_IDS.iter().any(|j| silk(j)), "no job grants silk");
    assert!(job_ability_ids(Some("webber")).contains(&"weave_web"));
    assert!(job_ability_ids(Some("webber")).contains(&"shoot_web"));
    assert!(job_ability_ids(Some("guard")).contains(&"shoot_web"));
    assert_eq!(job_definition(Some("weaver")).id, "webber");
    assert_eq!(job_options()[0], ("No job", "none"));
    let _ = CreatureId(0);
}
