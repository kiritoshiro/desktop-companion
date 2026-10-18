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
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Starts and watches the overlay process. A trait so tests never start one.
pub trait Launcher {
    fn launch(&mut self, preset: &Path, mode: OverlayMode) -> Result<(), String>;
    fn running(&mut self) -> bool;
    /// Ask it to save and quit (the stop-request file), then kill it if needed.
    fn stop(&mut self, state_dir: &Path) -> StopOutcome;
    /// The exit code once it has stopped on its own, reported once.
    fn take_exit(&mut self) -> Option<i32>;
}

/// This executable with `--engine` (the frozen build's command).
#[derive(Default)]
pub struct ProcessLauncher {
    child: Option<std::process::Child>,
}

impl Launcher for ProcessLauncher {
    fn launch(&mut self, preset: &Path, mode: OverlayMode) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut child = std::process::Command::new(exe)
            .arg("--engine")
            .arg("--preset")
            .arg(preset)
            .arg("--mode")
            .arg(mode.as_str())
            .spawn()
            .map_err(|e| e.to_string())?;
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
        self.child = None;
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
    Character(CharacterWindow),
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
}

impl<L: Launcher> SettingsHost<L> {
    pub fn new(root: Option<PathBuf>, state_dir: PathBuf, launcher: L) -> Self {
        let catalog = bridge::settings_catalog(root.as_deref());
        let presets = bridge::presets(root.as_deref());
        let mut window = ConfigWindow::new(catalog);
        let default = presets.iter().find(|p| p.label == "Default").cloned();
        window.set_presets(presets);
        if let Some(data) = default.and_then(|p| dc_content::preset_io::load_preset_value(Path::new(&p.path)).ok()) {
            window.load_preset(&data);
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

    /// The monitors for the raid setup card (`mode_menu.py`).
    fn screens() -> Vec<ScreenInfo> {
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
        let view = bridge::adventure_view(&profile, &self.state_dir, Self::screens(), &controls);
        self.window.shell.refresh_adventure(view);
        if let Some(Dialog::Character(c)) = self.dialog.as_mut() {
            let party = bridge::party_view(&profile);
            match c {
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
        if self.launcher.running() && self.overlay_mode != Some(mode) {
            self.stop();
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
    pub fn stop(&mut self) {
        if !self.launcher.running() {
            self.launched_preset = None;
            session_control::clear_stop_request(&self.state_dir);
            self.window.set_overlay_running(false);
            self.window.set_status("No overlay process is running from this window.");
            return;
        }
        if self.channel_connected(100) {
            self.client.send(&ChannelMessage::StopRequest.to_map());
        }
        let outcome = self.launcher.stop(&self.state_dir);
        self.launched_preset = None;
        self.overlay_mode = None;
        self.window.set_overlay_running(false);
        self.window.set_status(match outcome {
            StopOutcome::Graceful => "Overlay stopped and saved its spiders.",
            StopOutcome::Terminated => {
                "Overlay did not respond and was closed; recent progress may not have been saved."
            }
            StopOutcome::NotRunning => "No overlay process is running from this window.",
        });
    }

    /// `reset_saved_progress` (already confirmed in the window).
    fn reset_progress(&mut self) {
        if self.channel_connected(100) {
            self.client.send(&ChannelMessage::ResetProgress.to_map());
            self.window.set_status("Saved progress reset in the running overlay.");
            return;
        }
        let path = self.state_dir.join("creatures.json");
        if dc_state::runtime_state::reset_saved_progress(&path) {
            self.window.set_status(if self.launcher.running() {
                "Saved progress reset. Restart the overlay to see it."
            } else {
                "Saved progress reset."
            });
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

    fn open_map_editor(&mut self, map_id: Option<&str>) {
        let map = map_id.and_then(|id| custom_maps::load_map(id, &self.state_dir)).map(|m| m.to_value());
        self.dialog = Some(Dialog::MapEditor(Box::new(MapEditor::new(map.as_ref(), bridge::progression_catalog()))));
    }

    fn save_map(&mut self, raw: &Value) -> Option<String> {
        let data = custom_maps::clean_map(raw);
        match custom_maps::save_map(&data, &self.state_dir) {
            Ok(id) => {
                if let Some(Dialog::MapEditor(e)) = self.dialog.as_mut() {
                    e.saved(&id);
                }
                self.refresh_adventure();
                Some(id)
            }
            Err(e) => {
                self.window.set_status(format!("Could not save the map: {e}"));
                None
            }
        }
    }

    /// Carry out one action. Returns false to close the window.
    pub fn act(&mut self, action: UiAction) -> bool {
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
            }
            UiAction::LaunchOverlay { mode: OverlayMode::Adventure, .. } => self.start_adventure(),
            UiAction::LaunchOverlay { mode, preset } => self.launch(mode, &preset),
            UiAction::StopOverlay => self.stop(),
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
                self.dialog = Some(Dialog::Character(CharacterWindow::open(
                    bridge::party_view(&p),
                    bridge::progression_catalog(),
                    None,
                )));
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
            UiAction::DeleteMap(id) => {
                custom_maps::delete_map(&id, &self.state_dir);
                self.dialog = None;
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
            UiAction::CloseWindow => {
                if self.dialog.take().is_some() {
                    profile_changed = true;
                } else {
                    self.on_close();
                    return false;
                }
            }
            // Spider and pause actions belong to the overlay's own dialogs.
            other => log::debug!("settings window ignores {other:?}"),
        }
        if profile_changed {
            self.refresh_adventure();
        }
        true
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
        if self.launcher.running() {
            self.stop();
        }
        self.client.close();
    }

    /// The window and any dialog over it.
    pub fn draw(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
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
                    Dialog::Character(CharacterWindow::Single(d)) => d.show(ui),
                    Dialog::Character(CharacterWindow::Party(d)) => d.show(ui),
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
}

impl<L: Launcher> WindowApp for SettingsHost<L> {
    fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        self.draw(ctx)
    }

    fn handle(&mut self, actions: Vec<UiAction>) -> bool {
        for action in actions {
            if !self.act(action) {
                return false;
            }
        }
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
