//! Small painting helpers standing in for what QPainter gave the Python for
//! free: gradients, rounded-rectangle outlines, Bézier paths, tiled textures
//! and the `border-image` nine-slice of `card_frame.png`.

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2, pos2, vec2};

/// The outline of a rounded rectangle, clockwise from the top-left corner.
pub fn rounded_rect_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    if r <= 0.5 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }
    let mut points = Vec::with_capacity(36);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    let steps = 8;
    // Start on the top edge just after the top-left corner.
    for (centre, start) in corners {
        for i in 0..=steps {
            let a = (start + 90.0 * i as f32 / steps as f32).to_radians();
            points.push(centre + vec2(a.cos(), a.sin()) * r);
        }
    }
    points
}

/// Fill a convex polygon, each vertex coloured by `color_at` (a gradient).
pub fn fill_convex_gradient(painter: &Painter, points: &[Pos2], color_at: impl Fn(Pos2) -> Color32) {
    if points.len() < 3 {
        return;
    }
    let centre = points.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / points.len() as f32;
    let centre = centre.to_pos2();
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, color_at(centre));
    for p in points {
        mesh.colored_vertex(*p, color_at(*p));
    }
    let n = points.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

/// `qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 top, stop:1 bottom)` in a
/// rounded rectangle.
pub fn vertical_gradient(painter: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let points = rounded_rect_points(rect, radius);
    let (y0, h) = (rect.top(), rect.height().max(1.0));
    fill_convex_gradient(painter, &points, |p| crate::theme::mix(top, bottom, (p.y - y0) / h));
}

/// A horizontal gradient (`x1:0 .. x2:1`) in a rounded rectangle.
pub fn horizontal_gradient(painter: &Painter, rect: Rect, radius: f32, left: Color32, right: Color32) {
    let points = rounded_rect_points(rect, radius);
    let (x0, w) = (rect.left(), rect.width().max(1.0));
    fill_convex_gradient(painter, &points, |p| crate::theme::mix(left, right, (p.x - x0) / w));
}

/// A radial gradient filling a circle (or an ellipse when `radius` differs
/// per axis): `inner` at `focus`, `outer` at the rim.
pub fn radial_ellipse(painter: &Painter, centre: Pos2, radius: Vec2, focus: Pos2, inner: Color32, outer: Color32) {
    let segments = 40;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(focus, inner);
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(centre + vec2(a.cos() * radius.x, a.sin() * radius.y), outer);
    }
    for i in 0..segments as u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % segments as u32);
    }
    painter.add(Shape::mesh(mesh));
}

/// A soft glow: concentric rings from `color` at `inner_frac` of the radius
/// fading to transparent at the rim (QRadialGradient with two stops).
pub fn glow(painter: &Painter, centre: Pos2, radius: f32, inner_frac: f32, color: Color32) {
    let rings = 10;
    let segments = 40u32;
    let mut mesh = Mesh::default();
    let clear = crate::theme::with_alpha(color, 0);
    for ring in 0..=rings {
        let t = ring as f32 / rings as f32;
        let r = radius * t;
        let c = if t <= inner_frac {
            color
        } else {
            crate::theme::mix(color, clear, (t - inner_frac) / (1.0 - inner_frac).max(1e-3))
        };
        for i in 0..segments {
            let a = i as f32 / segments as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(centre + vec2(a.cos(), a.sin()) * r, c);
        }
    }
    for ring in 0..rings as u32 {
        for i in 0..segments {
            let a = ring * segments + i;
            let b = ring * segments + (i + 1) % segments;
            let c = (ring + 1) * segments + i;
            let d = (ring + 1) * segments + (i + 1) % segments;
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Points along a cubic Bézier from `p0` to `p3`.
pub fn cubic(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, steps: usize) -> Vec<Pos2> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            let v = p0.to_vec2() * (u * u * u)
                + p1.to_vec2() * (3.0 * u * u * t)
                + p2.to_vec2() * (3.0 * u * t * t)
                + p3.to_vec2() * (t * t * t);
            v.to_pos2()
        })
        .collect()
}

