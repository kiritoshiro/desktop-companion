//! Port of `manager/naming.py` and scene settings in `manager/core.py`.
use crate::CreatureManager;
use serde_json::Value;
use std::sync::Arc;
impl CreatureManager {
    pub fn update_hover(&mut self, x: f64, y: f64) -> Option<u64> {
        let id = self.creature_at(x, y);
        for c in &mut self.creatures {
            c.hovered = Some(c.id) == id;
        }
        id
    }
    pub fn clear_hover(&mut self) {
        for c in &mut self.creatures {
            c.hovered = false;
        }
    }
    pub fn set_naming_enabled(&mut self, e: bool) -> String {
        self.naming_enabled = e;
        if !e {
            self.clear_hover();
        }
        format!("Right-click naming turned {}.", onoff(e))
    }
    pub fn set_always_show_names(&mut self, e: bool) -> String {
        self.always_show_names = e;
        format!("Spider names {}.", if e { "always shown" } else { "shown on hover only" })
    }
    pub fn apply_label_overrides(&mut self) {
        for c in &mut self.creatures {
            c.force_show_level = self.always_show_levels;
            c.force_show_health = self.always_show_health;
            c.force_show_xp = self.always_show_xp;
            c.force_show_stamina = self.always_show_stamina;
        }
    }
    pub fn set_always_show_levels(&mut self, e: bool) -> String {
        self.always_show_levels = e;
        self.apply_label_overrides();
        format!("Level now shown {}.", if e { "on every spider" } else { "only where it was pinned" })
    }
    pub fn set_always_show_health(&mut self, e: bool) -> String {
        self.always_show_health = e;
        self.apply_label_overrides();
        format!("Health bar now shown {}.", if e { "on every spider" } else { "only where it was pinned" })
    }
    pub fn set_always_show_xp(&mut self, e: bool) -> String {
        self.always_show_xp = e;
        self.apply_label_overrides();
        format!("XP bar {}.", if e { "shown on every spider" } else { "hidden" })
    }
    pub fn set_always_show_stamina(&mut self, e: bool) -> String {
        self.always_show_stamina = e;
        self.apply_label_overrides();
        format!("Stamina bar {}.", if e { "shown on every spider" } else { "hidden" })
    }
    pub fn name_creature(&mut self, id: u64, name: &str) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to name.".into();
        };
        c.set_name(name);
        let name = c.name.clone();
        self.save_or_warn();
        if name.is_empty() {
            "Cleared this spider's name.".into()
        } else {
            format!("Named this spider \u{201c}{name}\u{201d}.")
        }
    }
    pub fn set_creature_level_pin(&mut self, id: u64, e: bool) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to update.".into();
        };
        c.set_level_label_pinned(e);
        self.save_or_warn();
        format!("Level display {} this spider's name.", if e { "pinned above" } else { "removed from" })
    }
    pub fn set_creature_health_pin(&mut self, id: u64, e: bool) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to update.".into();
        };
        c.set_health_label_pinned(e);
        self.save_or_warn();
        format!("Health bar {} this spider.", if e { "pinned above" } else { "removed from" })
    }
    pub fn set_creature_team(&mut self, id: u64, team: &str) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to update.".into();
        };
        c.set_team(team);
        let team = dc_content::util::py_repr(&Value::String(c.progression.team_id.clone()));
        self.save_or_warn();
        format!("Assigned this spider to team {team}.")
    }
    pub fn set_creature_relation(&mut self, a: u64, b: u64, relation: &str) -> String {
        if a == b || self.creature(a).is_none() || self.creature(b).is_none() {
            return "No pair of spiders selected.".into();
        }
        let relation = relation.trim().to_lowercase();
        if !["friend", "neutral", "foe"].contains(&relation.as_str()) {
            return "Relation must be friend, neutral, or foe.".into();
        }
        let left = self.creature(a).unwrap();
        let right = self.creature(b).unwrap();
        let (lid, rid) = (left.progression_id.clone(), right.progression_id.clone());
        let message = format!("Set {} and {} to {relation}.", left.display_name(), right.display_name());
        self.creature_mut(a).unwrap().progression.relation_overrides.insert(rid, relation.clone());
        self.creature_mut(b).unwrap().progression.relation_overrides.insert(lid, relation);
        self.save_or_warn();
        message
    }
    pub fn equip_creature_item(&mut self, id: u64, item: &str) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to equip.".into();
        };
        let (ok, msg) = c.equip_item(item);
        if ok {
            self.save_or_warn();
        }
        msg
    }
    pub fn unequip_creature_item(&mut self, id: u64, slot: &str) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to unequip.".into();
        };
        let (ok, msg) = c.unequip_item(slot);
        if ok {
            self.save_or_warn();
        }
        msg
    }
    pub fn unlock_creature_ability(&mut self, id: u64, ability: &str) -> String {
        let Some(c) = self.creature_mut(id) else {
            return "No spider there to unlock.".into();
        };
        let (ok, msg) = c.unlock_progression_ability(ability);
        if ok {
            self.save_or_warn();
        }
        msg
    }
    pub fn set_interferable(&mut self, e: bool) {
        self.interferable = e;
        if !e {
            if let Some(id) = self.dragged_creature.take() {
                if let Some(c) = self.creature_mut(id) {
                    c.dragging = false;
                    c.kin.motion_paused = false;
                    c.kin.inertia_vx = 0.0;
                    c.kin.inertia_vy = 0.0;
                    c.kin.inertia_timer = 0.0;
                    c.enter_idle();
                }
            }
        }
    }
    pub fn set_size_scale(&mut self, scale: f64) {
        self.size_scale = crate::constants::clamp(scale, 0.45, 2.25);
        for c in &mut self.creatures {
            c.set_size_scale(self.size_scale);
        }
        self.fly_world.configure(dc_world::flies::FlyConfigure { scale: Some(self.size_scale), ..Default::default() });
    }
    pub fn set_mood_mode(&mut self, mode: &str) -> String {
        self.mood_mode = if mode.is_empty() { "auto".into() } else { mode.to_lowercase() };
        for c in &mut self.creatures {
            c.set_mood_mode(&self.mood_mode);
        }
        let label = if self.mood_mode == "auto" {
            "Auto (per personality)".into()
        } else {
            let mut chars = self.mood_mode.chars();
            chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str()
        };
        format!("Mood set to {label} for {} spider(s).", self.creatures.len())
    }
    pub fn set_gait_style(&mut self, style: &str) -> String {
        let gait = dc_kinematics::constants::normalize_gait_style(Some(style));
        let name = gait.as_str();
        self.gait_style = Some(name.into());
        for c in &mut self.creatures {
            c.set_gait_style(name);
        }
        let label = dc_kinematics::constants::GAIT_LABELS
            .iter()
            .find(|(s, _)| *s == name)
            .map(|(_, l)| *l)
            .unwrap_or("Classic");
        format!("Movement set to {label} for {} spider(s).", self.creatures.len())
    }
    pub fn set_social_play(&mut self, e: bool) -> String {
        self.social_play = e;
        for c in &mut self.creatures {
            c.set_allow_social(e);
        }
        self.refresh_neighbor_links();
        format!("Social play turned {}.", onoff(e))
    }
    pub fn set_conflict_enabled(&mut self, e: bool) -> String {
        self.conflict_enabled = e;
        format!("Conflict turned {}.", onoff(e))
    }
    pub fn set_allow_mouse_capture(&mut self, e: bool) -> String {
        self.allow_mouse_capture = e;
        self.mouse_web_world.enabled = e;
        if !e {
            self.mouse_web_world.clear();
        }
        format!("Mouse web-trapping turned {}.", onoff(e))
    }
    pub fn set_desktop_icons_enabled(&mut self, e: bool) -> String {
        self.desktop_icons_enabled = e;
        format!("Desktop icon awareness turned {}.", onoff(e))
    }
    pub fn set_team_stances(&mut self, raw: Option<&Value>) -> String {
        self.team_stances = Arc::new(dc_state::progression::normalize_team_stances(raw));
        for c in &mut self.creatures {
            c.team_stances = Some(self.team_stances.clone());
        }
        let n: usize = self.team_stances.values().map(|row| row.len()).sum();
        format!("Team relations updated ({} declared).", n / 2)
    }
    pub fn set_team_profiles(&mut self, raw: Option<&Value>, used: &[String]) -> String {
        self.team_profiles = Arc::new(dc_state::teams::normalize_teams(raw, used));
        for c in &mut self.creatures {
            c.team_profiles = Some(self.team_profiles.clone());
        }
        format!("Teams updated ({} named).", self.team_profiles.len())
    }
    pub fn team_payload(&self) -> Value {
        dc_state::teams::teams_payload(&self.team_profiles)
    }
    pub fn set_creature_skill(&mut self, id: u64, skill: &str, e: bool) -> String {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id) else {
            return "No spider selected.".into();
        };
        let Some(s) = dc_content::skills::skill_by_id(skill) else {
            return "Unknown skill.".into();
        };
        c.set_skill_enabled(skill, e, Some(&mut self.web_world));
        format!("{} {} for {}.", s.display_name, if e { "enabled" } else { "disabled" }, c.display_name())
    }
}
fn onoff(e: bool) -> &'static str {
    if e { "on" } else { "off" }
}
