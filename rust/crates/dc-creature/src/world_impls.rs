//! The bridges between [`Creature`] and the other crates' traits:
//!
//! * [`dc_world::WorldCreature`] for `Creature` (what webs, flies, bases and
//!   silk read and write on a spider);
//! * this crate's narrow world traits ([`WebWorldApi`], [`FlyWorldApi`],
//!   [`MouseWebApi`], [`BaseLookup`]) for `dc-world`'s `WebWorld`,
//!   `FlyWorld`, `MouseWebWorld` and `BaseWorld`, so a manager can hand its
//!   real worlds to [`crate::world::Env`];
//! * [`dc_adventure::adventure::ControlledSpider`] for `Creature` (the
//!   Adventure player's controller).

use crate::creature::{After, Creature, JumpKind};
use crate::progression::Relation;
use crate::state::State;
use crate::world::{BaseLookup, CreatureView, FlyWorldApi, MouseWebApi, PreyView, ShotKind, Shooter, WebWorldApi};
use crate::world::{CreatureId, FlyId, WebId};
use dc_adventure::adventure::ControlledSpider;
use dc_core::PyRandom;
use dc_world::creature_view as wc;
use dc_world::{BaseWorld, FlyWorld, JobIntent, MouseWebWorld, WebWorld, WorldCreature};

#[inline]
fn wid(id: WebId) -> wc::WebId {
    wc::WebId(id)
}

impl Relation {
    /// The same relation in `dc-world`'s terms.
    pub fn to_world(self) -> wc::Relation {
        match self {
            Relation::Friend => wc::Relation::Friend,
            Relation::Foe => wc::Relation::Foe,
            Relation::Neutral => wc::Relation::Neutral,
        }
    }
}

// ---------------------------------------------------------------------------
// WorldCreature for Creature
// ---------------------------------------------------------------------------

