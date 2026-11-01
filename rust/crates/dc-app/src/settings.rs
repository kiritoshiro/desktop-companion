//! The settings process (`app/config_ui.py` `ConfigWindow` and `main`): the
//! wood-themed window from dc-ui, with what Python's window did around it.
//! It launches and stops the overlay (this same executable with
//! `--engine`), talks to it over the live channel, and carries out the
//! window's actions on presets, the Adventure profile, controls and maps.
//! The character, controls, map-editor and admin dialogs open on top of the
//! window (they were modal `QDialog`s).

use dc_adventure::adventure_profile::{
    self as profile_mod, AdventureProfile, HERO, PLAYER_TWO, clean_name, load_profile, profile_path, save_profile,
};
use dc_adventure::{admin, armoury, campaign, controls as controls_mod, custom_maps};
use dc_platform::live_channel::{ChannelMessage, ClientEvent, SettingsChannelClient, channel_name};
use dc_platform::session_control::{self, StopOutcome};
use dc_ui::model::ScreenInfo;
use dc_ui::windows::admin::AdminDialog;
use dc_ui::windows::character::CharacterWindow;
use dc_ui::windows::config::ConfigWindow;
use dc_ui::windows::controls::ControlsDialog;
use dc_ui::windows::map_editor::MapEditor;
use dc_ui::{OverlayMode, PollOutcome, RunOptions, UiAction, WindowApp, bridge};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod thumbnails;

/// Starts and watches the overlay process. A trait so tests never start one.
pub trait Launcher {
    fn launch(&mut self, preset: &Path, mode: OverlayMode) -> Result<(), String>;
    fn running(&mut self) -> bool;
    /// Ask it to save and quit (the stop-request file), then kill it if needed.
    fn stop(&mut self, state_dir: &Path) -> StopOutcome;
    /// The exit code once it has stopped on its own, reported once.
    fn take_exit(&mut self) -> Option<i32>;
}

/// The overlay's arguments after the executable: the frozen build's
/// `[exe, "--engine", "--preset", path, "--mode", mode]`.
pub fn engine_args(preset: &Path, mode: OverlayMode) -> Vec<OsString> {
    vec!["--engine".into(), "--preset".into(), preset.as_os_str().to_owned(), "--mode".into(), mode.as_str().into()]
}

/// This executable with `--engine` (the frozen build's command).
#[derive(Default)]
pub struct ProcessLauncher {
    child: Option<std::process::Child>,
}

impl Launcher for ProcessLauncher {
    fn launch(&mut self, preset: &Path, mode: OverlayMode) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut child =
            std::process::Command::new(exe).args(engine_args(preset, mode)).spawn().map_err(|e| e.to_string())?;
        // Spawning succeeds whenever the executable starts; an overlay that
        // fails a moment later shows up as an exit code (DC-43).
        std::thread::sleep(Duration::from_millis(600));
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "The overlay process exited with code {} right after starting. The log file has the details.",
                status.code().unwrap_or(-1)
            ));
        }
        self.child = Some(child);
        Ok(())
    }

    fn running(&mut self) -> bool {
        self.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
    }

    fn stop(&mut self, state_dir: &Path) -> StopOutcome {
        let outcome = session_control::stop_process_default(self.child.as_mut(), state_dir);
        if outcome != StopOutcome::StillRunning {
            self.child = None;
        }
        outcome
    }

    fn take_exit(&mut self) -> Option<i32> {
        let code = self.child.as_mut().and_then(|c| c.try_wait().ok().flatten()).map(|s| s.code().unwrap_or(-1));
        if code.is_some() {
            self.child = None;
        }
        code
    }
}

/// A dialog open over the settings window.
pub enum Dialog {
    Character(Box<CharacterWindow>),
    Controls(ControlsDialog),
    MapEditor(Box<MapEditor>),
    Admin(AdminDialog),
}

