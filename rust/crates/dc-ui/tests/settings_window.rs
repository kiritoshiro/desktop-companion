//! The settings window, headless. Ports the intent of test_settings_table,
//! test_settings_consolidation, test_settings_polish, test_settings_layout,
//! test_reset_progress (the window side), test_team_identity's round trip,
//! and test_adventure_polish's "the randomize row is gone".

mod common;

use common::Harness;
use dc_ui::windows::config::{
    ConfigWindow, RANDOM_CATEGORY_ID, RANDOM_MODEL_ID, SlotSpec, TEAM_COMBO_WIDTH, TEAM_ICON_SIZE,
};
use dc_ui::windows::mode_menu::Page;
use dc_ui::{OverlayMode, UiAction, sample};
use serde_json::{Value, json};

fn window() -> ConfigWindow {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.load_preset(&sample::default_preset());
    w.shell.page = Page::Companion;
    w
}

fn run(h: &mut Harness, w: &mut ConfigWindow) -> Vec<UiAction> {
    h.settle(&mut |ctx| w.ui(ctx))
}

#[test]
fn the_version_is_in_the_title() {
    assert_eq!(ConfigWindow::title(), format!("Desktop Bug Companion {}", dc_core::VERSION));
}

#[test]
fn every_section_has_an_illustrated_purpose_heading() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    for (title, purpose) in [
        ("Saved colony setups", "Name, load, save, or refresh a complete spider configuration."),
        ("Colony roster", "Each slot sets a spider's body, temperament, job, team, and abilities."),
        ("Factions & relations", "Choose who lives together and who competes across the colony."),
        ("Desktop behavior", "Set spider size, dragging, and the labels that stay visible."),
        ("Prey & food", "Release flies for hunters to chase, trap, and eat."),
        ("Ready the colony", "Save this setup, then start or stop the desktop overlay."),
    ] {
        h.assert_label(title);
        h.assert_label(purpose);
        h.assert_label(&format!("{title}. {purpose}"));
        h.assert_label(&format!("{title} symbol"));
    }
}

#[test]
fn the_randomize_row_and_the_three_overrides_are_gone() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    for gone in ["Randomize:", "Surprise me", "Mood:", "Movement:", "Social play"] {
        assert!(!h.has_label(gone), "{gone} should be gone");
    }
    for switch in ["Names", "Levels", "Health", "XP", "Stamina", "Draggable"] {
        assert!(h.toggled(switch).is_some(), "switch {switch}");
    }
}

#[test]
fn the_skills_button_counts_what_can_be_selected() {
    let w = window();
    // Slot 0 chose only web_walk; slot 2 follows its temperament (web walk).
    assert_eq!(w.skills_label(0).0, "1 of 5");
    assert!(w.skills_label(0).1.starts_with("web walk."));
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    h.assert_label("1 of 5");
}

#[test]
fn the_count_matches_the_dialog_that_opens() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    h.click("slot0:skills", &mut |ctx| w.ui(ctx));
    assert!(w.dialog_open());
    run(&mut h, &mut w);
    for id in ["drift", "weave_web", "web_walk", "shoot_web", "wall_web"] {
        h.probe(&format!("ability:{id}"));
    }
    assert_eq!(h.toggled("Walk on webs"), Some(true));
    assert_eq!(h.toggled("Shoot trapping web"), Some(false));
    h.click("ability:shoot_web", &mut |ctx| w.ui(ctx));
    h.click("dialog:OK", &mut |ctx| w.ui(ctx));
    assert!(!w.dialog_open());
    assert_eq!(w.skills_label(0).0, "2 of 5");
    let slots = w.collect_slots();
    assert_eq!(slots[0]["abilities"], json!(["web_walk", "shoot_web"]));
}

