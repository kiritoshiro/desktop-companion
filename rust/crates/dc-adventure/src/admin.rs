//! Port of the data functions of app/admin_ui.py (the dialog itself is
//! dc-ui's).
//!
//! Admin mode: the owner's switches for testing Adventure.
//!
//! - **Admin mode**: every map is open, whatever has been won.
//! - **Reset my Adventure progress**: the hero, companions, armoury, amber
//!   and map results start again. Admin mode and the screen choices stay.
//! - Quick gifts: amber, every armour piece, every companion, a hero level.
//!
//! Everything here reads and writes the profile file at `path`.

use std::path::Path;

use crate::adventure_profile::{
    AdventureProfile, HERO, fresh_profile, hero_progression, load_profile, save_profile, store_progression,
    unlock_companion,
};
use crate::armoury::{Loot, add_loot};
use crate::campaign::COMPANIONS;
use crate::progression::{ARMOR_CATALOG, MAX_LEVEL};

pub fn set_admin(path: &Path, on: bool) -> bool {
    let mut profile = load_profile(path);
    profile.admin = on;
    save_profile(&profile, path)
}

/// A new Adventure profile, keeping only the player's settings.
pub fn reset_progress(path: &Path) -> AdventureProfile {
    let old = load_profile(path);
    let profile = AdventureProfile {
        admin: old.admin,
        all_screens: old.all_screens,
        disabled_screens: old.disabled_screens,
        ..fresh_profile()
    };
    save_profile(&profile, path);
    profile
}

/// Add amber; returns the new total.
pub fn give_amber(path: &Path, amount: i64) -> i64 {
    let mut profile = load_profile(path);
    profile.armoury.amber += amount;
    save_profile(&profile, path);
    profile.armoury.amber
}

/// Every piece not yet owned. Pieces already owned get no spare: a spare is
/// an upgrade, and the gift should not hand out upgrades unasked.
pub fn give_all_armour(path: &Path) -> usize {
    let mut profile = load_profile(path);
    let owned = profile.armoury.owned.clone();
    let added = ARMOR_CATALOG
        .iter()
        .filter(|item| !owned.iter().any(|o| o == item.id))
        .filter(|item| add_loot(&mut profile, item.id) == Loot::New)
        .count();
    save_profile(&profile, path);
    added
}

pub fn unlock_all_companions(path: &Path) -> usize {
    let mut profile = load_profile(path);
    let added = COMPANIONS.iter().filter(|c| unlock_companion(&mut profile, c.id)).count();
    save_profile(&profile, path);
    added
}

/// Levels gained bring their skill points, as if earned; levels taken away
/// take only points still unspent.
pub fn set_hero_level(path: &Path, level: i64) -> i64 {
    let mut profile = load_profile(path);
    let mut state = hero_progression(&profile);
    let level = level.clamp(1, MAX_LEVEL);
    state.skill_points = (state.skill_points + level - state.level).max(0);
    state.level = level;
    state.xp = 0;
    store_progression(&mut profile, HERO, state);
    save_profile(&profile, path);
    level
}

#[cfg(test)]
mod tests {
    //! Port of the admin tests in tests/test_editor_and_admin.py.
    use super::*;
    use crate::adventure_profile::{profile_path, record_result};
    use crate::campaign::{chosen_map, map_unlocked};

    fn state(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dc-admin-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn admin_mode_opens_every_map() {
        let dir = state("open");
        let path = profile_path(&dir);
        save_profile(&fresh_profile(), &path);
        assert!(!map_unlocked(&Vec::new(), "queen", false, &dir));
        set_admin(&path, true);
        let mut profile = load_profile(&path);
        assert!(profile.admin && map_unlocked(&Vec::new(), "queen", true, &dir));
        profile.selected_map = "queen".into();
        save_profile(&profile, &path);
        assert_eq!(chosen_map(&load_profile(&path), &dir).id, "queen");
    }

    #[test]
    fn reset_progress_starts_again_but_keeps_the_settings() {
        let dir = state("reset");
        let path = profile_path(&dir);
        let mut profile = fresh_profile();
        record_result(&mut profile, "territory", true, 90.0);
        profile.admin = true;
        profile.disabled_screens = vec!["\\\\.\\DISPLAY2".into()];
        profile.armoury.amber = 500;
        save_profile(&profile, &path);
        reset_progress(&path);
        let after = load_profile(&path);
        assert!(after.missions.is_empty() && after.armoury.amber == 0);
        assert!(after.admin && after.disabled_screens == vec!["\\\\.\\DISPLAY2".to_string()]);
    }

    #[test]
    fn the_admin_gifts() {
        let dir = state("gifts");
        let path = profile_path(&dir);
        save_profile(&fresh_profile(), &path);
        assert_eq!(give_amber(&path, 1000), 1000);
        assert!(give_all_armour(&path) > 20);
        assert!(load_profile(&path).armoury.spares.is_empty(), "no spares, so no free upgrades");
        assert_eq!(give_all_armour(&path), 0);
        assert!(load_profile(&path).armoury.spares.is_empty());
        assert!(unlock_all_companions(&path) >= 3);
        assert_eq!(set_hero_level(&path, 12), 12);
        let hero = hero_progression(&load_profile(&path));
        assert!(hero.level == 12 && hero.skill_points >= 11);
    }
}
