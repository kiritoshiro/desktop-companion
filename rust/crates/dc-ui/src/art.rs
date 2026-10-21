//! Port of app/skill_art.py, plus the small icons config_ui.py painted for
//! itself (the remove cross, the random-skin dice, the fallback spider, the
//! colour swatches and team dots).
//!
//! Every skill gets a round brass medallion with its own picture -- a heart
//! for Vitality, a shell for Hardened carapace, fangs for Power strike, a web
//! for Web crafter -- and every branch a crest over its column. Locked skills
//! show the same picture dark and drained; learned ones glow. The pictures
//! are drawn straight into the egui painter in the same 64x64 box the Python
//! used, so they are crisp at any scale.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::paint;
use crate::theme::{self, hex, mix, rgb, rgba, with_alpha};

/// Which branch each column of the tree is (character_ui.TREE_LAYOUT).
pub const BRANCHES: &[(&str, &str)] = &[("body", "Body"), ("fangs", "Fangs"), ("legs", "Legs"), ("silk", "Silk")];

/// Which branch a skill belongs to.
pub fn skill_branch(ability_id: &str) -> &'static str {
    match ability_id {
        "vitality" | "carapace_harden" => "body",
        "power_strike" | "apex_predator" => "fangs",
        "quick_step" | "long_stride" => "legs",
        "silk_sense" | "web_crafter" | "silk_tracking" => "silk",
        _ => "body",
    }
}

/// `BRANCH_COLOURS`.
pub fn branch_colour(branch: &str) -> Color32 {
    match branch {
        "fangs" => hex("#d89a2c"),
        "legs" => hex("#5e9c4c"),
        "silk" => hex("#6aa6c8"),
        _ => hex("#c0503c"),
    }
}

const INK: Color32 = rgb(40, 24, 12);
const CREAM: Color32 = rgb(250, 238, 210);

/// How a medallion looks: `"learned"`, `"available"` or `"locked"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Learned,
    Available,
    Locked,
}

impl Look {
    pub fn from_name(name: &str) -> Look {
        match name {
            "learned" | "unlocked" => Look::Learned,
            "locked" => Look::Locked,
            _ => Look::Available,
        }
    }
}

/// The painter for one 64x64 picture box mapped onto a screen rectangle.
struct Canvas<'a> {
    painter: &'a Painter,
    origin: Pos2,
    scale: f32,
    /// The picture sits at 78% inside the medallion.
    inset: bool,
    look: Look,
}

impl Canvas<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        let (x, y) = if self.inset { (32.0 + (x - 32.0) * 0.78, 32.0 + (y - 32.0) * 0.78) } else { (x, y) };
        self.origin + vec2(x, y) * self.scale
    }
    fn len(&self, l: f32) -> f32 {
        l * self.scale * if self.inset { 0.78 } else { 1.0 }
    }
    /// Locked pictures are drained to grey and darkened (`Format_Grayscale8`
    /// plus a (30, 20, 10, 120) wash).
    fn c(&self, color: Color32) -> Color32 {
        if self.look != Look::Locked {
            return color;
        }
        let luma = (color.r() as f32 * 0.299 + color.g() as f32 * 0.587 + color.b() as f32 * 0.114).round() as u8;
        let grey = Color32::from_rgba_unmultiplied(luma, luma, luma, color.a());
        let washed = mix(grey, rgb(30, 20, 10), 120.0 / 255.0);
        with_alpha(washed, color.a())
    }
    fn stroke(&self, color: Color32, width: f32) -> Stroke {
        Stroke::new(self.len(width), self.c(color))
    }
    fn pts(&self, raw: &[(f32, f32)]) -> Vec<Pos2> {
        raw.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
    fn line(&self, a: (f32, f32), b: (f32, f32), stroke: Stroke) {
        self.painter.line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], stroke);
    }
    fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, fill: Color32, stroke: Stroke) {
        self.painter.add(egui::Shape::Ellipse(egui::epaint::EllipseShape {
            center: self.p(cx, cy),
            radius: vec2(self.len(rx), self.len(ry)),
            fill: self.c(fill),
            stroke,
            angle: 0.0,
        }));
    }
    /// Fill a star-shaped outline from `centre`, coloured by `color_at` in box
    /// coordinates (a linear gradient), then stroke it.
    fn fill_path(
        &self,
        outline: &[Pos2],
        centre: (f32, f32),
        color_at: &dyn Fn(Pos2) -> Color32,
        stroke: Option<Stroke>,
    ) {
        let mut mesh = egui::epaint::Mesh::default();
        let c = self.p(centre.0, centre.1);
        mesh.colored_vertex(c, self.c(color_at(self.unmap(c))));
        for p in outline {
            mesh.colored_vertex(*p, self.c(color_at(self.unmap(*p))));
        }
        let n = outline.len() as u32;
        for i in 0..n {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        self.painter.add(egui::Shape::mesh(mesh));
        if let Some(stroke) = stroke {
            paint::closed(self.painter, outline.to_vec(), stroke);
        }
    }
    fn unmap(&self, p: Pos2) -> Pos2 {
        let v = (p - self.origin) / self.scale;
        if self.inset { pos2(32.0 + (v.x - 32.0) / 0.78, 32.0 + (v.y - 32.0) / 0.78) } else { v.to_pos2() }
    }
    fn cubic_path(&self, start: (f32, f32), segments: &[[(f32, f32); 3]]) -> Vec<Pos2> {
        let mut points = vec![self.p(start.0, start.1)];
        let mut last = start;
        for seg in segments {
            let pts = paint::cubic(
                pos2(last.0, last.1),
                pos2(seg[0].0, seg[0].1),
                pos2(seg[1].0, seg[1].1),
                pos2(seg[2].0, seg[2].1),
                12,
            );
            points.extend(pts.into_iter().skip(1).map(|p| self.p(p.x, p.y)));
            last = seg[2];
        }
        points
    }
}

