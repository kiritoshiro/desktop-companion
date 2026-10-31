//! View models: the data the windows show, in plain structs.
//!
//! dc-ui never reads presets, profiles or model folders itself. The app layer
//! fills these from dc-content, dc-state and dc-adventure and hands them in;
//! the windows answer with [`crate::UiAction`]s.
//!
//! The types here are dc-ui's own, written while those crates were being
//! written alongside it; [`crate::bridge`] now fills them from the real
//! crates. Each module notes the Python it mirrors:
//!
//! * [`catalog`] -- content/discovery (models, personalities), content/skills,
//!   world/jobs (`JOB_OPTIONS`), content/palettes, content/preset_io
//!   (`LABEL_SWITCH_DEFAULTS`), content/body_plans, content/enemy_kinds.
//! * [`progression`] -- state/progression (`ABILITY_TREE`, `ARMOR_CATALOG`,
//!   `ARMOR_SETS`, `ProgressionState`, `xp_to_next_level`).
//! * [`controls`] -- app/controls (`ControlSettings`, `ACTIONS`, bindings).
//! * [`adventure`] -- app/adventure_profile, app/campaign, app/armoury,
//!   app/custom_maps as the Adventure page and character window see them.

pub mod adventure;
pub mod catalog;
pub mod controls;
pub mod progression;

pub use adventure::*;
pub use catalog::*;
pub use controls::*;
pub use progression::*;

/// An RGB triple as presets store colours (`[r, g, b]`).
pub type Rgb = [u8; 3];

/// Paints a model's thumbnail or a live spider's portrait (dc-paint).
/// Given a model id (or creature key) and the size wanted, it returns a
/// texture, or None for the fallback drawing.
pub type ThumbnailFn = Box<dyn Fn(&str, egui::Vec2) -> Option<egui::TextureHandle>>;

/// Paints an armour piece's picture (dc-paint's `armour_art.armour_icon`).
pub type ArmourIconFn = Box<dyn Fn(&str, &str, u32) -> Option<egui::TextureHandle>>;
