//! Port of app/map_editor.py: make your own Adventure maps.
//!
//! On the left, the map: the main screen above and the second screen below.
//! Drag a building or an enemy to move it (dropping it on the lower screen
//! puts it on the second screen), click to select it. On the right, four
//! tabs: Map, Buildings, Enemies and Boss. The map is the same JSON
//! `custom_maps` writes; its rules (`clean_map`, `problems`, `new_map`) are
//! dc-adventure's. Saving, deleting and opening are [`UiAction`]s; the app
//! answers with [`MapEditor::saved`], [`MapEditor::load`] and
//! [`MapEditor::set_library`].

use dc_adventure::custom_maps;
use egui::{Align2, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::UiAction;
use crate::model::{ARMOR_TIERS, MAX_ITEM_LEVEL, ProgressionCatalog};
use crate::probe;
use crate::theme::{self, *};
use crate::widgets::{self, CarvedButton};

pub const NOMINAL_W: f32 = 1920.0;
pub const NOMINAL_H: f32 = 1080.0;
/// The mission's own placement rule: y = 115 + (height - 360) * fy.
pub const TOP: f32 = 115.0;
pub const SPAN: f32 = NOMINAL_H - 360.0;

/// `ROLE_NAMES`.
pub fn role_name(role: &str) -> &'static str {
    match role {
        "weaver" => "Weaver (shoots silk)",
        "hunter" => "Hunter (pounces)",
        "spitter" => "Spitter (acid)",
        _ => "Guard (holds ground)",
    }
}

fn building_kinds() -> Vec<(&'static str, &'static str)> {
    custom_maps::BUILDING_KINDS.iter().map(|k| (k.id(), custom_maps::building_name(*k))).collect()
}

