//! The carved woodland pop-up menu: `wood_theme.app_qss`'s `QMenu` look
//! (a dark wood board, a walnut border, 22 px `menu_*.png` emblems, oak
//! highlight, cream text, check indicators) for the overlay's right-click
//! menu in `app/engine.py`.
//!
//! The Qt menu showed each action's `toolTip` as a native tooltip beside it.
//! A pop-up window only as big as the menu would clip an egui tooltip, so the
//! hover explanation is written into a parchment strip along the bottom of
//! the menu instead. Submenus (`QMenu.addMenu`) open in place under their
//! entry, for the same reason.
//!
//! The menu is pure egui: [`WoodMenu::ui`] draws it over the whole viewport
//! and reports the chosen entry, an Esc dismissal, and the size it wants,
//! which the window host uses to size the pop-up.

use std::collections::BTreeSet;

use egui::accesskit::Role;
use egui::{Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2};

use crate::art;
use crate::assets::UiAssets;
use crate::probe;
use crate::theme::{self, *};
use crate::widgets;

/// `QMenu { padding: 4px }` plus the 2 px border.
pub const MENU_PADDING: f32 = 6.0;
/// `QMenu::item { padding: 3px ...; min-height: 20px }`.
pub const ROW_HEIGHT: f32 = 26.0;
/// `QMenu::separator { height: 1px; margin: 4px 8px }`.
pub const SEPARATOR_HEIGHT: f32 = 9.0;
/// `QMenu { icon-size: 22px }`.
pub const ICON_SIZE: f32 = 22.0;
/// `QMenu::indicator { width: 14px }`.
pub const CHECK_SIZE: f32 = 14.0;
/// How far a submenu's entries sit in from their parent.
pub const SUBMENU_INDENT: f32 = 14.0;
/// The explanation strip: three lines of 8.5 pt text.
pub const TIP_LINES: f32 = 3.0;
pub const MIN_WIDTH: f32 = 220.0;
/// `QMenu::item:disabled { color: #d7c29e }`.
pub const DISABLED_TEXT: Color32 = rgb(0xd7, 0xc2, 0x9e);
/// `QMenu::item:selected { color: #fff5e0 }`.
pub const SELECTED_TEXT: Color32 = rgb(0xff, 0xf5, 0xe0);

/// The picture at the left of an entry.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuIcon {
    /// `wood_theme.menu_icon(name)`: `menu_<name>.png`.
    Emblem(String),
    /// `skill_icon(id, "learned" | "available", 24)`.
    Skill { id: String, learned: bool },
}

impl MenuIcon {
    pub fn emblem(name: &str) -> Option<MenuIcon> {
        Some(MenuIcon::Emblem(name.to_owned()))
    }
}

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// A disabled bold heading (`title.setEnabled(False)` + bold font).
    Title {
        text: String,
        icon: Option<MenuIcon>,
    },
    /// An action. `checked` is `Some` for a checkable one.
    Item {
        id: String,
        text: String,
        icon: Option<MenuIcon>,
        tip: Option<String>,
        checked: Option<bool>,
        enabled: bool,
    },
    /// A disabled line of text (the tray menu's `add_note`).
    Note {
        text: String,
        icon: Option<MenuIcon>,
    },
    Separator,
    /// `QMenu.addMenu`: opens under its entry when clicked.
    Submenu {
        id: String,
        text: String,
        icon: Option<MenuIcon>,
        tip: Option<String>,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    pub fn title(text: impl Into<String>, icon: Option<MenuIcon>) -> Self {
        MenuEntry::Title { text: text.into(), icon }
    }

    pub fn item(id: impl Into<String>, text: impl Into<String>, icon: Option<MenuIcon>) -> Self {
        MenuEntry::Item { id: id.into(), text: text.into(), icon, tip: None, checked: None, enabled: true }
    }

    pub fn check(id: impl Into<String>, text: impl Into<String>, icon: Option<MenuIcon>, checked: bool) -> Self {
        MenuEntry::Item { id: id.into(), text: text.into(), icon, tip: None, checked: Some(checked), enabled: true }
    }

    pub fn note(text: impl Into<String>) -> Self {
        MenuEntry::Note { text: text.into(), icon: None }
    }

    pub fn submenu(
        id: impl Into<String>,
        text: impl Into<String>,
        icon: Option<MenuIcon>,
        entries: Vec<MenuEntry>,
    ) -> Self {
        MenuEntry::Submenu { id: id.into(), text: text.into(), icon, tip: None, entries }
    }

    /// `setToolTip`: the hover explanation.
    pub fn tip(mut self, text: impl Into<String>) -> Self {
        match &mut self {
            MenuEntry::Item { tip, .. } | MenuEntry::Submenu { tip, .. } => *tip = Some(text.into()),
            _ => {}
        }
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        if let MenuEntry::Item { enabled, .. } = &mut self {
            *enabled = on;
        }
        self
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            MenuEntry::Item { id, .. } | MenuEntry::Submenu { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            MenuEntry::Title { text, .. }
            | MenuEntry::Item { text, .. }
            | MenuEntry::Note { text, .. }
            | MenuEntry::Submenu { text, .. } => Some(text),
            MenuEntry::Separator => None,
        }
    }

    fn icon(&self) -> Option<&MenuIcon> {
        match self {
            MenuEntry::Title { icon, .. }
            | MenuEntry::Item { icon, .. }
            | MenuEntry::Note { icon, .. }
            | MenuEntry::Submenu { icon, .. } => icon.as_ref(),
            MenuEntry::Separator => None,
        }
    }

    fn tip_text(&self) -> Option<&str> {
        match self {
            MenuEntry::Item { tip, .. } | MenuEntry::Submenu { tip, .. } => tip.as_deref(),
            _ => None,
        }
    }
}

