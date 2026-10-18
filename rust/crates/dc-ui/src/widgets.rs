//! The painted widgets of the wood theme: carved buttons (`wood_button`,
//! `brass_button`), check boxes and pill switches, segment controls, key caps,
//! the inspector's `VitalBar` and `StatTile`, the Companion section headers,
//! level badges, team chips, XP bars and wood tab bars.
//!
//! Every widget reports itself to accesskit with its label, so the windows
//! "stay reachable without sight" as the Qt ones did through accessible names.

use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2, Widget,
    WidgetInfo, accesskit::Role, pos2, vec2,
};

use crate::assets::UiAssets;
use crate::paint;
use crate::theme::{self, *};

/// The style sheet a button follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonLook {
    /// `wood_button`: oak gradient, light text.
    Wood,
    /// `brass_button` (`#primaryButton`, `#modeChoice`): brass, dark text.
    Brass,
    /// `#stopButton`: wood that turns red on hover.
    Stop,
    /// A segment of a segment control (`#segment`); `checked` is brass.
    Segment { left: bool, right: bool, checked: bool },
    /// A checkable pill (`#whoButton`): brass when checked.
    Toggle { checked: bool },
    /// A skill-tree node (`#skillNode`).
    SkillNode(NodeLook),
}

/// The three looks of a skill node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeLook {
    Unlocked,
    Available,
    Locked,
}

/// A carved button. Build it, then `ui.add(...)`.
pub struct CarvedButton {
    text: String,
    look: ButtonLook,
    icon: Option<TextureHandle>,
    icon_size: f32,
    font: FontId,
    padding: Vec2,
    min_size: Vec2,
    enabled: bool,
    tooltip: Option<String>,
    accessible: Option<String>,
    align_left: bool,
}

impl CarvedButton {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            look: ButtonLook::Wood,
            icon: None,
            icon_size: 22.0,
            font: theme::bold(9.0),
            padding: vec2(14.0, 6.0),
            min_size: vec2(0.0, 30.0),
            enabled: true,
            tooltip: None,
            accessible: None,
            align_left: false,
        }
    }
    pub fn look(mut self, look: ButtonLook) -> Self {
        self.look = look;
        self
    }
    pub fn brass(self) -> Self {
        self.look(ButtonLook::Brass)
    }
    pub fn icon(mut self, icon: Option<TextureHandle>, size: f32) -> Self {
        self.icon = icon;
        self.icon_size = size;
        self
    }
    pub fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }
    pub fn padding(mut self, padding: Vec2) -> Self {
        self.padding = padding;
        self
    }
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    /// The accessible name when the text alone does not say it (icon buttons).
    pub fn accessible(mut self, name: impl Into<String>) -> Self {
        self.accessible = Some(name.into());
        self
    }
    pub fn align_left(mut self) -> Self {
        self.align_left = true;
        self
    }
}

struct ButtonColors {
    top: Color32,
    bottom: Color32,
    text: Color32,
    border: Color32,
    border_px: f32,
    radius: f32,
    dashed: bool,
}

