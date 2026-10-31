//! Port of the progression, stats, damage and team methods of
//! `creature/core.py`.

use crate::bump;
use crate::constants::*;
use crate::creature::Creature;
use crate::pymath::{clamp, py_round};
use crate::state::State;
use crate::world::{CreatureId, CreatureView};
use dc_state::progression::{
    ABILITY_TREE, MAX_LEVEL, ability_by_id, armor_by_id, equipped_items, growth_multipliers, normalize_team_id,
    set_bonus_effects, team_stance, xp_to_next_level,
};
use serde_json::{Value, json};

/// A relation between two spiders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    Friend,
    Neutral,
    Foe,
}

impl Relation {
    pub fn parse(s: &str) -> Relation {
        match s {
            "friend" => Relation::Friend,
            "foe" => Relation::Foe,
            _ => Relation::Neutral,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Friend => "friend",
            Relation::Neutral => "neutral",
            Relation::Foe => "foe",
        }
    }
}

/// `round(x, 2)` for the snapshot.
fn round2(x: f64) -> f64 {
    dc_content::util::py_round_n(x, 2)
}

impl Creature {
    /// `_speed_mult()`
    pub fn speed_mult(&self) -> f64 {
        let webbed = if self.webbed_held() {
            0.0
        } else if self.webbed() {
            WEBBED_SPEED_MULT
        } else {
            1.0
        };
        let fleeing = if self.fleeing() { FLEE_SPEED_MULT } else { 1.0 };
        self.pf("speed_multiplier", 1.0) * self.progression_speed_multiplier * webbed * fleeing
    }

    pub fn level(&self) -> i64 {
        self.progression.level
    }

    pub fn xp(&self) -> i64 {
        self.progression.xp
    }

    /// `_progression_effect(key)`: talents, then worn pieces, then set bonuses.
    pub fn progression_effect(&self, key: &str) -> f64 {
        let mut value = 0.0;
        for ability_id in &self.progression.unlocked_abilities {
            if let Some(node) = ability_by_id(ability_id) {
                value += node.effects.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(0.0);
            }
        }
        for item in equipped_items(&self.progression) {
            value += item.stat(key);
        }
        value += set_bonus_effects(&self.progression).get(key).copied().unwrap_or(0.0);
        value
    }

    /// `_apply_progression_stats(reset_resources, carry_wounds)`
    pub fn apply_progression_stats(&mut self, reset_resources: bool, carry_wounds: bool) {
        let (size_mult, growth_speed) = growth_multipliers(self.progression.level);
        self.progression_size_multiplier = size_mult;
        self.progression_speed_multiplier = clamp(growth_speed + self.progression_effect("speed"), 0.72, 2.40);
        let level = self.progression.level as f64;
        let old_max_hp = self.max_hp;
        let old_max_energy = self.max_energy;
        self.max_hp = 1.0f64.max(self.combat_base_hp + (level - 1.0) * 5.0 + self.progression_effect("max_hp"));
        self.max_energy =
            1.0f64.max(self.combat_base_energy + (level - 1.0) * 3.0 + self.progression_effect("max_energy"));
        self.armor = 0.0f64.max(self.combat_base_armor + (level - 1.0) * 0.18 + self.progression_effect("armor"));
        self.damage = 0.1f64.max(self.combat_base_damage + (level - 1.0) * 0.35 + self.progression_effect("damage"));
        if let Some((hp, bite, shell)) = self.stat_scale {
            self.max_hp *= hp;
            self.damage *= bite;
            self.armor += shell;
        }
        self.kin.size = self.progression_base_size * self.size_scale * size_mult;
        if reset_resources {
            self.hp = self.max_hp;
            self.energy = self.max_energy;
        } else if carry_wounds {
            self.hp = clamp(self.hp + (self.max_hp - old_max_hp), 0.0, self.max_hp);
            self.energy = clamp(self.energy + (self.max_energy - old_max_energy), 0.0, self.max_energy);
        } else {
            let hp_ratio = self.hp / 1.0f64.max(old_max_hp);
            let energy_ratio = self.energy / 1.0f64.max(old_max_energy);
            self.hp = clamp(hp_ratio * self.max_hp, 0.0, self.max_hp);
            self.energy = clamp(energy_ratio * self.max_energy, 0.0, self.max_energy);
        }
    }

