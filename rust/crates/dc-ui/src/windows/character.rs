//! Port of app/character_ui.py: the Adventure hero's character window --
//! name, skill tree and armour -- and the two-player `PartyDialog`.
//!
//! The skill tree is a carved column per branch (Body, Fangs, Legs, Silk)
//! under its crest, each skill a medallion with its own picture, linked from
//! its prerequisites. Armour is worn on the anatomy doll with the bag below
//! it. Every change is applied to the window's [`PartyView`] at once (so the
//! window stays truthful) and reported as a [`UiAction`] for the app to save.

use std::collections::BTreeMap;

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::UiAction;
use crate::art::{self, BRANCHES};
use crate::model::{
    ArmourIconFn, HERO, MAX_ITEM_LEVEL, MAX_LEVEL, PLAYER_TWO, PartyView, Progression, ProgressionCatalog, SkillNode,
    clean_name, xp_to_next_level,
};
use crate::paint;
use crate::probe;
use crate::stat_text::{self, effect_lines, short_effect};
use crate::theme::{self, *};
use crate::widgets::{self, ButtonLook, CarvedButton, NodeLook};
use crate::windows::armour::{ArmourEvent, InventoryBag, SpiderDoll};

pub const NODE_W: f32 = 196.0;
pub const NODE_H: f32 = 68.0;
pub const COL_GAP: f32 = 22.0;
pub const ROW_GAP: f32 = 38.0;
pub const HEADER_H: f32 = 52.0;
pub const EDGE: f32 = 10.0;
pub const ICON: f32 = 46.0;

/// `TREE_LAYOUT`: (column, row) of each skill.
pub fn tree_layout(id: &str) -> Option<(usize, usize)> {
    Some(match id {
        "vitality" => (0, 0),
        "carapace_harden" => (0, 1),
        "power_strike" => (1, 0),
        "apex_predator" => (1, 2),
        "quick_step" => (2, 0),
        "long_stride" => (2, 1),
        "silk_sense" => (3, 0),
        "web_crafter" => (3, 1),
        "silk_tracking" => (3, 2),
        _ => return None,
    })
}

/// `tree_positions`: (x, y) of every node; a skill the layout does not name
/// goes in a spare column rather than vanishing.
pub fn tree_positions(tree: &[SkillNode]) -> BTreeMap<String, (f32, f32)> {
    let spare = 4;
    tree.iter()
        .enumerate()
        .map(|(index, node)| {
            let (col, row) = tree_layout(&node.id).unwrap_or((spare + index / 3, index % 3));
            (node.id.clone(), (EDGE + col as f32 * (NODE_W + COL_GAP), HEADER_H + row as f32 * (NODE_H + ROW_GAP)))
        })
        .collect()
}