fn linear(a: (f32, f32), b: (f32, f32), c0: Color32, c1: Color32) -> impl Fn(Pos2) -> Color32 {
    let d = vec2(b.0 - a.0, b.1 - a.1);
    let len2 = d.length_sq().max(1e-6);
    move |p: Pos2| {
        let t = ((p.x - a.0) * d.x + (p.y - a.1) * d.y) / len2;
        mix(c0, c1, t)
    }
}

// -- the pictures, drawn in a 64 x 64 box ---------------------------------------

fn heart(c: &Canvas) {
    let outline = c.cubic_path(
        (32.0, 48.0),
        &[
            [(12.0, 36.0), (14.0, 16.0), (26.0, 18.0)],
            [(30.0, 18.0), (32.0, 22.0), (32.0, 24.0)],
            [(32.0, 22.0), (34.0, 18.0), (38.0, 18.0)],
            [(50.0, 16.0), (52.0, 36.0), (32.0, 48.0)],
        ],
    );
    let fill = hex("#d8453a");
    c.fill_path(&outline, (32.0, 34.0), &|_| fill, Some(c.stroke(INK, 2.2)));
    c.ellipse(25.0, 25.0, 3.5, 2.5, rgba(255, 220, 210, 170), Stroke::NONE);
}

fn shell(c: &Canvas) {
    let outline = c.cubic_path(
        (32.0, 14.0),
        &[
            [(46.0, 16.0), (50.0, 30.0), (46.0, 40.0)],
            [(42.0, 48.0), (36.0, 51.0), (32.0, 52.0)],
            [(28.0, 51.0), (22.0, 48.0), (18.0, 40.0)],
            [(14.0, 30.0), (18.0, 16.0), (32.0, 14.0)],
        ],
    );
    let grad = linear((20.0, 14.0), (44.0, 52.0), hex("#a07a4c"), hex("#4e3218"));
    c.fill_path(&outline, (32.0, 33.0), &grad, Some(c.stroke(INK, 2.2)));
    let s = c.stroke(hex("#e8c890"), 1.6);
    c.line((32.0, 17.0), (32.0, 49.0), s);
    for (y, w) in [(26.0, 10.0), (34.0, 12.0), (42.0, 9.0)] {
        c.line((32.0 - w, y - 3.0), (32.0, y), s);
        c.line((32.0 + w, y - 3.0), (32.0, y), s);
    }
}

fn fangs(c: &Canvas) {
    for side in [-1.0f32, 1.0] {
        let x = |dx: f32| 32.0 + side * dx;
        let mut outline = c.cubic_path(
            (x(4.0), 16.0),
            &[[(x(16.0), 18.0), (x(16.0), 36.0), (x(6.0), 50.0)], [(x(8.0), 38.0), (x(6.0), 28.0), (x(2.0), 24.0)]],
        );
        outline.dedup();
        let grad = linear((32.0, 16.0), (32.0, 50.0), hex("#f4ead0"), hex("#b89868"));
        c.fill_path(&outline, (x(9.0), 28.0), &grad, Some(c.stroke(INK, 2.0)));
    }
    let venom = hex("#7ad04a");
    c.ellipse(26.0, 52.0, 2.2, 3.0, venom, Stroke::NONE);
    c.ellipse(38.0, 52.0, 2.2, 3.0, venom, Stroke::NONE);
}

