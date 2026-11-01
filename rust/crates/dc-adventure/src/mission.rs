//! Port of app/mission.py: isolated territory raids and the manager boundary.
pub use crate::mission_actor::MissionActor;
use crate::{
    adventure::{AimTarget, Bite, ControlledSpider, PlayerController, ShotAim, SilkShot},
    adventure_profile::*,
    armoury,
    campaign::{self, MapInfo, MapKind},
    controls::{Action, ControlSettings, second_player_controls},
    custom_maps::{self, CustomMap, SpiderSpec},
    encounters::{self, EncounterDirector, OutpostSite},
    map_layouts::{self, BuildingKind, Guard},
};
use dc_content::{
    enemy_kinds::{self, EnemyKind},
    enemy_traits::traits_for,
    palettes::enemy_palette,
    util::ColorMap,
};
use dc_core::PyRandom;
use dc_state::progression::{ProgressionState, armor_by_id};
use dc_world::{CreatureId, FlyId, FlyWorld, ScreenLayout, ScreenRect, tactics};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    f64::consts::{PI, TAU},
    path::{Path, PathBuf},
};

pub type SpiderId = CreatureId;
pub type Point = (f64, f64);
pub fn distance(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}
pub fn point(c: &impl ControlledSpider) -> Point {
    (c.x(), c.y())
}
pub fn aim_target(c: &impl ControlledSpider) -> AimTarget {
    AimTarget::new(c.x(), c.y(), c.size())
}

/// Additional creature operations used by the mission, beyond player controls.
/// The adapter must retain dead spiders by ID until the mission is discarded.
pub trait MissionSpider: ControlledSpider {
    fn index(&self) -> usize;
    fn hp(&self) -> f64;
    fn max_hp(&self) -> f64;
    fn set_hp(&mut self, hp: f64);
    fn set_max_hp(&mut self, hp: f64);
    fn damage(&self) -> f64;
    fn set_damage(&mut self, damage: f64);
    fn size_scale(&self) -> f64;
    fn set_size_scale(&mut self, scale: f64);
    fn margin(&self) -> f64;
    fn set_position(&mut self, position: Point);
    fn progression(&self) -> &ProgressionState;
    fn progression_mut(&mut self) -> &mut ProgressionState;
    fn apply_progression_stats(&mut self, reset_resources: bool);
    fn set_name(&mut self, name: &str);
    fn display_name(&self) -> String;
    fn heal(&mut self, amount: f64);
    fn gain_experience(&mut self, amount: i64, reason: &str);
}

