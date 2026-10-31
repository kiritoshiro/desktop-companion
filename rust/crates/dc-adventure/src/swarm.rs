//! Port of app/swarm.py: fly swarm levels, catches, rivals and escape clocks.
use crate::{adventure::ControlledSpider, controls::ControlSettings, mission::*};
use dc_core::PyRandom;
use dc_world::{Fly, FlyId, ScreenLayout, flies::FlyState};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, f64::consts::PI, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SwarmRules {
    pub target: i64,
    pub seconds: f64,
    pub rivals: usize,
    pub max_flies: usize,
    pub spawn_every: f64,
    pub escape_after: f64,
}
pub const RULES: [(&str, SwarmRules); 2] = [
    (
        "swarm",
        SwarmRules { target: 18, seconds: 150.0, rivals: 1, max_flies: 10, spawn_every: 3.2, escape_after: 26.0 },
    ),
    (
        "storm",
        SwarmRules { target: 30, seconds: 170.0, rivals: 4, max_flies: 16, spawn_every: 2.4, escape_after: 20.0 },
    ),
];
pub fn rules(id: &str) -> SwarmRules {
    RULES.iter().find(|(key, _)| *key == id).unwrap_or(&RULES[0]).1
}

pub struct FlySwarmMission {
    pub mission: TerritoryMission,
    pub caught: i64,
    pub rival_caught: i64,
    pub escaped: i64,
    pub fly_age: BTreeMap<FlyId, f64>,
    pub nest_clock: f64,
    pub rules: SwarmRules,
    pub time_left: f64,
    next_fly_id: u64,
}
impl FlySwarmMission {
    pub const EAT_REACH_PINNED: f64 = 1.7;
    pub const EAT_REACH_FLYING: f64 = 0.8;
    pub const FLY_HEAL: f64 = 6.0;
    pub const FLY_SILK: f64 = 1.0;
    pub const FLY_XP: i64 = 5;
    pub const RIVAL_AGGRO: f64 = 380.0;
    pub const DEFENDER_LEASH: f64 = 260.0;
    pub fn new(
        host: &mut impl MissionHost,
        controls: ControlSettings,
        layout: ScreenLayout,
        state_dir: &Path,
        rng: &mut PyRandom,
    ) -> Result<Self, String> {
        let mission = TerritoryMission::new(host, controls, layout, state_dir, rng)?;
        let rules = rules(&mission.map_info.id);
        let world = host.fly_world_mut();
        world.playfield = Some(mission.layout.playfield.clone());
        world.enabled = false;
        world.use_spawner = false;
        let mut m = Self {
            mission,
            caught: 0,
            rival_caught: 0,
            escaped: 0,
            fly_age: BTreeMap::new(),
            nest_clock: 0.0,
            rules,
            time_left: rules.seconds,
            next_fly_id: 1,
        };
        for _ in 0..4 {
            m.release_fly(host, rng);
        }
        Ok(m)
    }
    pub fn nests(&self) -> impl Iterator<Item = &MissionSite> {
        self.mission.sites.iter().filter(|s| s.kind == "flynest")
    }
    pub fn objective(&self, host: &impl MissionHost) -> String {
        if self.mission.state == "victory" {
            return "VICTORY - the swarm is yours".into();
        }
        if self.mission.state == "defeat" {
            return if host.creature(self.mission.hero).dead() {
                "RAID ENDED - your spider has fallen"
            } else {
                "DEFEAT - the swarm got away"
            }
            .into();
        }
        let left = (self.time_left as i64).max(0);
        format!(
            "Catch flies {}/{}  ·  {}:{:02} left  ·  rivals {}/{}",
            self.caught,
            self.rules.target,
            left / 60,
            left % 60,
            self.rival_caught,
            self.rules.target
        )
    }
    pub fn update(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        self.mission.update_with(host, dt, rng, &mut SwarmEnvironment);
        if self.mission.ended() {
            return;
        }
        let dt = dt.clamp(0.0, 0.05);
        self.time_left = (self.time_left - dt).max(0.0);
        self.flies(host, dt, rng);
        if self.caught >= self.rules.target {
            self.end(host, true, "The swarm is caught");
        } else if self.rival_caught >= self.rules.target || self.time_left <= 0.0 {
            self.end(host, false, "The swarm got away");
        }
    }
    pub fn end(&mut self, host: &mut impl MissionHost, won: bool, text: &str) {
        self.mission.state = if won { "victory" } else { "defeat" }.into();
        if won {
            for p in &self.mission.players {
                host.creature_mut(p.creature).gain_experience(80, "swarm caught");
            }
        }
        self.mission.announce(text);
        self.mission.clear_player_keys();
        self.mission.finish(host, won);
    }
    pub fn release_fly(&mut self, host: &mut impl MissionHost, rng: &mut PyRandom) -> Option<FlyId> {
        let nests = self.nests().collect::<Vec<_>>();
        if nests.is_empty() || host.fly_world().flies.iter().filter(|f| !f.eaten()).count() >= self.rules.max_flies {
            return None;
        }
        let nest = rng.choice(&nests);
        let angle = rng.uniform(-PI, PI);
        let (sw, sh) = host.screen_size();
        let id = FlyId(self.next_fly_id);
        let fly = Fly::new(id, nest.x + angle.cos() * 30.0, nest.y + angle.sin() * 20.0, sw, sh, Some(angle), 1.0, rng);
        self.next_fly_id += 1;
        host.fly_world_mut().flies.push(fly);
        self.fly_age.insert(id, 0.0);
        Some(id)
    }
    pub fn flies(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        self.nest_clock -= dt * self.nests().count().max(1) as f64;
        if self.nest_clock <= 0.0 {
            self.nest_clock = self.rules.spawn_every;
            self.release_fly(host, rng);
        }
        let spiders = host.creatures().into_iter().filter(|&c| !host.creature(c).dead()).collect::<Vec<_>>();
        let flies = host.fly_world().flies.iter().map(|f| f.id).collect::<Vec<_>>();
        for id in flies {
            host.update_fly(id, dt, rng, &self.mission.layout);
            let Some(fly) = host.fly_world().fly(id) else {
                continue;
            };
            if fly.eaten() || fly.removable() {
                continue;
            }
            let age = self.fly_age.entry(id).or_default();
            if !fly.trapped() {
                *age += dt;
            }
            if *age >= self.rules.escape_after {
                host.fly_world_mut().fly_mut(id).unwrap().state = FlyState::Eaten;
                self.escaped += 1;
                continue;
            }
            if let Some(eater) = self.eater(host, id, &spiders) {
                self.eat(host, id, eater);
            }
        }
        host.fly_world_mut().flies.retain(|f| !f.eaten() && !f.removable());
    }
    pub fn eater(&self, host: &impl MissionHost, id: FlyId, spiders: &[SpiderId]) -> Option<SpiderId> {
        let fly = host.fly_world().fly(id)?;
        spiders.iter().copied().find(|&id| {
            let c = host.creature(id);
            if c.airborne() {
                return false;
            }
            let reach =
                c.size() * if fly.trapped() { Self::EAT_REACH_PINNED } else { Self::EAT_REACH_FLYING } + fly.size;
            distance(point(c), (fly.x, fly.y)) <= reach
        })
    }
    pub fn eat(&mut self, host: &mut impl MissionHost, fly: FlyId, spider: SpiderId) {
        host.fly_world_mut().fly_mut(fly).unwrap().begin_eaten();
        let c = host.creature_mut(spider);
        c.heal(Self::FLY_HEAL);
        if TerritoryMission::ours(c) {
            self.caught += 1;
            if let Some(control) = self.mission.control_mut(spider) {
                control.silk = (control.silk + Self::FLY_SILK).min(control.silk_capacity as f64);
            }
            c.gain_experience(Self::FLY_XP, "fly caught");
            if [1, 5, 10, self.rules.target - 3].contains(&self.caught) {
                self.mission.announce(format!("{} of {} flies caught", self.caught, self.rules.target));
            }
        } else {
            self.rival_caught += 1;
            if self.rival_caught == self.rules.target - 3 {
                self.mission.announce("The rivals are 3 flies from taking the swarm!");
            }
        }
    }
    pub fn restarted(&self, host: &mut impl MissionHost, rng: &mut PyRandom) -> Result<Self, String> {
        Self::new(
            host,
            self.mission.controls.clone(),
            self.mission.layout.clone(),
            self.mission.progress_path.parent().unwrap(),
            rng,
        )
    }
}
pub struct SwarmEnvironment;
impl<H: MissionHost> MissionEnvironment<H> for SwarmEnvironment {
    fn spawning(&mut self, _m: &mut TerritoryMission, _host: &mut H, _dt: f64, _rng: &mut PyRandom) {}
    fn actor_goal(&self, _m: &TerritoryMission, host: &H, actor: &MissionActor) -> Option<Point> {
        let c = host.creature(actor.creature);
        let fly = host
            .fly_world()
            .flies
            .iter()
            .filter(|f| f.alive() || f.trapped())
            .filter(|f| {
                if actor.role == "ally" {
                    f.trapped()
                } else {
                    actor.aggro_range.is_some()
                        || distance((f.x, f.y), actor.defend_point) < FlySwarmMission::DEFENDER_LEASH
                }
            })
            .min_by(|a, b| distance((a.x, a.y), point(c)).total_cmp(&distance((b.x, b.y), point(c))))?;
        (distance((fly.x, fly.y), point(c)) <= 520.0).then_some((fly.x, fly.y))
    }
}
