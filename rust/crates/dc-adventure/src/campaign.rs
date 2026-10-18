//! Port of app/campaign.py
//!
//! The Adventure campaign: maps, companions, what enemies wear and what they
//! drop. Pure data and small rules: the mission, the Adventure page and the
//! character window all read it.
//!
//! `find_map`, `chosen_map` and `map_unlocked` also look at maps made in the
//! editor, so they take the state folder those live under.

use std::path::Path;
use std::sync::LazyLock;

use dc_core::PyRandom;

use crate::adventure_profile::{AdventureProfile, MissionRecord};
use crate::custom_maps;
use crate::progression::{ARMOR_CATALOG, ARMOR_TIERS, ArmorItem, Dict, MAX_ITEM_LEVEL, armor_set, dict_get};

/// "raid": the woodland buildings; "reclaim": the frozen desktop itself;
/// "swarm": the fly levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapKind {
    Raid,
    Reclaim,
    Swarm,
}

impl MapKind {
    pub fn id(self) -> &'static str {
        match self {
            MapKind::Raid => "raid",
            MapKind::Reclaim => "reclaim",
            MapKind::Swarm => "swarm",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapInfo {
    pub id: String,
    pub title: String,
    pub blurb: String,
    /// 1 = first map; later maps are tougher.
    pub tier: i64,
    /// The map that must be won first.
    pub unlock_after: Option<String>,
    /// Armour qualities enemies wear, and so drop.
    pub enemy_tiers: Vec<String>,
    /// The boss at the Thorn nest.
    pub guardian: String,
    /// The armour set it wears complete.
    pub guardian_set: String,
    /// Joins you the first time the map is won.
    pub reward_companion: Option<String>,
    pub kind: MapKind,
}

#[allow(clippy::too_many_arguments)]
fn map(
    id: &str,
    title: &str,
    blurb: &str,
    tier: i64,
    unlock_after: Option<&str>,
    enemy_tiers: &[&str],
    guardian: &str,
    guardian_set: &str,
    reward_companion: Option<&str>,
    kind: MapKind,
) -> MapInfo {
    MapInfo {
        id: id.into(),
        title: title.into(),
        blurb: blurb.into(),
        tier,
        unlock_after: unlock_after.map(str::to_string),
        enemy_tiers: enemy_tiers.iter().map(|s| s.to_string()).collect(),
        guardian: guardian.into(),
        guardian_set: guardian_set.into(),
        reward_companion: reward_companion.map(str::to_string),
        kind,
    }
}

/// The built-in maps, in campaign order.
pub static MAPS: LazyLock<Vec<MapInfo>> = LazyLock::new(|| {
    use MapKind::*;
    vec![
        map(
            "territory",
            "Take back the desktop",
            "Capture Food or Silk, seal the Hatchery, survive the counterattack and claim Thorn nest.",
            1,
            None,
            &["common", "uncommon"],
            "Thorn guardian",
            "forager",
            Some("weaver"),
            Raid,
        ),
        map(
            "swarm",
            "Fly swarm",
            "Flies pour from nests on every screen. Pin them with silk and eat them before the rivals do.",
            1,
            None,
            &["common"],
            "Swarm",
            "forager",
            None,
            Swarm,
        ),
        map(
            "ember",
            "Ember hollow",
            "Raiders in bronze hold the hollow. Their warden wears the full Warden plate.",
            2,
            Some("territory"),
            &["uncommon", "rare"],
            "Hollow warden",
            "warden",
            Some("hunter"),
            Raid,
        ),
        map(
            "reclaim",
            "Reclaim the desktop",
            "Your desktop is frozen and infested. Destroy the nest on every screen before the acid eats it. \
             Esc gives it back at once.",
            2,
            Some("territory"),
            &["uncommon", "rare"],
            "The Devourer",
            "brood",
            None,
            Reclaim,
        ),
        map(
            "storm",
            "Fly storm",
            "A storm of fast flies, more rivals, less time. Rival dens wait on your other screens.",
            3,
            Some("ember"),
            &["rare", "epic"],
            "Storm",
            "brood",
            None,
            Swarm,
        ),
        map(
            "obsidian",
            "Obsidian deep",
            "Rune-cut raiders and a brood matriarch in black glass.",
            3,
            Some("ember"),
            &["rare", "epic"],
            "Brood matriarch",
            "brood",
            Some("sentinel"),
            Raid,
        ),
        map(
            "queen",
            "Queen of thorns",
            "The nest mother herself, in Sunforged regalia. Her guard wear crystal and obsidian.",
            4,
            Some("obsidian"),
            &["epic", "legendary"],
            "Queen of thorns",
            "sun",
            None,
            Raid,
        ),
    ]
});

/// `MAP_BY_ID.get(id)`.
pub fn map_by_id(id: &str) -> Option<&'static MapInfo> {
    MAPS.iter().find(|m| m.id == id)
}

pub const DEFAULT_MAP: &str = "territory";

#[derive(Debug, Clone, PartialEq)]
pub struct CompanionInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// How it fights: "scout" bites, "weaver" shoots silk, "hunter" pounces.
    pub style: &'static str,
    pub blurb: &'static str,
}