impl WorldCreature for Creature {
    fn id(&self) -> wc::CreatureId {
        wc::CreatureId(self.id)
    }
    fn index(&self) -> i64 {
        self.index
    }
    fn progression_id(&self) -> &str {
        &self.progression_id
    }
    fn team_id(&self) -> &str {
        if self.progression.team_id.is_empty() { "neutral" } else { &self.progression.team_id }
    }
    fn job_id(&self) -> &str {
        &self.job_id
    }
    fn x(&self) -> f64 {
        self.kin.x
    }
    fn y(&self) -> f64 {
        self.kin.y
    }
    fn heading(&self) -> f64 {
        self.kin.heading
    }
    fn size(&self) -> f64 {
        self.kin.size
    }
    fn level(&self) -> f64 {
        Creature::level(self) as f64
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn dragging(&self) -> bool {
        self.dragging
    }
    fn airborne(&self) -> bool {
        self.airborne
    }
    fn job_busy(&self) -> bool {
        self.job_busy
    }
    fn player_controlled(&self) -> bool {
        self.player_control
    }
    fn webbed(&self) -> bool {
        Creature::webbed(self)
    }
    fn is_feeding(&self) -> bool {
        self.state == State::Feed
    }
    fn hunting_prey(&self) -> bool {
        self.hunting_prey
    }
    fn acts_as_hunter(&self) -> bool {
        Creature::acts_as_hunter(self)
    }
    fn prey_fly(&self) -> Option<wc::FlyId> {
        self.prey.map(|p| wc::FlyId(p.id))
    }
    fn desktop_fully_hidden(&self) -> bool {
        self.desktop_fully_hidden
    }
    fn current_speed(&self) -> f64 {
        self.kin.current_speed
    }
    fn aim_intent(&self) -> f64 {
        self.aim_intent
    }
    fn web_homing(&self) -> f64 {
        self.progression_effect("web_homing")
    }
    fn relation_to(&self, other: &Self) -> wc::Relation {
        Creature::relation_to(self, &other.view()).to_world()
    }
    fn set_job_intent(&mut self, intent: &JobIntent<'_>) {
        self.job_mode = intent.mode.as_str().to_string();
        self.job_target = intent.target;
        self.job_alert_target = intent.alert_target.map(|c| c.0);
        self.job_base_id = intent.base_id.map(str::to_string);
        self.job_facing = intent.facing;
    }
    fn set_job_base_id(&mut self, base_id: Option<&str>) {
        self.job_base_id = base_id.map(str::to_string);
    }
    fn heal(&mut self, amount: f64) -> f64 {
        Creature::heal(self, amount)
    }
    fn web_pinned(&mut self, _kind: &str, by: wc::CreatureId) {
        Creature::web_pinned(self, Some(by.0));
    }
    fn rng(&mut self) -> &mut PyRandom {
        &mut self.rng
    }
}

// ---------------------------------------------------------------------------
// The narrow world traits for dc-world's worlds
// ---------------------------------------------------------------------------

impl WebWorldApi for WebWorld {
    fn find_repairable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        WebWorld::find_repairable_web(self, x, y, max_dist).map(|w| w.0)
    }
    fn find_adoptable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        WebWorld::find_adoptable_web(self, x, y, max_dist).map(|w| w.0)
    }
    fn find_walkable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        WebWorld::find_walkable_web(self, x, y, max_dist).map(|w| w.0)
    }
    fn intact_complete_count(&self) -> usize {
        WebWorld::intact_complete_count(self)
    }
    fn claim_site(&mut self, builder: CreatureId, size: f64, prefer_corner: bool) -> Option<WebId> {
        // Python passed no `rng`, so the site draw is not the creature's.
        WebWorld::claim_site(self, wc::CreatureId(builder), size, prefer_corner, None, None).map(|w| w.0)
    }
    fn adopt(&mut self, web: WebId, builder: CreatureId) -> bool {
        WebWorld::adopt(self, wid(web), wc::CreatureId(builder))
    }
    fn abandon(&mut self, web: WebId) {
        WebWorld::abandon(self, Some(wid(web)));
    }
    fn claim_repair(&mut self, web: WebId, repairer: CreatureId) -> bool {
        WebWorld::claim_repair(self, wid(web), wc::CreatureId(repairer))
    }
    fn release_repair(&mut self, web: WebId) {
        WebWorld::release_repair(self, Some(wid(web)));
    }
    fn contains(&self, web: WebId) -> bool {
        WebWorld::contains(self, wid(web))
    }
    fn is_complete(&self, web: WebId) -> bool {
        self.web(wid(web)).is_some_and(|w| w.is_complete())
    }
    fn is_damaged(&self, web: WebId) -> bool {
        self.web(wid(web)).is_some_and(|w| w.is_damaged())
    }
    fn damaged_centroid(&self, web: WebId) -> Option<(f64, f64)> {
        self.web(wid(web)).and_then(|w| w.damaged_centroid())
    }
    fn reach_inset(&self, web: WebId) -> f64 {
        self.web(wid(web)).map_or(38.0, |w| w.reach_inset)
    }
    fn hub(&self, web: WebId) -> (f64, f64) {
        self.web(wid(web)).map_or((0.0, 0.0), |w| w.hub)
    }
    fn working_point(&self, web: WebId, screen_w: f64, screen_h: f64) -> ((f64, f64), bool) {
        self.web(wid(web)).map_or(((0.0, 0.0), false), |w| w.working_point_on(true, screen_w, screen_h))
    }
    fn advance(&mut self, web: WebId, dt: f64, weave_speed: f64, spider: (f64, f64), lead_max: f64) {
        if let Some(w) = self.web_mut(wid(web)) {
            w.advance(dt, weave_speed, spider, lead_max);
        }
    }
    fn repair_near(&mut self, web: WebId, x: f64, y: f64, dt: f64) {
        if let Some(w) = self.web_mut(wid(web)) {
            w.repair_near(x, y, dt, dc_world::webs::REPAIR_RATE, 1e9);
        }
    }
    fn walkable_point(&mut self, web: WebId, screen_w: f64, screen_h: f64) -> (f64, f64) {
        let Some(pos) = self.webs.iter().position(|w| w.id == wid(web)) else { return (0.0, 0.0) };
        let (webs, rng) = (&self.webs, &mut self.rng);
        webs[pos].walkable_point(rng, screen_w, screen_h)
    }
    fn pluck(&mut self, web: WebId, at: (f64, f64), strength: f64) {
        if let Some(w) = self.web_mut(wid(web)) {
            w.pluck(at, strength);
        }
    }
}

/// The two parties of a silk shot, as `dc-world`'s `launch_web_shot*` reads
/// them (`ShooterPose::of`, `web_homing`, the target's id, pose and flags).
struct ShotParty {
    id: u64,
    x: f64,
    y: f64,
    heading: f64,
    size: f64,
    homing: f64,
    dead: bool,
    webbed: bool,
    rng: PyRandom,
}

impl ShotParty {
    fn shooter(s: &Shooter) -> Self {
        ShotParty {
            id: s.id,
            x: s.x,
            y: s.y,
            heading: s.heading,
            size: s.size,
            homing: s.homing,
            dead: false,
            webbed: false,
            rng: PyRandom::from_u64(0),
        }
    }

    fn target(t: &CreatureView) -> Self {
        ShotParty {
            id: t.id,
            x: t.x,
            y: t.y,
            heading: 0.0,
            size: t.size,
            homing: 0.0,
            dead: t.dead,
            webbed: t.webbed,
            rng: PyRandom::from_u64(0),
        }
    }
}

