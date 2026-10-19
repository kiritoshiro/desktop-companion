//! Adventure mode in the overlay: the half of `app/engine.py`'s
//! `OverlayWindow` that runs a mission instead of the Companion colony.
//!
//! [`AdventureGame`] holds the manager, the mission and every Adventure flag
//! of the window (`map_view`, `controls_released`, `_adventure_paused`, the
//! dragged HUD) and does what `tick`, `keyPressEvent`, `_adventure_action`,
//! `mousePressEvent`, `_toggle_map_view`, `_toggle_controls`,
//! `_show_adventure_pause` and `_do_graceful_stop` did with them. It owns no
//! window: what it wants from the window (a menu shown, the keyboard back,
//! the release hotkey, a tray message, the overlay closed) comes out as
//! [`Effect`]s the engine carries out, so tests drive it headlessly with
//! synthetic keys.
//!
//! - [`input`]: winit keys and buttons to the controls' names.
//! - [`draw`]: the backdrop, buildings, spiders, HUDs and banners.
//! - [`screens`]: the playable monitors and the Reclaim desktop capture.
//!
//! Python's modal dialogs became requests to `OverlayUi`; the raid stays
//! paused until the UI layer sends [`EngineCommand::Resume`] (or reports
//! that nothing is open any more, see [`AdventureGame::set_ui_open`]).

pub mod draw;
pub mod input;
pub mod screens;

use crate::overlay_ui::{EngineCommand, UiRequest};
use dc_adventure::controls::{Action, ControlSettings, Key, MouseButton, controls_path, load_controls};
use dc_adventure::mission::{MissionHost, SpiderId, TerritoryMission};
use dc_adventure::mission_factory::{Mission, create_mission};
use dc_art::desktop_surface::SurfaceSnapshot;
use dc_art::mission_ui::{HudWindow, command_rects, hud_rect, mission_banner_rect};
use dc_core::{IRect, PyRandom};
use dc_creature::{Creature, LabelMetrics};
use dc_manager::CreatureManager;
use dc_world::ScreenRect;
use input::KeyInput;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How long a pause waits for the UI layer to show something before it
/// gives the raid back (a UI layer that opens nothing must not freeze it).
pub const UI_GRACE_SECONDS: f64 = 0.5;

/// Something the game wants from the window or the process.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Show a menu or dialog (`OverlayUi::request`).
    Ui(UiRequest),
    /// `_announce`: a short tray message.
    Announce(String),
    /// Take the keyboard (raise, `activateWindow`, `setFocus`).
    Activate,
    /// Controls released: the desktop gets the mouse and keyboard back.
    Deactivate,
    /// (Re-)register the release key system-wide (`_register_release_hotkey`).
    RegisterReleaseHotkey(String),
    /// `set_target_fps`: the frame rate the player chose.
    SetTargetFps(f64),
    /// Progress is saved: close the overlay (`_do_graceful_stop`).
    Stop,
}

/// Why the raid is paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    /// Esc / the pause binding: the pause menu.
    Menu,
    /// K / the skills binding: the character window.
    Character,
}

/// Everything needed to start a raid without a window.
pub struct StartOptions<'a> {
    /// Content root (models, personalities, presets).
    pub root: &'a Path,
    pub preset: &'a Path,
    pub state_dir: &'a Path,
    /// The overlay's size (physical pixels).
    pub size: (f64, f64),
    /// The playable monitors' usable areas, overlay-local, and the main one.
    pub rects: Vec<ScreenRect>,
    pub primary: usize,
    /// The main monitor's usable area, overlay-local (`primary_rect_local`).
    pub primary_rect: Option<IRect>,
    pub seed: Option<i64>,
    pub label_metrics: Option<Box<dyn LabelMetrics>>,
}

/// `spider(host, id)`: the mission's view of a creature. Spelled out because
/// `CreatureManager` has an inherent `creature` with another signature.
pub fn spider(m: &CreatureManager, id: SpiderId) -> &Creature {
    MissionHost::creature(m, id)
}