    /// `gain_experience(amount, reason)`: award XP, return level-up events.
    pub fn gain_experience(&mut self, amount: f64) -> Vec<String> {
        let amount = if amount.is_finite() { (py_round(amount) as i64).max(0) } else { 0 };
        if amount <= 0 {
            return Vec::new();
        }
        self.progression.total_xp += amount;
        if self.progression.level >= MAX_LEVEL {
            self.progression.xp = xp_to_next_level(MAX_LEVEL);
            return Vec::new();
        }
        self.progression.xp += amount;
        let mut events = Vec::new();
        while self.progression.level < MAX_LEVEL {
            let threshold = xp_to_next_level(self.progression.level);
            if self.progression.xp < threshold {
                break;
            }
            self.progression.xp -= threshold;
            self.progression.level += 1;
            self.progression.skill_points += 1;
            self.apply_progression_stats(false, true);
            events.push(format!("reached level {}", self.progression.level));
            if !self.player_control && !self.chooses_own_skills {
                events.extend(self.spend_skill_points());
            }
        }
        if self.progression.level >= MAX_LEVEL {
            self.progression.xp = xp_to_next_level(MAX_LEVEL);
        }
        events
    }

    /// `_spend_skill_points()`: cheapest-first by level, then tree order.
    pub fn spend_skill_points(&mut self) -> Vec<String> {
        let mut unlocked = Vec::new();
        while self.progression.skill_points > 0 {
            let node = ABILITY_TREE
                .iter()
                .enumerate()
                .filter(|(_, n)| self.progression.can_unlock(n.id))
                .min_by_key(|(i, n)| (n.level_required, *i))
                .map(|(_, n)| n);
            let Some(node) = node else { break };
            self.progression.skill_points -= node.cost;
            self.progression.unlocked_abilities.push(node.id.to_string());
            unlocked.push(format!("learned {}", node.name));
        }
        if !unlocked.is_empty() {
            self.apply_progression_stats(false, true);
        }
        unlocked
    }

    /// `unlock_progression_ability(ability_id)`
    pub fn unlock_progression_ability(&mut self, ability_id: &str) -> (bool, String) {
        let ability_id = ability_id.trim().to_lowercase();
        let Some(node) = ability_by_id(&ability_id) else {
            return (false, "Unknown progression ability.".into());
        };
        if self.progression.unlocked_abilities.contains(&ability_id) {
            return (false, format!("{} is already unlocked.", node.name));
        }
        if self.progression.level < node.level_required {
            return (false, format!("{} unlocks at level {}.", node.name, node.level_required));
        }
        if self.progression.skill_points < node.cost {
            return (false, "This spider has no skill points available.".into());
        }
        if node.prerequisites.iter().any(|req| !self.progression.unlocked_abilities.iter().any(|a| a == req)) {
            return (false, "Unlock the prerequisite ability first.".into());
        }
        self.progression.skill_points -= node.cost;
        self.progression.unlocked_abilities.push(ability_id);
        self.apply_progression_stats(false, false);
        (true, format!("Unlocked {}.", node.name))
    }

    /// `equip_item(item_id)`
    pub fn equip_item(&mut self, item_id: &str) -> (bool, String) {
        let Some(item) = armor_by_id(&item_id.trim().to_lowercase()) else {
            return (false, "Unknown armor item.".into());
        };
        if !self.progression.inventory.iter().any(|i| i == item.id) {
            return (false, "That item is not in this spider's inventory.".into());
        }
        if !(item.size_min <= self.size_scale && self.size_scale <= item.size_max) {
            return (false, "That armor does not fit this spider's size.".into());
        }
        self.progression.equip(item.id);
        self.apply_progression_stats(false, false);
        (true, format!("Equipped {}.", item.name))
    }

    /// `unequip_item(slot)`
    pub fn unequip_item(&mut self, slot: &str) -> (bool, String) {
        if !self.progression.unequip(slot) {
            return (false, "Nothing is equipped in that anatomy slot.".into());
        }
        self.apply_progression_stats(false, false);
        (true, format!("Unequipped {} armor.", slot.replace('_', " ")))
    }

    pub fn add_inventory_item(&mut self, item_id: &str) -> bool {
        self.progression.add_item(&item_id.trim().to_lowercase())
    }

    /// `take_damage(amount, source)`: what landed after armour.
    pub fn take_damage(&mut self, amount: f64, source: Option<CreatureId>) -> f64 {
        if self.dead || amount.is_nan() {
            return 0.0;
        }
        let incoming = amount.max(0.0);
        let dealt = if incoming > 0.0 { 0.1f64.max(incoming - self.armor) } else { 0.0 };
        self.hp = clamp(self.hp - dealt, 0.0, self.max_hp);
        if dealt > 0.0 {
            self.last_attacker = source;
            self.hurt_flash = 1.0;
            self.note_damage(dealt);
        }
        if self.hp <= 0.0 {
            self.die();
        }
        dealt
    }

    /// `webbed`
    #[inline]
    pub fn webbed(&self) -> bool {
        self.webbed_timer > 0.0
    }

    /// `webbed_held`
    #[inline]
    pub fn webbed_held(&self) -> bool {
        self.webbed_timer > (WEBBED_SECONDS - WEBBED_HOLD_SECONDS)
    }

    /// `fleeing`
    #[inline]
    pub fn fleeing(&self) -> bool {
        self.flee_timer > 0.0
    }

