//! Port of `creature/render_detail.py`: leg detail too small to be seen is
//! not drawn (DC-73). Geometry-only decisions the leg painter consults.
//!
//! The module flag `ENABLED` is the `enabled` argument here (true in
//! production; false restores the full-detail drawing exactly).

use crate::config::LegChainConfig;

/// How far a joint node has to stand proud of its leg to read as a joint.
pub const JOINT_PROTRUDE_MIN_PX: f64 = 0.50;
/// How far the hair over-stroke must stick out past its segment to read as a fringe.
pub const HAIR_FRINGE_MIN_PX: f64 = 0.75;
/// Warmth a merged segment takes from the hair colour.
pub const MERGED_HAIR_TINT: f64 = 0.30;

/// `LegDetail`: what is worth drawing on one spider's legs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegDetail {
    pub separate_hair: bool,
    pub merge_hair: bool,
}

impl LegDetail {
    pub fn strokes_per_segment(&self) -> usize {
        if self.separate_hair { 2 } else { 1 }
    }
}

/// `hair_detail(base_width, chain_config)`, decided once per spider from the
/// resting leg width.
pub fn hair_detail(base_width: f64, chain: Option<&LegChainConfig>, enabled: bool) -> LegDetail {
    let hairy = chain.is_some_and(|c| c.hairy && c.hair_scale > 0.0);
    if !hairy {
        return LegDetail { separate_hair: false, merge_hair: false };
    }
    if !enabled {
        return LegDetail { separate_hair: true, merge_hair: false };
    }
    let hair_scale = chain.map(|c| c.hair_scale).unwrap_or(0.0);
    hair_detail_for(base_width, hair_scale)
}

/// The fringe decision for an explicit `hair_scale` (a hairy chain).
pub fn hair_detail_for(base_width: f64, hair_scale: f64) -> LegDetail {
    let fringe = base_width * (0.12 + hair_scale * 0.55) * 0.5;
    if fringe >= HAIR_FRINGE_MIN_PX {
        LegDetail { separate_hair: true, merge_hair: false }
    } else {
        LegDetail { separate_hair: false, merge_hair: true }
    }
}

/// `hair_pen_width(width, hair_scale)`
pub fn hair_pen_width(width: f64, hair_scale: f64) -> f64 {
    width * (1.12 + hair_scale * 0.55)
}

/// `joint_node_shows(radius, *adjacent_widths)`: would the node stand proud
/// of the thinner neighbouring segment?
pub fn joint_node_shows(radius: f64, adjacent_widths: &[f64], enabled: bool) -> bool {
    if !enabled {
        return true;
    }
    let min = adjacent_widths.iter().copied().filter(|w| *w > 0.0).fold(f64::INFINITY, f64::min);
    if !min.is_finite() {
        return true;
    }
    radius >= min * 0.5 + JOINT_PROTRUDE_MIN_PX
}
