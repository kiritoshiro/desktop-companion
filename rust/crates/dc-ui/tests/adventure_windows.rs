//! The mode menu and the Adventure windows, headless. Ports the intent of
//! test_adventure_polish, test_ingame_polish, test_inspector_ui,
//! test_adventure_controls (the editor), test_local_coop (the party window
//! and locked two-player keys), test_editor_and_admin (the windows) and
//! test_wood_theme (the modes are named and pictured).

mod common;

use common::Harness;
use dc_ui::model::{ControlSettings, HERO, PLAYER_TWO};
use dc_ui::windows::adventure::{AdventureSettingsDialog, PauseDialog};
use dc_ui::windows::character::{CharacterDialog, PartyDialog};
use dc_ui::windows::config::ConfigWindow;
use dc_ui::windows::controls::{ControlsDialog, ControlsEditor};
use dc_ui::windows::inspector::{CreatureInspector, InspectorView, OtherSpider};
use dc_ui::windows::mode_menu::{ModeShell, Page};
use dc_ui::windows::{AdminDialog, MapEditor};
use dc_ui::{UiAction, sample};

fn shell_frame(shell: &mut ModeShell) -> impl FnMut(&egui::Context) -> Vec<UiAction> + '_ {
    move |ctx| {
        ctx.root_ui(|ui| {
            shell.show(ui, &mut |ui| {
                ui.label("companion page");
                Vec::new()
            })
        })
    }
}

#[test]
fn the_three_modes_are_named_pictured_and_open_their_pages() {
    let mut h = Harness::new(1200.0, 900.0);
    let mut shell = ModeShell::new(sample::adventure_view());
    h.settle(&mut shell_frame(&mut shell));
    for label in ["Desktop Companion", "Choose how to play", "Play Companion", "Play Adventure", "Preview"] {
        h.assert_label(label);
    }
    let actions = h.click("mode:skirmish", &mut shell_frame(&mut shell));
    assert_eq!(shell.page, Page::Skirmish);
    assert!(actions.contains(&UiAction::ShowMode("skirmish".into())));
    h.settle(&mut shell_frame(&mut shell));
    h.assert_label("Adventure  \u{b7}  Play as your spider");
    h.assert_label("Skirmish missions");
}

#[test]
fn leaving_the_adventure_page_closes_adventure() {
    let mut h = Harness::new(1200.0, 900.0);
    let mut shell = ModeShell::new(sample::adventure_view());
    shell.page = Page::Skirmish;
    h.settle(&mut shell_frame(&mut shell));
    let actions = h.click("button:Modes", &mut shell_frame(&mut shell));
    assert!(actions.contains(&UiAction::LeaveAdventure));
    assert_eq!(shell.page, Page::Home);
    // Leaving the Companion page does not.
    shell.page = Page::Companion;
    h.settle(&mut shell_frame(&mut shell));
    let actions = h.click("button:Modes", &mut shell_frame(&mut shell));
    assert!(!actions.contains(&UiAction::LeaveAdventure));
}

#[test]
fn missions_are_listed_and_the_maps_open_one_by_one() {
    let mut h = Harness::new(1200.0, 2400.0);
    let mut view = sample::adventure_view();
    view.maps.iter_mut().find(|m| m.id == "territory").unwrap().record =
        dc_ui::model::MissionRecord { victories: 2, defeats: 0, best_seconds: Some(125.0) };
    let mut shell = ModeShell::new(view);
    shell.page = Page::Skirmish;
    h.settle(&mut shell_frame(&mut shell));
    // Every map card has its own picture slot and title.
    for map in &shell.adventure.maps.clone() {
        h.probe(&format!("map:{}", map.id));
        h.assert_label(&map.title);
    }
    h.assert_label("\u{2714} Won \u{d7}2 \u{b7} best 2:05");
    h.assert_label("Coming soon");
    // Ember opened by the win; Obsidian still waits on Ember.
    h.probe("map:ember:play");
    h.probe("map:obsidian:locked");
    h.assert_label("Win \u{201c}Ember hollow\u{201d} to unlock");
    let card = h.probe("map:ember");
    assert!(card.right() <= 1200.0 && card.width() <= 240.0, "cards fit the window: {card:?}");
    let actions = h.click("map:ember:play", &mut shell_frame(&mut shell));
    assert!(actions.contains(&UiAction::PlayMap { map_id: "ember".into() }));
}

