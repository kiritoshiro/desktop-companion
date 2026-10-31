//! Port of app/controls_ui.py: the Adventure controls editor.
//!
//! Aim cone, the facing option, and one rebindable row per action. Click a
//! binding button, then press a key or a mouse button; Esc cancels and a key
//! already in use swaps places. Every change yields
//! [`UiAction::SaveControls`] at once, as the Qt editor saved `controls.json`
//! at once. In two-player mode the key buttons and Reset are locked, but the
//! list still scrolls and movement and aim still apply.

use egui::{Rect, RichText, Ui, vec2};

use crate::UiAction;
use crate::model::controls::{ACTIONS, AIM_CONES, ControlSettings, MOVEMENT_MODES, cone_label, key_name, mouse_name};
use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};

/// `ControlsEditor`.
pub struct ControlsEditor {
    pub settings: ControlSettings,
    /// The action whose button is waiting for a key.
    pub listening: Option<String>,
    /// `lock_bindings`: two-player mode keeps fixed keys.
    pub locked: bool,
    /// The frame listening started, so the click that started it is not
    /// taken as the new binding.
    listen_frame: u64,
    /// A mouse binding was just taken on this button: its own click is not a new listen.
    suppress_click: Option<String>,
    button_rects: Vec<(String, Rect)>,
}

impl ControlsEditor {
    pub fn new(settings: ControlSettings) -> Self {
        Self {
            settings,
            listening: None,
            locked: false,
            listen_frame: 0,
            suppress_click: None,
            button_rects: Vec::new(),
        }
    }

    pub fn lock_bindings(&mut self, locked: bool) {
        self.locked = locked;
        if locked {
            self.listening = None;
        }
    }

    /// `_rebind` and save.
    pub fn rebind(&mut self, action: &str, name: &str, actions: &mut Vec<UiAction>) {
        self.settings.rebind(action, name);
        self.listening = None;
        actions.push(UiAction::SaveControls(self.settings.clone()));
    }

    /// `reset` and save.
    pub fn reset(&mut self, actions: &mut Vec<UiAction>) {
        self.settings.reset();
        self.listening = None;
        actions.push(UiAction::SaveControls(self.settings.clone()));
    }