pub const COMPANIONS: [CompanionInfo; 4] = [
    CompanionInfo {
        id: "scout",
        name: "Scout",
        style: "scout",
        blurb: "Your first companion. Follows, guards, bites.",
    },
    CompanionInfo {
        id: "weaver",
        name: "Silk weaver",
        style: "weaver",
        blurb: "Keeps its distance and pins foes with silk.",
    },
    CompanionInfo { id: "hunter", name: "Hunter", style: "hunter", blurb: "Closes in with pounces." },
    CompanionInfo { id: "sentinel", name: "Sentinel", style: "scout", blurb: "A heavy biter that holds the line." },
];

/// `COMPANION_BY_ID.get(id)`.
pub fn companion_by_id(id: &str) -> Option<&'static CompanionInfo> {
    COMPANIONS.iter().find(|c| c.id == id)
}

pub const STARTING_COMPANIONS: [&str; 1] = ["scout"];
/// How many companions come on a raid, first unlocked first.
pub const PARTY_SIZE: usize = 3;

/// Amber paid for one spare piece, by quality.
pub const SELL_PRICES: [(&str, i64); 5] =
    [("common", 5), ("uncommon", 12), ("rare", 30), ("epic", 70), ("legendary", 160)];
/// Chance an enemy drops one of the pieces it wore when it dies.
pub const DROP_CHANCE: f64 = 0.35;

/// `SELL_PRICES.get(tier, 0)`.
pub fn sell_price_of_tier(tier: &str) -> i64 {
    SELL_PRICES.iter().find(|(t, _)| *t == tier).map_or(0, |(_, p)| *p)
}

/// A built-in map, or one made in the map editor.
pub fn find_map(map_id: &str, state_dir: &Path) -> Option<MapInfo> {
    if let Some(info) = map_by_id(map_id) {
        return Some(info.clone());
    }
    custom_maps::load_map(map_id, state_dir).map(|data| custom_maps::map_info(&data))
}

/// The map the next raid is played on: the one chosen on the Adventure page,
/// or the first when that one is unknown or still locked.
pub fn chosen_map(profile: &AdventureProfile, state_dir: &Path) -> MapInfo {
    let mut chosen = profile.selected_map.as_str();
    if !map_unlocked(&profile.missions, chosen, profile.admin, state_dir) {
        chosen = DEFAULT_MAP;
    }
    find_map(chosen, state_dir).unwrap_or_else(|| map_by_id(DEFAULT_MAP).expect("the first map").clone())
}

