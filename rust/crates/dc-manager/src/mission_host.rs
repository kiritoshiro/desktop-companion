//! `MissionHost` for `CreatureManager`: everything `app/mission.py`,
//! `swarm.py` and `reclaim.py` did to the manager directly. Python's missions
//! held the manager and reached into its lists; the Rust missions make every
//! choice (and random draw) themselves and ask the manager to carry it out.

use crate::CreatureManager;
use dc_adventure::adventure::SilkShot;
use dc_adventure::mission::{MissionHost, SpawnRequest, SpiderId};
use dc_core::PyRandom;
use dc_creature::creature::LabelStyle;
use dc_creature::{Creature, CreatureConfig, Env, Population, UpdateOutcome};
use dc_world::flies::FearField;
use dc_world::{FlyId, FlyWorld, ScreenLayout, WorldCreature};
use serde_json::json;
use std::sync::Arc;

/// Where `mission.py` updated its spiders from: far off-screen, so nothing
/// reacts to the real cursor.
const NO_CURSOR: f64 = -10000.0;

impl CreatureManager {
    fn mission_spider(&self, id: SpiderId) -> Option<&Creature> {
        self.creatures.iter().chain(&self.mission_graveyard).find(|c| c.id == id.0)
    }

    /// `FlyWorld.update`'s fear field over the live spiders, for flies the
    /// swarm moves one at a time (`fly.update(dt, spiders, web_world)`).
    fn fear_reach(spiders: &[Creature]) -> f64 {
        let mut reach = 0.0f64;
        for sp in spiders {
            let mut r = sp.size() * 5.5 + 110.0 + sp.current_speed() * 0.22 + sp.aim_intent() * 70.0;
            if sp.airborne() {
                r += sp.size() * 3.0;
            }
            reach = if r.is_finite() { reach.max(r) } else { f64::INFINITY };
        }
        reach
    }
}

impl MissionHost for CreatureManager {
    type Spider = Creature;

    fn begin_mission(&mut self) -> Result<(), String> {
        if self.mission_source.is_none() {
            let source = self
                .creatures
                .iter()
                .find(|c| !c.dead)
                .ok_or_else(|| "Adventure needs at least one spider in the preset".to_string())?;
            self.mission_source = Some(((*source.model).clone(), (*source.personality).clone()));
        }
        self.conflict_enabled = true;
        self.interferable = false;
        self.naming_enabled = false;
        self.allow_mouse_capture = false;
        self.cages.clear();
        self.web_world.clear();
        self.fly_world.clear();
        self.fly_world.clear_spawners();
        self.fly_world.remains.clear();
        self.base_world.clear();
        self.carcasses.clear();
        self.creatures.clear();
        self.render_order.clear();
        self.mission_graveyard.clear();
        // Assigned, not set: no spider is left to hand it to.
        self.team_stances =
            Arc::new(dc_state::progression::normalize_team_stances(Some(&json!({"adventurers": {"rivals": "foe"}}))));
        Ok(())
    }

    fn screen_size(&self) -> (f64, f64) {
        (self.screen_w, self.screen_h)
    }

    fn models_contains(&self, model_id: &str) -> bool {
        self.models.get(model_id).is_some()
    }

    fn creatures(&self) -> Vec<SpiderId> {
        self.creatures.iter().map(|c| dc_world::CreatureId(c.id)).collect()
    }

    fn creature(&self, id: SpiderId) -> &Creature {
        self.mission_spider(id).expect("a mission spider id")
    }

    fn creature_mut(&mut self, id: SpiderId) -> &mut Creature {
        self.creatures
            .iter_mut()
            .chain(self.mission_graveyard.iter_mut())
            .find(|c| c.id == id.0)
            .expect("a mission spider id")
    }

    /// `_spawn`: `manager._create_creature(...)` and what the mission then set
    /// on the new spider, in the same order.
    fn create_creature(&mut self, request: SpawnRequest) -> SpiderId {
        let (source_model, personality) = self.mission_source.clone().expect("begin_mission first");
        let model = match &request.model_id {
            Some(id) => self.models.get(id).cloned().unwrap_or(source_model),
            None => source_model,
        };
        let mut cfg = CreatureConfig::new(model, personality, self.screen_w, self.screen_h);
        cfg.index = request.serial as i64;
        cfg.skills = Some(request.skills.iter().map(|s| s.to_string()).collect());
        cfg.color_overrides = request.colors.as_ref().map(|c| serde_json::to_value(c).expect("colours"));
        cfg.progression_state = Some(request.progression.to_value());
        cfg.progression_id = Some(request.progression_id.clone());
        let team = request.progression.team_id.clone();
        let mut c = self.create_creature(cfg, Some(request.pos), Some(&team), "");
        if request.enemy_kind.is_some() {
            let scale = c.size_scale * request.size_scale;
            c.set_size_scale(scale);
        }
        c.set_name(&request.name);
        c.label_style = match request.label_style {
            "hero" => LabelStyle::Hero,
            "quiet" => LabelStyle::Quiet,
            _ => LabelStyle::Full,
        };
        if request.chooses_own_skills {
            c.chooses_own_skills = true;
        }
        c.playfield = Some(Arc::new(request.layout.playfield.clone()));
        c.hp = c.max_hp;
        c.energy = c.max_energy;
        c.kin.heading = request.heading;
        c.kin.target_heading = request.heading;
        if let Some(scale) = request.stat_scale {
            c.stat_scale = Some(scale);
            c.apply_progression_stats(true, false);
        }
        let id = dc_world::CreatureId(c.id);
        self.creatures.push(c);
        id
    }