impl WorldCreature for ShotParty {
    fn id(&self) -> wc::CreatureId {
        wc::CreatureId(self.id)
    }
    fn index(&self) -> i64 {
        0
    }
    fn progression_id(&self) -> &str {
        ""
    }
    fn team_id(&self) -> &str {
        "neutral"
    }
    fn job_id(&self) -> &str {
        "none"
    }
    fn x(&self) -> f64 {
        self.x
    }
    fn y(&self) -> f64 {
        self.y
    }
    fn heading(&self) -> f64 {
        self.heading
    }
    fn size(&self) -> f64 {
        self.size
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn webbed(&self) -> bool {
        self.webbed
    }
    fn web_homing(&self) -> f64 {
        self.homing
    }
    fn relation_to(&self, _other: &Self) -> wc::Relation {
        wc::Relation::Neutral
    }
    fn set_job_intent(&mut self, _intent: &JobIntent<'_>) {}
    fn set_job_base_id(&mut self, _base_id: Option<&str>) {}
    fn rng(&mut self) -> &mut PyRandom {
        &mut self.rng
    }
}

impl FlyWorldApi for FlyWorld {
    fn fly(&self, id: FlyId) -> Option<PreyView> {
        FlyWorld::fly(self, wc::FlyId(id)).map(|f| PreyView {
            id,
            x: f.x,
            y: f.y,
            size: f.size,
            alive: f.alive(),
            eaten: f.eaten(),
            trapped: f.trapped(),
            dragging: f.dragging,
            is_moving: f.is_moving(),
        })
    }
    fn launch_web_shot(&mut self, shooter: &Shooter, fly: FlyId, kind: ShotKind) -> bool {
        FlyWorld::launch_web_shot(self, &ShotParty::shooter(shooter), wc::FlyId(fly), kind.as_str())
    }
    fn launch_web_shot_at_creature(&mut self, shooter: &Shooter, target: &CreatureView, kind: ShotKind) -> bool {
        FlyWorld::launch_web_shot_at_creature(
            self,
            &ShotParty::shooter(shooter),
            &ShotParty::target(target),
            kind.as_str(),
        )
    }
}

impl MouseWebApi for MouseWebWorld {
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn busy(&self) -> bool {
        MouseWebWorld::busy(self)
    }
    fn shoot(&mut self, origin: (f64, f64), target: (f64, f64), kind: ShotKind, homing: f64, lead: (f64, f64)) -> bool {
        MouseWebWorld::shoot(self, origin, target, kind.as_str(), homing, lead)
    }
}

impl BaseLookup for BaseWorld {
    fn base_point(&self, key: &str) -> Option<(f64, f64)> {
        self.base(key).map(|b| (b.x, b.y))
    }
}

// ---------------------------------------------------------------------------
// ControlledSpider for Creature
// ---------------------------------------------------------------------------

impl ControlledSpider for Creature {
    fn x(&self) -> f64 {
        self.kin.x
    }
    fn y(&self) -> f64 {
        self.kin.y
    }
    fn heading(&self) -> f64 {
        self.kin.heading
    }
    fn size(&self) -> f64 {
        self.kin.size
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn airborne(&self) -> bool {
        self.airborne
    }
    fn webbed(&self) -> bool {
        Creature::webbed(self)
    }
    fn webbed_held(&self) -> bool {
        Creature::webbed_held(self)
    }
    fn energy(&self) -> f64 {
        self.energy
    }
    fn set_energy(&mut self, energy: f64) {
        self.energy = energy;
    }
    fn max_energy(&self) -> f64 {
        self.max_energy
    }
    fn spend_energy(&mut self, amount: f64) -> bool {
        Creature::spend_energy(self, amount)
    }
    fn attack_cooldown(&self) -> f64 {
        self.attack_cooldown
    }
    fn set_attack_cooldown(&mut self, seconds: f64) {
        self.attack_cooldown = seconds;
    }
    fn target(&self) -> (f64, f64) {
        (self.kin.target_x, self.kin.target_y)
    }
    fn set_target(&mut self, x: f64, y: f64) {
        self.kin.target_x = x;
        self.kin.target_y = y;
    }
    fn set_target_heading(&mut self, heading: f64) {
        self.kin.target_heading = heading;
    }
    fn speed(&self) -> f64 {
        self.kin.speed
    }
    fn set_speed(&mut self, speed: f64) {
        self.kin.speed = speed;
    }
    fn speed_mult(&self) -> f64 {
        Creature::speed_mult(self)
    }
    fn set_motion_paused(&mut self, paused: bool) {
        self.kin.motion_paused = paused;
    }
    fn reverse_walk(&self) -> bool {
        self.reverse_walk
    }
    fn set_reverse_walk(&mut self, reverse: bool) {
        self.reverse_walk = reverse;
    }
    fn set_player_state(&mut self) {
        self.state = State::Player;
    }
    fn has_skill(&self, skill: &str) -> bool {
        Creature::has_skill(self, skill)
    }
    fn launch_jump(&mut self, x: f64, y: f64, kind: &str, after: &str) {
        Creature::launch_jump(self, x, y, JumpKind::parse(kind), After::parse(after), None, None, 1.0);
    }
    fn begin_strike(&mut self, x: f64, y: f64) {
        Creature::begin_strike(self, x, y);
    }
    fn take_player_control(&mut self) {
        self.player_control = true;
        self.prey = None;
        self.hunting_prey = false;
        self.foe = None;
        self.job_mode = "idle".to_string();
        self.enter_idle();
    }
    fn release_player_control(&mut self) {
        self.reverse_walk = false;
        if self.player_control {
            self.player_control = false;
            self.enter_idle();
        }
    }
    fn advance_pose(&mut self, dt: f64) {
        Creature::advance_pose(self, dt);
    }
}
