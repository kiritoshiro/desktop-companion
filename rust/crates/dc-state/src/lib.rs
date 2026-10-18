//! Port of `desktop_bug/state/*`: what outlives one session.
//!
//! - [`progression`]: XP curve, levels, talents, armour, per-spider state,
//!   team ids and stances.
//! - [`teams`]: team names and colours.
//! - [`runtime_state`]: `creatures.json` (schema 3): keys, migration,
//!   eviction, reset and atomic save.

pub mod progression;
pub mod runtime_state;
pub mod teams;

pub use progression::{ArmorItem, ProgressionState, Stances};
pub use runtime_state::{RuntimeState, SavedCreature, Scene};
pub use teams::{TeamProfile, TeamProfiles};
