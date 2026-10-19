//! Port of app/inspector_ui.py: the window a right-click on a spider opens.
//!
//! A header with a live portrait, name, team chip, level badge and XP; an
//! Overview of painted vitals and stat tiles beside the team and its
//! relationships; the same skill tree, anatomy doll and bag as the Adventure
//! character window, wired to this living spider.
//!
//! The overlay refreshes the window with [`CreatureInspector::set_view`]
//! (the Qt window polled every 400 ms); every change is applied to the view
//! at once and reported as a [`UiAction`] for `CreatureManager`.

use egui::{Color32, RichText, Sense, Ui, Vec2, vec2};

use crate::UiAction;
use crate::model::{ArmourIconFn, MAX_LEVEL, Progression, ProgressionCatalog, ThumbnailFn, xp_to_next_level};
use crate::probe;
use crate::stat_text::{self, effect_lines, short_effect};
use crate::teams::{self, TeamProfiles};
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton, VitalBar};
use crate::windows::armour::{ArmourEvent, InventoryBag, SpiderDoll};
use crate::windows::character::skill_tree;

pub const PORTRAIT: f32 = 104.0;

/// `RELATION_COLOURS`.
pub fn relation_colour(relation: &str) -> Color32 {
    match relation {
        "friend" => hex("#7ec46a"),
        "foe" => hex("#e0604c"),
        _ => hex("#c9b48a"),
    }
}

/// Another spider on the desktop.
#[derive(Clone, Debug, PartialEq)]
pub struct OtherSpider {
    pub key: String,
    pub name: String,
    pub relation: String,
}

/// One live spider as the inspector shows it (`progression_snapshot` and friends).
///
/// The overlay fills it from the live `dc_creature::Creature` in the
/// manager (dc-app `overlay_ui::live::inspector_view`).
#[derive(Clone, Debug, Default)]
pub struct InspectorView {
    pub key: String,
    pub display_name: String,
    pub model_name: String,
    pub job_name: String,
    pub hp: f32,
    pub max_hp: f32,
    pub energy: f32,
    pub max_energy: f32,
    pub armor: f32,
    pub damage: f32,
    pub progression: Progression,
    pub level_pinned: bool,
    pub health_pinned: bool,
    /// `creature.health_bar_color(fraction)`; None uses the theme's red.
    pub health_colour: Option<Color32>,
    pub others: Vec<OtherSpider>,
    pub team_profiles: TeamProfiles,
    /// False once the spider has left the desktop: the window closes.
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorTab {
    Overview,
    Skills,
    Armour,
}

/// `CreatureInspectorDialog`.
pub struct CreatureInspector {
    pub view: InspectorView,
    pub catalog: ProgressionCatalog,
    pub tab: InspectorTab,
    pub selected: Option<String>,
    pub catalogue_choice: Option<String>,
    pub team_text: String,
    doll: SpiderDoll,
    bag: InventoryBag,
    portrait: Option<ThumbnailFn>,
    icons: Option<ArmourIconFn>,
}

impl CreatureInspector {
    pub fn new(view: InspectorView, catalog: ProgressionCatalog) -> Self {
        Self {
            view,
            catalog,
            tab: InspectorTab::Overview,
            selected: None,
            catalogue_choice: None,
            team_text: String::new(),
            doll: SpiderDoll::new(false),
            bag: InventoryBag::new(5),
            portrait: None,
            icons: None,
        }
    }

    pub fn title(&self) -> String {
        format!("Inspect {}", self.view.display_name)
    }

    /// The live portrait (dc-paint's `portrait(creature)`), keyed by spider.
    pub fn set_portrait(&mut self, portrait: ThumbnailFn) {
        self.portrait = Some(portrait);
    }

    pub fn set_icons(&mut self, icons: ArmourIconFn) {
        self.icons = Some(icons);
    }

    /// `refresh`: the spider as it is now.
    pub fn set_view(&mut self, view: InspectorView) {
        self.view = view;
    }

    fn team_colour(&self) -> Color32 {
        let id = teams::normalize_team_id(&self.view.progression.team_id);
        match self.view.team_profiles.get(&id) {
            Some(p) => Color32::from_rgb(p.color[0], p.color[1], p.color[2]),
            None if id == teams::NEUTRAL => hex("#c9b48a"),
            None => hex("#e8c170"),
        }
    }

    /// `_refresh_team_choices`: every named team plus this spider's own.
    pub fn team_choices(&self) -> Vec<(String, String)> {
        let mut options = vec![("Neutral / solo".to_owned(), teams::NEUTRAL.to_owned())];
        options.extend(self.view.team_profiles.values().map(|p| (p.name.clone(), p.id.clone())));
        let current = teams::normalize_team_id(&self.view.progression.team_id);
        if current != teams::NEUTRAL && !options.iter().any(|(_, id)| *id == current) {
            options.push((teams::team_label(&current, &self.view.team_profiles), current));
        }
        options
    }

