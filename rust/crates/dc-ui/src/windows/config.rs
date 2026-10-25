//! Port of app/config_ui.py: the settings window (`ConfigWindow`).
//!
//! Six parchment panels on an oak board, in a scroll area so they never
//! overlap: Saved colony setups, Colony roster (the slot table), Factions &
//! relations, Desktop behavior, Prey & food and Ready the colony, with the
//! status bar below. The window sits inside the mode menu (`ModeShell`), as
//! the Qt one did.
//!
//! The window holds the preset being edited; launching, saving and loading
//! are [`UiAction`]s for the app layer, which answers through
//! [`ConfigWindow::load_preset`], [`ConfigWindow::set_presets`],
//! [`ConfigWindow::set_status`] and [`ConfigWindow::set_overlay_running`].

use std::collections::BTreeMap;

use egui::{Align, Color32, Layout, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};
use serde_json::{Map, Value, json};

use crate::art;
use crate::model::{PresetEntry, Rgb, SettingsCatalog, ThumbnailFn};
use crate::probe;
use crate::teams::{self, Stances, TeamProfile, TeamProfiles};
use crate::theme::{self, *};
use crate::widgets::{self, ButtonLook, CarvedButton};
use crate::windows::mode_menu::ModeShell;
use crate::{OverlayMode, UiAction};

pub const RANDOM_MODEL_ID: &str = "__random_model__";
pub const RANDOM_PERSONALITY_ID: &str = "__random_personality__";
pub const RANDOM_PALETTE_ID: &str = "__random_palette__";
pub const RANDOM_CATEGORY_ID: &str = "__random_category__";
pub const NEW_TEAM_SENTINEL: &str = "__new_team__";
/// A new row lands on the kind the project is about (DC-52).
pub const DEFAULT_CATEGORY_ID: &str = "tarantula";
pub const MODEL_ICON_SIZE: f32 = 34.0;
pub const SWATCH_ICON_SIZE: f32 = 20.0;
/// Qt's team picker is 52 px wide: enough for its 14 px swatch and arrow.
pub const TEAM_COMBO_WIDTH: f32 = 52.0;
pub const TEAM_ICON_SIZE: f32 = 14.0;

pub const SIZE_OPTIONS: [(&str, f64); 5] = [
    ("Tiny (60%)", 0.60),
    ("Small (80%)", 0.80),
    ("Normal (100%)", 1.00),
    ("Large (125%)", 1.25),
    ("Huge (160%)", 1.60),
];

pub const COLOR_KEYS: [(&str, &str); 7] = [
    ("body", "Body"),
    ("legs", "Legs"),
    ("highlight", "Highlights"),
    ("eyes", "Eyes"),
    ("leg_band", "Leg bands"),
    ("leg_dark", "Leg shadows"),
    ("leg_tip", "Leg tips"),
];

/// The swatch shows the widest part of the creature first.
pub const SWATCH_KEY_ORDER: [&str; 6] = ["body", "abdomen", "legs", "head", "eyes", "accent"];

/// The slot table's short headers (`SLOT_HEADERS`); colour and remove are
/// blank on purpose and carry tooltips.
pub const SLOT_HEADERS: [&str; 9] = ["Category", "Skin", "", "Temperament", "Count", "Skills", "Team", "Job", ""];

/// The Always show switches, by preset key.
pub const LABEL_SWITCHES: [(&str, &str, &str); 5] = [
    (
        "always_show_names",
        "Names",
        "Show every spider's name all the time, not only on hover. A spider nobody has named shows its model's name.",
    ),
    ("always_show_levels", "Levels", "Show every spider's level beside its name."),
    ("always_show_health", "Health", "Show every spider's health bar, not only ones pinned one at a time."),
    ("always_show_xp", "XP", "Show every spider's progress to its next level as a thin line under its level."),
    ("always_show_stamina", "Stamina", "Show every spider's stamina (energy) under its health bar."),
];

/// `BODY_PLAN_LABELS`.
pub fn body_plan_label(plan: &str) -> String {
    match plan {
        "bug" => "Bug".into(),
        "segmented" => "Segmented".into(),
        "jumper" => "Jumper".into(),
        "tarantula" => "Tarantula".into(),
        other => teams::title_case(other),
    }
}

/// `_normalize_color_overrides`: safe RGB lists for the per-slot palette.
pub fn normalize_color_overrides(value: Option<&Value>) -> BTreeMap<String, Rgb> {
    let mut out = BTreeMap::new();
    let Some(Value::Object(map)) = value else { return out };
    for (key, rgb) in map {
        let key = key.trim();
        let Value::Array(parts) = rgb else { continue };
        if key.is_empty() || parts.len() != 3 {
            continue;
        }
        let channels: Option<Vec<u8>> = parts
            .iter()
            .map(|p| {
                p.as_f64()
                    .or_else(|| p.as_str().and_then(|s| s.parse().ok()))
                    .map(|f| f.trunc().clamp(0.0, 255.0) as u8)
            })
            .collect();
        if let Some(c) = channels {
            out.insert(key.to_owned(), [c[0], c[1], c[2]]);
        }
    }
    out
}

fn new_slot_id() -> String {
    // uuid4().hex[:12]
    let mut rng = dc_core::PyRandom::from_entropy();
    format!("slot-{:012x}", rng.getrandbits(48))
}

/// One creature slot: a row of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotRow {
    pub slot_id: String,
    pub category: String,
    pub model: String,
    pub personality: String,
    pub count: u32,
    /// Carried since the "Pick 1-10" column went away, so it round-trips.
    pub count_random: bool,
    pub skill_ids: Vec<String>,
    pub ability_ids: Option<Vec<String>>,
    /// Only a saved list or a manual edit sticks when the temperament changes.
    pub skills_custom: bool,
    pub color_overrides: BTreeMap<String, Rgb>,
    pub team: String,
    pub last_team: String,
    pub job: String,
}

/// Prey settings (`flies`).
#[derive(Clone, Debug, PartialEq)]
pub struct FliesSettings {
    pub enabled: bool,
    pub min_interval: f64,
    pub max_interval: f64,
    pub max_flies: u32,
    pub spawner: bool,
}

impl Default for FliesSettings {
    fn default() -> Self {
        Self { enabled: false, min_interval: 4.0, max_interval: 9.0, max_flies: 6, spawner: false }
    }
}

/// The modal dialogs of the window.
#[derive(Clone, Debug)]
enum Dialog {
    Colors { slot: usize, current: BTreeMap<String, Rgb>, preset: Option<String> },
    Abilities { slot: usize, checks: Vec<(String, bool)> },
    ClearSlots,
    ResetProgress,
    NewTeam { slot: usize, name: String },
    Message { title: String, text: String },
}

/// The settings window.
pub struct ConfigWindow {
    pub catalog: SettingsCatalog,
    pub presets: Vec<PresetEntry>,
    pub preset_name: String,
    /// The path chosen in "Saved preset".
    pub preset_choice: Option<String>,
    pub slots: Vec<SlotRow>,
    pub size_index: usize,
    pub interferable: bool,
    pub label_switches: BTreeMap<String, bool>,
    pub flies: FliesSettings,
    /// Everything the preset carried, kept so a save does not drop settings
    /// this window has no widget for.
    loaded_settings: Map<String, Value>,
    team_profiles: TeamProfiles,
    team_stances: Stances,
    team_name_edits: BTreeMap<String, String>,
    pub status: String,
    pub overlay_running: bool,
    /// Per-spider detail from a live `session_state`.
    pub live_creature_state: Vec<Value>,
    thumbnail: Option<ThumbnailFn>,
    dialog: Option<Dialog>,
    pub shell: ModeShell,
    rng: dc_core::PyRandom,
    pending: Vec<UiAction>,
}

impl ConfigWindow {
    /// A window over `catalog`, with one fresh slot (as when no default
    /// preset exists).
    pub fn new(catalog: SettingsCatalog) -> Self {
        let mut window = Self {
            label_switches: LABEL_SWITCHES
                .iter()
                .map(|(k, _, _)| ((*k).to_owned(), catalog.label_default(k)))
                .collect(),
            catalog,
            presets: Vec::new(),
            preset_name: "Default".into(),
            preset_choice: None,
            slots: Vec::new(),
            size_index: 2,
            interferable: true,
            flies: FliesSettings::default(),
            loaded_settings: Map::new(),
            team_profiles: TeamProfiles::new(),
            team_stances: Stances::new(),
            team_name_edits: BTreeMap::new(),
            status: String::new(),
            overlay_running: false,
            live_creature_state: Vec::new(),
            thumbnail: None,
            dialog: None,
            shell: ModeShell::new(crate::model::AdventureView::default()),
            rng: dc_core::PyRandom::from_entropy(),
            pending: Vec::new(),
        };
        window.refresh_discovery_status();
        window.add_slot(SlotSpec::default());
        window
    }

