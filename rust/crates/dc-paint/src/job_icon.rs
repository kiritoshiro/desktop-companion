//! Job icons for the name label (Rust-only; the Python app had none).
//!
//! One glyph per job in `dc_content::jobs::JOB_IDS`, drawn with plain
//! canvas shapes so the CPU and GPU backends both get it: a shield for
//! Guard, a hammer for Builder, a crosshair for Hunter, an eye for Scout and
//! a web for Web tender. `"none"` (and anything unknown) has no icon.

use dc_core::{Color, Rect};
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen};

/// Does `job_id` have an icon? `false` for `"none"` and unknown ids.
pub fn has_job_icon(job_id: &str) -> bool {
    matches!(job_id, "hunter" | "builder" | "guard" | "scout" | "webber")
}

/// Draws `job_id`'s icon centred on `(cx, cy)`, fitting a circle of radius
/// `r`. Returns whether anything was drawn.
pub fn draw_job_icon(p: &mut Canvas, job_id: &str, cx: f64, cy: f64, r: f64, color: Color) -> bool {
    if !has_job_icon(job_id) {
        return false;
    }
    let line = |w: f64| Pen::new(color, w).cap(Cap::Round).join(Join::Round);
    let w = (r * 0.3).max(1.1);
    p.save();
    match job_id {
        "guard" => {
            let mut shield = Path::new();
            shield.move_to(cx, cy - r);
            shield.line_to(cx + r * 0.82, cy - r * 0.66);
            shield.line_to(cx + r * 0.78, cy - r * 0.05);
            shield.quad_to(cx + r * 0.62, cy + r * 0.62, cx, cy + r);
            shield.quad_to(cx - r * 0.62, cy + r * 0.62, cx - r * 0.78, cy - r * 0.05);
            shield.line_to(cx - r * 0.82, cy - r * 0.66);
            shield.close_subpath();
            p.set_pen(Pen::NONE);
            p.set_brush(color);
            p.draw_path(&shield);
        }
        "builder" => {
            p.translate(cx, cy);
            p.rotate(-40.0);
            p.set_pen(line(w * 1.1));
            p.draw_line(0.0, -r * 0.35, 0.0, r * 0.95);
            p.set_pen(Pen::NONE);
            p.set_brush(color);
            p.draw_rounded_rect(Rect::new(-r * 0.78, -r * 0.95, r * 1.56, r * 0.62), r * 0.12, r * 0.12);
        }
        "hunter" => {
            p.set_pen(line(w));
            p.set_brush(Brush::None);
            p.draw_ellipse_at(cx, cy, r * 0.62, r * 0.62);
            for (dx, dy) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
                p.draw_line(cx + dx * r * 0.32, cy + dy * r * 0.32, cx + dx * r, cy + dy * r);
            }
        }
        "scout" => {
            let mut eye = Path::new();
            eye.move_to(cx - r, cy);
            eye.quad_to(cx, cy - r * 1.05, cx + r, cy);
            eye.quad_to(cx, cy + r * 1.05, cx - r, cy);
            eye.close_subpath();
            p.set_pen(line(w));
            p.set_brush(Brush::None);
            p.draw_path(&eye);
            p.set_pen(Pen::NONE);
            p.set_brush(color);
            p.draw_ellipse_at(cx, cy, r * 0.34, r * 0.34);
        }
        _ => {
            // "webber": six spokes and two rings.
            p.set_pen(line(w * 0.7));
            p.set_brush(Brush::None);
            let spoke = |i: usize, k: f64| {
                let a = std::f64::consts::PI / 3.0 * i as f64 - std::f64::consts::FRAC_PI_2;
                (cx + a.cos() * r * k, cy + a.sin() * r * k)
            };
            for i in 0..6 {
                let (x, y) = spoke(i, 1.0);
                p.draw_line(cx, cy, x, y);
            }
            for k in [0.45, 0.85] {
                let ring: Vec<(f64, f64)> = (0..6).map(|i| spoke(i, k)).collect();
                p.draw_polygon(&ring);
            }
        }
    }
    p.restore();
    true
}