/// All creation choices have already been made here, including random draws.
/// `None` model means the source hero's model/personality cached by `begin_mission`.
#[derive(Debug, Clone)]
pub struct SpawnRequest {
    pub serial: usize,
    pub pos: Point,
    pub progression: ProgressionState,
    pub progression_id: String,
    pub model_id: Option<String>,
    pub colors: Option<ColorMap>,
    pub skills: [&'static str; 4],
    pub enemy_kind: Option<String>,
    pub size_scale: f64,
    pub name: String,
    pub label_style: &'static str,
    pub chooses_own_skills: bool,
    pub heading: f64,
    pub stat_scale: Option<(f64, f64, f64)>,
    pub layout: ScreenLayout,
}

/// Adapter implemented by dc-app over CreatureManager; no dc-manager dependency.
pub trait MissionHost {
    type Spider: MissionSpider;
    /// Cache previous hero (or first live preset spider) model/personality, then
    /// clear creatures/render order/cages/webs/flies/spawners/remains/bases/carcasses.
    /// Set mission mode, conflict=true, interferable/naming/mouse-capture=false,
    /// and adventurers -> rivals = foe. Error if no source spider exists.
    fn begin_mission(&mut self) -> Result<(), String>;
    fn screen_size(&self) -> Point;
    fn models_contains(&self, model_id: &str) -> bool;
    fn creatures(&self) -> Vec<SpiderId>;
    fn creature(&self, id: SpiderId) -> &Self::Spider;
    fn creature_mut(&mut self, id: SpiderId) -> &mut Self::Spider;
    fn create_creature(&mut self, request: SpawnRequest) -> SpiderId;
    fn refresh_neighbor_links(&mut self);
    fn refresh_render_order(&mut self);
    /// Creature.update through its player_control branch: timers, web struggle,
    /// damage numbers, energy regeneration; return false if dead. The mission
    /// invokes the controller immediately afterwards, preserving creature order.
    /// `struggling` is always true for actors, PlayerController.struggling for players.
    fn update_creature(&mut self, id: SpiderId, dt: f64, struggling: bool) -> bool;
    fn bury_the_dead(&mut self);
    /// Keep only the last four carcasses, then update/filter them.
    fn update_carcasses(&mut self, dt: f64);
    fn trade_blow(&mut self, attacker: SpiderId, target: SpiderId);
    fn splash(&mut self, attacker: SpiderId, target: SpiderId, damage: f64);
    fn shoot(&mut self, shooter: SpiderId, shot: SilkShot);
    fn update_projectiles(&mut self, dt: f64);
    fn fly_world(&self) -> &FlyWorld;
    fn fly_world_mut(&mut self) -> &mut FlyWorld;
    /// Update this fly against live creatures and web_world, using the mission
    /// RNG shared by swarm flies (not FlyWorld's autonomous spawn RNG).
    fn update_fly(&mut self, id: FlyId, dt: f64, rng: &mut PyRandom, layout: &ScreenLayout);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Loot {
    pub item_id: String,
    pub x: f64,
    pub y: f64,
    pub age: f64,
}
pub const LOOT_REACH: f64 = 42.0;
#[derive(Debug, Clone, PartialEq)]
pub struct MissionSite {
    pub kind: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub owned: bool,
    pub progress: f64,
    pub contested: bool,
    pub reserves: i64,
    pub warning: f64,
    pub supply: f64,
    pub screen: usize,
    pub guards: Vec<Guard>,
}
impl MissionSite {
    pub fn new(kind: &str, name: &str, pos: Point) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
            x: pos.0,
            y: pos.1,
            owned: false,
            progress: 0.0,
            contested: false,
            reserves: 0,
            warning: 0.0,
            supply: 180.0,
            screen: 0,
            guards: vec![],
        }
    }
    pub fn point(&self) -> Point {
        (self.x, self.y)
    }
}
impl OutpostSite for MissionSite {
    fn owned(&self) -> bool {
        self.owned
    }
    fn reserves(&self) -> i64 {
        self.reserves
    }
    fn set_warning(&mut self, v: f64) {
        self.warning = v;
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AcidGlob {
    pub shooter: SpiderId,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub flight: f64,
    pub age: f64,
    pub done: bool,
}
impl AcidGlob {
    pub const SPEED: f64 = 430.0;
    pub const SPLASH: f64 = 38.0;
    pub fn t(&self) -> f64 {
        (self.age / self.flight.max(0.05)).min(1.0)
    }
    pub fn position(&self) -> (f64, f64, f64) {
        let t = self.t();
        (
            self.x0 + (self.x1 - self.x0) * t,
            self.y0 + (self.y1 - self.y0) * t,
            (t * PI).sin() * (self.flight * 120.0).min(90.0),
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Splash {
    pub x: f64,
    pub y: f64,
    pub age: f64,
}
#[derive(Debug, Clone)]
pub struct MissionPlayer {
    pub creature: SpiderId,
    pub control: PlayerController,
}
#[derive(Debug, Clone, Default)]
pub struct SpiderMissionState {
    pub enemy_kind: Option<String>,
    pub mission_loot: Vec<String>,
    pub loot_rolled: bool,
    pub spiderling: bool,
    pub tunnel_cooldown: f64,
    pub heavy_walked: f64,
    pub venom_base: f64,
    pub venom_applied: Option<f64>,
}

pub struct TerritoryMission {
    pub controls: ControlSettings,
    pub elapsed: f64,
    pub state: String,
    pub command: String,
    pub attack_target: Option<SpiderId>,
    pub hover_target: Option<SpiderId>,
    pub notice: String,
    pub notice_time: f64,
    pub wave_clock: f64,
    pub pending: Vec<(String, String, bool)>,
    pub counter_started: bool,
    pub guardian: Option<SpiderId>,
    pub guardian_warning: Option<f64>,
    pub actors: Vec<MissionActor>,
    pub saved: bool,
    pub save_error: String,
    pub end_clock: f64,
    pub serial: usize,
    pub progress_path: PathBuf,
    pub profile: AdventureProfile,
    pub hero_name: String,
    pub map_info: MapInfo,
    pub custom: Option<CustomMap>,
    pub cap: usize,
    pub wave_every: f64,
    pub tactics_level: i64,
    pub squad_clock: f64,
    pub hazards: Vec<AcidGlob>,
    pub splashes: Vec<Splash>,
    pub loot: Vec<Loot>,
    pub found: Vec<(String, armoury::Loot)>,
    pub reward_text: String,
    pub start_progress: ProgressionState,
    pub layout: ScreenLayout,
    pub area: ScreenRect,
    pub sites: Vec<MissionSite>,
    pub hero: SpiderId,
    pub players: Vec<MissionPlayer>,
    pub second_hero: Option<SpiderId>,
    pub allies: Vec<SpiderId>,
    pub ally: Option<SpiderId>,
    pub defend_point: Point,
    pub director: EncounterDirector,
    pub outposts: Vec<usize>,
    pub spider_state: BTreeMap<SpiderId, SpiderMissionState>,
    pub bonus_clock: f64,
    pub amber_clock: f64,
    pub amber_mined: i64,
    pub nursery_clocks: BTreeMap<(usize, bool), f64>,
}

impl TerritoryMission {
    pub const CAP: usize = 12;
    pub const CAPTURE_SECONDS: f64 = 4.0;
    pub const END_SCREEN_SECONDS: f64 = 4.5;
    pub const WAVE_EVERY: f64 = 24.0;
    pub const EARLY_CAP: usize = 8;
    pub const EARLY_WAVE_EVERY: f64 = 36.0;
    pub const NOT_FOOTHOLDS: [&str; 6] = ["home", "hatchery", "nest", "outpost", "infestation", "flynest"];
    pub const GUARD_SPOTS: [Point; 8] = [
        (88.0, -6.0),
        (-88.0, -6.0),
        (72.0, -48.0),
        (-72.0, -48.0),
        (104.0, 24.0),
        (-104.0, 24.0),
        (0.0, -78.0),
        (40.0, -80.0),
    ];
    pub const BASE_REACH: f64 = 92.0;
    pub const SCOUT_HEAL_REACH: f64 = 110.0;
    pub const VENOM_BONUS: f64 = 1.2;
    pub const LOOKOUT_BONUS: f64 = 1.3;
    pub const AMBER_EVERY: f64 = 6.0;
    pub const NURSERY_EVERY: f64 = 40.0;
    pub const NURSERY_BROOD: usize = 2;
    pub const TUNNEL_COOLDOWN: f64 = 1.2;
    pub const HEAVY_LANDING: f64 = 1.12;
    pub const HEAVY_WALK: f64 = 1.5;
    pub const STEP_EVERY: f64 = 110.0;
    pub const SQUAD_EVERY: f64 = 0.4;
    pub const SQUAD_REACH: f64 = 300.0;

    pub fn new(
        host: &mut impl MissionHost,
        controls: ControlSettings,
        layout: ScreenLayout,
        state_dir: &Path,
        rng: &mut PyRandom,
    ) -> Result<Self, String> {
        let progress_path = profile_path(state_dir);
        let profile = load_profile(&progress_path);
        let map_info = campaign::chosen_map(&profile, state_dir);
        let custom = custom_maps::load_map(&map_info.id, state_dir);
        let mut cap = custom.as_ref().map_or(Self::CAP, |c| c.cap as usize);
        let mut wave_every = custom.as_ref().map_or(Self::WAVE_EVERY, |c| c.wave_every);
        let chosen = custom.as_ref().map_or(-1, |c| c.tactics);
        let tactics_level = if chosen >= 0 { chosen } else { tactics::tactics_level(map_info.tier as i32) as i64 };
        if custom.is_none() {
            if map_info.tier <= 1 {
                cap = Self::EARLY_CAP;
                wave_every = Self::EARLY_WAVE_EVERY;
            } else if map_info.tier >= 3 {
                cap += 2 * (map_info.tier - 2) as usize;
            }
        }
        let mut start_progress = hero_progression(&profile);
        start_progress.relation_overrides = Default::default();
        host.begin_mission()?;
        let encounter = match map_info.kind {
            MapKind::Reclaim => "reclaim",
            MapKind::Swarm => "swarm",
            _ => "raid",
        };
        let director =
            EncounterDirector::new(encounters::profile(encounter).unwrap().clone(), layout.clone(), map_info.tier);
        let mut m = Self {
            controls,
            elapsed: 0.0,
            state: "active".into(),
            command: "follow".into(),
            attack_target: None,
            hover_target: None,
            notice: String::new(),
            notice_time: 9.0,
            wave_clock: 28.0,
            pending: vec![],
            counter_started: false,
            guardian: None,
            guardian_warning: None,
            actors: vec![],
            saved: false,
            save_error: String::new(),
            end_clock: 0.0,
            serial: 0,
            hero_name: profile.name.clone(),
            progress_path,
            profile,
            map_info,
            custom,
            cap,
            wave_every,
            tactics_level,
            squad_clock: 0.0,
            hazards: vec![],
            splashes: vec![],
            loot: vec![],
            found: vec![],
            reward_text: String::new(),
            start_progress,
            area: layout.primary().rect,
            layout,
            sites: vec![],
            hero: CreatureId(0),
            players: vec![],
            second_hero: None,
            allies: vec![],
            ally: None,
            defend_point: (0.0, 0.0),
            director,
            outposts: vec![],
            spider_state: BTreeMap::new(),
            bonus_clock: 0.0,
            amber_clock: 0.0,
            amber_mined: 0,
            nursery_clocks: BTreeMap::new(),
        };
        m.sites = m.build_sites();
        m.hero = m.spawn(host, "hero", m.sites[0].point(), false, None, None, rng).ok_or("Could not spawn hero")?;
        let player = PlayerController::new(host.creature_mut(m.hero), Some(m.controls.clone()));
        m.players.push(MissionPlayer { creature: m.hero, control: player });
        if m.profile.two_player {
            m.players[0].control.controls = ControlSettings { movement: m.controls.movement, ..Default::default() };
            let pos = point(host.creature(m.hero));
            let id = m
                .spawn(host, "hero", m.layout.clamp(pos.0, pos.1 + 65.0, 40.0), false, None, None, rng)
                .ok_or("Could not spawn player two")?;
            m.second_hero = Some(id);
            let c = host.creature_mut(id);
            let mut p = spider_progression(&m.profile, PLAYER_TWO);
            p.team_id = "adventurers".into();
            p.relation_overrides = Default::default();
            *c.progression_mut() = p;
            c.apply_progression_stats(true);
            c.set_name(&m.profile.player_two.name);
            m.players.push(MissionPlayer {
                creature: id,
                control: PlayerController::new(c, Some(second_player_controls(Some(m.controls.movement)))),
            });
            for p in &mut m.players {
                p.control.keyboard_aim = true;
            }
            m.cap += 1;
        }
        let companions: Vec<_> =
            m.profile.companions.iter().take(campaign::PARTY_SIZE).map(|(id, _)| id.clone()).collect();
        for (slot, id) in companions.iter().enumerate() {
            let p = point(host.creature(m.hero));
            if let Some(c) =
                m.spawn(host, "ally", (p.0 + 45.0, p.1 + 65.0 + slot as f64 * 40.0), false, Some(id), None, rng)
            {
                m.allies.push(c);
            }
        }
        m.ally = m.allies.first().copied();
        m.defend_point = point(host.creature(m.ally.unwrap_or(m.hero)));
        m.notice = m.opening_notice();
        m.opening_spawns(host, rng);
        let points = m.sites.iter().map(MissionSite::point).collect::<Vec<_>>();
        for plan in m.director.outposts(&points, 240.0) {
            let mut site = MissionSite::new(&plan.kind, &plan.name, (plan.x, plan.y));
            site.reserves = plan.reserves;
            site.screen = plan.screen;
            m.outposts.push(m.sites.len());
            m.sites.push(site);
            for (i, role) in plan.defenders.iter().enumerate() {
                let angle = 0.6 + i as f64 * (TAU / plan.defenders.len().max(3) as f64);
                let pos = m.layout.clamp(plan.x + angle.cos() * 135.0, plan.y + 10.0 + angle.sin() * 80.0, 40.0);
                m.spawn(host, role, pos, false, None, None, rng);
            }
        }
        if !m.outposts.is_empty() && encounter == "raid" {
            m.notice += &format!(" Outposts wait on your other screen{}.", if m.outposts.len() > 1 { "s" } else { "" });
        }
        host.refresh_neighbor_links();
        host.refresh_render_order();
        Ok(m)
    }

    pub fn build_sites(&self) -> Vec<MissionSite> {
        let main = *self.layout.primary();
        if self.map_info.kind != MapKind::Raid {
            let reclaim = self.map_info.kind == MapKind::Reclaim;
            let mut home = MissionSite::new(
                "home",
                if reclaim { "Last foothold" } else { "Home burrow" },
                main.anchor(if reclaim { 0.13 } else { 0.14 }, 0.62, 110.0),
            );
            home.owned = true;
            home.screen = main.index;
            let mut sites = vec![home];
            if !reclaim {
                let spots = [(0.62, 0.35), (0.40, 0.72), (0.78, 0.70)];
                for s in &self.layout.screens {
                    let (fx, fy) = spots[s.index % 3];
                    let mut nest = MissionSite::new("flynest", "Fly nest", s.anchor(fx, fy, 120.0));
                    nest.owned = true;
                    nest.screen = s.index;
                    sites.push(nest);
                }
            }
            return sites;
        }
        let far = self
            .layout
            .others()
            .into_iter()
            .max_by(|a, b| distance(a.centre(), main.centre()).total_cmp(&distance(b.centre(), main.centre())));
        let places = self
            .custom
            .as_ref()
            .map(custom_maps::placements)
            .unwrap_or_else(|| map_layouts::layout_for(&self.map_info.id).to_vec());
        places
            .iter()
            .filter_map(|p| {
                let (screen, (fx, fy)) = if p.far {
                    if let Some(s) = far { (s, (p.fx, p.fy)) } else { (main, p.alt?) }
                } else {
                    (main, (p.fx, p.fy))
                };
                let r = screen.rect;
                let mut s = MissionSite::new(
                    p.kind.id(),
                    &p.name,
                    r.clamp(r.x + r.w * fx, r.y + 115.0 + (r.h - 360.0).max(160.0) * fy, 85.0),
                );
                s.owned = p.owned;
                s.reserves = p.reserves;
                s.screen = screen.index;
                s.guards = p.guards.clone();
                s.supply = p.supply;
                Some(s)
            })
            .collect()
    }
    pub fn site(&self, kind: &str) -> Option<usize> {
        self.sites.iter().position(|s| s.kind == kind)
    }
    pub fn footholds(&self) -> impl Iterator<Item = &MissionSite> {
        self.sites.iter().filter(|s| !Self::NOT_FOOTHOLDS.contains(&s.kind.as_str()))
    }
    pub fn classic_footholds(&self) -> bool {
        let kinds = self.footholds().map(|s| s.kind.as_str()).collect::<Vec<_>>();
        kinds.iter().all(|s| ["food", "silk", "amber"].contains(s))
            && kinds.contains(&"food")
            && kinds.contains(&"silk")
    }
    fn opening_notice(&self) -> String {
        match self.map_info.kind {
            MapKind::Reclaim => format!(
                "Reclaim the desktop: destroy the nest on {} before they devour its words and melt it.",
                if self.layout.screens.len() > 1 { "every screen" } else { "the screen" }
            ),
            MapKind::Swarm => format!(
                "{}: flies swarm from nests on {}. Pin them with silk, then eat them - before the rivals do.",
                self.map_info.title,
                if self.layout.multi() { "every screen" } else { "the screen" }
            ),
            _ => format!(
                "{}: {}, then seal the Hatchery. Hold clear sites for 4 seconds.",
                self.map_info.title,
                if self.classic_footholds() { "capture Food or Silk" } else { "capture any building" }
            ),
        }
    }
    fn opening_spawns(&mut self, host: &mut impl MissionHost, rng: &mut PyRandom) {
        if self.map_info.kind == MapKind::Reclaim {
            return;
        }
        if self.map_info.kind == MapKind::Swarm {
            for i in 0..crate::swarm::rules(&self.map_info.id).rivals {
                let pos = self.layout.primary().anchor(0.55 + 0.1 * (i % 3) as f64, 0.25 + 0.2 * (i / 3) as f64, 100.0);
                if self.spawn(host, "hunter", pos, false, None, None, rng).is_some() {
                    self.actors.last_mut().unwrap().aggro_range = Some(380.0);
                }
            }
            return;
        }
        for site in self.sites.clone() {
            for (i, guard) in site.guards.iter().enumerate() {
                let d = Self::GUARD_SPOTS[i % 8];
                let pos = self.layout.clamp(site.x + d.0, site.y + d.1, 40.0);
                let spec = match guard {
                    Guard::Spec(s) => Some(s),
                    _ => None,
                };
                self.spawn(host, guard.role(), pos, false, None, spec, rng);
            }
        }
        if let Some(custom) = self.custom.clone() {
            for spec in &custom.enemies {
                let p = spec.place.as_ref().unwrap();
                let others = self.layout.others();
                let screen = if p.far && !others.is_empty() { others[0] } else { *self.layout.primary() };
                let r = screen.rect;
                let pos = r.clamp(r.x + r.w * p.fx, r.y + 115.0 + (r.h - 360.0).max(160.0) * p.fy, 60.0);
                self.spawn(host, &spec.role, pos, false, None, Some(spec), rng);
            }
        }
    }

    pub fn enemy_kind(&self, role: &str, rng: &mut PyRandom) -> Option<&'static EnemyKind> {
        let pool = enemy_kinds::kinds_for_tier(self.map_info.tier, role == "guardian");
        if role == "guardian" {
            return pool.into_iter().reduce(|a, b| if b.tier > a.tier { b } else { a });
        }
        if pool.is_empty() {
            return None;
        }
        let tags: &[&str] = match role {
            "weaver" | "spitter" => &["ranged"],
            "hunter" => &["hunter", "fast"],
            "guard" => &["heavy"],
            _ => &[],
        };
        let mut suited: Vec<_> = pool.iter().copied().filter(|k| k.tags.iter().any(|t| tags.contains(t))).collect();
        if suited.is_empty() {
            suited = pool;
        }
        if role == "guard" {
            let held: Vec<_> =
                suited.iter().copied().filter(|k| !traits_for(Some(k.id), role).temper().chase_forever).collect();
            if !held.is_empty() {
                suited = held;
            }
        }
        let top = suited.iter().map(|k| k.tier).max().unwrap();
        let newest: Vec<_> = suited.iter().copied().filter(|k| k.tier == top).collect();
        if rng.random() < 0.65 {
            suited = newest;
        }
        Some(*rng.choice(&suited))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        host: &mut impl MissionHost,
        role: &str,
        pos: Point,
        raider: bool,
        companion: Option<&str>,
        spec: Option<&SpiderSpec>,
        rng: &mut PyRandom,
    ) -> Option<SpiderId> {
        if role != "guardian" && host.creatures().iter().filter(|&&id| !host.creature(id).dead()).count() >= self.cap {
            return None;
        }
        self.serial += 1;
        let ours = role == "hero" || role == "ally";
        let mut worn = vec![];
        let mut chosen_spec = spec.cloned();
        let hero_level = self.start_progress.level.max(1);
        let mut progress = if role == "hero" {
            self.start_progress.clone()
        } else if role == "ally" {
            if let Some(id) = companion {
                let mut p = companion_progression(&self.profile, id);
                p.relation_overrides = Default::default();
                p
            } else {
                ProgressionState::from_value(&serde_json::json!({"level":(hero_level-1).max(1)}))
            }
        } else {
            let (items, mut level) = campaign::enemy_loadout(&self.map_info, role, rng);
            worn = items;
            if chosen_spec.is_none() && role == "guardian" {
                chosen_spec = self.custom.as_ref().map(|c| c.boss.clone());
            }
            let mut bonus = 0;
            if let Some(s) = &chosen_spec {
                if let Some(p) = s.armor.pieces() {
                    worn = p.to_vec();
                    level = s.item_level;
                }
                bonus = s.level_bonus;
            }
            let mut p = ProgressionState::from_value(
                &serde_json::json!({"level":(hero_level+self.map_info.tier-1+bonus).max(1),"inventory":worn}),
            );
            for id in &worn {
                if let Some(item) = armor_by_id(id) {
                    p.equipped.insert(item.slot, id.clone());
                    p.item_levels.insert(id.clone(), level);
                }
            }
            p
        };
        progress.team_id = if ours { "adventurers" } else { "rivals" }.into();
        let mut kind = if ours { None } else { self.enemy_kind(role, rng) };
        if let Some(s) = &chosen_spec
            && let Some(k) = enemy_kinds::enemy_kinds().iter().find(|k| k.id == s.kind)
        {
            kind = Some(k);
        }
        let mut colors = None;
        let mut model_id = None;
        if !ours {
            if let Some(k) = kind.filter(|k| host.models_contains(k.model_id)) {
                model_id = Some(k.model_id.into());
                colors = Some(enemy_kinds::pick_skin(k, rng.randrange(1 << 30)));
            } else {
                kind = None;
                colors = Some(enemy_palette());
            }
        }
        let name = if role == "ally" {
            companion
                .map(|id| {
                    self.profile
                        .companions
                        .iter()
                        .find(|(k, _)| k == id)
                        .map(|(_, v)| v.name.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| campaign::companion_by_id(id).unwrap().name)
                        .to_owned()
                })
                .unwrap_or_else(|| "Spiderling".into())
        } else if role == "hero" {
            self.hero_name.clone()
        } else if role == "guardian" {
            self.map_info.guardian.clone()
        } else {
            kind.map(|k| k.name.to_owned()).unwrap_or_else(|| {
                let mut s = role.to_owned();
                s[..1].make_ascii_uppercase();
                s
            })
        };
        let traits = traits_for(kind.map(|k| k.id), role);
        let stat_scale = if ours {
            None
        } else {
            Some((
                (if role == "guardian" {
                    2.5
                } else if role == "weaver" {
                    0.75
                } else {
                    0.9
                }) * traits.health
                    * chosen_spec.as_ref().map_or(1.0, |s| s.hp),
                (if role == "guardian" { 1.25 } else { 0.65 }) * traits.bite,
                traits.shell,
            ))
        };
        let request = SpawnRequest {
            serial: self.serial,
            pos,
            progression: progress,
            progression_id: format!("mission-{}", self.serial),
            model_id,
            colors,
            skills: ["jump", "shoot_web", "chase", "approach"],
            enemy_kind: kind.map(|k| k.id.into()),
            size_scale: kind.map_or(1.0, |k| k.size_scale),
            name,
            label_style: if role == "hero" { "hero" } else { "quiet" },
            chooses_own_skills: role == "hero",
            heading: if ours { 0.0 } else { PI },
            stat_scale,
            layout: self.layout.clone(),
        };
        let enemy_kind = request.enemy_kind.clone();
        let id = host.create_creature(request);
        self.spider_state.insert(
            id,
            SpiderMissionState {
                enemy_kind: enemy_kind.clone(),
                mission_loot: worn,
                loot_rolled: ours,
                ..Default::default()
            },
        );
        if role != "hero" {
            let capacity = self.silk_capacity_for(host.creature(id));
            let style = if role == "ally" {
                Some(companion.map(|id| campaign::companion_by_id(id).unwrap().style).unwrap_or("scout"))
            } else {
                None
            };
            let actor = MissionActor::new(
                id,
                host.creature_mut(id),
                self.controls.clone(),
                role,
                pos,
                raider,
                style,
                self.allies.len(),
                capacity,
                enemy_kind.as_deref(),
                companion,
            );
            self.actors.push(actor);
        }
        host.refresh_render_order();
        self.apply_building_bonuses(host);
        Some(id)
    }
}

/// Mode hooks replace Python's overrides without lending the manager to a mode.
pub trait MissionEnvironment<H: MissionHost> {
    fn actor_goal(&self, _m: &TerritoryMission, _host: &H, _actor: &MissionActor) -> Option<Point> {
        None
    }
    fn spawning(&mut self, m: &mut TerritoryMission, host: &mut H, dt: f64, rng: &mut PyRandom) {
        m.spawning(host, dt, rng);
    }
    fn on_acid(&mut self, _m: &mut TerritoryMission, _x: f64, _y: f64, _rng: &mut PyRandom) {}
    fn on_landing(&mut self, _m: &mut TerritoryMission, _c: &H::Spider) {}
    fn on_heavy_step(&mut self, _m: &mut TerritoryMission, _c: &H::Spider) {}
}
pub struct RaidEnvironment;
impl<H: MissionHost> MissionEnvironment<H> for RaidEnvironment {}

impl TerritoryMission {
    pub fn announce(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_time = 5.0;
    }
    pub fn ended(&self) -> bool {
        self.state != "active"
    }
    pub fn end_screen_done(&self) -> bool {
        self.ended() && self.end_clock >= Self::END_SCREEN_SECONDS
    }
    pub fn clear_player_keys(&mut self) {
        for p in &mut self.players {
            p.control.clear_keys();
        }
    }
    pub fn ours(c: &impl MissionSpider) -> bool {
        c.progression().team_id == "adventurers"
    }
    pub fn foes(host: &impl MissionHost, a: SpiderId, b: SpiderId) -> bool {
        a != b && Self::ours(host.creature(a)) != Self::ours(host.creature(b))
    }
    pub fn objective(&self, host: &impl MissionHost) -> String {
        if self.state == "victory" {
            return "VICTORY - the desktop is yours".into();
        }
        if self.state == "defeat" {
            return "RAID ENDED - your spider has fallen".into();
        }
        if !self.footholds().any(|s| s.owned) {
            return if self.classic_footholds() {
                "01 / Capture the Food cache or Silk loom"
            } else {
                "01 / Capture any building for a foothold"
            }
            .into();
        }
        if self.site("hatchery").is_some_and(|i| !self.sites[i].owned) {
            return "02 / Seal the Hatchery to stop reinforcements".into();
        }
        if self.pending.iter().any(|e| e.2)
            || self.actors.iter().any(|a| a.raider && !a.from_outpost && !host.creature(a.creature).dead())
        {
            return "03 / Defeat the counterattack".into();
        }
        "04 / Defeat the guardian and claim Thorn nest".into()
    }
    pub fn issue(&mut self, host: &impl MissionHost, command: &str, aim: Point) {
        if self.ended() {
            return;
        }
        if !self.allies.iter().any(|&c| !host.creature(c).dead()) {
            self.announce("Your companions have fallen. You can still finish the raid.");
            return;
        }
        if command == "attack" {
            let mut target = host
                .creatures()
                .into_iter()
                .filter(|&c| !host.creature(c).dead() && Self::foes(host, self.hero, c))
                .min_by(|&a, &b| {
                    distance(point(host.creature(a)), aim).total_cmp(&distance(point(host.creature(b)), aim))
                });
            if target.is_none_or(|c| distance(point(host.creature(c)), aim) > 100.0) {
                target = self.hover_target;
            }
            if target.is_none_or(|c| host.creature(c).dead()) {
                self.announce("Point at an enemy, then press Attack target.");
                return;
            }
            self.attack_target = target;
        }
        self.command = command.into();
        for a in &mut self.actors {
            if a.role == "ally" {
                a.think = 0.0;
                if command == "defend" {
                    a.defend_point = point(host.creature(a.creature));
                }
            }
        }
        if command == "defend"
            && let Some(id) = self.ally
        {
            self.defend_point = point(host.creature(id));
        }
        let who = if self.allies.len() <= 1 { "Scout" } else { "Companions" };
        let what = match command {
            "defend" => "defending this position",
            "attack" => "attacking your target",
            _ => "following you",
        };
        self.announce(format!("{who}: {what}"));
    }
    pub fn bite(host: &mut impl MissionHost, id: SpiderId, control: &mut PlayerController) -> Bite {
        let ids: Vec<_> =
            host.creatures().into_iter().filter(|&c| !host.creature(c).dead() && Self::foes(host, id, c)).collect();
        let targets = ids.iter().map(|&c| aim_target(host.creature(c))).collect::<Vec<_>>();
        let result = control.bite(host.creature_mut(id), &targets, true);
        if let Bite::Hit(i) = result {
            host.trade_blow(id, ids[i]);
        }
        result
    }
    pub fn shoot(host: &mut impl MissionHost, id: SpiderId, control: &mut PlayerController, aim: ShotAim) -> bool {
        if let Some(shot) = control.shoot(host.creature_mut(id), aim) {
            host.shoot(id, shot);
            true
        } else {
            false
        }
    }
    pub fn update(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        self.update_with(host, dt, rng, &mut RaidEnvironment);
    }
    pub fn update_with<H: MissionHost>(
        &mut self,
        host: &mut H,
        dt: f64,
        rng: &mut PyRandom,
        env: &mut impl MissionEnvironment<H>,
    ) {
        if self.ended() {
            self.end_clock += dt.max(0.0);
            return;
        }
        let dt = dt.clamp(0.0, 0.05);
        self.elapsed += dt;
        for p in &mut self.players {
            if p.control.held.contains(&Action::Bite) {
                Self::bite(host, p.creature, &mut p.control);
            }
        }
        for c in host.creatures() {
            if !host.creature(c).dead()
                && Self::foes(host, self.hero, c)
                && distance(point(host.creature(c)), self.players[0].control.aim) < 75.0
            {
                self.hover_target = Some(c);
            }
        }
        self.notice_time = (self.notice_time - dt).max(0.0);
        for id in host.creatures() {
            let c = host.creature(id);
            let was_airborne = c.airborne();
            let before = point(c);
            let p = self.players.iter().position(|p| p.creature == id);
            let a = self.actors.iter().position(|a| a.creature == id);
            let struggling = p.is_none_or(|p| self.players[p].control.struggling());
            // Creature.update clamps independently, including dt=0.
            let creature_dt = dt.max(0.001);
            if host.update_creature(id, creature_dt, struggling) {
                if let Some(p) = p {
                    self.players[p].control.update(host.creature_mut(id), creature_dt);
                } else if let Some(a) = a {
                    let mut actor = self.actors[a].clone();
                    actor.update(self, host, creature_dt, env);
                    self.actors[a] = actor;
                }
            }
            let c = host.creature_mut(id);
            let pos = self.layout.clamp(c.x(), c.y(), c.margin());
            c.set_position(pos);
            if !c.dead() {
                self.through_tunnel(host, id, dt);
                let c = host.creature(id);
                if was_airborne && !c.airborne() {
                    env.on_landing(self, c);
                } else if !c.airborne() {
                    let moved = distance(point(c), before);
                    if c.size_scale() >= Self::HEAVY_WALK && moved > 0.0 {
                        let s = self.spider_state.get_mut(&id).unwrap();
                        s.heavy_walked += moved;
                        if s.heavy_walked >= Self::STEP_EVERY {
                            s.heavy_walked = 0.0;
                            env.on_heavy_step(self, c);
                        }
                    }
                }
            }
        }
        host.update_projectiles(dt);
        self.update_hazards(host, dt, rng, env);
        self.drop_loot(host, rng);
        host.bury_the_dead();
        self.collect_loot(host, dt);
        self.actors.retain(|a| !host.creature(a.creature).dead());
        host.update_carcasses(dt);
        if self.players.iter().all(|p| host.creature(p.creature).dead()) {
            self.state = "defeat".into();
            self.clear_player_keys();
            self.finish(host, false);
            return;
        }
        if self.command == "attack" && self.attack_target.is_none_or(|c| host.creature(c).dead()) {
            self.command = "follow".into();
            self.attack_target = None;
        }
        for i in 0..self.sites.len() {
            let pos = self.sites[i].point();
            let contested = host.creatures().iter().any(|&c| {
                !host.creature(c).dead()
                    && Self::foes(host, self.hero, c)
                    && distance(point(host.creature(c)), pos) < 125.0
            });
            self.sites[i].contested = contested;
            let near = self
                .players
                .iter()
                .any(|p| !host.creature(p.creature).dead() && distance(point(host.creature(p.creature)), pos) < 92.0);
            let held_safe = if self.sites[i].owned { !contested } else { !self.adventurers_near(host, i) };
            if held_safe {
                self.heal_at(host, i, dt);
                self.refill_silk_at(host, i, dt);
            }
            if self.sites[i].owned {
                continue;
            }
            let unlocked = self.sites[i].kind != "nest"
                || (self.site("hatchery").is_some_and(|i| self.sites[i].owned)
                    && self.footholds().any(|s| s.owned)
                    && self.guardian.is_some_and(|c| host.creature(c).dead())
                    && self.pending.is_empty()
                    && !self.actors.iter().any(|a| a.raider && !a.from_outpost));
            if near && !contested && unlocked {
                self.sites[i].progress = (self.sites[i].progress + dt / Self::CAPTURE_SECONDS).min(1.0);
                if self.sites[i].progress >= 1.0 {
                    self.capture(host, i);
                }
            } else if !near {
                self.sites[i].progress = (self.sites[i].progress - dt * 0.12).max(0.0);
            }
        }
        env.spawning(self, host, dt, rng);
        self.outpost_waves(host, dt, rng);
        self.building_work(host, dt, rng);
        self.plan_squads(host, dt);
    }
    pub fn adventurers_near(&self, host: &impl MissionHost, i: usize) -> bool {
        self.players
            .iter()
            .map(|p| p.creature)
            .chain(self.allies.iter().copied())
            .any(|id| !host.creature(id).dead() && distance(point(host.creature(id)), self.sites[i].point()) < 125.0)
    }
    fn holders(&self, host: &impl MissionHost, i: usize) -> Vec<(SpiderId, f64)> {
        let s = &self.sites[i];
        let mut holders = vec![];
        if s.owned {
            for p in &self.players {
                if !host.creature(p.creature).dead() {
                    holders.push((p.creature, Self::BASE_REACH));
                }
            }
        }
        for a in &self.actors {
            if (a.role == "ally") == s.owned && !host.creature(a.creature).dead() {
                holders.push((a.creature, if s.owned { Self::SCOUT_HEAL_REACH } else { Self::BASE_REACH }));
            }
        }
        holders
    }
    pub fn control_mut(&mut self, id: SpiderId) -> Option<&mut PlayerController> {
        if let Some(p) = self.players.iter_mut().find(|p| p.creature == id) {
            return Some(&mut p.control);
        }
        self.actors.iter_mut().find(|a| a.creature == id).map(|a| &mut a.control)
    }
    pub fn heal_rate(kind: &str) -> Option<f64> {
        match kind {
            "food" => Some(8.0),
            "home" | "nest" => Some(3.0),
            _ => None,
        }
    }
    pub fn silk_rate(kind: &str) -> Option<f64> {
        match kind {
            "silk" => Some(3.0),
            "home" | "nest" => Some(1.0),
            _ => None,
        }
    }
    pub fn silk_capacity_for(&self, spider: &impl MissionSpider) -> i64 {
        if self.map_info.kind == MapKind::Reclaim {
            return PlayerController::SILK_CAPACITY;
        }
        if self.site("silk").is_some_and(|i| self.sites[i].owned == Self::ours(spider)) {
            PlayerController::LOOM_SILK_CAPACITY
        } else {
            PlayerController::SILK_CAPACITY
        }
    }
    pub fn heal_at(&mut self, host: &mut impl MissionHost, i: usize, dt: f64) {
        let site = &self.sites[i];
        let Some(rate) = Self::heal_rate(&site.kind) else {
            return;
        };
        if site.kind == "home" && !site.owned {
            return;
        }
        let pos = site.point();
        for (id, reach) in self.holders(host, i) {
            if self.sites[i].supply <= 0.0 {
                return;
            }
            let c = host.creature_mut(id);
            if distance(point(c), pos) >= reach {
                continue;
            }
            let amount = (dt * rate).min(self.sites[i].supply).min(c.max_hp() - c.hp());
            if amount > 0.0 {
                c.heal(amount);
                self.sites[i].supply -= amount;
            }
        }
    }
    pub fn refill_silk_at(&mut self, host: &impl MissionHost, i: usize, dt: f64) {
        let site = &self.sites[i];
        let Some(rate) = Self::silk_rate(&site.kind) else {
            return;
        };
        if site.kind == "home" && !site.owned {
            return;
        }
        let pos = site.point();
        for (id, reach) in self.holders(host, i) {
            if distance(point(host.creature(id)), pos) >= reach {
                continue;
            }
            if let Some(c) = self.control_mut(id) {
                c.silk = (c.silk + dt * rate).min(c.silk_capacity as f64);
            }
        }
    }
    pub fn capture(&mut self, host: &mut impl MissionHost, i: usize) {
        if self.sites[i].owned {
            return;
        }
        self.sites[i].owned = true;
        for p in &self.players {
            host.creature_mut(p.creature).gain_experience(35, "territory captured");
        }
        let kind = self.sites[i].kind.clone();
        let effect = BuildingKind::from_id(&kind).and_then(BuildingKind::effect);
        self.announce(format!(
            "{} secured{}",
            self.sites[i].name,
            effect.map(|s| format!(" - {s} for your side")).unwrap_or_default()
        ));
        self.apply_building_bonuses(host);
        if kind == "silk" {
            for p in &mut self.players {
                p.control.silk_capacity = PlayerController::LOOM_SILK_CAPACITY;
                p.control.silk = p.control.silk_capacity as f64;
            }
            for a in &mut self.actors {
                if a.role != "ally" {
                    a.control.silk_capacity = PlayerController::SILK_CAPACITY;
                    a.control.silk = a.control.silk.min(a.control.silk_capacity as f64);
                }
            }
        }
        if kind == "hatchery" {
            self.sites[i].reserves = 0;
            self.sites[i].warning = 0.0;
            self.pending.retain(|e| e.0 != "hatchery");
        }
        if !Self::NOT_FOOTHOLDS.contains(&kind.as_str())
            && !self.counter_started
            && let Some(nest) = self.site("nest")
        {
            self.counter_started = true;
            self.pending.push(("nest".into(), "hunter".into(), true));
            if self.map_info.tier >= 2 {
                self.pending.push(("nest".into(), "guard".into(), true));
            }
            if self.map_info.tier >= 4 {
                self.pending.push(("nest".into(), "weaver".into(), true));
            }
            self.sites[nest].warning = 4.0;
            self.announce("Outpost secured! Counterattack from Thorn nest in 4 seconds.");
        }
        if kind == "outpost" || kind == "infestation" {
            self.sites[i].reserves = 0;
            self.sites[i].warning = 0.0;
            for p in &self.players {
                host.creature_mut(p.creature).gain_experience(25, "outpost taken");
            }
            self.announce(format!(
                "{} taken on {} - no more raids from it",
                self.sites[i].name,
                self.layout.screens[self.sites[i].screen].name()
            ));
        }
        if kind == "nest" {
            self.state = "victory".into();
            for p in &self.players {
                host.creature_mut(p.creature).gain_experience(100, "raid complete");
            }
            self.clear_player_keys();
            self.finish(host, true);
        }
    }
    pub fn drop_loot(&mut self, host: &impl MissionHost, rng: &mut PyRandom) {
        for id in host.creatures() {
            let c = host.creature(id);
            let Some(s) = self.spider_state.get_mut(&id) else {
                continue;
            };
            if !c.dead() || s.loot_rolled {
                continue;
            }
            s.loot_rolled = true;
            for (i, item_id) in
                campaign::roll_drop(&s.mission_loot, Some(id) == self.guardian, rng).into_iter().enumerate()
            {
                let (x, y) = self.layout.clamp(c.x() + i as f64 * 30.0, c.y() + i as f64 * 12.0, 30.0);
                self.loot.push(Loot { item_id, x, y, age: 0.0 });
            }
        }
    }
    pub fn collect_loot(&mut self, host: &impl MissionHost, dt: f64) {
        let pickers = self
            .players
            .iter()
            .map(|p| p.creature)
            .chain(self.allies.iter().copied())
            .filter(|&c| !host.creature(c).dead())
            .map(|c| point(host.creature(c)))
            .collect::<Vec<_>>();
        let mut remaining = vec![];
        for mut loot in std::mem::take(&mut self.loot) {
            loot.age += dt;
            if !pickers.iter().any(|&p| distance(p, (loot.x, loot.y)) < LOOT_REACH) {
                remaining.push(loot);
                continue;
            }
            let result = armoury::add_loot(&mut self.profile, &loot.item_id);
            let item = armor_by_id(&loot.item_id).unwrap();
            let extra = if result == armoury::Loot::Spare { " (spare - stack it to upgrade)" } else { "" };
            let mut tier = item.tier.to_owned();
            tier[..1].make_ascii_uppercase();
            self.announce(format!("Found {} - {tier}{extra}", item.name));
            self.found.push((loot.item_id, result));
        }
        self.loot = remaining;
    }
    pub fn spawning(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        let Some(hatch) = self.site("hatchery") else {
            return;
        };
        self.wave_clock -= dt;
        if !self.sites[hatch].owned && self.sites[hatch].reserves > 0 && self.wave_clock <= 0.0 {
            if !self.pending.iter().any(|e| e.0 == "hatchery") {
                self.pending.push(("hatchery".into(), "hunter".into(), true));
                self.sites[hatch].warning = 3.0;
                self.announce("The Hatchery is stirring - a hunter is emerging");
            }
            self.wave_clock = self.wave_every;
        }
        for s in &mut self.sites {
            s.warning = (s.warning - dt).max(0.0);
        }
        let mut idx = 0;
        while idx < self.pending.len() {
            let (source, role, raider) = self.pending[idx].clone();
            let site = self.site(&source).unwrap();
            if self.sites[site].owned || (source == "hatchery" && self.sites[site].reserves <= 0) {
                self.pending.remove(idx);
                continue;
            }
            if self.sites[site].warning > 0.0 || host.creatures().len() >= 8 {
                idx += 1;
                continue;
            }
            let s = &self.sites[site];
            let pos = self.layout.clamp(s.x + 50.0, s.y + 50.0, 65.0);
            if self
                .players
                .iter()
                .any(|p| !host.creature(p.creature).dead() && distance(point(host.creature(p.creature)), pos) < 150.0)
            {
                idx += 1;
                continue;
            }
            if self.spawn(host, &role, pos, raider, None, None, rng).is_some() {
                self.pending.remove(idx);
                if source == "hatchery" {
                    self.sites[site].reserves -= 1;
                }
                self.sites[site].warning = if self.pending.iter().any(|e| e.0 == source) { 1.5 } else { 0.0 };
            } else {
                idx += 1;
            }
        }
        if self.guardian.is_none()
            && self.sites[hatch].owned
            && self.footholds().any(|s| s.owned)
            && self.pending.is_empty()
            && !self.actors.iter().any(|a| a.raider && !a.from_outpost)
        {
            let Some(nest) = self.site("nest") else {
                return;
            };
            if self.guardian_warning.is_none() {
                self.guardian_warning = Some(4.0);
                self.announce("Thorn nest is stirring. The guardian will emerge in 4 seconds.");
            }
            let warning = (self.guardian_warning.unwrap() - dt).max(0.0);
            self.guardian_warning = Some(warning);
            self.sites[nest].warning = warning;
            let pos = self.sites[nest].point();
            if warning <= 0.0
                && self.players.iter().all(|p| {
                    host.creature(p.creature).dead() || distance(point(host.creature(p.creature)), pos) >= 150.0
                })
            {
                self.guardian = self.spawn(host, "guardian", pos, false, None, None, rng);
                if self.guardian.is_some() {
                    self.announce("Thorn guardian awakened. Bait its strike, then counterattack.");
                }
            }
        }
    }
    pub fn holds(&self, kind: &str, ours: bool) -> bool {
        self.sites.iter().any(|s| s.kind == kind && s.owned == ours)
    }
    pub fn apply_building_bonuses(&mut self, host: &mut impl MissionHost) {
        for id in host.creatures() {
            let c = host.creature_mut(id);
            let mult = if self.holds("venom", Self::ours(c)) { Self::VENOM_BONUS } else { 1.0 };
            let s = self.spider_state.get_mut(&id).unwrap();
            if s.venom_applied.is_none_or(|v| (c.damage() - v).abs() > 1e-6) {
                s.venom_base = c.damage();
            }
            c.set_damage(s.venom_base * mult);
            s.venom_applied = Some(c.damage());
        }
        let adventurer = self.holds("lookout", true);
        let rival = self.holds("lookout", false);
        for p in &mut self.players {
            let holds = if Self::ours(host.creature(p.creature)) { adventurer } else { rival };
            p.control.web_range = PlayerController::WEB_RANGE * if holds { Self::LOOKOUT_BONUS } else { 1.0 };
        }
        for a in &mut self.actors {
            let holds = if Self::ours(host.creature(a.creature)) { adventurer } else { rival };
            let reach = if holds { Self::LOOKOUT_BONUS } else { 1.0 };
            a.control.web_range = PlayerController::WEB_RANGE * reach;
            a.spit_range = MissionActor::SPIT_RANGE * reach;
        }
    }
    pub fn building_work(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        self.bonus_clock -= dt;
        if self.bonus_clock <= 0.0 {
            self.bonus_clock = 0.5;
            self.apply_building_bonuses(host);
        }
        for i in 0..self.sites.len() {
            if self.sites[i].kind == "amber" && self.sites[i].owned {
                self.amber_clock += dt;
                if self.amber_clock >= Self::AMBER_EVERY {
                    self.amber_clock = 0.0;
                    self.profile.armoury.amber += 1;
                    self.amber_mined += 1;
                }
            } else if self.sites[i].kind == "nursery" {
                self.nursery(host, i, dt, rng);
            }
        }
    }
    pub fn nursery(&mut self, host: &mut impl MissionHost, i: usize, dt: f64, rng: &mut PyRandom) {
        let ours = self.sites[i].owned;
        let clock = self.nursery_clocks.entry((i, ours)).or_insert(Self::NURSERY_EVERY * 0.5);
        *clock -= dt;
        if *clock > 0.0 {
            return;
        }
        *clock = Self::NURSERY_EVERY;
        let brood = host
            .creatures()
            .iter()
            .filter(|&&id| {
                !host.creature(id).dead()
                    && self.spider_state.get(&id).is_some_and(|s| s.spiderling)
                    && Self::ours(host.creature(id)) == ours
            })
            .count();
        if brood >= Self::NURSERY_BROOD {
            return;
        }
        let site = &self.sites[i];
        let pos = self.layout.clamp(site.x + rng.uniform(-50.0, 50.0), site.y + 55.0, 40.0);
        let Some(id) = self.spawn(host, if ours { "ally" } else { "hunter" }, pos, false, None, None, rng) else {
            return;
        };
        let s = self.spider_state.get_mut(&id).unwrap();
        s.spiderling = true;
        s.mission_loot.clear();
        let c = host.creature_mut(id);
        c.set_size_scale(c.size_scale() * 0.62);
        c.set_max_hp(c.max_hp() * 0.5);
        c.set_hp(c.max_hp());
        if !ours {
            c.set_name("Brood spiderling");
        }
        self.announce(if ours { "A spiderling hatched for you" } else { "The enemy's nursery hatched a spiderling" });
    }
    pub fn through_tunnel(&mut self, host: &mut impl MissionHost, id: SpiderId, dt: f64) {
        let s = self.spider_state.get_mut(&id).unwrap();
        if s.tunnel_cooldown > 0.0 {
            s.tunnel_cooldown = (s.tunnel_cooldown - dt).max(0.0);
            return;
        }
        let c = host.creature_mut(id);
        let Some((_, (fx, fy))) = self.layout.tunnel_at(c.x(), c.y(), dc_world::screen_layout::TUNNEL_REACH) else {
            return;
        };
        let (cx, cy) = self.layout.screen_at(fx, fy).centre();
        let d = (cx - fx).hypot(cy - fy).max(1.0);
        let pos = self.layout.clamp(fx + (cx - fx) / d * 48.0, fy + (cy - fy) / d * 48.0, c.margin());
        c.set_position(pos);
        c.set_target(pos.0, pos.1);
        s.tunnel_cooldown = Self::TUNNEL_COOLDOWN;
        if id == self.hero {
            self.announce(format!("Through the tunnel to {}", self.layout.screen_at(pos.0, pos.1).name()));
        }
    }
    pub fn outpost_waves(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        // Site identities are indices, not copies: director warning writes feed
        // the same site that capture and painters see.
        let mut sites = self.outposts.iter().map(|&i| self.sites[i].clone()).collect::<Vec<_>>();
        let waves = self.director.update(dt, &mut sites);
        for (&i, s) in self.outposts.iter().zip(sites) {
            self.sites[i].warning = s.warning;
        }
        for (index, role) in waves {
            let i = self.outposts[index];
            if host.creatures().len() >= self.cap - 1 {
                self.director.retry(index);
                continue;
            }
            let s = &self.sites[i];
            let pos = self.layout.clamp(s.x + 45.0, s.y + 55.0, 60.0);
            if self.spawn(host, &role, pos, true, None, None, rng).is_none() {
                self.director.retry(index);
                continue;
            }
            self.actors.last_mut().unwrap().from_outpost = true;
            self.sites[i].reserves -= 1;
            self.announce(format!(
                "Raiders from {} on {}",
                self.sites[i].name,
                self.layout.screens[self.sites[i].screen].name()
            ));
        }
    }
    pub fn spit_acid(&mut self, host: &mut impl MissionHost, shooter: SpiderId, pos: Point) {
        let (x1, y1) = self.layout.clamp(pos.0, pos.1, 8.0);
        let c = host.creature_mut(shooter);
        let distance = distance((x1, y1), point(c));
        self.hazards.push(AcidGlob {
            shooter,
            x0: c.x(),
            y0: c.y(),
            x1,
            y1,
            flight: distance / AcidGlob::SPEED,
            age: 0.0,
            done: false,
        });
        c.begin_strike(x1, y1);
    }
    pub fn update_hazards<H: MissionHost>(
        &mut self,
        host: &mut H,
        dt: f64,
        rng: &mut PyRandom,
        env: &mut impl MissionEnvironment<H>,
    ) {
        let mut remaining = vec![];
        for mut g in std::mem::take(&mut self.hazards) {
            g.age += dt;
            if g.age >= g.flight && !g.done {
                g.done = true;
                self.splashes.push(Splash { x: g.x1, y: g.y1, age: 0.0 });
                for id in host.creatures() {
                    let t = host.creature(id);
                    if t.dead()
                        || id == g.shooter
                        || t.airborne()
                        || !Self::foes(host, g.shooter, id)
                        || distance(point(t), (g.x1, g.y1)) > AcidGlob::SPLASH + t.size() * 0.5
                    {
                        continue;
                    }
                    let damage = host.creature(g.shooter).damage() * 1.1;
                    host.splash(g.shooter, id, damage);
                }
                env.on_acid(self, g.x1, g.y1, rng);
            }
            if !g.done {
                remaining.push(g);
            }
        }
        self.hazards = remaining;
        for s in &mut self.splashes {
            s.age += dt;
        }
        self.splashes.retain(|s| s.age < 0.9);
    }
    pub fn plan_squads(&mut self, host: &impl MissionHost, dt: f64) {
        self.squad_clock -= dt;
        if self.squad_clock > 0.0 {
            return;
        }
        self.squad_clock = Self::SQUAD_EVERY;
        let enemies = self
            .actors
            .iter()
            .enumerate()
            .filter(|(_, a)| a.role != "ally" && !host.creature(a.creature).dead())
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        for &i in &enemies {
            self.actors[i].order = None;
            self.actors[i].doctrine = tactics::Doctrine::None;
        }
        if self.tactics_level <= 0 || enemies.len() < 2 {
            return;
        }
        let foes = host
            .creatures()
            .into_iter()
            .filter(|&id| !host.creature(id).dead() && Self::ours(host.creature(id)))
            .map(|id| {
                let c = host.creature(id);
                tactics::Foe {
                    key: id,
                    x: c.x(),
                    y: c.y(),
                    health: c.hp() / c.max_hp().max(1.0),
                    threat: if id == self.hero { 1.6 } else { 1.0 },
                }
            })
            .collect::<Vec<_>>();
        let mut attack = vec![];
        let mut posts: Vec<(Option<usize>, Vec<usize>)> = vec![];
        for i in enemies {
            let a = &self.actors[i];
            if a.raider || a.hunting {
                attack.push(i);
                continue;
            }
            let site = self
                .sites
                .iter()
                .enumerate()
                .min_by(|(_, s), (_, t)| distance(s.point(), a.home).total_cmp(&distance(t.point(), a.home)))
                .and_then(|(i, s)| (distance(s.point(), a.home) <= Self::SQUAD_REACH).then_some(i));
            if let Some((_, g)) = posts.iter_mut().find(|(s, _)| *s == site) {
                g.push(i);
            } else {
                posts.push((site, vec![i]));
            }
        }
        let squads = std::iter::once((None, attack, tactics::Stance::Attack))
            .chain(posts.into_iter().map(|(s, g)| (s, g, tactics::Stance::Defend)));
        for (site, group, stance) in squads {
            if group.len() < 2 {
                continue;
            }
            let anchor = site.map(|i| self.sites[i].point()).unwrap_or(self.actors[group[0]].home);
            let members = group
                .iter()
                .map(|&i| {
                    let a = &self.actors[i];
                    let c = host.creature(a.creature);
                    let refuge = self
                        .sites
                        .iter()
                        .filter(|s| !s.owned && Self::heal_rate(&s.kind).is_some())
                        .min_by(|a, b| distance(a.point(), point(c)).total_cmp(&distance(b.point(), point(c))));
                    tactics::Member {
                        key: a.creature,
                        x: c.x(),
                        y: c.y(),
                        health: c.hp() / c.max_hp().max(1.0),
                        role: a.traits.and_then(|t| tactics::Role::parse(t.squad_role)).unwrap_or_default(),
                        reach: a.fighting_reach(c),
                        abilities: a
                            .traits
                            .map(|t| t.abilities.iter().map(|s| s.to_string()).collect())
                            .unwrap_or_default(),
                        anchor: Some(refuge.map(MissionSite::point).unwrap_or(a.home)),
                    }
                })
                .collect::<Vec<_>>();
            let notice = group
                .iter()
                .filter_map(|&i| self.actors[i].traits.map(|t| t.temper().notice))
                .reduce(f64::max)
                .unwrap_or(300.0);
            let result = tactics::plan(&members, &foes, self.tactics_level as i32, stance, Some(anchor), notice);
            for i in group {
                let a = &mut self.actors[i];
                a.order = result.orders.iter().find(|(id, _)| *id == a.creature).map(|(_, o)| o.clone());
                a.doctrine = result.doctrine;
            }
        }
    }
    pub fn refresh_from_profile(&mut self, host: &mut impl MissionHost) {
        self.profile = load_profile(&self.progress_path);
        host.creature_mut(self.hero).set_name(&self.profile.name);
        let mut pairs = vec![(self.hero, hero_progression(&self.profile))];
        if let Some(id) = self.second_hero {
            host.creature_mut(id).set_name(&self.profile.player_two.name);
            pairs.push((id, spider_progression(&self.profile, PLAYER_TWO)));
        }
        for a in &self.actors {
            if a.role == "ally"
                && !host.creature(a.creature).dead()
                && let Some(id) = &a.companion_id
            {
                pairs.push((a.creature, companion_progression(&self.profile, id)));
            }
        }
        for (id, p) in pairs {
            let c = host.creature_mut(id);
            let live = c.progression_mut();
            live.level = p.level;
            live.xp = p.xp;
            live.total_xp = p.total_xp;
            live.skill_points = p.skill_points;
            live.unlocked_abilities = p.unlocked_abilities;
            live.inventory = p.inventory;
            live.equipped = p.equipped;
            live.item_levels = p.item_levels;
            c.apply_progression_stats(false);
        }
        self.apply_building_bonuses(host);
    }
    pub fn finish(&mut self, host: &impl MissionHost, won: bool) {
        if self.saved {
            return;
        }
        let first_win = won && mission_record(&self.profile, &self.map_info.id).victories == 0;
        record_result(&mut self.profile, &self.map_info.id, won, self.elapsed);
        if first_win
            && let Some(reward) = &self.map_info.reward_companion
            && unlock_companion(&mut self.profile, reward)
        {
            self.reward_text = format!("{} joins you", campaign::companion_by_id(reward).unwrap().name);
        }
        self.save_progress(host);
    }
    pub fn save_progress(&mut self, host: &impl MissionHost) -> bool {
        self.profile.progression = Some(host.creature(self.hero).progression().clone());
        if let Some(id) = self.second_hero {
            store_progression(&mut self.profile, PLAYER_TWO, host.creature(id).progression().clone());
        }
        for a in &self.actors {
            if a.role == "ally"
                && let Some(id) = &a.companion_id
            {
                store_progression(&mut self.profile, id, host.creature(a.creature).progression().clone());
            }
        }
        if save_profile(&self.profile, &self.progress_path) {
            self.saved = self.ended();
            self.save_error.clear();
            true
        } else {
            self.save_error = "Progress could not be saved. Retry from the pause menu.".into();
            false
        }
    }
    pub fn restarted(&self, host: &mut impl MissionHost, rng: &mut PyRandom) -> Result<Self, String> {
        Self::new(host, self.controls.clone(), self.layout.clone(), self.progress_path.parent().unwrap(), rng)
    }
}