#[test]
fn the_team_column_shows_a_colour_and_no_words() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    w.load_preset(&sample::colony_preset());
    run(&mut h, &mut w);
    h.assert_label("Team: Porch guard");
    h.assert_label("Team: Rivals");
    h.assert_label("Team: Neutral / solo");
    let combo = h.probe("slot0:team");
    let icon = h.probe("slot0:team-icon");
    assert!(combo.width() >= TEAM_COMBO_WIDTH, "team combo is only {} px wide", combo.width());
    assert!((icon.width() - TEAM_ICON_SIZE).abs() < 0.1, "team swatch is {} px wide", icon.width());
    assert!((icon.height() - TEAM_ICON_SIZE).abs() < 0.1, "team swatch is {} px high", icon.height());
    assert!(
        combo.right() - icon.right() >= 20.0,
        "team swatch leaves only {} px for the arrow and spacing",
        combo.right() - icon.right()
    );
    assert!(w.team_tooltip(0).starts_with("Team: Porch guard."));
    assert!(w.team_tooltip(0).contains("Foes fight."));
    assert_eq!(dc_ui::teams::default_color("neutral"), [255, 255, 255]);
}

#[test]
fn the_label_switches_round_trip_and_the_retired_overrides_are_dropped() {
    let mut w = window();
    let preset = w.current_preset_data().unwrap();
    let settings = &preset["settings"];
    for retired in ["mood_mode", "social_play", "gait_style"] {
        assert!(settings.get(retired).is_none(), "{retired} dropped");
    }
    // Unknown settings survive a save (mouse_capture has no widget).
    assert_eq!(settings["mouse_capture"], json!(true));
    // Defaults from LABEL_SWITCH_DEFAULTS where the preset says nothing.
    assert_eq!(settings["always_show_names"], json!(true));
    assert_eq!(settings["always_show_xp"], json!(false));
    w.label_switches.insert("always_show_xp".into(), true);
    w.label_switches.insert("always_show_names".into(), false);
    let preset = w.current_preset_data().unwrap();
    let mut back = ConfigWindow::new(sample::settings_catalog());
    back.load_preset(&preset);
    assert!(back.label_switches["always_show_xp"]);
    assert!(!back.label_switches["always_show_names"]);
}

#[test]
fn a_new_slot_starts_as_a_tarantula_and_a_named_model_keeps_its_plan() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    assert_eq!(w.slots[0].category, "tarantula");
    assert_eq!(w.slots[0].model, "tarantula");
    w.add_slot(SlotSpec { model: Some("chosen_one".into()), ..Default::default() });
    assert_eq!(w.slots[1].category, "jumper");
    assert_eq!(w.slots[1].model, "chosen_one");
    w.add_slot(SlotSpec { model: Some(RANDOM_MODEL_ID.into()), ..Default::default() });
    assert_eq!(w.slots[2].category, RANDOM_CATEGORY_ID);
}

#[test]
fn the_category_column_lists_only_the_body_plans_and_the_skins_follow_it() {
    let w = ConfigWindow::new(sample::settings_catalog());
    let categories = w.catalog.categories();
    assert_eq!(categories, vec!["bug", "segmented", "tarantula", "jumper"]);
    let jumpers: Vec<String> = w.catalog.skins_for(Some("jumper")).iter().map(|m| m.id.clone()).collect();
    assert!(jumpers.contains(&"chosen_one".to_owned()));
    assert!(!jumpers.contains(&"tarantula".to_owned()));
    // Adventure enemies are never skins, even under Any kind.
    assert!(w.catalog.skins_for(None).iter().all(|m| !m.enemy));
}

#[test]
fn switching_category_never_leaves_the_slot_empty() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.set_category(0, "segmented");
    assert_ne!(w.slots[0].model, RANDOM_MODEL_ID);
    assert_eq!(w.catalog.models[&w.slots[0].model].body_plan, "segmented");
    w.set_category(0, RANDOM_CATEGORY_ID);
    assert_eq!(w.catalog.models[&w.slots[0].model].body_plan, "segmented", "Any kind keeps the skin");
    // The preset stores a model, never a category.
    let slot = &w.collect_slots()[0];
    assert!(slot.get("category").is_none());
    assert!(w.catalog.models.contains_key(slot["model"].as_str().unwrap()));
}

#[test]
fn loading_a_preset_puts_the_category_back_and_keeps_every_slot_field() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.load_preset(&sample::colony_preset());
    assert_eq!(w.preset_name, "Colony");
    assert_eq!(w.slots.len(), 3);
    assert_eq!(w.slots[0].category, "tarantula");
    assert_eq!(w.slots[0].team, "hunters");
    assert_eq!(w.slots[0].job, "guard");
    assert_eq!(w.slots[0].count, 3);
    assert_eq!(w.slots[2].category, "jumper");
    assert!(w.slots[2].count_random);
    assert_eq!(w.size_scale(), 1.25);
    assert!(!w.interferable);
    assert!(w.flies.enabled && w.flies.spawner);
    assert_eq!(w.flies.max_flies, 10);
}

