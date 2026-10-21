//! Port of `desktop_bug/manager`: colony ownership and frame orchestration.
pub mod cages;
pub mod colony;
pub mod combat;
pub mod constants;
pub mod core;
pub mod hunting;
pub mod naming;
pub mod persistence;
pub mod prey;
pub mod render;
pub mod surfaces;
pub use core::{CreatureManager, MouseInput};
pub use surfaces::DesktopSurface;
pub mod mission_host;
pub mod paint;
pub mod roster;