pub struct SettingsHost<L: Launcher> {
    pub window: ConfigWindow,
    pub dialog: Option<Dialog>,
    root: Option<PathBuf>,
    state_dir: PathBuf,
    launcher: L,
    client: SettingsChannelClient,
    overlay_mode: Option<OverlayMode>,
    launched_preset: Option<PathBuf>,
    /// Hidden while an Adventure overlay runs; shown again when it exits.
    hidden_for_adventure: bool,
    /// Set by the handlers, read by `poll`.
    visibility: Option<bool>,
    /// The frame that asked for Adventure hid the window before the launch
    /// (`start_adventure` hid it before starting the overlay); shown again
    /// unless the Adventure overlay is now running.
    hid_for_start: bool,
    /// The monitors for the raid setup card (dc-platform; tests pass their own).
    screens: fn() -> Vec<ScreenInfo>,
    /// The skin thumbnails need the window's context, so they are made on
    /// the first frame (and again after the library is reloaded).
    thumbnails_installed: bool,
}

impl<L: Launcher> SettingsHost<L> {
    pub fn new(root: Option<PathBuf>, state_dir: PathBuf, launcher: L) -> Self {
        let catalog = bridge::settings_catalog(root.as_deref());
        let presets = bridge::presets(root.as_deref());
        let mut window = ConfigWindow::new(catalog);
        window.set_presets(presets);
        // `find_data_file("presets", "default.json", root=self.root)`; with
        // none the window keeps its one fresh slot.
        let default = dc_content::discovery::find_data_file(&["presets", "default.json"], root.as_deref());
        if default.exists() {
            match dc_content::preset_io::load_preset_value(&default) {
                Ok(data) => {
                    window.load_preset(&data);
                    window.set_status(format!("Loaded preset: {}", default.display()));
                }
                Err(e) => window.set_status(format!("Could not load preset: {e}")),
            }
        }
        let client = SettingsChannelClient::new(channel_name(&state_dir));
        let mut host = Self {
            window,
            dialog: None,
            root,
            state_dir,
            launcher,
            client,
            overlay_mode: None,
            launched_preset: None,
            hidden_for_adventure: false,
            visibility: None,
            hid_for_start: false,
            screens: Self::system_screens,
            thumbnails_installed: false,
        };
        host.refresh_adventure();
        host
    }

    fn profile_path(&self) -> PathBuf {
        profile_path(&self.state_dir)
    }

    fn profile(&self) -> AdventureProfile {
        load_profile(&self.profile_path())
    }

    fn save(&mut self, profile: &AdventureProfile) {
        if !save_profile(profile, &self.profile_path()) {
            self.window.set_status(format!("Could not write {}", self.profile_path().display()));
        }
    }

    /// Use `screens` for the raid setup card instead of the real monitors.
    #[cfg(test)]
    pub fn set_screens(&mut self, screens: fn() -> Vec<ScreenInfo>) {
        self.screens = screens;
        self.refresh_adventure();
    }

    /// The monitors for the raid setup card (`mode_menu.py`).
    fn system_screens() -> Vec<ScreenInfo> {
        let list = dc_platform::monitors::monitors();
        let primary = dc_platform::monitors::primary_index(&list);
        let states = dc_platform::screen_activity::monitor_states();
        list.iter()
            .enumerate()
            .map(|(i, m)| ScreenInfo {
                name: m.name.clone(),
                number: i + 1,
                width: m.rect.w.max(0) as u32,
                height: m.rect.h.max(0) as u32,
                primary: i == primary,
                state: states.get(&m.name).map_or("active", |s| s.as_str()).to_string(),
            })
            .collect()
    }

    /// `ModeShell.refresh_adventure`.
    pub fn refresh_adventure(&mut self) {
        let profile = self.profile();
        let controls = controls_mod::load_controls(&controls_mod::controls_path(&self.state_dir));
        let view = bridge::adventure_view(&profile, &self.state_dir, (self.screens)(), &controls);
        self.window.shell.refresh_adventure(view);
        if let Some(Dialog::Character(c)) = self.dialog.as_mut() {
            let party = bridge::party_view(&profile);
            match c.as_mut() {
                CharacterWindow::Single(d) => d.set_party(party),
                CharacterWindow::Party(d) => d.set_party(party),
            }
        }
    }

    fn channel_connected(&mut self, timeout_ms: u64) -> bool {
        self.client.is_connected() || self.client.try_connect(timeout_ms)
    }