    /// The version belongs where a user can read it off and quote it.
    pub fn title() -> String {
        format!("Desktop Bug Companion {}", dc_core::VERSION)
    }

    /// Thumbnails and portraits from dc-paint.
    pub fn set_thumbnails(&mut self, thumbnail: ThumbnailFn) {
        self.thumbnail = Some(thumbnail);
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    pub fn set_overlay_running(&mut self, running: bool) {
        self.overlay_running = running;
    }

    /// `refresh_presets`.
    pub fn set_presets(&mut self, presets: Vec<PresetEntry>) {
        if self.preset_choice.as_ref().is_none_or(|p| !presets.iter().any(|e| &e.path == p)) {
            self.preset_choice = presets.first().map(|p| p.path.clone());
        }
        self.presets = presets;
    }

    /// `refresh_all`: a new catalogue, keeping the rows.
    pub fn set_catalog(&mut self, catalog: SettingsCatalog) {
        let rows = self.collect_slots();
        self.catalog = catalog;
        self.refresh_discovery_status();
        self.slots.clear();
        if rows.is_empty() {
            self.add_slot(SlotSpec::default());
        } else {
            for slot in rows {
                self.add_slot(SlotSpec::from_json(&slot));
            }
        }
    }

    fn refresh_discovery_status(&mut self) {
        let mut warnings = self.catalog.warnings.clone();
        if self.catalog.models.is_empty() {
            warnings.push("No valid models found in models/*/model.json".into());
        }
        if self.catalog.personalities.is_empty() {
            warnings.push("No valid personalities found in personalities/*.json".into());
        }
        self.status = if warnings.is_empty() {
            "Ready. Build a preset, then press Save and launch overlay.".into()
        } else {
            warnings.join("\n")
        };
    }

    // -- slots ------------------------------------------------------------------

    /// `add_slot`.
    pub fn add_slot(&mut self, spec: SlotSpec) {
        let catalog = &self.catalog;
        let model_id = spec.model.clone();
        let category = match model_id.as_deref() {
            Some(id) if id != RANDOM_MODEL_ID => catalog.plan_of_model(id).unwrap_or_else(|| RANDOM_CATEGORY_ID.into()),
            Some(_) => RANDOM_CATEGORY_ID.into(),
            None => {
                if catalog.categories().iter().any(|c| c == DEFAULT_CATEGORY_ID) {
                    DEFAULT_CATEGORY_ID.into()
                } else {
                    RANDOM_CATEGORY_ID.into()
                }
            }
        };
        let model = fill_skin(catalog, &category, model_id.as_deref());
        let selectable = catalog.selectable_personality_ids(spec.personality.as_deref());
        let personality = match spec.personality.as_deref() {
            Some(p) if p == RANDOM_PERSONALITY_ID || selectable.iter().any(|s| s == p) => p.to_owned(),
            Some(_) | None => {
                if catalog.personalities.contains_key("balanced") {
                    "balanced".into()
                } else {
                    selectable.first().cloned().unwrap_or_else(|| RANDOM_PERSONALITY_ID.into())
                }
            }
        };
        let job = catalog.normalize_job_id(spec.job.as_deref().unwrap_or("none"));
        let pid = Some(personality.as_str()).filter(|p| *p != RANDOM_PERSONALITY_ID);
        let (skill_ids, ability_ids, skills_custom) = if let Some(abilities) = &spec.abilities {
            (
                catalog.skills_with_selected_abilities(pid, Some(abilities)),
                Some(catalog.normalize_ability_ids(Some(abilities))),
                true,
            )
        } else if let Some(skills) = &spec.skills {
            (catalog.normalize_skill_ids(Some(skills)), Some(catalog.normalize_ability_ids(Some(skills))), true)
        } else {
            (catalog.default_skills_for(pid, &job), None, false)
        };
        let team = teams::normalize_team_id(spec.team.as_deref().unwrap_or("neutral"));
        if team != teams::NEUTRAL && !self.team_profiles.contains_key(&team) {
            // A preset can name a team the block never described.
            self.team_profiles.insert(
                team.clone(),
                TeamProfile {
                    id: team.clone(),
                    name: teams::title_case(&team.replace('_', " ")),
                    color: teams::default_color(&team),
                },
            );
        }
        self.slots.push(SlotRow {
            slot_id: spec.slot_id.unwrap_or_else(new_slot_id),
            category,
            model,
            personality,
            count: spec.count.clamp(1, 50),
            count_random: spec.count_random,
            skill_ids,
            ability_ids,
            skills_custom,
            color_overrides: spec.colors,
            last_team: team.clone(),
            team,
            job,
        });
        self.ensure_team_profiles();
    }

    /// `remove_slot_by_button`.
    pub fn remove_slot(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
        }
        self.ensure_team_profiles();
    }

    /// `_on_category_changed`.
    pub fn set_category(&mut self, index: usize, category: &str) {
        let Some(row) = self.slots.get_mut(index) else { return };
        row.category = category.to_owned();
        row.model = fill_skin(&self.catalog, category, Some(&row.model.clone()));
    }

    /// `_on_personality_changed`: follow the new temperament's defaults
    /// unless the slot's abilities were customised.
    pub fn set_personality(&mut self, index: usize, personality: &str) {
        let Some(row) = self.slots.get_mut(index) else { return };
        row.personality = personality.to_owned();
        if !row.skills_custom {
            let pid = Some(personality).filter(|p| *p != RANDOM_PERSONALITY_ID);
            row.skill_ids = self.catalog.default_skills_for(pid, &row.job);
        }
    }

    /// `_on_job_changed`.
    pub fn set_job(&mut self, index: usize, job: &str) {
        let Some(row) = self.slots.get_mut(index) else { return };
        row.job = self.catalog.normalize_job_id(job);
        if !row.skills_custom {
            let pid = Some(row.personality.as_str()).filter(|p| *p != RANDOM_PERSONALITY_ID);
            row.skill_ids = self.catalog.default_skills_for(pid, &row.job);
        }
    }

    /// Choose a team for a slot; the New team entry asks for a name first.
    pub fn set_team(&mut self, index: usize, team: &str) {
        if team == NEW_TEAM_SENTINEL {
            self.dialog = Some(Dialog::NewTeam { slot: index, name: String::new() });
            return;
        }
        if let Some(row) = self.slots.get_mut(index) {
            row.team = teams::normalize_team_id(team);
            row.last_team = row.team.clone();
        }
        self.ensure_team_profiles();
    }

    /// `_prompt_for_new_team` accepted: a team made from `name`.
    pub fn create_team(&mut self, index: usize, name: &str) -> Option<String> {
        if name.trim().is_empty() {
            return None;
        }
        let id = teams::unique_team_id(name, &self.team_profiles);
        self.team_profiles.insert(
            id.clone(),
            TeamProfile {
                id: id.clone(),
                name: teams::normalize_team_name(name, &id),
                color: teams::default_color(&id),
            },
        );
        self.set_team(index, &id);
        Some(id)
    }

    /// `edit_skills_for_button` accepted.
    pub fn set_abilities(&mut self, index: usize, selected: Vec<String>) {
        let ability_ids = self.catalog.ability_skill_ids();
        let Some(row) = self.slots.get_mut(index) else { return };
        let mut ids: Vec<String> = row.skill_ids.iter().filter(|id| !ability_ids.contains(id)).cloned().collect();
        ids.extend(selected.iter().cloned());
        row.skill_ids = self.catalog.normalize_skill_ids(Some(&ids));
        row.ability_ids = Some(selected);
        row.skills_custom = true;
    }

    /// `edit_colors_for_button` accepted.
    pub fn set_colors(&mut self, index: usize, colors: BTreeMap<String, Rgb>) {
        if let Some(row) = self.slots.get_mut(index) {
            row.color_overrides = colors;
        }
    }

    /// `_palette_for_button`: the colours a slot will produce, overrides first, up to four.
    pub fn slot_palette(&self, index: usize) -> Vec<Rgb> {
        let Some(row) = self.slots.get(index) else { return Vec::new() };
        let mut palette = self.catalog.model_color_defaults(Some(&row.model), RANDOM_MODEL_ID);
        palette.extend(row.color_overrides.iter().map(|(k, v)| (k.clone(), *v)));
        let mut ordered: Vec<Rgb> = SWATCH_KEY_ORDER.iter().filter_map(|k| palette.get(*k).copied()).collect();
        ordered.extend(palette.iter().filter(|(k, _)| !SWATCH_KEY_ORDER.contains(&k.as_str())).map(|(_, v)| *v));
        ordered.truncate(4);
        ordered
    }