#[test]
fn the_raid_setup_switches_players_and_screens() {
    let mut h = Harness::new(1200.0, 2400.0);
    let mut shell = ModeShell::new(sample::adventure_view());
    shell.page = Page::Skirmish;
    h.settle(&mut shell_frame(&mut shell));
    h.assert_label("Raids use every screen  (2 screens found; outposts wait on the others)");
    h.assert_label("Use screen 2 (2560\u{d7}1440) in missions");
    assert_eq!(h.toggled("Use screen 2 (2560\u{d7}1440) in missions"), Some(true));
    // One player: the saved keys as key caps.
    h.assert_label("WASD");
    let actions = h.click("segment:Two players \u{b7} one keyboard", &mut shell_frame(&mut shell));
    assert!(actions.contains(&UiAction::SetTwoPlayer(true)));
    h.settle(&mut shell_frame(&mut shell));
    h.assert_label("P1");
    h.assert_label("P2");
    let actions = h.click("pill:screen2", &mut shell_frame(&mut shell));
    assert!(actions.contains(&UiAction::SetScreen { name: "\\\\.\\DISPLAY2".into(), on: false }));
    let actions = h.click("pill:all-screens", &mut shell_frame(&mut shell));
    assert!(actions.contains(&UiAction::SetAllScreens(false)));
    for (key, action) in [
        ("button:Controls", UiAction::OpenControls),
        ("button:Map editor", UiAction::OpenMapEditor { map_id: None }),
        ("button:Admin", UiAction::OpenAdmin),
        ("button:Character", UiAction::OpenCharacter),
    ] {
        let actions = h.click(key, &mut shell_frame(&mut shell));
        assert!(actions.contains(&action), "{key}");
    }
}

#[test]
fn with_one_screen_the_screen_switch_is_disabled() {
    let mut h = Harness::new(1200.0, 2400.0);
    let mut view = sample::adventure_view();
    view.screens.truncate(1);
    let mut shell = ModeShell::new(view);
    shell.page = Page::Skirmish;
    h.settle(&mut shell_frame(&mut shell));
    let label = "Raids use every screen  (1 screen found; outposts wait on the others)";
    assert_eq!(h.disabled(label), Some(true));
    assert!(!h.has_label_containing("Use screen 2"));
}

#[test]
fn playing_from_the_settings_window_launches_adventure() {
    let mut h = Harness::new(1200.0, 2400.0);
    let mut w = ConfigWindow::new(sample::settings_catalog());
    w.shell = ModeShell::new(sample::adventure_view());
    w.shell.page = Page::Skirmish;
    h.settle(&mut |ctx| w.ui(ctx));
    let actions = h.click("map:territory:play", &mut |ctx| w.ui(ctx));
    assert!(actions.contains(&UiAction::PlayMap { map_id: "territory".into() }));
    assert!(actions.iter().any(|a| matches!(a, UiAction::LaunchOverlay { mode: dc_ui::OverlayMode::Adventure, .. })));
}

#[test]
fn the_controls_editor_rebinds_saves_and_resets() {
    let mut h = Harness::new(900.0, 900.0);
    let mut dialog = ControlsDialog::new(ControlSettings::default());
    let mut frame = |ctx: &egui::Context| dialog.ui(ctx);
    h.settle(&mut frame);
    h.assert_label("Adventure controls");
    h.click("binding:jump", &mut frame);
    assert_eq!(dialog.editor.listening.as_deref(), Some("jump"));
    let actions = h.key(egui::Key::W, &mut |ctx| dialog.ui(ctx));
    let saved = actions.iter().find_map(|a| match a {
        UiAction::SaveControls(s) => Some(s.clone()),
        _ => None,
    });
    let saved = saved.expect("saved at once");
    assert_eq!(saved.binding("jump"), "W");
    assert_eq!(saved.binding("move_up"), "Space", "swapped, never a gap");
    // Esc cancels listening.
    h.click("binding:bite", &mut |ctx| dialog.ui(ctx));
    h.key(egui::Key::Escape, &mut |ctx| dialog.ui(ctx));
    assert!(dialog.editor.listening.is_none());
    assert_eq!(dialog.editor.settings.binding("bite"), "F");
    let actions = h.click("button:Reset to defaults", &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::SaveControls(ControlSettings::default())));
    let actions = h.click("button:Done", &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::CloseWindow));
}