/// What a frame of the menu produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuOutput {
    /// The id of the action chosen this frame.
    pub chosen: Option<String>,
    /// For a checkable action, its new state (`toggled(bool)`).
    pub checked: Option<bool>,
    /// Esc: close without choosing.
    pub dismissed: bool,
    /// The size the menu wants, in points.
    pub size: Vec2,
}

/// The menu and what is open and highlighted in it.
#[derive(Clone, Debug, Default)]
pub struct WoodMenu {
    pub entries: Vec<MenuEntry>,
    /// Expanded submenus, by id.
    pub open: BTreeSet<String>,
    /// The entry under the pointer or the keyboard highlight, by text.
    hovered_tip: Option<String>,
    /// Keyboard highlight: an index into [`WoodMenu::rows`].
    keyboard: Option<usize>,
    width: Option<f32>,
}

/// One visible row with its nesting depth.
#[derive(Clone, Debug)]
struct Row<'a> {
    entry: &'a MenuEntry,
    depth: usize,
}

impl WoodMenu {
    pub fn new(entries: Vec<MenuEntry>) -> Self {
        Self { entries, ..Default::default() }
    }

    /// Every action and submenu id, depth first (closed submenus included).
    pub fn ids(&self) -> Vec<String> {
        fn walk(entries: &[MenuEntry], out: &mut Vec<String>) {
            for e in entries {
                if let Some(id) = e.id() {
                    out.push(id.to_owned());
                }
                if let MenuEntry::Submenu { entries, .. } = e {
                    walk(entries, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.entries, &mut out);
        out
    }

    /// Find an entry by id anywhere in the menu.
    pub fn find(&self, id: &str) -> Option<&MenuEntry> {
        fn walk<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a MenuEntry> {
            for e in entries {
                if e.id() == Some(id) {
                    return Some(e);
                }
                if let MenuEntry::Submenu { entries, .. } = e
                    && let Some(found) = walk(entries, id)
                {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.entries, id)
    }

    fn rows(&self) -> Vec<Row<'_>> {
        fn walk<'a>(entries: &'a [MenuEntry], depth: usize, open: &BTreeSet<String>, out: &mut Vec<Row<'a>>) {
            for e in entries {
                out.push(Row { entry: e, depth });
                if let MenuEntry::Submenu { id, entries, .. } = e
                    && open.contains(id)
                {
                    walk(entries, depth + 1, open, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.entries, 0, &self.open, &mut out);
        out
    }

    fn all_entries(&self) -> Vec<(&MenuEntry, usize)> {
        fn walk<'a>(entries: &'a [MenuEntry], depth: usize, out: &mut Vec<(&'a MenuEntry, usize)>) {
            for e in entries {
                out.push((e, depth));
                if let MenuEntry::Submenu { entries, .. } = e {
                    walk(entries, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.entries, 0, &mut out);
        out
    }

    fn has_checks(&self) -> bool {
        self.all_entries().iter().any(|(e, _)| matches!(e, MenuEntry::Item { checked: Some(_), .. }))
    }

    fn has_tips(&self) -> bool {
        self.all_entries().iter().any(|(e, _)| e.tip_text().is_some())
    }

    fn text_left(&self, depth: usize) -> f32 {
        let check = if self.has_checks() { CHECK_SIZE + 6.0 } else { 0.0 };
        8.0 + depth as f32 * SUBMENU_INDENT + check + ICON_SIZE + 8.0
    }

    fn row_font(entry: &MenuEntry) -> egui::FontId {
        if matches!(entry, MenuEntry::Title { .. }) { theme::bold(9.0) } else { theme::font(9.0) }
    }

    /// The width every entry fits in (submenus counted even when closed, so
    /// opening one does not resize the pop-up sideways).
    fn measure_width(&self, ui: &Ui) -> f32 {
        let mut width = MIN_WIDTH;
        for (entry, depth) in self.all_entries() {
            let Some(text) = entry.text() else { continue };
            let galley = ui.painter().layout_no_wrap(text.to_owned(), Self::row_font(entry), CREAM);
            let arrow = if matches!(entry, MenuEntry::Submenu { .. }) { 16.0 } else { 0.0 };
            width = width.max(self.text_left(depth) + galley.size().x + 22.0 + arrow + 2.0 * MENU_PADDING);
        }
        width.ceil()
    }

    fn tip_height(&self) -> f32 {
        if self.has_tips() { (theme::pt(8.5) * 1.35 * TIP_LINES + 12.0).ceil() } else { 0.0 }
    }

    /// The height of the visible rows plus the explanation strip.
    fn measure_height(&self) -> f32 {
        let rows: f32 = self
            .rows()
            .iter()
            .map(|r| if matches!(r.entry, MenuEntry::Separator) { SEPARATOR_HEIGHT } else { ROW_HEIGHT })
            .sum();
        let tips = self.tip_height();
        (2.0 * MENU_PADDING + rows + if tips > 0.0 { tips + 4.0 } else { 0.0 }).ceil()
    }

    /// Draw over the whole viewport.
    pub fn ui(&mut self, ctx: &egui::Context) -> MenuOutput {
        ctx.root_ui(|ui| self.show(ui))
    }

    /// Activatable rows (items and submenus), in order, as row indices.
    fn activatable(&self) -> Vec<usize> {
        self.rows()
            .iter()
            .enumerate()
            .filter(|(_, r)| match r.entry {
                MenuEntry::Item { enabled, .. } => *enabled,
                MenuEntry::Submenu { .. } => true,
                _ => false,
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn show(&mut self, ui: &mut Ui) -> MenuOutput {
        let mut out = MenuOutput::default();
        let width = match self.width {
            Some(w) => w,
            None => {
                let w = self.measure_width(ui);
                self.width = Some(w);
                w
            }
        };
        out.size = vec2(width, self.measure_height());

        // Keyboard: Esc closes, arrows move, Enter chooses (QMenu's keys).
        let (escape, up, down, enter) = ui.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter) || i.key_pressed(Key::Space),
            )
        });
        if escape {
            out.dismissed = true;
            return out;
        }
        let order = self.activatable();
        if (up || down) && !order.is_empty() {
            let at = self.keyboard.and_then(|k| order.iter().position(|i| *i == k));
            let next = match (at, down) {
                (None, true) => 0,
                (None, false) => order.len() - 1,
                (Some(p), true) => (p + 1) % order.len(),
                (Some(p), false) => (p + order.len() - 1) % order.len(),
            };
            self.keyboard = Some(order[next]);
        }

        let full = ui.max_rect();
        widgets::wood_board(ui, full, true);
        ui.painter().rect_stroke(
            full.shrink(1.0),
            CornerRadius::same(6),
            Stroke::new(2.0, WALNUT_DEEP),
            StrokeKind::Inside,
        );

        let rows: Vec<(MenuEntry, usize)> = self.rows().into_iter().map(|r| (r.entry.clone(), r.depth)).collect();
        let has_checks = self.has_checks();
        let mut y = full.top() + MENU_PADDING;
        let left = full.left() + MENU_PADDING;
        let row_width = width - 2.0 * MENU_PADDING;
        let mut hovered_tip: Option<String> = None;
        let mut toggle_submenu: Option<String> = None;
        let pointer_moved = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
        for (index, (entry, depth)) in rows.iter().enumerate() {
            if matches!(entry, MenuEntry::Separator) {
                let line_y = y + SEPARATOR_HEIGHT / 2.0;
                ui.painter().line_segment(
                    [pos2(left + 8.0, line_y), pos2(left + row_width - 8.0, line_y)],
                    Stroke::new(1.0, WALNUT_DEEP),
                );
                y += SEPARATOR_HEIGHT;
                continue;
            }
            let rect = Rect::from_min_size(pos2(left, y), vec2(row_width, ROW_HEIGHT));
            y += ROW_HEIGHT;
            let (interactive, enabled) = match entry {
                MenuEntry::Item { enabled, .. } => (true, *enabled),
                MenuEntry::Submenu { .. } => (true, true),
                _ => (false, false),
            };
            let sense = if interactive && enabled { Sense::click() } else { Sense::hover() };
            let id = ui.scope_id().with(("menu-row", index, entry.text().unwrap_or("")));
            let response = ui.interact(rect, id, sense);
            if let Some(key) = entry.id() {
                probe::record(ui.ctx(), format!("menu:{key}"), rect);
            }
            let text = entry.text().unwrap_or("").to_owned();
            response.widget_info(|| match entry {
                MenuEntry::Item { checked: Some(c), enabled, .. } => {
                    WidgetInfo::selected(Role::MenuItemCheckBox, *enabled, *c, &text)
                }
                MenuEntry::Item { enabled, .. } => WidgetInfo::labeled(Role::MenuItem, *enabled, &text),
                MenuEntry::Submenu { id, .. } => {
                    WidgetInfo::selected(Role::MenuItem, true, self.open.contains(id), &text)
                }
                _ => WidgetInfo::labeled(Role::Label, false, &text),
            });
            if response.hovered() && pointer_moved && interactive {
                self.keyboard = Some(index);
            }
            let highlighted = interactive && enabled && (response.hovered() || self.keyboard == Some(index));
            if highlighted {
                ui.painter().rect_filled(rect.shrink2(vec2(0.0, 1.0)), CornerRadius::same(4), OAK);
                if let Some(tip) = entry.tip_text() {
                    hovered_tip = Some(tip.to_owned());
                }
            }
            let colour = if !enabled {
                DISABLED_TEXT
            } else if highlighted {
                SELECTED_TEXT
            } else {
                CREAM
            };
            let mut x = rect.left() + 8.0 + *depth as f32 * SUBMENU_INDENT;
            if has_checks {
                if let MenuEntry::Item { checked: Some(c), .. } = entry {
                    draw_check(
                        ui,
                        Rect::from_center_size(pos2(x + CHECK_SIZE / 2.0, rect.center().y), Vec2::splat(CHECK_SIZE)),
                        *c,
                    );
                }
                x += CHECK_SIZE + 6.0;
            }
            let icon_rect = Rect::from_center_size(pos2(x + ICON_SIZE / 2.0, rect.center().y), Vec2::splat(ICON_SIZE));
            if let Some(icon) = entry.icon() {
                draw_icon(ui, icon_rect, icon, enabled);
            }
            x += ICON_SIZE + 8.0;
            ui.painter().text(
                pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &text,
                Self::row_font(entry),
                colour,
            );
            if let MenuEntry::Submenu { id, .. } = entry {
                let arrow = if self.open.contains(id) { "\u{25be}" } else { "\u{25b8}" };
                ui.painter().text(
                    pos2(rect.right() - 10.0, rect.center().y),
                    egui::Align2::CENTER_CENTER,
                    arrow,
                    theme::font(9.0),
                    colour,
                );
            }
            let activated = response.clicked() || (enter && self.keyboard == Some(index) && enabled);
            if activated && interactive && enabled {
                match entry {
                    MenuEntry::Submenu { id, .. } => toggle_submenu = Some(id.clone()),
                    MenuEntry::Item { id, checked, .. } => {
                        out.chosen = Some(id.clone());
                        out.checked = checked.map(|c| !c);
                    }
                    _ => {}
                }
            }
        }
        if let Some(id) = toggle_submenu {
            if !self.open.remove(&id) {
                self.open.insert(id);
            }
            self.keyboard = None;
            out.size = vec2(width, self.measure_height());
            ui.ctx().request_repaint();
        }
        if let Some(chosen) = &out.chosen
            && let Some(new_state) = out.checked
        {
            set_checked(&mut self.entries, chosen, new_state);
        }
        if hovered_tip != self.hovered_tip {
            self.hovered_tip = hovered_tip;
            ui.ctx().request_repaint();
        }
        let tip_height = self.tip_height();
        if tip_height > 0.0 {
            let strip = Rect::from_min_size(pos2(left, y + 4.0), vec2(row_width, tip_height));
            ui.painter().rect(
                strip,
                CornerRadius::same(4),
                PARCHMENT,
                Stroke::new(1.0, WALNUT_DEEP),
                StrokeKind::Inside,
            );
            if let Some(tip) = &self.hovered_tip {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(strip.shrink2(vec2(8.0, 5.0))));
                child.add(egui::Label::new(egui::RichText::new(tip).font(theme::font(8.5)).color(INK)).wrap());
            }
        }
        out
    }
}

fn set_checked(entries: &mut [MenuEntry], id: &str, state: bool) {
    for e in entries {
        match e {
            MenuEntry::Item { id: own, checked: Some(c), .. } if own == id => *c = state,
            MenuEntry::Submenu { entries, .. } => set_checked(entries, id, state),
            _ => {}
        }
    }
}

/// `QMenu::indicator`: a parchment box with an ink tick.
fn draw_check(ui: &Ui, rect: Rect, checked: bool) {
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(3), PARCHMENT, Stroke::new(1.0, WALNUT_DEEP), StrokeKind::Inside);
    if checked {
        let a: Pos2 = rect.left_center() + vec2(3.0, 0.0);
        let b: Pos2 = rect.center_bottom() + vec2(-1.0, -3.5);
        let c: Pos2 = rect.right_top() + vec2(-3.0, 3.0);
        p.line_segment([a, b], Stroke::new(2.0, INK));
        p.line_segment([b, c], Stroke::new(2.0, INK));
    }
}

fn draw_icon(ui: &Ui, rect: Rect, icon: &MenuIcon, enabled: bool) {
    match icon {
        MenuIcon::Emblem(name) => {
            if let Some(texture) = UiAssets::menu_icon(ui.ctx(), name) {
                let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(170) };
                ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
            }
        }
        MenuIcon::Skill { id, learned } => {
            let look = art::Look::from_name(if *learned { "learned" } else { "available" });
            art::skill_icon(ui.painter(), rect, id, look);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WoodMenu {
        WoodMenu::new(vec![
            MenuEntry::title("SPIDER  \u{b7}  Bob", MenuIcon::emblem("companion")),
            MenuEntry::Separator,
            MenuEntry::item("inspect", "Inspect", MenuIcon::emblem("inspect")).tip("Look closer."),
            MenuEntry::check("pin", "Pin level above name", MenuIcon::emblem("pin"), false),
            MenuEntry::submenu(
                "skills",
                "Skills for this spider",
                MenuIcon::emblem("skills"),
                vec![MenuEntry::check("skill:wander", "Wander / roam", None, true)],
            ),
        ])
    }

    #[test]
    fn ids_walk_into_submenus() {
        let menu = sample();
        assert_eq!(menu.ids(), ["inspect", "pin", "skills", "skill:wander"]);
        assert!(menu.find("skill:wander").is_some());
    }

    #[test]
    fn opening_a_submenu_adds_its_rows() {
        let mut menu = sample();
        let closed = menu.measure_height();
        menu.open.insert("skills".into());
        assert_eq!(menu.measure_height(), closed + ROW_HEIGHT);
    }

    #[test]
    fn checkable_entries_report_their_new_state() {
        let mut menu = sample();
        set_checked(&mut menu.entries, "skill:wander", false);
        assert!(matches!(menu.find("skill:wander"), Some(MenuEntry::Item { checked: Some(false), .. })));
    }
}
