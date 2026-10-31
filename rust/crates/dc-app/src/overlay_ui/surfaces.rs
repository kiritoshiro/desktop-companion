//! The UI layer without windows: which menus and dialogs are open, the
//! egui of each one, and the flows between them (`app/engine.py`:
//! `_show_context_menu`, `_prompt_name`, `_show_inspector`,
//! `_show_adventure_pause`, `_open_character_window`). Each open thing is a
//! [`Surface`] with its own `egui::Context`; the winit shell gives each one a
//! window and calls [`UiCore::draw`] from that window's frame, and tests call
//! it from a headless `Context::run_pass`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dc_adventure::adventure_profile::{AdventureProfile, load_profile, profile_path, save_profile};
use dc_adventure::controls::{controls_path, load_controls, save_controls};
use dc_manager::CreatureManager;
use dc_ui::UiAction;
use dc_ui::windows::character::CharacterWindow;
use dc_ui::windows::inspector::{CreatureInspector, InspectorTab, InspectorView};
use dc_ui::windows::menu::WoodMenu;
use dc_ui::windows::prompt::{PromptAnswer, TextPrompt};
use dc_ui::windows::{AdventureSettingsDialog, ControlsDialog, PauseDialog};
use egui::{Key, Vec2, vec2};

use super::context_menu::{Choice, MenuContext};
use super::live::{self, PORTRAIT, PortraitPainter, PortraitSlot};
use super::{AdventureContext, CreatureKey, EngineCommand, PlayerIndex, UiRequest};

/// `CreatureInspectorDialog`'s refresh timer.
pub const INSPECTOR_REFRESH: Duration = Duration::from_millis(400);

pub type SurfaceId = u64;

/// Where a surface's window goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    /// A pop-up whose top-left corner is at this desktop point (physical px).
    AtPoint(f64, f64),
    /// Centred on the monitor holding this desktop point, or the main one.
    Centered(Option<(f64, f64)>),
}

/// How a surface is sized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// The content says (menus): [`Surface::size`] is updated every frame.
    Content,
    /// This wide; the height is measured.
    FitHeight(f32),
    /// A dialog of this size (points), shrunk to fit the monitor.
    Fixed(f32, f32),
}

/// One menu or dialog.
pub struct Surface {
    pub id: SurfaceId,
    pub ctx: egui::Context,
    pub title: String,
    pub placement: Placement,
    pub sizing: Sizing,
    /// The size the content wants, in points.
    pub size: Vec2,
    /// A pop-up menu (no frame, closes when clicked outside).
    pub popup: bool,
    /// Filled from the scene (a menu or inspector waits for a `sync`).
    pub ready: bool,
    /// New data since the last frame (the window should redraw).
    pub dirty: bool,
    pub view: View,
}

/// What a surface shows.
pub enum View {
    Menu {
        menu: Box<WoodMenu>,
        context: Option<Box<MenuContext>>,
        creature: Option<CreatureKey>,
        at: (f64, f64),
    },
    Prompt {
        prompt: TextPrompt,
        creature: CreatureKey,
    },
    Inspector {
        inspector: Box<CreatureInspector>,
        creature: CreatureKey,
        portrait: Rc<RefCell<PortraitSlot>>,
        refreshed: Option<Instant>,
        then_pause: bool,
    },
    Pause(PauseDialog),
    Character {
        window: Box<CharacterWindow>,
        profile: Box<AdventureProfile>,
        path: PathBuf,
        then_pause: bool,
    },
    Controls(Box<ControlsDialog>),
    Settings {
        dialog: Box<AdventureSettingsDialog>,
        accepted_fps: Option<u32>,
    },
}

impl View {
    pub fn kind(&self) -> &'static str {
        match self {
            View::Menu { .. } => "menu",
            View::Prompt { .. } => "prompt",
            View::Inspector { .. } => "inspector",
            View::Pause(_) => "pause",
            View::Character { .. } => "character",
            View::Controls(_) => "controls",
            View::Settings { .. } => "settings",
        }
    }
}

