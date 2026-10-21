//! Adventure in the overlay, without a window: the engine parts of
//! `tests/test_map_view_and_release.py`, `test_local_coop.py`,
//! `test_adventure_controls.py`, `test_adventure_hero.py`,
//! `test_adventure.py` and `test_adventure_overlay_frames.py`.
//!
//! Every test runs on its own state folder, passed explicitly, and drives the
//! game with synthetic keys; frames are painted by the CPU renderer.

use super::draw::AdventureDrawer;
use super::input::KeyInput;
use super::*;
use dc_adventure::adventure::ControlledSpider;
use dc_adventure::adventure_profile::{AdventureProfile, fresh_profile, load_profile, mission_record, save_profile};
use dc_adventure::campaign::MapKind;
use dc_adventure::controls::save_controls;
use dc_adventure::mission::MissionSpider;
use dc_art::mission_ui::hud_rect;
use dc_paint::PaintSwitches;
use dc_render::{CpuRenderer, Image, Scene};

const W: f64 = 1600.0;
const H: f64 = 1000.0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("dc-app-adventure-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

/// An Adventure overlay on `presets/colony.json` (seed 4) over one
/// 1600 x 1000 screen, with the profile `setup` leaves in the state folder.
fn game(state: &Scratch, setup: impl FnOnce(&mut AdventureProfile)) -> AdventureGame {
    let mut profile = fresh_profile();
    setup(&mut profile);
    assert!(save_profile(&profile, &profile_path(&state.0)));
    let root = root();
    let preset = root.join("presets").join("colony.json");
    let mut game = AdventureGame::start(
        StartOptions {
            root: &root,
            preset: &preset,
            state_dir: &state.0,
            size: (W, H),
            rects: vec![ScreenRect::new(0.0, 0.0, W, H)],
            primary: 0,
            primary_rect: Some(IRect::new(0, 0, W as i32, H as i32)),
            seed: Some(4),
            label_metrics: None,
        },
        None,
    )
    .unwrap();
    // No real system-wide hotkey from a test: the key works in the window.
    game.release_hotkey = false;
    game.take_effects();
    game
}

fn profile_path(state: &Path) -> PathBuf {
    dc_adventure::adventure_profile::profile_path(state)
}

fn press(game: &mut AdventureGame, key: Key) {
    game.key(KeyInput::press(key));
}

fn release(game: &mut AdventureGame, key: Key) {
    game.key(KeyInput::release(key));
}

fn held(game: &AdventureGame, player: usize) -> Vec<Action> {
    game.territory().players[player].control.held.iter().copied().collect()
}

fn render(game: &mut AdventureGame, drawer: &mut AdventureDrawer) -> Image {
    let mut scene = Scene::new();
    {
        let mut canvas = scene.canvas();
        drawer.paint(game, &mut canvas);
    }
    CpuRenderer::new().render(&scene, W as u32, H as u32, None)
}

/// Share of the sampled points in `r` that are painted at all.
fn painted_share(image: &Image, r: IRect) -> f64 {
    let mut hits = 0;
    let mut total = 0;
    for fy in 1..20 {
        for fx in 1..20 {
            let x = (r.x + r.w * fx / 20) as u32;
            let y = (r.y + r.h * fy / 20) as u32;
            total += 1;
            if image.pixel(x, y).a > 0 {
                hits += 1;
            }
        }
    }
    hits as f64 / total as f64
}

/// `_covered`: share of the mission's main screen painted opaque.
fn covered(image: &Image, area: dc_world::ScreenRect) -> f64 {
    let mut hits = 0;
    let mut total = 0;
    for fx in 1..20 {
        for fy in 1..20 {
            let x = (area.x + area.w * fx as f64 / 20.0) as u32;
            let y = (area.y + area.h * fy as f64 / 20.0) as u32;
            total += 1;
            if image.pixel(x, y).a == 255 {
                hits += 1;
            }
        }
    }
    hits as f64 / total as f64
}

fn hero_position(game: &AdventureGame) -> (f64, f64) {
    let c = game.player_spider(0);
    (ControlledSpider::x(c), ControlledSpider::y(c))
}

// -- starting a raid ---------------------------------------------------------------

#[test]
fn a_raid_starts_on_the_profile_s_chosen_map() {
    let state = scratch("chosen");
    let g = game(&state, |p| {
        p.selected_map = "ember".into();
        p.admin = true;
    });
    assert_eq!(g.territory().map_info.id, "ember");
    assert!(matches!(g.mission, Mission::Territory(_)));
    assert_eq!(g.player_count(), 1);
    assert!(g.hero_alive());
    assert!(g.captures_mouse(200.0, 200.0), "the raid has the screen");

    let swarm = scratch("chosen-swarm");
    let g = game(&swarm, |p| {
        p.selected_map = "swarm".into();
        p.admin = true;
    });
    assert_eq!(g.territory().map_info.kind, MapKind::Swarm);
    assert!(matches!(g.mission, Mission::Swarm(_)));

    // A locked map falls back to the first one, as on the Adventure page.
    let locked = scratch("chosen-locked");
    let g = game(&locked, |p| p.selected_map = "ember".into());
    assert_eq!(g.territory().map_info.id, "territory");
}

#[test]
fn starting_asks_for_the_release_hotkey_and_the_keyboard() {
    let state = scratch("start-effects");
    let mut profile = fresh_profile();
    profile.admin = true;
    assert!(save_profile(&profile, &profile_path(&state.0)));
    let root = root();
    let preset = root.join("presets").join("colony.json");
    let mut g = AdventureGame::start(
        StartOptions {
            root: &root,
            preset: &preset,
            state_dir: &state.0,
            size: (W, H),
            rects: vec![],
            primary: 0,
            primary_rect: None,
            seed: Some(4),
            label_metrics: None,
        },
        None,
    )
    .unwrap();
    let effects = g.take_effects();
    assert!(effects.contains(&Effect::RegisterReleaseHotkey("F8".into())));
    assert!(effects.contains(&Effect::Activate));
    assert!(g.take_effects().is_empty());
}

// -- keys --------------------------------------------------------------------------

#[test]
fn keys_move_the_player() {
    let state = scratch("move");
    let mut g = game(&state, |_| {});
    g.cursor = (800.0, 500.0);
    let start = hero_position(&g);
    let heading = ControlledSpider::heading(g.player_spider(0));
    press(&mut g, Key::Char('W'));
    assert_eq!(held(&g, 0), vec![Action::MoveUp]);
    for _ in 0..60 {
        g.tick(1.0 / 60.0);
    }
    let moved = hero_position(&g);
    let walked = (moved.0 - start.0).hypot(moved.1 - start.1);
    assert!(walked > 5.0, "W walks the spider forward ({walked:.1} px)");
    release(&mut g, Key::Char('W'));
    assert!(held(&g, 0).is_empty());

    // Turn and walk by default: D turns on the spot.
    press(&mut g, Key::Char('D'));
    for _ in 0..30 {
        g.tick(1.0 / 60.0);
    }
    release(&mut g, Key::Char('D'));
    let turned = ControlledSpider::heading(g.player_spider(0));
    assert!((turned - heading).abs() > 0.05, "D turns the spider");
}

#[test]
fn the_overlay_follows_the_bindings_and_esc_always_pauses() {
    let state = scratch("bindings");
    let mut settings = ControlSettings::default();
    settings.rebind(Action::Jump, "J");
    settings.rebind(Action::Pause, "P");
    assert!(save_controls(&settings, &controls_path(&state.0)));
    let mut g = game(&state, |_| {});
    g.tick(0.02);

    press(&mut g, Key::Char('J'));
    let cooldown = g.territory().players[0].control.jump_cooldown;
    assert!(cooldown > 0.0, "J jumps");
    g.key(KeyInput::press(Key::Char('J')).repeated());
    assert_eq!(g.territory().players[0].control.jump_cooldown, cooldown, "auto-repeat does not jump again");
    press(&mut g, Key::Space);
    assert!(held(&g, 0).is_empty(), "Space no longer means anything");

    press(&mut g, Key::Char('P'));
    assert!(g.paused);
    assert_eq!(g.take_effects(), vec![Effect::Ui(UiRequest::Pause)]);
    assert!(g.apply(&EngineCommand::Resume));
    assert!(!g.paused);

    press(&mut g, Key::Esc);
    assert!(g.paused, "Esc pauses whatever pause is bound to");
    assert!(g.take_effects().contains(&Effect::Ui(UiRequest::Pause)));
    g.apply(&EngineCommand::Resume);

    press(&mut g, Key::Char('W'));
    assert_eq!(held(&g, 0), vec![Action::MoveUp]);
    release(&mut g, Key::Char('W'));
    assert!(held(&g, 0).is_empty());
}

#[test]
fn esc_pauses_the_raid_until_the_menu_lets_go() {
    let state = scratch("pause");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    press(&mut g, Key::Char('W'));
    press(&mut g, Key::Esc);
    assert!(g.paused);
    assert_eq!(g.pause_reason, Some(PauseReason::Menu));
    assert!(held(&g, 0).is_empty(), "pausing lets go of every key");
    assert_eq!(g.take_effects(), vec![Effect::Ui(UiRequest::Pause)]);
    press(&mut g, Key::Esc);
    assert!(g.take_effects().is_empty(), "one pause menu at a time");

    let elapsed = g.territory().elapsed;
    g.tick(0.05);
    assert_eq!(g.territory().elapsed, elapsed, "the raid waits");
    press(&mut g, Key::Char('W'));
    assert!(held(&g, 0).is_empty(), "no game keys while paused");

    // The menu opens, then closes: the raid goes on.
    g.set_ui_open(true, 0.02);
    g.set_ui_open(true, 1.0);
    assert!(g.paused, "still paused while the menu is open");
    g.set_ui_open(false, 0.02);
    assert!(!g.paused);
    assert!(g.take_effects().contains(&Effect::Activate));
    g.tick(0.05);
    assert!(g.territory().elapsed > elapsed);

    // A UI layer that never shows anything does not freeze the raid.
    press(&mut g, Key::Esc);
    g.set_ui_open(false, 0.1);
    assert!(g.paused);
    g.set_ui_open(false, UI_GRACE_SECONDS);
    assert!(!g.paused);
}

#[test]
fn k_saves_and_opens_the_character_window_and_profile_changes_come_back() {
    let state = scratch("character");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let path = profile_path(&state.0);
    std::fs::remove_file(&path).unwrap();
    press(&mut g, Key::Char('K'));
    assert!(g.paused);
    assert_eq!(g.pause_reason, Some(PauseReason::Character));
    assert_eq!(g.take_effects(), vec![Effect::Ui(UiRequest::Character { player: 0 })]);
    assert!(path.exists(), "the raid's progress is saved before the window opens");

    // The character window renames the hero; the live spider follows.
    let mut profile = load_profile(&path);
    profile.name = "Night Weaver".into();
    assert!(save_profile(&profile, &path));
    assert!(g.apply(&EngineCommand::ProfileChanged));
    assert_eq!(MissionSpider::display_name(g.player_spider(0)), "Night Weaver");
    g.apply(&EngineCommand::Resume);
    assert!(!g.paused);
}

#[test]
fn spider_and_overlay_actions_are_not_adventure_s() {
    let state = scratch("foreign");
    let mut g = game(&state, |_| {});
    assert!(!g.apply(&EngineCommand::OverlayAction { action: "pause_all".into() }));
    assert!(!g.apply(&EngineCommand::SpiderAction { creature: "x".into(), action: "rename".into(), arg: None }));
}

// -- the map and the release key (test_map_view_and_release.py) ------------------------

#[test]
fn the_map_key_swaps_the_desktop_for_the_mission_map_and_back() {
    let state = scratch("map");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let area = g.territory().area;
    let image = render(&mut g, &mut drawer);
    assert!(covered(&image, area) < 0.5, "the desktop shows through");
    press(&mut g, Key::Char('`'));
    assert!(g.map_view);
    assert_eq!(g.take_effects(), vec![Effect::Announce("Showing the mission map".into())]);
    let image = render(&mut g, &mut drawer);
    assert!(covered(&image, area) > 0.97, "the map covers the desktop");
    press(&mut g, Key::Char('`'));
    assert!(!g.map_view);
    assert_eq!(drawer.backdrop.cached(), 1, "the backdrop is painted once and reused");
}

#[test]
fn the_tilde_key_opens_the_map_whatever_the_layout_calls_it() {
    let state = scratch("tilde");
    let mut g = game(&state, |_| {});
    // Shift+` reads as ~; a national layout's letter there is still VK_OEM_3.
    press(&mut g, Key::Char('~'));
    assert!(g.map_view);
    g.key(KeyInput { key: Key::Char('Ė'), under_esc: true, repeat: false, pressed: true });
    assert!(!g.map_view);
}

#[test]
fn releasing_the_controls_frees_the_desktop_and_pauses_the_raid() {
    let state = scratch("release");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    press(&mut g, Key::Char('W'));
    press(&mut g, Key::F(8));
    assert!(g.controls_released);
    assert!(held(&g, 0).is_empty());
    assert!(!g.captures_mouse(200.0, 200.0), "clicks reach the desktop");
    let effects = g.take_effects();
    assert!(effects.contains(&Effect::Deactivate));
    assert!(effects.iter().any(|e| matches!(e, Effect::Announce(t) if t.contains("Press F8"))));
    let elapsed = g.territory().elapsed;
    g.tick(0.05);
    assert_eq!(g.territory().elapsed, elapsed, "the raid waits");
    press(&mut g, Key::Char('`'));
    assert!(!g.map_view, "no game keys while released");
    press(&mut g, Key::Esc);
    assert!(!g.paused, "no pause menu while released");
    press(&mut g, Key::F(8));
    assert!(!g.controls_released);
    assert!(g.captures_mouse(200.0, 200.0));
    assert!(g.take_effects().contains(&Effect::Activate));
}

#[test]
fn with_the_hotkey_registered_only_windows_toggles_the_release() {
    let state = scratch("hotkey");
    let mut g = game(&state, |_| {});
    g.release_hotkey = true;
    press(&mut g, Key::F(8));
    assert!(!g.controls_released, "WM_HOTKEY delivers it instead");
    g.release_hotkey_pressed();
    assert!(g.controls_released);
    g.release_hotkey_pressed();
    assert!(!g.controls_released);
}

#[test]
fn the_release_banner_is_drawn_while_released() {
    let state = scratch("banner");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let area = g.territory().area;
    let band =
        IRect::new((area.x + area.w * 0.4) as i32, (area.y + area.h * 0.42) as i32 + 4, (area.w * 0.2) as i32, 26);
    let before = painted_share(&render(&mut g, &mut drawer), band);
    g.toggle_controls();
    let after = painted_share(&render(&mut g, &mut drawer), band);
    assert!(after > 0.9 && after > before, "banner {before:.2} -> {after:.2}");
}

// -- two players (test_local_coop.py) ---------------------------------------------------

#[test]
fn two_players_take_their_own_keys_and_release_clears_both() {
    let state = scratch("coop");
    let mut g = game(&state, |p| p.two_player = true);
    assert_eq!(g.player_count(), 2);
    assert!(g.territory().players.iter().all(|p| p.control.controls.turn_movement()), "turn and walk for both");
    for key in [Key::Char('W'), Key::Up, Key::Char('F'), Key::Char('J')] {
        press(&mut g, key);
    }
    assert_eq!(held(&g, 0), vec![Action::MoveUp, Action::Bite]);
    assert_eq!(held(&g, 1), vec![Action::MoveUp, Action::Bite]);
    release(&mut g, Key::Char('W'));
    release(&mut g, Key::Char('F'));
    assert!(held(&g, 0).is_empty());
    assert_eq!(held(&g, 1), vec![Action::MoveUp, Action::Bite]);
    g.key(KeyInput::release(Key::Char('J')).repeated());
    assert!(held(&g, 1).contains(&Action::Bite), "an auto-repeat release is ignored");
    release(&mut g, Key::Char('J'));
    assert_eq!(held(&g, 1), vec![Action::MoveUp]);

    // Either player can open the map; the release lets go of both.
    press(&mut g, Key::Char('~'));
    assert!(g.map_view);
    press(&mut g, Key::Char('W'));
    press(&mut g, Key::F(8));
    assert!(g.controls_released);
    assert!(held(&g, 0).is_empty() && held(&g, 1).is_empty(), "player 2 stops too");
    press(&mut g, Key::F(8));

    // Map, pause and losing the keyboard clear both players as well.
    press(&mut g, Key::Up);
    press(&mut g, Key::Char('A'));
    g.focus_lost();
    assert!(held(&g, 0).is_empty() && held(&g, 1).is_empty());
    press(&mut g, Key::Up);
    press(&mut g, Key::Esc);
    assert!(g.paused && held(&g, 1).is_empty());
}

#[test]
fn player_two_s_skills_key_asks_for_their_character() {
    let state = scratch("coop-skills");
    let mut g = game(&state, |p| p.two_player = true);
    press(&mut g, Key::Char('P'));
    assert_eq!(g.take_effects(), vec![Effect::Ui(UiRequest::Character { player: 1 })]);
}

#[test]
fn a_held_bite_in_coop_lands_and_a_coop_frame_draws_both_huds() {
    let state = scratch("coop-frame");
    let mut g = game(&state, |p| p.two_player = true);
    press(&mut g, Key::Char('J'));
    for _ in 0..4 {
        g.tick(0.02);
    }
    let second = g.territory().players[1].creature;
    assert!(spider(&g.manager, second).attack_cooldown > 0.0, "player 2's held bite strikes");
    let window = g.hud_window();
    let (a, b) = (hud_rect(&window, 0), hud_rect(&window, 1));
    assert!(a.x + a.w <= b.x || b.x + b.w <= a.x, "the two HUDs do not overlap");
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let image = render(&mut g, &mut drawer);
    assert!(painted_share(&image, a) > 0.9);
    assert!(painted_share(&image, b) > 0.9);
}

// -- the mouse ---------------------------------------------------------------------------

#[test]
fn the_status_panel_can_be_dragged_without_shooting() {
    let state = scratch("drag");
    let mut g = game(&state, |_| {});
    let original = hud_rect(&g.hud_window(), 0);
    let pressed = (original.x + original.w / 2, original.y + original.h / 2);
    let silk = g.territory().players[0].control.silk;
    g.mouse_press(MouseButton::Left, pressed.0 as f64, pressed.1 as f64);
    g.cursor_moved(250.0, 220.0);
    let moved = hud_rect(&g.hud_window(), 0);
    assert_eq!((moved.x, moved.y), (250 - (pressed.0 - original.x), 220 - (pressed.1 - original.y)));
    g.mouse_release(MouseButton::Left);
    g.cursor_moved(600.0, 600.0);
    assert_eq!(hud_rect(&g.hud_window(), 0), moved, "the drag ends on release");
    assert_eq!(g.territory().players[0].control.silk, silk, "no shot");
}

#[test]
fn a_bound_mouse_button_shoots_at_the_pointer() {
    let state = scratch("mouse-shot");
    let mut settings = ControlSettings::default();
    settings.rebind(Action::Shoot, "Mouse Left");
    assert!(save_controls(&settings, &controls_path(&state.0)));
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let (x, y) = hero_position(&g);
    let heading = ControlledSpider::heading(g.player_spider(0));
    let target = (x + heading.cos() * 150.0, y + heading.sin() * 150.0);
    let silk = g.territory().players[0].control.silk;
    g.mouse_press(MouseButton::Left, target.0, target.1);
    assert_eq!(g.territory().players[0].control.aim, target, "the click aims the shot");
    assert!(g.territory().players[0].control.silk < silk, "the shot is fired");
    g.mouse_release(MouseButton::Left);
}

#[test]
fn a_command_button_orders_the_companions() {
    let state = scratch("command");
    let mut g = game(&state, |_| {});
    let rects = command_rects(&g.hud_window());
    let (_, defend) = rects[1];
    g.mouse_press(MouseButton::Left, (defend.x + 5) as f64, (defend.y + 5) as f64);
    assert_eq!(g.territory().command, "defend");
    press(&mut g, Key::Char('1'));
    assert_eq!(g.territory().command, "follow");
}

// -- the end of a raid (test_adventure_hero.py) ----------------------------------------

#[test]
fn a_won_raid_is_recorded_and_the_overlay_closes_after_the_title() {
    let state = scratch("victory");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let nest = g.territory().site("nest").expect("the raid has a nest");
    let (mission, host) = (g.mission.territory_mut(), &mut g.manager);
    mission.capture(host, nest);
    assert_eq!(g.territory().state, "victory");
    let record = mission_record(&load_profile(&profile_path(&state.0)), "territory");
    assert_eq!(record.victories, 1);
    assert!(record.best_seconds.is_some());

    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    render(&mut g, &mut drawer);
    press(&mut g, Key::Char('W'));
    assert!(held(&g, 0).is_empty(), "no moves after the end");
    let mut ticks = 0;
    while !g.is_stopping() {
        g.tick(0.05);
        ticks += 1;
        assert!(ticks < 200, "the end title never finished");
    }
    assert!(ticks >= 80, "the title shows for its 4.5 s ({ticks} ticks)");
    assert!(g.take_effects().contains(&Effect::Stop));
}

#[test]
fn a_fallen_hero_ends_the_raid_as_a_defeat() {
    let state = scratch("defeat");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    let hero = g.territory().players[0].creature;
    spider_mut(&mut g.manager, hero).take_damage(100000.0, None);
    g.tick(0.02);
    assert_eq!(g.territory().state, "defeat");
    assert!(!g.is_stopping(), "the title shows first");
    g.territory_mut().end_clock = TerritoryMission::END_SCREEN_SECONDS;
    g.tick(0.02);
    assert!(g.is_stopping(), "the overlay closes so the Adventure page comes back");
    assert!(g.take_effects().contains(&Effect::Stop));
    let record = mission_record(&load_profile(&profile_path(&state.0)), "territory");
    assert_eq!(record.defeats, 1);
}

#[test]
fn defeat_waits_for_both_players() {
    let state = scratch("coop-defeat");
    let mut g = game(&state, |p| p.two_player = true);
    g.tick(0.02);
    let hero = g.territory().players[0].creature;
    spider_mut(&mut g.manager, hero).take_damage(100000.0, None);
    g.tick(0.02);
    assert_eq!(g.territory().state, "active");
    let second = g.territory().players[1].creature;
    spider_mut(&mut g.manager, second).take_damage(100000.0, None);
    g.tick(0.02);
    assert_eq!(g.territory().state, "defeat");
}

#[test]
fn leaving_from_the_pause_menu_saves_the_hero_and_stops() {
    let state = scratch("exit");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    std::fs::remove_file(profile_path(&state.0)).unwrap();
    press(&mut g, Key::Esc);
    g.apply(&EngineCommand::ExitMission);
    assert!(g.is_stopping());
    assert!(g.take_effects().contains(&Effect::Stop));
    assert!(profile_path(&state.0).exists(), "what the hero earned is kept");
    assert!(!state.0.join("creatures.json").exists(), "the Companion colony is not overwritten");
}

#[test]
fn restart_builds_a_fresh_raid() {
    let state = scratch("restart");
    let mut g = game(&state, |_| {});
    for _ in 0..10 {
        g.tick(0.05);
    }
    press(&mut g, Key::Esc);
    g.apply(&EngineCommand::RestartMission);
    assert!(!g.paused);
    assert_eq!(g.territory().elapsed, 0.0);
    assert_eq!(g.territory().state, "active");
    g.tick(0.05);
    assert!(g.territory().elapsed > 0.0);
}

#[test]
fn changed_controls_are_picked_up() {
    let state = scratch("controls");
    let mut g = game(&state, |_| {});
    let mut settings = ControlSettings::default();
    settings.rebind(Action::Release, "F9");
    assert!(save_controls(&settings, &controls_path(&state.0)));
    g.apply(&EngineCommand::ControlsChanged);
    assert_eq!(g.controls.binding(Action::Release), "F9");
    assert!(g.take_effects().contains(&Effect::RegisterReleaseHotkey("F9".into())));
    press(&mut g, Key::F(9));
    assert!(g.controls_released);
}

// -- frames --------------------------------------------------------------------------------

#[test]
fn an_adventure_frame_paints_the_hud_and_the_banner() {
    let state = scratch("frame");
    let mut g = game(&state, |_| {});
    for _ in 0..4 {
        g.tick(0.02);
    }
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let image = render(&mut g, &mut drawer);
    let window = g.hud_window();
    assert!(painted_share(&image, hud_rect(&window, 0)) > 0.9, "the player HUD is drawn");
    assert!(painted_share(&image, mission_banner_rect(&window)) > 0.9, "the objective banner is drawn");
    let hero = hero_position(&g);
    let around = IRect::new(hero.0 as i32 - 12, hero.1 as i32 - 12, 24, 24);
    assert!(painted_share(&image, around) > 0.0, "the hero is drawn");
}

#[test]
fn reclaim_freezes_the_desktop_and_ends_on_the_blue_screen() {
    let state = scratch("reclaim");
    let mut g = game(&state, |p| {
        p.selected_map = "reclaim".into();
        p.admin = true;
    });
    let Mission::Reclaim(_) = &g.mission else { panic!("not Reclaim") };
    assert!(g.captures_mouse(5.0, 5.0));
    g.tick(0.02);
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let area = g.territory().area;
    let image = render(&mut g, &mut drawer);
    assert!(covered(&image, area) > 0.97, "the frozen desktop covers the screen");
    if let Mission::Reclaim(r) = &mut g.mission {
        r.crash();
    }
    assert_eq!(g.territory().state, "crashed");
    for _ in 0..40 {
        g.tick(0.05);
    }
    let image = render(&mut g, &mut drawer);
    assert!(covered(&image, area) > 0.97, "the blue screen covers the screen");
    while !g.is_stopping() {
        g.tick(0.05);
        assert!(g.territory().end_clock < 20.0);
    }
}

/// Offscreen frames for a person to look at: set `DC_ADVENTURE_FRAMES` to a
/// folder and run this test; it writes raid, map view, released, two-player,
/// victory and Reclaim frames there as PNG.
#[test]
fn frames_to_png_on_request() {
    let Some(out) = std::env::var_os("DC_ADVENTURE_FRAMES").map(PathBuf::from) else { return };
    std::fs::create_dir_all(&out).unwrap();
    let mut drawer = AdventureDrawer::new(PaintSwitches::default());
    let save = |g: &mut AdventureGame, drawer: &mut AdventureDrawer, name: &str| {
        render(g, drawer).save_png(out.join(format!("{name}.png"))).unwrap();
    };
    let state = scratch("frames");
    let mut g = game(&state, |_| {});
    g.cursor = (900.0, 420.0);
    for _ in 0..30 {
        g.tick(0.05);
    }
    save(&mut g, &mut drawer, "raid");
    g.toggle_map_view();
    save(&mut g, &mut drawer, "map_view");
    g.toggle_map_view();
    g.toggle_controls();
    save(&mut g, &mut drawer, "released");
    g.toggle_controls();
    let nest = g.territory().site("nest").unwrap();
    let (mission, host) = (g.mission.territory_mut(), &mut g.manager);
    mission.capture(host, nest);
    for _ in 0..10 {
        g.tick(0.05);
    }
    save(&mut g, &mut drawer, "victory");

    let coop = scratch("frames-coop");
    let mut g = game(&coop, |p| p.two_player = true);
    for _ in 0..20 {
        g.tick(0.05);
    }
    save(&mut g, &mut drawer, "coop");

    let reclaim = scratch("frames-reclaim");
    let mut g = game(&reclaim, |p| {
        p.selected_map = "reclaim".into();
        p.admin = true;
    });
    for _ in 0..20 {
        g.tick(0.05);
    }
    save(&mut g, &mut drawer, "reclaim");
    if let Mission::Reclaim(r) = &mut g.mission {
        r.crash();
    }
    for _ in 0..60 {
        g.tick(0.05);
    }
    save(&mut g, &mut drawer, "blue_screen");
}

#[test]
fn the_menus_announce_set_the_frame_rate_and_save() {
    let state = scratch("menu-commands");
    let mut g = game(&state, |_| {});
    g.tick(0.02);
    assert!(g.apply(&EngineCommand::Announce("Saved".into())));
    assert!(g.apply(&EngineCommand::SetTargetFps(30.0)));
    assert_eq!(g.take_effects(), vec![Effect::Announce("Saved".into()), Effect::SetTargetFps(30.0)]);

    // "Retry saving victory" writes again even after a save went through.
    let path = profile_path(&state.0);
    let nest = g.territory().site("nest").unwrap();
    let (mission, host) = (g.mission.territory_mut(), &mut g.manager);
    mission.capture(host, nest);
    assert!(g.territory().saved);
    std::fs::remove_file(&path).unwrap();
    assert!(g.apply(&EngineCommand::SaveProgress));
    assert!(path.exists());
    assert_eq!(mission_record(&load_profile(&path), "territory").victories, 1);
}