fn building_name(kind: &str) -> String {
    building_kinds().iter().find(|(k, _)| *k == kind).map(|(_, n)| (*n).to_owned()).unwrap_or_else(|| kind.to_owned())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Building(usize),
    Enemy(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorTab {
    Map,
    Buildings,
    Enemies,
    Boss,
}

/// The two screens inside the canvas rect (`MapCanvas.screens`).
pub fn screens(canvas: Rect) -> (Rect, Rect) {
    let h = (canvas.height() - 64.0) / 2.0;
    let w = (canvas.width() - 24.0).min(h * NOMINAL_W / NOMINAL_H);
    let h = w * NOMINAL_H / NOMINAL_W;
    let x = canvas.left() + (canvas.width() - w) / 2.0;
    (
        Rect::from_min_size(pos2(x, canvas.top() + 24.0), vec2(w, h)),
        Rect::from_min_size(pos2(x, canvas.top() + 24.0 + h + 30.0), vec2(w, h)),
    )
}

/// `to_canvas`.
pub fn to_canvas(canvas: Rect, fx: f32, fy: f32, far: bool) -> Pos2 {
    let (main, second) = screens(canvas);
    let rect = if far { second } else { main };
    let s = rect.width() / NOMINAL_W;
    let x = (NOMINAL_W * fx).clamp(85.0, NOMINAL_W - 85.0);
    let y = (TOP + SPAN * fy).clamp(85.0, NOMINAL_H - 85.0);
    pos2(rect.left() + x * s, rect.top() + y * s)
}

/// `from_canvas`: (fx, fy, far) of a point; below the main screen is far.
pub fn from_canvas(canvas: Rect, point: Pos2) -> (f32, f32, bool) {
    let (main, second) = screens(canvas);
    let far = second.contains(point) || (!main.contains(point) && point.y > main.bottom());
    let rect = if far { second } else { main };
    let s = rect.width() / NOMINAL_W;
    let fx = (point.x - rect.left()) / s / NOMINAL_W;
    let fy = ((point.y - rect.top()) / s - TOP) / SPAN;
    (fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0), far)
}

fn f(v: &Value, k: &str) -> f32 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn round3(x: f32) -> f64 {
    (x as f64 * 1000.0).round() / 1000.0
}

/// `MapEditor`.
pub struct MapEditor {
    pub data: Value,
    pub selected: Option<Pick>,
    pub guard_index: Option<usize>,
    pub tab: EditorTab,
    pub dirty: bool,
    /// (id, title) of the saved maps (`list_maps`).
    pub library: Vec<(String, String)>,
    pub catalog: ProgressionCatalog,
    /// (id, name, boss).
    pub enemy_kinds: Vec<(String, String, bool)>,
    /// (id, name).
    pub companions: Vec<(String, String)>,
    new_kind: String,
    dragging: Option<Pick>,
    confirm_delete: bool,
}

impl MapEditor {
    /// A new map, or `map` (a saved map's JSON).
    pub fn new(map: Option<&Value>, catalog: ProgressionCatalog) -> Self {
        let mut editor = Self {
            data: Value::Null,
            selected: None,
            guard_index: None,
            tab: EditorTab::Map,
            dirty: false,
            library: Vec::new(),
            catalog,
            enemy_kinds: crate::bridge::enemy_kind_list(),
            companions: crate::bridge::companions(),
            new_kind: "food".into(),
            dragging: None,
            confirm_delete: false,
        };
        match map {
            Some(m) => editor.load(m),
            None => editor.load(&custom_maps::new_map("My map").to_value()),
        }
        editor
    }

    pub fn title() -> &'static str {
        "Map editor"
    }

    /// `load`: show a map, cleaned as custom_maps reads it.
    pub fn load(&mut self, raw: &Value) {
        let id = raw.get("id").cloned().unwrap_or(json!(""));
        self.data = custom_maps::clean_map(raw).to_value();
        self.data["id"] = id;
        self.selected = None;
        self.dirty = false;
        self.confirm_delete = false;
        if !self.buildings().is_empty() {
            self.select(Pick::Building(0));
        }
    }

    /// The saved maps for the chooser.
    pub fn set_library(&mut self, maps: Vec<(String, String)>) {
        self.library = maps;
    }

    /// The app saved the map under `id`.
    pub fn saved(&mut self, id: &str) {
        self.data["id"] = json!(id);
        self.dirty = false;
    }

    /// `problems`: what stops this map being played.
    pub fn problems(&self) -> Vec<String> {
        custom_maps::problems(&custom_maps::clean_map(&self.data))
    }

    pub fn buildings(&self) -> &[Value] {
        self.data["buildings"].as_array().map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn enemies(&self) -> &[Value] {
        self.data["enemies"].as_array().map(Vec::as_slice).unwrap_or(&[])
    }

    fn entry_mut(&mut self, pick: Pick) -> Option<&mut Value> {
        match pick {
            Pick::Building(i) => self.data["buildings"].get_mut(i),
            Pick::Enemy(i) => self.data["enemies"].get_mut(i),
        }
    }

    pub fn select(&mut self, pick: Pick) {
        let ok = match pick {
            Pick::Building(i) => i < self.buildings().len(),
            Pick::Enemy(i) => i < self.enemies().len(),
        };
        if !ok {
            self.selected = None;
            return;
        }
        self.selected = Some(pick);
        match pick {
            Pick::Building(i) => {
                self.tab = EditorTab::Buildings;
                self.guard_index = (!self.buildings()[i]["guards"].as_array().is_none_or(Vec::is_empty)).then_some(0);
            }
            Pick::Enemy(_) => self.tab = EditorTab::Enemies,
        }
    }

    /// `add_building`.
    pub fn add_building(&mut self, kind: &str) {
        let guards = if kind == "home" { json!([]) } else { json!([custom_maps::spider_spec("guard").to_value()]) };
        let building = json!({"kind": kind, "name": building_name(kind), "fx": 0.5, "fy": 0.5, "far": false,
                              "owned": kind == "home", "reserves": if kind == "hatchery" { 6 } else { 0 },
                              "supply": 180.0, "guards": guards});
        if let Some(list) = self.data["buildings"].as_array_mut() {
            list.push(building);
        }
        self.dirty = true;
        let last = self.buildings().len() - 1;
        self.select(Pick::Building(last));
    }

    pub fn remove_selected(&mut self) {
        match self.selected {
            Some(Pick::Building(i)) => {
                if let Some(list) = self.data["buildings"].as_array_mut() {
                    list.remove(i);
                }
            }
            Some(Pick::Enemy(i)) => {
                if let Some(list) = self.data["enemies"].as_array_mut() {
                    list.remove(i);
                }
            }
            None => return,
        }
        self.selected = None;
        self.dirty = true;
    }

    /// `add_enemy`.
    pub fn add_enemy(&mut self) {
        let mut spec = custom_maps::spider_spec("hunter").to_value();
        spec["fx"] = json!(0.6);
        spec["fy"] = json!(0.5);
        spec["far"] = json!(false);
        if let Some(list) = self.data["enemies"].as_array_mut() {
            list.push(spec);
        }
        self.dirty = true;
        let last = self.enemies().len() - 1;
        self.select(Pick::Enemy(last));
    }

    /// `add_guard` (at most eight).
    pub fn add_guard(&mut self) {
        let Some(Pick::Building(i)) = self.selected else { return };
        let Some(guards) = self.data["buildings"][i]["guards"].as_array_mut() else { return };
        if guards.len() < 8 {
            guards.push(custom_maps::spider_spec("guard").to_value());
            self.guard_index = Some(guards.len() - 1);
            self.dirty = true;
        }
    }

    pub fn remove_guard(&mut self) {
        let (Some(Pick::Building(i)), Some(g)) = (self.selected, self.guard_index) else { return };
        let Some(guards) = self.data["buildings"][i]["guards"].as_array_mut() else { return };
        if g < guards.len() {
            guards.remove(g);
            self.guard_index = (!guards.is_empty()).then_some(0);
            self.dirty = true;
        }
    }

    /// Move an entry to a canvas point (a drag).
    pub fn move_to(&mut self, pick: Pick, canvas: Rect, point: Pos2) {
        let (fx, fy, far) = from_canvas(canvas, point);
        if let Some(entry) = self.entry_mut(pick) {
            entry["fx"] = json!(round3(fx));
            entry["fy"] = json!(round3(fy));
            entry["far"] = json!(far);
            self.dirty = true;
        }
    }

    fn items(&self, canvas: Rect) -> Vec<(Pick, Pos2)> {
        let mut out = Vec::new();
        for (i, b) in self.buildings().iter().enumerate() {
            out.push((Pick::Building(i), to_canvas(canvas, f(b, "fx"), f(b, "fy"), self::b(b, "far"))));
        }
        for (i, e) in self.enemies().iter().enumerate() {
            out.push((Pick::Enemy(i), to_canvas(canvas, f(e, "fx"), f(e, "fy"), self::b(e, "far"))));
        }
        out
    }

    /// `item_at`: the nearest entry within 26 px.
    pub fn item_at(&self, canvas: Rect, point: Pos2) -> Option<Pick> {
        let mut best = None;
        let mut best_d = 26.0;
        for (pick, at) in self.items(canvas) {
            let d = at.distance(point);
            if d < best_d {
                best = Some(pick);
                best_d = d;
            }
        }
        best
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut actions = Vec::new();
        widgets::wood_board(ui, ui.max_rect(), true);
        theme::with_visuals(ui, theme::dialog_visuals(), |ui| {
            egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                self.top_bar(ui, &mut actions);
                ui.horizontal_top(|ui| {
                    let total = ui.available_width();
                    let left = (total * 0.6).max(560.0);
                    ui.allocate_ui(vec2(left, ui.available_height()), |ui| {
                        ui.vertical(|ui| {
                            self.canvas(ui, vec2(left, (ui.available_height() - 60.0).max(600.0)));
                            widgets::wrapped(
                                ui,
                                "Drag buildings and enemies to move them; drop one on the lower screen to put it on the second screen. Red dots under a building are its guards.",
                                theme::font(9.0),
                                CREAM,
                            );
                            let found = self.problems();
                            let text = if found.is_empty() { "Ready to play.".to_owned() } else { found.join("  ") };
                            widgets::wrapped(ui, text, theme::font(9.0), hex("#ffb08a"));
                        });
                    });
                    ui.vertical(|ui| {
                        ui.set_min_width(360.0);
                        self.tabs(ui);
                    });
                });
            });
        });
        actions
    }

    fn top_bar(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        ui.horizontal(|ui| {
            ui.label("Map");
            let id = s(&self.data, "id").to_owned();
            let mut options: Vec<(String, String)> = self.library.clone().into_iter().map(|(i, t)| (t, i)).collect();
            if !id.starts_with(custom_maps::PREFIX) {
                options.push(("(new, not saved)".into(), String::new()));
            }
            let mut chosen = id.clone();
            if widgets::combo(ui, "map-chooser", 220.0, &mut chosen, &options).changed()
                && !chosen.is_empty()
                && chosen != id
            {
                actions.push(UiAction::OpenMap(chosen));
            }
            let problems = self.problems();
            for (key, text, enabled) in [
                ("new", "New", true),
                ("save", "Save", true),
                ("delete", "Delete", id.starts_with(custom_maps::PREFIX)),
                ("play", "Save and play", problems.is_empty()),
                ("close", "Close", true),
            ] {
                let r = ui.add(CarvedButton::new(text).enabled(enabled));
                probe::record(ui.ctx(), format!("editor:{key}"), r.rect);
                if !r.clicked() {
                    continue;
                }
                match key {
                    "new" => self.load(&custom_maps::new_map("My map").to_value()),
                    "save" => {
                        actions.push(UiAction::SaveMap(custom_maps::clean_map(&self.data).to_value()));
                    }
                    "delete" => self.confirm_delete = true,
                    "play" => {
                        actions.push(UiAction::SaveAndPlayMap(custom_maps::clean_map(&self.data).to_value()));
                        actions.push(UiAction::CloseWindow);
                    }
                    _ => actions.push(UiAction::CloseWindow),
                }
            }
        });
        if self.confirm_delete {
            ui.horizontal(|ui| {
                widgets::text(
                    ui,
                    format!("Delete \u{201c}{}\u{201d}?", s(&self.data, "title")),
                    theme::bold(10.0),
                    CREAM,
                );
                let yes = ui.add(CarvedButton::new("Yes"));
                probe::record(ui.ctx(), "dialog:Yes", yes.rect);
                if yes.clicked() {
                    actions.push(UiAction::DeleteMap(s(&self.data, "id").to_owned()));
                    self.load(&custom_maps::new_map("My map").to_value());
                }
                if ui.add(CarvedButton::new("No")).clicked() {
                    self.confirm_delete = false;
                }
            });
        }
    }

    fn canvas(&mut self, ui: &mut Ui, size: Vec2) {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        probe::record(ui.ctx(), "editor:canvas", rect);
        if (response.drag_started() || response.clicked())
            && let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
            && let Some(pick) = self.item_at(rect, pos)
        {
            self.select(pick);
            self.dragging = Some(pick);
        }
        if response.dragged()
            && let (Some(pick), Some(pos)) = (self.dragging, response.interact_pointer_pos())
        {
            self.move_to(pick, rect, pos);
        }
        if response.drag_stopped() {
            self.dragging = None;
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::CornerRadius::ZERO, rgb(24, 20, 16));
        let (main, second) = screens(rect);
        for (screen, label) in [(main, "Main screen"), (second, "Second screen (if you have one)")] {
            painter.rect(
                screen,
                egui::CornerRadius::ZERO,
                rgb(88, 104, 80),
                Stroke::new(1.5, hex("#9a7951")),
                egui::StrokeKind::Middle,
            );
            painter.text(
                pos2(screen.center().x, screen.top() - 11.0),
                Align2::CENTER_CENTER,
                label,
                theme::bold(9.0),
                hex("#f7df93"),
            );
        }
        let scale = (main.width() / 1400.0).clamp(0.3, 0.6);
        for (i, building) in self.buildings().iter().enumerate() {
            let at = to_canvas(rect, f(building, "fx"), f(building, "fy"), self::b(building, "far"));
            // TODO(content): dc-paint's `building_art(kind, owned)` picture.
            let art = Rect::from_center_size(at - vec2(0.0, 44.0 * scale), vec2(240.0, 190.0) * scale * 0.6);
            let owned = self::b(building, "owned");
            painter.rect(
                art,
                egui::CornerRadius::same(6),
                if owned { hex("#6a8a4a") } else { hex("#7a4a32") },
                Stroke::new(1.5, WALNUT_DEEP),
                egui::StrokeKind::Middle,
            );
            let kind = s(building, "kind");
            painter.text(art.center(), Align2::CENTER_CENTER, building_name(kind), theme::bold(7.0), CREAM);
            painter.text(
                pos2(at.x, at.y + 15.0),
                Align2::CENTER_CENTER,
                s(building, "name"),
                theme::font(8.0),
                hex("#fff0cd"),
            );
            let guards = building["guards"].as_array().map(Vec::len).unwrap_or(0);
            for g in 0..guards {
                painter.circle_filled(pos2(at.x - 18.0 + g as f32 * 9.0, at.y + 26.0), 3.5, hex("#e0604a"));
            }
            if kind == "nest" {
                let boss = s(&self.data["boss"], "name");
                painter.text(
                    pos2(at.x, at.y - 58.0),
                    Align2::CENTER_CENTER,
                    format!("\u{265b} {}", if boss.is_empty() { "Guardian" } else { boss }),
                    theme::font(8.0),
                    hex("#f6cf6a"),
                );
            }
            if self.selected == Some(Pick::Building(i)) {
                painter.add(egui::Shape::ellipse_stroke(at, vec2(30.0, 22.0), Stroke::new(2.0, hex("#f7df93"))));
            }
        }
        for (i, enemy) in self.enemies().iter().enumerate() {
            let at = to_canvas(rect, f(enemy, "fx"), f(enemy, "fy"), self::b(enemy, "far"));
            painter.circle(at, 8.0, hex("#d0493a"), Stroke::new(1.5, hex("#2a0f0a")));
            let initial = s(enemy, "role").chars().next().unwrap_or('?').to_uppercase().to_string();
            painter.text(at, Align2::CENTER_CENTER, initial, theme::bold(7.0), hex("#fff0cd"));
            if self.selected == Some(Pick::Enemy(i)) {
                painter.circle_stroke(at, 14.0, Stroke::new(2.0, hex("#f7df93")));
            }
        }
    }

    fn tabs(&mut self, ui: &mut Ui) {
        let tabs = [("Map", None), ("Buildings", None), ("Enemies", None), ("Boss", None)];
        let current = match self.tab {
            EditorTab::Map => 0,
            EditorTab::Buildings => 1,
            EditorTab::Enemies => 2,
            EditorTab::Boss => 3,
        };
        if let Some(i) = widgets::tab_bar(ui, &tabs, current) {
            self.tab = [EditorTab::Map, EditorTab::Buildings, EditorTab::Enemies, EditorTab::Boss][i];
        }
        egui::Frame::new()
            .fill(rgba(34, 20, 10, 200))
            .stroke(Stroke::new(1.0, BRASS_DEEP))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().id_salt("editor-tab").show(ui, |ui| {
                    let before = self.data.clone();
                    match self.tab {
                        EditorTab::Map => self.map_tab(ui),
                        EditorTab::Buildings => self.building_tab(ui),
                        EditorTab::Enemies => self.enemy_tab(ui),
                        EditorTab::Boss => self.boss_tab(ui),
                    }
                    if self.data != before {
                        self.dirty = true;
                    }
                });
            });
    }

    fn map_tab(&mut self, ui: &mut Ui) {
        egui::Grid::new("map-form").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
            text_field(ui, "Title", &mut self.data, "title", 60);
            text_field(ui, "One line", &mut self.data, "blurb", 200);
            int_field(ui, "Tier (enemy strength)", &mut self.data, "tier", 1, 6);
            ui.label("Random armour");
            ui.vertical(|ui| {
                let mut tiers: Vec<String> = self.data["enemy_tiers"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                let mut changed = false;
                egui::Grid::new("tier-checks").show(ui, |ui| {
                    for (i, tier) in ARMOR_TIERS.iter().enumerate() {
                        let mut on = tiers.iter().any(|t| t == tier);
                        if widgets::check(ui, &mut on, capitalize(tier)).changed() {
                            changed = true;
                            if on {
                                tiers.push((*tier).to_owned());
                            } else {
                                tiers.retain(|t| t != tier);
                            }
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
                if changed {
                    tiers.sort_by_key(|t| ARMOR_TIERS.iter().position(|x| x == t));
                    self.data["enemy_tiers"] = json!(tiers);
                }
            });
            ui.end_row();
            ui.label("First win rewards");
            let mut reward = self.data["reward_companion"].as_str().map(str::to_owned);
            let mut options: Vec<(String, Option<String>)> = vec![("None".into(), None)];
            options.extend(self.companions.iter().map(|(id, name)| (name.clone(), Some(id.clone()))));
            if widgets::combo(ui, "reward", 200.0, &mut reward, &options).changed() {
                self.data["reward_companion"] = json!(reward);
            }
            ui.end_row();
            int_field(ui, "Most spiders at once", &mut self.data, "cap", 4, 30);
            ui.label("Hatchery sends one every");
            let mut wave = self.data["wave_every"].as_f64().unwrap_or(24.0);
            if ui.add(egui::DragValue::new(&mut wave).range(5.0..=300.0).suffix(" s")).changed() {
                self.data["wave_every"] = json!(wave);
            }
            ui.end_row();
            ui.label("Enemy teamwork");
            let mut tactics = self.data["tactics"].as_i64().unwrap_or(-1);
            let options: Vec<(String, i64)> = [
                (-1, "By the tier"),
                (0, "None: each on its own"),
                (1, "Focus fire"),
                (2, "Formations: flank, screen, perimeter"),
                (3, "Advanced: encircle, bait and ambush"),
            ]
            .iter()
            .map(|(v, t)| ((*t).to_owned(), *v))
            .collect();
            if widgets::combo(ui, "tactics", 240.0, &mut tactics, &options).changed() {
                self.data["tactics"] = json!(tactics);
            }
            ui.end_row();
        });
    }

    fn building_tab(&mut self, ui: &mut Ui) {
        let rows: Vec<String> = self
            .buildings()
            .iter()
            .map(|b| {
                let kind = building_name(s(b, "kind"));
                let shown = if s(b, "name") == kind { String::new() } else { format!("  \u{b7}  {kind}") };
                let guards = b["guards"].as_array().map(Vec::len).unwrap_or(0);
                let where_ = if self::b(b, "far") { "second screen" } else { "main" };
                format!("{}{shown}  \u{b7}  {guards} guards  \u{b7}  {where_}", s(b, "name"))
            })
            .collect();
        for (i, row) in rows.iter().enumerate() {
            let r = ui.selectable_label(self.selected == Some(Pick::Building(i)), RichText::new(row).color(INK));
            probe::record(ui.ctx(), format!("editor:building{i}"), r.rect);
            if r.clicked() {
                self.select(Pick::Building(i));
            }
        }
        ui.horizontal(|ui| {
            let kinds: Vec<(String, String)> =
                building_kinds().iter().map(|(k, n)| ((*n).to_owned(), (*k).to_owned())).collect();
            widgets::combo(ui, "new-kind", 140.0, &mut self.new_kind, &kinds);
            let add = ui.add(CarvedButton::new("Add building"));
            probe::record(ui.ctx(), "editor:add-building", add.rect);
            if add.clicked() {
                let kind = self.new_kind.clone();
                self.add_building(&kind);
            }
            if ui.add(CarvedButton::new("Remove")).clicked() && matches!(self.selected, Some(Pick::Building(_))) {
                self.remove_selected();
            }
        });
        let Some(Pick::Building(i)) = self.selected else { return };
        let mut building = self.buildings()[i].clone();
        egui::Grid::new("building-form").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
            text_field(ui, "Name", &mut building, "name", 40);
            ui.label("Kind");
            let mut kind = s(&building, "kind").to_owned();
            let kinds: Vec<(String, String)> =
                building_kinds().iter().map(|(k, n)| ((*n).to_owned(), (*k).to_owned())).collect();
            if widgets::combo(ui, "b-kind", 160.0, &mut kind, &kinds).changed() {
                building["kind"] = json!(kind);
            }
            ui.end_row();
            bool_field(ui, "Yours at the start", &mut building, "owned");
            bool_field(ui, "On the second screen", &mut building, "far");
            int_field(ui, "Hatchery reserves", &mut building, "reserves", 0, 50);
            ui.label("Healing supply");
            let mut supply = building["supply"].as_f64().unwrap_or(180.0);
            if ui.add(egui::DragValue::new(&mut supply).range(0.0..=5000.0)).changed() {
                building["supply"] = json!(supply);
            }
            ui.end_row();
        });
        if s(&building, "name").trim().is_empty() {
            building["name"] = json!(building_name(s(&building, "kind")));
        }
        ui.label("Guards");
        let guards: Vec<Value> = building["guards"].as_array().cloned().unwrap_or_default();
        for (g, guard) in guards.iter().enumerate() {
            let kind = self.kind_name(s(guard, "kind"));
            let armour = if guard["armor"].is_array() {
                format!("{} pieces", guard["armor"].as_array().map(Vec::len).unwrap_or(0))
            } else {
                "map armour".into()
            };
            let label = format!("{}  \u{b7}  {kind}  \u{b7}  {armour}", capitalize(s(guard, "role")));
            if ui.selectable_label(self.guard_index == Some(g), RichText::new(label).color(INK)).clicked() {
                self.guard_index = Some(g);
            }
        }
        self.data["buildings"][i] = building;
        ui.horizontal(|ui| {
            let add = ui.add(CarvedButton::new("Add guard"));
            probe::record(ui.ctx(), "editor:add-guard", add.rect);
            if add.clicked() {
                self.add_guard();
            }
            if ui.add(CarvedButton::new("Remove guard")).clicked() {
                self.remove_guard();
            }
        });
        if let Some(g) = self.guard_index
            && let Some(mut spec) = self.data["buildings"][i]["guards"].get(g).cloned()
        {
            self.spider_editor(ui, ("guard", i, g), &mut spec, false);
            self.data["buildings"][i]["guards"][g] = spec;
        }
    }

    fn enemy_tab(&mut self, ui: &mut Ui) {
        let rows: Vec<String> = self
            .enemies()
            .iter()
            .map(|e| {
                format!(
                    "{}  \u{b7}  {}  \u{b7}  +{} levels",
                    capitalize(s(e, "role")),
                    self.kind_name(s(e, "kind")),
                    e["level_bonus"].as_i64().unwrap_or(0)
                )
            })
            .collect();
        for (i, row) in rows.iter().enumerate() {
            if ui.selectable_label(self.selected == Some(Pick::Enemy(i)), RichText::new(row).color(INK)).clicked() {
                self.select(Pick::Enemy(i));
            }
        }
        ui.horizontal(|ui| {
            let add = ui.add(CarvedButton::new("Add enemy"));
            probe::record(ui.ctx(), "editor:add-enemy", add.rect);
            if add.clicked() {
                self.add_enemy();
            }
            if ui.add(CarvedButton::new("Remove")).clicked() && matches!(self.selected, Some(Pick::Enemy(_))) {
                self.remove_selected();
            }
        });
        let Some(Pick::Enemy(i)) = self.selected else { return };
        let mut spec = self.enemies()[i].clone();
        let mut far = self::b(&spec, "far");
        if widgets::check(ui, &mut far, "On the second screen").changed() {
            spec["far"] = json!(far);
        }
        self.spider_editor(ui, ("enemy", i, 0), &mut spec, false);
        self.data["enemies"][i] = spec;
    }

    fn boss_tab(&mut self, ui: &mut Ui) {
        let mut boss = self.data["boss"].clone();
        egui::Grid::new("boss-form").num_columns(2).show(ui, |ui| text_field(ui, "Name", &mut boss, "name", 40));
        widgets::wrapped(
            ui,
            "The boss rises at the enemy nest once the Hatchery is sealed.\nHealth here multiplies its boss health.",
            theme::font(9.0),
            CREAM,
        );
        self.spider_editor(ui, ("boss", 0, 0), &mut boss, true);
        if s(&boss, "name").trim().is_empty() {
            boss["name"] = json!("Guardian");
        }
        self.data["boss"] = boss;
    }

    fn kind_name(&self, kind: &str) -> String {
        self.enemy_kinds
            .iter()
            .find(|(id, _, _)| id == kind)
            .map(|(_, n, _)| n.clone())
            .unwrap_or_else(|| "any kind".into())
    }

    /// `SpiderEditor`: how it fights, what it is, how strong, what it wears.
    fn spider_editor(&self, ui: &mut Ui, salt: (&str, usize, usize), spec: &mut Value, boss: bool) {
        egui::Grid::new(("spider-form", salt)).num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
            if !boss {
                ui.label("Fights as");
                let mut role = s(spec, "role").to_owned();
                let roles: Vec<(String, String)> =
                    custom_maps::ROLES.iter().map(|r| (role_name(r).to_owned(), (*r).to_owned())).collect();
                if widgets::combo(ui, ("role", salt), 200.0, &mut role, &roles).changed() {
                    spec["role"] = json!(role);
                }
                ui.end_row();
            }
            ui.label("Kind");
            let mut kind = s(spec, "kind").to_owned();
            let mut kinds: Vec<(String, String)> = vec![("Auto (the map picks)".into(), "auto".into())];
            for (id, name, is_boss) in &self.enemy_kinds {
                if *is_boss == boss || boss {
                    kinds.push((format!("{name}{}", if *is_boss { "  (boss)" } else { "" }), id.clone()));
                }
            }
            if widgets::combo(ui, ("kind", salt), 200.0, &mut kind, &kinds).changed() {
                spec["kind"] = json!(kind);
            }
            ui.end_row();
            ui.label("Levels above the map");
            let mut level = spec["level_bonus"].as_i64().unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut level).range(-5..=20).prefix("+")).changed() {
                spec["level_bonus"] = json!(level);
            }
            ui.end_row();
            ui.label("Health");
            let mut hp = spec["hp"].as_f64().unwrap_or(1.0);
            if ui.add(egui::DragValue::new(&mut hp).range(0.2..=10.0).speed(0.1).suffix(" \u{d7} health")).changed() {
                spec["hp"] = json!((hp * 100.0).round() / 100.0);
            }
            ui.end_row();
            let mut auto = !spec["armor"].is_array();
            ui.label("");
            if widgets::check(ui, &mut auto, "Armour by the map's rules").changed() {
                spec["armor"] = if auto { json!("auto") } else { json!([]) };
            }
            ui.end_row();
            let worn: Vec<String> = spec["armor"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            let mut pieces = worn.clone();
            for slot in self.catalog.slots() {
                ui.label(capitalize(&slot.replace('_', " ")));
                let mut current = worn
                    .iter()
                    .find(|id| self.catalog.item(id).is_some_and(|i| i.slot == slot))
                    .cloned()
                    .unwrap_or_default();
                let mut options: Vec<(String, String)> = vec![("\u{2014} none \u{2014}".into(), String::new())];
                options.extend(
                    self.catalog
                        .armour
                        .iter()
                        .filter(|i| i.slot == slot)
                        .map(|i| (format!("{}  ({})", i.name, i.tier), i.id.clone())),
                );
                ui.add_enabled_ui(!auto, |ui| {
                    if widgets::combo(ui, ("slot", salt, &slot), 220.0, &mut current, &options).changed() {
                        pieces.retain(|id| self.catalog.item(id).is_none_or(|i| i.slot != slot));
                        if !current.is_empty() {
                            pieces.push(current.clone());
                        }
                    }
                });
                ui.end_row();
            }
            if !auto && pieces != worn {
                spec["armor"] = json!(pieces);
            }
            ui.label("Armour level");
            let mut item_level = spec["item_level"].as_i64().unwrap_or(1);
            ui.add_enabled_ui(!auto, |ui| {
                if ui.add(egui::DragValue::new(&mut item_level).range(1..=MAX_ITEM_LEVEL as i64)).changed() {
                    spec["item_level"] = json!(item_level);
                }
            });
            ui.end_row();
        });
    }
}

