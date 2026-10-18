//! Port of `manager/colony.py`.
use crate::CreatureManager;
use dc_core::PyRandom;
use dc_world::jobs::BaseSite;
pub const TEAM_POPULATION_CAP: usize = 5;
pub const RAISE_FOOD_COST: f64 = 32.0;
pub const RAISE_SECONDS: f64 = 8.0;
pub const RAISE_MIN_COMPLETION: f64 = 0.35;
pub const RAISED_MODEL_ID: &str = "tarantula";
impl CreatureManager {
    pub fn living_team_members(&self, team: &str) -> usize {
        self.creatures.iter().filter(|c| !c.dead && c.progression.team_id == team).count()
    }
    pub fn update_colony_growth(&mut self, dt: f64, rng: &mut PyRandom) {
        for i in 0..self.base_world.bases.len() {
            let s = &self.base_world.bases[i];
            if s.team_id == "neutral" {
                continue;
            }
            if self.living_team_members(&s.team_id) >= TEAM_POPULATION_CAP
                || s.completion() < RAISE_MIN_COMPLETION
                || s.larder < RAISE_FOOD_COST
            {
                self.raise_timers.remove(&s.id);
                continue;
            }
            let elapsed = self.raise_timers.get(&s.id).copied().unwrap_or(0.0) + dt.max(0.0);
            if elapsed < RAISE_SECONDS {
                self.raise_timers.insert(s.id.clone(), elapsed);
                continue;
            }
            self.raise_timers.remove(&s.id);
            self.base_world.bases[i].larder -= RAISE_FOOD_COST;
            let site = self.base_world.bases[i].clone();
            self.raise_spider_at(&site, rng);
        }
    }
    pub fn birth_point(&self, s: &BaseSite, rng: &mut PyRandom) -> (f64, f64) {
        let a = rng.uniform(0.0, std::f64::consts::TAU);
        let r = s.radius() * rng.uniform(0.20, 0.55);
        (
            20.0f64.max((self.screen_w - 20.0).min(s.x + a.cos() * r)),
            20.0f64.max((self.screen_h - 20.0).min(s.y + a.sin() * r * 0.7)),
        )
    }
    pub fn raise_spider_at(&mut self, s: &BaseSite, rng: &mut PyRandom) {
        let Some(model) = self.models.get(RAISED_MODEL_ID).cloned() else {
            return;
        };
        let Some(personality) = self.valid_personality_for_model(&model, None, rng) else {
            return;
        };
        let pos = self.birth_point(s, rng);
        let colors = dc_content::palettes::random_palette(rng);
        let mut cfg = dc_creature::CreatureConfig::new(model, personality, self.screen_w, self.screen_h);
        cfg.index = self.creatures.len() as i64;
        cfg.color_overrides = Some(serde_json::to_value(colors).expect("palette JSON"));
        let c = self.create_creature(cfg, Some(pos), Some(&s.team_id), "");
        self.creatures.push(c);
        self.refresh_neighbor_links();
        self.save_or_warn();
    }
}
