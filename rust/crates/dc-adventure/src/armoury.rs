//! Port of app/armoury.py
//!
//! What the player can do with the armour they own. The armoury lives in the
//! Adventure profile: one of each piece owned, its level, spare duplicates
//! and amber. The hero and every companion dress from it, and a piece is worn
//! by one spider at a time.
//!
//! - A piece found again becomes a spare.
//! - A spare stacked onto its piece raises it a level, up to five.
//! - Spares sell for amber.

use crate::adventure_profile::{AdventureProfile, Armoury, HERO, PLAYER_TWO, spider_progression, store_progression};
use crate::campaign::sell_price_of_tier;
use crate::progression::{Dict, MAX_ITEM_LEVEL, armor_by_id, dict_get, dict_pop, dict_set, retain_equipped};

pub fn armoury(profile: &mut AdventureProfile) -> &mut Armoury {
    &mut profile.armoury
}

pub fn level_of(profile: &AdventureProfile, item_id: &str) -> i64 {
    dict_get(&profile.armoury.levels, item_id).copied().unwrap_or(1)
}

pub fn spares_of(profile: &AdventureProfile, item_id: &str) -> i64 {
    dict_get(&profile.armoury.spares, item_id).copied().unwrap_or(0)
}

/// What `add_loot` did with a found piece. Python returns `"new"`,
/// `"spare"` or `""`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loot {
    Unknown,
    New,
    Spare,
}

impl Loot {
    pub fn as_str(self) -> &'static str {
        match self {
            Loot::Unknown => "",
            Loot::New => "new",
            Loot::Spare => "spare",
        }
    }
}

/// Put a found piece in the armoury.
pub fn add_loot(profile: &mut AdventureProfile, item_id: &str) -> Loot {
    if armor_by_id(item_id).is_none() {
        return Loot::Unknown;
    }
    let store = &mut profile.armoury;
    if !store.owned.iter().any(|i| i == item_id) {
        store.owned.push(item_id.to_string());
        return Loot::New;
    }
    let spares = dict_get(&store.spares, item_id).copied().unwrap_or(0);
    dict_set(&mut store.spares, item_id, spares + 1);
    Loot::Spare
}

pub fn can_upgrade(profile: &AdventureProfile, item_id: &str) -> bool {
    profile.armoury.owned.iter().any(|i| i == item_id)
        && spares_of(profile, item_id) > 0
        && level_of(profile, item_id) < MAX_ITEM_LEVEL
}

fn take_spares(spares: &mut Dict<i64>, item_id: &str, count: i64) {
    let left = dict_get(spares, item_id).copied().unwrap_or(0) - count;
    if left <= 0 {
        dict_pop(spares, item_id);
    } else {
        dict_set(spares, item_id, left);
    }
}

/// Stack one spare onto the piece: +1 level.
pub fn upgrade(profile: &mut AdventureProfile, item_id: &str) -> bool {
    if !can_upgrade(profile, item_id) {
        return false;
    }
    let level = level_of(profile, item_id) + 1;
    take_spares(&mut profile.armoury.spares, item_id, 1);
    dict_set(&mut profile.armoury.levels, item_id, level);
    true
}

pub fn sell_price(item_id: &str) -> i64 {
    armor_by_id(item_id).map_or(0, |item| sell_price_of_tier(item.tier))
}

/// Sell up to `count` spares; returns the amber earned. The piece itself is
/// kept.
pub fn sell_spares(profile: &mut AdventureProfile, item_id: &str, count: i64) -> i64 {
    let count = count.max(0).min(spares_of(profile, item_id));
    if count <= 0 {
        return 0;
    }
    take_spares(&mut profile.armoury.spares, item_id, count);
    let earned = count * sell_price(item_id);
    profile.armoury.amber += earned;
    earned
}

/// Everyone who can wear armour: the hero, player two, then each companion.
pub fn party(profile: &AdventureProfile) -> Vec<String> {
    let mut who = vec![HERO.to_string(), PLAYER_TWO.to_string()];
    who.extend(profile.companions.iter().map(|(k, _)| k.clone()));
    who
}