fn button_colors(look: ButtonLook, hovered: bool, pressed: bool, enabled: bool) -> ButtonColors {
    let (top, bottom, text, border, radius) = match look {
        ButtonLook::Wood | ButtonLook::Stop => (OAK_LIGHT, OAK_BOTTOM, BUTTON_TEXT, WALNUT_DEEP, 7.0),
        ButtonLook::Brass => (BRASS_TOP, BRASS_DEEP, WALNUT_DEEP, BRASS_BORDER, 7.0),
        ButtonLook::Segment { checked: true, .. } => (BRASS_TOP, BRASS_DEEP, WALNUT_DEEP, BRASS_DEEP, 8.0),
        ButtonLook::Segment { checked: false, .. } => {
            let fill = if hovered { rgba(90, 58, 32, 220) } else { rgba(20, 12, 6, 200) };
            (fill, fill, CREAM, BRASS_DEEP, 8.0)
        }
        ButtonLook::Toggle { checked: true } => (BRASS, BRASS, WALNUT_DEEP, WALNUT_DEEP, 7.0),
        ButtonLook::Toggle { checked: false } => (OAK_LIGHT, OAK_BOTTOM, BUTTON_TEXT, WALNUT_DEEP, 7.0),
        ButtonLook::SkillNode(NodeLook::Unlocked) => (BRASS, BRASS_DEEP, WALNUT_DEEP, WALNUT_DEEP, 12.0),
        ButtonLook::SkillNode(NodeLook::Available) => {
            let fill = if hovered { rgb(0x5a, 0x38, 0x20) } else { WALNUT };
            (fill, fill, CREAM, BRASS, 12.0)
        }
        ButtonLook::SkillNode(NodeLook::Locked) => {
            let fill = rgba(34, 20, 10, 215);
            (fill, fill, rgb(0xb8, 0xa0, 0x80), rgb(0x7a, 0x64, 0x48), 12.0)
        }
    };
    if !enabled {
        return ButtonColors {
            top: DISABLED_BG,
            bottom: DISABLED_BG,
            text: if matches!(look, ButtonLook::SkillNode(_)) { text } else { DISABLED_TEXT },
            border,
            border_px: 1.0,
            radius,
            dashed: matches!(look, ButtonLook::SkillNode(NodeLook::Locked)),
        };
    }
    let carved = matches!(look, ButtonLook::Wood | ButtonLook::Brass | ButtonLook::Stop | ButtonLook::Toggle { .. });
    let (mut top, mut bottom) = (top, bottom);
    let mut text = text;
    if carved && hovered && !pressed {
        if look == ButtonLook::Stop {
            top = STOP_HOVER;
            bottom = STOP_HOVER;
            text = STOP_HOVER_TEXT;
        } else {
            top = lift22(top);
            bottom = lift22(bottom);
        }
    }
    if carved && pressed {
        std::mem::swap(&mut top, &mut bottom);
    }
    let border_px = if matches!(look, ButtonLook::SkillNode(_)) { 2.0 } else { 1.0 };
    ButtonColors {
        top,
        bottom,
        text,
        border,
        border_px,
        radius,
        dashed: matches!(look, ButtonLook::SkillNode(NodeLook::Locked)),
    }
}

