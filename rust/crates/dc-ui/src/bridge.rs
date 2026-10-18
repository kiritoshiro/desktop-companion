//! The mappings from the real content, state and adventure types onto the
//! windows' view models (the `TODO(content)` notes in [`crate::model`]).
//!
//! The app layer builds a window's data with these, so the settings window
//! shows exactly what dc-content discovers and the character window exactly
//! what the Adventure profile holds.

use std::collections::BTreeMap;
use std::path::Path;

use dc_adventure::adventure_profile::{self as profile_mod, AdventureProfile};
use dc_adventure::{campaign, custom_maps};
use dc_content::{discovery, enemy_kinds, jobs, palettes, personality_profiles, skills};
use dc_state::progression as prog;

use crate::model::*;

fn rgb(c: dc_core::Color) -> Rgb {
    [c.r, c.g, c.b]
}

fn color_map(map: &dc_content::ColorMap) -> BTreeMap<String, Rgb> {
    map.iter().map(|(k, v)| (k.to_owned(), rgb(*v))).collect()
}

fn file_added(path: Option<&str>) -> f64 {
    path.and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.created().or_else(|_| m.modified()).ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The settings window's catalogue from dc-content discovery under `root`
/// (`refresh_discovery`).
pub fn settings_catalog(root: Option<&Path>) -> SettingsCatalog {
    let (models, mut warnings) = discovery::discover_models(root);
    let (personalities, personality_warnings) = discovery::discover_personalities(root);
    warnings.extend(personality_warnings);
    let models = models
        .iter()
        .map(|(id, m)| {
            (
                id.to_owned(),
                ModelInfo {
                    id: m.id.clone(),
                    display_name: m.display_name.clone(),
                    body_plan: m.body_plan.clone().unwrap_or_default(),
                    colors: color_map(&m.colors),
                    enemy: enemy_kinds::is_enemy_model(&m.id),
                    added: file_added(m.path.as_deref()),
                },
            )
        })
        .collect();
    let personality_infos = personalities
        .iter()
        .map(|(id, p)| {
            let t = personality_profiles::temperament_for(p);
            (
                id.to_owned(),
                PersonalityInfo {
                    id: p.id.clone(),
                    display_name: p.display_name.clone(),
                    canonical: p.canonical,
                    description: p.description.clone().unwrap_or_default(),
                    temperament: vec![
                        ("energy".into(), t.energy),
                        ("curiosity".into(), t.curiosity),
                        ("boldness".into(), t.boldness),
                        ("sociability".into(), t.sociability),
                        ("patience".into(), t.patience),
                        ("caution".into(), t.caution),
                    ],
                    default_skills: skills::default_skills_for_personality(Some(p.into())),
                },
            )
        })
        .collect();
    let mut catalog = base_catalog();
    catalog.models = models;
    catalog.personalities = personality_infos;
    catalog.warnings = warnings;
    catalog
}

/// The registry-only part of the catalogue: skills, jobs, palettes, plans.
pub fn base_catalog() -> SettingsCatalog {
    SettingsCatalog {
        models: BTreeMap::new(),
        personalities: BTreeMap::new(),
        compact_temperaments: personality_profiles::COMPACT_TEMPERAMENT_IDS.iter().map(|s| (*s).to_owned()).collect(),
        skills: skills::SKILLS
            .iter()
            .map(|s| SkillInfo {
                id: s.id.into(),
                display_name: s.display_name.into(),
                category: s.category.into(),
                description: s.description.into(),
                short_label: skills::short_label(s.id).unwrap_or(s.id).into(),
            })
            .collect(),
        common_skill_ids: skills::common_skill_ids().into_iter().map(String::from).collect(),
        jobs: jobs::JOB_DEFINITIONS
            .iter()
            .map(|j| JobInfo {
                id: j.id.into(),
                display_name: j.display_name.into(),
                description: j.description.into(),
                ability_ids: j.ability_ids.iter().map(|a| (*a).to_owned()).collect(),
            })
            .collect(),
        palettes: palettes::named_palettes()
            .iter()
            .map(|p| PaletteInfo { id: p.id.into(), name: p.name.into(), colors: color_map(&p.colors) })
            .collect(),
        body_plans: dc_content::body_plans::BODY_PLAN_IDS.iter().map(|s| (*s).to_owned()).collect(),
        enemy_body_plans: enemy_kinds::ENEMY_BODY_PLANS.iter().map(|s| (*s).to_owned()).collect(),
        label_switch_defaults: dc_content::preset::LABEL_SWITCH_DEFAULTS
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect(),
        warnings: Vec::new(),
    }
}

/// `refresh_presets`: the library, labelled by each preset's own name.
pub fn presets(root: Option<&Path>) -> Vec<PresetEntry> {
    discovery::discover_presets(root)
        .into_iter()
        .map(|path| {
            let label = dc_content::preset_io::load_preset_value(&path)
                .ok()
                .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned))
                .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            PresetEntry { label, path: path.to_string_lossy().into_owned() }
        })
        .collect()
}

