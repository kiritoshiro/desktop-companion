//! Port of the drawing half of `creature/regal.py`: `_render_regal`, the
//! fluffy jumper (`regal_fluff` / `regal_scout`). Pure: the gesture is read
//! from [`crate::draw::RegalExpression`], which the caller zeroes when
//! `_regal_blocked()`.

use crate::color::{qt_darker, qt_lighter};
use crate::procedural::Paint;
use crate::sprite::draw_equipment;
use dc_core::Color;
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen, PenStyle, RadialGradient};
use std::f64::consts::{PI, TAU};

fn round_cap(color: Color, width: f64) -> Pen {
    Pen::full(color, width, PenStyle::Solid, Cap::Round, Join::Bevel)
}

type Line = ((f64, f64), (f64, f64));

/// `plush(cx, cy, rx, ry, color, hairs)` inside `_render_regal`.
#[allow(clippy::too_many_arguments)]
fn plush(p: &mut Canvas, size: f64, fur: Color, cx: f64, cy: f64, rx: f64, ry: f64, color: Color, hairs: usize) {
    let hairs = if size < 40.0 { hairs / 2 } else { hairs };
    let gradient = RadialGradient::new(cx - rx * 0.3, cy - ry * 0.4, rx.max(ry) * 1.6)
        .with_stop(0.0, qt_lighter(color, 155))
        .with_stop(0.6, color)
        .with_stop(1.0, qt_darker(color, 170));
    p.set_pen(Pen::NONE);
    p.set_brush(gradient);
    p.draw_ellipse_at(cx, cy, rx, ry);
    p.set_pen(round_cap(fur, 0.009));
    let mut lines: Vec<Line> = Vec::with_capacity(hairs);
    for i in 0..hairs {
        let fi = i as f64;
        let a = fi * TAU / hairs as f64 + (fi * 4.3).sin() * 0.014;
        let length = 0.035 + 0.07 * (0.5 + 0.5 * (fi * 7.3).sin());
        let (x, y) = (cx + a.cos() * rx * 0.94, cy + a.sin() * ry * 0.94);
        lines.push(((x, y), (x + (a + 0.3).cos() * length, y + (a + 0.3).sin() * length)));
    }
    p.draw_lines(&lines);
    if hairs > 0 {
        p.set_pen(round_cap(qt_darker(fur, 140), 0.008));
        lines.clear();
        for i in 0..hairs {
            let fi = i as f64;
            let a = fi * 2.39996;
            let radius = ((fi + 0.5) / hairs as f64).sqrt() * 0.9;
            let (x, y) = (cx + a.cos() * rx * radius, cy + a.sin() * ry * radius);
            lines.push(((x, y), (x + a.cos() * 0.028, y + a.sin() * 0.035)));
        }
        p.draw_lines(&lines);
    }
}

