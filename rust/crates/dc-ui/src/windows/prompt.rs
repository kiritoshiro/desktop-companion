//! `QInputDialog.getText(self, "Name this spider", "Spider name:", ...)`
//! from `app/engine.py` `_prompt_name`: one line of text, OK and Cancel,
//! in the dialog wood.

use egui::{Key, Ui, vec2};

use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};

/// What the prompt was answered with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptAnswer {
    Accepted(String),
    Cancelled,
}

/// A one-line text prompt.
pub struct TextPrompt {
    pub title: String,
    pub label: String,
    pub text: String,
    focused_once: bool,
}

impl TextPrompt {
    pub fn new(title: impl Into<String>, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self { title: title.into(), label: label.into(), text: text.into(), focused_once: false }
    }

    /// `_prompt_name`'s dialog for a spider currently called `current`.
    pub fn spider_name(current: &str) -> Self {
        Self::new("Name this spider", "Spider name:", current)
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Option<PromptAnswer> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<PromptAnswer> {
        let mut answer = None;
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            return Some(PromptAnswer::Cancelled);
        }
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 18, top: 14, bottom: 14 }).show(ui, |ui| {
                ui.set_min_width(300.0);
                widgets::brass_title(ui, &self.title, 12.0);
                widgets::text(ui, &self.label, theme::font(9.0), CREAM);
                let edit = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(ui.available_width()));
                probe::record(ui.ctx(), "prompt:text", edit.rect);
                if !self.focused_once {
                    edit.request_focus();
                    self.focused_once = true;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    answer = Some(PromptAnswer::Accepted(self.text.clone()));
                }
                ui.add_space(6.0);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 32.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let cancel = ui.add(CarvedButton::new("Cancel"));
                        probe::record(ui.ctx(), "button:Cancel", cancel.rect);
                        if cancel.clicked() {
                            answer = Some(PromptAnswer::Cancelled);
                        }
                        let ok = ui.add(CarvedButton::new("OK").brass());
                        probe::record(ui.ctx(), "button:OK", ok.rect);
                        if ok.clicked() {
                            answer = Some(PromptAnswer::Accepted(self.text.clone()));
                        }
                    },
                );
            });
        });
        answer
    }
}