    /// `_apply_live_if_running`: rewrite the running overlay's preset (the
    /// fallback it polls) and push it over the channel.
    fn apply_live_if_running(&mut self, data: &Value) -> bool {
        let Some(path) = self.launched_preset.clone() else { return false };
        if !self.launcher.running() {
            return false;
        }
        if let Err(e) = dc_content::preset_io::save_preset_value(data, Some(&path)) {
            self.window.set_status(format!("Could not apply live: {e}"));
            return false;
        }
        if self.channel_connected(100) {
            let message = ChannelMessage::PresetUpdate {
                preset_path: Some(path.to_string_lossy().into_owned()),
                data: data.clone(),
            };
            self.client.send(&message.to_map());
        }
        true
    }

    fn save_preset(&mut self, data: &Value) {
        match dc_content::preset_io::save_preset_value(data, None) {
            Ok(path) => {
                self.window.set_presets(bridge::presets(self.root.as_deref()));
                // `preset_combo.findData(str(path))`: select what was saved
                // when the library lists it.
                let saved = path.to_string_lossy().into_owned();
                if self.window.presets.iter().any(|p| p.path == saved) {
                    self.window.preset_choice = Some(saved);
                }
                let applied = self.apply_live_if_running(data);
                self.window.set_status(if applied {
                    format!("Saved your preset to {}\nApplied changes to the running overlay.", path.display())
                } else {
                    format!("Saved your preset to {}", path.display())
                });
            }
            Err(e) => self.window.set_status(format!("Could not save preset: {e}")),
        }
    }

    /// `launch_engine`.
    pub fn launch(&mut self, mode: OverlayMode, preset: &Value) {
        if self.launcher.running() && self.overlay_mode != Some(mode) && self.stop() == StopOutcome::StillRunning {
            // The other mode's overlay is still up; launching beside it would
            // run two.
            return;
        }
        if self.launcher.running() {
            if self.apply_live_if_running(preset) {
                self.window.set_status("Applied changes to the running overlay. No restart needed.");
            }
            return;
        }
        // The user's own folder: a shipped preset is never written over.
        let name = preset.get("name").and_then(Value::as_str).unwrap_or("Default");
        let path = dc_content::discovery::user_presets_dir().join(dc_content::preset_io::safe_preset_filename(name));
        let path = match dc_content::preset_io::save_preset_value(preset, Some(&path)) {
            Ok(p) => p,
            Err(e) => {
                self.window.set_status(format!("Could not prepare preset: {e}"));
                return;
            }
        };
        session_control::clear_stop_request(&self.state_dir);
        match self.launcher.launch(&path, mode) {
            Ok(()) => {
                self.launched_preset = Some(path);
                self.overlay_mode = Some(mode);
                self.window.set_overlay_running(true);
                self.window.set_status(
                    "Overlay launched. Edit and press Save to apply changes live, or Stop overlay to close it.",
                );
            }
            Err(e) => {
                log::error!("Overlay did not start: {e}");
                self.window.set_status(format!("Overlay failed to start. {e}"));
            }
        }
    }

    /// `start_adventure`: out of the way first, so Reclaim's picture of the
    /// desktop does not have this window in it.
    fn start_adventure(&mut self) {
        let preset = match self.window.current_preset_data() {
            Ok(p) => p,
            Err(e) => {
                self.window.set_status(e);
                return;
            }
        };
        self.visibility = Some(false);
        self.launch(OverlayMode::Adventure, &preset);
        if self.overlay_mode == Some(OverlayMode::Adventure) && self.launcher.running() {
            self.hidden_for_adventure = true;
        } else {
            self.visibility = Some(true);
        }
    }

    /// `stop_overlay`.
    pub fn stop(&mut self) -> StopOutcome {
        if !self.launcher.running() {
            self.launched_preset = None;
            session_control::clear_stop_request(&self.state_dir);
            self.window.set_overlay_running(false);
            self.window.set_status("No overlay process is running from this window.");
            return StopOutcome::NotRunning;
        }
        if self.channel_connected(100) {
            self.client.send(&ChannelMessage::StopRequest.to_map());
        }
        let outcome = self.launcher.stop(&self.state_dir);
        if outcome == StopOutcome::StillRunning {
            // Still up: keep tracking it as it is (its mode decides how a
            // reset reaches it), and say so.
            self.window.set_status("Overlay did not respond and could not be closed; it may still be running.");
            return outcome;
        }
        self.launched_preset = None;
        self.overlay_mode = None;
        self.window.set_overlay_running(false);
        self.window.set_status(match outcome {
            StopOutcome::Graceful => "Overlay stopped and saved its spiders.",
            StopOutcome::Terminated => {
                "Overlay did not respond and was closed; recent progress may not have been saved."
            }
            StopOutcome::NotRunning => "No overlay process is running from this window.",
            StopOutcome::StillRunning => "Overlay did not respond and could not be closed; it may still be running.",
        });
        outcome
    }

