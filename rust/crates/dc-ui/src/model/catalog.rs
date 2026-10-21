//! What the settings window offers: models, temperaments, abilities, jobs,
//! palettes. Mirrors content/discovery, content/skills, world/jobs,
//! content/palettes and content/preset_io as config_ui.py used them.
//!
//! [`crate::bridge::settings_catalog`] builds [`SettingsCatalog`] from
//! `dc_content` discovery, skills, palettes and body plans. The logic methods
//! below port the Python helpers config_ui called (`normalize_skill_ids`,
//! `normalize_ability_ids`, `compact_ability_summary`,
//! `skills_with_default_abilities`, ...) over that catalogue.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Rgb;

/// One creature model (`models/*/model.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    /// `bug`, `segmented`, `jumper`, `tarantula`, or an enemy plan.
    pub body_plan: String,
    pub colors: BTreeMap<String, Rgb>,
    /// Adventure enemies are not Companion skins (`is_enemy_model`).
    pub enemy: bool,
    /// When the model folder was added (`st_ctime`), newest first in the list.
    pub added: f64,
}

/// One temperament (`personalities/*.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PersonalityInfo {
    pub id: String,
    pub display_name: String,
    /// False for a legacy personality, which is labelled "Legacy: ...".
    pub canonical: bool,
    pub description: String,
    /// `temperament` / `traits`: energy, curiosity, boldness, sociability,
    /// patience, caution out of 10.
    pub temperament: Vec<(String, f64)>,
    /// `default_skills_for_personality(personality)`.
    pub default_skills: Vec<String>,
}

/// One skill of the registry (`content/skills.SKILLS`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SkillInfo {
    pub id: String,
    pub display_name: String,
    /// `"Ability"` for the five a slot can pick; everything else is a
    /// behaviour chosen by the temperament.
    pub category: String,
    pub description: String,
    pub short_label: String,
}

/// One job (`world/jobs.JOB_DEFINITIONS`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct JobInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub ability_ids: Vec<String>,
}

/// A named palette (`content/palettes.NAMED_PALETTES`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PaletteInfo {
    pub id: String,
    pub name: String,
    pub colors: BTreeMap<String, Rgb>,
}

/// A preset in the library (`discover_presets`): its name and file.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PresetEntry {
    pub label: String,
    pub path: String,
}

/// Everything the settings window offers.
#[derive(Clone, Debug, Default)]
pub struct SettingsCatalog {
    pub models: BTreeMap<String, ModelInfo>,
    pub personalities: BTreeMap<String, PersonalityInfo>,
    /// `COMPACT_TEMPERAMENT_IDS`: the fresh choices, in menu order.
    pub compact_temperaments: Vec<String>,
    /// The registry, in its stable order.
    pub skills: Vec<SkillInfo>,
    /// `COMMON_SKILL_IDS`.
    pub common_skill_ids: Vec<String>,
    pub jobs: Vec<JobInfo>,
    pub palettes: Vec<PaletteInfo>,
    /// `BODY_PLAN_IDS`.
    pub body_plans: Vec<String>,
    /// `ENEMY_BODY_PLANS`: Adventure-only plans not offered as a category.
    pub enemy_body_plans: Vec<String>,
    /// `LABEL_SWITCH_DEFAULTS`.
    pub label_switch_defaults: BTreeMap<String, bool>,
    /// Discovery warnings shown in the status bar.
    pub warnings: Vec<String>,
}

/// `palettes.PALETTE_KEYS`.
pub const PALETTE_KEYS: [&str; 7] = ["body", "legs", "leg_band", "leg_dark", "leg_tip", "highlight", "eyes"];

fn hsv(hue: f64, saturation: f64, value: f64) -> Rgb {
    let h = hue.rem_euclid(1.0);
    let s = saturation.clamp(0.0, 1.0);
    let v = value.clamp(0.0, 1.0);
    // colorsys.hsv_to_rgb
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let c = |x: f64| dc_core::math::py_round(x * 255.0) as u8;
    [c(r), c(g), c(b)]
}