/// Points along a quadratic Bézier.
pub fn quad(p0: Pos2, p1: Pos2, p2: Pos2, steps: usize) -> Vec<Pos2> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            (p0.to_vec2() * (u * u) + p1.to_vec2() * (2.0 * u * t) + p2.to_vec2() * (t * t)).to_pos2()
        })
        .collect()
}

/// An open polyline.
pub fn polyline(painter: &Painter, points: Vec<Pos2>, stroke: Stroke) {
    painter.add(Shape::Path(PathShape::line(points, PathStroke::from(stroke))));
}

/// A closed outline.
pub fn closed(painter: &Painter, points: Vec<Pos2>, stroke: Stroke) {
    painter.add(Shape::Path(PathShape::closed_line(points, PathStroke::from(stroke))));
}

/// A dashed outline round a rounded rectangle (`border: 2px dashed`).
pub fn dashed_rounded_rect(painter: &Painter, rect: Rect, radius: f32, stroke: Stroke) {
    let mut points = rounded_rect_points(rect, radius);
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    for shape in Shape::dashed_line(&points, stroke, 6.0, 4.0) {
        painter.add(shape);
    }
}

/// A dashed line between two points (`Qt.DashLine`).
pub fn dashed(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    for shape in Shape::dashed_line(&[a, b], stroke, 6.0, 4.0) {
        painter.add(shape);
    }
}

/// Fill `rect` with a repeating texture of `tile` pixels (a wood tile).
pub fn tiled(painter: &Painter, rect: Rect, texture: TextureId, tile: Vec2, tint: Color32) {
    let uv = Rect::from_min_max(
        pos2(rect.left() / tile.x, rect.top() / tile.y),
        pos2(rect.right() / tile.x, rect.bottom() / tile.y),
    );
    painter.image(texture, rect, uv, tint);
}

/// `border-image: url(card_frame.png) 30 30 30 30 stretch stretch;
/// border-width: 30px` -- the carved frame, corners kept and edges stretched.
pub fn nine_slice(painter: &Painter, rect: Rect, texture: TextureId, image_size: Vec2, border: f32) {
    let bx = (border / image_size.x).min(0.5);
    let by = (border / image_size.y).min(0.5);
    let b = border.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let xs = [rect.left(), rect.left() + b, rect.right() - b, rect.right()];
    let ys = [rect.top(), rect.top() + b, rect.bottom() - b, rect.bottom()];
    let us = [0.0, bx, 1.0 - bx, 1.0];
    let vs = [0.0, by, 1.0 - by, 1.0];
    let mut mesh = Mesh::with_texture(texture);
    for row in 0..3 {
        for col in 0..3 {
            let r = Rect::from_min_max(pos2(xs[col], ys[row]), pos2(xs[col + 1], ys[row + 1]));
            let uv = Rect::from_min_max(pos2(us[col], vs[row]), pos2(us[col + 1], vs[row + 1]));
            mesh.add_rect_with_uv(r, uv, Color32::WHITE);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Text with a one-pixel lip under it, as if cut into the board
/// (`mode_menu._engrave`'s drop shadow).
pub fn engraved_text(
    painter: &Painter,
    pos: Pos2,
    anchor: egui::Align2,
    text: &str,
    font: egui::FontId,
    color: Color32,
    light_lip: bool,
) -> Rect {
    let lip = if light_lip { crate::theme::rgba(255, 226, 170, 150) } else { crate::theme::rgba(20, 9, 2, 200) };
    painter.text(pos + vec2(0.0, 1.5), anchor, text, font.clone(), lip);
    painter.text(pos, anchor, text, font, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_outline_stays_inside() {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 40.0));
        for p in rounded_rect_points(rect, 7.0) {
            assert!(rect.expand(0.01).contains(p), "{p:?}");
        }
    }

    #[test]
    fn bezier_ends_at_its_end_points() {
        let pts = cubic(pos2(0.0, 0.0), pos2(1.0, 5.0), pos2(4.0, 5.0), pos2(5.0, 0.0), 10);
        assert_eq!(pts.first().copied(), Some(pos2(0.0, 0.0)));
        assert!((pts.last().unwrap().x - 5.0).abs() < 1e-4);
    }
}
