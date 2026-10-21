//! The overlay's menus and dialogs: the spider right-click menu, the tray
//! menu, and the windows the overlay opens over the desktop (inspector,
//! pause menu, character & armour, controls) — `app/engine.py`'s QMenu and
//! dialog code.
//!
//! The engine owns this value and talks to it only through this interface:
//! it forwards [`UiRequest`]s, passes window events and a frame call, and
//! applies the [`EngineCommand`]s it gets back. Everything behind the
//! interface (which windows exist, how egui is driven) is this module's
//! business. `crates/dc-app/OVERLAY_UI.md` is the engine's checklist.
//!
//! Inside: [`surfaces`] keeps what is open and runs each surface's egui (no
//! windows, so tests drive it headlessly); [`shell`] gives each surface a
//! winit window in the engine's event loop; [`context_menu`] builds the
//! right-click menu; [`actions`] carries out menu actions on the
//! [`CreatureManager`]; [`live`] reads live spiders into the inspector and
//! paints their portraits.

// Parts of the interface have no caller in the binary (`UiRequest::
// {Inspector, Controls, CloseAll}`, `tray_event`, test helpers); they stay
// for the engine and the tests.
#![allow(dead_code)]

pub mod actions;
pub mod context_menu;
pub mod live;
pub mod shell;
pub mod surfaces;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use dc_manager::CreatureManager;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

// For the engine (see OVERLAY_UI.md).
#[allow(unused_imports)]
pub use actions::{
    apply_overlay_action, apply_spider_action, initial_status, tray_checks, tray_event, tray_event_checked, tray_menu,
    tray_menu_for,
};

/// Stable key of a live creature (the manager's progression id).
pub type CreatureKey = String;

/// Which Adventure player a request is about (0 = P1, 1 = P2).
pub type PlayerIndex = usize;

/// Something the engine wants shown.
#[derive(Clone, Debug, PartialEq)]
pub enum UiRequest {
    /// Right-click on a spider at a desktop position (physical px).
    SpiderMenu { creature: CreatureKey, at: (f64, f64) },
    /// Right-click on empty overlay space / the tray icon's menu.
    OverlayMenu { at: (f64, f64) },
    /// The inspector for one spider (Companion mode).
    Inspector { creature: CreatureKey },
    /// Adventure: Esc pause menu.
    Pause,
    /// Adventure: K / pause menu "Character & armour".
    Character { player: PlayerIndex },
    /// Adventure: controls editor from the pause menu.
    Controls,
    /// Close everything this module has open.
    CloseAll,
}

/// Something the menus or dialogs want the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineCommand {
    Quit,
    /// Resume an Adventure raid after the pause menu closes.
    Resume,
    /// Leave the raid (pause menu "Exit"), or restart it.
    ExitMission,
    RestartMission,
    /// Re-read progression after the character window closed.
    ProfileChanged,
    /// Re-read controls after the controls editor closed.
    ControlsChanged,
    /// A spider-menu action, by Python's action name (e.g. "rename",
    /// "cage", "release", "feed", "pin_health"), with an optional argument.
    /// Carry it out with [`apply_spider_action`].
    SpiderAction {
        creature: CreatureKey,
        action: String,
        arg: Option<String>,
    },
    /// Overlay-menu action by name (e.g. "pause_all", "open_settings").
    /// Carry it out with [`apply_overlay_action`].
    OverlayAction {
        action: String,
    },
    /// `_announce(message)`: show a tray balloon and put the text on the
    /// tray menu's status line (id `"status"`).
    Announce(String),
    /// `set_target_fps(fps)` (tray Performance, Adventure settings).
    SetTargetFps(f64),
    /// Save the raid's progress (`mission.save_progress()`; outside a raid
    /// `manager.save_runtime_state()`). Comes before the character window
    /// opens and from the pause menu's save / exit.
    SaveProgress,
}

/// What the engine tells the UI about the Adventure it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct AdventureContext {
    /// A raid is running (`self.mission is not None`): the pause menu offers
    /// restart / exit raid, and "Character & armour" opens the party windows.
    pub in_mission: bool,
    /// How many play (`len(mission.players)`): two opens the party window.
    pub players: usize,
    /// The frame rate the player chose (the Adventure settings dialog).
    pub target_fps: f64,
    /// Outside a raid, each player's spider: "Character & armour" opens the
    /// inspector's skill tree on it.
    pub player_creatures: Vec<CreatureKey>,
}

impl Default for AdventureContext {
    fn default() -> Self {
        Self { in_mission: true, players: 1, target_fps: 60.0, player_creatures: Vec::new() }
    }
}

/// The overlay's UI layer. The engine creates one and calls it every frame.
pub struct OverlayUi {
    core: surfaces::UiCore,
    shell: shell::Shell,
}

impl Default for OverlayUi {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayUi {
    pub fn new() -> Self {
        Self::with_state_dir(dc_content::discovery::state_dir())
    }

    /// Profile and controls are read from and written to `state_dir`.
    pub fn with_state_dir(state_dir: PathBuf) -> Self {
        Self { core: surfaces::UiCore::new(state_dir), shell: shell::Shell::default() }
    }

    /// Ask for something to be shown.
    pub fn request(&mut self, el: &ActiveEventLoop, request: UiRequest) {
        self.core.request(request);
        self.shell.frame(&mut self.core, el);
    }

    /// True when a menu or dialog is open, so the engine pauses input to the
    /// game and keeps the cursor out of click-through.
    pub fn is_open(&self) -> bool {
        self.core.is_open()
    }

    /// True if this window id belongs to the UI layer (the engine passes the
    /// event on to [`OverlayUi::window_event`] instead of handling it).
    pub fn owns_window(&self, id: WindowId) -> bool {
        self.shell.owns(id)
    }

    pub fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: &WindowEvent) {
        self.shell.window_event(&mut self.core, el, id, event);
    }

    /// Called once per engine tick, after the simulation, to run and paint.
    pub fn frame(&mut self, el: &ActiveEventLoop) {
        self.shell.frame(&mut self.core, el);
    }

    /// Commands produced since the last call. Apply them before the next
    /// [`OverlayUi::frame`]: a window that needs a command carried out first
    /// (the character window after [`EngineCommand::SaveProgress`]) opens on
    /// the frame after this call.
    pub fn take_commands(&mut self) -> Vec<EngineCommand> {
        self.core.take_commands()
    }

    // -- additions (see OVERLAY_UI.md) ----------------------------------------

    /// Read the scene: a new menu's entries, the inspector (every 400 ms)
    /// and its portrait. Call every tick before [`OverlayUi::frame`]; a menu
    /// or inspector appears once it has been synced. Cheap when nothing is
    /// open.
    pub fn sync(&mut self, manager: &CreatureManager) {
        if self.core.is_open() {
            self.core.sync(manager);
        }
    }

    /// Where the overlay's top-left corner is on the desktop (physical px),
    /// so a click's desktop position maps to the manager's coordinates.
    pub fn set_overlay_origin(&mut self, origin: (i32, i32)) {
        self.core.origin = origin;
    }

    /// What kind of Adventure is running (pause menu choices, party window).
    pub fn set_adventure(&mut self, adventure: AdventureContext) {
        self.core.adventure = adventure;
    }

    /// The id of the base picked up with "Move the … base…", if any.
    pub fn carried_base(&self) -> Option<&str> {
        self.core.carried_base.as_deref()
    }
}
