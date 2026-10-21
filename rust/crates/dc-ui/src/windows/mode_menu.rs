//! Port of app/mode_menu.py: mode selection around the Companion settings.
//!
//! Three modes: Companion (the living desktop colony), Adventure (take
//! control of one spider; skirmish missions grow here) and Strategy (colony
//! command, later). The Adventure page lists the campaign maps as picture
//! cards, then your own maps, then the raid setup card: the One/Two player
//! segment control, the keys as key caps, and a pill switch per screen.

use egui::{Align, Align2, Layout, RichText, Sense, TextureHandle, Ui, Vec2, pos2, vec2};

use crate::UiAction;
use crate::assets::UiAssets;
use crate::model::{AdventureView, ControlSettings, CustomMapCard, MapCard};
use crate::paint;
use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};

/// `MODES`: (kind, title, description).
pub const MODES: [(&str, &str, &str); 3] = [
    (
        "companion",
        "Companion",
        "Your spiders live on the desktop: they hunt flies, spin webs, build bases and grow. Manage the colony.",
    ),
    (
        "skirmish",
        "Adventure",
        "Take control of one spider. Hunt, fight and level up with every skill it has. Skirmish missions grow here.",
    ),
    (
        "strategy",
        "Strategy",
        "Command a colony against rival teams: build bases, direct squads, take territory. Coming later.",
    ),
];

pub const MISSION_COLUMNS: usize = 3;

/// Paints a map card's picture (app/map_art.map_picture, in dc-paint):
/// `(map id, kind, locked)`.
pub type MapPictureFn = Box<dyn Fn(&str, &str, bool) -> Option<TextureHandle>>;

/// Which page of the shell is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Companion,
    Skirmish,
    Strategy,
}

impl Page {
    pub fn kind(self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Companion => "companion",
            Page::Skirmish => "skirmish",
            Page::Strategy => "strategy",
        }
    }
    pub fn from_kind(kind: &str) -> Page {
        match kind {
            "companion" => Page::Companion,
            "skirmish" => Page::Skirmish,
            "strategy" => Page::Strategy,
            _ => Page::Home,
        }
    }
}

/// The mode menu and its pages (`ModeShell`).
pub struct ModeShell {
    pub page: Page,
    pub adventure: AdventureView,
    map_picture: Option<MapPictureFn>,
}

/// The essential keys of one player as (key, what) pairs (`refresh_controls_summary`).
pub fn controls_summary(settings: &ControlSettings) -> (Vec<(String, String)>, String) {
    let keys: Vec<String> =
        ["move_up", "move_left", "move_down", "move_right"].iter().map(|a| settings.binding(a)).collect();
    let walk = if keys.iter().all(|k| k.chars().count() == 1) { keys.concat() } else { keys.join("/") };
    let walking = if settings.turn_movement() { "walk and turn" } else { "walk" };
    let mut caps = vec![(walk, walking.to_owned())];
    for action in ["sprint", "jump", "shoot", "bite", "pause"] {
        caps.push((settings.binding(action), settings.action_text(action).0.to_lowercase()));
    }
    let cone = if settings.aim_cone >= 360 {
        "free aim".to_owned()
    } else {
        format!("mouse aims in a {}\u{b0} cone", settings.aim_cone)
    };
    let mut note = cone.clone();
    if let Some(first) = note.get(..1) {
        note = first.to_uppercase() + &note[1..];
    }
    (caps, format!("{note} \u{b7} ` map \u{b7} F8 release"))
}