impl Widget for CarvedButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let wrap =
            if matches!(self.look, ButtonLook::SkillNode(_)) { self.min_size.x.max(40.0) } else { f32::INFINITY };
        let galley = ui.painter().layout(self.text.clone(), self.font.clone(), Color32::WHITE, wrap);
        let icon_w = if self.icon.is_some() { self.icon_size + 6.0 } else { 0.0 };
        let content =
            vec2(galley.size().x + icon_w, galley.size().y.max(if self.icon.is_some() { self.icon_size } else { 0.0 }));
        let size = (content + self.padding * 2.0 + vec2(0.0, 2.0)).max(self.min_size);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);
        let label = self.accessible.clone().unwrap_or_else(|| self.text.replace('\n', " "));
        let enabled = self.enabled;
        let selected = match self.look {
            ButtonLook::Segment { checked, .. } | ButtonLook::Toggle { checked } => Some(checked),
            _ => None,
        };
        response.widget_info(|| match selected {
            Some(on) => WidgetInfo::selected(Role::Button, enabled, on, &label),
            None => WidgetInfo::labeled(Role::Button, enabled, &label),
        });
        if ui.is_rect_visible(rect) {
            let hovered = response.hovered() && enabled;
            let pressed = response.is_pointer_button_down_on() && enabled;
            let colors = button_colors(self.look, hovered, pressed, enabled);
            let painter = ui.painter();
            let carved = matches!(
                self.look,
                ButtonLook::Wood | ButtonLook::Brass | ButtonLook::Stop | ButtonLook::Toggle { .. }
            );
            let r = colors.radius;
            let radius = match self.look {
                ButtonLook::Segment { left, right, .. } => CornerRadius {
                    nw: if left { r as u8 } else { 0 },
                    sw: if left { r as u8 } else { 0 },
                    ne: if right { r as u8 } else { 0 },
                    se: if right { r as u8 } else { 0 },
                },
                _ => CornerRadius::same(r as u8),
            };
            // `border-bottom: 3px` that collapses to 1px while pressed.
            let bottom_lip = if carved && enabled { if pressed { 1.0 } else { 3.0 } } else { colors.border_px };
            if colors.dashed {
                painter.rect_filled(rect, radius, colors.top);
                paint::dashed_rounded_rect(painter, rect.shrink(1.0), r, Stroke::new(2.0, colors.border));
            } else if matches!(self.look, ButtonLook::Segment { .. }) {
                painter.rect(rect, radius, colors.top, Stroke::new(1.0, colors.border), StrokeKind::Inside);
                if colors.top != colors.bottom {
                    paint::vertical_gradient(painter, rect.shrink(1.0), 0.0, colors.top, colors.bottom);
                }
            } else {
                painter.rect_filled(rect, radius, colors.border);
                let inner = Rect::from_min_max(
                    rect.min + vec2(colors.border_px, colors.border_px),
                    rect.max - vec2(colors.border_px, bottom_lip),
                );
                paint::vertical_gradient(painter, inner, (r - 1.0).max(0.0), colors.top, colors.bottom);
            }
            let shift = if carved && pressed { 2.0 } else { 0.0 };
            let mut x = if self.align_left { rect.left() + self.padding.x } else { rect.center().x - content.x / 2.0 };
            let cy = rect.center().y - if carved { 1.0 } else { 0.0 } + shift;
            if let Some(icon) = &self.icon {
                let icon_rect = Rect::from_center_size(pos2(x + self.icon_size / 2.0, cy), Vec2::splat(self.icon_size));
                let tint = if enabled { Color32::WHITE } else { rgba(255, 255, 255, 120) };
                painter.image(icon.id(), icon_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
                x += icon_w;
            }
            let text_pos = pos2(x, cy - galley.size().y / 2.0);
            painter.galley_with_override_text_color(text_pos, galley, colors.text);
        }
        if let Some(tip) = &self.tooltip {
            response = tooltip(response, tip);
        }
        response
    }
}

/// A wood button with the given text.
pub fn wood_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(CarvedButton::new(text))
}

/// A brass button with the given text.
pub fn brass_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(CarvedButton::new(text).brass())
}

/// A tooltip in the theme's look (`QToolTip`: parchment, ink).
pub fn tooltip(response: Response, text: &str) -> Response {
    let text = text.to_owned();
    response.on_hover_ui(move |ui| {
        ui.set_max_width(420.0);
        ui.label(egui::RichText::new(&text).color(TOOLTIP_TEXT).font(theme::font(9.0)));
    })
}

/// Tooltip lines with their own colours (stat_text's coloured HTML).
pub fn rich_tooltip(response: Response, lines: Vec<(String, Color32, bool)>) -> Response {
    response.on_hover_ui(move |ui| {
        ui.set_max_width(420.0);
        for (text, color, strong) in &lines {
            let font = if *strong { theme::bold(9.5) } else { theme::font(9.0) };
            ui.label(egui::RichText::new(text).color(*color).font(font));
        }
    })
}

/// The text colour of plain labels in the current look.
pub fn label_color(ui: &Ui) -> Color32 {
    ui.visuals().widgets.noninteractive.fg_stroke.color
}

/// A label in the given font and colour.
pub fn text(ui: &mut Ui, text: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.label(egui::RichText::new(text.into()).font(font).color(color))
}

/// A wrapped label in the given font and colour.
pub fn wrapped(ui: &mut Ui, text: impl Into<String>, font: FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).font(font).color(color)).wrap())
}

// -- check boxes and switches ------------------------------------------------

/// A check box whose box always shows, ticked or not (map_editor's
/// `CHECKBOXES`), its label in the current look's text colour.
pub fn check(ui: &mut Ui, value: &mut bool, label: impl Into<String>) -> Response {
    check_enabled(ui, value, label, true)
}