    /// `reset_saved_progress` (already confirmed in the window).
    ///
    /// Differs from Python: a running overlay this window cannot ask over the
    /// channel (unreachable, or an Adventure raid, which ignores the request)
    /// is stopped before the file is reset, and waited for even when it has
    /// to be killed. It holds the colony in memory, and Python's "restart to
    /// see it" let its next save write the old progress back over the reset.
    ///
    /// Still as Python: a message the channel accepted counts as done (there
    /// is no acknowledgement), and an overlay this window did not start is
    /// not stopped (it cannot see one).
    fn reset_progress(&mut self) {
        let adventure = self.overlay_mode == Some(OverlayMode::Adventure);
        if !adventure && self.channel_connected(100) && self.client.send(&ChannelMessage::ResetProgress.to_map()) {
            self.window.set_status("Saved progress reset in the running overlay.");
            return;
        }
        let outcome = if self.launcher.running() { self.stop() } else { StopOutcome::NotRunning };
        if outcome == StopOutcome::StillRunning {
            // It could still save its old colony over a reset file.
            self.window.set_status(
                "The overlay did not respond and could not be closed, so saved progress was not reset (the                  overlay would write it back). Quit it from its tray icon, then try again.",
            );
            return;
        }
        let path = self.state_dir.join("creatures.json");
        if dc_state::runtime_state::reset_saved_progress(&path) {
            self.window.set_status(match outcome {
                StopOutcome::Graceful => {
                    "The overlay was stopped (it saved first) and its saved progress reset. Launch it again to \
                     start fresh."
                }
                StopOutcome::Terminated => {
                    "The overlay did not respond and was closed, then its saved progress was reset. Cages, webs \
                     and nests placed since its last save were lost."
                }
                StopOutcome::NotRunning | StopOutcome::StillRunning => "Saved progress reset.",
            });
        } else if outcome == StopOutcome::Terminated {
            self.window.set_status(format!(
                "The overlay did not respond and was closed, so cages, webs and nests placed since its last save \
                 were lost; then its saved progress could not be reset: could not write {}.",
                path.display()
            ));
        } else {
            self.window.set_status(format!("Could not write {}.", path.display()));
        }
    }

    /// `play_map`: remember the map for the overlay, then start Adventure.
    fn play_map(&mut self, map_id: &str) {
        let mut profile = self.profile();
        if !campaign::map_unlocked(&profile.missions, map_id, profile.admin, &self.state_dir) {
            return;
        }
        profile.selected_map = map_id.to_string();
        self.save(&profile);
        self.start_adventure();
    }

    /// The character window's `_entry()["name"]`.
    fn rename(profile: &mut AdventureProfile, who: &str, name: &str) -> bool {
        let name = clean_name(Some(&Value::String(name.to_string())));
        let entry = if who == HERO {
            &mut profile.name
        } else if who == PLAYER_TWO {
            &mut profile.player_two.name
        } else {
            match profile.companions.iter_mut().find(|(k, _)| k == who) {
                Some((_, e)) => &mut e.name,
                None => return false,
            }
        };
        if *entry == name {
            return false;
        }
        *entry = name;
        true
    }

    /// The character window's `_unlock`.
    fn unlock(profile: &mut AdventureProfile, who: &str, ability_id: &str) -> bool {
        let mut state = profile_mod::spider_progression(profile, who);
        if !state.can_unlock(ability_id) {
            return false;
        }
        let Some(node) = dc_state::progression::ability_by_id(ability_id) else { return false };
        state.skill_points -= node.cost;
        state.unlocked_abilities.push(ability_id.to_string());
        profile_mod::store_progression(profile, who, state);
        true
    }