/// `SkillTree`: draw the tree for `state`; returns the skill clicked.
pub fn skill_tree(ui: &mut Ui, id_salt: &str, state: &Progression, catalog: &ProgressionCatalog) -> Option<String> {
    let positions = tree_positions(&catalog.tree);
    let width = positions.values().map(|p| p.0).fold(0.0, f32::max) + NODE_W + 8.0 + EDGE;
    let height = positions.values().map(|p| p.1).fold(0.0, f32::max) + NODE_H + 14.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    probe::record(ui.ctx(), format!("tree:{id_salt}"), rect);
    let o = rect.min;
    let painter = ui.painter().clone();
    // A carved, tinted column per branch, its crest and name on top.
    for (col, (branch, title)) in BRANCHES.iter().enumerate() {
        let x = EDGE + col as f32 * (NODE_W + COL_GAP) + 4.0;
        let column = Rect::from_min_size(o + vec2(x - 6.0, 2.0), vec2(NODE_W + 12.0, height - 4.0));
        let tint = art::branch_colour(branch);
        paint::vertical_gradient(&painter, column, 12.0, theme::with_alpha(tint, 70), theme::with_alpha(tint, 8));
        painter.rect_stroke(
            column,
            CornerRadius::same(12),
            Stroke::new(1.2, rgba(0, 0, 0, 110)),
            egui::StrokeKind::Inside,
        );
        art::branch_crest(&painter, Rect::from_min_size(o + vec2(x + 6.0, 9.0), Vec2::splat(34.0)), branch);
        painter.text(o + vec2(x + 47.0, 27.0), Align2::LEFT_CENTER, *title, theme::bold(11.0), rgba(0, 0, 0, 160));
        painter.text(o + vec2(x + 46.0, 26.0), Align2::LEFT_CENTER, *title, theme::bold(11.0), BRASS);
    }
    // Carved links from each prerequisite.
    for node in &catalog.tree {
        for required in &node.prerequisites {
            let (Some(a), Some(b)) = (positions.get(required), positions.get(&node.id)) else { continue };
            let start = o + vec2(a.0 + 4.0 + NODE_W / 2.0, a.1 + 4.0 + NODE_H);
            let end = o + vec2(b.0 + 4.0 + NODE_W / 2.0, b.1 + 4.0);
            let mid = (start.y + end.y) / 2.0;
            let points = paint::cubic(start, pos2(start.x, mid), pos2(end.x, mid), end, 24);
            let lit = state.unlocked_abilities.contains(required);
            paint::polyline(&painter, points.clone(), Stroke::new(7.0, rgba(20, 12, 6, 200)));
            paint::polyline(&painter, points, Stroke::new(3.0, if lit { BRASS } else { INK_SOFT }));
        }
    }
    let mut clicked = None;
    for node in &catalog.tree {
        let (x, y) = positions[&node.id];
        let node_rect = Rect::from_min_size(o + vec2(x + 4.0, y + 4.0), vec2(NODE_W, NODE_H));
        let (look, note) = state.node_state(catalog, node);
        let id = ui.make_persistent_id(("skill", id_salt, &node.id));
        let response = ui.interact(node_rect, id, Sense::click());
        let label = format!("{}: {note}", node.name);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, &label));
        probe::record(ui.ctx(), format!("skill:{id_salt}:{}", node.id), node_rect);
        // The node itself, as `QPushButton#skillNode[state=...]`.
        let (top, bottom, text, border, dashed) = match look {
            NodeLook::Unlocked => (BRASS, BRASS_DEEP, WALNUT_DEEP, WALNUT_DEEP, false),
            NodeLook::Available => {
                let fill = if response.hovered() { rgb(0x5a, 0x38, 0x20) } else { WALNUT };
                (fill, fill, CREAM, BRASS, false)
            }
            NodeLook::Locked => {
                (rgba(34, 20, 10, 215), rgba(34, 20, 10, 215), rgb(0xb8, 0xa0, 0x80), rgb(0x7a, 0x64, 0x48), true)
            }
        };
        paint::vertical_gradient(&painter, node_rect, 12.0, top, bottom);
        if dashed {
            paint::dashed_rounded_rect(&painter, node_rect.shrink(1.0), 12.0, Stroke::new(2.0, border));
        } else {
            painter.rect_stroke(node_rect, CornerRadius::same(12), Stroke::new(2.0, border), egui::StrokeKind::Inside);
        }
        let icon = Rect::from_min_size(node_rect.min + vec2(8.0, (NODE_H - ICON) / 2.0), Vec2::splat(ICON));
        let art_look = match look {
            NodeLook::Unlocked => art::Look::Learned,
            NodeLook::Available => art::Look::Available,
            NodeLook::Locked => art::Look::Locked,
        };
        art::skill_icon(&painter, icon, &node.id, art_look);
        let weight = if look == NodeLook::Locked { theme::font(9.0) } else { theme::bold(9.0) };
        let galley = painter.layout(format!("{}\n{note}", node.name), weight, text, NODE_W - ICON - 24.0);
        painter.galley(pos2(icon.right() + 8.0, node_rect.center().y - galley.size().y / 2.0), galley, text);
        let response = widgets::rich_tooltip(response, stat_text::skill_tooltip(node, Some(state), catalog));
        if response.clicked() {
            clicked = Some(node.id.clone());
        }
    }
    clicked
}

/// The tabs of a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetTab {
    Armour,
    Skills,
}

/// `CharacterDialog`'s contents for one spider of the party.
pub struct CharacterSheet {
    pub who: String,
    pub fixed_who: bool,
    pub tab: SheetTab,
    pub selected: Option<String>,
    name_edit: String,
    name_focused: bool,
    doll: SpiderDoll,
    bag: InventoryBag,
    pub show_done: bool,
}