/// [`check`], greyed out when `enabled` is false.
pub fn check_enabled(ui: &mut Ui, value: &mut bool, label: impl Into<String>, enabled: bool) -> Response {
    let label = label.into();
    let color = if enabled { label_color(ui) } else { theme::with_alpha(label_color(ui), 140) };
    let galley = ui.painter().layout(label.clone(), theme::font(9.0), color, ui.available_width().max(60.0) - 22.0);
    let size = vec2(22.0 + galley.size().x, galley.size().y.max(18.0));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, mut response) = ui.allocate_exact_size(size, sense);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let checked = *value;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, checked, &label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let b = Rect::from_center_size(pos2(rect.left() + 8.0, rect.center().y), Vec2::splat(14.0));
        let face = if !enabled {
            rgb(0x8a, 0x7a, 0x64)
        } else if checked {
            rgb(0x9a, 0x6a, 0x34)
        } else {
            rgb(0xf5, 0xe6, 0xc8)
        };
        painter.rect(b, CornerRadius::same(3), face, Stroke::new(1.0, rgb(0xc9, 0xa3, 0x6a)), StrokeKind::Inside);
        if checked {
            paint::polyline(
                painter,
                vec![
                    b.left_center() + vec2(3.0, 0.0),
                    b.center_bottom() + vec2(-1.0, -3.5),
                    b.right_top() + vec2(-2.5, 3.0),
                ],
                Stroke::new(2.0, CREAM),
            );
        }
        if response.hovered() && enabled {
            painter.rect_stroke(b.expand(1.0), CornerRadius::same(4), Stroke::new(1.0, BRASS), StrokeKind::Outside);
        }
        painter.galley(pos2(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    response
}

/// The raid setup's pill switch (`QCheckBox::indicator` 34x18, brass when on).
pub fn pill_switch(ui: &mut Ui, value: &mut bool, label: impl Into<String>, enabled: bool) -> Response {
    let label = label.into();
    let color = if enabled { CREAM } else { CREAM_SOFT };
    let galley = ui.painter().layout(label.clone(), theme::font(9.0), color, (ui.available_width() - 44.0).max(80.0));
    let size = vec2(44.0 + galley.size().x, galley.size().y.max(20.0));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, mut response) = ui.allocate_exact_size(size, sense);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let on = *value;
    response.widget_info(|| WidgetInfo::selected(Role::Switch, enabled, on, &label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let pill = Rect::from_min_size(pos2(rect.left(), rect.center().y - 9.0), vec2(34.0, 18.0));
        if on {
            paint::horizontal_gradient(painter, pill, 9.0, BRASS_DEEP, rgb(0xf0, 0xcf, 0x7a));
            painter.rect_stroke(pill, CornerRadius::same(9), Stroke::new(1.0, BRASS), StrokeKind::Inside);
        } else {
            painter.rect(
                pill,
                CornerRadius::same(9),
                rgba(20, 12, 6, 220),
                Stroke::new(1.0, BRASS_DEEP),
                StrokeKind::Inside,
            );
        }
        let knob_x = if on { pill.right() - 9.0 } else { pill.left() + 9.0 };
        let knob = if enabled { CREAM } else { CREAM_SOFT };
        painter.circle(pos2(knob_x, pill.center().y), 6.5, knob, Stroke::new(1.0, WALNUT_DEEP));
        painter.galley(pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    response
}

/// A segment control (`QPushButton#segment` pair); returns the new index when
/// one is clicked.
pub fn segments(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, label) in labels.iter().enumerate() {
            let look =
                ButtonLook::Segment { left: index == 0, right: index + 1 == labels.len(), checked: index == selected };
            let response =
                ui.add(CarvedButton::new(*label).look(look).padding(vec2(16.0, 6.0)).min_size(vec2(0.0, 30.0)));
            crate::probe::record(ui.ctx(), format!("segment:{label}"), response.rect);
            if response.clicked() {
                chosen = Some(index);
            }
        }
    });
    chosen
}