/// How a surface ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Done, a choice, OK.
    Done,
    /// Esc, the window's close button, a click outside.
    Dismissed,
}

/// A window that opens once the engine has taken the commands asked for
/// first (the raid's progress is saved before the character window reads
/// the profile).
enum Pending {
    Character { player: PlayerIndex, then_pause: bool },
}

/// The UI layer's state.
pub struct UiCore {
    pub surfaces: Vec<Surface>,
    commands: Vec<EngineCommand>,
    /// Bumped by every `take_commands`.
    generation: u64,
    pending: Vec<(u64, Pending)>,
    next_id: SurfaceId,
    pub origin: (i32, i32),
    pub adventure: AdventureContext,
    pub state_dir: PathBuf,
    /// `_moving_base_id`.
    pub carried_base: Option<String>,
    portraits: Option<PortraitPainter>,
    /// Paint portraits (off in tests that do not need them).
    pub paint_portraits: bool,
}

impl UiCore {
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            surfaces: Vec::new(),
            commands: Vec::new(),
            generation: 0,
            pending: Vec::new(),
            next_id: 1,
            origin: (0, 0),
            adventure: AdventureContext::default(),
            state_dir,
            carried_base: None,
            portraits: None,
            paint_portraits: true,
        }
    }

    pub fn is_open(&self) -> bool {
        !self.surfaces.is_empty() || !self.pending.is_empty()
    }

    pub fn take_commands(&mut self) -> Vec<EngineCommand> {
        self.generation += 1;
        std::mem::take(&mut self.commands)
    }

    pub fn surface(&self, id: SurfaceId) -> Option<&Surface> {
        self.surfaces.iter().find(|s| s.id == id)
    }

    pub fn surface_mut(&mut self, id: SurfaceId) -> Option<&mut Surface> {
        self.surfaces.iter_mut().find(|s| s.id == id)
    }

    /// The id of the open surface of a kind (`"menu"`, `"pause"`, ...).
    pub fn find(&self, kind: &str) -> Option<SurfaceId> {
        self.surfaces.iter().find(|s| s.view.kind() == kind).map(|s| s.id)
    }

    fn push(&mut self, title: impl Into<String>, placement: Placement, sizing: Sizing, view: View) -> SurfaceId {
        let id = self.next_id;
        self.next_id += 1;
        let ctx = egui::Context::default();
        dc_ui::theme::apply_app_theme(&ctx);
        let popup = matches!(view, View::Menu { .. });
        let ready = !matches!(view, View::Menu { .. } | View::Inspector { .. });
        let size = match sizing {
            Sizing::Fixed(w, h) => vec2(w, h),
            Sizing::FitHeight(w) => vec2(w, 200.0),
            Sizing::Content => vec2(dc_ui::windows::menu::MIN_WIDTH, 100.0),
        };
        let view = match view {
            View::Inspector { mut inspector, creature, portrait, refreshed, then_pause } => {
                inspector.set_portrait(live::portrait_fn(&ctx, portrait.clone()));
                inspector.set_icons(live::armour_icon_fn(&ctx));
                View::Inspector { inspector, creature, portrait, refreshed, then_pause }
            }
            View::Character { mut window, profile, path, then_pause } => {
                match window.as_mut() {
                    CharacterWindow::Single(d) => d.set_icons(live::armour_icon_fn(&ctx)),
                    CharacterWindow::Party(d) => d.set_icons(live::armour_icon_fn(&ctx)),
                }
                View::Character { window, profile, path, then_pause }
            }
            other => other,
        };
        self.surfaces.push(Surface {
            id,
            ctx,
            title: title.into(),
            placement,
            sizing,
            size,
            popup,
            ready,
            dirty: false,
            view,
        });
        id
    }

    fn close_kind(&mut self, kind: &str) {
        self.surfaces.retain(|s| s.view.kind() != kind);
    }

    /// Ask for something to be shown.
    pub fn request(&mut self, request: UiRequest) {
        match request {
            UiRequest::SpiderMenu { creature, at } => self.open_menu(Some(creature), at),
            UiRequest::OverlayMenu { at } => self.open_menu(None, at),
            UiRequest::Inspector { creature } => self.open_inspector(creature, InspectorTab::Overview, false, None),
            UiRequest::Pause => {
                // `if self._adventure_paused: return`.
                let busy = self.surfaces.iter().any(|s| {
                    matches!(s.view, View::Pause(_) | View::Character { .. } | View::Settings { .. })
                        || matches!(s.view, View::Inspector { then_pause: true, .. })
                }) || !self.pending.is_empty();
                if !busy {
                    self.open_pause();
                }
            }
            UiRequest::Character { player } => self.open_character(player, false),
            UiRequest::Controls => {
                if self.find("controls").is_none() {
                    let settings = load_controls(&controls_path(&self.state_dir));
                    let dialog = ControlsDialog::new((&settings).into());
                    self.push(
                        ControlsDialog::title(),
                        Placement::Centered(None),
                        Sizing::Fixed(640.0, 640.0),
                        View::Controls(Box::new(dialog)),
                    );
                }
            }
            UiRequest::CloseAll => {
                self.surfaces.clear();
                self.pending.clear();
            }
        }
    }

    fn open_menu(&mut self, creature: Option<CreatureKey>, at: (f64, f64)) {
        // One menu at a time; a new right-click replaces it.
        self.close_kind("menu");
        self.close_kind("prompt");
        self.push(
            "Desktop Bug Companion",
            Placement::AtPoint(at.0, at.1),
            Sizing::Content,
            View::Menu { menu: Box::new(WoodMenu::new(Vec::new())), context: None, creature, at },
        );
    }

    fn open_inspector(&mut self, creature: CreatureKey, tab: InspectorTab, then_pause: bool, near: Option<(f64, f64)>) {
        if let Some(existing) = self
            .surfaces
            .iter()
            .find(|s| matches!(&s.view, View::Inspector { creature: c, .. } if *c == creature))
            .map(|s| s.id)
        {
            if let Some(View::Inspector { inspector, .. }) = self.surface_mut(existing).map(|s| &mut s.view) {
                inspector.tab = tab;
            }
            return;
        }
        let mut inspector = CreatureInspector::new(
            InspectorView { key: creature.clone(), alive: true, ..Default::default() },
            dc_ui::bridge::progression_catalog(),
        );
        inspector.tab = tab;
        self.push(
            "Inspect",
            Placement::Centered(near),
            Sizing::Fixed(1200.0, 780.0),
            View::Inspector {
                inspector: Box::new(inspector),
                creature,
                portrait: Rc::new(RefCell::new(PortraitSlot::default())),
                refreshed: None,
                then_pause,
            },
        );
    }

    fn open_pause(&mut self) {
        let dialog = PauseDialog::new(self.adventure.in_mission);
        self.push(PauseDialog::title(), Placement::Centered(None), Sizing::FitHeight(344.0), View::Pause(dialog));
    }

    /// `_open_character_window`: in a raid, save its progress first and
    /// open the party windows on the profile; outside one, the inspector's
    /// skill tree for the player's spider.
    fn open_character(&mut self, player: PlayerIndex, then_pause: bool) {
        if self.find("character").is_some() || !self.pending.is_empty() {
            return;
        }
        if !self.adventure.in_mission {
            match self.adventure.player_creatures.get(player).or(self.adventure.player_creatures.first()).cloned() {
                Some(key) => self.open_inspector(key, InspectorTab::Skills, then_pause, None),
                None if then_pause => self.open_pause(),
                None => {}
            }
            return;
        }
        self.commands.push(EngineCommand::SaveProgress);
        self.pending.push((self.generation, Pending::Character { player, then_pause }));
    }

    /// Open what was waiting for the engine to take its commands.
    pub fn promote_pending(&mut self) {
        let generation = self.generation;
        let (ready, waiting): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.pending).into_iter().partition(|(g, _)| *g < generation);
        self.pending = waiting;
        for (_, pending) in ready {
            match pending {
                Pending::Character { player, then_pause } => {
                    let path = profile_path(&self.state_dir);
                    let profile = load_profile(&path);
                    let two_player = self.adventure.players > 1;
                    let window = CharacterWindow::open(
                        dc_ui::bridge::party_view(&profile),
                        dc_ui::bridge::progression_catalog(),
                        Some(two_player),
                    );
                    // Either player's key opens the same window (`_show_adventure_skills`);
                    // the party window shows both sheets.
                    let _ = player;
                    let (title, sizing) = match &window {
                        CharacterWindow::Single(_) => {
                            (dc_ui::windows::CharacterDialog::title(), Sizing::Fixed(760.0, 940.0))
                        }
                        CharacterWindow::Party(_) => {
                            (dc_ui::windows::PartyDialog::title(), Sizing::Fixed(1180.0, 940.0))
                        }
                    };
                    self.push(
                        title,
                        Placement::Centered(None),
                        sizing,
                        View::Character { window: Box::new(window), profile: Box::new(profile), path, then_pause },
                    );
                }
            }
        }
    }

    /// Read the scene into whatever is waiting for it: a new menu's
    /// entries, the inspectors (every 400 ms) and their portraits.
    pub fn sync(&mut self, manager: &CreatureManager) {
        let now = Instant::now();
        let origin = self.origin;
        // `_carried_base`: a base removed while carried is forgotten.
        if let Some(id) = &self.carried_base
            && !manager.base_world.bases.iter().any(|s| &s.id == id)
        {
            self.carried_base = None;
        }
        let mut portraits_wanted: Vec<(CreatureKey, Rc<RefCell<PortraitSlot>>)> = Vec::new();
        for s in &mut self.surfaces {
            match &mut s.view {
                View::Menu { menu, context, creature, at } if context.is_none() => {
                    let local = (at.0 - origin.0 as f64, at.1 - origin.1 as f64);
                    let ctx =
                        MenuContext::from_manager(manager, creature.as_deref(), local, self.carried_base.as_deref());
                    **menu = WoodMenu::new(ctx.entries());
                    *context = Some(Box::new(ctx));
                    s.ready = true;
                }
                View::Inspector { inspector, creature, portrait, refreshed, .. }
                    if refreshed.is_none_or(|t| now.duration_since(t) >= INSPECTOR_REFRESH) =>
                {
                    *refreshed = Some(now);
                    let view = live::inspector_view(manager, creature);
                    s.title = format!("Inspect {}", view.display_name);
                    inspector.set_view(view);
                    s.ready = true;
                    s.dirty = true;
                    portraits_wanted.push((creature.clone(), portrait.clone()));
                }
                _ => {}
            }
        }
        if self.paint_portraits && !portraits_wanted.is_empty() {
            let painter = self.portraits.get_or_insert_with(PortraitPainter::new);
            for (key, slot) in portraits_wanted {
                if let Some(c) = manager.creatures.iter().find(|c| c.progression_id == key) {
                    let image = painter.portrait(c, PORTRAIT);
                    slot.borrow_mut().set(live::color_image(&image));
                }
            }
        }
    }

    /// Run one egui pass of surface `id` on `ctx` (its own context) and act
    /// on what it produced.
    pub fn draw(&mut self, id: SurfaceId, ctx: &egui::Context) {
        let Some(surface) = self.surfaces.iter_mut().find(|s| s.id == id) else { return };
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let sizing = surface.sizing;
        let mut outcome = Outcome::Keep;
        match &mut surface.view {
            View::Menu { menu, context, .. } => {
                if context.is_some() {
                    let out = menu.ui(ctx);
                    surface.size = out.size;
                    if out.dismissed {
                        outcome = Outcome::Finish(Finish::Dismissed);
                    } else if let Some(chosen) = out.chosen {
                        outcome = Outcome::Menu(chosen, out.checked);
                    }
                }
            }
            View::Prompt { prompt, .. } => {
                let (answer, used) = ctx.root_ui(|ui| (prompt.show(ui), ui.min_rect().size()));
                fit_height(&mut surface.size, sizing, used);
                match answer {
                    Some(PromptAnswer::Accepted(text)) => outcome = Outcome::Prompt(text),
                    Some(PromptAnswer::Cancelled) => outcome = Outcome::Finish(Finish::Dismissed),
                    None => {}
                }
            }
            View::Inspector { inspector, .. } => {
                if escape {
                    outcome = Outcome::Finish(Finish::Dismissed);
                } else {
                    outcome = Outcome::Actions(inspector.ui(ctx));
                }
            }
            View::Pause(dialog) => {
                if escape {
                    outcome = Outcome::Finish(Finish::Dismissed);
                } else {
                    let (actions, used) = ctx.root_ui(|ui| (dialog.show(ui), ui.min_rect().size()));
                    fit_height(&mut surface.size, sizing, used);
                    outcome = Outcome::Actions(actions);
                }
            }
            View::Character { window, .. } => {
                if escape {
                    outcome = Outcome::Finish(Finish::Dismissed);
                } else {
                    outcome = Outcome::Actions(window.ui(ctx));
                }
            }
            View::Controls(dialog) => {
                // Esc first cancels a binding being listened for.
                if escape && dialog.editor.listening.is_none() {
                    outcome = Outcome::Finish(Finish::Dismissed);
                } else {
                    outcome = Outcome::Actions(dialog.ui(ctx));
                }
            }
            View::Settings { dialog, .. } => {
                if escape && dialog.controls.listening.is_none() {
                    outcome = Outcome::Finish(Finish::Dismissed);
                } else {
                    // Taller than a small monitor: the board scrolls (the Qt
                    // dialog grew past the screen instead).
                    let (actions, used) = ctx.root_ui(|ui| {
                        dc_ui::widgets::wood_board(ui, ui.max_rect(), true);
                        let out =
                            egui::ScrollArea::vertical().id_salt("adventure-settings").show(ui, |ui| dialog.show(ui));
                        (out.inner, out.content_size)
                    });
                    fit_height(&mut surface.size, sizing, used);
                    outcome = Outcome::Actions(actions);
                }
            }
        }
        self.act(id, outcome);
    }

    /// The window was closed from outside the egui (its close button, a
    /// click elsewhere, focus lost): as Esc.
    pub fn dismiss(&mut self, id: SurfaceId) {
        self.finish(id, Finish::Dismissed);
    }

    fn act(&mut self, id: SurfaceId, outcome: Outcome) {
        match outcome {
            Outcome::Keep => {}
            Outcome::Finish(how) => self.finish(id, how),
            Outcome::Menu(chosen, checked) => {
                let Some(surface) = self.surface(id) else { return };
                let View::Menu { context: Some(context), at, .. } = &surface.view else { return };
                let choice = context.choose(&chosen, checked);
                let at = *at;
                self.finish(id, Finish::Done);
                self.apply_choice(choice, at);
            }
            Outcome::Prompt(text) => {
                let creature = match self.surface(id).map(|s| &s.view) {
                    Some(View::Prompt { creature, .. }) => creature.clone(),
                    _ => return,
                };
                self.finish(id, Finish::Done);
                self.commands.push(EngineCommand::SpiderAction { creature, action: "rename".into(), arg: Some(text) });
            }
            Outcome::Actions(actions) => {
                for action in actions {
                    if self.surface(id).is_none() {
                        break;
                    }
                    self.act_ui(id, action);
                }
            }
        }
    }

    fn apply_choice(&mut self, choice: Choice, at: (f64, f64)) {
        match choice {
            Choice::Nothing => {}
            Choice::Commands(commands) => self.commands.extend(commands),
            Choice::Inspect(key) => self.open_inspector(key, InspectorTab::Overview, false, Some(at)),
            Choice::PromptName { creature, current } => {
                self.push(
                    "Name this spider",
                    Placement::Centered(Some(at)),
                    Sizing::FitHeight(340.0),
                    View::Prompt { prompt: TextPrompt::spider_name(&current), creature },
                );
            }
            Choice::PickUpBase(base) => {
                self.carried_base = Some(base.id);
                self.commands.push(EngineCommand::Announce(format!(
                    "Picked up the {} base. Right-click where it should go.",
                    base.team
                )));
            }
            Choice::CancelBaseMove => {
                self.carried_base = None;
                self.commands.push(EngineCommand::Announce("Left the base where it is.".into()));
            }
            Choice::DropBase(commands) => {
                self.carried_base = None;
                self.commands.extend(commands);
            }
        }
    }

    /// One action of a dialog.
    fn act_ui(&mut self, id: SurfaceId, action: UiAction) {
        if action == UiAction::CloseWindow {
            // The pause menu's choice already said what closing means.
            if !matches!(self.surface(id).map(|s| &s.view), Some(View::Pause(_))) {
                self.finish(id, Finish::Done);
            }
            return;
        }
        let Some(surface) = self.surfaces.iter_mut().find(|s| s.id == id) else { return };
        match &mut surface.view {
            View::Inspector { creature, .. } => {
                let creature = creature.clone();
                let (name, arg) = match action {
                    UiAction::SetLevelPin(on) => ("pin_level", super::actions::on_off(on).to_owned()),
                    UiAction::SetHealthPin(on) => ("pin_health", super::actions::on_off(on).to_owned()),
                    UiAction::SetTeam(team) => ("team", team),
                    UiAction::SetRelation { other, relation } => ("relation", format!("{other}={relation}")),
                    UiAction::UnlockSkill { ability_id, .. } => ("unlock", ability_id),
                    UiAction::Equip { item_id, .. } => ("equip", item_id),
                    UiAction::Unequip { slot, .. } => ("unequip", slot),
                    UiAction::AddInventoryItem(item) => ("add_item", item),
                    other => {
                        log::debug!("inspector action not handled: {other:?}");
                        return;
                    }
                };
                self.commands.push(EngineCommand::SpiderAction { creature, action: name.into(), arg: Some(arg) });
            }
            View::Pause(_) => {
                if let UiAction::PauseChoice(choice) = action {
                    self.pause_choice(id, &choice);
                }
            }
            View::Character { window, profile, path, .. } => {
                if dc_ui::bridge::apply_profile_action(profile, &action) {
                    if !save_profile(profile, path) {
                        log::warn!("Could not write {}", path.display());
                    }
                    let party = dc_ui::bridge::party_view(profile);
                    match window.as_mut() {
                        CharacterWindow::Single(d) => d.set_party(party),
                        CharacterWindow::Party(d) => d.set_party(party),
                    }
                }
            }
            View::Controls(_) => {
                if let UiAction::SaveControls(c) = action {
                    self.save_controls(&c);
                }
            }
            View::Settings { accepted_fps, .. } => match action {
                UiAction::SetFrameRate(fps) => *accepted_fps = Some(fps),
                UiAction::SaveControls(c) => self.save_controls(&c),
                _ => {}
            },
            View::Menu { .. } | View::Prompt { .. } => {}
        }
    }

    fn save_controls(&mut self, c: &dc_ui::model::ControlSettings) {
        let settings: dc_adventure::ControlSettings = c.into();
        if !save_controls(&settings, &controls_path(&self.state_dir)) {
            log::warn!("Could not save the controls");
        }
    }

    /// `_show_adventure_pause`'s loop, one choice at a time.
    fn pause_choice(&mut self, id: SurfaceId, choice: &str) {
        let in_mission = self.adventure.in_mission;
        match choice {
            // `continue`: the menu comes straight back.
            "save" => self.commands.push(EngineCommand::SaveProgress),
            "skills" => {
                self.remove(id);
                self.open_character(0, true);
            }
            "settings" => {
                self.remove(id);
                let controls = load_controls(&controls_path(&self.state_dir));
                let dialog = AdventureSettingsDialog::new(
                    self.adventure.target_fps.round().max(1.0) as u32,
                    (&controls).into(),
                    self.adventure.players,
                );
                self.push(
                    AdventureSettingsDialog::title(),
                    Placement::Centered(None),
                    Sizing::FitHeight(700.0),
                    View::Settings { dialog: Box::new(dialog), accepted_fps: None },
                );
            }
            "restart" => {
                self.remove(id);
                if in_mission {
                    self.commands.push(EngineCommand::RestartMission);
                }
                self.commands.push(EngineCommand::Resume);
            }
            "release" => {
                // Offered only outside a raid (`self.mission is None`), which
                // Python has not run since Adventure became the territory
                // mission (5e57876): its free-roam code is unreachable there
                // too, so `player.release()` has no command here.
                log::warn!("Release spider: no free-roaming Adventure to release from");
                self.remove(id);
                self.commands.push(EngineCommand::Resume);
            }
            "exit" => {
                self.remove(id);
                self.commands.push(EngineCommand::SaveProgress);
                self.commands.push(EngineCommand::ExitMission);
            }
            _ => {
                self.remove(id);
                self.commands.push(EngineCommand::Resume);
            }
        }
    }

    fn remove(&mut self, id: SurfaceId) -> Option<Surface> {
        let index = self.surfaces.iter().position(|s| s.id == id)?;
        Some(self.surfaces.remove(index))
    }

    /// Close a surface and do what its closing meant.
    fn finish(&mut self, id: SurfaceId, how: Finish) {
        let Some(surface) = self.remove(id) else { return };
        match surface.view {
            View::Menu { .. } | View::Prompt { .. } => {}
            View::Inspector { then_pause, .. } => {
                if then_pause {
                    self.open_pause();
                }
            }
            // Closed without a choice: `choice = "resume"`.
            View::Pause(_) => self.commands.push(EngineCommand::Resume),
            View::Character { then_pause, .. } => {
                // `self.mission.refresh_from_profile()`.
                self.commands.push(EngineCommand::ProfileChanged);
                if then_pause {
                    self.open_pause();
                }
            }
            View::Controls(_) => self.commands.push(EngineCommand::ControlsChanged),
            View::Settings { accepted_fps, .. } => {
                // `if settings.exec_() == QDialog.Accepted: self.set_target_fps(...)`.
                if how == Finish::Done
                    && let Some(fps) = accepted_fps
                {
                    self.commands.push(EngineCommand::SetTargetFps(fps as f64));
                    self.adventure.target_fps = fps as f64;
                }
                // `_reload_controls_if_changed(force=True)`.
                self.commands.push(EngineCommand::ControlsChanged);
                self.open_pause();
            }
        }
    }
}

/// What a frame of a surface produced.
enum Outcome {
    Keep,
    Finish(Finish),
    Menu(String, Option<bool>),
    Prompt(String),
    Actions(Vec<UiAction>),
}

fn fit_height(size: &mut Vec2, sizing: Sizing, used: Vec2) {
    if let Sizing::FitHeight(w) = sizing {
        *size = vec2(w, used.y.ceil().max(60.0));
    }
}
