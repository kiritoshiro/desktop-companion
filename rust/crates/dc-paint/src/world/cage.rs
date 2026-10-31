//! Port of `Cage.draw` (`world/cage.py`): the terrarium fence with its
//! corner grips and member tag.

use dc_core::{Color, Rect};
use dc_render::{Brush, Canvas, Cap, Font, FontMetrics, Join, Pen, PenStyle, TextFlags};
use serde::{Deserialize, Serialize};

/// `Cage.HANDLE`.
pub const HANDLE: f64 = 11.0;

/// `Cage` plus the two arguments of `draw`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct CageDraw {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Spiders whose `cage` is this one.
    pub member_count: usize,
    /// `cage is active_cage` (being dragged).
    pub active: bool,
}

/// `Cage.draw(painter, member_count, active)`.
pub fn draw_cage(p: &mut Canvas, cage: &CageDraw) {
    let rect = Rect::new(cage.x, cage.y, cage.w, cage.h);
    let radius = 22.0f64.min(cage.w * 0.12).min(cage.h * 0.12);
    let wash = Color::rgba(150, 205, 235, if cage.active { 40 } else { 26 });
    p.set_pen(Pen::NONE);
    p.set_brush(wash);
    p.draw_rounded_rect(rect, radius, radius);

    p.set_brush(Brush::None);
    p.set_pen(Pen::full(Color::rgba(40, 120, 165, 70), 6.0, PenStyle::Solid, Cap::Round, Join::Round));
    p.draw_rounded_rect(rect, radius, radius);
    let line = if cage.active { Color::rgba(225, 245, 255, 235) } else { Color::rgba(205, 232, 245, 205) };
    p.set_pen(Pen::full(line, 2.2, PenStyle::Solid, Cap::Round, Join::Round));
    p.draw_rounded_rect(rect, radius, radius);

    p.set_brush(Color::rgba(245, 252, 255, 240));
    p.set_pen(Pen::new(Color::rgba(40, 120, 165, 230), 1.6));
    let g = HANDLE;
    let (right, bottom) = (cage.x + cage.w, cage.y + cage.h);
    for (cx, cy) in [(cage.x, cage.y), (right, cage.y), (cage.x, bottom), (right, bottom)] {
        p.draw_rounded_rect(Rect::new(cx - g * 0.5, cy - g * 0.5, g, g), 3.0, 3.0);
    }

    if cage.member_count > 0 {
        p.set_pen(Pen::new(Color::rgba(40, 120, 165, 220), 1.0));
        p.set_brush(Color::rgba(245, 252, 255, 220));
        let label =
            if cage.member_count > 1 { format!("\u{1F577} {}", cage.member_count) } else { "\u{1F577}".to_string() };
        let mut font = Font::default();
        font.set_point_size_f(9.0);
        p.set_font(font);
        let fm = FontMetrics::new(&font);
        let tw = (fm.horizontal_advance(&label) + 12) as f64;
        let th = (fm.height() + 4) as f64;
        let (bx, by) = (cage.x + 6.0, cage.y + 6.0);
        p.draw_rounded_rect(Rect::new(bx, by, tw, th), 5.0, 5.0);
        p.set_pen(Color::rgba(20, 70, 100, 255));
        p.draw_text_in(Rect::new(bx, by, tw, th), TextFlags::ALIGN_CENTER, &label);
    }
}