/// `_render_regal(painter)`.
pub(crate) fn render_regal(p: &mut Canvas, cx: &Paint) {
    let d = cx.d;
    let r = d.regal;
    let size = d.size;
    let fur = cx.qcolor("highlight", 255);
    let body = cx.qcolor("body", 255);
    let legs = cx.qcolor("legs", 255);
    p.save();
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(0, 0, 0, 32));
    let shrink = 1.0 / (1.0 + d.jump_z / size.max(1.0));
    p.draw_ellipse_at(d.x, d.y + size * 0.25, size * 0.9 * shrink, size * 0.5 * shrink);
    for leg in d.legs.iter().rev() {
        let Some(points) = leg.regal_points else { continue };
        for i in 0..3 {
            let (a, b) = (points[i], points[i + 1]);
            let width = size * [0.20, 0.15, 0.085][i];
            p.set_pen(round_cap(legs, width));
            p.set_brush(Brush::None);
            let mut curve = Path::new();
            curve.move_to(a.0, a.1);
            curve.quad_to((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5 - width * 0.22, b.0, b.1);
            p.draw_path(&curve);
            p.set_pen(round_cap(qt_darker(fur, 125), width * 0.94));
            p.draw_line(
                a.0 + (b.0 - a.0) * 0.28,
                a.1 + (b.1 - a.1) * 0.28,
                a.0 + (b.0 - a.0) * 0.46,
                a.1 + (b.1 - a.1) * 0.46,
            );
            let mut groups: [Vec<Line>; 2] = [Vec::new(), Vec::new()];
            let count = if size < 40.0 { 10 } else { 18 };
            for j in 0..count {
                let fj = j as f64;
                let t = 0.12 + fj * 0.65 / count as f64;
                let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                let angle = (b.1 - a.1).atan2(b.0 - a.0) + PI / 2.0 + 0.25 * (fj * 4.7).sin();
                let hair = width * (0.58 + 0.22 * (fj * 2.3).sin());
                for sign in [-1.0, 1.0] {
                    let idx = usize::from(0.23 < t && t < 0.52);
                    groups[idx].push((
                        (x + angle.cos() * width * 0.3 * sign, y + angle.sin() * width * 0.3 * sign),
                        (x + angle.cos() * hair * sign, y + angle.sin() * hair * sign),
                    ));
                }
            }
            for (color, lines) in [qt_darker(fur, 180), fur].into_iter().zip(groups.iter()) {
                p.set_pen(round_cap(color, (size * 0.009).max(0.35)));
                p.draw_lines(lines);
            }
        }
    }
    let (lunge_x, lunge_y) = d.combat_body_offset;
    p.translate(d.x + lunge_x, d.y - d.jump_z + d.body_bob + lunge_y - size * r.bounce);
    p.rotate(d.heading.to_degrees() + 90.0);
    p.scale(size, size);
    p.rotate(r.tilt.to_degrees());

    plush(p, size, fur, 0.0, 0.32, 0.48, 0.65, body, 72);
    p.set_pen(Pen::NONE);
    p.set_brush(fur);
    for (x, y, w, h) in [(0.0, 0.14, 0.17, 0.12), (-0.21, 0.55, 0.09, 0.13), (0.21, 0.55, 0.09, 0.13)] {
        p.draw_ellipse_at(x, y, w, h);
    }
    p.save();
    p.translate(r.yaw * 0.12, -0.32 - r.pitch * 0.09);
    p.rotate(r.yaw.to_degrees() * 0.23);
    if cx.look.regal_face_forward {
        p.scale(1.0, -1.0);
    }
    plush(p, size, fur, 0.0, 0.0, 0.59, 0.48, body, 84);
    p.set_pen(round_cap(fur, 0.065));
    let mut brow = Path::new();
    brow.move_to(-0.46, -0.19);
    brow.quad_to(0.0, -0.45, 0.46, -0.19);
    p.draw_path(&brow);
    let eyes_col = cx.qcolor("eyes", 255);
    for (x, y, radius) in [
        (-0.49, -0.16, 0.08),
        (0.49, -0.16, 0.08),
        (-0.39, -0.33, 0.032),
        (0.39, -0.33, 0.032),
        (-0.29, -0.39, 0.05),
        (0.29, -0.39, 0.05),
        (-0.205, -0.075, 0.182),
        (0.205, -0.075, 0.182),
    ] {
        p.set_pen(Pen::new(Color::rgb(113, 128, 151), 0.016));
        let g = RadialGradient::new(x - radius * 0.28, y - radius * 0.3, radius * 1.5)
            .with_stop(0.0, Color::rgb(43, 59, 86))
            .with_stop(0.55, eyes_col)
            .with_stop(1.0, Color::rgb(3, 6, 12));
        p.set_brush(g);
        p.draw_ellipse_at(x, y, radius, radius);
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgb(249, 252, 255));
        p.draw_ellipse_at(x - radius * 0.32, y - radius * 0.35, radius * 0.23, radius * 0.18);
        if radius > 0.1 {
            p.set_brush(Color::rgba(103, 145, 211, 190));
            p.draw_ellipse_at(x + radius * 0.28, y + radius * 0.42, radius * 0.21, radius * 0.09);
        }
    }
    for side in [-1.0, 1.0] {
        let c = cx.qcolor("accent", 255);
        plush(p, size, fur, side * 0.13, 0.26, 0.115, 0.19, c, 0);
        p.save();
        p.translate(side * (0.29 + r.palps.abs() * 0.045), 0.27 + side * r.palps * 0.065);
        p.rotate(side * (14.0 + r.palps * 22.0));
        p.set_pen(round_cap(legs, 0.11));
        p.draw_line(0.0, -0.13, 0.0, 0.10);
        plush(p, size, fur, 0.0, 0.065, 0.145, 0.205, qt_darker(fur, 110), 36);
        p.restore();
    }
    p.restore();
    p.restore();
    draw_equipment(p, cx);
}