/// `palette_from_hue`: a whole spider's palette from one hue.
pub fn palette_from_hue(hue: f64, saturation: f64, value: f64) -> BTreeMap<String, Rgb> {
    let mut colors = BTreeMap::new();
    colors.insert("body".into(), hsv(hue, saturation, value * 0.30));
    colors.insert("legs".into(), hsv(hue, saturation * 0.94, value * 0.44));
    colors.insert("leg_band".into(), hsv(hue + 0.030, saturation * 0.86, value * 0.86));
    colors.insert("leg_dark".into(), hsv(hue, saturation, value * 0.21));
    colors.insert("leg_tip".into(), hsv(hue, saturation * 0.90, value * 0.17));
    colors.insert("highlight".into(), hsv(hue + 0.018, saturation * 0.64, value * 0.92));
    colors.insert("eyes".into(), hsv(hue + 0.090, 0.64, 0.97));
    colors
}

/// `random_palette`: anywhere on the hue circle, in ranges that still look
/// like a spider. Draw order as in Python: random, uniform, uniform.
pub fn random_palette(rng: &mut dc_core::PyRandom) -> BTreeMap<String, Rgb> {
    let hue = rng.random();
    let saturation = rng.uniform(0.34, 0.78);
    let value = rng.uniform(0.62, 0.94);
    palette_from_hue(hue, saturation, value)
}

/// The twelve named palettes (`NAMED_PALETTES`).
pub fn named_palettes() -> Vec<PaletteInfo> {
    [
        ("cocoa", "Cocoa", 0.055, 0.62, 0.74),
        ("copper", "Copper", 0.075, 0.78, 0.88),
        ("ember", "Ember", 0.015, 0.74, 0.82),
        ("amber", "Amber", 0.115, 0.76, 0.92),
        ("moss", "Moss", 0.270, 0.52, 0.70),
        ("jade", "Jade", 0.430, 0.55, 0.76),
        ("frost", "Frost", 0.545, 0.44, 0.88),
        ("ink", "Ink", 0.620, 0.58, 0.60),
        ("plum", "Plum", 0.775, 0.54, 0.74),
        ("rose", "Rose", 0.925, 0.50, 0.90),
        ("ash", "Ash", 0.600, 0.08, 0.72),
        ("bone", "Bone", 0.100, 0.16, 0.95),
    ]
    .into_iter()
    .map(|(id, name, h, s, v)| PaletteInfo { id: id.into(), name: name.into(), colors: palette_from_hue(h, s, v) })
    .collect()
}