    /// The swatch's tooltip and accessible name (`_refresh_colors_button`).
    pub fn swatch_description(&self, index: usize) -> (String, String) {
        let Some(row) = self.slots.get(index) else { return Default::default() };
        let overrides = &row.color_overrides;
        let description = if !overrides.is_empty() {
            format!("Custom palette: {}. Click to edit.", overrides.keys().cloned().collect::<Vec<_>>().join(", "))
        } else if !self.slot_palette(index).is_empty() {
            "The model's own palette. Click to choose custom colors for this slot.".into()
        } else {
            "Use the model's default palette. Click to choose custom colors for this slot.".into()
        };
        let name = if overrides.is_empty() {
            "Colors for this slot: model default".into()
        } else {
            format!("Colors for this slot: {} custom", overrides.len())
        };
        (description, name)
    }

    /// The skills button: "2 of 5" and its tooltip (`_refresh_skills_button`).
    pub fn skills_label(&self, index: usize) -> (String, String) {
        let Some(row) = self.slots.get(index) else { return Default::default() };
        let summary = self.catalog.compact_ability_summary(&row.skill_ids);
        (
            self.catalog.short_ability_label(&row.skill_ids),
            format!("{summary}.\nClick to choose which abilities this creature slot can use at launch."),
        )
    }

    /// `_refresh_team_box_tooltip`.
    pub fn team_tooltip(&self, index: usize) -> String {
        let team = self.slots.get(index).map(|r| r.team.clone()).unwrap_or_else(|| "neutral".into());
        let name = teams::team_label(&team, &self.team_profiles);
        format!(
            "Team: {name}.\nName your teams and set what stands between them in the Teams panel below.\n\n{}",
            teams::HOSTILITY_NOTE
        )
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
        self.status = "All creature slots cleared. Add a slot before saving or launching.".into();
        self.ensure_team_profiles();
    }

    /// `set_random_count_options`: roll a real count into every slot now.
    pub fn roll_random_counts(&mut self) {
        if self.slots.is_empty() {
            let count = self.rng.randint(1, 10) as u32;
            self.add_slot(SlotSpec { count, ..Default::default() });
        }
        for row in &mut self.slots {
            row.count_random = false;
            row.count = self.rng.randint(1, 10) as u32;
        }
        self.status = "Rolled a new count from 1 to 10 for every slot.".into();
    }

    /// `set_random_model_options`.
    pub fn randomize_models(&mut self) {
        if self.slots.is_empty() {
            self.add_slot(SlotSpec { model: Some(RANDOM_MODEL_ID.into()), ..Default::default() });
        }
        for row in &mut self.slots {
            row.model = RANDOM_MODEL_ID.into();
        }
        self.status = "Every slot will choose a random model when the overlay launches.".into();
    }

    /// `set_random_personality_options`.
    pub fn randomize_personalities(&mut self) {
        if self.slots.is_empty() {
            self.add_slot(SlotSpec { personality: Some(RANDOM_PERSONALITY_ID.into()), ..Default::default() });
        }
        for index in 0..self.slots.len() {
            self.set_personality(index, RANDOM_PERSONALITY_ID);
        }
        self.status = "Every slot will choose a random personality when the overlay launches.".into();
    }

    // -- teams ---------------------------------------------------------------------