// -- keys ------------------------------------------------------------------

/// A key as a small key cap (`QLabel#keyCap`).
pub fn key_cap(ui: &mut Ui, key: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(key.to_owned(), theme::mono_bold(9.0), KEYCAP_INK);
    let size = vec2(galley.size().x + 14.0, galley.size().y + 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let key_owned = key.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &key_owned));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(5), KEYCAP_RIM);
        let face = Rect::from_min_max(rect.min + vec2(1.0, 1.0), rect.max - vec2(1.0, 3.0));
        painter.rect_filled(face, CornerRadius::same(4), KEYCAP_FACE);
        painter.galley(
            pos2(face.center().x - galley.size().x / 2.0, face.center().y - galley.size().y / 2.0),
            galley,
            KEYCAP_INK,
        );
    }
    response
}

/// A player badge (`QLabel#playerBadge`): brass for P1, blue for P2.
pub fn player_badge(ui: &mut Ui, badge: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(badge.to_owned(), theme::bold(9.0), WALNUT_DEEP);
    let size = vec2((galley.size().x + 16.0).max(22.0 + 16.0), galley.size().y + 2.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let badge_owned = badge.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &badge_owned));
    let fill = if badge == "P2" { P2_BLUE } else { BRASS };
    ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, WALNUT_DEEP);
    response
}

/// `_key_row`: `"P1  [W] forward  [S] back ..."` as a row of key caps.
pub fn key_row(ui: &mut Ui, badge: Option<&str>, keys: &[(String, String)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if let Some(badge) = badge {
            player_badge(ui, badge);
        }
        for (key, what) in keys {
            key_cap(ui, key);
            ui.label(egui::RichText::new(what).font(theme::font(9.0)).color(CREAM_SOFT));
            ui.add_space(6.0);
        }
    });
}

// -- headers, badges, chips, bars -----------------------------------------------

/// `_section_header`: an illustrated, readable heading for one Companion group.
pub fn section_header(ui: &mut Ui, icon_name: &str, title: &str, purpose: &str) -> Response {
    let icon = UiAssets::menu_icon(ui.ctx(), icon_name);
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (emblem, emblem_response) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
        let symbol = format!("{title} symbol");
        emblem_response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &symbol));
        if let Some(icon) = &icon {
            ui.painter().image(
                icon.id(),
                Rect::from_center_size(emblem.center(), Vec2::splat(36.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            text(ui, title, theme::bold(11.0), WALNUT_DEEP);
            wrapped(ui, purpose, theme::font(8.5), INK_SOFT);
        });
    });
    let name = format!("{title}. {purpose}");
    let response = inner.response;
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, &name));
    response
}

/// The round brass level badge (`QLabel#levelBadge`, 52x52).
pub fn level_badge(ui: &mut Ui, level: u32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
    let text = level.to_string();
    let label = format!("Level {level}");
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &label));
    let painter = ui.painter();
    let c = rect.center();
    painter.circle_filled(c, 26.0, WALNUT_DEEP);
    paint::radial_ellipse(painter, c, Vec2::splat(24.0), c + vec2(0.0, -4.0), BRASS, BRASS_DEEP);
    painter.text(c, Align2::CENTER_CENTER, text, theme::bold(16.0), WALNUT_DEEP);
    response
}

/// A team chip (`QLabel#teamChip`): the team's name on its colour.
pub fn team_chip(ui: &mut Ui, name: &str, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(name.to_owned(), theme::bold(8.5), WALNUT_DEEP);
    let size = vec2(galley.size().x + 20.0, galley.size().y + 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let label = format!("Team: {name}");
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &label));
    ui.painter().rect_filled(rect, CornerRadius::same(9), color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, WALNUT_DEEP);
    response
}