impl CharacterSheet {
    pub fn new(who: &str, fixed_who: bool) -> Self {
        Self {
            who: who.to_owned(),
            fixed_who,
            tab: SheetTab::Armour,
            selected: None,
            name_edit: String::new(),
            name_focused: false,
            doll: SpiderDoll::new(fixed_who),
            bag: InventoryBag::new(if fixed_who { 4 } else { 5 }),
            show_done: !fixed_who,
        }
    }

    /// `choose_spider`.
    pub fn choose_spider(&mut self, party: &PartyView, who: &str) {
        if party.member(who).is_some() {
            self.who = who.to_owned();
            self.name_focused = false;
        }
    }

    // -- changes (the armoury rules, applied to the view at once) ---------------

    /// `_unlock`.
    pub fn unlock(
        &mut self,
        party: &mut PartyView,
        catalog: &ProgressionCatalog,
        ability_id: &str,
    ) -> Option<UiAction> {
        let member = party.member_mut(&self.who)?;
        if !member.progression.can_unlock(catalog, ability_id) {
            return None;
        }
        let cost = catalog.node(ability_id).map(|n| n.cost).unwrap_or(1);
        member.progression.skill_points -= cost;
        member.progression.unlocked_abilities.push(ability_id.to_owned());
        Some(UiAction::UnlockSkill { who: self.who.clone(), ability_id: ability_id.to_owned() })
    }

    /// `wear`: on this spider; whoever wore it takes it off.
    pub fn equip(&mut self, party: &mut PartyView, catalog: &ProgressionCatalog, item_id: &str) -> Option<UiAction> {
        let item = catalog.item(item_id)?;
        if !party.owned.iter().any(|o| o == item_id) || party.member(&self.who).is_none() {
            return None;
        }
        for member in party.members.iter_mut() {
            if member.who != self.who {
                member.progression.equipped.retain(|_, i| i != item_id);
            }
        }
        let member = party.member_mut(&self.who)?;
        member.progression.equipped.insert(item.slot.clone(), item_id.to_owned());
        self.selected = Some(item_id.to_owned());
        Some(UiAction::Equip { who: self.who.clone(), item_id: item_id.to_owned() })
    }

    /// `take_off`.
    pub fn unequip(&mut self, party: &mut PartyView, slot: &str) -> Option<UiAction> {
        let member = party.member_mut(&self.who)?;
        member.progression.equipped.remove(slot)?;
        Some(UiAction::Unequip { who: self.who.clone(), slot: slot.to_owned() })
    }

    /// `upgrade`: stack one spare onto the piece.
    pub fn upgrade(&mut self, party: &mut PartyView) -> Option<UiAction> {
        let item = self.selected.clone()?;
        if !party.can_upgrade(&item) {
            return None;
        }
        let spares = party.spares_of(&item) - 1;
        if spares == 0 {
            party.spares.remove(&item);
        } else {
            party.spares.insert(item.clone(), spares);
        }
        party.levels.insert(item.clone(), party.level_of(&item) + 1);
        Some(UiAction::UpgradeItem { item_id: item })
    }

    /// `sell_spares`.
    pub fn sell(&mut self, party: &mut PartyView, catalog: &ProgressionCatalog, count: u32) -> Option<UiAction> {
        let item = self.selected.clone()?;
        let count = count.min(party.spares_of(&item));
        if count == 0 {
            return None;
        }
        let left = party.spares_of(&item) - count;
        if left == 0 {
            party.spares.remove(&item);
        } else {
            party.spares.insert(item.clone(), left);
        }
        party.amber += count as i64 * party.sell_price(catalog, &item);
        Some(UiAction::SellSpares { item_id: item, count })
    }

    /// `_rename`.
    pub fn rename(&mut self, party: &mut PartyView, text: &str) -> Option<UiAction> {
        let name = clean_name(text);
        self.name_edit = name.clone();
        let member = party.member_mut(&self.who)?;
        if member.name == name {
            return None;
        }
        member.name = name.clone();
        Some(UiAction::RenameSpider { who: self.who.clone(), name })
    }

