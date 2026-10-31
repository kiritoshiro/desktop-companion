//! Port of the preset/roster methods in `manager/core.py`.
use crate::{CreatureManager, constants::*};
use dc_content::{
    personality_profiles::{COMPACT_TEMPERAMENT_IDS, PersonalityRef},
    skills::*,
    util::{py_float, py_int, py_truthy},
};
use dc_core::PyRandom;
use dc_creature::CreatureConfig;
use serde_json::{Value, json};
use std::path::Path;
#[derive(Clone, Debug)]
pub struct Traits {
    pub model: String,
    pub personality: String,
    pub skills: Vec<String>,
    pub colors: Option<Value>,
    pub team: Option<String>,
    pub job: String,
}
fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}
impl CreatureManager {
    pub fn random_model_id(&self, rng: &mut PyRandom) -> Option<String> {
        let mut pool: Vec<_> = self.models.keys().filter(|k| !dc_content::enemy_kinds::is_enemy_model(k)).collect();
        if pool.is_empty() {
            pool = self.models.keys().collect();
        }
        if pool.is_empty() { None } else { Some((*rng.choice(&pool)).into()) }
    }
    pub fn random_personality_id(&self, rng: &mut PyRandom) -> Option<String> {
        let mut pool: Vec<_> =
            COMPACT_TEMPERAMENT_IDS.into_iter().filter(|k| self.personalities.get(k).is_some()).collect();
        if pool.is_empty() {
            pool = self.personalities.keys().collect();
        }
        if pool.is_empty() { None } else { Some((*rng.choice(&pool)).into()) }
    }
    pub fn valid_personality_for_model(
        &self,
        model: &Value,
        preferred: Option<&str>,
        rng: &mut PyRandom,
    ) -> Option<Value> {
        if let Some(p) = preferred.and_then(|p| self.personalities.get(p)) {
            return Some(p.clone());
        }
        if let Some(p) =
            model.get("default_personality").and_then(Value::as_str).and_then(|id| self.personalities.get(id))
        {
            return Some(p.clone());
        }
        let values: Vec<_> = self.personalities.values().collect();
        if values.is_empty() { None } else { Some((*rng.choice(&values)).clone()) }
    }
    pub fn load_preset(&mut self, path: &Path, rng: &mut PyRandom) -> Result<(), dc_content::ContentError> {
        let data = dc_content::preset_io::load_preset_value(path)?;
        self.progression_namespace = dc_state::runtime_state::normalize_namespace(
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("default"),
        );
        self.load_preset_data(&data, rng);
        Ok(())
    }
    fn apply_settings(&mut self, data: &Value, live: bool) {
        let settings = &data["settings"];
        if !settings.is_object() && !settings.is_null() {
            return;
        }
        if let Some(v) = settings.get("size_scale") {
            if let Some(n) = py_float(v) {
                self.set_size_scale(n);
            } else if !live {
                self.size_scale = 1.0;
                self.warnings.push("Preset settings.size_scale was invalid; using normal size.".into());
            }
        }
        if let Some(v) = settings.get("interferable") {
            self.set_interferable(py_truthy(v));
        }
        if let Some(v) = settings.get("mood_mode") {
            self.set_mood_mode(v.as_str().unwrap_or("auto"));
        }
        if let Some(v) = settings.get("social_play") {
            self.set_social_play(py_truthy(v));
        }
        if let Some(v) = settings.get("conflict") {
            self.set_conflict_enabled(py_truthy(v));
        }
        if let Some(v) = settings.get("gait_style")
            && (live || !v.is_null())
        {
            self.set_gait_style(v.as_str().unwrap_or("classic"));
        }
        for (key, default) in dc_content::preset::LABEL_SWITCH_DEFAULTS {
            if live && settings.get(key).is_none() {
                continue;
            }
            let v = settings.get(key).map(py_truthy).unwrap_or(default);
            match key {
                "always_show_names" => {
                    self.set_always_show_names(v);
                }
                "always_show_levels" => {
                    self.set_always_show_levels(v);
                }
                "always_show_health" => {
                    self.set_always_show_health(v);
                }
                "always_show_xp" => {
                    self.set_always_show_xp(v);
                }
                _ => {
                    self.set_always_show_stamina(v);
                }
            }
        }
        if live {
            if let Some(v) = settings.get("allow_mouse_capture") {
                self.set_allow_mouse_capture(py_truthy(v));
            }
            if let Some(v) = settings.get("desktop_icons_enabled") {
                self.set_desktop_icons_enabled(py_truthy(v));
            }
        }
        if !live || settings.get("team_relations").is_some() {
            self.set_team_stances(settings.get("team_relations"));
        }
        let used: Vec<_> = data["slots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.get("team_id").or_else(|| s.get("team")).and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        let fallback = self.team_payload();
        self.set_team_profiles(settings.get("teams").or(if live { Some(&fallback) } else { None }), &used);
        self.apply_fly_settings(settings);
    }
    fn slot_traits(&mut self, slot: &Value, rng: &mut PyRandom) -> Option<(Traits, i64)> {
        let mid = slot["model"].as_str().unwrap_or("");
        let mid = if mid == RANDOM_MODEL_ID { self.random_model_id(rng)? } else { mid.into() };
        let Some(model) = self.models.get(&mid).cloned() else {
            self.warnings.push(format!("Preset slot references missing model: {mid}"));
            return None;
        };
        let pid = slot
            .get("personality")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| model["default_personality"].as_str())
            .unwrap_or("");
        let pid =
            if pid == RANDOM_PERSONALITY_ID { self.random_personality_id(rng).unwrap_or_default() } else { pid.into() };
        let personality =
            self.personalities.get(&pid).cloned().or_else(|| self.valid_personality_for_model(&model, None, rng))?;
        let pd = dc_creature::creature::typed_personality(&personality);
        let job = dc_content::jobs::normalize_job_id(slot["job"].as_str()).to_string();
        let extra = dc_world::jobs::job_ability_ids(Some(&job));
        let skills = if slot.get("abilities").is_some_and(|v| !v.is_null()) {
            let mut a = strings(slot.get("abilities"));
            a.extend(extra.iter().map(|s| s.to_string()));
            skills_with_selected_abilities(Some(PersonalityRef::Def(&pd)), Some(&a))
        } else if slot.get("skills").is_none_or(Value::is_null) {
            skills_with_default_abilities(Some(PersonalityRef::Def(&pd)), Some(extra))
        } else {
            let mut s = strings(slot.get("skills"));
            let bad = unknown_skill_ids(Some(&s));
            if !bad.is_empty() {
                self.warnings.push(format!("Preset slot for {mid} ignored unknown skill(s): {}", bad.join(", ")));
            }
            s.extend(extra.iter().map(|s| s.to_string()));
            normalize_skill_ids(Some(&s))
        };
        let count = if slot.get("count_random").is_some_and(py_truthy) {
            rng.randint(1, 10)
        } else {
            slot.get("count").and_then(py_int).unwrap_or(1).clamp(1, 50)
        };
        Some((
            Traits {
                model: mid,
                personality: personality["id"].as_str().unwrap_or(&pid).into(),
                skills,
                colors: slot.get("colors").filter(|v| !v.is_null()).cloned(),
                team: Some(
                    slot.get("team_id")
                        .or_else(|| slot.get("team"))
                        .and_then(Value::as_str)
                        .unwrap_or("neutral")
                        .into(),
                ),
                job,
            },
            count,
        ))
    }
    pub fn load_preset_data(&mut self, data: &Value, rng: &mut PyRandom) {
        self.preset_data = data.clone();
        self.creatures.clear();
        self.slot_ids.clear();
        self.web_world.clear();
        self.mouse_web_world.clear();
        self.fly_world.clear_hunters();
        self.cages.clear();
        self.cage_drag = None;
        self.dragged_creature = None;
        self.dragged_base = None;
        self.apply_settings(data, false);
        for (si, slot) in data["slots"].as_array().into_iter().flatten().enumerate() {
            let Some((t, count)) = self.slot_traits(slot, rng) else {
                continue;
            };
            let fallback = format!("slot-{si}");
            let slot_id = slot
                .get("slot_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&fallback);
            let ids = self.slot_member_ids(slot_id, count as usize);
            for id in ids {
                let mut cfg = self.config_for_traits(&t);
                cfg.index = self.creatures.len() as i64;
                cfg.progression_id = Some(id);
                let mut c = self.create_creature(cfg, None, t.team.as_deref(), slot_id);
                c.kin.x += rng.uniform(-80.0, 80.0);
                c.kin.y += rng.uniform(-80.0, 80.0);
                c.resize_screen(self.screen_w, self.screen_h);
                self.creatures.push(c);
            }
        }
        if self.creatures.is_empty() {
            self.fallback_creature(false, rng);
        }
        self.refresh_neighbor_links();
        self.refresh_render_order();
    }
    fn config_for_traits(&self, t: &Traits) -> CreatureConfig {
        let mut cfg = CreatureConfig::new(
            self.models.get(&t.model).unwrap().clone(),
            self.personalities.get(&t.personality).unwrap().clone(),
            self.screen_w,
            self.screen_h,
        );
        cfg.skills = Some(t.skills.clone());
        cfg.color_overrides = t.colors.clone();
        cfg.job_id = t.job.clone();
        cfg
    }
    fn fallback_creature(&mut self, random: bool, rng: &mut PyRandom) {
        if self.models.is_empty() || self.personalities.is_empty() {
            return;
        }
        let model = if random {
            self.random_model_id(rng).and_then(|id| self.models.get(&id)).cloned()
        } else {
            self.models
                .iter()
                .find(|(id, _)| !dc_content::enemy_kinds::is_enemy_model(id))
                .map(|(_, m)| m)
                .or_else(|| self.models.values().next())
                .cloned()
        }
        .unwrap();
        let personality = if random {
            self.valid_personality_for_model(&model, None, rng)
        } else {
            model["default_personality"]
                .as_str()
                .and_then(|id| self.personalities.get(id))
                .or_else(|| self.personalities.values().next())
                .cloned()
        };
        if let Some(p) = personality {
            let mut cfg = CreatureConfig::new(model, p, self.screen_w, self.screen_h);
            cfg.skills = Some(default_skill_ids().into_iter().map(str::to_string).collect());
            cfg.progression_id = Some(self.slot_member_ids("fallback", 1)[0].clone());
            let c = self.create_creature(cfg, None, None, "fallback");
            self.creatures.push(c);
        }
    }
    pub fn replace_with_traits(&mut self, traits: &[Traits], keep_positions: bool, rng: &mut PyRandom) {
        let old = std::mem::take(&mut self.creatures);
        let slots = self.slot_ids.clone();
        self.slot_ids.clear();
        self.web_world.clear();
        self.mouse_web_world.clear();
        self.fly_world.clear_hunters();
        self.dragged_creature = None;
        for (index, t) in traits.iter().enumerate() {
            let Some(model) = self.models.get(&t.model).cloned() else {
                continue;
            };
            let Some(personality) = self
                .personalities
                .get(&t.personality)
                .cloned()
                .or_else(|| self.valid_personality_for_model(&model, None, rng))
            else {
                continue;
            };
            let previous = if keep_positions { old.get(index) } else { None };
            let mut cfg = CreatureConfig::new(model, personality, self.screen_w, self.screen_h);
            cfg.index = index as i64;
            cfg.skills = Some(normalize_skill_ids(Some(&t.skills)));
            cfg.color_overrides = t.colors.clone();
            cfg.job_id = t.job.clone();
            cfg.progression_id = previous.map(|c| c.progression_id.clone());
            cfg.progression_state = previous.map(|c| c.progression.to_value());
            let slot = previous.and_then(|c| slots.get(&c.id)).map(String::as_str).unwrap_or("");
            let mut c = self.create_creature(cfg, previous.map(|c| (c.kin.x, c.kin.y)), t.team.as_deref(), slot);
            if let Some(p) = previous {
                c.set_name(&p.name);
            }
            self.creatures.push(c);
        }
        if self.creatures.is_empty() {
            self.fallback_creature(true, rng);
        }
        if keep_positions && !self.cages.is_empty() {
            let ids: Vec<_> = self.creatures.iter().map(|c| c.id).collect();
            for id in ids {
                self.settle_creature_membership(id);
            }
        }
        self.refresh_neighbor_links();
        self.refresh_render_order();
    }
    pub fn reload_from_preset_data(&mut self, data: &Value, rng: &mut PyRandom) -> String {
        if !data.is_object() {
            return "Ignored malformed preset data.".into();
        }
        self.apply_settings(data, true);
        let mut traits = vec![];
        for slot in data["slots"].as_array().into_iter().flatten() {
            if let Some((t, count)) = self.slot_traits(slot, rng) {
                traits.extend(std::iter::repeat_n(t, count as usize));
            }
        }
        self.replace_with_traits(&traits, true, rng);
        format!("Applied live preset to {} spider(s).", self.creatures.len())
    }
    fn existing_traits(&self) -> Vec<Traits> {
        self.creatures
            .iter()
            .map(|c| Traits {
                model: c.model["id"].as_str().unwrap_or("").into(),
                personality: c.personality["id"].as_str().unwrap_or("").into(),
                skills: c.skill_ids(),
                colors: Some(serde_json::to_value(&c.color_overrides).expect("colors")),
                team: Some(c.progression.team_id.clone()),
                job: c.job_id.clone(),
            })
            .collect()
    }
    pub fn randomize_models(&mut self, rng: &mut PyRandom) -> String {
        if self.creatures.is_empty() || self.models.is_empty() {
            return "No models available.".into();
        }
        let mut traits = self.existing_traits();
        for t in &mut traits {
            t.model = self.random_model_id(rng).unwrap();
            let p = self
                .valid_personality_for_model(self.models.get(&t.model).unwrap(), Some(&t.personality), rng)
                .unwrap();
            t.personality = p["id"].as_str().unwrap().into();
        }
        self.replace_with_traits(&traits, true, rng);
        format!("Randomized model for {} spider(s).", self.creatures.len())
    }
    pub fn randomize_personalities(&mut self, rng: &mut PyRandom) -> String {
        if self.creatures.is_empty() || self.personalities.is_empty() {
            return "No personalities available.".into();
        }
        let mut traits = self.existing_traits();
        for t in &mut traits {
            t.personality = self.random_personality_id(rng).unwrap();
        }
        self.replace_with_traits(&traits, true, rng);
        format!("Randomized personality for {} spider(s).", self.creatures.len())
    }
    pub fn randomize_count(&mut self, low: i64, high: i64, rng: &mut PyRandom) -> String {
        if self.models.is_empty() || self.personalities.is_empty() {
            return "No models/personalities available.".into();
        }
        let count = rng.randint(low, high);
        let mut old = self.existing_traits();
        if old.is_empty() {
            let mid = self.random_model_id(rng).unwrap();
            let p = self.valid_personality_for_model(self.models.get(&mid).unwrap(), None, rng).unwrap();
            old.push(Traits {
                model: mid,
                personality: p["id"].as_str().unwrap().into(),
                skills: default_skill_ids().into_iter().map(str::to_string).collect(),
                colors: None,
                team: Some("neutral".into()),
                job: "none".into(),
            });
        }
        let traits = (0..count).map(|i| old[i as usize % old.len()].clone()).collect::<Vec<_>>();
        self.replace_with_traits(&traits, true, rng);
        format!("Randomized count: {} spider(s).", self.creatures.len())
    }
    pub fn randomize_everything(&mut self, rng: &mut PyRandom) -> String {
        if self.models.is_empty() || self.personalities.is_empty() {
            return "No models/personalities available.".into();
        }
        let count = rng.randint(1, 10);
        let traits = (0..count)
            .map(|_| Traits {
                model: self.random_model_id(rng).unwrap(),
                personality: self.random_personality_id(rng).unwrap(),
                skills: default_skill_ids().into_iter().map(str::to_string).collect(),
                colors: None,
                team: Some("neutral".into()),
                job: "none".into(),
            })
            .collect::<Vec<_>>();
        self.replace_with_traits(&traits, false, rng);
        format!("Randomized everything: {} spider(s).", self.creatures.len())
    }
    pub fn session_snapshot(&self) -> Value {
        json!({"mood_mode":self.mood_mode,"always_show_names":self.always_show_names,"always_show_levels":self.always_show_levels,"always_show_health":self.always_show_health,"always_show_xp":self.always_show_xp,"always_show_stamina":self.always_show_stamina,"size_scale":self.size_scale,"social_play":self.social_play,"flies_enabled":self.flies_enabled,"interferable":self.interferable,"creatures":self.creatures.iter().map(|c|json!({"id":c.progression_id,"name":c.name,"team_id":c.progression.team_id})).collect::<Vec<_>>()})
    }
}
