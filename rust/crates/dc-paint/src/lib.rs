//! `dc-paint`: the creature and world painters of Desktop Bug Companion,
//! ported from PyQt5 `QPainter` code onto [`dc_render::Canvas`].
//!
//! Every painter reads a plain snapshot struct (see [`draw`] and [`world`])
//! whose field names are the Python attribute names, so the simulation
//! crates fill them field by field. Nothing here advances a simulation or
//! draws from a random stream: painting is pure, as in the Python.
//!
//! Entry points:
//!
//! * [`CreaturePainter::render`] — `Creature.render` (procedural, sprite-rig
//!   and regal bodies, web net, name label, damage numbers).
//! * [`world`] — `Web.draw`, `FlyWorld.render`, `MouseWebWorld.render`,
//!   `BaseWorld.render`, `Cage.draw`, `Carcass.draw`.
//! * [`scene::render_scene`] — `CreatureManager.render`: the layer order.

pub mod antenna;
pub mod armour_art;
pub mod batch;
pub mod color;
pub mod compare;
pub mod creature;
pub mod damage;
pub mod draw;
pub mod label;
pub mod look;
pub(crate) mod procedural;
pub mod regal;
pub mod resolve;
pub mod scene;
pub mod sprite;
pub mod sprite_tint;
pub mod web_net;
pub mod world;

pub use color::Palette;
pub use creature::CreaturePainter;
pub use draw::{CreatureDraw, LabelDraw, LegDraw, Mood, NetSpoke, RegalExpression, Worn};
pub use look::{CreatureLook, RenderMode};

/// The module-level render switches of the Python: `render_detail.ENABLED`,
/// `render_batch.BATCH_LEGS`, `render_batch.LEG_WIDTH_STEP` and the
/// `DESKTOP_BUG_FAST_PIXMAPS` environment switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintSwitches {
    /// `render_detail.ENABLED`: skip leg detail too small to see (DC-73).
    pub detail_enabled: bool,
    /// `render_batch.BATCH_LEGS`: collect the leg pass into a few paths.
    pub batch_legs: bool,
    /// `render_batch.LEG_WIDTH_STEP` (0 = exact widths).
    pub leg_width_step: f64,
    /// `DESKTOP_BUG_FAST_PIXMAPS=1`: sprite rigs without smooth pixmap
    /// transforms.
    pub fast_pixmaps: bool,
}

impl Default for PaintSwitches {
    fn default() -> Self {
        PaintSwitches {
            detail_enabled: true,
            batch_legs: batch::BATCH_LEGS,
            leg_width_step: batch::LEG_WIDTH_STEP,
            fast_pixmaps: false,
        }
    }
}

impl PaintSwitches {
    /// The defaults with `DESKTOP_BUG_FAST_PIXMAPS` read from the environment.
    pub fn from_env() -> Self {
        let fast = std::env::var("DESKTOP_BUG_FAST_PIXMAPS")
            .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        PaintSwitches { fast_pixmaps: fast, ..PaintSwitches::default() }
    }
}