#[test]
fn count_random_survives_and_is_announced() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.load_preset(&sample::colony_preset());
    let preset = w.current_preset_data().unwrap();
    assert_eq!(preset["slots"][2]["count_random"], json!(true));
    let summary = w.summary();
    assert!(summary.contains("3 slot(s), 5 fixed creature(s) plus 1 random-count slot(s)."), "{summary}");
    assert!(summary.contains("Teams: Porch guard (1), Rivals (1)."), "{summary}");
    assert!(summary.contains("Size: Large (125%). dragging off;"), "{summary}");
}

#[test]
fn random_counts_now_roll_a_real_number() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.load_preset(&sample::colony_preset());
    w.roll_random_counts();
    for row in &w.slots {
        assert!(!row.count_random);
        assert!((1..=10).contains(&row.count));
    }
}

#[test]
fn the_remove_icon_is_drawn_and_works() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    h.assert_label("Remove this creature slot");
    let before = w.slots.len();
    let second = w.slots[1].slot_id.clone();
    h.click("slot0:remove", &mut |ctx| w.ui(ctx));
    assert_eq!(w.slots.len(), before - 1);
    assert_eq!(w.slots[0].slot_id, second);
}

#[test]
fn the_colour_swatch_shows_the_slot_palette_and_follows_the_model() {
    let mut w = window();
    // Slot 0 (tarantula) has custom highlight/leg colours; the swatch is
    // still led by the model's own body and legs.
    let palette = w.slot_palette(0);
    assert_eq!(palette[0], [24, 20, 20]);
    assert!(palette.len() <= 4);
    assert_eq!(w.swatch_description(0).1, "Colors for this slot: 3 custom");
    assert_eq!(w.swatch_description(2).1, "Colors for this slot: model default");
    w.set_colors(0, Default::default());
    assert!(w.collect_slots()[0].get("colors").is_none());
}

#[test]
fn clear_slots_asks_first() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    h.click("button:Clear slots", &mut |ctx| w.ui(ctx));
    assert!(w.dialog_open());
    run(&mut h, &mut w);
    h.click("dialog:No", &mut |ctx| w.ui(ctx));
    assert_eq!(w.slots.len(), 5);
    h.click("button:Clear slots", &mut |ctx| w.ui(ctx));
    run(&mut h, &mut w);
    h.click("dialog:Yes", &mut |ctx| w.ui(ctx));
    assert!(w.slots.is_empty());
    assert_eq!(w.status, "All creature slots cleared. Add a slot before saving or launching.");
    // Launching with no slots warns rather than launching.
    let actions = h.click("button:Save and launch overlay", &mut |ctx| w.ui(ctx));
    assert!(!actions.iter().any(|a| matches!(a, UiAction::LaunchOverlay { .. })));
    assert!(w.dialog_open());
}

#[test]
fn launch_stop_save_and_load_become_actions() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    w.set_presets(vec![dc_ui::model::PresetEntry { label: "Default".into(), path: "presets/default.json".into() }]);
    run(&mut h, &mut w);
    let actions = h.click("button:Save and launch overlay", &mut |ctx| w.ui(ctx));
    let launch = actions.iter().find_map(|a| match a {
        UiAction::LaunchOverlay { mode, preset } => Some((*mode, preset.clone())),
        _ => None,
    });
    let (mode, preset) = launch.expect("launch");
    assert_eq!(mode, OverlayMode::Companion);
    assert_eq!(preset["name"], json!("Default"));
    assert_eq!(preset["slots"].as_array().unwrap().len(), 5);
    let actions = h.click("button:Stop overlay", &mut |ctx| w.ui(ctx));
    assert!(actions.contains(&UiAction::StopOverlay));
    let actions = h.click("button:Save", &mut |ctx| w.ui(ctx));
    assert!(actions.iter().any(|a| matches!(a, UiAction::SavePreset { .. })));
    let actions = h.click("button:Load", &mut |ctx| w.ui(ctx));
    assert!(actions.contains(&UiAction::LoadPreset { path: "presets/default.json".into() }));
    let actions = h.click("button:Refresh library", &mut |ctx| w.ui(ctx));
    assert!(actions.contains(&UiAction::RefreshLibrary));
}