#[test]
fn a_mouse_button_can_be_bound() {
    let mut h = Harness::new(900.0, 900.0);
    let mut editor = ControlsEditor::new(ControlSettings::default());
    let mut frame = |ctx: &egui::Context| ctx.root_ui(|ui| editor.show(ui));
    h.settle(&mut frame);
    h.click("binding:shoot", &mut frame);
    let actions = h.click("binding:shoot", &mut frame);
    assert!(actions.iter().any(|a| matches!(a, UiAction::SaveControls(s) if s.binding("shoot") == "Mouse Left")));
    assert!(editor.listening.is_none(), "the binding click does not start listening again");
}

#[test]
fn the_settings_list_still_works_in_two_player_but_its_keys_are_locked() {
    let mut h = Harness::new(900.0, 1600.0);
    let mut dialog = AdventureSettingsDialog::new(60, ControlSettings::default(), 2);
    h.settle(&mut |ctx| dialog.ui(ctx));
    assert!(h.has_label_containing("Two-player mode uses fixed keys"));
    assert_eq!(h.disabled("Jump: Space"), Some(true));
    assert_eq!(h.disabled("Reset to defaults"), Some(true));
    h.click("binding:jump", &mut |ctx| dialog.ui(ctx));
    assert!(dialog.controls.listening.is_none());
    let actions = h.click("button:Apply and return", &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::SetFrameRate(60)));
}

#[test]
fn the_pause_menu_offers_raid_choices_in_a_raid() {
    let mut h = Harness::new(400.0, 500.0);
    let mut pause = PauseDialog::new(true);
    h.settle(&mut |ctx| pause.ui(ctx));
    for label in ["Resume", "Character & armour", "Settings", "Restart raid", "Retry saving victory", "Exit raid"] {
        h.assert_label(label);
    }
    let actions = h.click("pause:restart", &mut |ctx| pause.ui(ctx));
    assert!(actions.contains(&UiAction::PauseChoice("restart".into())));
    let mut free = PauseDialog::new(false);
    h.settle(&mut |ctx| free.ui(ctx));
    h.assert_label("Release spider");
}

#[test]
fn the_in_game_window_is_the_character_window_and_armour_goes_on_and_off() {
    let mut h = Harness::new(1000.0, 1300.0);
    let mut dialog = CharacterDialog::new(sample::party_view(), sample::progression_catalog(), HERO);
    h.settle(&mut |ctx| dialog.ui(ctx));
    h.assert_label("Spider anatomy: drag armour onto it");
    h.assert_label("Level 6");
    // XP is written above the bar, not on it.
    h.assert_label("Level 6  \u{b7}  40 / 294 XP");
    // Drag a piece from the bag onto the doll: it goes on.
    let tile = h.probe("tile:hero:sun_aegis");
    let doll = h.probe("doll");
    let actions = drag(&mut h, tile.center(), doll.center(), &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::Equip { who: HERO.into(), item_id: "sun_aegis".into() }), "{actions:?}");
    assert_eq!(
        dialog.party.member(HERO).unwrap().progression.equipped.get("carapace").map(String::as_str),
        Some("sun_aegis")
    );
    // Picked pieces show their panel; take it off by dragging back to the bag.
    let doll = h.probe("doll");
    let bag = h.probe("bag:hero");
    let carapace_point = doll.center() + egui::vec2(0.0, -0.22 * 80.0 + 8.0);
    let actions = drag(&mut h, carapace_point, bag.center(), &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::Unequip { who: HERO.into(), slot: "carapace".into() }), "{actions:?}");
    assert!(!dialog.party.member(HERO).unwrap().progression.equipped.contains_key("carapace"));
}

fn drag(
    h: &mut Harness,
    from: egui::Pos2,
    to: egui::Pos2,
    ui: &mut dyn FnMut(&egui::Context) -> Vec<UiAction>,
) -> Vec<UiAction> {
    use egui::{Event, Modifiers, PointerButton};
    let mut actions = Vec::new();
    let mut push = |h: &mut Harness, e: Event, ui: &mut dyn FnMut(&egui::Context) -> Vec<UiAction>| {
        actions.extend(h.event_frame(e, ui));
    };
    push(h, Event::PointerMoved(from), ui);
    push(
        h,
        Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ui,
    );
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        push(h, Event::PointerMoved(from + (to - from) * t), ui);
    }
    push(
        h,
        Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        ui,
    );
    push(h, Event::PointerMoved(egui::pos2(1.0, 1.0)), ui);
    actions
}