    /// `_bonus_text`: what skills, armour and complete sets add.
    pub fn bonus_lines(party: &PartyView, who: &str, catalog: &ProgressionCatalog) -> Vec<(String, Color32)> {
        let dress = party.dress(who);
        let mut totals: Vec<(String, f64)> = Vec::new();
        let mut add = |k: &str, v: f64| match totals.iter_mut().find(|(key, _)| key == k) {
            Some(entry) => entry.1 += v,
            None => totals.push((k.to_owned(), v)),
        };
        for ability in &dress.unlocked_abilities {
            if let Some(node) = catalog.node(ability) {
                for (k, v) in &node.effects {
                    add(k, *v);
                }
            }
        }
        for item in dress.equipped_items(catalog) {
            for (k, v) in item.effects() {
                add(&k, v);
            }
        }
        for (k, v) in dress.set_bonus_effects(catalog) {
            add(&k, v);
        }
        let lines: Vec<String> = effect_lines(&totals, catalog).into_iter().map(|(t, _)| short_effect(&t)).collect();
        let mut out = vec![if lines.is_empty() {
            ("No bonuses yet. Learn skills and wear armour to grow stronger.".to_owned(), CREAM_SOFT)
        } else {
            (format!("Bonuses  {}", lines.join("  \u{b7}  ")), CREAM_SOFT)
        }];
        // Only the sets being worn.
        for set in &catalog.sets {
            if dress.equipped.values().any(|i| catalog.item(i).is_some_and(|it| it.set_id == set.id)) {
                let (text, colour, _) = stat_text::set_line(&set.id, Some(&dress), catalog, CREAM_SOFT, BRASS);
                out.push((text, colour));
            }
        }
        out
    }