    /// Listen for the next key or mouse button.
    fn capture(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        let Some(action) = self.listening.clone() else { return };
        let frame = ui.ctx().cumulative_frame_nr();
        if frame <= self.listen_frame {
            return;
        }
        let own = self.button_rects.iter().find(|(a, _)| *a == action).map(|(_, r)| *r);
        let events = ui.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => {
                    if key == egui::Key::Escape {
                        self.listening = None;
                    } else {
                        self.rebind(&action, &key_name(key), actions);
                    }
                    return;
                }
                egui::Event::Key { .. } => {}
                egui::Event::PointerButton { pos, button, pressed: true, .. } => {
                    if own.is_some_and(|r| r.contains(pos)) {
                        self.suppress_click = Some(action.clone());
                        self.rebind(&action, mouse_name(button), actions);
                    } else {
                        // A press elsewhere takes the focus away: cancel.
                        self.listening = None;
                    }
                    return;
                }
                _ => {}
            }
        }
        // Modifiers arrive as modifier state, not key events.
        let modifiers = ui.input(|i| i.modifiers);
        for (on, name) in [(modifiers.shift, "Shift"), (modifiers.ctrl, "Ctrl"), (modifiers.alt, "Alt")] {
            if on {
                self.rebind(&action, name, actions);
                return;
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        self.capture(ui, &mut actions);
        ui.horizontal(|ui| {
            ui.label("Movement:");
            let options: Vec<(String, String)> = MOVEMENT_MODES.iter().map(|(m, l)| ((*l).to_owned(), (*m).to_owned())).collect();
            let r = widgets::combo(ui, "movement", 170.0, &mut self.settings.movement, &options).on_hover_text(
                "Screen directions: W/A/S/D walk up, left, down and right on the screen.\nTurn and walk: W walks forward, S backs up slowly while facing forward, A/D turn.",
            );
            probe::record(ui.ctx(), "controls:movement", r.rect);
            if r.changed() {
                actions.push(UiAction::SaveControls(self.settings.clone()));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Aim cone:");
            let mut cone = if AIM_CONES.contains(&self.settings.aim_cone) { self.settings.aim_cone } else { 90 };
            let options: Vec<(String, u32)> = AIM_CONES.iter().map(|d| (cone_label(*d).to_owned(), *d)).collect();
            let r = widgets::combo(ui, "aim-cone", 170.0, &mut cone, &options)
                .on_hover_text("How far from where your spider is looking you can aim. The mouse only aims; it never moves the spider.");
            if r.changed() {
                self.settings.aim_cone = cone;
                actions.push(UiAction::SaveControls(self.settings.clone()));
            }
        });
        // Facing the mouse would fight A/D turning, so it is a screen-mode option.
        let enabled = !self.settings.turn_movement();
        let r = widgets::check_enabled(
            ui,
            &mut self.settings.face_mouse_when_still,
            "Turn to face the mouse when standing still",
            enabled,
        )
        .on_hover_text(
            "Off: the spider keeps facing the way it last walked. On: standing still, it turns towards the pointer.",
        );
        probe::record(ui.ctx(), "controls:face-mouse", r.rect);
        if r.changed() {
            actions.push(UiAction::SaveControls(self.settings.clone()));
        }
        widgets::wrapped(
            ui,
            "The mouse aims inside the cone in front of your spider; walk to turn. Click a button below, then press the new key or mouse button. A button already in use swaps places. Esc always pauses.",
            theme::font(9.0),
            CREAM_SOFT,
        );
        self.button_rects.clear();
        theme::bindings_frame().show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("bindings-scroll").min_scrolled_height(240.0).max_height(420.0).show(
                ui,
                |ui| {
                    egui::Grid::new("bindings").spacing(vec2(12.0, 6.0)).num_columns(3).show(ui, |ui| {
                        for (action, _, _) in ACTIONS {
                            let (label, text) = self.settings.action_text(action);
                            ui.label(RichText::new(&label).font(theme::bold(9.0)).color(CREAM));
                            let listening = self.listening.as_deref() == Some(*action);
                            let caption = if listening {
                                "Press a key or click\u{2026}".to_owned()
                            } else {
                                self.settings.binding(action)
                            };
                            let r = ui.add(
                                CarvedButton::new(caption)
                                    .font(theme::mono_bold(9.0))
                                    .min_size(vec2(130.0, 28.0))
                                    .enabled(!self.locked)
                                    .accessible(format!("{label}: {}", self.settings.binding(action)))
                                    .tooltip("Click, then press a key or mouse button. Esc cancels."),
                            );
                            probe::record(ui.ctx(), format!("binding:{action}"), r.rect);
                            self.button_rects.push(((*action).to_owned(), r.rect));
                            if r.clicked() && self.suppress_click.take().as_deref() == Some(*action) {
                                // The release of the click that was just bound.
                            } else if r.clicked() && !listening {
                                self.listening = Some((*action).to_owned());
                                self.listen_frame = ui.ctx().cumulative_frame_nr();
                            }
                            ui.add(
                                egui::Label::new(RichText::new(text).font(theme::font(9.0)).color(CREAM_SOFT)).wrap(),
                            );
                            ui.end_row();
                        }
                    });
                },
            );
        });
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 34.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let r = ui.add(CarvedButton::new("Reset to defaults").enabled(!self.locked));
                probe::record(ui.ctx(), "button:Reset to defaults", r.rect);
                if r.clicked() {
                    self.reset(&mut actions);
                }
            },
        );
        actions
    }
}

/// `ControlsDialog`: the editor on its own walnut board.
pub struct ControlsDialog {
    pub editor: ControlsEditor,
    pub open: bool,
}

impl ControlsDialog {
    pub fn new(settings: ControlSettings) -> Self {
        Self { editor: ControlsEditor::new(settings), open: true }
    }

    pub fn title() -> &'static str {
        "Adventure controls"
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        let rect = ui.max_rect();
        widgets::wood_board(ui, rect, true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 20 }).show(ui, |ui| {
                ui.set_min_width(640.0);
                widgets::brass_title(ui, "Adventure controls", 16.0);
                actions.extend(self.editor.show(ui));
                let done = ui.add(CarvedButton::new("Done").min_size(vec2(ui.available_width(), 30.0)));
                probe::record(ui.ctx(), "button:Done", done.rect);
                if done.clicked() {
                    self.open = false;
                    actions.push(UiAction::CloseWindow);
                }
            });
        });
        actions
    }
}