/// `records`: mission id -> record from the profile. Your own maps are always
/// open; in admin mode every map is.
pub fn map_unlocked(records: &Dict<MissionRecord>, map_id: &str, admin: bool, state_dir: &Path) -> bool {
    let Some(info) = find_map(map_id, state_dir) else {
        return false;
    };
    let Some(after) = info.unlock_after.as_deref().filter(|_| !admin) else {
        return true;
    };
    dict_get(records, after).is_some_and(|r| r.victories > 0)
}

/// The catalogue pieces of the given qualities, in catalogue order.
pub fn pieces_of_tiers(tiers: &[String]) -> Vec<&'static ArmorItem> {
    ARMOR_CATALOG.iter().filter(|i| tiers.iter().any(|t| t == i.tier)).collect()
}

/// (armour ids to wear, their level) for a mission enemy.
///
/// The guardian wears its map's set complete, a level above its guard;
/// everyone else a few pieces of the map's qualities, more and better on
/// later maps. At most one per slot.
pub fn enemy_loadout(info: &MapInfo, role: &str, rng: &mut PyRandom) -> (Vec<String>, i64) {
    let level = info.tier.clamp(1, MAX_ITEM_LEVEL);
    if role == "guardian" {
        let pieces = armor_set(&info.guardian_set).map(|s| s.pieces()).unwrap_or_default();
        return (pieces.into_iter().map(str::to_string).collect(), MAX_ITEM_LEVEL.min(level + 1));
    }
    // Nothing on the first maps, a piece or two on the second, most of a
    // suit by the last.
    let (low, high) = match info.tier {
        1 => (0, 0),
        2 => (0, 2),
        3 => (2, 3),
        4 => (3, 5),
        _ => (1, 3),
    };
    let mut pool = pieces_of_tiers(&info.enemy_tiers);
    rng.shuffle(&mut pool);
    let wanted = rng.randint(low, high) as usize;
    let mut chosen: Vec<String> = Vec::new();
    let mut slots: Vec<&str> = Vec::new();
    for item in pool {
        if chosen.len() >= wanted {
            break;
        }
        if !slots.contains(&item.slot) {
            chosen.push(item.id.to_string());
            slots.push(item.slot);
        }
    }
    (chosen, (level - 1).max(1))
}

/// What a dead enemy leaves behind: one worn piece, sometimes; a guardian
/// always leaves two.
pub fn roll_drop(worn: &[String], guardian: bool, rng: &mut PyRandom) -> Vec<String> {
    if worn.is_empty() {
        return Vec::new();
    }
    if guardian {
        return rng.sample(worn, worn.len().min(2));
    }
    if rng.random() < DROP_CHANCE { vec![rng.choice(worn).clone()] } else { Vec::new() }
}