fn crown(c: &Canvas) {
    let outline =
        c.pts(&[(14.0, 44.0), (14.0, 22.0), (22.0, 32.0), (32.0, 16.0), (42.0, 32.0), (50.0, 22.0), (50.0, 44.0)]);
    let grad = linear((14.0, 16.0), (50.0, 44.0), hex("#ffe08a"), hex("#b07818"));
    c.fill_path(&outline, (32.0, 38.0), &grad, Some(c.stroke(INK, 2.2)));
    let base = Rect::from_min_max(c.p(14.0, 44.0), c.p(50.0, 50.0));
    c.painter.rect(base, CornerRadius::ZERO, c.c(grad(pos2(32.0, 47.0))), c.stroke(INK, 2.2), StrokeKind::Middle);
    for (x, colour) in [(22.0, "#c83040"), (32.0, "#3a8ad8"), (42.0, "#c83040")] {
        c.ellipse(x, 39.0, 3.0, 3.0, hex(colour), Stroke::NONE);
    }
}

fn legs(c: &Canvas, stride: bool) {
    let s = c.stroke(INK, 3.2);
    let base = (30.0, 34.0);
    for (dx, dy) in [(-16.0f32, -10.0f32), (-18.0, 2.0), (-15.0, 13.0), (-8.0, 20.0)] {
        let knee = (base.0 + dx * 0.6, base.1 + dy - 8.0);
        c.line(base, knee, s);
        c.line(knee, (base.0 + dx, base.1 + dy), s);
    }
    c.ellipse(34.0, 32.0, 8.0, 6.0, hex("#5a3a20"), s);
    let speed = c.stroke(hex("#f0d8a0"), 2.0);
    for (x0, y0, x1, y1) in [(42.0, 22.0, 56.0, 22.0), (44.0, 30.0, 58.0, 30.0), (42.0, 38.0, 54.0, 38.0)] {
        c.line((x0, y0), (x1, y1), speed);
    }
    if stride {
        let arc = paint::quad(pos2(12.0, 52.0), pos2(32.0, 4.0), pos2(54.0, 50.0), 20);
        let points: Vec<Pos2> = arc.iter().map(|p| c.p(p.x, p.y)).collect();
        for shape in egui::Shape::dashed_line(&points, c.stroke(hex("#f0d8a0"), 1.8), c.len(5.0), c.len(3.0)) {
            c.painter.add(shape);
        }
    }
}

fn eye(c: &Canvas) {
    let mut outline = paint::quad(pos2(10.0, 32.0), pos2(32.0, 12.0), pos2(54.0, 32.0), 14);
    outline.extend(paint::quad(pos2(54.0, 32.0), pos2(32.0, 52.0), pos2(10.0, 32.0), 14).into_iter().skip(1));
    outline.pop();
    let outline: Vec<Pos2> = outline.iter().map(|p| c.p(p.x, p.y)).collect();
    c.fill_path(&outline, (32.0, 32.0), &|_| CREAM, Some(c.stroke(INK, 2.2)));
    paint::radial_ellipse(
        c.painter,
        c.p(32.0, 32.0),
        Vec2::splat(c.len(9.0)),
        c.p(32.0, 32.0),
        c.c(hex("#bff0ff")),
        c.c(hex("#2a78a8")),
    );
    c.painter.circle_stroke(c.p(32.0, 32.0), c.len(9.0), c.stroke(INK, 2.2));
    c.ellipse(32.0, 32.0, 3.5, 3.5, INK, Stroke::NONE);
    let ray = c.stroke(hex("#e8f4ff"), 1.2);
    for a in (0..360).step_by(60) {
        let r = (a as f32).to_radians();
        c.line(
            (32.0 + r.cos() * 13.0, 32.0 + r.sin() * 13.0 * 0.7),
            (32.0 + r.cos() * 20.0, 32.0 + r.sin() * 20.0 * 0.7),
            ray,
        );
    }
}