pub fn spider_mut(m: &mut CreatureManager, id: SpiderId) -> &mut Creature {
    MissionHost::creature_mut(m, id)
}

pub struct AdventureGame {
    pub manager: CreatureManager,
    pub mission: Mission,
    /// The mission's own stream (`TerritoryMission.rng`).
    pub rng: PyRandom,
    pub state_dir: PathBuf,
    /// `self.controls`: player one's bindings from `controls.json`.
    pub controls: ControlSettings,
    controls_mtime: Option<SystemTime>,
    /// The mission's painted map instead of the desktop (the ` / ~ key).
    pub map_view: bool,
    /// The spider's controls handed back to the computer (F8).
    pub controls_released: bool,
    /// `_adventure_paused`.
    pub paused: bool,
    pub pause_reason: Option<PauseReason>,
    pause_clock: f64,
    ui_open: bool,
    ui_seen_open: bool,
    /// The release key is a registered Windows hotkey (`_release_hotkey`),
    /// so a key press in the window must not toggle it a second time.
    pub release_hotkey: bool,
    /// `_adventure_hud_position` / `_adventure_hud_drag_offset`.
    pub hud_position: Option<(i32, i32)>,
    hud_drag_offset: Option<(i32, i32)>,
    left_down: bool,
    /// The pointer, overlay-local.
    pub cursor: (f64, f64),
    /// The overlay's size and the main monitor in it.
    pub size: (f64, f64),
    pub primary_rect: Option<IRect>,
    stopping: bool,
    effects: Vec<Effect>,
}

impl AdventureGame {
    /// `OverlayWindow.__init__` in Adventure: the manager on the preset, then
    /// the mission chosen in the adventure profile (`create_mission`).
    /// `capture` is asked for the desktop only for Reclaim the desktop.
    pub fn start(
        opts: StartOptions<'_>,
        capture: Option<&mut dyn FnMut() -> Option<SurfaceSnapshot>>,
    ) -> Result<Self, String> {
        let mut manager_rng = match opts.seed {
            Some(seed) => PyRandom::from_int(seed as i128),
            None => PyRandom::from_entropy(),
        };
        let mut manager =
            CreatureManager::new(opts.root, opts.preset, opts.size, opts.seed, opts.state_dir, true, &mut manager_rng)
                .map_err(|e| format!("could not load {}: {e:?}", opts.preset.display()))?;
        if let Some(metrics) = opts.label_metrics {
            manager.set_label_metrics(metrics);
        }
        for warning in &manager.warnings {
            log::warn!("{warning}");
        }
        let rects =
            if opts.rects.is_empty() { vec![ScreenRect::new(0.0, 0.0, opts.size.0, opts.size.1)] } else { opts.rects };
        manager.set_screen_rects(&rects);
        // Python's mission draws from an unseeded `random.Random()`; a seeded
        // run seeds it too, so `--seed` replays the raid.
        let mut rng = match opts.seed {
            Some(seed) => PyRandom::from_int(seed as i128),
            None => PyRandom::from_entropy(),
        };
        let path = controls_path(opts.state_dir);
        let controls = load_controls(&path);
        let mission =
            create_mission(&mut manager, controls.clone(), &rects, opts.primary, opts.state_dir, capture, &mut rng)?;
        let mut game = Self {
            manager,
            mission,
            rng,
            state_dir: opts.state_dir.to_path_buf(),
            controls,
            controls_mtime: dc_platform::session_control::file_mtime(&path),
            map_view: false,
            controls_released: false,
            paused: false,
            pause_reason: None,
            pause_clock: 0.0,
            ui_open: false,
            ui_seen_open: false,
            release_hotkey: false,
            hud_position: None,
            hud_drag_offset: None,
            left_down: false,
            cursor: (-100000.0, -100000.0),
            size: opts.size,
            primary_rect: opts.primary_rect,
            stopping: false,
            effects: Vec::new(),
        };
        let release = game.controls.binding(Action::Release).to_string();
        game.effects.push(Effect::RegisterReleaseHotkey(release));
        game.effects.push(Effect::Activate);
        log::info!(
            "Adventure: {} ({:?}) on {} screen(s), {} player(s)",
            game.territory().map_info.id,
            game.territory().map_info.kind,
            game.territory().layout.count(),
            game.territory().players.len()
        );
        Ok(game)
    }