impl SettingsCatalog {
    /// `DEFAULT_SKILL_IDS`, in registry order.
    pub fn default_skill_ids(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.id.clone()).collect()
    }

    /// `ABILITY_SKILL_IDS`: the capabilities a slot can choose.
    pub fn ability_skill_ids(&self) -> Vec<String> {
        self.skills.iter().filter(|s| s.category == "Ability").map(|s| s.id.clone()).collect()
    }

    /// The ability skills themselves, in order (the abilities dialog).
    pub fn ability_skills(&self) -> Vec<&SkillInfo> {
        self.skills.iter().filter(|s| s.category == "Ability").collect()
    }

    /// `normalize_skill_ids`: known ids in registry order; `None` is the full set.
    pub fn normalize_skill_ids(&self, value: Option<&[String]>) -> Vec<String> {
        let Some(value) = value else { return self.default_skill_ids() };
        let wanted: Vec<String> = value.iter().map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()).collect();
        self.skills.iter().filter(|s| wanted.contains(&s.id)).map(|s| s.id.clone()).collect()
    }

    /// `normalize_ability_ids`: only capabilities, in registry order; `None`
    /// is every ability.
    pub fn normalize_ability_ids(&self, value: Option<&[String]>) -> Vec<String> {
        let Some(value) = value else { return self.ability_skill_ids() };
        let wanted: Vec<String> = value.iter().map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()).collect();
        self.ability_skill_ids().into_iter().filter(|id| wanted.contains(id)).collect()
    }

    fn short_label<'a>(&'a self, id: &'a str) -> &'a str {
        self.skills
            .iter()
            .find(|s| s.id == id)
            .map(|s| if s.short_label.is_empty() { s.id.as_str() } else { s.short_label.as_str() })
            .unwrap_or(id)
    }

    /// `compact_ability_summary`.
    pub fn compact_ability_summary(&self, skill_ids: &[String]) -> String {
        let ids = self.normalize_ability_ids(Some(skill_ids));
        let all = self.ability_skill_ids();
        if !all.is_empty() && ids.len() == all.len() {
            return "All abilities".into();
        }
        if ids.is_empty() {
            return "No optional abilities".into();
        }
        let labels: Vec<&str> = ids.iter().map(|id| self.short_label(id)).collect();
        if labels.len() <= 3 {
            return labels.join(", ");
        }
        format!("{} abilities: {}\u{2026}", labels.len(), labels[..3].join(", "))
    }

    /// `_short_ability_label`: "2 of 5" -- only what the dialog can select.
    pub fn short_ability_label(&self, skill_ids: &[String]) -> String {
        let selectable = self.normalize_ability_ids(Some(skill_ids));
        format!("{} of {}", selectable.len(), self.ability_skill_ids().len())
    }

    /// `default_skills_for_personality`, or the full set for an unknown one.
    fn personality_default_skills(&self, personality_id: Option<&str>) -> Option<Vec<String>> {
        personality_id.and_then(|id| self.personalities.get(id)).map(|p| p.default_skills.clone())
    }

    /// `skills_with_selected_abilities`: the temperament's behaviours plus an
    /// explicit ability choice.
    pub fn skills_with_selected_abilities(
        &self,
        personality_id: Option<&str>,
        abilities: Option<&[String]>,
    ) -> Vec<String> {
        let base = self.personality_default_skills(personality_id).unwrap_or_else(|| self.default_skill_ids());
        let ability_ids = self.ability_skill_ids();
        let mut out: Vec<String> = base.into_iter().filter(|id| !ability_ids.contains(id)).collect();
        out.extend(self.normalize_ability_ids(abilities));
        out
    }

    /// `skills_with_default_abilities`: the temperament's own abilities plus `extra`.
    pub fn skills_with_default_abilities(&self, personality_id: Option<&str>, extra: &[String]) -> Vec<String> {
        let base = self.personality_default_skills(personality_id).unwrap_or_else(|| self.default_skill_ids());
        let ability_ids = self.ability_skill_ids();
        let mut wanted: Vec<String> = base.into_iter().filter(|id| ability_ids.contains(id)).collect();
        wanted.extend(extra.iter().map(|e| e.trim().to_lowercase()));
        self.skills_with_selected_abilities(personality_id, Some(&wanted))
    }

    /// `normalize_job_id`.
    pub fn normalize_job_id(&self, value: &str) -> String {
        let id = value.trim().to_lowercase();
        let id = match id.as_str() {
            "" | "default" => "none".to_owned(),
            "weaver" => "webber".to_owned(),
            "web_builder" => "builder".to_owned(),
            _ => id,
        };
        if self.jobs.iter().any(|j| j.id == id) { id } else { "none".into() }
    }

    /// `job_ability_ids`.
    pub fn job_ability_ids(&self, job_id: &str) -> Vec<String> {
        let id = self.normalize_job_id(job_id);
        self.jobs.iter().find(|j| j.id == id).map(|j| j.ability_ids.clone()).unwrap_or_default()
    }

    /// `ConfigWindow._default_skills_for`: a real temperament's common-plus-
    /// specialty default, or the common set for Random/unknown, plus the job's
    /// abilities.
    pub fn default_skills_for(&self, personality_id: Option<&str>, job_id: &str) -> Vec<String> {
        let job = self.job_ability_ids(job_id);
        match personality_id.and_then(|id| self.personalities.get(id)) {
            None => {
                let mut ids = self.common_skill_ids.clone();
                ids.extend(job);
                self.normalize_skill_ids(Some(&ids))
            }
            Some(p) => self.skills_with_default_abilities(Some(&p.id), &job),
        }
    }

    /// `selectable_personality_ids`: the compact menu plus a legacy id already in use.
    pub fn selectable_personality_ids(&self, include_id: Option<&str>) -> Vec<String> {
        let mut values = self.compact_temperaments.clone();
        if let Some(legacy) = include_id.map(|s| s.trim().to_lowercase())
            && !legacy.is_empty()
            && !values.contains(&legacy)
            && (self.personalities.is_empty() || self.personalities.contains_key(&legacy))
        {
            values.push(legacy);
        }
        values
    }

    /// `_plan_of_model`: the body plan a model is built on.
    pub fn plan_of_model(&self, model_id: &str) -> Option<String> {
        let model = self.models.get(model_id)?;
        let plan = model.body_plan.trim().to_lowercase();
        self.body_plans.contains(&plan).then_some(plan)
    }

    /// The Companion categories: every body plan but the enemy ones.
    pub fn categories(&self) -> Vec<String> {
        self.body_plans.iter().filter(|p| !self.enemy_body_plans.contains(p)).cloned().collect()
    }

    /// Skins for a category (or every non-enemy skin), newest first then by name.
    pub fn skins_for(&self, category: Option<&str>) -> Vec<&ModelInfo> {
        let mut models: Vec<&ModelInfo> =
            self.models.values().filter(|m| !m.enemy).filter(|m| category.is_none_or(|c| m.body_plan == c)).collect();
        models.sort_by(|a, b| {
            b.added
                .partial_cmp(&a.added)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
        });
        models
    }

    /// `_model_color_defaults`: a model's palette; for Random, the first value
    /// of each key across every model.
    pub fn model_color_defaults(&self, model_id: Option<&str>, random_id: &str) -> BTreeMap<String, Rgb> {
        let mut defaults = BTreeMap::new();
        let models: Vec<&ModelInfo> = match model_id {
            Some(id) if id == random_id => self.models.values().collect(),
            Some(id) => self.models.get(id).into_iter().collect(),
            None => Vec::new(),
        };
        for model in models {
            for (key, value) in &model.colors {
                defaults.entry(key.clone()).or_insert(*value);
            }
        }
        defaults
    }

    /// The named palette by id.
    pub fn palette(&self, id: &str) -> Option<&PaletteInfo> {
        self.palettes.iter().find(|p| p.id == id)
    }

    /// `LABEL_SWITCH_DEFAULTS[key]`.
    pub fn label_default(&self, key: &str) -> bool {
        self.label_switch_defaults.get(key).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_match_the_python_values() {
        let palettes = named_palettes();
        let cocoa = &palettes[0];
        assert_eq!(cocoa.colors["body"], [57, 33, 22]);
        assert_eq!(cocoa.colors["eyes"], [247, 227, 89]);
        let bone = palettes.iter().find(|p| p.id == "bone").unwrap();
        assert_eq!(bone.colors["leg_band"], [208, 202, 180]);
        let jade = palettes.iter().find(|p| p.id == "jade").unwrap();
        assert_eq!(jade.colors["highlight"], [116, 178, 159]);
    }

    #[test]
    fn skill_helpers_follow_the_registry() {
        let c = crate::sample::settings_catalog();
        assert_eq!(c.ability_skill_ids().len(), 5);
        assert_eq!(c.short_ability_label(&c.default_skill_ids()), "5 of 5");
        let picked = vec!["shoot_web".to_owned(), "web_walk".to_owned(), "approach".to_owned()];
        assert_eq!(c.short_ability_label(&picked), "2 of 5");
        assert_eq!(c.compact_ability_summary(&picked), "web walk, web trap");
        assert_eq!(c.compact_ability_summary(&[]), "No optional abilities");
        assert_eq!(c.compact_ability_summary(&c.ability_skill_ids()), "All abilities");
        let guard = c.default_skills_for(Some("balanced"), "guard");
        assert!(guard.contains(&"shoot_web".to_owned()));
        assert!(guard.contains(&"web_walk".to_owned()));
        assert!(!guard.contains(&"weave_web".to_owned()));
        let random = c.default_skills_for(None, "webber");
        assert!(random.contains(&"weave_web".to_owned()));
        assert_eq!(c.normalize_job_id("weaver"), "webber");
        assert_eq!(c.normalize_job_id("nonsense"), "none");
    }

    #[test]
    fn a_legacy_temperament_stays_selectable_where_it_is_used() {
        let mut c = crate::sample::settings_catalog();
        c.personalities.insert(
            "grumpy".into(),
            PersonalityInfo { id: "grumpy".into(), display_name: "Grumpy".into(), ..Default::default() },
        );
        assert!(!c.selectable_personality_ids(None).contains(&"grumpy".to_owned()));
        assert!(c.selectable_personality_ids(Some("grumpy")).contains(&"grumpy".to_owned()));
    }
}
