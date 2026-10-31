//! dc-ui: the egui windows of Desktop Bug Companion, ported from the PyQt5
//! windows in `src/desktop_bug/app/`.
//!
//! * [`theme`] -- `wood_theme.py`: palette, fonts, visuals, frames.
//! * [`widgets`] -- carved buttons, pill switches, key caps, `VitalBar`,
//!   `StatTile`, section headers, tab bars.
//! * [`assets`] -- the PNG emblems and wood textures of `assets/ui`.
//! * [`art`] -- `skill_art.py` medallions and config_ui's small icons.
//! * [`windows`] -- one struct per window; each has
//!   `fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction>`.
//! * [`runner`] -- [`run_window`], a native winit + egui-wgpu host.
//! * [`host`] -- [`host::HostedWindow`], the same stack inside another
//!   program's event loop (the overlay's menus and dialogs).
//! * [`model`], [`teams`], [`stat_text`] -- the view models the app fills in.
//!
//! dc-ui never spawns processes or writes files; the windows answer with
//! [`UiAction`]s and the app layer carries them out.

pub mod action;
pub mod art;
pub mod assets;
pub mod bridge;
pub mod host;
pub mod model;
pub mod paint;
pub mod probe;
pub mod runner;
pub mod sample;
pub mod stat_text;
pub mod teams;
pub mod theme;
pub mod widgets;
pub mod windows;

pub use action::{OverlayMode, UiAction};
pub use runner::{POLL_INTERVAL, PollOutcome, RunOptions, WindowApp, run_window, run_window_with};
