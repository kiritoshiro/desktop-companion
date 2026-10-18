//! The overlay's menus and dialogs: the spider right-click menu, the tray
//! menu, and the windows the overlay opens over the desktop (inspector,
//! pause menu, character & armour, controls) — `app/engine.py`'s QMenu and
//! dialog code.
//!
//! The engine owns this value and talks to it only through this interface:
//! it forwards [`UiRequest`]s, passes window events and a frame call, and
//! applies the [`EngineCommand`]s it gets back. Everything behind the
//! interface (which windows exist, how egui is driven) is this module's
//! business.

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

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
    SpiderAction { creature: CreatureKey, action: String, arg: Option<String> },
    /// Overlay-menu action by name (e.g. "pause_all", "open_settings").
    OverlayAction { action: String },
}

/// The overlay's UI layer. The engine creates one and calls it every frame.
#[derive(Default)]
pub struct OverlayUi {
    commands: Vec<EngineCommand>,
}

impl OverlayUi {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask for something to be shown.
    pub fn request(&mut self, _el: &ActiveEventLoop, _request: UiRequest) {}

    /// True when a menu or dialog is open, so the engine pauses input to the
    /// game and keeps the cursor out of click-through.
    pub fn is_open(&self) -> bool {
        false
    }

    /// True if this window id belongs to the UI layer (the engine passes the
    /// event on to [`OverlayUi::window_event`] instead of handling it).
    pub fn owns_window(&self, _id: WindowId) -> bool {
        false
    }

    pub fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, _event: &WindowEvent) {}

    /// Called once per engine tick, after the simulation, to run and paint.
    pub fn frame(&mut self, _el: &ActiveEventLoop) {}

    /// Commands produced since the last call.
    pub fn take_commands(&mut self) -> Vec<EngineCommand> {
        std::mem::take(&mut self.commands)
    }
}
