//! Port of the dialogs in app/adventure_ui.py: the pause menu and the
//! Adventure settings (frame rate plus the controls editor, its bindings
//! locked in two-player mode). The HUD painter (`draw_hud`) belongs to
//! dc-paint; [`short_binding`] and [`hud_rect`] are its pure helpers.

use egui::{Rect, Ui, pos2, vec2};

use crate::UiAction;
use crate::model::ControlSettings;
use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};
use crate::windows::controls::ControlsEditor;

pub const HUD_WIDTH: f32 = 430.0;
pub const HUD_HEIGHT: f32 = 210.0;

/// `short_binding`: a binding as it fits on an ability slot.
pub fn short_binding(name: &str) -> String {
    match name {
        "Mouse Left" => "L CLICK".into(),
        "Mouse Right" => "R CLICK".into(),
        "Mouse Middle" => "M CLICK".into(),
        "Mouse Back" => "M4".into(),
        "Mouse Forward" => "M5".into(),
        other => other.to_uppercase(),
    }
}

/// `hud_rect`: where a player's status panel sits. `home` is the main
/// monitor inside the overlay, `window` the whole overlay, `position` a
/// dragged position, `players` how many play.
pub fn hud_rect(window: Rect, home: Rect, position: Option<egui::Pos2>, players: usize, player_index: usize) -> Rect {
    let area = if home.width() >= 200.0 && home.height() >= 200.0 { home.intersect(window) } else { window };
    if players > 1 {
        let width = HUD_WIDTH.min(((area.width() - 32.0) / 2.0).floor().max(200.0));
        return Rect::from_min_size(
            pos2(area.left() + 10.0 + player_index as f32 * (width + 12.0), area.bottom() - HUD_HEIGHT - 14.0),
            vec2(width, HUD_HEIGHT),
        );
    }
    let width = HUD_WIDTH.min((window.width() - 16.0).max(1.0));
    let height = HUD_HEIGHT.min((window.height() - 16.0).max(1.0));
    match position {
        None => Rect::from_min_size(
            pos2(
                (area.left() + ((area.width() - width) / 2.0).floor()).max(8.0),
                (area.bottom() - height - 14.0).max(8.0),
            ),
            vec2(width, height),
        ),
        Some(p) => Rect::from_min_size(
            pos2(p.x.min(window.width() - width - 8.0).max(8.0), p.y.min(window.height() - height - 8.0).max(8.0)),
            vec2(width, height),
        ),
    }
}

/// `PauseDialog`.
pub struct PauseDialog {
    /// In a raid the menu offers restart and exit instead of release.
    pub in_mission: bool,
    pub choice: Option<String>,
}

impl PauseDialog {
    pub fn new(in_mission: bool) -> Self {
        Self { in_mission, choice: None }
    }

    pub fn title() -> &'static str {
        "Adventure paused"
    }

    pub fn choices(&self) -> &'static [(&'static str, &'static str)] {
        if self.in_mission {
            &[
                ("Resume", "resume"),
                ("Character & armour", "skills"),
                ("Settings", "settings"),
                ("Restart raid", "restart"),
                ("Retry saving victory", "save"),
                ("Exit raid", "exit"),
            ]
        } else {
            &[
                ("Resume", "resume"),
                ("Character & armour", "skills"),
                ("Save", "save"),
                ("Settings", "settings"),
                ("Release spider", "release"),
                ("Save and exit", "exit"),
            ]
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 20 }).show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.vertical_centered(|ui| widgets::brass_title(ui, "Adventure paused", 16.0));
                ui.add_space(4.0);
                for (label, choice) in self.choices() {
                    let r = ui.add(CarvedButton::new(*label).min_size(vec2(ui.available_width(), 30.0)));
                    probe::record(ui.ctx(), format!("pause:{choice}"), r.rect);
                    if r.clicked() {
                        self.choice = Some((*choice).to_owned());
                        actions.push(UiAction::PauseChoice((*choice).to_owned()));
                        actions.push(UiAction::CloseWindow);
                    }
                }
            });
        });
        actions
    }
}

