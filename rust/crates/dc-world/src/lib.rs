//! The world the spiders live in: where they may stand, how the screens join,
//! how squads fight, and the shared simulation objects every spider uses —
//! webs, flies, cursor silk, colony bases, cages, carcasses and aimed silk
//! (ports of `world/*`).
//!
//! Drawing is out of scope: every world exposes the state a painter needs as
//! public fields or getters (see `PORT_STATUS.md`). Creatures are reached
//! through [`creature_view::WorldCreature`].

pub mod aimed_silk;
pub mod cage;
pub mod carcass;
pub mod creature_view;
pub mod flies;
pub mod jobs;
pub mod mouse_webs;
pub mod playfield;
pub mod pymath;
pub mod screen_layout;
pub mod spatial;
pub mod tactics;
pub mod webs;

pub use cage::Cage;
pub use carcass::Carcass;
pub use creature_view::{CreatureId, FlyId, JobIntent, JobMode, Relation, WebId, WorldCreature};
pub use flies::{Fly, FlyWorld};
pub use jobs::{BaseSite, BaseWorld};
pub use mouse_webs::MouseWebWorld;
pub use playfield::{Playfield, ScreenRect, rects_from_geometry};
pub use screen_layout::{LinkKind, Screen, ScreenLayout, ScreenLink};
pub use webs::{Web, WebWorld};

/// `(x, y, w, h)`, the shape every Python `footprint()` / dirty rect uses.
pub type Xywh = (f64, f64, f64, f64);
/// `(x0, y0, x1, y1)`, the shape every Python `bbox()` uses.
pub type Bbox = (f64, f64, f64, f64);
/// A point.
pub type Point = (f64, f64);
