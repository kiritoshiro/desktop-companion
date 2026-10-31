//! Adventure game logic: ports of the Qt-free `app/` modules.
//!
//! Data and rules only; no window, no painting. File-backed modules take the
//! state folder (or file path) explicitly instead of resolving it globally.
//!
//! Wave 2 adds `mission`, `swarm` and `reclaim` beside these (see
//! `PORT_STATUS.md` for the interfaces they use).

pub mod admin;
pub mod adventure;
pub mod adventure_profile;
pub mod armoury;
pub mod campaign;
pub mod controls;
pub mod custom_maps;
pub mod encounters;
pub mod map_layouts;
pub mod mission;
pub mod mission_actor;
pub mod mission_factory;
pub mod progression;
pub mod pyjson;
pub mod reclaim;
pub mod stat_text;
pub mod swarm;

pub use adventure::{ControlledSpider, PlayerController};
pub use adventure_profile::AdventureProfile;
pub use campaign::{MapInfo, MapKind};
pub use controls::{Action, ControlSettings, Key};
pub use custom_maps::{CustomMap, SpiderSpec};
pub use encounters::{EncounterDirector, EncounterProfile, OutpostPlan};
pub use map_layouts::{BuildingKind, Placement};