/// The thin XP bar (`QProgressBar#xpBar`, 10px, gold chunk, no text on it).
pub fn xp_bar(ui: &mut Ui, value: f32, maximum: f32, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
    let fraction = if maximum > 0.0 { (value / maximum).clamp(0.0, 1.0) } else { 0.0 };
    response.widget_info(|| {
        let mut info = WidgetInfo::new(Role::ProgressIndicator);
        info.value = Some(fraction as f64 * 100.0);
        info
    });
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(5), WALNUT_DEEP, Stroke::new(1.0, BRASS_DEEP), StrokeKind::Inside);
    if fraction > 0.0 {
        let fill =
            Rect::from_min_size(rect.min + vec2(1.0, 1.0), vec2((rect.width() - 2.0) * fraction, rect.height() - 2.0));
        paint::horizontal_gradient(painter, fill, 4.0, rgb(0xc9, 0xa0, 0x44), rgb(0xf2, 0xd4, 0x88));
    }
    response
}

/// `VitalBar`: a caption, a value and a thick painted bar.
pub struct VitalBar {
    pub caption: String,
    pub value: f32,
    pub maximum: f32,
    pub colour: Color32,
    pub min_width: f32,
}

impl VitalBar {
    pub fn new(caption: impl Into<String>, colour: Color32) -> Self {
        Self { caption: caption.into(), value: 0.0, maximum: 0.0, colour, min_width: 240.0 }
    }

    /// `set_value`: the maximum never drops below a hair, so the bar never divides by zero.
    pub fn set_value(&mut self, value: f32, maximum: f32, colour: Option<Color32>) {
        self.value = value;
        self.maximum = maximum.max(1e-6);
        if let Some(colour) = colour {
            self.colour = colour;
        }
    }

    /// The fill fraction, clamped to 0..1.
    pub fn fraction(&self) -> f32 {
        (self.value / self.maximum.max(1e-6)).clamp(0.0, 1.0)
    }

    pub fn show(&self, ui: &mut Ui) -> Response {
        let width = ui.available_width().max(self.min_width);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
        let label = format!("{} {:.0} / {:.0}", self.caption, self.value, self.maximum);
        let fraction = self.fraction();
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::ProgressIndicator, true, &label);
            info.value = Some(fraction as f64 * 100.0);
            info
        });
        let painter = ui.painter();
        let top = Rect::from_min_size(rect.min, vec2(rect.width(), 16.0));
        painter.text(top.left_center(), Align2::LEFT_CENTER, self.caption.to_uppercase(), theme::bold(9.0), CREAM_SOFT);
        painter.text(
            top.right_center(),
            Align2::RIGHT_CENTER,
            format!("{:.0} / {:.0}", self.value, self.maximum),
            theme::bold(9.0),
            CREAM,
        );
        let track = Rect::from_min_size(rect.min + vec2(0.0, 20.0), vec2(rect.width(), 14.0));
        painter.rect(
            track,
            CornerRadius::same(7),
            rgba(20, 12, 6, 220),
            Stroke::new(1.0, BRASS_DEEP),
            StrokeKind::Inside,
        );
        if fraction > 0.0 {
            let fill = Rect::from_min_size(
                track.min + vec2(2.0, 2.0),
                vec2((track.width() - 4.0) * fraction, track.height() - 4.0),
            );
            paint::vertical_gradient(painter, fill, 5.0, lighter(self.colour, 135), darker(self.colour, 120));
            let shine = Rect::from_min_size(fill.min + vec2(2.0, 1.0), vec2((fill.width() - 4.0).max(0.0), 3.0));
            painter.rect_filled(shine, CornerRadius::same(1), rgba(255, 255, 255, 50));
        }
        response
    }
}

/// `StatTile`: an icon, a big number and what it means.
pub fn stat_tile(ui: &mut Ui, kind: &str, value: &str, caption: &str) -> Response {
    theme::stat_tile()
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                crate::art::stat_icon(ui.painter(), icon, kind);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    text(ui, value, theme::bold(15.0), CREAM);
                    wrapped(ui, caption, theme::font(8.0), CREAM_SOFT);
                });
            });
        })
        .response
}