fn text_field(ui: &mut Ui, label: &str, data: &mut Value, key: &str, limit: usize) {
    ui.label(label);
    let mut text = s(data, key).to_owned();
    if ui.add(egui::TextEdit::singleline(&mut text).char_limit(limit).desired_width(240.0)).changed() {
        data[key] = json!(text);
    }
    ui.end_row();
}

fn int_field(ui: &mut Ui, label: &str, data: &mut Value, key: &str, low: i64, high: i64) {
    ui.label(label);
    let mut value = data[key].as_i64().unwrap_or(low);
    if ui.add(egui::DragValue::new(&mut value).range(low..=high)).changed() {
        data[key] = json!(value);
    }
    ui.end_row();
}

fn bool_field(ui: &mut Ui, label: &str, data: &mut Value, key: &str) {
    ui.label("");
    let mut on = b(data, key);
    if widgets::check(ui, &mut on, label).changed() {
        data[key] = json!(on);
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> MapEditor {
        MapEditor::new(None, crate::bridge::progression_catalog())
    }

    #[test]
    fn a_new_map_is_playable() {
        let e = editor();
        assert!(e.problems().is_empty(), "{:?}", e.problems());
        assert_eq!(e.buildings().len(), crate::sample::new_custom_map()["buildings"].as_array().unwrap().len());
        assert_eq!(e.selected, Some(Pick::Building(0)));
    }

    #[test]
    fn a_map_without_its_essentials_says_why() {
        let mut e = editor();
        e.selected = Some(Pick::Building(0));
        e.remove_selected();
        assert!(e.problems().iter().any(|p| p.contains("Home burrow")), "{:?}", e.problems());
    }

    #[test]
    fn dragging_moves_a_building_to_the_other_screen() {
        let mut e = editor();
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 700.0));
        let (_, second) = screens(canvas);
        e.move_to(Pick::Building(1), canvas, second.center());
        let b = &e.buildings()[1];
        assert_eq!(b["far"], json!(true));
        assert!((b["fx"].as_f64().unwrap() - 0.5).abs() < 0.01);
        let back = to_canvas(canvas, 0.5, b["fy"].as_f64().unwrap() as f32, true);
        assert_eq!(e.item_at(canvas, back), Some(Pick::Building(1)));
        assert!(e.dirty);
    }

    #[test]
    fn guards_and_enemies_are_added_and_capped() {
        let mut e = editor();
        e.select(Pick::Building(1));
        for _ in 0..10 {
            e.add_guard();
        }
        assert_eq!(e.buildings()[1]["guards"].as_array().unwrap().len(), 8);
        e.add_enemy();
        assert_eq!(e.enemies().len(), 1);
        assert_eq!(e.enemies()[0]["role"], json!("hunter"));
        e.add_building("lookout");
        assert_eq!(e.buildings().last().unwrap()["name"], json!("Lookout"));
    }
}