    /// The saved maps for the editor's chooser (`_refresh_chooser`).
    fn map_library(&self) -> Vec<(String, String)> {
        custom_maps::list_maps(&self.state_dir).into_iter().map(|m| (m.id, m.title)).collect()
    }

    fn refresh_map_library(&mut self) {
        let library = self.map_library();
        if let Some(Dialog::MapEditor(e)) = self.dialog.as_mut() {
            e.set_library(library);
        }
    }

    fn open_map_editor(&mut self, map_id: Option<&str>) {
        let map = map_id.and_then(|id| custom_maps::load_map(id, &self.state_dir)).map(|m| m.to_value());
        let mut editor = MapEditor::new(map.as_ref(), bridge::progression_catalog());
        editor.set_library(self.map_library());
        self.dialog = Some(Dialog::MapEditor(Box::new(editor)));
    }

    fn save_map(&mut self, raw: &Value) -> Option<String> {
        let data = custom_maps::clean_map(raw);
        match custom_maps::save_map(&data, &self.state_dir) {
            Ok(id) => {
                if let Some(Dialog::MapEditor(e)) = self.dialog.as_mut() {
                    e.saved(&id);
                }
                self.refresh_map_library();
                self.refresh_adventure();
                Some(id)
            }
            Err(e) => {
                self.window.set_status(format!("Could not save the map: {e}"));
                None
            }
        }
    }

