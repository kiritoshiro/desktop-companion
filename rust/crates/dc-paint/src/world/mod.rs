//! The world painters: `world/webs.py`, `flies.py`, `mouse_webs.py`,
//! `jobs.py` (bases), `cage.py` and `carcass.py`, each drawn from a plain
//! snapshot struct named after the Python class (`Web` → [`WebDraw`], …)
//! with the Python attribute names.
//!
//! `clip` arguments are the Python `(x0, y0, x1, y1)` tuples the manager
//! derives from `painter.clipBoundingRect()`.

pub mod bases;
pub mod cage;
pub mod carcass;
pub mod flies;
pub mod mouse_webs;
pub mod webs;

pub use bases::{BaseDraw, render_bases};
pub use cage::{CageDraw, draw_cage};
pub use carcass::{CarcassDraw, draw_carcass};
pub use flies::{FlyDraw, FlyRemainsDraw, FlySpawnerDraw, FlyWorldDraw, RemainsPart, WebShotDraw, render_flies};
pub use mouse_webs::{CaptureDraw, MouseShotDraw, MouseWebDraw, SpentSplatDraw, StickyHoldDraw, render_mouse_webs};
pub use webs::{StrandDraw, WebDraw, render_webs};

/// `(x0, y0, x1, y1)`.
pub type Clip = (f64, f64, f64, f64);

/// `QPen(color, width)` with a round cap (Python's `pen.setCapStyle(Qt.RoundCap)`).
pub(crate) fn round_cap_pen(color: dc_core::Color, width: f64) -> dc_render::Pen {
    dc_render::Pen::full(color, width, dc_render::PenStyle::Solid, dc_render::Cap::Round, dc_render::Join::Bevel)
}