fn web(c: &Canvas, thread: bool) {
    let spokes = 8;
    let s = c.stroke(hex("#f4f0e4"), 1.6);
    for i in 0..spokes {
        let a = i as f32 / spokes as f32 * std::f32::consts::TAU;
        c.line((32.0, 32.0), (32.0 + a.cos() * 22.0, 32.0 + a.sin() * 22.0), s);
    }
    let ring = c.stroke(hex("#e8e0cc"), 1.2);
    for r in [7.0f32, 13.0, 19.0] {
        let pts: Vec<Pos2> = (0..=spokes)
            .map(|i| {
                let a = i as f32 / spokes as f32 * std::f32::consts::TAU;
                c.p(32.0 + a.cos() * r, 32.0 + a.sin() * r)
            })
            .collect();
        paint::polyline(c.painter, pts, ring);
    }
    if thread {
        let gold = hex("#ffe890");
        let pts = paint::cubic(pos2(32.0, 32.0), pos2(44.0, 36.0), pos2(40.0, 50.0), pos2(54.0, 52.0), 16);
        paint::polyline(c.painter, pts.iter().map(|p| c.p(p.x, p.y)).collect(), c.stroke(gold, 2.2));
        c.ellipse(54.0, 52.0, 3.2, 3.2, gold, Stroke::NONE);
    }
    c.ellipse(32.0, 32.0, 4.0, 4.0, INK, Stroke::NONE);
}

/// `PICTURES` / `BRANCH_PICTURES` as one dispatcher.
fn picture(c: &Canvas, name: &str) {
    match name {
        "vitality" | "body" => heart(c),
        "carapace_harden" | "armor" | "worn" => shell(c),
        "power_strike" | "fangs" | "damage" => fangs(c),
        "apex_predator" | "points" => crown(c),
        "quick_step" | "legs" | "speed" => legs(c, false),
        "long_stride" => legs(c, true),
        "silk_sense" => eye(c),
        "web_crafter" | "silk" => web(c, false),
        "silk_tracking" => web(c, true),
        _ => heart(c),
    }
}

/// `_medallion`: the round plate a picture sits on.
fn medallion(c: &Canvas, rim: Color32) {
    let painter = c.painter;
    painter.circle_filled(c.p(33.0, 34.0), c.scale * 29.0, rgba(0, 0, 0, 90));
    let (inner, outer) = if c.look == Look::Locked {
        (rgb(70, 56, 44), rgb(30, 22, 16))
    } else {
        (theme::lighter(rim, 115), theme::darker(rim, 260))
    };
    paint::radial_ellipse(painter, c.p(32.0, 32.0), Vec2::splat(c.scale * 29.0), c.p(28.0, 24.0), inner, outer);
    let (r0, r1) =
        if c.look == Look::Locked { (rgb(110, 96, 80), rgb(60, 50, 40)) } else { (hex("#fbe3a0"), hex("#8a5a1c")) };
    let n = 48;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p0 = c.p(32.0 + a0.cos() * 28.0, 32.0 + a0.sin() * 28.0);
        let p1 = c.p(32.0 + a1.cos() * 28.0, 32.0 + a1.sin() * 28.0);
        let t = ((a0.cos() + a0.sin()) * -0.5 + 1.0) / 2.0;
        painter.line_segment([p0, p1], Stroke::new(c.scale * 3.4, mix(r0, r1, 1.0 - t)));
    }
}

/// `_render`: the medallion and picture filling `rect`.
fn render(painter: &Painter, rect: Rect, name: &str, rim: Color32, look: Look) {
    let size = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(size / 2.0);
    let plate = Canvas { painter, origin, scale: size / 64.0, inset: false, look };
    if look == Look::Learned {
        paint::glow(painter, plate.p(32.0, 32.0), plate.scale * 32.0, 0.7, rgba(255, 220, 120, 150));
    }
    medallion(&plate, rim);
    let inner = Canvas { inset: true, ..plate };
    picture(&inner, name);
}

/// `skill_icon`: a skill's medallion.
pub fn skill_icon(painter: &Painter, rect: Rect, ability_id: &str, look: Look) {
    let branch = skill_branch(ability_id);
    let name = match ability_id {
        "vitality" | "carapace_harden" | "power_strike" | "apex_predator" | "quick_step" | "long_stride"
        | "silk_sense" | "web_crafter" | "silk_tracking" => ability_id,
        _ => branch,
    };
    render(painter, rect, name, branch_colour(branch), look);
}

/// `branch_crest`: the crest over a branch's column.
pub fn branch_crest(painter: &Painter, rect: Rect, branch: &str) {
    render(painter, rect, branch, branch_colour(branch), Look::Available);
}

