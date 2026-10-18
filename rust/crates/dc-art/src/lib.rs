//! `dc-art`: mission and menu art for the Rust port of Desktop Bug Companion.
//!
//! Ports of `app/mission_art.py`, `app/mission_backdrop.py`, `app/map_art.py`,
//! `app/skill_art.py`, `app/mission_ui.py` (with the HUD parts of
//! `app/adventure_ui.py` and the engine's banners), `app/desktop_surface.py`,
//! `app/screen_text.py` and `tools/generate_ui_art.py` (the menu emblems),
//! all painting onto `dc-render`. See `PORT_STATUS.md` for the API the
//! Reclaim mission, the mission and the engine call.

pub mod common;
pub mod desktop_surface;
pub mod map_art;
pub mod mission_art;
pub mod mission_backdrop;
pub mod mission_ui;
pub mod qt;
pub mod screen_text;
pub mod skill_art;
pub mod ui_art;