/// `AdventureSettingsDialog`.
pub struct AdventureSettingsDialog {
    pub fps: u32,
    pub controls: ControlsEditor,
    pub two_players: bool,
}

pub const FPS_CHOICES: [u32; 3] = [30, 45, 60];

impl AdventureSettingsDialog {
    /// `target_fps`: the overlay's frame policy; `players`: how many play.
    pub fn new(target_fps: u32, controls: ControlSettings, players: usize) -> Self {
        let fps = *FPS_CHOICES.iter().min_by_key(|v| (**v as i64 - target_fps as i64).abs()).unwrap_or(&60);
        let mut editor = ControlsEditor::new(controls);
        let two_players = players > 1;
        // Fixed keys in two-player; movement and aim still apply to both.
        editor.lock_bindings(two_players);
        Self { fps, controls: editor, two_players }
    }

    pub fn title() -> &'static str {
        "Adventure settings"
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 20 }).show(ui, |ui| {
                ui.set_min_width(640.0);
                widgets::brass_title(ui, "Adventure settings", 16.0);
                ui.label("Frame rate");
                let options: Vec<(String, u32)> = FPS_CHOICES.iter().map(|v| (format!("{v} FPS"), *v)).collect();
                if widgets::combo(ui, "fps", 120.0, &mut self.fps, &options).changed() {
                    actions.push(UiAction::SetFrameRate(self.fps));
                }
                widgets::brass_title(ui, "Controls", 16.0);
                if self.two_players {
                    widgets::wrapped(
                        ui,
                        "Two-player mode uses fixed keys so the players never share one. Both walk the way chosen under Movement (turn and walk by default).\n\
                         P1: W forward, S back, A/D turn \u{b7} F bite \u{b7} Q web \u{b7} Space jump \u{b7} Shift sprint \u{b7} K gear\n\
                         P2: \u{2191} forward, \u{2193} back, \u{2190}/\u{2192} turn \u{b7} J bite \u{b7} U web \u{b7} I jump \u{b7} O sprint \u{b7} P gear\n\
                         Esc pauses both \u{b7} ` shows the map \u{b7} F8 releases the controls",
                        theme::font(9.0),
                        CREAM_SOFT,
                    );
                }
                actions.extend(self.controls.show(ui));
                let note = if self.two_players { "Two player status panels sit side by side." } else { "Drag the status panel to move it." };
                widgets::text(ui, note, theme::font(9.0), CREAM_SOFT);
                let done = ui.add(CarvedButton::new("Apply and return").min_size(vec2(ui.available_width(), 30.0)));
                probe::record(ui.ctx(), "button:Apply and return", done.rect);
                if done.clicked() {
                    actions.push(UiAction::SetFrameRate(self.fps));
                    actions.push(UiAction::CloseWindow);
                }
            });
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_shorten_for_the_ability_slots() {
        assert_eq!(short_binding("Mouse Left"), "L CLICK");
        assert_eq!(short_binding("Space"), "SPACE");
    }

    #[test]
    fn two_status_panels_sit_side_by_side() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(3840.0, 1080.0));
        let home = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));
        let one = hud_rect(window, home, None, 2, 0);
        let two = hud_rect(window, home, None, 2, 1);
        assert!(one.right() < two.left());
        assert!(two.right() <= 1920.0);
        let solo = hud_rect(window, home, None, 1, 0);
        assert!((solo.center().x - 960.0).abs() < 1.0);
        let moved = hud_rect(window, home, Some(pos2(5000.0, 5.0)), 1, 0);
        assert_eq!(moved.right(), 3840.0 - 8.0);
        assert_eq!(moved.top(), 8.0);
    }

    #[test]
    fn the_frame_rate_snaps_to_a_choice() {
        assert_eq!(AdventureSettingsDialog::new(50, ControlSettings::default(), 1).fps, 45);
        assert!(AdventureSettingsDialog::new(60, ControlSettings::default(), 2).controls.locked);
    }
}
