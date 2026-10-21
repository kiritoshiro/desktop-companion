//! Port of app/admin_ui.py: the owner's switches for testing Adventure.
//!
//! Admin mode (every map open), reset the Adventure progress (asks first),
//! and quick gifts: amber, every armour piece, every companion, a hero
//! level. The profile edits themselves (`reset_progress`, `give_amber`, ...)
//! live in dc-adventure; this window asks for them and shows the answer the
//! app gives back through [`AdminDialog::say`].

use egui::{Ui, vec2};

use crate::UiAction;
use crate::model::MAX_LEVEL;
use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};

pub struct AdminDialog {
    pub admin: bool,
    pub level: u32,
    pub status: String,
    confirm_reset: bool,
}

impl AdminDialog {
    pub fn new(admin: bool, hero_level: u32) -> Self {
        Self { admin, level: hero_level.clamp(1, MAX_LEVEL), status: String::new(), confirm_reset: false }
    }

    pub fn title() -> &'static str {
        "Admin"
    }

    /// `_say`: the result line ("Amber: 1120", "3 new pieces added", ...).
    pub fn say(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                ui.set_min_width(320.0);
                let r = widgets::check(ui, &mut self.admin, "Admin mode: every map is open");
                probe::record(ui.ctx(), "check:admin", r.rect);
                if r.changed() {
                    actions.push(UiAction::SetAdmin(self.admin));
                }
                let full = vec2(ui.available_width(), 30.0);
                if self.confirm_reset {
                    widgets::wrapped(
                        ui,
                        "Start Adventure again? The hero, companions, armour, amber and map results are reset. Your Companion colony is not touched.",
                        theme::font(9.5),
                        CREAM,
                    );
                    ui.horizontal(|ui| {
                        let yes = ui.add(CarvedButton::new("Yes"));
                        probe::record(ui.ctx(), "dialog:Yes", yes.rect);
                        if yes.clicked() {
                            actions.push(UiAction::ResetAdventureProgress);
                            self.level = 1;
                            self.say("Adventure progress reset.");
                            self.confirm_reset = false;
                        }
                        if ui.add(CarvedButton::new("No")).clicked() {
                            self.confirm_reset = false;
                        }
                    });
                } else {
                    let r = ui.add(CarvedButton::new("Reset my Adventure progress\u{2026}").min_size(full));
                    probe::record(ui.ctx(), "admin:reset", r.rect);
                    if r.clicked() {
                        self.confirm_reset = true;
                    }
                }
                for (key, text, action) in [
                    ("admin:amber", "Give 1000 amber", UiAction::GiveAmber(1000)),
                    ("admin:armour", "Give every armour piece", UiAction::GiveAllArmour),
                    ("admin:companions", "Unlock every companion", UiAction::UnlockAllCompanions),
                ] {
                    let r = ui.add(CarvedButton::new(text).min_size(full));
                    probe::record(ui.ctx(), key, r.rect);
                    if r.clicked() {
                        actions.push(action);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.level).range(1..=MAX_LEVEL));
                    let r = ui.add(CarvedButton::new("Set hero level"));
                    probe::record(ui.ctx(), "admin:level", r.rect);
                    if r.clicked() {
                        actions.push(UiAction::SetHeroLevel(self.level));
                    }
                });
                widgets::wrapped(ui, &self.status, theme::font(9.5), CREAM);
                let close = ui.add(CarvedButton::new("Close").min_size(full));
                probe::record(ui.ctx(), "button:Close", close.rect);
                if close.clicked() {
                    actions.push(UiAction::CloseWindow);
                }
            });
        });
        actions
    }
}
