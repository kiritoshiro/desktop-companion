//! A headless egui harness: frames, clicks on recorded widgets, and the
//! accesskit tree of what was drawn.
#![allow(dead_code)]

use dc_ui::UiAction;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

pub struct Harness {
    pub ctx: Context,
    time: f64,
    pending: Vec<Event>,
    size: egui::Vec2,
    pub labels: Vec<String>,
    pub nodes: Vec<(egui::accesskit::Role, String, bool, Option<bool>)>,
}

impl Harness {
    pub fn new(width: f32, height: f32) -> Self {
        let ctx = Context::default();
        dc_ui::theme::apply_app_theme(&ctx);
        dc_ui::probe::enable(&ctx);
        ctx.enable_accesskit();
        Self { ctx, time: 0.0, pending: Vec::new(), size: vec2(width, height), labels: Vec::new(), nodes: Vec::new() }
    }

    /// Run one frame of `ui`, returning its actions.
    pub fn frame(&mut self, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        self.time += 1.0 / 30.0;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events: std::mem::take(&mut self.pending),
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut output = self.ctx.run_pass(input, |ctx| actions = ui(ctx));
        output.textures_delta.clear();
        self.labels.clear();
        self.nodes.clear();
        if let Some(update) = &output.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                let label = node.label().or(node.value()).unwrap_or("").to_owned();
                let toggled = node.toggled().map(|t| t == egui::accesskit::Toggled::True);
                if !label.is_empty() {
                    self.labels.push(label.clone());
                }
                self.nodes.push((node.role(), label, node.is_disabled(), toggled));
            }
        }
        actions
    }

    /// Run a few frames so layout settles.
    pub fn settle(&mut self, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        let mut all = Vec::new();
        for _ in 0..3 {
            all.extend(self.frame(ui));
        }
        all
    }

    pub fn probe(&self, key: &str) -> Rect {
        dc_ui::probe::rect(&self.ctx, key)
            .unwrap_or_else(|| panic!("no widget recorded as {key:?}; recorded: {:?}", dc_ui::probe::keys(&self.ctx)))
    }

    /// Click at `pos` over three frames (move, press, release).
    pub fn click_at(&mut self, pos: Pos2, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        let mut actions = Vec::new();
        self.pending.push(Event::PointerMoved(pos));
        actions.extend(self.frame(ui));
        self.pending.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        actions.extend(self.frame(ui));
        self.pending.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        actions.extend(self.frame(ui));
        // Move away so nothing stays hovered.
        self.pending.push(Event::PointerMoved(pos2(1.0, 1.0)));
        actions.extend(self.frame(ui));
        actions
    }

    /// Click the centre of a recorded widget.
    pub fn click(&mut self, key: &str, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        let rect = self.probe(key);
        self.click_at(rect.center(), ui)
    }

    /// Run one frame with one input event.
    pub fn event_frame(&mut self, event: Event, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        self.pending.push(event);
        self.frame(ui)
    }

    /// Press a key.
    pub fn key(&mut self, key: egui::Key, ui: &mut dyn FnMut(&Context) -> Vec<UiAction>) -> Vec<UiAction> {
        self.pending.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        let mut actions = self.frame(ui);
        self.pending.push(Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        actions.extend(self.frame(ui));
        actions
    }

    pub fn has_label(&self, text: &str) -> bool {
        self.labels.iter().any(|l| l == text)
    }

    pub fn has_label_containing(&self, text: &str) -> bool {
        self.labels.iter().any(|l| l.contains(text))
    }

    pub fn assert_label(&self, text: &str) {
        assert!(self.has_label(text), "no {text:?} among {:#?}", self.labels);
    }

    /// The toggled state of the switch or check box labelled `label`.
    pub fn toggled(&self, label: &str) -> Option<bool> {
        self.nodes.iter().find(|(_, l, _, _)| l == label).and_then(|n| n.3)
    }

    pub fn disabled(&self, label: &str) -> Option<bool> {
        self.nodes.iter().find(|(_, l, _, _)| l == label).map(|n| n.2)
    }
}