    /// `web_pinned(kind, by)`: take a hit of trapping silk.
    pub fn web_pinned(&mut self, by: Option<CreatureId>) {
        if self.dead {
            return;
        }
        let fresh = !self.webbed();
        self.webbed_timer = self.webbed_timer.max(WEBBED_SECONDS);
        self.webbed_by = by;
        if fresh {
            self.pin_net();
        }
        bump!(self.mood, arousal = 0.3, valence = -0.25);
    }

    /// `_die()`
    pub fn die(&mut self) {
        self.dead = true;
        self.hp = 0.0;
        self.state = State::Downed;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.current_speed = 0.0;
        self.job_mode = "idle".into();
        self.job_target = None;
        self.job_alert_target = None;
        self.job_facing = None;
        self.prey = None;
        self.hunting_prey = false;
    }

    pub fn heal(&mut self, amount: f64) -> f64 {
        if amount.is_nan() {
            return 0.0;
        }
        let before = self.hp;
        self.hp = clamp(self.hp + amount.max(0.0), 0.0, self.max_hp);
        self.hp - before
    }

    pub fn spend_energy(&mut self, amount: f64) -> bool {
        if amount.is_nan() {
            return false;
        }
        let cost = amount.max(0.0);
        if cost > self.energy {
            return false;
        }
        self.energy -= cost;
        true
    }

    /// `_regenerate_energy(dt)`
    pub fn regenerate_energy(&mut self, dt: f64) {
        if self.energy >= self.max_energy || self.dragging {
            return;
        }
        let regen = 8.0 + self.progression_effect("energy_regen");
        self.energy = clamp(self.energy + dt.max(0.0) * regen, 0.0, self.max_energy);
    }

    pub fn set_team(&mut self, team_id: &str) {
        self.progression.team_id = normalize_team_id(team_id);
    }

    /// `set_job(job_id)`
    pub fn set_job(&mut self, job_id: &str) {
        self.job_id = dc_content::jobs::normalize_job_id(Some(job_id)).to_string();
        self.job_mode = "idle".into();
        self.job_target = None;
        self.job_alert_target = None;
        self.job_base_id = None;
        self.job_facing = None;
    }

    pub fn job_label(&self) -> &'static str {
        dc_content::jobs::job_by_id(&self.job_id).map(|j| j.display_name).unwrap_or("None")
    }

    /// `relation_to(other)`: a per-pair override, the team stance, or neutral.
    pub fn relation_to(&self, other: &CreatureView) -> Relation {
        let key: &str = &other.progression_id;
        if !key.is_empty()
            && let Some(rel) = self.progression.relation_overrides.get(key)
        {
            return Relation::parse(rel);
        }
        match team_stance(&self.progression.team_id, &other.team_id, self.team_stances.as_deref()) {
            Some(rel) => Relation::parse(&rel),
            None => Relation::Neutral,
        }
    }

    /// `progression_snapshot()`
    pub fn progression_snapshot(&self) -> Value {
        let mut data = self.progression.to_value();
        if let Value::Object(m) = &mut data {
            m.insert("job".into(), json!(self.job_id));
            m.insert("level".into(), json!(self.level()));
            m.insert("xp_to_next".into(), json!(xp_to_next_level(self.level())));
            m.insert("max_hp".into(), json!(round2(self.max_hp)));
            m.insert("hp".into(), json!(round2(self.hp)));
            m.insert("max_energy".into(), json!(round2(self.max_energy)));
            m.insert("energy".into(), json!(round2(self.energy)));
            m.insert("armor".into(), json!(round2(self.armor)));
            m.insert("damage".into(), json!(round2(self.damage)));
        }
        data
    }

    /// `_personality_flag(key, default)`
    pub fn personality_flag(&self, key: &str, default: bool) -> bool {
        match self.p().get(key) {
            None => default,
            Some(Value::String(s)) => matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
            Some(v) => crate::pymath::py_truthy(v),
        }
    }

    #[inline]
    pub fn has_skill(&self, skill_id: &str) -> bool {
        self.skills.has(skill_id)
    }

    pub fn skill_ids(&self) -> Vec<String> {
        self.skills.ids()
    }

    /// `wary_of_cursor`
    #[inline]
    pub fn wary_of_cursor(&self) -> bool {
        self.cursor_pressure >= CURSOR_WARY_THRESHOLD
    }

    pub fn health_fraction(&self) -> f64 {
        clamp(self.hp / 1.0f64.max(self.max_hp), 0.0, 1.0)
    }

    pub fn stamina_fraction(&self) -> f64 {
        clamp(self.energy / 1.0f64.max(self.max_energy), 0.0, 1.0)
    }

    /// Progress toward the next level; full at the level cap.
    pub fn xp_fraction(&self) -> f64 {
        if self.level() >= MAX_LEVEL {
            return 1.0;
        }
        clamp(self.xp() as f64 / 1.0f64.max(xp_to_next_level(self.level()) as f64), 0.0, 1.0)
    }
}
