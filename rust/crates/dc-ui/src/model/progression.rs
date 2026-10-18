//! Skills, armour and a spider's progression as the character, armour and
//! inspector windows show them. Mirrors state/progression.py.
//!
//! [`crate::bridge`] has `From<&dc_state::progression::ProgressionState>` for
//! [`Progression`] and builds [`ProgressionCatalog`] from dc-state's
//! `ABILITY_TREE`, `ARMOR_CATALOG` and `ARMOR_SETS`. The sample catalog in
//! [`crate::sample`] carries the Python values verbatim, checked equal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `AbilityNode`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SkillNode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub level_required: u32,
    pub cost: u32,
    pub prerequisites: Vec<String>,
    /// (stat, amount): `max_hp`, `armor`, `damage`, `max_energy`,
    /// `energy_regen`, `speed`, `web_homing`.
    pub effects: Vec<(String, f64)>,
}

/// `ArmorItem`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ArmourItem {
    pub id: String,
    pub name: String,
    pub slot: String,
    pub description: String,
    pub armor: f64,
    pub max_hp: f64,
    pub max_energy: f64,
    pub damage: f64,
    pub speed: f64,
    pub set_id: String,
    pub tier: String,
}

/// `ArmorSet`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ArmourSet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub effects: Vec<(String, f64)>,
}

/// `ARMOR_TIERS`, lowest first.
pub const ARMOR_TIERS: [&str; 5] = ["common", "uncommon", "rare", "epic", "legendary"];
/// `MAX_LEVEL`.
pub const MAX_LEVEL: u32 = 30;
/// `MAX_ITEM_LEVEL`.
pub const MAX_ITEM_LEVEL: u32 = 5;
/// `LEVEL_STEP`: each item level adds 20% of the piece's base stats.
pub const LEVEL_STEP: f64 = 0.2;
/// `armour_ui.SLOT_ORDER`.
pub const SLOT_ORDER: [&str; 5] = ["head", "pedipalps", "carapace", "abdomen", "legs"];

/// `xp_to_next_level`.
pub fn xp_to_next_level(level: u32) -> u32 {
    let level = level.clamp(1, MAX_LEVEL) as f64;
    dc_core::math::py_round(92.0 + level * 21.0 + level * level * 2.1) as u32
}

/// `level_multiplier`.
pub fn level_multiplier(level: u32) -> f64 {
    1.0 + LEVEL_STEP * (level.clamp(1, MAX_ITEM_LEVEL) - 1) as f64
}

impl ArmourItem {
    /// `item_effects`: the non-zero stats, in `ITEM_STATS` order.
    pub fn effects(&self) -> Vec<(String, f64)> {
        [
            ("armor", self.armor),
            ("max_hp", self.max_hp),
            ("damage", self.damage),
            ("max_energy", self.max_energy),
            ("speed", self.speed),
        ]
        .into_iter()
        .filter(|(_, v)| *v != 0.0)
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    }

    /// `item_at_level`: stats scaled to `level`.
    pub fn at_level(&self, level: u32) -> ArmourItem {
        let k = level_multiplier(level);
        ArmourItem {
            armor: self.armor * k,
            max_hp: self.max_hp * k,
            max_energy: self.max_energy * k,
            damage: self.damage * k,
            speed: self.speed * k,
            ..self.clone()
        }
    }

    /// `armour_ui.short_name`: a set piece without its set's first word.
    pub fn short_name(&self) -> String {
        let name = if !self.set_id.is_empty() {
            self.name.split_once(' ').map(|(_, rest)| rest).unwrap_or(&self.name)
        } else {
            &self.name
        };
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }

    pub fn tier_index(&self) -> usize {
        ARMOR_TIERS.iter().position(|t| *t == self.tier).unwrap_or(0)
    }
}

/// The tree, the armour and the sets.
#[derive(Clone, Debug, Default)]
pub struct ProgressionCatalog {
    pub tree: Vec<SkillNode>,
    pub armour: Vec<ArmourItem>,
    pub sets: Vec<ArmourSet>,
    /// `PlayerController.WEB_ENERGY` and `JUMP_ENERGY`, named in stat text.
    pub web_energy: f64,
    pub jump_energy: f64,
}

impl ProgressionCatalog {
    pub fn node(&self, id: &str) -> Option<&SkillNode> {
        self.tree.iter().find(|n| n.id == id)
    }
    pub fn item(&self, id: &str) -> Option<&ArmourItem> {
        self.armour.iter().find(|i| i.id == id)
    }
    pub fn set(&self, id: &str) -> Option<&ArmourSet> {
        self.sets.iter().find(|s| s.id == id)
    }
    /// `ArmorSet.pieces`.
    pub fn set_pieces(&self, set_id: &str) -> Vec<&ArmourItem> {
        self.armour.iter().filter(|i| i.set_id == set_id).collect()
    }
    /// The distinct slots, in catalogue order (`SLOT_COUNT`).
    pub fn slots(&self) -> Vec<String> {
        let mut slots: Vec<String> = Vec::new();
        for item in &self.armour {
            if !slots.contains(&item.slot) {
                slots.push(item.slot.clone());
            }
        }
        slots
    }
}

/// `ProgressionState`, as far as the windows read it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Progression {
    pub level: u32,
    pub xp: u32,
    pub total_xp: u32,
    pub skill_points: u32,
    pub unlocked_abilities: Vec<String>,
    pub inventory: Vec<String>,
    /// slot -> item id.
    pub equipped: BTreeMap<String, String>,
    pub team_id: String,
    /// item id -> level (missing means 1).
    pub item_levels: BTreeMap<String, u32>,
}

