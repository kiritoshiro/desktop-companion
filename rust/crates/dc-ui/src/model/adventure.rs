//! The Adventure profile as the mode menu, character window and admin window
//! see it. Mirrors app/adventure_profile.py (`hero_progression`,
//! `mission_record`, `MAX_NAME_LENGTH`, `clean_name`), app/campaign.py
//! (`MAPS`, `map_unlocked`, `SELL_PRICES`, `COMPANIONS`) and app/armoury.py
//! (`party`, `worn_by`, `can_upgrade`, `sell_price`).
//!
//! [`crate::bridge::adventure_view`] and [`crate::bridge::party_view`] build
//! these from dc-adventure's profile, campaign and armoury.

use std::collections::BTreeMap;

use super::{Progression, ProgressionCatalog};

/// `HERO` and `PLAYER_TWO`: who a sheet dresses.
pub const HERO: &str = "hero";
pub const PLAYER_TWO: &str = "player_two";
/// `MAX_NAME_LENGTH`.
pub const MAX_NAME_LENGTH: usize = 24;
pub const DEFAULT_HERO_NAME: &str = "Wayfarer";

/// `clean_name`: whitespace collapsed, at most 24 characters, never empty.
pub fn clean_name(value: &str) -> String {
    let text: String = value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_NAME_LENGTH).collect();
    if text.is_empty() { DEFAULT_HERO_NAME.into() } else { text }
}

/// `mission_record`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MissionRecord {
    pub victories: u32,
    pub defeats: u32,
    pub best_seconds: Option<f64>,
}

impl MissionRecord {
    /// The status line on a mission card ("✔ Won ×2 · best 3:07").
    pub fn status_text(&self) -> (String, bool) {
        if self.victories > 0 {
            let time = self
                .best_seconds
                .map(|best| {
                    format!(" \u{b7} best {}:{:02}", (best / 60.0).floor() as i64, (best % 60.0).floor() as i64)
                })
                .unwrap_or_default();
            (format!("\u{2714} Won \u{d7}{}{}", self.victories, time), true)
        } else if self.defeats > 0 {
            ("Not won yet".into(), false)
        } else {
            (String::new(), false)
        }
    }
}

/// One campaign map (`campaign.MapInfo`) with the player's record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapCard {
    pub id: String,
    pub title: String,
    pub blurb: String,
    /// `raid`, `swarm`, `reclaim`.
    pub kind: String,
    pub unlock_after: Option<String>,
    pub playable: bool,
    pub record: MissionRecord,
}

/// A map made in the map editor (`custom_maps.list_maps`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomMapCard {
    pub id: String,
    pub title: String,
    pub blurb: String,
    pub kind: String,
    pub tier: u32,
    pub buildings: usize,
    pub victories: u32,
    /// `not custom_maps.problems(data)`.
    pub playable: bool,
}

/// A monitor (`QGuiApplication.screens()` + `screen_activity.monitor_states`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenInfo {
    pub name: String,
    pub number: usize,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    /// `active`, `asleep` or `off`.
    pub state: String,
}

/// Everything the Adventure page shows.
#[derive(Clone, Debug, Default)]
pub struct AdventureView {
    pub hero_name: String,
    pub level: u32,
    pub skill_points: u32,
    pub companions: usize,
    pub amber: i64,
    pub admin: bool,
    pub two_player: bool,
    pub all_screens: bool,
    pub disabled_screens: Vec<String>,
    pub maps: Vec<MapCard>,
    pub custom_maps: Vec<CustomMapCard>,
    pub screens: Vec<ScreenInfo>,
    pub controls: super::ControlSettings,
}

impl AdventureView {
    /// `map_unlocked`: admin opens everything, the first maps are always open,
    /// the rest open once the map before is won.
    pub fn map_unlocked(&self, map_id: &str) -> bool {
        if map_id.starts_with("custom-") {
            return true;
        }
        let Some(map) = self.maps.iter().find(|m| m.id == map_id && m.playable) else { return false };
        if self.admin {
            return true;
        }
        match &map.unlock_after {
            None => true,
            Some(before) => self.maps.iter().find(|m| &m.id == before).is_some_and(|m| m.record.victories > 0),
        }
    }

    pub fn map(&self, map_id: &str) -> Option<&MapCard> {
        self.maps.iter().find(|m| m.id == map_id)
    }