/// The ability tree, armour catalogue and sets of dc-state.
pub fn progression_catalog() -> ProgressionCatalog {
    let effects = |e: &[(&str, f64)]| e.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect::<Vec<_>>();
    ProgressionCatalog {
        tree: prog::ABILITY_TREE
            .iter()
            .map(|n| SkillNode {
                id: n.id.into(),
                name: n.name.into(),
                description: n.description.into(),
                level_required: n.level_required as u32,
                cost: n.cost as u32,
                prerequisites: n.prerequisites.iter().map(|p| (*p).to_owned()).collect(),
                effects: effects(n.effects),
            })
            .collect(),
        armour: prog::ARMOR_CATALOG.iter().map(armour_item).collect(),
        sets: prog::ARMOR_SETS
            .iter()
            .map(|s| ArmourSet {
                id: s.id.into(),
                name: s.name.into(),
                description: s.description.into(),
                effects: effects(s.effects),
            })
            .collect(),
        web_energy: dc_adventure::PlayerController::WEB_ENERGY,
        jump_energy: dc_adventure::PlayerController::JUMP_ENERGY,
    }
}

pub fn armour_item(i: &prog::ArmorItem) -> ArmourItem {
    ArmourItem {
        id: i.id.into(),
        name: i.name.into(),
        slot: i.slot.into(),
        description: i.description.into(),
        armor: i.armor,
        max_hp: i.max_hp,
        max_energy: i.max_energy,
        damage: i.damage,
        speed: i.speed,
        set_id: i.set_id.into(),
        tier: i.tier.into(),
    }
}

impl From<&prog::ProgressionState> for Progression {
    fn from(s: &prog::ProgressionState) -> Self {
        Progression {
            level: s.level.max(1) as u32,
            xp: s.xp.max(0) as u32,
            total_xp: s.total_xp.max(0) as u32,
            skill_points: s.skill_points.max(0) as u32,
            unlocked_abilities: s.unlocked_abilities.clone(),
            inventory: s.inventory.clone(),
            equipped: s.equipped.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect(),
            team_id: s.team_id.clone(),
            item_levels: s.item_levels.iter().map(|(k, v)| (k.to_owned(), (*v).max(1) as u32)).collect(),
        }
    }
}

impl From<&dc_adventure::ControlSettings> for ControlSettings {
    fn from(c: &dc_adventure::ControlSettings) -> Self {
        ControlSettings::from_json(&c.to_dict())
    }
}

impl From<&ControlSettings> for dc_adventure::ControlSettings {
    fn from(c: &ControlSettings) -> Self {
        dc_adventure::ControlSettings::from_dict(&c.to_json())
    }
}

/// The Adventure page from the profile (`refresh_adventure`). `screens`
/// comes from dc-platform's monitor list.
pub fn adventure_view(
    profile: &AdventureProfile,
    state_dir: &Path,
    screens: Vec<ScreenInfo>,
    controls: &dc_adventure::ControlSettings,
) -> AdventureView {
    let hero = profile_mod::hero_progression(profile);
    let record = |id: &str| {
        let r = profile_mod::mission_record(profile, id);
        MissionRecord {
            victories: r.victories.max(0) as u32,
            defeats: r.defeats.max(0) as u32,
            best_seconds: r.best_seconds,
        }
    };
    let mut maps: Vec<MapCard> = campaign::MAPS
        .iter()
        .map(|m| MapCard {
            id: m.id.clone(),
            title: m.title.clone(),
            blurb: m.blurb.clone(),
            kind: m.kind.id().into(),
            unlock_after: m.unlock_after.clone(),
            playable: true,
            record: record(&m.id),
        })
        .collect();
    maps.push(MapCard {
        id: "burrow".into(),
        title: "Hold the burrow".into(),
        blurb: "Wave after wave comes for your home. Keep it.".into(),
        kind: "raid".into(),
        unlock_after: None,
        playable: false,
        record: MissionRecord::default(),
    });
    let custom = custom_maps::list_maps(state_dir)
        .into_iter()
        .map(|data| CustomMapCard {
            victories: record(&data.id).victories,
            playable: custom_maps::problems(&data).is_empty(),
            id: data.id.clone(),
            title: data.title.clone(),
            blurb: data.blurb.clone(),
            kind: "raid".into(),
            tier: data.tier.max(1) as u32,
            buildings: data.buildings.len(),
        })
        .collect();
    AdventureView {
        hero_name: profile.name.clone(),
        level: hero.level.max(1) as u32,
        skill_points: hero.skill_points.max(0) as u32,
        companions: profile.companions.len(),
        amber: profile.armoury.amber,
        admin: profile.admin,
        two_player: profile.two_player,
        all_screens: profile.all_screens,
        disabled_screens: profile.disabled_screens.clone(),
        maps,
        custom_maps: custom,
        screens,
        controls: controls.into(),
    }
}