impl Default for Progression {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            total_xp: 0,
            skill_points: 0,
            unlocked_abilities: Vec::new(),
            inventory: vec!["silk_carapace".into(), "leg_guard_set".into()],
            equipped: BTreeMap::new(),
            team_id: "neutral".into(),
            item_levels: BTreeMap::new(),
        }
    }
}

impl Progression {
    /// `can_unlock`.
    pub fn can_unlock(&self, catalog: &ProgressionCatalog, ability_id: &str) -> bool {
        let Some(node) = catalog.node(ability_id) else { return false };
        if self.unlocked_abilities.iter().any(|a| a == ability_id) {
            return false;
        }
        self.level >= node.level_required
            && self.skill_points >= node.cost
            && node.prerequisites.iter().all(|r| self.unlocked_abilities.contains(r))
    }

    pub fn item_level(&self, item_id: &str) -> u32 {
        self.item_levels.get(item_id).copied().unwrap_or(1)
    }

    /// `equipped_items`: worn pieces at their raised level.
    pub fn equipped_items(&self, catalog: &ProgressionCatalog) -> Vec<ArmourItem> {
        self.equipped
            .values()
            .filter_map(|id| catalog.item(id).map(|item| item.at_level(self.item_level(id))))
            .collect()
    }

    /// `set_pieces_worn`.
    pub fn set_pieces_worn(&self, catalog: &ProgressionCatalog, set_id: &str) -> usize {
        catalog.set_pieces(set_id).iter().filter(|p| self.equipped.values().any(|w| *w == p.id)).count()
    }

    /// `set_bonus_effects`: the bonuses of every set worn complete.
    pub fn set_bonus_effects(&self, catalog: &ProgressionCatalog) -> BTreeMap<String, f64> {
        let mut totals = BTreeMap::new();
        for set in &catalog.sets {
            let pieces = catalog.set_pieces(&set.id).len();
            if pieces > 0 && self.set_pieces_worn(catalog, &set.id) == pieces {
                for (k, v) in &set.effects {
                    *totals.entry(k.clone()).or_insert(0.0) += v;
                }
            }
        }
        totals
    }

    /// The node look and its note (`SkillTree.show_state`).
    pub fn node_state(&self, catalog: &ProgressionCatalog, node: &SkillNode) -> (crate::widgets::NodeLook, String) {
        use crate::widgets::NodeLook;
        if self.unlocked_abilities.contains(&node.id) {
            return (NodeLook::Unlocked, "Learned".into());
        }
        if self.can_unlock(catalog, &node.id) {
            return (NodeLook::Available, format!("Learn \u{b7} {} pt", node.cost));
        }
        let missing: Vec<String> = node
            .prerequisites
            .iter()
            .filter(|r| !self.unlocked_abilities.contains(r))
            .map(|r| catalog.node(r).map(|n| n.name.clone()).unwrap_or_else(|| r.clone()))
            .collect();
        let note = if self.level < node.level_required {
            format!("Level {}", node.level_required)
        } else if !missing.is_empty() {
            format!("Needs {}", missing.join(", "))
        } else {
            format!("{} pt", node.cost)
        };
        (NodeLook::Locked, note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_curve_matches_the_python() {
        assert_eq!(xp_to_next_level(1), 115);
        assert_eq!(xp_to_next_level(10), 512);
        assert_eq!(xp_to_next_level(30), 2612);
        assert_eq!(xp_to_next_level(0), 115);
    }

    #[test]
    fn set_pieces_lose_their_set_word() {
        let c = crate::sample::progression_catalog();
        assert_eq!(c.item("sun_gauntlets").unwrap().short_name(), "Palp gauntlets");
        assert_eq!(c.item("chelicerae_cap").unwrap().short_name(), "Fang cap");
        assert_eq!(c.slots().len(), 5);
    }

    #[test]
    fn unlocking_needs_level_points_and_prerequisites() {
        let c = crate::sample::progression_catalog();
        let mut p = Progression { level: 5, skill_points: 2, ..Default::default() };
        assert!(p.can_unlock(&c, "vitality"));
        assert!(!p.can_unlock(&c, "carapace_harden"));
        p.unlocked_abilities.push("vitality".into());
        assert!(p.can_unlock(&c, "carapace_harden"));
        assert!(!p.can_unlock(&c, "vitality"));
        let (_, note) = p.node_state(&c, c.node("apex_predator").unwrap());
        assert_eq!(note, "Level 15");
        p.level = 15;
        let (_, note) = p.node_state(&c, c.node("apex_predator").unwrap());
        assert_eq!(note, "Needs Power strike, Hardened carapace");
    }

    #[test]
    fn a_complete_set_adds_its_bonus() {
        let c = crate::sample::progression_catalog();
        let mut p = Progression::default();
        for piece in c.set_pieces("forager") {
            p.inventory.push(piece.id.clone());
            p.equipped.insert(piece.slot.clone(), piece.id.clone());
        }
        let bonus = p.set_bonus_effects(&c);
        assert_eq!(bonus.get("max_energy"), Some(&8.0));
        assert_eq!(p.set_pieces_worn(&c, "forager"), 5);
        assert!((c.item("sun_aegis").unwrap().at_level(5).armor - 3.0 * 1.8).abs() < 1e-9);
    }
}