#[test]
fn a_skill_is_learned_from_the_tree() {
    let mut h = Harness::new(1000.0, 1300.0);
    let mut dialog = CharacterDialog::new(sample::party_view(), sample::progression_catalog(), HERO);
    h.settle(&mut |ctx| dialog.ui(ctx));
    h.click("tab:Skills", &mut |ctx| dialog.ui(ctx));
    h.settle(&mut |ctx| dialog.ui(ctx));
    h.assert_label("2 skill points to spend \u{b7} one per level");
    // Every skill has its picture and says what it needs.
    h.assert_label("Vitality: Learned");
    h.assert_label("Apex predator: Level 15");
    h.assert_label("Hardened carapace: Learn \u{b7} 1 pt");
    let actions = h.click("skill:hero:carapace_harden", &mut |ctx| dialog.ui(ctx));
    assert!(actions.contains(&UiAction::UnlockSkill { who: HERO.into(), ability_id: "carapace_harden".into() }));
    assert_eq!(dialog.party.member(HERO).unwrap().progression.skill_points, 1);
    // A locked node does nothing.
    let actions = h.click("skill:hero:apex_predator", &mut |ctx| dialog.ui(ctx));
    assert!(!actions.iter().any(|a| matches!(a, UiAction::UnlockSkill { .. })));
}

#[test]
fn the_party_window_puts_each_bag_below_its_spider() {
    let mut h = Harness::new(1180.0, 940.0);
    let mut party = sample::party_view();
    party.two_player = true;
    let mut dialog = PartyDialog::new(party, sample::progression_catalog());
    h.settle(&mut |ctx| dialog.ui(ctx));
    h.assert_label("Player 1  \u{b7}  WASD \u{b7} K");
    h.assert_label("Player 2  \u{b7}  Arrows \u{b7} P");
    let doll = h.probe("doll:compact");
    let bag_one = h.probe("bag:hero");
    let bag_two = h.probe("bag:player_two");
    assert!(bag_one.center().x < bag_two.center().x, "side by side");
    assert!(doll.bottom() <= bag_two.top() + 1.0 || doll.bottom() <= bag_one.top() + 1.0, "the bag is below the doll");
    assert_eq!(dialog.sheets[1].who, PLAYER_TWO);
}

#[test]
fn the_inspector_shows_vitals_tiles_team_and_what_hostility_does() {
    let mut h = Harness::new(1200.0, 780.0);
    let catalog = sample::progression_catalog();
    let teams = dc_ui::teams::normalize_teams(None, &["hunters".into(), "rivals".into()]);
    let view = InspectorView {
        key: "spider-1".into(),
        display_name: "Bo".into(),
        model_name: "Tarantula".into(),
        job_name: "Guard".into(),
        hp: 30.0,
        max_hp: 60.0,
        energy: 20.0,
        max_energy: 80.0,
        armor: 2.5,
        damage: 4.0,
        progression: dc_ui::model::Progression {
            level: 4,
            xp: 10,
            skill_points: 2,
            team_id: "hunters".into(),
            ..Default::default()
        },
        others: vec![OtherSpider { key: "spider-2".into(), name: "Mo".into(), relation: "neutral".into() }],
        team_profiles: teams,
        alive: true,
        ..Default::default()
    };
    let mut inspector = CreatureInspector::new(view, catalog);
    assert_eq!(inspector.title(), "Inspect Bo");
    h.settle(&mut |ctx| inspector.ui(ctx));
    for label in [
        "Vitals",
        "Health 30 / 60",
        "Stamina 20 / 80",
        "Armour: softens every hit",
        "Damage: each bite",
        "Skill points to spend",
        "Armour pieces worn",
        "Team: Hunters",
        "Overview",
        "Skill tree",
        "Inventory & armour",
        "Health bar (without the name)",
        "Mo",
        "Portrait of Bo",
    ] {
        h.assert_label(label);
    }
    assert!(h.has_label_containing("Foes fight."));
    // A pinned health bar is its own switch.
    let actions = h.click("check:health-pin", &mut |ctx| inspector.ui(ctx));
    assert!(actions.contains(&UiAction::SetHealthPin(true)));
    assert!(!actions.iter().any(|a| matches!(a, UiAction::SetLevelPin(_))));
    // Typing a team name does not assign every prefix along the way.
    let edit = h.probe("inspector:team-text");
    let actions = h.click_at(edit.center(), &mut |ctx| inspector.ui(ctx));
    assert!(actions.is_empty());
    inspector.team_text = "Porch guard".into();
    h.settle(&mut |ctx| inspector.ui(ctx));
    assert_eq!(inspector.view.progression.team_id, "hunters");
    // Choosing a team by name assigns the id behind it.
    assert_eq!(inspector.commit_team("Rivals"), Some(UiAction::SetTeam("rivals".into())));
    assert_eq!(inspector.commit_team("Porch guard"), Some(UiAction::SetTeam("porch_guard".into())));
    // The armour tab is the doll and the bag; the skill tree has pictures.
    h.click("tab:Inventory & armour", &mut |ctx| inspector.ui(ctx));
    h.settle(&mut |ctx| inspector.ui(ctx));
    h.probe("doll");
    h.probe("bag:inspector");
    h.assert_label("Pick a piece to see what it gives.");
    let actions = h.click("button:Add to bag", &mut |ctx| inspector.ui(ctx));
    assert!(actions.iter().any(|a| matches!(a, UiAction::AddInventoryItem(_))));
    h.click("tab:Skill tree", &mut |ctx| inspector.ui(ctx));
    h.settle(&mut |ctx| inspector.ui(ctx));
    h.assert_label("Vitality: Learn \u{b7} 1 pt");
    // A spider that left the desktop closes its window.
    inspector.view.alive = false;
    let actions = h.frame(&mut |ctx| inspector.ui(ctx));
    assert!(actions.contains(&UiAction::CloseWindow));
}