/// `stat_icon`: the small pictures of the inspector's stat tiles.
pub fn stat_icon(painter: &Painter, rect: Rect, kind: &str) {
    let colour = match kind {
        "damage" => hex("#d89a2c"),
        "points" => hex("#c9a044"),
        "worn" => hex("#6a7a8c"),
        "speed" => hex("#5e9c4c"),
        "silk" => hex("#6aa6c8"),
        _ => hex("#8a6a44"),
    };
    render(painter, rect, kind, colour, Look::Available);
}

// -- config_ui's own icons -------------------------------------------------------

/// `_remove_icon`: a small red cross, drawn rather than shipped.
pub fn remove_cross(painter: &Painter, rect: Rect) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - vec2(12.0, 12.0) * s;
    let stroke = Stroke::new(3.0 * s, rgb(196, 72, 72));
    painter.line_segment([o + vec2(7.0, 7.0) * s, o + vec2(17.0, 17.0) * s], stroke);
    painter.line_segment([o + vec2(17.0, 7.0) * s, o + vec2(7.0, 17.0) * s], stroke);
}

/// `_random_model_icon`: a die for "Random skin at launch".
pub fn random_model_icon(painter: &Painter, rect: Rect) {
    let s = rect.width().min(rect.height()) / 96.0;
    let o = rect.center() - vec2(48.0, 48.0) * s;
    painter.rect_filled(
        Rect::from_min_size(o + vec2(24.0, 24.0) * s, vec2(48.0, 48.0) * s),
        CornerRadius::same((10.0 * s) as u8),
        rgba(52, 47, 60, 235),
    );
    for (x, y) in [(36.0, 36.0), (60.0, 36.0), (48.0, 48.0), (36.0, 60.0), (60.0, 60.0)] {
        painter.circle_filled(o + vec2(x, y) * s, 4.0 * s, rgba(245, 238, 255, 245));
    }
}

/// `_draw_fallback_model_icon`: a colour-based spider when no portrait exists.
pub fn fallback_model_icon(painter: &Painter, rect: Rect, colors: &dyn Fn(&str) -> Option<[u8; 3]>) {
    let s = rect.width().min(rect.height()) / 96.0;
    let o = rect.center() - vec2(48.0, 48.0) * s;
    let col = |name: &str, default: [u8; 3], alpha: u8| {
        let c = colors(name).unwrap_or(default);
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha)
    };
    let ellipse = |x: f32, y: f32, w: f32, h: f32, fill: Color32| {
        painter.add(egui::Shape::ellipse_filled(
            o + vec2(x + w / 2.0, y + h / 2.0) * s,
            vec2(w / 2.0, h / 2.0) * s,
            fill,
        ));
    };
    ellipse(18.0, 39.0, 60.0, 25.0, col("legs", [26, 22, 20], 135));
    let body = col("body", [48, 38, 33], 255);
    ellipse(26.0, 34.0, 31.0, 28.0, body);
    ellipse(51.0, 37.0, 24.0, 22.0, body);
    ellipse(32.0, 39.0, 15.0, 10.0, col("highlight", [95, 82, 70], 175));
    let eyes = col("eyes", [185, 55, 48], 245);
    ellipse(64.0, 43.0, 5.0, 5.0, eyes);
    ellipse(64.0, 51.0, 5.0, 5.0, eyes);
}

/// `_swatch_icon`: up to four bands of a slot's colours, brightly outlined
/// when customised, or a dashed hint when there are none.
pub fn swatch_icon(painter: &Painter, rect: Rect, colors: &[[u8; 3]], custom: bool) {
    let inner = rect.shrink(2.0);
    if colors.is_empty() {
        painter.rect_filled(inner, CornerRadius::same(6), rgba(255, 255, 255, 20));
        paint::dashed_rounded_rect(painter, inner, 6.0, Stroke::new(1.4, rgb(140, 146, 158)));
        return;
    }
    let band = inner.height() / colors.len() as f32;
    for (index, c) in colors.iter().enumerate() {
        let top = inner.top() + band * index as f32;
        painter.rect_filled(
            Rect::from_min_size(pos2(inner.left(), top.round()), vec2(inner.width(), band.round() + 1.0)),
            CornerRadius::ZERO,
            Color32::from_rgb(c[0], c[1], c[2]),
        );
    }
    let stroke = if custom { Stroke::new(2.0, rgba(255, 255, 255, 230)) } else { Stroke::new(1.0, rgba(0, 0, 0, 70)) };
    painter.rect_stroke(inner, CornerRadius::same(6), stroke, StrokeKind::Middle);
}