    fn team_ids_in_use(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for row in &self.slots {
            let id = teams::normalize_team_id(&row.team);
            if id != teams::NEUTRAL && !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// `_ensure_team_profiles`: every team in use gets an identity.
    fn ensure_team_profiles(&mut self) {
        let payload = teams::teams_payload(&self.team_profiles);
        self.team_profiles = teams::normalize_teams(Some(&payload), &self.team_ids_in_use());
    }

    pub fn team_profiles(&self) -> &TeamProfiles {
        &self.team_profiles
    }

    pub fn team_stances(&self) -> &Stances {
        &self.team_stances
    }

    /// `_rename_team`.
    pub fn rename_team(&mut self, team_id: &str, text: &str) {
        let Some(profile) = self.team_profiles.get_mut(team_id) else { return };
        profile.name = teams::normalize_team_name(text, team_id);
    }

    /// `_pick_team_color`.
    pub fn set_team_color(&mut self, team_id: &str, color: Rgb) {
        if let Some(profile) = self.team_profiles.get_mut(team_id) {
            profile.color = color;
        }
    }

    /// `_set_stance`: declare one pair, both ways, keeping a chosen neutral.
    pub fn set_stance(&mut self, left: &str, right: &str, relation: &str) {
        self.team_stances.entry(left.to_owned()).or_default().insert(right.to_owned(), relation.to_owned());
        self.team_stances.entry(right.to_owned()).or_default().insert(left.to_owned(), relation.to_owned());
    }

    // -- presets --------------------------------------------------------------------

    /// `collect_slots`: the rows as preset slots.
    pub fn collect_slots(&self) -> Vec<Value> {
        self.slots
            .iter()
            .map(|row| {
                let mut slot = Map::new();
                slot.insert("model".into(), json!(row.model));
                slot.insert("personality".into(), json!(row.personality));
                slot.insert("count".into(), json!(row.count));
                slot.insert("count_random".into(), json!(row.count_random));
                slot.insert("slot_id".into(), json!(row.slot_id));
                slot.insert("team".into(), json!(if row.team.is_empty() { "neutral" } else { &row.team }));
                slot.insert("job".into(), json!(self.catalog.normalize_job_id(&row.job)));
                if row.skills_custom {
                    let source = row.ability_ids.clone().unwrap_or_else(|| row.skill_ids.clone());
                    slot.insert("abilities".into(), json!(self.catalog.normalize_ability_ids(Some(&source))));
                }
                if !row.color_overrides.is_empty() {
                    slot.insert("colors".into(), json!(row.color_overrides));
                }
                Value::Object(slot)
            })
            .collect()
    }

    pub fn size_scale(&self) -> f64 {
        SIZE_OPTIONS[self.size_index.min(SIZE_OPTIONS.len() - 1)].1
    }

    /// `current_settings_data`.
    pub fn current_settings_data(&mut self) -> Value {
        self.ensure_team_profiles();
        let mut settings = self.loaded_settings.clone();
        // `dict.pop` keeps the order of the rest (a plain `remove` swaps).
        for retired in ["mood_mode", "social_play", "gait_style"] {
            settings.shift_remove(retired);
        }
        settings.insert("size_scale".into(), json!(self.size_scale()));
        settings.insert("interferable".into(), json!(self.interferable));
        for (key, _, _) in LABEL_SWITCHES {
            settings.insert(key.into(), json!(self.label_switches.get(key).copied().unwrap_or(false)));
        }
        settings.insert("teams".into(), teams::teams_payload(&self.team_profiles));
        settings.insert("team_relations".into(), teams::minimal_stances(&self.team_stances));
        let round2 = |v: f64| (v * 100.0).round() / 100.0;
        settings.insert(
            "flies".into(),
            json!({
                "enabled": self.flies.enabled,
                "min_interval": round2(self.flies.min_interval),
                "max_interval": round2(self.flies.max_interval),
                "max_flies": self.flies.max_flies,
                "spawner": self.flies.spawner,
            }),
        );
        Value::Object(settings)
    }

    /// `current_preset_data` with `validate_preset`'s essential check.
    pub fn current_preset_data(&mut self) -> Result<Value, String> {
        let slots = self.collect_slots();
        if slots.is_empty() {
            return Err("Add at least one creature slot before saving or launching.".into());
        }
        let name = self.preset_name.trim();
        Ok(json!({
            "name": if name.is_empty() { "Default" } else { name },
            "slots": slots,
            "settings": self.current_settings_data(),
        }))
    }

    /// `load_preset_path`: show a preset that the app has read.
    pub fn load_preset(&mut self, data: &Value) {
        self.preset_name = data.get("name").and_then(Value::as_str).unwrap_or("Default").to_owned();
        let settings = data.get("settings").cloned().unwrap_or(Value::Null);
        // Teams first: a preset that renamed its teams shows those names.
        self.team_profiles = teams::normalize_teams(settings.get("teams"), &[]);
        self.slots.clear();
        if let Some(slots) = data.get("slots").and_then(Value::as_array) {
            for (index, slot) in slots.iter().enumerate() {
                let mut spec = SlotSpec::from_json(slot);
                if spec.slot_id.is_none() {
                    spec.slot_id = Some(format!("slot-{index}"));
                }
                self.add_slot(spec);
            }
        }
        self.apply_settings(&settings);
    }

    /// `apply_settings_to_ui`.
    pub fn apply_settings(&mut self, settings: &Value) {
        let empty = Map::new();
        let map = settings.as_object().unwrap_or(&empty);
        self.loaded_settings = map.clone();
        let used = self.team_ids_in_use();
        self.team_profiles = teams::normalize_teams(map.get("teams"), &used);
        self.team_stances = teams::normalize_team_stances(map.get("team_relations"));
        self.team_name_edits.clear();
        let scale = map.get("size_scale").and_then(Value::as_f64).unwrap_or(1.0);
        self.size_index = closest_size(scale);
        self.interferable = map.get("interferable").and_then(Value::as_bool).unwrap_or(true);
        for (key, _, _) in LABEL_SWITCHES {
            let value = map.get(key).and_then(Value::as_bool).unwrap_or_else(|| self.catalog.label_default(key));
            self.label_switches.insert(key.into(), value);
        }
        let flies = map.get("flies").and_then(Value::as_object).cloned().unwrap_or_default();
        let mn = flies.get("min_interval").and_then(Value::as_f64).unwrap_or(4.0);
        let mx = flies.get("max_interval").and_then(Value::as_f64).unwrap_or(9.0).max(mn);
        self.flies = FliesSettings {
            enabled: flies.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            min_interval: mn.clamp(0.3, 120.0),
            max_interval: mx.clamp(0.4, 240.0),
            max_flies: flies.get("max_flies").and_then(Value::as_u64).unwrap_or(6).min(40) as u32,
            spawner: flies.get("spawner").and_then(Value::as_bool).unwrap_or(false),
        };
    }

    /// `_apply_live_session_state`: a tray-driven change from the running overlay.
    pub fn apply_live_session_state(&mut self, state: &Value) {
        let Some(map) = state.as_object() else { return };
        for (key, _, _) in LABEL_SWITCHES {
            if let Some(v) = map.get(key).and_then(Value::as_bool) {
                self.label_switches.insert(key.into(), v);
            }
        }
        if let Some(scale) = map.get("size_scale").and_then(Value::as_f64) {
            self.size_index = closest_size(scale);
        }
        if let Some(v) = map.get("flies_enabled").and_then(Value::as_bool) {
            self.flies.enabled = v;
        }
        if let Some(v) = map.get("interferable").and_then(Value::as_bool) {
            self.interferable = v;
        }
        self.live_creature_state = map.get("creatures").and_then(Value::as_array).cloned().unwrap_or_default();
        self.status = "Live update received from the running overlay.".into();
    }

    /// `update_summary`: the preset in one paragraph (the launch tooltip).
    pub fn summary(&self) -> String {
        let rows = self.slots.len();
        let mut fixed = 0;
        let mut random_count_rows = 0;
        let mut random_models = 0;
        let mut random_personalities = 0;
        let mut custom_skills = 0;
        let mut custom_colors = 0;
        let mut team_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut job_counts: BTreeMap<String, usize> = BTreeMap::new();
        for row in &self.slots {
            if row.model == RANDOM_MODEL_ID {
                random_models += 1;
            }
            if row.personality == RANDOM_PERSONALITY_ID {
                random_personalities += 1;
            }
            if row.count_random {
                random_count_rows += 1;
            } else {
                fixed += row.count;
            }
            if row.skills_custom {
                custom_skills += 1;
            }
            if !row.color_overrides.is_empty() {
                custom_colors += 1;
            }
            if row.team != teams::NEUTRAL {
                *team_counts.entry(row.team.clone()).or_default() += 1;
            }
            if row.job != "none" {
                *job_counts.entry(row.job.clone()).or_default() += 1;
            }
        }
        let creature_text = if rows == 0 {
            "No creature slots yet. Add at least one slot to launch.".to_owned()
        } else {
            let random_text = if random_count_rows > 0 {
                format!(" plus {random_count_rows} random-count slot(s)")
            } else {
                String::new()
            };
            let mut text = format!("{rows} slot(s), {fixed} fixed creature(s){random_text}.");
            if random_models > 0 || random_personalities > 0 {
                text += &format!(
                    " Random choices: {random_models} model slot(s), {random_personalities} personality slot(s)."
                );
            }
            if custom_skills > 0 {
                text += &format!(" Custom abilities: {custom_skills} slot(s).");
            }
            if custom_colors > 0 {
                text += &format!(" Custom colors: {custom_colors} slot(s).");
            }
            if !team_counts.is_empty() {
                let parts: Vec<String> = team_counts
                    .iter()
                    .map(|(id, n)| format!("{} ({n})", teams::team_label(id, &self.team_profiles)))
                    .collect();
                text += &format!(" Teams: {}.", parts.join(", "));
            }
            if !job_counts.is_empty() {
                let parts: Vec<String> = job_counts.iter().map(|(id, n)| format!("{id} ({n})")).collect();
                text += &format!(" Jobs: {}.", parts.join(", "));
            }
            text
        };
        let size_text = SIZE_OPTIONS[self.size_index.min(4)].0;
        let drag = if self.interferable { "dragging on" } else { "dragging off" };
        let shown: Vec<&str> = [
            ("names", "always_show_names"),
            ("levels", "always_show_levels"),
            ("health", "always_show_health"),
            ("XP", "always_show_xp"),
            ("stamina", "always_show_stamina"),
        ]
        .iter()
        .filter(|(_, key)| self.label_switches.get(*key).copied().unwrap_or(false))
        .map(|(name, _)| *name)
        .collect();
        let labels = if shown.is_empty() {
            "labels on hover".to_owned()
        } else {
            format!("always showing {}", shown.join(", "))
        };
        format!("Preset summary: {creature_text} Size: {size_text}. {drag}; {labels}.")
    }

    // -- actions ----------------------------------------------------------------

    fn warn(&mut self, title: &str, text: &str) {
        self.dialog = Some(Dialog::Message { title: title.into(), text: text.into() });
    }

    fn save(&mut self) {
        match self.current_preset_data() {
            Ok(preset) => self.pending.push(UiAction::SavePreset { preset }),
            Err(e) => self.warn("No creature slots", &e),
        }
    }

    fn launch(&mut self, mode: OverlayMode) {
        match self.current_preset_data() {
            Ok(preset) => self.pending.push(UiAction::LaunchOverlay { mode, preset }),
            Err(e) => self.warn("No creature slots", &e),
        }
    }

    // -- drawing --------------------------------------------------------------

    /// Draw a frame; returns what the app should do.
    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        ctx.root_ui(|ui| self.show(ui))
    }

    /// Draw inside a parent `Ui` (the whole window).
    pub fn show(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let mut shell = std::mem::replace(&mut self.shell, ModeShell::new(Default::default()));
        let mut actions = shell.show(ui, &mut |ui| self.companion_page(ui));
        self.shell = shell;
        if actions.iter().any(|a| matches!(a, UiAction::PlayMap { .. })) {
            // Adventure saves and launches the preset too (`start_adventure`).
            if let Ok(preset) = self.current_preset_data() {
                actions.push(UiAction::LaunchOverlay { mode: OverlayMode::Adventure, preset });
            }
        }
        actions.append(&mut self.pending);
        actions
    }

    /// The Companion page: the panels and the status bar.
    pub fn companion_page(&mut self, ui: &mut Ui) -> Vec<UiAction> {
        let ctx = ui.ctx().clone();
        theme::with_visuals(ui, theme::companion_visuals(), |ui| {
            egui::Panel::bottom("config-status")
                .frame(egui::Frame::new().fill(WALNUT).inner_margin(egui::Margin::symmetric(8, 4)))
                .show(ui, |ui| {
                    ui.label(RichText::new(&self.status).font(theme::font(9.0)).color(CREAM));
                });
            egui::ScrollArea::both().id_salt("config-scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.set_min_width(ui.available_width().max(820.0));
                ui.spacing_mut().item_spacing.y = 5.0;
                widgets::text(ui, "Desktop Bug Companion", theme::bold(17.0), WALNUT_DEEP).on_hover_text(
                    "Build a creature preset by choosing a preset, adding creature slots, tuning overlay behavior, then launching. Settings are saved automatically when you launch.",
                );
                self.preset_group(ui);
                self.creatures_group(ui);
                self.teams_group(ui);
                self.behavior_group(ui);
                self.flies_group(ui);
                self.launch_group(ui);
            });
        });
        self.dialogs(&ctx);
        std::mem::take(&mut self.pending)
    }