#[test]
fn the_admin_window_switches_gifts_and_resets() {
    let mut h = Harness::new(420.0, 500.0);
    let mut admin = AdminDialog::new(false, 3);
    h.settle(&mut |ctx| admin.ui(ctx));
    let actions = h.click("check:admin", &mut |ctx| admin.ui(ctx));
    assert!(actions.contains(&UiAction::SetAdmin(true)));
    let actions = h.click("admin:amber", &mut |ctx| admin.ui(ctx));
    assert!(actions.contains(&UiAction::GiveAmber(1000)));
    let actions = h.click("admin:level", &mut |ctx| admin.ui(ctx));
    assert!(actions.contains(&UiAction::SetHeroLevel(3)));
    let actions = h.click("admin:reset", &mut |ctx| admin.ui(ctx));
    assert!(!actions.contains(&UiAction::ResetAdventureProgress));
    h.settle(&mut |ctx| admin.ui(ctx));
    let actions = h.click("dialog:Yes", &mut |ctx| admin.ui(ctx));
    assert!(actions.contains(&UiAction::ResetAdventureProgress));
    assert_eq!(admin.level, 1);
}

#[test]
fn the_editor_builds_a_map_and_will_not_play_a_broken_one() {
    let mut h = Harness::new(1280.0, 900.0);
    let mut editor = MapEditor::new(None, dc_ui::bridge::progression_catalog());
    h.settle(&mut |ctx| editor.ui(ctx));
    h.assert_label("Ready to play.");
    h.click("editor:add-building", &mut |ctx| editor.ui(ctx));
    assert_eq!(editor.buildings().len(), 6);
    let actions = h.click("editor:save", &mut |ctx| editor.ui(ctx));
    let saved = actions.iter().find_map(|a| match a {
        UiAction::SaveMap(v) => Some(v.clone()),
        _ => None,
    });
    assert_eq!(saved.unwrap()["buildings"].as_array().unwrap().len(), 6);
    editor.saved("custom-my-map");
    // Break it: no home.
    editor.select(dc_ui::windows::map_editor::Pick::Building(0));
    editor.remove_selected();
    h.settle(&mut |ctx| editor.ui(ctx));
    assert_eq!(h.disabled("Save and play"), Some(true));
    let actions = h.click("editor:play", &mut |ctx| editor.ui(ctx));
    assert!(!actions.iter().any(|a| matches!(a, UiAction::SaveAndPlayMap(_))));
    h.click("editor:delete", &mut |ctx| editor.ui(ctx));
    h.settle(&mut |ctx| editor.ui(ctx));
    let actions = h.click("dialog:Yes", &mut |ctx| editor.ui(ctx));
    assert!(actions.contains(&UiAction::DeleteMap("custom-my-map".into())));
}