    /// Carry out one action.
    pub fn act(&mut self, action: UiAction) {
        let mut profile_changed = false;
        match action {
            UiAction::SavePreset { preset } => self.save_preset(&preset),
            UiAction::LoadPreset { path } => match dc_content::preset_io::load_preset_value(Path::new(&path)) {
                Ok(data) => {
                    self.window.load_preset(&data);
                    self.window.set_status(format!("Loaded preset: {path}"));
                }
                Err(e) => self.window.set_status(format!("Could not load preset: {e}")),
            },
            UiAction::RefreshLibrary => {
                self.window.set_catalog(bridge::settings_catalog(self.root.as_deref()));
                self.window.set_presets(bridge::presets(self.root.as_deref()));
                self.thumbnails_installed = false;
            }
            UiAction::LaunchOverlay { mode: OverlayMode::Adventure, .. } => self.start_adventure(),
            UiAction::LaunchOverlay { mode, preset } => self.launch(mode, &preset),
            UiAction::StopOverlay => {
                self.stop();
            }
            UiAction::ResetProgress => self.reset_progress(),
            UiAction::OpenProjectFolder => {
                let root = self.root.clone().unwrap_or_else(dc_content::discovery::app_root);
                if let Err(e) = std::process::Command::new("explorer").arg(&root).spawn() {
                    self.window.set_status(format!("Could not open folder: {e}"));
                }
            }
            UiAction::ShowMode(_) => {}
            UiAction::LeaveAdventure => {
                if self.overlay_mode == Some(OverlayMode::Adventure) && self.launcher.running() {
                    self.stop();
                }
            }
            UiAction::PlayMap { map_id } => self.play_map(&map_id),
            UiAction::SetTwoPlayer(on) => {
                let mut p = self.profile();
                p.two_player = on;
                self.save(&p);
                profile_changed = true;
            }
            UiAction::SetAllScreens(on) => {
                let mut p = self.profile();
                p.all_screens = on;
                self.save(&p);
                profile_changed = true;
            }
            UiAction::SetScreen { name, on } => {
                let mut p = self.profile();
                p.disabled_screens.retain(|n| *n != name);
                if !on {
                    p.disabled_screens.push(name);
                }
                self.save(&p);
                profile_changed = true;
            }
            UiAction::OpenCharacter => {
                let p = self.profile();
                self.dialog = Some(Dialog::Character(Box::new(CharacterWindow::open(
                    bridge::party_view(&p),
                    bridge::progression_catalog(),
                    None,
                ))));
            }
            UiAction::OpenControls => {
                let c = controls_mod::load_controls(&controls_mod::controls_path(&self.state_dir));
                self.dialog = Some(Dialog::Controls(ControlsDialog::new((&c).into())));
            }
            UiAction::OpenMapEditor { map_id } => self.open_map_editor(map_id.as_deref()),
            UiAction::OpenAdmin => {
                let p = self.profile();
                let level = profile_mod::hero_progression(&p).level.max(1) as u32;
                self.dialog = Some(Dialog::Admin(AdminDialog::new(p.admin, level)));
            }
            UiAction::SaveControls(c) => {
                let settings: dc_adventure::ControlSettings = (&c).into();
                if !controls_mod::save_controls(&settings, &controls_mod::controls_path(&self.state_dir)) {
                    self.window.set_status("Could not save the controls.");
                }
                profile_changed = true;
            }
            UiAction::RenameSpider { who, name } => {
                let mut p = self.profile();
                if Self::rename(&mut p, &who, &name) {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::UnlockSkill { who, ability_id } => {
                let mut p = self.profile();
                if Self::unlock(&mut p, &who, &ability_id) {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::Equip { who, item_id } => {
                let mut p = self.profile();
                if armoury::wear(&mut p, &who, &item_id) {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::Unequip { who, slot } => {
                let mut p = self.profile();
                if armoury::take_off(&mut p, &who, &slot) {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::UpgradeItem { item_id } => {
                let mut p = self.profile();
                if armoury::upgrade(&mut p, &item_id) {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::SellSpares { item_id, count } => {
                let mut p = self.profile();
                if armoury::sell_spares(&mut p, &item_id, count as i64) > 0 {
                    self.save(&p);
                    profile_changed = true;
                }
            }
            UiAction::SetAdmin(on) => {
                admin::set_admin(&self.profile_path(), on);
                profile_changed = true;
            }
            UiAction::ResetAdventureProgress => {
                admin::reset_progress(&self.profile_path());
                profile_changed = true;
            }
            UiAction::GiveAmber(n) => {
                admin::give_amber(&self.profile_path(), n);
                profile_changed = true;
            }
            UiAction::GiveAllArmour => {
                admin::give_all_armour(&self.profile_path());
                profile_changed = true;
            }
            UiAction::UnlockAllCompanions => {
                admin::unlock_all_companions(&self.profile_path());
                profile_changed = true;
            }
            UiAction::SetHeroLevel(level) => {
                admin::set_hero_level(&self.profile_path(), level as i64);
                profile_changed = true;
            }
            UiAction::SaveMap(raw) => {
                self.save_map(&raw);
            }
            // The editor stays open on a new map (`MapEditor.delete`).
            UiAction::DeleteMap(id) => {
                custom_maps::delete_map(&id, &self.state_dir);
                self.refresh_map_library();
                profile_changed = true;
            }
            UiAction::OpenMap(id) => {
                if let (Some(map), Some(Dialog::MapEditor(e))) =
                    (custom_maps::load_map(&id, &self.state_dir), self.dialog.as_mut())
                {
                    e.load(&map.to_value());
                }
            }
            UiAction::SaveAndPlayMap(raw) => {
                if let Some(id) = self.save_map(&raw) {
                    self.dialog = None;
                    self.play_map(&id);
                }
            }
            // Only the dialogs send it: the settings window itself closes
            // through its title bar (`WindowApp::on_close`). A dialog that
            // already closed itself in this batch ("Save and play" sends
            // `SaveAndPlayMap` then `CloseWindow`) leaves nothing to close.
            UiAction::CloseWindow => {
                if self.dialog.take().is_some() {
                    profile_changed = true;
                }
            }
            // Spider and pause actions belong to the overlay's own dialogs.
            other => log::debug!("settings window ignores {other:?}"),
        }
        if profile_changed {
            self.refresh_adventure();
        }
    }

    /// Carry out a frame's actions, as the runner does after drawing it.
    pub fn handle_actions(&mut self, actions: Vec<UiAction>) {
        let mut played = false;
        for action in actions {
            match action {
                UiAction::PlayMap { .. } => played = true,
                // The window adds this beside `PlayMap`; `play_map` already
                // started Adventure, or refused a locked map.
                UiAction::LaunchOverlay { mode: OverlayMode::Adventure, .. } if played => continue,
                _ => {}
            }
            self.act(action);
        }
        if std::mem::take(&mut self.hid_for_start) && !self.hidden_for_adventure {
            self.visibility = Some(true);
        }
    }

    /// Will these actions start Adventure (and so hide the window first)?
    fn starts_adventure(actions: &[UiAction]) -> bool {
        actions.iter().any(|a| {
            matches!(a, UiAction::LaunchOverlay { mode: OverlayMode::Adventure, .. } | UiAction::SaveAndPlayMap(_))
        })
    }

    /// `update_process_status` and the channel, every poll.
    pub fn watch(&mut self) -> PollOutcome {
        let mut outcome = PollOutcome { repaint: false, visible: self.visibility.take() };
        if let Some(code) = self.launcher.take_exit() {
            self.launched_preset = None;
            self.overlay_mode = None;
            self.window.set_overlay_running(false);
            self.window.set_status(format!("Overlay exited with code {code}."));
            outcome.repaint = true;
        }
        if self.hidden_for_adventure && !self.launcher.running() {
            // A mission just ended: show the new level and the result.
            self.hidden_for_adventure = false;
            self.refresh_adventure();
            outcome.visible = Some(true);
            outcome.repaint = true;
        }
        if self.launcher.running() && !self.client.is_connected() {
            self.client.try_connect(50);
        }
        for event in self.client.poll() {
            if let ClientEvent::Message(map) = event
                && let ChannelMessage::SessionState { state } = ChannelMessage::from_map(&map)
            {
                self.window.apply_live_session_state(&state);
                outcome.repaint = true;
            }
        }
        outcome
    }

    /// `closeEvent`: closing the window saves and stops the overlay.
    pub fn on_close(&mut self) {
        // Known gap: an overlay that cannot even be killed (StillRunning)
        // outlives the window, which closes anyway and stops tracking it;
        // keeping the window open needs a veto in dc-ui's `WindowApp`.
        if self.launcher.running() {
            self.stop();
        }
        self.client.close();
    }

    /// The window and any dialog over it.
    pub fn draw(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        if !self.thumbnails_installed {
            self.window.set_thumbnails(thumbnails::model_thumbnail_fn(ctx, self.root.as_deref()));
            self.thumbnails_installed = true;
        }
        let mut actions = self.window.ui(ctx);
        let Some(dialog) = self.dialog.as_mut() else { return actions };
        let (title, size) = match dialog {
            Dialog::Character(_) => ("Character", [980.0, 720.0]),
            Dialog::Controls(_) => ("Controls", [560.0, 640.0]),
            Dialog::MapEditor(_) => ("Map editor", [1100.0, 760.0]),
            Dialog::Admin(_) => ("Admin", [420.0, 420.0]),
        };
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                actions.extend(match dialog {
                    Dialog::Character(c) => match c.as_mut() {
                        CharacterWindow::Single(d) => d.show(ui),
                        CharacterWindow::Party(d) => d.show(ui),
                    },
                    Dialog::Controls(d) => d.show(ui),
                    Dialog::MapEditor(d) => d.show(ui),
                    Dialog::Admin(d) => d.show(ui),
                });
            });
        if !open {
            actions.push(UiAction::CloseWindow);
        }
        actions
    }

    /// [`Self::draw`], hiding the window before an Adventure launch: the
    /// runner applies this frame's viewport commands before it hands the
    /// actions to [`WindowApp::handle`], so the window is gone before the
    /// overlay starts, as `start_adventure` hid it before `launch_engine`.
    pub fn frame(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        let actions = self.draw(ctx);
        if Self::starts_adventure(&actions) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hid_for_start = true;
        }
        actions
    }
}

impl<L: Launcher> WindowApp for SettingsHost<L> {
    fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        self.frame(ctx)
    }

    /// The window closes only through its title bar.
    fn handle(&mut self, actions: Vec<UiAction>) -> bool {
        self.handle_actions(actions);
        true
    }

    fn poll(&mut self) -> PollOutcome {
        self.watch()
    }

    fn on_close(&mut self) {
        SettingsHost::on_close(self);
    }
}

/// `config_ui.main`: the settings window.
pub fn run(state_dir: PathBuf, seconds: Option<f64>) -> Result<(), String> {
    let root = Some(dc_content::discovery::app_root());
    let host = SettingsHost::new(root, state_dir, ProcessLauncher::default());
    let mut options = RunOptions::new(ConfigWindow::title(), [960.0, 860.0]);
    options.min_size = Some([840.0, 560.0]);
    options.exit_after = seconds.map(Duration::from_secs_f64);
    dc_ui::run_window_with(options, host).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