/// Item id -> who wears it.
pub fn worn_by(profile: &AdventureProfile) -> Dict<String> {
    let mut worn = Vec::new();
    for who in party(profile) {
        for (_, item_id) in spider_progression(profile, &who).equipped.iter() {
            dict_set(&mut worn, item_id, who.clone());
        }
    }
    worn
}

/// Put a piece on `who`, taking it off whoever wore it before.
pub fn wear(profile: &mut AdventureProfile, who: &str, item_id: &str) -> bool {
    let everyone = party(profile);
    if armor_by_id(item_id).is_none()
        || !profile.armoury.owned.iter().any(|i| i == item_id)
        || !everyone.iter().any(|w| w == who)
    {
        return false;
    }
    for other in everyone.iter().filter(|o| *o != who) {
        let mut state = spider_progression(profile, other);
        if state.equipped.values().any(|i| i == item_id) {
            retain_equipped(&mut state, |i| i != item_id);
            store_progression(profile, other, state);
        }
    }
    let mut state = spider_progression(profile, who);
    state.equip(item_id);
    store_progression(profile, who, state);
    true
}

pub fn take_off(profile: &mut AdventureProfile, who: &str, slot: &str) -> bool {
    let mut state = spider_progression(profile, who);
    if !state.unequip(slot) {
        return false;
    }
    store_progression(profile, who, state);
    true
}

#[cfg(test)]
mod tests {
    //! Port of the armoury tests in tests/test_loot_and_campaign.py.
    use super::*;
    use crate::adventure_profile::{fresh_profile, hero_progression, unlock_companion};

    #[test]
    fn duplicates_stack_to_upgrade_up_to_five_levels() {
        let mut profile = fresh_profile();
        assert_eq!(add_loot(&mut profile, "warden_crest"), Loot::New);
        assert!(!can_upgrade(&profile, "warden_crest"));
        for _ in 0..6 {
            assert_eq!(add_loot(&mut profile, "warden_crest"), Loot::Spare);
        }
        let mut upgrades = 0;
        while upgrade(&mut profile, "warden_crest") {
            upgrades += 1;
        }
        assert_eq!(upgrades, 4);
        assert_eq!(level_of(&profile, "warden_crest"), MAX_ITEM_LEVEL);
        assert_eq!(spares_of(&profile, "warden_crest"), 2);
        assert_eq!(add_loot(&mut profile, "nope"), Loot::Unknown);
    }

    #[test]
    fn spares_sell_for_amber_and_the_piece_is_kept() {
        let mut profile = fresh_profile();
        add_loot(&mut profile, "frost_aegis");
        add_loot(&mut profile, "frost_aegis");
        add_loot(&mut profile, "frost_aegis");
        assert_eq!(sell_spares(&mut profile, "frost_aegis", 5), 2 * 70);
        assert_eq!(profile.armoury.amber, 140);
        assert!(profile.armoury.owned.iter().any(|i| i == "frost_aegis"));
        assert_eq!(spares_of(&profile, "frost_aegis"), 0);
        assert!(profile.armoury.spares.is_empty());
        assert_eq!(sell_spares(&mut profile, "frost_aegis", 1), 0);
    }

    #[test]
    fn armour_goes_on_one_spider_at_a_time_companions_included() {
        let mut profile = fresh_profile();
        unlock_companion(&mut profile, "weaver");
        assert!(wear(&mut profile, HERO, "silk_carapace"));
        assert_eq!(dict_get(&worn_by(&profile), "silk_carapace").map(String::as_str), Some(HERO));
        assert!(wear(&mut profile, "weaver", "silk_carapace"));
        assert_eq!(dict_get(&worn_by(&profile), "silk_carapace").map(String::as_str), Some("weaver"));
        assert!(hero_progression(&profile).equipped.is_empty());
        assert!(!wear(&mut profile, "dragon", "silk_carapace"));
        assert!(!wear(&mut profile, HERO, "sun_crown"), "not owned");
        assert!(take_off(&mut profile, "weaver", "carapace"));
        assert!(!take_off(&mut profile, "weaver", "carapace"));
        assert!(worn_by(&profile).is_empty());
        assert_eq!(party(&profile), vec![HERO, PLAYER_TWO, "scout", "weaver"]);
    }
}
