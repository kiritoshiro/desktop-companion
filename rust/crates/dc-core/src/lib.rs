//! Shared foundation for the Rust port of Desktop Bug Companion:
//! CPython-compatible randomness, math helpers, colours and rectangles.

pub mod color;
pub mod geom;
pub mod math;
pub mod rng;

pub use color::Color;
pub use geom::{IRect, Rect};
pub use math::Vec2;
pub use rng::PyRandom;

/// Application version, kept equal to `desktop_bug.__version__`.
pub const VERSION: &str = "0.1.0";