    /// Draw the sheet against the shared `party`.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        party: &mut PartyView,
        catalog: &ProgressionCatalog,
        icons: Option<&ArmourIconFn>,
    ) -> Vec<UiAction> {
        let mut actions = Vec::new();
        if party.member(&self.who).is_none() {
            self.who = HERO.into();
        }
        let key = self.who.clone();
        let state = party.member(&self.who).map(|m| m.progression.clone()).unwrap_or_default();
        // Header: level badge, name, XP.
        ui.horizontal(|ui| {
            widgets::level_badge(ui, state.level);
            ui.vertical(|ui| {
                if !self.name_focused {
                    self.name_edit = party.name_of(&self.who);
                }
                let edit = egui::TextEdit::singleline(&mut self.name_edit)
                    .char_limit(crate::model::MAX_NAME_LENGTH)
                    .hint_text("Name your spider")
                    .font(theme::bold(16.0))
                    .text_color(CREAM)
                    .background_color(WALNUT_DEEP)
                    .desired_width(ui.available_width().min(420.0));
                let r = ui.add(edit);
                probe::record(ui.ctx(), format!("name:{key}"), r.rect);
                self.name_focused = r.has_focus();
                if r.lost_focus() {
                    let text = self.name_edit.clone();
                    actions.extend(self.rename(party, &text));
                }
                let need = xp_to_next_level(state.level);
                let text = if state.level >= MAX_LEVEL {
                    "Max level".to_owned()
                } else {
                    format!("Level {}  \u{b7}  {} / {need} XP", state.level, state.xp)
                };
                // The number is on the label: on the bar it was cream on pale gold.
                widgets::text(ui, text, theme::bold(10.0), CREAM);
                widgets::xp_bar(ui, state.xp.min(need) as f32, need as f32, ui.available_width().min(420.0));
            });
        });
        let tabs = [("Armour", Some("armor")), ("Skills", Some("skills"))];
        let selected = if self.tab == SheetTab::Armour { 0 } else { 1 };
        ui.push_id(("tabs", &key), |ui| {
            if let Some(index) = widgets::tab_bar(ui, &tabs, selected) {
                self.tab = if index == 0 { SheetTab::Armour } else { SheetTab::Skills };
            }
        });
        theme::tab_pane().show(ui, |ui| {
            ui.set_width(ui.available_width());
            match self.tab {
                SheetTab::Skills => {
                    theme::sheet_panel().show(ui, |ui| {
                        widgets::brass_title(ui, "Skill tree", 13.0);
                        let points = state.skill_points;
                        let text = if points > 0 {
                            format!(
                                "{points} skill point{} to spend \u{b7} one per level",
                                if points != 1 { "s" } else { "" }
                            )
                        } else {
                            "No points to spend \u{b7} you earn one each level".into()
                        };
                        widgets::text(ui, text, theme::bold(10.0), CREAM);
                        egui::ScrollArea::horizontal().id_salt(("tree-scroll", &key)).show(ui, |ui| {
                            if let Some(id) = skill_tree(ui, &key, &state, catalog) {
                                actions.extend(self.unlock(party, catalog, &id));
                            }
                        });
                    });
                }
                SheetTab::Armour => {
                    let scroll_height = if self.fixed_who { f32::INFINITY } else { 760.0 };
                    egui::ScrollArea::vertical().id_salt(("armour-scroll", &key)).max_height(scroll_height).show(
                        ui,
                        |ui| {
                            theme::sheet_panel()
                                .show(ui, |ui| self.armour_tab(ui, party, catalog, icons, &mut actions));
                        },
                    );
                }
            }
        });
        if self.show_done {
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
        }
        actions
    }

    fn armour_tab(
        &mut self,
        ui: &mut Ui,
        party: &mut PartyView,
        catalog: &ProgressionCatalog,
        icons: Option<&ArmourIconFn>,
        actions: &mut Vec<UiAction>,
    ) {
        let key = self.who.clone();
        widgets::brass_title(ui, "Armour", 13.0);
        // One button per spider that can wear armour.
        ui.horizontal(|ui| {
            let members: Vec<(String, String)> =
                party.members.iter().map(|m| (m.who.clone(), m.name.clone())).collect();
            for (who, name) in members {
                if self.fixed_who && who != self.who {
                    continue;
                }
                if who == PLAYER_TWO && !party.two_player && self.who != PLAYER_TWO && !self.fixed_who {
                    continue;
                }
                let r = ui.add(
                    CarvedButton::new(name)
                        .look(ButtonLook::Toggle { checked: who == self.who })
                        .padding(vec2(10.0, 3.0)),
                );
                probe::record(ui.ctx(), format!("who:{who}"), r.rect);
                if r.clicked() {
                    self.choose_spider(party, &who);
                }
            }
        });
        widgets::wrapped(
            ui,
            "Drag a piece from the bag onto the spider. Drag it back, or double-click, to take it off.",
            theme::font(9.0),
            CREAM_SOFT,
        );
        let dress = party.dress(&self.who);
        let mut events = Vec::new();
        ui.vertical_centered(|ui| {
            ui.push_id(("doll", &key), |ui| events.extend(self.doll.show(ui, &dress, catalog, icons)));
        });
        ui.horizontal(|ui| {
            widgets::brass_title(ui, if self.fixed_who { "Shared bag" } else { "Bag" }, 13.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    CarvedButton::new("Shop").enabled(false).tooltip("Coming later: spend amber on armour and more."),
                );
                ui.label(
                    RichText::new(format!("{} amber", party.amber))
                        .font(theme::bold(10.0))
                        .color(rgb(0xf5, 0xb0, 0x41)),
                );
            });
        });
        let worn: Vec<String> = party.worn_by().keys().cloned().collect();
        let spares = party.spares.clone();
        events.extend(self.bag.show(ui, &key, &dress, catalog, Some(&worn), &spares, icons, 270.0));
        for event in events {
            match event {
                ArmourEvent::Equip(item) => actions.extend(self.equip(party, catalog, &item)),
                ArmourEvent::Unequip(slot) => actions.extend(self.unequip(party, &slot)),
                ArmourEvent::Picked(item) => self.selected = catalog.item(&item).map(|i| i.id.clone()),
            }
        }
        self.item_panel(ui, party, catalog, actions);
        for (text, colour) in Self::bonus_lines(party, &self.who, catalog) {
            widgets::wrapped(ui, text, theme::font(9.0), colour);
        }
    }

    /// `_refresh_item_panel`: the selected piece, its level and spares.
    fn item_panel(
        &mut self,
        ui: &mut Ui,
        party: &mut PartyView,
        catalog: &ProgressionCatalog,
        actions: &mut Vec<UiAction>,
    ) {
        let Some(item) = self.selected.as_deref().and_then(|i| catalog.item(i)).cloned() else { return };
        if !party.owned.contains(&item.id) {
            return;
        }
        let level = party.level_of(&item.id);
        let spares = party.spares_of(&item.id);
        let worn =
            party.worn_by().get(&item.id).map(|w| format!(" \u{b7} worn by {}", party.name_of(w))).unwrap_or_default();
        theme::item_panel().show(ui, |ui| {
            ui.set_width(self.bag.width() - 20.0);
            ui.label(RichText::new(&item.name).font(theme::bold(10.0)).color(stat_text::tier_color(&item.tier)));
            let line = format!(
                "Level {level}/{MAX_ITEM_LEVEL} \u{b7} {spares} spare{}{worn}",
                if spares != 1 { "s" } else { "" }
            );
            widgets::wrapped(ui, line, theme::font(9.0), CREAM_SOFT);
            ui.horizontal(|ui| {
                let tip = if level < MAX_ITEM_LEVEL {
                    "Stack one spare onto it: +1 level, +20% of its stats."
                } else {
                    "Already at the highest level."
                };
                let up = ui.add(CarvedButton::new("Upgrade").enabled(party.can_upgrade(&item.id)).tooltip(tip));
                probe::record(ui.ctx(), format!("item:{}:upgrade", self.who), up.rect);
                if up.clicked() {
                    actions.extend(self.upgrade(party));
                }
                let price = party.sell_price(catalog, &item.id);
                let sell = ui.add(CarvedButton::new(format!("Sell spare (+{price})")).enabled(spares > 0));
                probe::record(ui.ctx(), format!("item:{}:sell", self.who), sell.rect);
                if sell.clicked() {
                    actions.extend(self.sell(party, catalog, 1));
                }
                let all = ui.add(CarvedButton::new("Sell all spares").enabled(spares > 1));
                if all.clicked() {
                    actions.extend(self.sell(party, catalog, 1_000_000));
                }
            });
        });
    }
}

