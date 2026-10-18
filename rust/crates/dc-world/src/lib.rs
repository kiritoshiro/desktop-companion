//! The world the spiders live in: where they may stand, how the screens join,
//! and how squads fight (ports of `world/*`).
//!
//! Modules are independent ports of one Python module each. Wave-2 modules
//! (`webs`, `flies`, `mouse_webs`, `jobs`, `cage`, `carcass`, `aimed_silk`,
//! `desktop_environment`) go beside these as further `pub mod` lines; nothing
//! here needs to change for them.

pub mod playfield;
pub mod screen_layout;
pub mod tactics;

pub use playfield::{Playfield, ScreenRect, rects_from_geometry};
pub use screen_layout::{LinkKind, Screen, ScreenLayout, ScreenLink};