/// The quality's place in [`ARMOR_TIERS`]; 0 when unknown.
pub fn tier_rank(tier: &str) -> usize {
    ARMOR_TIERS.iter().position(|t| *t == tier).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Port of the campaign parts of tests/test_loot_and_campaign.py and
    //! tests/test_mission_variety.py.
    use super::*;
    use crate::map_layouts::{BuildingKind, layout};
    use crate::progression::armor_by_id;
    use std::collections::HashSet;

    #[test]
    fn every_map_names_a_boss_set_and_later_maps_are_tougher() {
        let mut last = 0;
        for info in MAPS.iter() {
            assert!(armor_set(&info.guardian_set).is_some(), "{}", info.id);
            assert!(info.tier >= 1);
            if let Some(after) = &info.unlock_after {
                assert!(map_by_id(after).unwrap().tier < info.tier);
            }
            last = last.max(info.tier);
        }
        assert_eq!(map_by_id("queen").unwrap().tier, last);
    }

    #[test]
    fn enemies_wear_armour_of_their_maps_quality() {
        let mut rng = PyRandom::from_int(1);
        for info in MAPS.iter() {
            for _ in 0..20 {
                let (worn, _) = enemy_loadout(info, "guard", &mut rng);
                let slots: HashSet<_> = worn.iter().map(|i| armor_by_id(i).unwrap().slot).collect();
                assert_eq!(slots.len(), worn.len(), "one per slot");
                assert!(worn.iter().all(|i| info.enemy_tiers.iter().any(|t| t == armor_by_id(i).unwrap().tier)));
            }
        }
    }

    #[test]
    fn the_first_maps_enemies_wear_no_armour_and_later_ones_more() {
        let mut rng = PyRandom::from_int(2);
        let (first, last) = (map_by_id("territory").unwrap(), map_by_id("queen").unwrap());
        for role in ["guard", "weaver", "hunter"] {
            for _ in 0..30 {
                assert!(enemy_loadout(first, role, &mut rng).0.is_empty());
            }
        }
        assert!((0..30).map(|_| enemy_loadout(last, "guard", &mut rng).0.len()).min().unwrap() >= 3);
        let ember = map_by_id("ember").unwrap();
        assert!((0..60).map(|_| enemy_loadout(ember, "guard", &mut rng).0.len()).max().unwrap() <= 2);
    }

    #[test]
    fn bosses_wear_their_whole_set_a_level_above_their_guard() {
        let mut rng = PyRandom::from_int(3);
        for info in MAPS.iter() {
            let (mut worn, level) = enemy_loadout(info, "guardian", &mut rng);
            let mut pieces: Vec<String> =
                armor_set(&info.guardian_set).unwrap().pieces().into_iter().map(String::from).collect();
            worn.sort();
            pieces.sort();
            assert_eq!(worn, pieces);
            let (_, guard_level) = enemy_loadout(info, "guard", &mut rng);
            assert!(level > guard_level || level == MAX_ITEM_LEVEL);
        }
    }

    #[test]
    fn a_guardian_always_leaves_two_pieces() {
        let mut rng = PyRandom::from_int(4);
        let worn: Vec<String> = ["sun_crown", "sun_aegis", "sun_scales"].iter().map(|s| s.to_string()).collect();
        for _ in 0..20 {
            assert_eq!(roll_drop(&worn, true, &mut rng).len(), 2);
            assert!(roll_drop(&worn, false, &mut rng).len() <= 1);
        }
        assert!(roll_drop(&[], true, &mut rng).is_empty());
        assert_eq!(tier_rank("legendary"), 4);
        assert_eq!(tier_rank("??"), 0);
        assert_eq!(sell_price_of_tier("rare"), 30);
    }

    #[test]
    fn every_raid_map_has_its_own_layout_with_one_home_hatchery_and_nest() {
        let raids: Vec<&MapInfo> = MAPS.iter().filter(|m| m.kind == MapKind::Raid).collect();
        let mut shapes = HashSet::new();
        for info in &raids {
            let places = layout(&info.id).expect("a raid map has a layout");
            let kinds: Vec<BuildingKind> = places.iter().map(|p| p.kind).collect();
            assert_eq!(kinds[0], BuildingKind::Home);
            for k in [BuildingKind::Home, BuildingKind::Hatchery, BuildingKind::Nest] {
                assert_eq!(kinds.iter().filter(|x| **x == k).count(), 1);
            }
            let shape: Vec<String> = places.iter().map(|p| format!("{:?}{}{}{}", p.kind, p.fx, p.fy, p.far)).collect();
            shapes.insert(shape);
        }
        assert_eq!(shapes.len(), raids.len(), "no two maps are laid out alike");
    }

    #[test]
    fn later_maps_bring_the_new_buildings() {
        use BuildingKind::*;
        let new = [Venom, Lookout, Nursery, Amber];
        for id in ["ember", "obsidian", "queen"] {
            assert!(layout(id).unwrap().iter().any(|p| new.contains(&p.kind)), "{id}");
        }
        let queen: HashSet<_> = layout("queen").unwrap().iter().map(|p| p.kind).collect();
        assert!(new.iter().all(|k| queen.contains(k)));
    }
}