/// `CharacterDialog`: one spider's sheet in its own window.
pub struct CharacterDialog {
    pub party: PartyView,
    pub catalog: ProgressionCatalog,
    pub sheet: CharacterSheet,
    icons: Option<ArmourIconFn>,
}

impl CharacterDialog {
    pub fn new(party: PartyView, catalog: ProgressionCatalog, who: &str) -> Self {
        Self { party, catalog, sheet: CharacterSheet::new(who, false), icons: None }
    }

    pub fn title() -> &'static str {
        "Your Adventure spider"
    }

    pub fn set_icons(&mut self, icons: ArmourIconFn) {
        self.icons = Some(icons);
    }

    /// New profile data from the app.
    pub fn set_party(&mut self, party: PartyView) {
        self.party = party;
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 18, right: 18, top: 16, bottom: 16 }).show(ui, |ui| {
                actions = self.sheet.show(ui, &mut self.party, &self.catalog, self.icons.as_ref());
            });
        });
        actions
    }
}

/// `PartyDialog`: Player 1's spider on the left, Player 2's on the right,
/// each with the shared bag below its doll.
pub struct PartyDialog {
    pub party: PartyView,
    pub catalog: ProgressionCatalog,
    pub sheets: [CharacterSheet; 2],
    icons: Option<ArmourIconFn>,
}

impl PartyDialog {
    pub fn new(party: PartyView, catalog: ProgressionCatalog) -> Self {
        let mut sheets = [CharacterSheet::new(HERO, true), CharacterSheet::new(PLAYER_TWO, true)];
        for sheet in &mut sheets {
            sheet.show_done = false;
        }
        Self { party, catalog, sheets, icons: None }
    }

