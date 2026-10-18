//! Port of `manager/hunting.py`.
use crate::CreatureManager;
use dc_core::PyRandom;
use dc_creature::{Creature, State, world::FlyWorldApi};
use dc_world::{CreatureId, FlyId};
impl CreatureManager {
    pub fn creature_can_hunt(&self, c: &Creature) -> bool {
        if c.player_control || c.airborne || c.desktop_fully_hidden || c.feed_cooldown > 0.0 || c.dead {
            return false;
        }
        if c.dragging || Some(c.id) == self.dragged_creature {
            return true;
        }
        if dc_creature::constants::HUNT_BUSY_STATES.contains(&c.state) {
            return false;
        }
        c.has_skill("chase") || c.has_skill("approach") || c.has_skill("jump")
    }
    pub fn hunt_sense_radius(c: &Creature, trapped: bool) -> f64 {
        let sense = crate::constants::clamp(
            crate::surfaces::personality_float(c, "reaction_radius", 360.0) * 0.9,
            240.0,
            900.0,
        );
        if trapped { sense * 1.5 + 160.0 } else { sense }
    }
    pub fn update_prey_targets(&mut self, dt: f64, rng: &mut PyRandom) {
        for c in &mut self.creatures {
            c.feed_cooldown = (c.feed_cooldown - dt).max(0.0);
            c.pounce_cooldown = (c.pounce_cooldown - dt).max(0.0);
            c.trap_shot_cooldown = (c.trap_shot_cooldown - dt).max(0.0);
            c.prey_recheck = (c.prey_recheck - dt).max(0.0);
        }
        let flies: Vec<_> =
            self.fly_world.flies.iter().filter(|f| f.alive() && !f.eaten() && !f.dragging).map(|f| f.id).collect();
        if !self.flies_enabled || flies.is_empty() {
            for c in &mut self.creatures {
                if let Some(p) = c.prey.take() {
                    if let Some(f) = self.fly_world.fly_mut(FlyId(p.id)) {
                        f.remove_hunter(CreatureId(c.id));
                    }
                }
            }
            return;
        }
        for i in 0..self.creatures.len() {
            let can = self.creature_can_hunt(&self.creatures[i]);
            let mut prey = self.creatures[i].prey.map(|p| FlyId(p.id));
            let id = CreatureId(self.creatures[i].id);
            if let Some(p) = prey {
                let keep = self.fly_world.fly(p).is_some_and(|f| {
                    can && flies.contains(&p)
                        && (f.x - self.creatures[i].kin.x).hypot(f.y - self.creatures[i].kin.y)
                            <= Self::hunt_sense_radius(&self.creatures[i], f.trapped()) * 1.6
                });
                if !keep {
                    if let Some(f) = self.fly_world.fly_mut(p) {
                        f.remove_hunter(id);
                    }
                    prey = None;
                    self.creatures[i].prey = None;
                }
            }
            if !can {
                continue;
            }
            if prey.is_some() && self.creatures[i].prey_recheck > 0.0 {
                self.creatures[i].prey = prey.and_then(|p| FlyWorldApi::fly(&self.fly_world, p.0));
                continue;
            }
            self.creatures[i].prey_recheck = rng.uniform(0.2, 0.4);
            let c = &self.creatures[i];
            let mut best = None;
            let mut best_score = -1e18;
            for fid in &flies {
                let f = self.fly_world.fly(*fid).unwrap();
                let sense = Self::hunt_sense_radius(c, f.trapped());
                let d = (c.kin.x - f.x).hypot(c.kin.y - f.y);
                if d > sense && Some(*fid) != prey {
                    continue;
                }
                let claimers = f.hunters.iter().filter(|h| **h != id).count();
                let score = sense - d + if f.trapped() { 500.0 } else { 0.0 } - claimers as f64 * 140.0
                    + if Some(*fid) == prey { 90.0 } else { 0.0 };
                if score > best_score {
                    best_score = score;
                    best = Some(*fid);
                }
            }
            if best != prey {
                if let Some(p) = prey {
                    if let Some(f) = self.fly_world.fly_mut(p) {
                        f.remove_hunter(id);
                    }
                }
                if let Some(p) = best {
                    self.fly_world.fly_mut(p).unwrap().add_hunter(id);
                }
            }
            self.creatures[i].prey = best.and_then(|p| FlyWorldApi::fly(&self.fly_world, p.0));
        }
    }
    pub fn creature_focus(&self, i: usize, mx: f64, my: f64) -> (f64, f64, bool) {
        let c = &self.creatures[i];
        if let Some(f) = c.prey.and_then(|p| self.fly_world.fly(FlyId(p.id))) {
            if f.alive() && !f.eaten() {
                return (f.x, f.y, true);
            }
        }
        if let Some(f) = c.foe.and_then(|id| self.creature(id)) {
            if !f.dead {
                return (f.kin.x, f.kin.y, false);
            }
        }
        (mx, my, false)
    }
    pub fn resolve_fly_catches(&mut self, rng: &mut PyRandom) {
        for fi in 0..self.fly_world.flies.len() {
            let f = &self.fly_world.flies[fi];
            if !f.alive() || f.eaten() || f.dragging {
                continue;
            }
            if f.trapped() && f.tether_from.is_some() {
                if let Some(c) = f.trapper.and_then(|id| self.creature(id.0)) {
                    if !c.dragging {
                        let p = (
                            c.kin.x + c.kin.heading.cos() * c.kin.size * 0.6,
                            c.kin.y + c.kin.heading.sin() * c.kin.size * 0.6,
                        );
                        self.fly_world.flies[fi].tether_from = Some(p);
                    }
                }
            }
            let f = &self.fly_world.flies[fi];
            let mut best = None;
            let mut best_d = 1e18;
            for (i, c) in self.creatures.iter().enumerate() {
                if c.dragging || c.state == State::Feed || c.desktop_fully_hidden {
                    continue;
                }
                let d = (c.kin.x - f.x).hypot(c.kin.y - f.y);
                if d <= c.kin.size * 1.5 + f.size + 4.0 && d < best_d {
                    best = Some(i);
                    best_d = d;
                }
            }
            if let Some(i) = best {
                let (x, y, size) = (f.x, f.y, f.size);
                self.fly_world.add_remains((x, y), size / 9.0);
                self.fly_world.flies[fi].begin_eaten();
                let id = self.creatures[i].id;
                self.award_feed_xp(id, crate::constants::FEED_XP_REWARD, "fly");
                let c = &self.creatures[i];
                if c.job_id != "hunter" {
                    self.base_world
                        .credit_team_food(Some(&c.progression.team_id), dc_world::jobs::FLY_CATCH_RESOURCE_AMOUNT);
                }
                let hunters = self.fly_world.flies[fi].hunters.clone();
                for c in &mut self.creatures {
                    if hunters.contains(&CreatureId(c.id)) {
                        c.prey = None;
                        c.hunting_prey = false;
                    }
                }
                self.fly_world.flies[fi].hunters.clear();
                self.creatures[i].enter_feed(x, y);
                self.creatures[i].feed_cooldown = rng.uniform(0.6, 1.2);
            }
        }
    }
}