    pub fn territory(&self) -> &TerritoryMission {
        self.mission.territory()
    }

    pub fn territory_mut(&mut self) -> &mut TerritoryMission {
        self.mission.territory_mut()
    }

    pub fn player_count(&self) -> usize {
        self.territory().players.len()
    }

    /// Player `index`'s spider.
    #[cfg(test)]
    pub fn player_spider(&self, index: usize) -> &Creature {
        spider(&self.manager, self.territory().players[index].creature)
    }

    /// Effects queued since the last call.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping
    }

    fn announce(&mut self, text: impl Into<String>) {
        self.effects.push(Effect::Announce(text.into()));
    }

    /// What `hud_rect` and the other HUD layouts need of the window.
    pub fn hud_window(&self) -> HudWindow {
        HudWindow {
            width: self.size.0 as i32,
            height: self.size.1 as i32,
            primary: self.primary_rect,
            hud_position: self.hud_position,
            players: self.player_count(),
        }
    }

    /// The overlay moved to a new virtual-desktop size.
    pub fn resize(&mut self, size: (f64, f64), screen_rects: &[ScreenRect], primary_rect: Option<IRect>) {
        self.size = size;
        self.primary_rect = primary_rect;
        self.manager.resize(size.0, size.1);
        self.manager.set_screen_rects(screen_rects);
    }

    // -- the mouse ---------------------------------------------------------------

    /// `_adventure_captures_mouse`: capture the screen while playing; after
    /// release, every click goes to the desktop.
    pub fn captures_mouse(&self, x: f64, y: f64) -> bool {
        if self.controls_released {
            return false;
        }
        // A mission always has its player (`self.player is not None`).
        if !self.territory().players.is_empty() || self.paused {
            return true;
        }
        if matches!(self.mission, Mission::Reclaim(_)) {
            // Reclaim the desktop (`FREEZES_DESKTOP`): frozen until the raid ends.
            return true;
        }
        self.manager.creature_at(x, y).is_some()
    }

    fn input_blocked(&self) -> bool {
        self.paused || self.ui_open
    }

    /// `mousePressEvent` in Adventure.
    pub fn mouse_press(&mut self, button: MouseButton, x: f64, y: f64) {
        if button == MouseButton::Left {
            self.left_down = true;
        }
        if self.input_blocked() || self.stopping {
            return;
        }
        let local = (x as i32, y as i32);
        let window = self.hud_window();
        if button == MouseButton::Left {
            for (command, rect) in command_rects(&window) {
                if rect.contains(local.0, local.1) {
                    let aim = self.territory().players[0].control.aim;
                    let (mission, manager) = (self.mission.territory_mut(), &self.manager);
                    mission.issue(manager, command, aim);
                    return;
                }
            }
            if mission_banner_rect(&window).contains(local.0, local.1) {
                return;
            }
        }
        if button == MouseButton::Left && self.player_count() < 2 {
            let r = hud_rect(&window, 0);
            if r.contains(local.0, local.1) {
                self.hud_drag_offset = Some((local.0 - r.x, local.1 - r.y));
                return;
            }
        }
        let action = self.territory().players[0].control.controls.action_for_mouse(button);
        self.action(action, true, false, 0, Some((x, y)));
    }

    /// `mouseReleaseEvent` in Adventure.
    pub fn mouse_release(&mut self, button: MouseButton) {
        if button == MouseButton::Left {
            self.left_down = false;
            if self.hud_drag_offset.take().is_some() {
                return;
            }
        }
        // A mouse button bound to a held action (sprint, a direction).
        let action = self.territory().players[0].control.controls.action_for_mouse(button);
        self.action(action, false, false, 0, None);
    }

    /// `mouseMoveEvent`: the pointer, and the HUD while it is dragged.
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        self.cursor = (x, y);
        if let Some((dx, dy)) = self.hud_drag_offset {
            if self.left_down {
                self.hud_position = Some((x as i32 - dx, y as i32 - dy));
            } else {
                self.hud_drag_offset = None;
            }
        }
    }

    /// `focusOutEvent`: nothing stays held when the window loses the keyboard.
    pub fn focus_lost(&mut self) {
        self.hud_drag_offset = None;
        self.left_down = false;
        self.territory_mut().clear_player_keys();
    }

    // -- the keyboard ------------------------------------------------------------

    /// `keyPressEvent` / `keyReleaseEvent` in Adventure.
    pub fn key(&mut self, k: KeyInput) {
        if k.pressed { self.key_press(k) } else { self.key_release(k) }
    }

    fn key_press(&mut self, k: KeyInput) {
        let players = self.player_count();
        if players > 1 {
            // Each key belongs to one player; Esc, ` and F8 to player one.
            for i in 0..players {
                let controls = &self.territory().players[i].control.controls;
                let mut action = controls.action_for_key(k.key);
                if action.is_none() && k.under_esc && controls.binding(Action::MapView) == "`" {
                    action = Some(Action::MapView);
                }
                if action.is_some() {
                    self.action(action, true, k.repeat, i, None);
                    break;
                }
            }
            return;
        }
        let mut action = self.controls.action_for_key(k.key);
        if action.is_none() && k.under_esc && self.controls.binding(Action::MapView) == "`" {
            action = Some(Action::MapView);
        }
        if k.key == Key::Esc && action.is_none() {
            // Esc always pauses, whatever pause is bound to, so a player who
            // rebinds it can never lock themselves in.
            action = Some(Action::Pause);
        }
        self.action(action, true, k.repeat, 0, None);
    }

    fn key_release(&mut self, k: KeyInput) {
        if k.repeat {
            return;
        }
        for i in 0..self.player_count() {
            let action = self.territory().players[i].control.controls.action_for_key(k.key);
            self.action(action, false, false, i, None);
        }
    }

    /// `_adventure_action`: do what a bound key or mouse button means for
    /// `player`. `pointer` is the click position, given only by a mouse press
    /// (the shot goes where the player clicked). True if it meant anything.
    pub fn action(
        &mut self,
        action: Option<Action>,
        down: bool,
        repeat: bool,
        player: usize,
        pointer: Option<(f64, f64)>,
    ) -> bool {
        let Some(action) = action else { return false };
        if action == Action::Release {
            // With the global hotkey registered, Windows delivers the key as
            // WM_HOTKEY instead; this path serves when it could not be.
            if down && !repeat && !self.release_hotkey {
                self.toggle_controls();
            }
            return true;
        }
        if self.controls_released || self.stopping || self.ui_open {
            // Released, closing, or a menu or dialog has the input.
            return true;
        }
        match action {
            Action::MapView => {
                if down && !repeat {
                    self.toggle_map_view();
                }
                return true;
            }
            Action::Pause => {
                if down && !repeat {
                    self.show_pause();
                }
                return true;
            }
            Action::Skills => {
                if down && !repeat {
                    self.show_skills(player);
                }
                return true;
            }
            _ => {}
        }
        let player = player.min(self.player_count().saturating_sub(1));
        if let Some(pos) = pointer {
            self.territory_mut().players[player].control.aim = pos;
        }
        let (mission, manager) = (self.mission.territory_mut(), &mut self.manager);
        if mission.ended() {
            return true;
        }
        let command = match action {
            Action::CompanionFollow => Some("follow"),
            Action::CompanionDefend => Some("defend"),
            Action::CompanionAttack => Some("attack"),
            _ => None,
        };
        if let Some(command) = command {
            if down && !repeat && !self.paused {
                let aim = mission.players[player].control.aim;
                mission.issue(manager, command, aim);
            }
            return true;
        }
        if self.paused {
            return true;
        }
        let p = &mut mission.players[player];
        match action {
            Action::MoveUp
            | Action::MoveDown
            | Action::MoveLeft
            | Action::MoveRight
            | Action::Sprint
            | Action::Bite => p.control.set_held(action, down),
            Action::Jump if down && !repeat => {
                p.control.jump(spider_mut(manager, p.creature));
            }
            Action::Shoot if down && !repeat => {
                TerritoryMission::shoot(manager, p.creature, &mut p.control);
            }
            _ => {}
        }
        true
    }

    // -- the map, releasing the controls, pausing -----------------------------------

    /// `_toggle_map_view`.
    pub fn toggle_map_view(&mut self) {
        self.map_view = !self.map_view;
        self.announce(if self.map_view { "Showing the mission map" } else { "Showing your desktop" });
    }

    /// `_toggle_controls`: hand the mouse and keyboard to the computer (the
    /// raid pauses), or take them back for the spider. Both players let go.
    pub fn toggle_controls(&mut self) {
        self.controls_released = !self.controls_released;
        self.hud_drag_offset = None;
        self.territory_mut().clear_player_keys();
        if self.controls_released {
            self.effects.push(Effect::Deactivate);
            let key = self.controls.binding(Action::Release).to_string();
            self.announce(format!("Controls released - the raid is paused. Press {key} to take them back."));
        } else {
            self.effects.push(Effect::Activate);
            self.announce("Controls back on your spider");
        }
    }

    /// The release key arrived as a Windows hotkey (`WM_HOTKEY`).
    pub fn release_hotkey_pressed(&mut self) {
        self.toggle_controls();
    }

    fn pause(&mut self, reason: PauseReason) {
        self.paused = true;
        self.pause_reason = Some(reason);
        self.pause_clock = 0.0;
        self.ui_seen_open = false;
        let mission = self.territory_mut();
        mission.clear_player_keys();
        mission.players[0].control.paused = true;
    }

    /// `_show_adventure_pause`: freeze the raid and ask for the pause menu.
    pub fn show_pause(&mut self) {
        if self.paused {
            return;
        }
        self.pause(PauseReason::Menu);
        self.effects.push(Effect::Ui(UiRequest::Pause));
    }

    /// `_show_adventure_skills` / `_open_character_window`: the raid's
    /// progress is saved first, so the character window edits what the
    /// spiders have earned; `ProfileChanged` puts it back on them.
    pub fn show_skills(&mut self, player: usize) {
        if self.paused {
            return;
        }
        self.pause(PauseReason::Character);
        let (mission, manager) = (self.mission.territory_mut(), &self.manager);
        mission.save_progress(manager);
        self.effects.push(Effect::Ui(UiRequest::Character { player }));
    }

    /// The pause is over (`_adventure_paused = False`, `setFocus`).
    pub fn resume(&mut self) {
        if !self.paused {
            return;
        }
        self.paused = false;
        self.pause_reason = None;
        self.territory_mut().players[0].control.paused = false;
        self.effects.push(Effect::Activate);
    }

    /// Whether `OverlayUi` has something open, once a frame. Game input waits
    /// while it does; a pause ends when the UI closes everything (or showed
    /// nothing within [`UI_GRACE_SECONDS`]).
    pub fn set_ui_open(&mut self, open: bool, dt: f64) {
        self.ui_open = open;
        if open {
            self.ui_seen_open = true;
            return;
        }
        if self.paused {
            self.pause_clock += dt.max(0.0);
            if self.ui_seen_open || self.pause_clock >= UI_GRACE_SECONDS {
                self.resume();
            }
        }
    }

    /// Apply what the menus and dialogs asked for. Returns false for the
    /// commands that are not the raid's: spider and overlay actions, which
    /// the engine carries out on the manager.
    pub fn apply(&mut self, command: &EngineCommand) -> bool {
        match command {
            EngineCommand::Resume => self.resume(),
            EngineCommand::ExitMission | EngineCommand::Quit => self.stop(),
            EngineCommand::RestartMission => self.restart(),
            EngineCommand::ProfileChanged => {
                let (mission, manager) = (self.mission.territory_mut(), &mut self.manager);
                mission.refresh_from_profile(manager);
            }
            EngineCommand::ControlsChanged => self.reload_controls_if_changed(true),
            EngineCommand::Announce(text) => self.announce(text.clone()),
            EngineCommand::SetTargetFps(fps) => self.effects.push(Effect::SetTargetFps(*fps)),
            // The pause menu's save / "Retry saving victory" / exit, and the
            // save before the character window: always written, even when an
            // earlier save succeeded.
            EngineCommand::SaveProgress => {
                let (mission, manager) = (self.mission.territory_mut(), &self.manager);
                if !mission.save_progress(manager) {
                    log::warn!("Could not save the Adventure hero: {}", mission.save_error);
                }
            }
            EngineCommand::SpiderAction { .. } | EngineCommand::OverlayAction { .. } => return false,
        }
        true
    }

    /// The pause menu's Restart (`mission.restarted()`).
    pub fn restart(&mut self) {
        match self.mission.restarted(&mut self.manager, &mut self.rng) {
            Ok(mission) => {
                self.mission = mission;
                self.paused = true;
                self.resume();
            }
            Err(e) => {
                log::warn!("Could not restart the mission: {e}");
                self.announce(format!("Could not restart the mission: {e}"));
                self.resume();
            }
        }
    }

    /// `_reload_controls_if_changed`: new bindings from `controls.json`.
    pub fn reload_controls_if_changed(&mut self, force: bool) {
        let path = controls_path(&self.state_dir);
        let mtime = dc_platform::session_control::file_mtime(&path);
        if !force && mtime == self.controls_mtime {
            return;
        }
        self.controls_mtime = mtime;
        self.controls = load_controls(&path);
        let single = self.player_count() < 2;
        let controls = self.controls.clone();
        let p = &mut self.territory_mut().players[0];
        if single {
            p.control.controls = controls;
        }
        p.control.clear_keys();
        let release = self.controls.binding(Action::Release).to_string();
        self.effects.push(Effect::RegisterReleaseHotkey(release));
    }

    // -- the frame -----------------------------------------------------------------

    /// The Adventure half of `OverlayWindow.tick`, after the timers: aim at
    /// the pointer, run the mission, close after the end screen.
    pub fn tick(&mut self, dt: f64) {
        if self.stopping || self.controls_released {
            // The raid waits while the computer has the controls.
            return;
        }
        let cursor = self.cursor;
        self.territory_mut().players[0].control.aim = cursor;
        if self.paused {
            return;
        }
        self.mission.update(&mut self.manager, dt, &mut self.rng);
        if self.mission.end_screen_done() {
            // The VICTORY / DEFEAT title has had its moment: close, and the
            // settings window comes back on the Adventure page.
            self.stop();
        }
    }

    /// `_do_graceful_stop`: leaving mid-raid keeps what the hero has earned.
    /// With a mission the manager's `save_runtime_state` is the mission's
    /// `save_progress`; the Companion colony file is left alone.
    pub fn stop(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        self.save();
        self.effects.push(Effect::Stop);
    }

    /// `manager.save_runtime_state()` with a mission: its `save_progress`.
    pub fn save(&mut self) {
        let (mission, manager) = (self.mission.territory_mut(), &self.manager);
        if !mission.saved && !mission.save_progress(manager) {
            log::warn!("Could not save the Adventure hero: {}", mission.save_error);
        }
    }

    /// Whether player one's spider still lives.
    #[cfg(test)]
    pub fn hero_alive(&self) -> bool {
        !dc_adventure::adventure::ControlledSpider::dead(self.player_spider(0))
    }
}

#[cfg(test)]
mod tests;