/// A heading in the dialogs' brass (`QLabel#sectionTitle` of the character
/// window, 13pt) or `#dialogTitle` (16pt).
pub fn brass_title(ui: &mut Ui, title: &str, points: f32) -> Response {
    text(ui, title, theme::bold(points), BRASS)
}

/// A wood tab bar (`QTabBar::tab`): oak tabs, the selected one brass.
/// Returns the clicked index.
pub fn tab_bar(ui: &mut Ui, tabs: &[(&str, Option<&str>)], selected: usize) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (index, (label, icon_name)) in tabs.iter().enumerate() {
            let icon = icon_name.and_then(|name| UiAssets::menu_icon(ui.ctx(), name));
            let font = theme::font(9.0);
            let galley = ui.painter().layout_no_wrap((*label).to_owned(), font, Color32::WHITE);
            let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
            let size = vec2(galley.size().x + 24.0 + icon_w, 30.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let is_selected = index == selected;
            let label_owned = (*label).to_owned();
            response.widget_info(|| WidgetInfo::selected(Role::Tab, true, is_selected, &label_owned));
            crate::probe::record(ui.ctx(), format!("tab:{label}"), rect);
            let painter = ui.painter();
            let tab = if is_selected { rect } else { Rect::from_min_max(rect.min + vec2(0.0, 3.0), rect.max) };
            let radius = CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 };
            painter.rect_filled(tab, radius, WALNUT_DEEP);
            let inner = Rect::from_min_max(tab.min + vec2(1.0, 1.0), tab.max - vec2(1.0, 0.0));
            let (top, bottom, color) = if is_selected {
                (BRASS_TOP, BRASS_DEEP, WALNUT_DEEP)
            } else if response.hovered() {
                (lift22(OAK_LIGHT), lift22(OAK_BOTTOM), BUTTON_TEXT)
            } else {
                (OAK_LIGHT, OAK_BOTTOM, BUTTON_TEXT)
            };
            // Top corners only: a gradient over the full rect, then square the bottom.
            paint::vertical_gradient(painter, inner, 6.0, top, bottom);
            painter.rect_filled(
                Rect::from_min_max(pos2(inner.left(), inner.bottom() - 6.0), inner.max),
                CornerRadius::ZERO,
                bottom,
            );
            let mut x = inner.left() + 12.0;
            if let Some(icon) = &icon {
                painter.image(
                    icon.id(),
                    Rect::from_center_size(pos2(x + 9.0, inner.center().y), Vec2::splat(18.0)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                x += icon_w;
            }
            painter.galley(pos2(x, inner.center().y - galley.size().y / 2.0), galley, color);
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

/// The carved wood board: `wood_tile.png` (or `wood_dark_tile.png`) tiled
/// over `rect`, or the plain fallback colour.
pub fn wood_board(ui: &Ui, rect: Rect, dark: bool) {
    let name = if dark { "wood_dark_tile.png" } else { "wood_tile.png" };
    let fallback = if dark { WALNUT } else { OAK };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, fallback);
    if let Some(texture) = UiAssets::texture(ui.ctx(), name) {
        let size = texture.size_vec2();
        paint::tiled(painter, rect, texture.id(), size, Color32::WHITE);
    }
}

/// The carved card frame (`card_frame_css`): `card_frame.png` nine-sliced at
/// 30px, or a walnut box with a deep border.
pub fn card_frame(ui: &Ui, rect: Rect) {
    let painter = ui.painter();
    match UiAssets::texture(ui.ctx(), "card_frame.png") {
        Some(texture) => paint::nine_slice(painter, rect, texture.id(), texture.size_vec2(), 30.0),
        None => {
            painter.rect(rect, CornerRadius::same(16), WALNUT, Stroke::new(3.0, WALNUT_DEEP), StrokeKind::Inside);
        }
    }
}

/// Lay out `add` on a carved card: the frame is painted behind the contents
/// once their size is known. `padding` is the space kept inside the carving.
pub fn carved_card<R>(ui: &mut Ui, width: f32, padding: f32, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(egui::Shape::Noop);
    let inner = ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.add_space(padding);
        let r = ui.horizontal(|ui| {
            ui.add_space(padding);
            let r = ui.vertical(|ui| {
                ui.set_width(width - padding * 2.0);
                add(ui)
            });
            ui.add_space(padding);
            r.inner
        });
        ui.add_space(padding);
        r.inner
    });
    let rect = inner.response.rect;
    let shape = match UiAssets::texture(ui.ctx(), "card_frame.png") {
        Some(texture) => {
            let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
            let size = texture.size_vec2();
            let b = 30.0f32.min(rect.width() / 2.0).min(rect.height() / 2.0);
            let (bx, by) = (30.0 / size.x, 30.0 / size.y);
            let xs = [rect.left(), rect.left() + b, rect.right() - b, rect.right()];
            let ys = [rect.top(), rect.top() + b, rect.bottom() - b, rect.bottom()];
            let us = [0.0, bx, 1.0 - bx, 1.0];
            let vs = [0.0, by, 1.0 - by, 1.0];
            for row in 0..3 {
                for col in 0..3 {
                    mesh.add_rect_with_uv(
                        Rect::from_min_max(pos2(xs[col], ys[row]), pos2(xs[col + 1], ys[row + 1])),
                        Rect::from_min_max(pos2(us[col], vs[row]), pos2(us[col + 1], vs[row + 1])),
                        Color32::WHITE,
                    );
                }
            }
            egui::Shape::mesh(mesh)
        }
        None => egui::Shape::Rect(egui::epaint::RectShape::new(
            rect,
            CornerRadius::same(16),
            WALNUT,
            Stroke::new(3.0, WALNUT_DEEP),
            StrokeKind::Inside,
        )),
    };
    ui.painter().set(slot, shape);
    inner
}

