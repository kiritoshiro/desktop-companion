//! Pens and small helpers shared by the art modules.

use dc_core::Color;
use dc_render::{Cap, Join, Pen, PenStyle};

/// `QPen(c, w, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)`.
pub fn round_pen(c: Color, w: f64) -> Pen {
    Pen::full(c, w, PenStyle::Solid, Cap::Round, Join::Round)
}

/// `QPen(c, w, Qt.SolidLine, Qt.RoundCap)` (bevel join, as Qt's default).
pub fn round_cap_pen(c: Color, w: f64) -> Pen {
    Pen::full(c, w, PenStyle::Solid, Cap::Round, Join::Bevel)
}

/// `QColor.setAlpha(a)` on a copy.
pub fn alpha(c: Color, a: i64) -> Color {
    c.with_alpha(a.clamp(0, 255) as u8)
}