#[test]
fn reset_saved_progress_asks_then_asks_the_app() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    let actions = h.click("button:Reset saved progress...", &mut |ctx| w.ui(ctx));
    assert!(!actions.contains(&UiAction::ResetProgress));
    run(&mut h, &mut w);
    let actions = h.click("dialog:Yes", &mut |ctx| w.ui(ctx));
    assert!(actions.contains(&UiAction::ResetProgress));
}

#[test]
fn the_fly_options_show_only_when_flies_are_on_and_keep_a_valid_range() {
    let mut h = Harness::new(1100.0, 1600.0);
    let mut w = window();
    run(&mut h, &mut w);
    assert!(!h.has_label("Max flies at once:"));
    h.click("check:flies", &mut |ctx| w.ui(ctx));
    assert!(w.flies.enabled);
    run(&mut h, &mut w);
    h.assert_label("Max flies at once:");
    w.apply_settings(&json!({"flies": {"enabled": true, "min_interval": 12.0, "max_interval": 3.0}}));
    assert_eq!(w.flies.max_interval, 12.0);
}

#[test]
fn teams_round_trip_names_colours_and_stances() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.load_preset(&sample::colony_preset());
    assert_eq!(w.team_profiles()["hunters"].name, "Porch guard");
    assert_eq!(w.team_profiles()["hunters"].color, [0xe2, 0xa6, 0x4a]);
    w.rename_team("rivals", "Shed crew");
    w.set_team_color("rivals", [10, 20, 30]);
    w.set_stance("hunters", "rivals", "neutral");
    let preset = w.current_preset_data().unwrap();
    assert_eq!(preset["settings"]["teams"]["rivals"], json!({"name": "Shed crew", "color": "#0a141e"}));
    assert_eq!(preset["settings"]["team_relations"], json!({"hunters": {"rivals": "neutral"}}));
    let mut back = ConfigWindow::new(sample::settings_catalog());
    back.load_preset(&preset);
    assert_eq!(back.team_profiles()["rivals"].name, "Shed crew");
    assert_eq!(back.team_stances()["rivals"]["hunters"], "neutral");
}

#[test]
fn a_new_team_is_made_from_a_name() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    let id = w.create_team(0, "Porch guard").unwrap();
    assert_eq!(id, "porch_guard");
    assert_eq!(w.slots[0].team, "porch_guard");
    assert_eq!(w.team_profiles()["porch_guard"].name, "Porch guard");
    assert!(w.create_team(0, "   ").is_none());
}

#[test]
fn the_teams_panel_lists_pairs_with_their_stance() {
    let mut h = Harness::new(1100.0, 1800.0);
    let mut w = window();
    w.load_preset(&sample::colony_preset());
    run(&mut h, &mut w);
    h.assert_label("Porch guard and Rivals");
    h.probe("stance:hunters:rivals");
    h.assert_label("Between teams");
}

#[test]
fn a_live_update_touches_only_what_the_tray_can_change() {
    let mut w = window();
    let before = w.slots.clone();
    w.apply_live_session_state(&json!({"always_show_names": false, "size_scale": 0.6, "flies_enabled": true,
                                        "interferable": false, "creatures": [{"name": "Bo"}]}));
    assert_eq!(w.size_scale(), 0.6);
    assert!(w.flies.enabled && !w.interferable);
    assert!(!w.label_switches["always_show_names"]);
    assert_eq!(w.slots, before);
    assert_eq!(w.live_creature_state.len(), 1);
    assert_eq!(w.status, "Live update received from the running overlay.");
}

#[test]
fn the_window_draws_at_a_modest_size_without_panicking() {
    for (wd, ht) in [(840.0, 560.0), (960.0, 860.0), (1600.0, 1000.0)] {
        let mut h = Harness::new(wd, ht);
        let mut w = window();
        run(&mut h, &mut w);
        let roster = h.probe("group:creaturesGroup");
        let teams = h.probe("group:teamsGroup");
        assert!(roster.bottom() <= teams.top() + 1.0, "panels do not overlap at {wd}x{ht}");
    }
}

#[test]
fn an_empty_preset_is_refused() {
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.clear_slots();
    assert!(w.current_preset_data().is_err());
    let _: Value = json!(null);
}