/// A combo box over `(label, value)` options; returns true when the choice changed.
pub fn combo<T: PartialEq + Clone>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    current: &mut T,
    options: &[(String, T)],
) -> Response {
    let selected_text = options.iter().find(|(_, v)| v == current).map(|(l, _)| l.clone()).unwrap_or_default();
    let mut changed = false;
    let inner = egui::ComboBox::from_id_salt(id_salt)
        .width(width)
        .selected_text(egui::RichText::new(selected_text).color(ui.visuals().widgets.inactive.fg_stroke.color))
        .show_ui(ui, |ui| {
            for (label, value) in options {
                let is = value == current;
                if ui.selectable_label(is, egui::RichText::new(label).color(INK)).clicked() && !is {
                    *current = value.clone();
                    changed = true;
                }
            }
        });
    let mut response = inner.response;
    if changed {
        response.mark_changed();
    }
    response
}

/// A small filled swatch of a team's colour (`_team_icon`).
pub fn team_dot(painter: &egui::Painter, centre: egui::Pos2, color: Color32) {
    painter.circle(centre, 6.0, color, Stroke::new(1.3, rgba(40, 44, 54, 170)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vital_bar_never_divides_by_zero() {
        let mut bar = VitalBar::new("Health", HEALTH);
        bar.set_value(5.0, 0.0, None);
        assert_eq!(bar.fraction(), 1.0);
        bar.set_value(25.0, 100.0, Some(STAMINA));
        assert!((bar.fraction() - 0.25).abs() < 1e-6);
        assert_eq!(bar.colour, STAMINA);
    }

    #[test]
    fn disabled_buttons_use_the_disabled_colours() {
        let c = button_colors(ButtonLook::Wood, true, false, false);
        assert_eq!(c.top, DISABLED_BG);
        assert_eq!(c.text, DISABLED_TEXT);
        let hover = button_colors(ButtonLook::Wood, true, false, true);
        assert_eq!(hover.top, lift22(OAK_LIGHT));
        let pressed = button_colors(ButtonLook::Brass, true, true, true);
        assert_eq!(pressed.top, BRASS_DEEP);
        let stop = button_colors(ButtonLook::Stop, true, false, true);
        assert_eq!(stop.top, STOP_HOVER);
    }
}