    /// `_commit_team`: a chosen or typed name means that team; anything
    /// else becomes a new team id.
    pub fn commit_team(&mut self, text: &str) -> Option<UiAction> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let team = match self.team_choices().into_iter().find(|(label, _)| label == text) {
            Some((_, id)) => id,
            None => teams::normalize_team_id(&text.replace(' ', "_")),
        };
        if team == teams::normalize_team_id(&self.view.progression.team_id) {
            return None;
        }
        self.view.progression.team_id = team.clone();
        Some(UiAction::SetTeam(team))
    }

    /// The item label of the armour tab (`_refresh_item_label`).
    pub fn item_label(&self) -> String {
        match self.selected.as_deref().and_then(|i| self.catalog.item(i)) {
            None => "Pick a piece to see what it gives.".into(),
            Some(item) => {
                let effects: Vec<String> =
                    effect_lines(&item.effects(), &self.catalog).into_iter().map(|(t, _)| short_effect(&t)).collect();
                let body = if effects.is_empty() { item.description.clone() } else { effects.join(", ") };
                format!("{} \u{b7} {}\n{}", item.name, item.slot, body)
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        if !self.view.alive {
            actions.push(UiAction::CloseWindow);
            return actions;
        }
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 18, top: 16, bottom: 14 }).show(ui, |ui| {
                self.header(ui);
                ui.add_space(6.0);
                let tabs = [
                    ("Overview", Some("inspect")),
                    ("Skill tree", Some("skills")),
                    ("Inventory & armour", Some("armor")),
                ];
                let current = match self.tab {
                    InspectorTab::Overview => 0,
                    InspectorTab::Skills => 1,
                    InspectorTab::Armour => 2,
                };
                if let Some(index) = widgets::tab_bar(ui, &tabs, current) {
                    self.tab = [InspectorTab::Overview, InspectorTab::Skills, InspectorTab::Armour][index];
                }
                let footer = 40.0;
                theme::tab_pane().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height((ui.available_height() - footer).max(200.0));
                    egui::ScrollArea::vertical()
                        .id_salt("inspector-tab")
                        .max_height(ui.available_height() - footer)
                        .show(ui, |ui| match self.tab {
                            InspectorTab::Overview => self.overview(ui, &mut actions),
                            InspectorTab::Skills => self.skills(ui, &mut actions),
                            InspectorTab::Armour => self.armour(ui, &mut actions),
                        });
                });
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 34.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let done = ui.add(CarvedButton::new("Done"));
                        probe::record(ui.ctx(), "button:Done", done.rect);
                        if done.clicked() {
                            actions.push(UiAction::CloseWindow);
                        }
                    },
                );
            });
        });
        actions
    }

    fn header(&mut self, ui: &mut Ui) {
        let v = &self.view;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(PORTRAIT + 12.0), Sense::hover());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, format!("Portrait of {}", v.display_name))
            });
            crate::paint::radial_ellipse(
                ui.painter(),
                rect.center(),
                rect.size() / 2.0 - Vec2::splat(2.0),
                rect.center() - vec2(0.0, 6.0),
                rgba(120, 86, 50, 230),
                rgba(30, 18, 8, 240),
            );
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(14),
                egui::Stroke::new(2.0, BRASS_DEEP),
                egui::StrokeKind::Inside,
            );
            if let Some(texture) = self.portrait.as_ref().and_then(|f| f(&v.key, Vec2::splat(PORTRAIT))) {
                ui.painter().image(
                    texture.id(),
                    egui::Rect::from_center_size(rect.center(), Vec2::splat(PORTRAIT)),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            let colour = self.team_colour();
            let team = teams::team_label(&v.progression.team_id, &v.team_profiles);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    widgets::text(ui, &v.display_name, theme::bold(17.0), CREAM);
                    widgets::team_chip(ui, &team, colour);
                });
                widgets::text(ui, format!("{}  \u{b7}  {}", v.model_name, v.job_name), theme::font(9.5), CREAM_SOFT);
                let level = v.progression.level;
                let need = xp_to_next_level(level);
                let capped = level >= MAX_LEVEL;
                let text = if capped {
                    "Level cap reached".to_owned()
                } else {
                    format!("Level {level}  \u{b7}  {} / {need} XP", v.progression.xp)
                };
                widgets::text(ui, text, theme::bold(10.0), CREAM);
                let value = if capped { need } else { v.progression.xp.min(need) };
                widgets::xp_bar(ui, value as f32, need.max(1) as f32, (ui.available_width() - 70.0).max(120.0));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                widgets::level_badge(ui, v.progression.level);
            });
        });
    }

    fn overview(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        let half = (ui.available_width() - 14.0) / 2.0;
        ui.horizontal_top(|ui| {
            // Each column stacks top to bottom (`allocate_ui` alone would inherit
            // the row's left-to-right layout and spread the panel sideways).
            ui.allocate_ui_with_layout(vec2(half, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                theme::sheet_panel().show(ui, |ui| {
                    ui.set_width(half - 30.0);
                    widgets::brass_title(ui, "Vitals", 13.0);
                    let v = &self.view;
                    let mut health = VitalBar::new("Health", HEALTH);
                    health.set_value(v.hp, v.max_hp, v.health_colour);
                    health.show(ui);
                    let mut stamina = VitalBar::new("Stamina", STAMINA);
                    stamina.set_value(v.energy, v.max_energy, None);
                    stamina.show(ui);
                    let slots = self.catalog.slots().len();
                    let tiles = [
                        ("armor", format!("{:.1}", v.armor), "Armour: softens every hit"),
                        ("damage", format!("{:.1}", v.damage), "Damage: each bite"),
                        ("points", v.progression.skill_points.to_string(), "Skill points to spend"),
                        ("worn", format!("{} / {slots}", v.progression.equipped.len()), "Armour pieces worn"),
                    ];
                    egui::Grid::new("stat-tiles").spacing(vec2(10.0, 10.0)).num_columns(2).show(ui, |ui| {
                        for (index, (kind, value, caption)) in tiles.iter().enumerate() {
                            ui.allocate_ui(vec2((half - 60.0) / 2.0, 60.0), |ui| {
                                ui.set_width((half - 60.0) / 2.0);
                                widgets::stat_tile(ui, kind, value, caption);
                            });
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                    widgets::brass_title(ui, "Shown above this spider", 13.0);
                    let r = widgets::check(ui, &mut self.view.level_pinned, "Level and XP beside its name");
                    probe::record(ui.ctx(), "check:level-pin", r.rect);
                    if r.changed() {
                        actions.push(UiAction::SetLevelPin(self.view.level_pinned));
                    }
                    let r = widgets::check(ui, &mut self.view.health_pinned, "Health bar (without the name)").on_hover_text(
                        "Keeps a small health bar over this spider. The name stays hidden unless names are switched on.",
                    );
                    probe::record(ui.ctx(), "check:health-pin", r.rect);
                    if r.changed() {
                        actions.push(UiAction::SetHealthPin(self.view.health_pinned));
                    }
                });
            });
            // Each column stacks top to bottom (`allocate_ui` alone would inherit
            // the row's left-to-right layout and spread the panel sideways).
            ui.allocate_ui_with_layout(vec2(half, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                theme::sheet_panel().show(ui, |ui| {
                    ui.set_width(half - 30.0);
                    widgets::brass_title(ui, "Team", 13.0);
                    let options = self.team_choices();
                    let mut current = teams::normalize_team_id(&self.view.progression.team_id);
                    let r = widgets::combo(ui, "inspector-team", half - 60.0, &mut current, &options).on_hover_text(teams::HOSTILITY_NOTE);
                    probe::record(ui.ctx(), "inspector:team", r.rect);
                    if r.changed()
                        && let Some((label, _)) = options.iter().find(|(_, id)| *id == current)
                    {
                        let label = label.clone();
                        actions.extend(self.commit_team(&label));
                    }
                    // The editable half of the Qt combo: a typed name, committed on Enter.
                    let edit = ui.add(egui::TextEdit::singleline(&mut self.team_text).hint_text("Or type a team name").desired_width(half - 60.0));
                    probe::record(ui.ctx(), "inspector:team-text", edit.rect);
                    if edit.lost_focus() {
                        let text = std::mem::take(&mut self.team_text);
                        actions.extend(self.commit_team(&text));
                    }
                    widgets::wrapped(ui, teams::HOSTILITY_NOTE, theme::font(9.0), CREAM_SOFT);
                    widgets::brass_title(ui, "How it feels about the others", 13.0);
                    if self.view.others.is_empty() {
                        widgets::text(ui, "No other spiders on the desktop.", theme::font(9.0), CREAM_SOFT);
                    }
                    let options: Vec<(String, String)> = teams::RELATIONS.iter().map(|r| ((*r).to_owned(), (*r).to_owned())).collect();
                    let mut changes = Vec::new();
                    for other in &mut self.view.others {
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                            ui.painter().circle_filled(dot.center(), 5.0, relation_colour(&other.relation));
                            ui.add_sized(vec2(half - 200.0, 18.0), egui::Label::new(RichText::new(&other.name).font(theme::bold(9.5)).color(CREAM)));
                            let mut relation = other.relation.clone();
                            let r = widgets::combo(ui, ("relation", &other.key), 110.0, &mut relation, &options);
                            probe::record(ui.ctx(), format!("relation:{}", other.key), r.rect);
                            if r.changed() {
                                other.relation = relation.clone();
                                changes.push(UiAction::SetRelation { other: other.key.clone(), relation });
                            }
                        });
                    }
                    actions.extend(changes);
                });
            });
        });
    }

    fn skills(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        theme::sheet_panel().show(ui, |ui| {
            widgets::brass_title(ui, "Skill tree", 13.0);
            let points = self.view.progression.skill_points;
            widgets::text(
                ui,
                format!("{points} skill point{} to spend \u{b7} one per level", if points != 1 { "s" } else { "" }),
                theme::bold(10.0),
                CREAM,
            );
            ui.vertical_centered(|ui| {
                if let Some(id) = skill_tree(ui, "inspector", &self.view.progression, &self.catalog) {
                    let state = &mut self.view.progression;
                    if state.can_unlock(&self.catalog, &id) {
                        state.skill_points -= self.catalog.node(&id).map(|n| n.cost).unwrap_or(1);
                        state.unlocked_abilities.push(id.clone());
                        actions.push(UiAction::UnlockSkill { who: self.view.key.clone(), ability_id: id });
                    }
                }
            });
        });
    }

    fn armour(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        theme::sheet_panel().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let mut events = Vec::new();
                ui.vertical(|ui| {
                    widgets::brass_title(ui, "Armour", 13.0);
                    widgets::wrapped(
                        ui,
                        "Drag a piece from the bag onto the spider. Drag it back, or double-click, to take it off.",
                        theme::font(9.0),
                        CREAM_SOFT,
                    );
                    events.extend(self.doll.show(ui, &self.view.progression, &self.catalog, self.icons.as_ref()));
                });
                ui.vertical(|ui| {
                    widgets::brass_title(ui, "Bag", 13.0);
                    let spares = Default::default();
                    events.extend(self.bag.show(
                        ui,
                        "inspector",
                        &self.view.progression,
                        &self.catalog,
                        None,
                        &spares,
                        self.icons.as_ref(),
                        440.0,
                    ));
                    ui.set_max_width(self.bag.width());
                    widgets::wrapped(ui, self.item_label(), theme::font(9.0), CREAM_SOFT);
                    ui.horizontal(|ui| {
                        let owned = &self.view.progression.inventory;
                        let options: Vec<(String, Option<String>)> = self
                            .catalog
                            .armour
                            .iter()
                            .filter(|i| !owned.contains(&i.id))
                            .map(|i| (format!("{}  ({}, {})", i.name, i.tier, i.slot), Some(i.id.clone())))
                            .collect();
                        if self
                            .catalogue_choice
                            .as_ref()
                            .is_none_or(|c| !options.iter().any(|(_, id)| id.as_ref() == Some(c)))
                        {
                            self.catalogue_choice = options.first().and_then(|(_, id)| id.clone());
                        }
                        ui.add_enabled_ui(!options.is_empty(), |ui| {
                            widgets::combo(
                                ui,
                                "catalogue",
                                self.bag.width() - 130.0,
                                &mut self.catalogue_choice,
                                &options,
                            );
                        });
                        let add = ui.add(CarvedButton::new("Add to bag").enabled(!options.is_empty()));
                        probe::record(ui.ctx(), "button:Add to bag", add.rect);
                        if add.clicked()
                            && let Some(id) = self.catalogue_choice.clone()
                            && !self.view.progression.inventory.contains(&id)
                        {
                            self.view.progression.inventory.push(id.clone());
                            actions.push(UiAction::AddInventoryItem(id));
                        }
                    });
                });
                for event in events {
                    match event {
                        ArmourEvent::Equip(id) => {
                            if let Some(item) = self.catalog.item(&id)
                                && self.view.progression.inventory.contains(&id)
                            {
                                self.view.progression.equipped.insert(item.slot.clone(), id.clone());
                                self.selected = Some(id.clone());
                                actions.push(UiAction::Equip { who: self.view.key.clone(), item_id: id });
                            }
                        }
                        ArmourEvent::Unequip(slot) => {
                            if self.view.progression.equipped.remove(&slot).is_some() {
                                actions.push(UiAction::Unequip { who: self.view.key.clone(), slot });
                            }
                        }
                        ArmourEvent::Picked(id) => self.selected = self.catalog.item(&id).map(|i| i.id.clone()),
                    }
                }
            });
        });
        let _ = stat_text::DIM;
    }
}