    fn group(ui: &mut Ui, name: &str, tooltip: &str, add: impl FnOnce(&mut Ui)) {
        let response = theme::parchment_group()
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui)
            })
            .response;
        // The coloured tab of the group (`QGroupBox#...::title`).
        let tab = Rect::from_min_size(response.rect.left_top() + vec2(12.0, -2.0), vec2(28.0, 4.0));
        ui.painter().rect_filled(tab, egui::CornerRadius::same(2), theme::group_tab_color(name));
        probe::record(ui.ctx(), format!("group:{name}"), response.rect);
        if !tooltip.is_empty() {
            let _ = response.on_hover_text(tooltip);
        }
    }

    fn preset_group(&mut self, ui: &mut Ui) {
        Self::group(ui, "presetGroup", "Name, load, save, or refresh presets from the presets folder.", |ui| {
            widgets::section_header(
                ui,
                "preset",
                "Saved colony setups",
                "Name, load, save, or refresh a complete spider configuration.",
            );
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut self.preset_name).hint_text("Preset name").desired_width(200.0))
                    .on_hover_text("This becomes the saved preset file name.");
                ui.label("Saved preset:");
                let options: Vec<(String, Option<String>)> =
                    self.presets.iter().map(|p| (p.label.clone(), Some(p.path.clone()))).collect();
                widgets::combo(ui, "preset-combo", 200.0, &mut self.preset_choice, &options)
                    .on_hover_text("Choose an existing preset from the presets folder.");
                let load = ui.add(CarvedButton::new("Load"));
                probe::record(ui.ctx(), "button:Load", load.rect);
                if load.clicked()
                    && let Some(path) = self.preset_choice.clone()
                {
                    self.pending.push(UiAction::LoadPreset { path });
                }
                let save = ui.add(CarvedButton::new("Save").tooltip(
                    "Save the current preset. While the overlay is running, this also applies your changes to it live.",
                ));
                probe::record(ui.ctx(), "button:Save", save.rect);
                if save.clicked() {
                    self.save();
                }
            });
            let refresh = ui.add(
                CarvedButton::new("Refresh library").tooltip("Reload models, personalities, and presets from disk."),
            );
            probe::record(ui.ctx(), "button:Refresh library", refresh.rect);
            if refresh.clicked() {
                self.pending.push(UiAction::RefreshLibrary);
            }
        });
    }

    fn creatures_group(&mut self, ui: &mut Ui) {
        let tip = "Each row is one creature group. Temperament is stable personality, Job is a separate profession, and Abilities are true capabilities.";
        let mut events = Vec::new();
        Self::group(ui, "creaturesGroup", tip, |ui| {
            widgets::section_header(
                ui,
                "creatures",
                "Colony roster",
                "Each slot sets a spider's body, temperament, job, team, and abilities.",
            );
            egui::Frame::new()
                .fill(INPUT_BG)
                .stroke(egui::Stroke::new(1.0, INPUT_BORDER))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    ui.set_min_height(92.0);
                    egui::Grid::new("slot-table")
                        .striped(true)
                        .spacing(vec2(6.0, 3.0))
                        .min_row_height(MODEL_ICON_SIZE + 6.0)
                        .show(ui, |ui| {
                            for (col, header) in SLOT_HEADERS.iter().enumerate() {
                                let r = widgets::text(ui, *header, theme::bold(9.0), HEADER_TEXT);
                                match col {
                                    2 => {
                                        let _ = r.on_hover_text(
                                            "The colours this slot will produce. Click a swatch to change them.",
                                        );
                                    }
                                    8 => {
                                        let _ = r.on_hover_text("Remove a creature slot");
                                    }
                                    _ => {}
                                }
                            }
                            ui.end_row();
                            for index in 0..self.slots.len() {
                                if let Some(event) = self.slot_row(ui, index) {
                                    events.push(event);
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.horizontal(|ui| {
                let add = ui.add(CarvedButton::new("Add slot").tooltip("Add another creature group row."));
                probe::record(ui.ctx(), "button:Add slot", add.rect);
                if add.clicked() {
                    events.push(RowEvent::Add);
                }
                let clear = ui.add(CarvedButton::new("Clear slots").tooltip("Remove every creature group row."));
                probe::record(ui.ctx(), "button:Clear slots", clear.rect);
                if clear.clicked() {
                    events.push(RowEvent::Clear);
                }
            });
        });
        for event in events {
            match event {
                RowEvent::Add => self.add_slot(SlotSpec::default()),
                RowEvent::Clear => {
                    if self.slots.is_empty() {
                        self.status = "There are no creature slots to clear.".into();
                    } else {
                        self.dialog = Some(Dialog::ClearSlots);
                    }
                }
                RowEvent::Remove(i) => self.remove_slot(i),
                RowEvent::Category(i, c) => self.set_category(i, &c),
                RowEvent::Personality(i, p) => self.set_personality(i, &p),
                RowEvent::Job(i, j) => self.set_job(i, &j),
                RowEvent::Team(i, t) => self.set_team(i, &t),
                RowEvent::EditColors(i) => {
                    let current = self.slots[i].color_overrides.clone();
                    self.dialog = Some(Dialog::Colors { slot: i, current, preset: None });
                }
                RowEvent::EditSkills(i) => {
                    let current = self.catalog.normalize_skill_ids(Some(&self.slots[i].skill_ids));
                    let checks =
                        self.catalog.ability_skills().iter().map(|s| (s.id.clone(), current.contains(&s.id))).collect();
                    self.dialog = Some(Dialog::Abilities { slot: i, checks });
                }
            }
        }
    }

    fn thumbnail(&self, ui: &mut Ui, model: &str, size: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        if model == RANDOM_MODEL_ID {
            art::random_model_icon(ui.painter(), rect);
            return;
        }
        if let Some(texture) = self.thumbnail.as_ref().and_then(|f| f(model, Vec2::splat(size))) {
            ui.painter().image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            return;
        }
        let colors = self.catalog.models.get(model).map(|m| m.colors.clone()).unwrap_or_default();
        art::fallback_model_icon(ui.painter(), rect, &|k| colors.get(k).copied());
    }

    fn slot_row(&mut self, ui: &mut Ui, index: usize) -> Option<RowEvent> {
        let mut event = None;
        let row = self.slots[index].clone();
        // Category
        let mut categories: Vec<(String, String)> = vec![("Any kind".into(), RANDOM_CATEGORY_ID.into())];
        categories.extend(self.catalog.categories().into_iter().map(|p| (body_plan_label(&p), p)));
        let mut category = row.category.clone();
        let r = widgets::combo(ui, ("category", &row.slot_id), 84.0, &mut category, &categories).on_hover_text(
            "The spider's body plan: how it is built and how it walks. Every skin below is one of these four.",
        );
        probe::record(ui.ctx(), format!("slot{index}:category"), r.rect);
        if r.changed() {
            event = Some(RowEvent::Category(index, category));
        }
        // Skin, with thumbnails
        ui.horizontal(|ui| {
            self.thumbnail(ui, &row.model, MODEL_ICON_SIZE);
            let mut skins: Vec<(String, String)> = vec![("Random skin at launch".into(), RANDOM_MODEL_ID.into())];
            let wanted = (row.category != RANDOM_CATEGORY_ID).then_some(row.category.as_str());
            skins.extend(self.catalog.skins_for(wanted).into_iter().map(|m| (m.display_name.clone(), m.id.clone())));
            let selected = skins.iter().find(|(_, id)| *id == row.model).map(|(l, _)| l.clone()).unwrap_or_default();
            let mut chosen = None;
            let combo = egui::ComboBox::from_id_salt(("skin", &row.slot_id))
                .width(170.0)
                .selected_text(RichText::new(selected).color(INK))
                .show_ui(ui, |ui| {
                    for (label, id) in &skins {
                        ui.horizontal(|ui| {
                            self.thumbnail(ui, id, 24.0);
                            if ui.selectable_label(*id == row.model, RichText::new(label).color(INK)).clicked() {
                                chosen = Some(id.clone());
                            }
                        });
                    }
                });
            let _ = combo.response.on_hover_text(
                "Which artwork this slot uses. Skins with their own PNG art keep it; every skin can still be recoloured with the swatch beside it.",
            );
            if let Some(id) = chosen {
                self.slots[index].model = id;
            }
        });
        // Colours
        let palette = self.slot_palette(index);
        let (description, name) = self.swatch_description(index);
        let (rect, swatch) = ui.allocate_exact_size(vec2(34.0, 24.0), Sense::click());
        swatch.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, &name));
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(5),
            if swatch.hovered() { lift22(OAK_LIGHT) } else { OAK_LIGHT },
            egui::Stroke::new(1.0, WALNUT_DEEP),
            egui::StrokeKind::Inside,
        );
        art::swatch_icon(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(SWATCH_ICON_SIZE)),
            &palette,
            !row.color_overrides.is_empty(),
        );
        probe::record(ui.ctx(), format!("slot{index}:colors"), rect);
        if swatch.on_hover_text(description).clicked() {
            event = Some(RowEvent::EditColors(index));
        }
        // Temperament
        let mut temperaments: Vec<(String, String)> =
            vec![("Random personality at launch".into(), RANDOM_PERSONALITY_ID.into())];
        for id in self.catalog.selectable_personality_ids(Some(&row.personality)) {
            if let Some(p) = self.catalog.personalities.get(&id) {
                let label = if p.canonical { p.display_name.clone() } else { format!("Legacy: {}", p.display_name) };
                temperaments.push((label, p.id.clone()));
            }
        }
        let mut personality = row.personality.clone();
        let r = widgets::combo(ui, ("temperament", &row.slot_id), 140.0, &mut personality, &temperaments);
        let tip = self.catalog.personalities.get(&row.personality).map(|p| {
            let values: Vec<String> = p.temperament.iter().map(|(k, v)| format!("{k} {v:.0}/10")).collect();
            format!("{}\n{}", p.description, values.join(", ")).trim().to_owned()
        });
        let r = match tip {
            Some(t) if !t.is_empty() => r.on_hover_text(t),
            _ => r,
        };
        probe::record(ui.ctx(), format!("slot{index}:temperament"), r.rect);
        if r.changed() {
            event = Some(RowEvent::Personality(index, personality));
        }
        // Count
        let mut count = row.count;
        let r = ui.add(egui::DragValue::new(&mut count).range(1..=50).speed(0.1));
        if r.changed() {
            self.slots[index].count = count;
        }
        // Skills
        let (label, tip) = self.skills_label(index);
        let r = ui.add(CarvedButton::new(label).min_size(vec2(70.0, 26.0)).padding(vec2(8.0, 3.0)).tooltip(tip));
        probe::record(ui.ctx(), format!("slot{index}:skills"), r.rect);
        if r.clicked() {
            event = Some(RowEvent::EditSkills(index));
        }
        // Team: a colour and no words.
        let team_tip = self.team_tooltip(index);
        let team_name = teams::team_label(&row.team, &self.team_profiles);
        let team_color = teams::team_color(&row.team, &self.team_profiles);
        let mut options: Vec<(String, String, Rgb)> =
            vec![(teams::default_name("neutral"), "neutral".into(), teams::default_color("neutral"))];
        options.extend(self.team_profiles.values().map(|p| (p.name.clone(), p.id.clone(), p.color)));
        let mut picked = None;
        let icon_probe = format!("slot{index}:team-icon");
        let selected_team = egui::Atom::paint(Vec2::splat(TEAM_ICON_SIZE), move |ui, args| {
            art::team_icon(ui.painter(), args.rect, team_color);
            probe::record(ui.ctx(), &icon_probe, args.rect);
        });
        let combo = egui::ComboBox::from_id_salt(("team", &row.slot_id))
            .width(TEAM_COMBO_WIDTH)
            .selected_text(selected_team)
            .show_ui(ui, |ui| {
                for (name, id, color) in &options {
                    let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 20.0), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::accesskit::Role::ListBoxOption, true, *id == row.team, name)
                    });
                    if resp.hovered() || *id == row.team {
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(3), TABLE_SELECTION);
                    }
                    art::team_icon(
                        ui.painter(),
                        Rect::from_center_size(rect.center(), Vec2::splat(TEAM_ICON_SIZE)),
                        *color,
                    );
                    if resp.on_hover_text(name).clicked() {
                        picked = Some(id.clone());
                    }
                }
                ui.separator();
                if ui.selectable_label(false, RichText::new("New team...").color(INK)).clicked() {
                    picked = Some(NEW_TEAM_SENTINEL.into());
                }
            });
        let team_rect = combo.response.rect;
        combo.response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, format!("Team: {team_name}"))
        });
        let _ = combo.response.on_hover_text(team_tip);
        probe::record(ui.ctx(), format!("slot{index}:team"), team_rect);
        if let Some(team) = picked {
            event = Some(RowEvent::Team(index, team));
        }
        // Job
        let jobs: Vec<(String, String)> =
            self.catalog.jobs.iter().map(|j| (j.display_name.clone(), j.id.clone())).collect();
        let mut job = row.job.clone();
        let r = widgets::combo(ui, ("job", &row.slot_id), 90.0, &mut job, &jobs).on_hover_text(
            "A job is separate from temperament. Builders create a shared base; Guards patrol and protect it from declared foes.",
        );
        if r.changed() {
            event = Some(RowEvent::Job(index, job));
        }
        // Remove
        let (rect, remove) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
        remove.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, "Remove this creature slot")
        });
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(5),
            if remove.hovered() { lift22(OAK_LIGHT) } else { OAK_LIGHT },
            egui::Stroke::new(1.0, WALNUT_DEEP),
            egui::StrokeKind::Inside,
        );
        art::remove_cross(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(12.0)));
        probe::record(ui.ctx(), format!("slot{index}:remove"), rect);
        if remove.on_hover_text("Remove this creature slot").clicked() {
            event = Some(RowEvent::Remove(index));
        }
        event
    }

    fn teams_group(&mut self, ui: &mut Ui) {
        self.ensure_team_profiles();
        Self::group(ui, "teamsGroup", "", |ui| {
            widgets::section_header(
                ui,
                "teams",
                "Factions & relations",
                "Choose who lives together and who competes across the colony.",
            );
            widgets::wrapped(ui, teams::HOSTILITY_NOTE, theme::font(9.0), INK_SOFT);
            if self.team_profiles.is_empty() {
                widgets::wrapped(
                    ui,
                    "No teams yet. Give a creature slot a team above, and it appears here to be named and coloured.",
                    theme::font(9.0),
                    INK,
                );
                return;
            }
            let mut counts: BTreeMap<String, u32> = BTreeMap::new();
            for row in &self.slots {
                if row.team != teams::NEUTRAL {
                    *counts.entry(row.team.clone()).or_default() += row.count;
                }
            }
            let mut renames = Vec::new();
            let mut colours = Vec::new();
            egui::Grid::new("teams-grid").spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                for (id, profile) in &self.team_profiles {
                    let mut color = profile.color;
                    let r = ui.color_edit_button_srgb(&mut color).on_hover_text(format!("Colour for {}", profile.name));
                    if r.changed() {
                        colours.push((id.clone(), color));
                    }
                    let edit = self.team_name_edits.entry(id.clone()).or_insert_with(|| profile.name.clone());
                    let r = ui
                        .add(egui::TextEdit::singleline(edit).char_limit(32).desired_width(180.0))
                        .on_hover_text("What this team is called, wherever it is named.");
                    if r.lost_focus() {
                        renames.push((id.clone(), edit.clone()));
                    }
                    let count = counts.get(id).copied().unwrap_or(0);
                    ui.label(if count > 0 { format!("{count} spider(s)") } else { "no spiders yet".into() });
                    ui.label(format!("id: {id}"));
                    ui.end_row();
                }
            });
            for (id, color) in colours {
                self.set_team_color(&id, color);
            }
            for (id, text) in renames {
                self.rename_team(&id, &text);
                if let Some(p) = self.team_profiles.get(&id) {
                    self.team_name_edits.insert(id.clone(), p.name.clone());
                }
            }
            let pairs = teams::stance_pairs(&self.team_profiles, &self.team_stances);
            if pairs.is_empty() {
                widgets::wrapped(ui, "Add a second team to choose what stands between them.", theme::font(9.0), INK);
                return;
            }
            widgets::text(ui, "Between teams", theme::bold(9.0), INK);
            let mut changes = Vec::new();
            egui::Grid::new("stances-grid").spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                for (left, right, relation) in &pairs {
                    ui.label(format!(
                        "{} and {}",
                        teams::team_label(left, &self.team_profiles),
                        teams::team_label(right, &self.team_profiles)
                    ));
                    let options: Vec<(String, String)> =
                        teams::RELATIONS.iter().map(|r| (teams::stance_label(r).to_owned(), (*r).to_owned())).collect();
                    let mut value = relation.clone();
                    let r = widgets::combo(ui, ("stance", left, right), 140.0, &mut value, &options)
                        .on_hover_text(teams::describe_stance(left, right, relation, &self.team_profiles));
                    probe::record(ui.ctx(), format!("stance:{left}:{right}"), r.rect);
                    if r.changed() {
                        changes.push((left.clone(), right.clone(), value));
                    }
                    ui.end_row();
                }
            });
            for (l, r, v) in changes {
                self.set_stance(&l, &r, &v);
            }
        });
    }

    fn behavior_group(&mut self, ui: &mut Ui) {
        Self::group(ui, "behaviorGroup", "Tune global overlay behavior for all launched creatures.", |ui| {
            widgets::section_header(
                ui,
                "behavior",
                "Desktop behavior",
                "Set spider size, dragging, and the labels that stay visible.",
            );
            ui.horizontal(|ui| {
                ui.label("Size:");
                let options: Vec<(String, usize)> =
                    SIZE_OPTIONS.iter().enumerate().map(|(i, (l, _))| ((*l).to_owned(), i)).collect();
                widgets::combo(ui, "size-combo", 150.0, &mut self.size_index, &options)
                    .on_hover_text("Scale all creatures in the overlay.");
                let r = widgets::check(ui, &mut self.interferable, "Draggable").on_hover_text(
                    "When enabled, you can grab spiders; empty overlay space still remains click-through.",
                );
                probe::record(ui.ctx(), "check:Draggable", r.rect);
            });
            ui.horizontal(|ui| {
                ui.label("Always show:");
                ui.spacing_mut().item_spacing.x = 12.0;
                // Names, Levels, Health, XP, Stamina -- the Qt order.
                for key in [
                    "always_show_names",
                    "always_show_levels",
                    "always_show_health",
                    "always_show_xp",
                    "always_show_stamina",
                ] {
                    let (_, label, tip) =
                        LABEL_SWITCHES.iter().find(|(k, _, _)| *k == key).copied().unwrap_or_default();
                    let value = self.label_switches.entry(key.into()).or_insert(false);
                    let r = widgets::check(ui, value, label).on_hover_text(tip);
                    probe::record(ui.ctx(), format!("check:{label}"), r.rect);
                }
            });
        });
    }

    fn flies_group(&mut self, ui: &mut Ui) {
        Self::group(
            ui,
            "fliesGroup",
            "Flies are prey that buzz around the screen; the spiders hunt, trap, and eat them.",
            |ui| {
                widgets::section_header(
                    ui,
                    "flies",
                    "Prey & food",
                    "Release flies for hunters to chase, trap, and eat.",
                );
                let r = widgets::check(ui, &mut self.flies.enabled, "Spawn flies for the spiders to hunt")
                    .on_hover_text("Turn the fly spawner on or off. With flies off the spiders ignore prey.");
                probe::record(ui.ctx(), "check:flies", r.rect);
                // The timing, count and nest only matter once flies are on.
                if self.flies.enabled {
                    egui::Grid::new("flies-grid").spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                        ui.label("Spawn every (min):");
                        let min_changed = ui
                            .add(
                                egui::DragValue::new(&mut self.flies.min_interval)
                                    .range(0.3..=120.0)
                                    .speed(0.5)
                                    .fixed_decimals(1)
                                    .suffix(" s"),
                            )
                            .on_hover_text("Shortest gap between fly spawns. Set equal to the max for a fixed timer.")
                            .changed();
                        ui.label("to (max):");
                        let max_changed = ui
                            .add(
                                egui::DragValue::new(&mut self.flies.max_interval)
                                    .range(0.4..=240.0)
                                    .speed(0.5)
                                    .fixed_decimals(1)
                                    .suffix(" s"),
                            )
                            .on_hover_text(
                                "Longest gap between fly spawns. Each spawn waits a random time in this range.",
                            )
                            .changed();
                        // Keep the range valid: the other end follows.
                        if min_changed && self.flies.max_interval < self.flies.min_interval {
                            self.flies.max_interval = self.flies.min_interval;
                        }
                        if max_changed && self.flies.max_interval < self.flies.min_interval {
                            self.flies.min_interval = self.flies.max_interval;
                        }
                        ui.end_row();
                        ui.label("Max flies at once:");
                        ui.add(egui::DragValue::new(&mut self.flies.max_flies).range(0..=40))
                            .on_hover_text("How many live flies may share the screen at once.");
                        ui.end_row();
                    });
                    widgets::check(ui, &mut self.flies.spawner, "Flies emerge from a movable nest object").on_hover_text(
                    "When on, flies crawl out of a nest you can drag around the screen. When off, they drift in from the screen edges.",
                );
                }
            },
        );
    }

    fn launch_group(&mut self, ui: &mut Ui) {
        let summary = self.summary();
        Self::group(ui, "launchGroup", &summary, |ui| {
            widgets::section_header(
                ui,
                "launch",
                "Ready the colony",
                "Save this setup, then start or stop the desktop overlay.",
            );
            ui.horizontal(|ui| {
                let open = ui.add(CarvedButton::new("Open project folder").tooltip("Open the project folder where presets, models, and personalities live."));
                if open.clicked() {
                    self.pending.push(UiAction::OpenProjectFolder);
                }
                let reset = ui.add(CarvedButton::new("Reset saved progress...").tooltip(
                    "Forget every spider's level, XP and name, and remove all bases. Presets, settings, cages and webs are kept.",
                ));
                probe::record(ui.ctx(), "button:Reset saved progress...", reset.rect);
                if reset.clicked() {
                    self.dialog = Some(Dialog::ResetProgress);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let launch = ui.add(
                        CarvedButton::new("Save and launch overlay")
                            .brass()
                            .padding(vec2(16.0, 7.0))
                            .tooltip(format!("{summary} Start the overlay, or apply changes live if it is already running.")),
                    );
                    probe::record(ui.ctx(), "button:Save and launch overlay", launch.rect);
                    if launch.clicked() {
                        self.launch(OverlayMode::Companion);
                    }
                    let stop = ui.add(
                        CarvedButton::new("Stop overlay").look(ButtonLook::Stop).tooltip("Stop the overlay process launched from this window."),
                    );
                    probe::record(ui.ctx(), "button:Stop overlay", stop.rect);
                    if stop.clicked() {
                        self.pending.push(UiAction::StopOverlay);
                    }
                });
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else { return };
        let mut close = false;
        let mut replace: Option<Dialog> = None;
        let frame = theme::dialog_frame();
        let modal = egui::Modal::new(egui::Id::unique("config-dialog")).frame(frame);
        modal.show(ctx, |ui| {
            *ui.visuals_mut() = theme::dialog_visuals();
            match dialog {
                Dialog::Message { title, text } => {
                    widgets::brass_title(ui, &title, 13.0);
                    widgets::wrapped(ui, &text, theme::font(9.5), CREAM);
                    if ui.add(CarvedButton::new("OK")).clicked() {
                        close = true;
                    }
                }
                Dialog::ClearSlots => {
                    widgets::brass_title(ui, "Clear creature slots?", 13.0);
                    widgets::text(ui, "Remove every creature slot from this preset?", theme::font(9.5), CREAM);
                    ui.horizontal(|ui| {
                        let yes = ui.add(CarvedButton::new("Yes"));
                        probe::record(ui.ctx(), "dialog:Yes", yes.rect);
                        if yes.clicked() {
                            self.clear_slots();
                            close = true;
                        }
                        let no = ui.add(CarvedButton::new("No"));
                        probe::record(ui.ctx(), "dialog:No", no.rect);
                        if no.clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::ResetProgress => {
                    widgets::brass_title(ui, "Reset saved progress", 13.0);
                    widgets::wrapped(
                        ui,
                        "Forget every spider's level, XP and name, and remove all bases?\n\nPresets, settings, cages and webs are kept. This cannot be undone.",
                        theme::font(9.5),
                        CREAM,
                    );
                    ui.horizontal(|ui| {
                        let yes = ui.add(CarvedButton::new("Yes"));
                        probe::record(ui.ctx(), "dialog:Yes", yes.rect);
                        if yes.clicked() {
                            self.pending.push(UiAction::ResetProgress);
                            close = true;
                        }
                        let no = ui.add(CarvedButton::new("No").brass());
                        probe::record(ui.ctx(), "dialog:No", no.rect);
                        if no.clicked() {
                            self.status = String::new();
                            close = true;
                        }
                    });
                }
                Dialog::NewTeam { slot, mut name } => {
                    widgets::brass_title(ui, "New team", 13.0);
                    widgets::text(ui, "Name this team (for example: Porch guard)", theme::font(9.5), CREAM);
                    let edit = ui.add(egui::TextEdit::singleline(&mut name).desired_width(260.0));
                    edit.request_focus();
                    ui.horizontal(|ui| {
                        let ok = ui.add(CarvedButton::new("OK"));
                        probe::record(ui.ctx(), "dialog:OK", ok.rect);
                        if ok.clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                            if self.create_team(slot, &name).is_none() {
                                // Cancelled: the slot keeps its previous choice.
                            }
                            close = true;
                        } else if ui.add(CarvedButton::new("Cancel")).clicked() {
                            close = true;
                        } else {
                            replace = Some(Dialog::NewTeam { slot, name: name.clone() });
                        }
                    });
                }
                Dialog::Abilities { slot, mut checks } => {
                    widgets::brass_title(ui, "Choose spider abilities", 13.0);
                    widgets::text(ui, "Abilities", theme::bold(9.5), CREAM);
                    for (id, on) in checks.iter_mut() {
                        let skill = self.catalog.skills.iter().find(|s| &s.id == id).cloned().unwrap_or_default();
                        let r = widgets::check(ui, on, &skill.display_name).on_hover_text(&skill.description);
                        probe::record(ui.ctx(), format!("ability:{id}"), r.rect);
                    }
                    let mut done = None;
                    ui.horizontal(|ui| {
                        if ui.add(CarvedButton::new("All")).clicked() {
                            checks.iter_mut().for_each(|(_, on)| *on = true);
                        }
                        if ui.add(CarvedButton::new("None")).clicked() {
                            checks.iter_mut().for_each(|(_, on)| *on = false);
                        }
                        let ok = ui.add(CarvedButton::new("OK").brass());
                        probe::record(ui.ctx(), "dialog:OK", ok.rect);
                        if ok.clicked() {
                            done = Some(true);
                        }
                        if ui.add(CarvedButton::new("Cancel")).clicked() {
                            done = Some(false);
                        }
                    });
                    match done {
                        Some(true) => {
                            let selected = checks.iter().filter(|(_, on)| *on).map(|(id, _)| id.clone()).collect();
                            self.set_abilities(slot, selected);
                            close = true;
                        }
                        Some(false) => close = true,
                        None => replace = Some(Dialog::Abilities { slot, checks }),
                    }
                }
                Dialog::Colors { slot, mut current, mut preset } => {
                    widgets::brass_title(ui, "Customize spider colors", 13.0);
                    widgets::text(
                        ui,
                        "Choose colors for this creature slot. Unchanged fields use the selected model's defaults.",
                        theme::font(9.0),
                        CREAM,
                    );
                    let model = self.slots.get(slot).map(|r| r.model.clone()).unwrap_or_default();
                    let defaults = self.catalog.model_color_defaults(Some(&model), RANDOM_MODEL_ID);
                    ui.horizontal(|ui| {
                        ui.label("Preset:");
                        let mut options: Vec<(String, Option<String>)> = vec![("Pick a palette\u{2026}".into(), None)];
                        options.extend(self.catalog.palettes.iter().map(|p| (p.name.clone(), Some(p.id.clone()))));
                        options.push(("Surprise me".into(), Some(RANDOM_PALETTE_ID.into())));
                        let r = widgets::combo(ui, "palette-preset", 180.0, &mut preset, &options)
                            .on_hover_text("Fill every colour below from a ready-made palette. You can still change any single colour afterwards.");
                        if r.changed()
                            && let Some(id) = preset.clone()
                        {
                            let chosen = if id == RANDOM_PALETTE_ID {
                                crate::model::random_palette(&mut self.rng)
                            } else {
                                self.catalog.palette(&id).map(|p| p.colors.clone()).unwrap_or_default()
                            };
                            current.extend(chosen);
                        }
                    });
                    let mut keys: Vec<(String, String)> = COLOR_KEYS.iter().map(|(k, l)| ((*k).into(), (*l).into())).collect();
                    for key in defaults.keys() {
                        if !keys.iter().any(|(k, _)| k == key) {
                            keys.push((key.clone(), teams::title_case(&key.replace('_', " "))));
                        }
                    }
                    egui::Grid::new("color-keys").show(ui, |ui| {
                        for (key, label) in &keys {
                            ui.label(format!("{label}:"));
                            let mut rgb = current.get(key).or_else(|| defaults.get(key)).copied().unwrap_or([80, 70, 70]);
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                current.insert(key.clone(), rgb);
                            }
                            ui.label(if current.contains_key(key) { "Custom" } else { "Model default" });
                            ui.end_row();
                        }
                    });
                    let mut done = None;
                    ui.horizontal(|ui| {
                        let ok = ui.add(CarvedButton::new("OK").brass());
                        probe::record(ui.ctx(), "dialog:OK", ok.rect);
                        if ok.clicked() {
                            done = Some(true);
                        }
                        if ui.add(CarvedButton::new("Cancel")).clicked() {
                            done = Some(false);
                        }
                        if ui.add(CarvedButton::new("Reset to model defaults")).clicked() {
                            current.clear();
                            // Otherwise the picker still names the palette just cleared.
                            preset = None;
                        }
                    });
                    match done {
                        Some(true) => {
                            self.set_colors(slot, current);
                            close = true;
                        }
                        Some(false) => close = true,
                        None => replace = Some(Dialog::Colors { slot, current, preset }),
                    }
                }
            }
        });
        if close {
            self.dialog = None;
        } else if let Some(d) = replace {
            self.dialog = Some(d);
        }
    }

    /// Whether a modal dialog is open (tests).
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }
}

fn closest_size(scale: f64) -> usize {
    let mut best = 0;
    let mut distance = f64::INFINITY;
    for (i, (_, s)) in SIZE_OPTIONS.iter().enumerate() {
        let d = (s - scale).abs();
        if d < distance {
            distance = d;
            best = i;
        }
    }
    best
}

/// `_fill_skin_box`: the skin for a category, keeping the old one when it
/// belongs, else "spider", else the category's first real skin (never
/// quietly "Random").
pub fn fill_skin(catalog: &SettingsCatalog, category: &str, keep: Option<&str>) -> String {
    let wanted = (category != RANDOM_CATEGORY_ID).then_some(category);
    let skins: Vec<String> = catalog.skins_for(wanted).into_iter().map(|m| m.id.clone()).collect();
    for candidate in [keep, Some("spider")].into_iter().flatten() {
        if candidate == RANDOM_MODEL_ID || skins.iter().any(|s| s == candidate) {
            return candidate.to_owned();
        }
    }
    skins.first().cloned().unwrap_or_else(|| RANDOM_MODEL_ID.into())
}

#[derive(Clone, Debug)]
enum RowEvent {
    Add,
    Clear,
    Remove(usize),
    Category(usize, String),
    Personality(usize, String),
    Job(usize, String),
    Team(usize, String),
    EditColors(usize),
    EditSkills(usize),
}

/// The arguments of `add_slot`.
#[derive(Clone, Debug)]
pub struct SlotSpec {
    pub model: Option<String>,
    pub personality: Option<String>,
    pub count: u32,
    pub count_random: bool,
    pub skills: Option<Vec<String>>,
    pub abilities: Option<Vec<String>>,
    pub colors: BTreeMap<String, Rgb>,
    pub slot_id: Option<String>,
    pub team: Option<String>,
    pub job: Option<String>,
}

impl Default for SlotSpec {
    fn default() -> Self {
        Self {
            model: None,
            personality: None,
            count: 1,
            count_random: false,
            skills: None,
            abilities: None,
            colors: BTreeMap::new(),
            slot_id: None,
            team: None,
            job: None,
        }
    }
}

impl SlotSpec {
    /// A preset slot (`slot.get(...)` in `load_preset_path`).
    pub fn from_json(slot: &Value) -> Self {
        let s = |k: &str| slot.get(k).and_then(Value::as_str).map(str::to_owned);
        let list = |k: &str| {
            slot.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        };
        Self {
            model: s("model"),
            personality: s("personality"),
            count: slot.get("count").and_then(Value::as_f64).unwrap_or(1.0).max(1.0) as u32,
            count_random: slot.get("count_random").and_then(Value::as_bool).unwrap_or(false),
            skills: list("skills"),
            abilities: list("abilities"),
            colors: normalize_color_overrides(slot.get("colors")),
            slot_id: s("slot_id"),
            team: s("team_id").or_else(|| s("team")),
            job: s("job"),
        }
    }
}