/// The party and armoury the character window dresses.
pub fn party_view(profile: &AdventureProfile) -> PartyView {
    let mut members = vec![PartyMember {
        who: profile_mod::HERO.into(),
        name: profile.name.clone(),
        progression: (&profile_mod::spider_progression(profile, profile_mod::HERO)).into(),
    }];
    if profile.two_player || profile.player_two.progression.is_some() {
        members.push(PartyMember {
            who: profile_mod::PLAYER_TWO.into(),
            name: profile.player_two.name.clone(),
            progression: (&profile_mod::spider_progression(profile, profile_mod::PLAYER_TWO)).into(),
        });
    }
    for (id, entry) in &profile.companions {
        members.push(PartyMember {
            who: id.clone(),
            name: if entry.name.is_empty() { id.clone() } else { entry.name.clone() },
            progression: (&profile_mod::spider_progression(profile, id)).into(),
        });
    }
    PartyView {
        members,
        two_player: profile.two_player,
        amber: profile.armoury.amber,
        owned: profile.armoury.owned.clone(),
        spares: profile.armoury.spares.iter().map(|(k, v)| (k.clone(), (*v).max(0) as u32)).collect(),
        levels: profile.armoury.levels.iter().map(|(k, v)| (k.clone(), (*v).max(1) as u32)).collect(),
        sell_prices: campaign::SELL_PRICES.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
    }
}

/// `(id, name)` of every companion (`campaign.COMPANIONS`).
pub fn companions() -> Vec<(String, String)> {
    campaign::COMPANIONS.iter().map(|c| (c.id.to_owned(), c.name.to_owned())).collect()
}

/// `(id, name, boss)` of every enemy kind.
pub fn enemy_kind_list() -> Vec<(String, String, bool)> {
    enemy_kinds::enemy_kinds().iter().map(|k| (k.id.to_owned(), k.name.to_owned(), k.boss)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registries_match_the_sample_copies() {
        let real = base_catalog();
        let sample = crate::sample::settings_catalog();
        assert_eq!(real.skills, sample.skills);
        assert_eq!(real.jobs, sample.jobs);
        assert_eq!(real.common_skill_ids, sample.common_skill_ids);
        assert_eq!(real.palettes, sample.palettes);
        assert_eq!(real.body_plans, sample.body_plans);
        assert_eq!(real.label_switch_defaults, sample.label_switch_defaults);
        let p = progression_catalog();
        let s = crate::sample::progression_catalog();
        assert_eq!(p.tree, s.tree);
        assert_eq!(p.armour, s.armour);
        assert_eq!(p.sets, s.sets);
        assert_eq!((p.web_energy, p.jump_energy), (s.web_energy, s.jump_energy));
        assert_eq!(enemy_kind_list(), crate::sample::enemy_kinds());
        assert_eq!(companions(), crate::sample::companions());
    }

    #[test]
    fn discovery_finds_the_shipped_models_and_temperaments() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog = settings_catalog(Some(&root));
        assert!(catalog.models.contains_key("tarantula"), "{:?}", catalog.warnings);
        assert_eq!(catalog.models["tarantula"].body_plan, "tarantula");
        assert!(catalog.models.values().any(|m| m.enemy));
        assert!(catalog.personalities.contains_key("balanced"));
        let library = presets(Some(&root));
        assert!(library.iter().any(|p| p.label == "Default"), "{library:?}");
    }

    #[test]
    fn dc_ui_teams_agree_with_dc_state() {
        use serde_json::json;
        let raw = json!({"porch_guard": {"name": "Porch guard"}, "shed": "Shed crew"});
        let used = ["rivals".to_owned(), "pack_a".to_owned(), "midnight_crew".to_owned()];
        let mine = crate::teams::normalize_teams(Some(&raw), &used);
        let theirs = dc_state::teams::normalize_teams(Some(&raw), &used);
        assert_eq!(mine.len(), theirs.len());
        for (id, p) in theirs.iter() {
            assert_eq!(mine[id].name, p.name, "{id}");
            assert_eq!(mine[id].color, rgb(p.color), "{id}");
        }
        assert_eq!(crate::teams::HOSTILITY_NOTE, dc_state::teams::HOSTILITY_NOTE);
    }

    #[test]
    fn controls_convert_both_ways() {
        let real = dc_adventure::ControlSettings { aim_cone: 120, ..Default::default() };
        let view: ControlSettings = (&real).into();
        assert_eq!(view.aim_cone, 120);
        assert_eq!(view.binding("move_up"), real.binding(dc_adventure::Action::from_id("move_up").unwrap()));
        let back: dc_adventure::ControlSettings = (&view).into();
        assert_eq!(back.to_dict(), real.to_dict());
    }

    #[test]
    fn a_fresh_profile_maps_onto_the_views() {
        let profile = profile_mod::fresh_profile();
        let dir = std::env::temp_dir().join("dc-ui-bridge-test-none");
        let view = adventure_view(&profile, &dir, Vec::new(), &dc_adventure::ControlSettings::default());
        assert_eq!(view.hero_name, profile.name);
        assert!(view.map_unlocked("territory"));
        assert!(!view.map_unlocked("queen"));
        let party = party_view(&profile);
        assert_eq!(party.members[0].who, HERO);
        assert_eq!(party.sell_prices["legendary"], 160);
    }
}