    pub fn title() -> &'static str {
        "Players 1 & 2 \u{b7} Character, skills & shared bag"
    }

    pub fn set_icons(&mut self, icons: ArmourIconFn) {
        self.icons = Some(icons);
    }

    pub fn set_party(&mut self, party: PartyView) {
        self.party = party;
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 12 }).show(ui, |ui| {
                let width = (ui.available_width() - 12.0) / 2.0;
                let height = ui.available_height() - 44.0;
                ui.horizontal_top(|ui| {
                    for (index, keys) in ["WASD \u{b7} K", "Arrows \u{b7} P"].iter().enumerate() {
                        ui.allocate_ui(vec2(width, height), |ui| {
                            ui.set_width(width);
                            ui.vertical(|ui| {
                                widgets::brass_title(ui, &format!("Player {}  \u{b7}  {keys}", index + 1), 13.0);
                                egui::ScrollArea::vertical()
                                    .id_salt(("party-sheet", index))
                                    .max_height(height - 30.0)
                                    .show(ui, |ui| {
                                        // One bag: a change on either side shows on both at once.
                                        actions.extend(self.sheets[index].show(
                                            ui,
                                            &mut self.party,
                                            &self.catalog,
                                            self.icons.as_ref(),
                                        ));
                                    });
                            });
                        });
                    }
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
}

/// `show_party_windows`: which window the Character button opens.
// Only one of these is ever open, so the size difference costs nothing.
#[allow(clippy::large_enum_variant)]
pub enum CharacterWindow {
    Single(CharacterDialog),
    Party(PartyDialog),
}

impl CharacterWindow {
    pub fn open(party: PartyView, catalog: ProgressionCatalog, two_player: Option<bool>) -> Self {
        if two_player.unwrap_or(party.two_player) {
            CharacterWindow::Party(PartyDialog::new(party, catalog))
        } else {
            CharacterWindow::Single(CharacterDialog::new(party, catalog, HERO))
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        match self {
            CharacterWindow::Single(d) => d.ui(ctx),
            CharacterWindow::Party(d) => d.ui(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_a_place_in_its_branch_column() {
        let catalog = crate::sample::progression_catalog();
        let positions = tree_positions(&catalog.tree);
        for node in &catalog.tree {
            let (col, _) = tree_layout(&node.id).unwrap();
            let branch = art::skill_branch(&node.id);
            assert_eq!(BRANCHES[col].0, branch, "{}", node.id);
            for req in &node.prerequisites {
                assert!(positions[req].1 < positions[&node.id].1, "{req} above {}", node.id);
            }
        }
    }

    #[test]
    fn sheets_share_armour_but_not_skills() {
        let catalog = crate::sample::progression_catalog();
        let mut party = crate::sample::party_view();
        let mut one = CharacterSheet::new(HERO, true);
        let mut two = CharacterSheet::new(PLAYER_TWO, true);
        // Player two takes the hero's carapace: it leaves the hero.
        assert!(two.equip(&mut party, &catalog, "silk_carapace").is_some());
        assert!(party.member(HERO).unwrap().progression.equipped.is_empty());
        assert_eq!(party.worn_by()["silk_carapace"], PLAYER_TWO);
        // Skills stay each spider's own.
        assert!(one.unlock(&mut party, &catalog, "quick_step").is_some());
        assert!(!party.member(PLAYER_TWO).unwrap().progression.unlocked_abilities.contains(&"quick_step".to_owned()));
        assert_eq!(party.member(HERO).unwrap().progression.skill_points, 1);
    }

    #[test]
    fn upgrading_and_selling_spares() {
        let catalog = crate::sample::progression_catalog();
        let mut party = crate::sample::party_view();
        let mut sheet = CharacterSheet::new(HERO, false);
        sheet.selected = Some("warden_crest".into());
        assert_eq!(sheet.upgrade(&mut party), Some(UiAction::UpgradeItem { item_id: "warden_crest".into() }));
        assert_eq!(party.level_of("warden_crest"), 3);
        assert_eq!(party.spares_of("warden_crest"), 1);
        let amber = party.amber;
        assert!(sheet.sell(&mut party, &catalog, 99).is_some());
        assert_eq!(party.amber, amber + 30);
        assert_eq!(party.spares_of("warden_crest"), 0);
        assert!(sheet.sell(&mut party, &catalog, 1).is_none());
        assert!(sheet.upgrade(&mut party).is_none());
    }

    #[test]
    fn bonuses_name_skills_armour_and_worn_sets() {
        let catalog = crate::sample::progression_catalog();
        let party = crate::sample::party_view();
        let lines = CharacterSheet::bonus_lines(&party, HERO, &catalog);
        assert!(lines[0].0.starts_with("Bonuses  "), "{:?}", lines[0]);
        assert!(lines[0].0.contains("+26 max health"), "{:?}", lines[0]);
        let empty = CharacterSheet::bonus_lines(&party, "scout", &catalog);
        assert_eq!(empty[0].0, "No bonuses yet. Learn skills and wear armour to grow stronger.");
    }
}