/// The fixed two-player keys of the raid setup.
pub fn coop_keys() -> [(&'static str, Vec<(String, String)>); 2] {
    let k = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect::<Vec<_>>();
    [
        (
            "P1",
            k(&[
                ("W", "forward"),
                ("S", "back"),
                ("A D", "turn"),
                ("F", "bite"),
                ("Q", "web"),
                ("Space", "jump"),
                ("Shift", "sprint"),
                ("K", "gear"),
            ]),
        ),
        (
            "P2",
            k(&[
                ("\u{2191}", "forward"),
                ("\u{2193}", "back"),
                ("\u{2190} \u{2192}", "turn"),
                ("J", "bite"),
                ("U", "web"),
                ("I", "jump"),
                ("O", "sprint"),
                ("P", "gear"),
            ]),
        ),
    ]
}

impl ModeShell {
    pub fn new(adventure: AdventureView) -> Self {
        Self { page: Page::Home, adventure, map_picture: None }
    }

    pub fn set_map_pictures(&mut self, painter: MapPictureFn) {
        self.map_picture = Some(painter);
    }

    /// `refresh_adventure`: new profile data after a mission or a dialog.
    pub fn refresh_adventure(&mut self, adventure: AdventureView) {
        self.adventure = adventure;
    }

    /// `show_mode`. Leaving the Adventure page asks to close an Adventure
    /// overlay (`_page_changed`).
    pub fn show_mode(&mut self, page: Page, actions: &mut Vec<UiAction>) {
        if page == self.page {
            return;
        }
        if self.page == Page::Skirmish {
            actions.push(UiAction::LeaveAdventure);
        }
        self.page = page;
        actions.push(UiAction::ShowMode(page.kind().into()));
    }

    /// Draw the shell; `companion` draws the Companion page body.
    pub fn show(&mut self, ui: &mut Ui, companion: &mut dyn FnMut(&mut Ui) -> Vec<UiAction>) -> Vec<UiAction> {
        let mut actions = Vec::new();
        let rect = ui.max_rect();
        widgets::wood_board(ui, rect, false);
        theme::with_visuals(ui, theme::shell_visuals(), |ui| match self.page {
            Page::Home => self.home(ui, &mut actions),
            Page::Companion => {
                if self.page_header(ui, "Companion", "Your living desktop colony") {
                    self.show_mode(Page::Home, &mut actions);
                    return;
                }
                actions.extend(companion(ui));
            }
            Page::Skirmish => {
                if self.page_header(ui, "Adventure", "Play as your spider") {
                    self.show_mode(Page::Home, &mut actions);
                    return;
                }
                self.adventure_page(ui, &mut actions);
            }
            Page::Strategy => {
                if self.page_header(ui, "Strategy", "Colony command") {
                    self.show_mode(Page::Home, &mut actions);
                    return;
                }
                self.strategy_page(ui);
            }
        });
        actions
    }

    /// `_page`'s header bar; returns true when "← Modes" was pressed.
    fn page_header(&mut self, ui: &mut Ui, title: &str, subtitle: &str) -> bool {
        let mut back = false;
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 6 }).show(ui, |ui| {
            theme::page_header().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let r = ui.add(CarvedButton::new("\u{2190} Modes"));
                    probe::record(ui.ctx(), "button:Modes", r.rect);
                    back = r.clicked();
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                    paint::engraved_text(
                        ui.painter(),
                        rect.left_center(),
                        Align2::LEFT_CENTER,
                        &format!("{title}  \u{b7}  {subtitle}"),
                        theme::bold(17.0),
                        CREAM,
                        false,
                    );
                    ui.ctx().accesskit_node_builder(ui.scope_id().with(title), |node| {
                        node.set_role(egui::accesskit::Role::Heading);
                        node.set_label(format!("{title}  \u{b7}  {subtitle}"));
                    });
                });
            });
        });
        back
    }

    fn art(&self, ui: &mut Ui, kind: &str, width: f32) {
        if let Some(texture) = UiAssets::mode_art(ui.ctx(), kind) {
            let size = texture.size_vec2();
            let height = width * size.y / size.x.max(1.0);
            ui.add(egui::Image::new((texture.id(), vec2(width, height))));
        }
    }

    fn home(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        egui::ScrollArea::vertical().id_salt("home-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                let heading = ui.label(RichText::new("Desktop Companion").font(theme::bold(30.0)).color(WALNUT_DEEP));
                let _ = heading;
                ui.label(RichText::new("Choose how to play").font(theme::bold(12.0)).color(WALNUT));
                ui.add_space(14.0);
                let card_w = ((ui.available_width() - 2.0 * 18.0 - 72.0) / 3.0).clamp(250.0, 340.0);
                let total = card_w * 3.0 + 36.0;
                ui.allocate_ui_with_layout(vec2(total, 0.0), Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    for (kind, title, description) in MODES {
                        widgets::carved_card(ui, card_w, 30.0, |ui| {
                            self.art(ui, kind, card_w - 60.0);
                            widgets::text(ui, title, theme::bold(17.0), CREAM);
                            widgets::wrapped(ui, description, theme::font(10.0), CREAM_SOFT);
                            ui.add_space(6.0);
                            let label = if kind == "strategy" { "Preview".to_owned() } else { format!("Play {title}") };
                            let icon =
                                UiAssets::menu_icon(ui.ctx(), if kind == "skirmish" { "adventure" } else { kind });
                            let r = ui.add(
                                CarvedButton::new(label)
                                    .brass()
                                    .icon(icon, 30.0)
                                    .font(theme::bold(11.0))
                                    .padding(vec2(18.0, 9.0)),
                            );
                            probe::record(ui.ctx(), format!("mode:{kind}"), r.rect);
                            if r.clicked() {
                                self.show_mode(Page::from_kind(kind), actions);
                            }
                        });
                    }
                });
            });
        });
    }

    fn map_picture(&self, ui: &mut Ui, id: &str, kind: &str, locked: bool) {
        let size = vec2(196.0, 110.0);
        if let Some(texture) = self.map_picture.as_ref().and_then(|f| f(id, kind, locked)) {
            ui.add(egui::Image::new((texture.id(), size)));
            return;
        }
        // A plain stand-in until map_art is wired: the kind's ground and buildings.
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let ground = match kind {
            "swarm" => rgb(0x5e, 0x7a, 0x4c),
            "reclaim" => rgb(0x4a, 0x5a, 0x6e),
            _ => rgb(0x6e, 0x5a, 0x3a),
        };
        let ground = if locked { theme::darker(ground, 180) } else { ground };
        let painter = ui.painter();
        painter.rect(
            rect,
            egui::CornerRadius::same(6),
            ground,
            egui::Stroke::new(1.5, BRASS_DEEP),
            egui::StrokeKind::Inside,
        );
        let mut rng = dc_core::PyRandom::from_str_seed(id);
        for _ in 0..5 {
            let p = pos2(
                rect.left() + 18.0 + rng.random() as f32 * (rect.width() - 36.0),
                rect.top() + 18.0 + rng.random() as f32 * (rect.height() - 36.0),
            );
            painter.circle(
                p,
                7.0,
                theme::with_alpha(CREAM, if locked { 90 } else { 200 }),
                egui::Stroke::new(1.0, WALNUT_DEEP),
            );
        }
        if locked {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "\u{1f512}",
                theme::font(18.0),
                theme::with_alpha(CREAM, 160),
            );
        }
    }

    fn mission_card(&self, ui: &mut Ui, map: &MapCard, actions: &mut Vec<UiAction>) {
        let open = map.playable && self.adventure.map_unlocked(&map.id);
        let response = theme::mission_card(!open)
            .show(ui, |ui| {
                // A grid cell lays out left to right; a card stacks downwards.
                ui.with_layout(Layout::top_down(Align::Min), |ui| self.mission_card_body(ui, map, open, actions));
            })
            .response;
        if !open {
            paint::dashed_rounded_rect(ui.painter(), response.rect.shrink(1.0), 10.0, egui::Stroke::new(2.0, INK_SOFT));
        }
        probe::record(ui.ctx(), format!("map:{}", map.id), response.rect);
    }

    fn mission_card_body(&self, ui: &mut Ui, map: &MapCard, open: bool, actions: &mut Vec<UiAction>) {
        ui.set_width(206.0);
        ui.set_min_height(300.0);
        self.map_picture(ui, &map.id, &map.kind, !open);
        widgets::wrapped(ui, &map.title, theme::bold(11.0), CREAM);
        ui.allocate_ui(vec2(206.0, 104.0), |ui| {
            ui.set_min_height(104.0);
            widgets::wrapped(ui, &map.blurb, theme::font(9.0), CREAM_SOFT);
        });
        if map.playable {
            let (status, won) = map.record.status_text();
            if !status.is_empty() {
                let (font, color) = if won { (theme::bold(9.0), MISSION_DONE) } else { (theme::font(9.0), OAK_LIGHT) };
                widgets::wrapped(ui, &status, font, color);
            }
            if open {
                let r = ui.add(
                    CarvedButton::new("Play")
                        .brass()
                        .icon(UiAssets::menu_icon(ui.ctx(), "adventure"), 23.0)
                        .font(theme::bold(11.0)),
                );
                probe::record(ui.ctx(), format!("map:{}:play", map.id), r.rect);
                if r.clicked() {
                    actions.push(UiAction::PlayMap { map_id: map.id.clone() });
                }
            } else {
                let before = map
                    .unlock_after
                    .as_deref()
                    .and_then(|b| self.adventure.map(b))
                    .map(|m| m.title.clone())
                    .unwrap_or_default();
                let text = format!("Win \u{201c}{before}\u{201d} to unlock");
                let r = ui.label(RichText::new(text).font(theme::font(9.0)).italics().color(OAK_LIGHT));
                probe::record(ui.ctx(), format!("map:{}:locked", map.id), r.rect);
            }
        } else {
            ui.label(RichText::new("Coming soon").font(theme::font(9.0)).italics().color(OAK_LIGHT));
        }
    }

    fn custom_card(&self, ui: &mut Ui, data: &CustomMapCard, actions: &mut Vec<UiAction>) {
        theme::mission_card(false).show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.set_width(206.0);
                self.map_picture(ui, &data.id, if data.kind.is_empty() { "raid" } else { &data.kind }, false);
                widgets::wrapped(ui, &data.title, theme::bold(11.0), CREAM);
                let text = if data.blurb.is_empty() {
                    format!("Tier {}, {} buildings", data.tier, data.buildings)
                } else {
                    data.blurb.clone()
                };
                widgets::wrapped(ui, &text, theme::font(9.0), CREAM_SOFT);
                if data.victories > 0 {
                    widgets::text(ui, format!("\u{2714} Won \u{d7}{}", data.victories), theme::bold(9.0), MISSION_DONE);
                }
                ui.horizontal(|ui| {
                    let play = ui.add(CarvedButton::new("Play").brass().enabled(data.playable));
                    probe::record(ui.ctx(), format!("custom:{}:play", data.id), play.rect);
                    if play.clicked() {
                        actions.push(UiAction::PlayMap { map_id: data.id.clone() });
                    }
                    let edit = ui.add(CarvedButton::new("Edit"));
                    probe::record(ui.ctx(), format!("custom:{}:edit", data.id), edit.rect);
                    if edit.clicked() {
                        actions.push(UiAction::OpenMapEditor { map_id: Some(data.id.clone()) });
                    }
                });
            });
        });
    }

    /// One row of mission cards, each in a fixed 232px column, centred.
    fn card_row(ui: &mut Ui, count: usize, mut card: impl FnMut(&mut Ui, usize)) {
        let column = 232.0;
        let full = MISSION_COLUMNS as f32 * column + (MISSION_COLUMNS as f32 - 1.0) * 12.0;
        ui.allocate_ui_with_layout(vec2(full, 0.0), Layout::left_to_right(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for i in 0..count {
                ui.allocate_ui_with_layout(vec2(column, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_max_width(column);
                    card(ui, i);
                });
            }
        });
        ui.add_space(12.0);
    }

    fn heading(ui: &mut Ui, text: &str) {
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            paint::engraved_text(
                ui.painter(),
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                theme::bold(14.0),
                WALNUT_DEEP,
                true,
            );
            ui.ctx().accesskit_node_builder(ui.scope_id().with(text), |node| {
                node.set_role(egui::accesskit::Role::Heading);
                node.set_label(text.to_owned());
            });
        });
    }

    fn adventure_page(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        egui::ScrollArea::vertical().id_salt("adventure-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(760.0);
                self.art(ui, "skirmish", 200.0);
                // The hero, named and levelled in the character window.
                theme::hero_strip().show(ui, |ui| {
                    ui.set_width(716.0);
                    ui.horizontal(|ui| {
                        let line = self.adventure.hero_line();
                        ui.add_sized(
                            vec2(560.0, 30.0),
                            egui::Label::new(RichText::new(line).font(theme::bold(11.0)).color(CREAM)).wrap(),
                        );
                        let r = ui.add(
                            CarvedButton::new("Character\u{2026}")
                                .brass()
                                .icon(UiAssets::menu_icon(ui.ctx(), "skills"), 26.0),
                        );
                        probe::record(ui.ctx(), "button:Character", r.rect);
                        if r.clicked() {
                            actions.push(UiAction::OpenCharacter);
                        }
                    });
                });
                Self::heading(ui, "Skirmish missions");
                let maps = self.adventure.maps.clone();
                for row in maps.chunks(MISSION_COLUMNS) {
                    Self::card_row(ui, row.len(), |ui, i| self.mission_card(ui, &row[i], actions));
                }
                // Maps made in the map editor.
                let customs = self.adventure.custom_maps.clone();
                if !customs.is_empty() {
                    Self::heading(ui, "Your maps");
                    for row in customs.chunks(MISSION_COLUMNS) {
                        Self::card_row(ui, row.len(), |ui, i| self.custom_card(ui, &row[i], actions));
                    }
                }
                ui.add_space(8.0);
                self.setup_card(ui, actions);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let width = 3.0 * 140.0;
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    let controls = ui.add(
                        CarvedButton::new("Controls\u{2026}").icon(UiAssets::menu_icon(ui.ctx(), "behavior"), 22.0),
                    );
                    probe::record(ui.ctx(), "button:Controls", controls.rect);
                    if controls.clicked() {
                        actions.push(UiAction::OpenControls);
                    }
                    let editor = ui.add(
                        CarvedButton::new("Map editor\u{2026}").icon(UiAssets::menu_icon(ui.ctx(), "strategy"), 22.0),
                    );
                    probe::record(ui.ctx(), "button:Map editor", editor.rect);
                    if editor.clicked() {
                        actions.push(UiAction::OpenMapEditor { map_id: None });
                    }
                    let admin = ui
                        .add(CarvedButton::new("Admin\u{2026}").icon(UiAssets::menu_icon(ui.ctx(), "behavior"), 22.0));
                    probe::record(ui.ctx(), "button:Admin", admin.rect);
                    if admin.clicked() {
                        actions.push(UiAction::OpenAdmin);
                    }
                });
                ui.add_space(16.0);
            });
        });
    }

    /// The raid setup on a walnut card: who plays with which keys, and which screens.
    fn setup_card(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        theme::setup_card().show(ui, |ui| {
            ui.set_width(720.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                widgets::text(ui, "Players", theme::bold(11.0), BRASS);
                let two = self.adventure.two_player;
                if let Some(choice) = widgets::segments(ui, &["One player", "Two players \u{b7} one keyboard"], usize::from(two)) {
                    let on = choice == 1;
                    if on != two {
                        self.adventure.two_player = on;
                        actions.push(UiAction::SetTwoPlayer(on));
                    }
                }
                if self.adventure.two_player {
                    for (badge, keys) in coop_keys() {
                        widgets::key_row(ui, Some(badge), &keys);
                    }
                    widgets::wrapped(
                        ui,
                        "Both walk and turn and aim where they face \u{b7} either gear key opens both spiders \u{b7} Esc pauses both \u{b7} ` map \u{b7} F8 release",
                        theme::font(9.0),
                        CREAM_SOFT,
                    );
                } else {
                    let (caps, note) = controls_summary(&self.adventure.controls);
                    widgets::key_row(ui, None, &caps);
                    widgets::text(ui, note, theme::font(9.0), CREAM_SOFT);
                }
                let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(rule, egui::CornerRadius::ZERO, rgba(232, 193, 112, 70));
                widgets::text(ui, "Screens", theme::bold(11.0), BRASS);
                let count = self.adventure.screens.len().max(1);
                let found = if count != 1 { format!("{count} screens found") } else { "1 screen found".into() };
                let mut all = self.adventure.all_screens;
                let r = widgets::pill_switch(ui, &mut all, format!("Raids use every screen  ({found}; outposts wait on the others)"), count > 1);
                probe::record(ui.ctx(), "pill:all-screens", r.rect);
                if r.changed() {
                    self.adventure.all_screens = all;
                    actions.push(UiAction::SetAllScreens(all));
                }
                if count >= 2 {
                    let screens = self.adventure.screens.clone();
                    for screen in screens.iter().filter(|s| !s.primary) {
                        let note = match screen.state.as_str() {
                            "asleep" => " - asleep, skipped",
                            "off" => " - off, skipped",
                            _ => "",
                        };
                        let mut on = !self.adventure.disabled_screens.contains(&screen.name);
                        let label = format!("Use screen {} ({}\u{d7}{}) in missions{note}", screen.number, screen.width, screen.height);
                        let r = widgets::pill_switch(ui, &mut on, label, screen.state == "active").on_hover_text(
                            "Switch off a screen that shows another computer; asleep or off screens are skipped by themselves.",
                        );
                        probe::record(ui.ctx(), format!("pill:screen{}", screen.number), r.rect);
                        if r.changed() {
                            self.adventure.disabled_screens.retain(|n| n != &screen.name);
                            if !on {
                                self.adventure.disabled_screens.push(screen.name.clone());
                            }
                            actions.push(UiAction::SetScreen { name: screen.name.clone(), on });
                        }
                    }
                }
            });
        });
    }

    fn strategy_page(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("strategy-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                self.art(ui, "strategy", 520.0);
                ui.add_space(14.0);
                widgets::carved_card(ui, 620.0, 30.0, |ui| {
                    for line in [
                        "Build a base, direct squads and take rival territory.",
                        "Colony command is being developed. Coming later.",
                    ] {
                        widgets::wrapped(ui, line, theme::font(10.0), CREAM_SOFT);
                    }
                });
            });
        });
    }
}

/// A standalone mode menu window (the settings window embeds one).
pub fn shell_size_hint() -> Vec2 {
    vec2(960.0, 860.0)
}