    /// The hero line (`refresh_adventure`).
    pub fn hero_line(&self) -> String {
        let points = self.skill_points;
        let spend = if points > 0 {
            format!("  \u{b7}  {points} point{} to spend", if points != 1 { "s" } else { "" })
        } else {
            String::new()
        };
        let companions = self.companions;
        format!(
            "{}  \u{b7}  Level {}{}  \u{b7}  {} companion{}  \u{b7}  {} amber{}",
            self.hero_name,
            self.level,
            spend,
            companions,
            if companions != 1 { "s" } else { "" },
            self.amber,
            if self.admin { "  \u{b7}  ADMIN" } else { "" }
        )
    }
}

/// One spider that can wear armour (`armoury.party`).
#[derive(Clone, Debug, Default)]
pub struct PartyMember {
    pub who: String,
    pub name: String,
    pub progression: Progression,
}

/// The armoury and party the character window dresses.
#[derive(Clone, Debug, Default)]
pub struct PartyView {
    pub members: Vec<PartyMember>,
    pub two_player: bool,
    pub amber: i64,
    /// One of each piece owned.
    pub owned: Vec<String>,
    /// item id -> spare duplicates.
    pub spares: BTreeMap<String, u32>,
    /// item id -> level.
    pub levels: BTreeMap<String, u32>,
    /// `SELL_PRICES` by tier.
    pub sell_prices: BTreeMap<String, i64>,
}

impl PartyView {
    pub fn member(&self, who: &str) -> Option<&PartyMember> {
        self.members.iter().find(|m| m.who == who)
    }
    pub fn member_mut(&mut self, who: &str) -> Option<&mut PartyMember> {
        self.members.iter_mut().find(|m| m.who == who)
    }
    /// `worn_by`: item id -> who wears it.
    pub fn worn_by(&self) -> BTreeMap<String, String> {
        let mut worn = BTreeMap::new();
        for member in &self.members {
            for item in member.progression.equipped.values() {
                worn.insert(item.clone(), member.who.clone());
            }
        }
        worn
    }
    pub fn level_of(&self, item_id: &str) -> u32 {
        self.levels.get(item_id).copied().unwrap_or(1)
    }
    pub fn spares_of(&self, item_id: &str) -> u32 {
        self.spares.get(item_id).copied().unwrap_or(0)
    }
    /// `can_upgrade`.
    pub fn can_upgrade(&self, item_id: &str) -> bool {
        self.owned.iter().any(|o| o == item_id)
            && self.spares_of(item_id) > 0
            && self.level_of(item_id) < super::MAX_ITEM_LEVEL
    }
    /// `sell_price`.
    pub fn sell_price(&self, catalog: &ProgressionCatalog, item_id: &str) -> i64 {
        catalog.item(item_id).and_then(|i| self.sell_prices.get(&i.tier)).copied().unwrap_or(0)
    }
    /// A member's sheet as the doll and bag see it: inventory is the armoury,
    /// item levels are the armoury's.
    pub fn dress(&self, who: &str) -> Progression {
        let mut state = self.member(who).map(|m| m.progression.clone()).unwrap_or_default();
        state.inventory = self.owned.clone();
        state.item_levels = self.levels.clone();
        state
    }
    /// The display name of `who`.
    pub fn name_of(&self, who: &str) -> String {
        self.member(who).map(|m| m.name.clone()).unwrap_or_else(|| who.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Big   Red  "), "Big Red");
        assert_eq!(clean_name("   "), DEFAULT_HERO_NAME);
        assert_eq!(clean_name(&"x".repeat(40)).len(), MAX_NAME_LENGTH);
    }

    #[test]
    fn mission_status_text() {
        let won = MissionRecord { victories: 2, defeats: 1, best_seconds: Some(187.4) };
        assert_eq!(won.status_text().0, "\u{2714} Won \u{d7}2 \u{b7} best 3:07");
        assert_eq!(MissionRecord { defeats: 1, ..Default::default() }.status_text().0, "Not won yet");
        assert_eq!(MissionRecord::default().status_text().0, "");
    }

    #[test]
    fn maps_unlock_in_order() {
        let mut view = crate::sample::adventure_view();
        assert!(view.map_unlocked("territory"));
        assert!(!view.map_unlocked("ember"));
        view.maps.iter_mut().find(|m| m.id == "territory").unwrap().record.victories = 1;
        assert!(view.map_unlocked("ember"));
        assert!(!view.map_unlocked("obsidian"));
        view.admin = true;
        assert!(view.map_unlocked("queen"));
        assert!(view.map_unlocked("custom-anything"));
        assert!(!view.map_unlocked("burrow"));
    }
}