/// `_palette_icon`: three bands -- legs, body, band -- so a palette is
/// recognisable in the list.
pub fn palette_icon(painter: &Painter, rect: Rect, legs: [u8; 3], body: [u8; 3], band: [u8; 3]) {
    let h = rect.height() / 3.0;
    for (index, c) in [legs, body, band].into_iter().enumerate() {
        painter.rect_filled(
            Rect::from_min_size(pos2(rect.left(), rect.top() + h * index as f32), vec2(rect.width(), h + 1.0)),
            CornerRadius::ZERO,
            Color32::from_rgb(c[0], c[1], c[2]),
        );
    }
}

/// `_team_icon`: a filled dot with a firm outline (neutral is white).
pub fn team_icon(painter: &Painter, rect: Rect, color: [u8; 3]) {
    let r = rect.width().min(rect.height()) / 2.0 - 1.0;
    painter.circle(
        rect.center(),
        r,
        Color32::from_rgb(color[0], color[1], color[2]),
        Stroke::new(1.3, rgba(40, 44, 54, 170)),
    );
}

/// A stand-in armour picture by slot, coloured by tier, until dc-paint's
/// `armour_icon` is wired in through [`crate::model::ArmourIconFn`].
pub fn armour_placeholder(painter: &Painter, rect: Rect, slot: &str, tier_colour: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 64.0;
    let fill = theme::darker(tier_colour, 160);
    let stroke = Stroke::new(2.0 * s, tier_colour);
    match slot {
        "head" => {
            painter.add(egui::Shape::ellipse_filled(c, vec2(18.0, 14.0) * s, fill));
            painter.add(egui::Shape::ellipse_stroke(c, vec2(18.0, 14.0) * s, stroke));
            for dx in [-6.0f32, 0.0, 6.0] {
                painter.circle_filled(c + vec2(dx, -2.0) * s, 2.5 * s, theme::CREAM);
            }
        }
        "carapace" => {
            painter.add(egui::Shape::ellipse_filled(c, vec2(20.0, 16.0) * s, fill));
            painter.add(egui::Shape::ellipse_stroke(c, vec2(20.0, 16.0) * s, stroke));
            painter.circle_filled(c, 3.0 * s, tier_colour);
        }
        "abdomen" => {
            painter.add(egui::Shape::ellipse_filled(c, vec2(16.0, 22.0) * s, fill));
            for i in -1..=1 {
                let y = c.y + i as f32 * 8.0 * s;
                painter.line_segment([pos2(c.x - 12.0 * s, y), pos2(c.x + 12.0 * s, y)], stroke);
            }
        }
        "legs" => {
            for i in 0..4 {
                let y = c.y + (i as f32 - 1.5) * 9.0 * s;
                painter.line_segment(
                    [pos2(c.x - 20.0 * s, y - 4.0 * s), pos2(c.x + 20.0 * s, y + 4.0 * s)],
                    Stroke::new(4.0 * s, fill),
                );
                painter.line_segment(
                    [pos2(c.x - 20.0 * s, y - 4.0 * s), pos2(c.x + 20.0 * s, y + 4.0 * s)],
                    Stroke::new(1.2 * s, tier_colour),
                );
            }
        }
        _ => {
            for side in [-1.0f32, 1.0] {
                let base = c + vec2(side * 8.0, 10.0) * s;
                painter.line_segment([base, c + vec2(side * 14.0, -14.0) * s], Stroke::new(6.0 * s, fill));
                painter.line_segment([base, c + vec2(side * 14.0, -14.0) * s], Stroke::new(1.5 * s, tier_colour));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_a_branch_and_a_colour() {
        for id in [
            "vitality",
            "carapace_harden",
            "power_strike",
            "apex_predator",
            "quick_step",
            "long_stride",
            "silk_sense",
            "web_crafter",
            "silk_tracking",
        ] {
            let branch = skill_branch(id);
            assert!(BRANCHES.iter().any(|(b, _)| *b == branch), "{id}");
        }
        assert_eq!(branch_colour("silk"), hex("#6aa6c8"));
    }

    #[test]
    fn locked_pictures_are_drained() {
        let ctx = egui::Context::default();
        let painter = Painter::new(ctx, egui::LayerId::background(), Rect::EVERYTHING);
        let c = Canvas { painter: &painter, origin: Pos2::ZERO, scale: 1.0, inset: false, look: Look::Locked };
        let red = c.c(rgb(220, 40, 40));
        assert!((red.r() as i32 - red.g() as i32).abs() < 20, "{red:?}");
    }
}