    fn refresh_neighbor_links(&mut self) {
        CreatureManager::refresh_neighbor_links(self);
    }

    fn refresh_render_order(&mut self) {
        CreatureManager::refresh_render_order(self);
    }

    /// `c.update(dt, -10000, -10000, sw, sh)` up to the player-control branch.
    fn update_creature(&mut self, id: SpiderId, dt: f64, struggling: bool) -> bool {
        let Some(i) = self.creatures.iter().position(|c| c.id == id.0) else {
            return false;
        };
        self.creatures[i].player_struggling = struggling;
        let pop = Population::from_views(self.creatures.iter().map(Creature::view));
        let mut commands = Vec::new();
        let (w, h) = (self.screen_w, self.screen_h);
        let outcome = {
            let mut env = Env {
                population: &pop,
                webs: Some(&mut self.web_world),
                flies: Some(&mut self.fly_world),
                mouse_webs: Some(&mut self.mouse_web_world),
                bases: Some(&self.base_world),
                commands: &mut commands,
            };
            self.creatures[i].update(dt, NO_CURSOR, NO_CURSOR, w, h, &mut env)
        };
        let mut pop = pop;
        pop.set(self.creatures[i].view());
        for command in commands {
            let target = match command {
                dc_creature::Command::BumpMood { target, .. } | dc_creature::Command::EnterPlay { target, .. } => {
                    target
                }
            };
            if let Some(t) = self.creatures.iter_mut().find(|c| c.id == target) {
                t.apply_command(&command, &pop);
                pop.set(t.view());
            }
        }
        outcome == UpdateOutcome::PlayerControlled
    }

    fn bury_the_dead(&mut self) {
        let fallen: Vec<Creature> = self.creatures.iter().filter(|c| c.dead).cloned().collect();
        self.mission_graveyard.extend(fallen);
        CreatureManager::bury_the_dead(self);
    }

    /// `manager.carcasses = [c for c in manager.carcasses[-4:] if not c.update(dt)]`
    fn update_carcasses(&mut self, dt: f64) {
        let keep_from = self.carcasses.len().saturating_sub(4);
        self.carcasses.drain(..keep_from);
        self.carcasses.retain_mut(|c| !c.update(dt, 0));
    }

    fn trade_blow(&mut self, attacker: SpiderId, target: SpiderId) {
        CreatureManager::trade_blow(self, attacker.0, target.0);
    }

    fn splash(&mut self, attacker: SpiderId, target: SpiderId, damage: f64) {
        CreatureManager::splash(self, attacker.0, target.0, damage);
    }

    /// `manager.fly_world.projectiles.append(AimedSilk(spider, angle, reach, ...))`
    fn shoot(&mut self, shooter: SpiderId, shot: SilkShot) {
        let Some(c) = self.creatures.iter().find(|c| c.id == shooter.0) else {
            return;
        };
        self.fly_world.launch_aimed_silk(c, shot.angle, shot.reach);
    }

    /// `for p in projectiles: p.update(dt)`, then drop the finished ones.
    fn update_projectiles(&mut self, dt: f64) {
        self.fly_world.update_projectiles(dt, &mut self.creatures);
    }

    fn fly_world(&self) -> &FlyWorld {
        &self.fly_world
    }

    fn fly_world_mut(&mut self) -> &mut FlyWorld {
        &mut self.fly_world
    }

    /// `fly.update(dt, [live spiders], manager.web_world)` with the fly's own
    /// RNG, which for swarm flies is the mission's.
    fn update_fly(&mut self, id: FlyId, dt: f64, rng: &mut PyRandom, layout: &ScreenLayout) {
        let live: Vec<Creature>;
        let spiders: &[Creature] = if self.creatures.iter().any(|c| c.dead) {
            live = self.creatures.iter().filter(|c| !c.dead).cloned().collect();
            &live
        } else {
            &self.creatures
        };
        let fear = FearField { spiders, grid: None, reach: Self::fear_reach(spiders) };
        let Some(fly) = self.fly_world.flies.iter_mut().find(|f| f.id == id) else {
            return;
        };
        let mut scratch = Vec::new();
        fly.update(dt, &fear, &mut scratch, Some(&mut self.web_world), rng, Some(&layout.playfield));
    }
}
