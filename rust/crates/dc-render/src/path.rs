//! Vector paths with `QPainterPath` semantics.
//!
//! * A fresh path behaves as if it started with `MoveTo(0, 0)`.
//! * `move_to` after `move_to` replaces the dangling start point.
//! * `line_to` to the current point is skipped, as in Qt.
//! * `close_subpath` makes the next drawing call start at the subpath start.
//! * The default fill rule is [`FillRule::OddEven`], like `QPainterPath`.
//! * Arcs use Qt's angle convention: degrees, 0° at 3 o'clock, positive
//!   counter-clockwise on screen.
//!
//! Coordinates are stored as `f32`; the API takes `f64` like the Python code.

use crate::transform::{Transform, sin_cos_deg};
use dc_core::{Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Verb {
    MoveTo,
    LineTo,
    QuadTo,
    CubicTo,
    Close,
}

impl Verb {
    #[inline]
    pub fn point_count(self) -> usize {
        match self {
            Verb::MoveTo | Verb::LineTo => 1,
            Verb::QuadTo => 2,
            Verb::CubicTo => 3,
            Verb::Close => 0,
        }
    }
}

/// `Qt::FillRule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    #[default]
    OddEven,
    Winding,
}

/// One path element, as yielded by [`Path::elements`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathEl {
    MoveTo([f32; 2]),
    LineTo([f32; 2]),
    QuadTo([f32; 2], [f32; 2]),
    CubicTo([f32; 2], [f32; 2], [f32; 2]),
    Close,
}

/// `QPainterPath`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub(crate) verbs: Vec<Verb>,
    pub(crate) points: Vec<[f32; 2]>,
    fill_rule: FillRule,
    start: [f32; 2],
    need_move: bool,
}

#[inline]
fn p32(x: f64, y: f64) -> [f32; 2] {
    [x as f32, y as f32]
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(verbs: usize) -> Self {
        Self { verbs: Vec::with_capacity(verbs), points: Vec::with_capacity(verbs * 2), ..Self::default() }
    }

    /// Empties the path but keeps its buffers (and fill rule).
    pub fn clear(&mut self) {
        self.verbs.clear();
        self.points.clear();
        self.start = [0.0; 2];
        self.need_move = false;
    }

    /// `QPainterPath::isEmpty`: no elements, or only a dangling move.
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty() || (self.verbs.len() == 1 && self.verbs[0] == Verb::MoveTo)
    }

    pub fn verb_count(&self) -> usize {
        self.verbs.len()
    }

    /// Qt's `elementCount` (each curve control point is an element).
    pub fn element_count(&self) -> usize {
        self.verbs.iter().filter(|v| **v != Verb::Close).map(|v| v.point_count()).sum()
    }

    pub fn verbs(&self) -> &[Verb] {
        &self.verbs
    }

    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    pub fn fill_rule(&self) -> FillRule {
        self.fill_rule
    }

    pub fn set_fill_rule(&mut self, rule: FillRule) {
        self.fill_rule = rule;
    }

    /// The point the next drawing call starts from.
    pub fn current_position(&self) -> Vec2 {
        let p = if self.need_move { self.start } else { self.points.last().copied().unwrap_or([0.0; 2]) };
        Vec2::new(p[0] as f64, p[1] as f64)
    }

    #[inline]
    fn ensure_move(&mut self) {
        if self.verbs.is_empty() {
            self.verbs.push(Verb::MoveTo);
            self.points.push([0.0; 2]);
            self.start = [0.0; 2];
        } else if self.need_move {
            self.verbs.push(Verb::MoveTo);
            self.points.push(self.start);
        }
        self.need_move = false;
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        let p = p32(x, y);
        if self.verbs.last() == Some(&Verb::MoveTo) {
            *self.points.last_mut().unwrap() = p;
        } else {
            self.verbs.push(Verb::MoveTo);
            self.points.push(p);
        }
        self.start = p;
        self.need_move = false;
    }

    pub fn move_to_p(&mut self, p: impl Into<Vec2>) {
        let p = p.into();
        self.move_to(p.x, p.y);
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        let p = p32(x, y);
        self.ensure_move();
        if self.points.last() == Some(&p) {
            return;
        }
        self.verbs.push(Verb::LineTo);
        self.points.push(p);
    }

    pub fn line_to_p(&mut self, p: impl Into<Vec2>) {
        let p = p.into();
        self.line_to(p.x, p.y);
    }

    /// `quadTo(ctrl, end)`.
    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.ensure_move();
        let (c, p) = (p32(cx, cy), p32(x, y));
        if self.points.last() == Some(&c) && c == p {
            return;
        }
        self.verbs.push(Verb::QuadTo);
        self.points.push(c);
        self.points.push(p);
    }

    pub fn quad_to_p(&mut self, c: impl Into<Vec2>, p: impl Into<Vec2>) {
        let (c, p) = (c.into(), p.into());
        self.quad_to(c.x, c.y, p.x, p.y);
    }

    /// `cubicTo(c1, c2, end)`.
    pub fn cubic_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        self.ensure_move();
        let (a, b, p) = (p32(c1x, c1y), p32(c2x, c2y), p32(x, y));
        if self.points.last() == Some(&a) && a == b && b == p {
            return;
        }
        self.verbs.push(Verb::CubicTo);
        self.points.push(a);
        self.points.push(b);
        self.points.push(p);
    }

    pub fn cubic_to_p(&mut self, c1: impl Into<Vec2>, c2: impl Into<Vec2>, p: impl Into<Vec2>) {
        let (a, b, p) = (c1.into(), c2.into(), p.into());
        self.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y);
    }

    /// `closeSubpath`.
    pub fn close_subpath(&mut self) {
        if self.need_move {
            return;
        }
        match self.verbs.last() {
            None | Some(Verb::MoveTo) | Some(Verb::Close) => {}
            _ => {
                self.verbs.push(Verb::Close);
            }
        }
        self.need_move = true;
    }

    /// `arcMoveTo(rect, angle)`.
    pub fn arc_move_to(&mut self, rect: Rect, angle_deg: f64) {
        let (x, y) = arc_point(rect, angle_deg);
        self.move_to(x, y);
    }

    /// `arcTo(rect, startAngle, sweepLength)`: a line to the arc start, then
    /// the arc as cubic segments of at most 90°.
    pub fn arc_to(&mut self, rect: Rect, start_deg: f64, sweep_deg: f64) {
        if rect.w == 0.0 && rect.h == 0.0 {
            return;
        }
        let (sx, sy) = arc_point(rect, start_deg);
        self.line_to(sx, sy);
        self.arc_segments(rect, start_deg, sweep_deg);
    }

    fn arc_segments(&mut self, rect: Rect, start_deg: f64, sweep_deg: f64) {
        let sweep = sweep_deg.clamp(-360.0, 360.0);
        if sweep == 0.0 {
            return;
        }
        let n = ((sweep.abs() / 90.0) - 1e-9).ceil().max(1.0) as usize;
        let seg = sweep / n as f64;
        let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
        let k = 4.0 / 3.0 * (seg.to_radians() / 4.0).tan();
        for i in 0..n {
            let a0 = start_deg + seg * i as f64;
            let a1 = a0 + seg;
            let (s0, c0) = sin_cos_deg(a0);
            let (s1, c1) = sin_cos_deg(a1);
            // Unit point (cos a, -sin a); its derivative (-sin a, -cos a).
            let p0 = (c0, -s0);
            let p3 = (c1, -s1);
            let c1p = (p0.0 - k * s0, p0.1 - k * c0);
            let c2p = (p3.0 + k * s1, p3.1 + k * c1);
            self.cubic_to(
                cx + rx * c1p.0,
                cy + ry * c1p.1,
                cx + rx * c2p.0,
                cy + ry * c2p.1,
                cx + rx * p3.0,
                cy + ry * p3.1,
            );
        }
    }

    /// `addRect`. A null rect (zero width and height) adds nothing, as in Qt.
    pub fn add_rect(&mut self, r: Rect) {
        if r.w == 0.0 && r.h == 0.0 {
            return;
        }
        self.move_to(r.x, r.y);
        self.line_to(r.x + r.w, r.y);
        self.line_to(r.x + r.w, r.y + r.h);
        self.line_to(r.x, r.y + r.h);
        self.close_subpath();
    }

    /// `addEllipse(rect)`: clockwise from 3 o'clock, closed.
    pub fn add_ellipse(&mut self, r: Rect) {
        if r.w == 0.0 && r.h == 0.0 {
            return;
        }
        let (x, y) = arc_point(r, 0.0);
        self.move_to(x, y);
        self.arc_segments(r, 0.0, -360.0);
        self.close_subpath();
    }

    /// `addEllipse(QPointF center, rx, ry)`.
    pub fn add_ellipse_c(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        self.add_ellipse(Rect::new(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry));
    }

    /// `addRoundedRect(rect, xRadius, yRadius)` with `Qt::AbsoluteSize`.
    pub fn add_rounded_rect(&mut self, r: Rect, xr: f64, yr: f64) {
        let r = normalized(r);
        if r.w == 0.0 && r.h == 0.0 {
            return;
        }
        let rx = xr.min(r.w / 2.0);
        let ry = yr.min(r.h / 2.0);
        if rx <= 0.0 || ry <= 0.0 {
            self.add_rect(r);
            return;
        }
        let (rxx2, ryy2) = (rx * 2.0, ry * 2.0);
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        self.arc_move_to(Rect::new(x, y, rxx2, ryy2), 180.0);
        self.arc_to(Rect::new(x, y, rxx2, ryy2), 180.0, -90.0);
        self.arc_to(Rect::new(x + w - rxx2, y, rxx2, ryy2), 90.0, -90.0);
        self.arc_to(Rect::new(x + w - rxx2, y + h - ryy2, rxx2, ryy2), 0.0, -90.0);
        self.arc_to(Rect::new(x, y + h - ryy2, rxx2, ryy2), 270.0, -90.0);
        self.close_subpath();
    }

    /// `addRoundedRect(rect, xr, yr, Qt::RelativeSize)`: radii in percent of half the size.
    pub fn add_rounded_rect_relative(&mut self, r: Rect, xr_pct: f64, yr_pct: f64) {
        let r = normalized(r);
        self.add_rounded_rect(
            r,
            r.w / 2.0 * xr_pct.clamp(0.0, 100.0) / 100.0,
            r.h / 2.0 * yr_pct.clamp(0.0, 100.0) / 100.0,
        );
    }

    /// `addPolygon`: a move to the first point and lines to the rest (not closed).
    pub fn add_polygon<P: Into<Vec2> + Copy>(&mut self, pts: &[P]) {
        let mut it = pts.iter();
        if let Some(&p) = it.next() {
            self.move_to_p(p);
            for &p in it {
                self.line_to_p(p);
            }
        }
    }

    /// `addPath`: appends the other path's subpaths.
    pub fn add_path(&mut self, other: &Path) {
        if other.verbs.is_empty() {
            return;
        }
        if self.verbs.last() == Some(&Verb::MoveTo) {
            self.verbs.pop();
            self.points.pop();
        }
        self.verbs.extend_from_slice(&other.verbs);
        self.points.extend_from_slice(&other.points);
        self.start = other.start;
        self.need_move = other.need_move;
    }

    /// `QPainterPath::translate`.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let (dx, dy) = (dx as f32, dy as f32);
        for p in &mut self.points {
            p[0] += dx;
            p[1] += dy;
        }
        self.start[0] += dx;
        self.start[1] += dy;
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Path {
        let mut p = self.clone();
        p.translate(dx, dy);
        p
    }

    /// `QTransform::map(path)`.
    pub fn transformed(&self, t: &Transform) -> Path {
        let mut p = self.clone();
        let m = t.to_f32();
        for q in &mut p.points {
            *q = crate::transform::map_f32(&m, q[0], q[1]);
        }
        p.start = crate::transform::map_f32(&m, p.start[0], p.start[1]);
        p
    }

    pub fn elements(&self) -> impl Iterator<Item = PathEl> + '_ {
        PathRef::new(&self.verbs, &self.points).elements()
    }

    pub(crate) fn as_ref(&self) -> PathRef<'_> {
        PathRef::new(&self.verbs, &self.points)
    }

    /// `controlPointRect`.
    pub fn control_point_rect(&self) -> Rect {
        bounds_of(self.points.iter().copied())
    }

    /// `boundingRect`: tight bounds including curve extrema.
    pub fn bounding_rect(&self) -> Rect {
        self.as_ref().bounding_rect()
    }

    /// `contains(QPointF)` under the path's fill rule (subpaths implicitly closed).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.as_ref().contains(x as f32, y as f32, self.fill_rule)
    }

    pub fn contains_p(&self, p: impl Into<Vec2>) -> bool {
        let p = p.into();
        self.contains(p.x, p.y)
    }

    /// `contains(QRectF)`: the whole rectangle is inside the filled area.
    pub fn contains_rect(&self, r: Rect) -> bool {
        let corners = [(r.x, r.y), (r.right(), r.y), (r.right(), r.bottom()), (r.x, r.bottom())];
        if !corners.iter().all(|&(x, y)| self.contains(x, y)) {
            return false;
        }
        // No edge may cross the rectangle's interior.
        let inner = r.inflated(-1e-4);
        !self.as_ref().any_edge_hits_rect(inner)
    }

    /// `intersects(QRectF)`: any part of the rectangle overlaps the filled area
    /// or the outline.
    pub fn intersects_rect(&self, r: Rect) -> bool {
        if self.verbs.is_empty() {
            return false;
        }
        let b = self.bounding_rect();
        if b.right() < r.x || r.right() < b.x || b.bottom() < r.y || r.bottom() < b.y {
            return false;
        }
        let (cx, cy) = r.center();
        if self.contains(cx, cy) || self.contains(r.x, r.y) {
            return true;
        }
        self.as_ref().any_edge_hits_rect(r)
    }

    /// `intersects(QPainterPath)` approximated on flattened outlines: the
    /// outlines cross, or one contains a point of the other.
    pub fn intersects_path(&self, other: &Path) -> bool {
        let (a, b) = (self.bounding_rect(), other.bounding_rect());
        if a.right() < b.x || b.right() < a.x || a.bottom() < b.y || b.bottom() < a.y {
            return false;
        }
        let mut pa = Polylines::default();
        let mut pb = Polylines::default();
        pa.flatten(self.as_ref(), 0.05);
        pb.flatten(other.as_ref(), 0.05);
        for e in pa.edges() {
            for f in pb.edges() {
                if segments_cross(e.0, e.1, f.0, f.1) {
                    return true;
                }
            }
        }
        if let Some(p) = pa.pts.first()
            && other.contains(p[0] as f64, p[1] as f64)
        {
            return true;
        }
        if let Some(p) = pb.pts.first()
            && self.contains(p[0] as f64, p[1] as f64)
        {
            return true;
        }
        false
    }

    /// `QPainterPathStroker(pen).createStroke(path)`: the area the pen covers,
    /// as triangles under the winding rule. Good for hit tests.
    pub fn stroked(&self, pen: &crate::paint::Pen) -> Path {
        let width = if pen.width <= 0.0 { 1.0 } else { pen.width } as f32;
        let mut lines = Polylines::default();
        lines.flatten(self.as_ref(), 0.05);
        if pen.is_dashed() {
            let mut dashed = Polylines::default();
            let pat: Vec<f32> = pen.dash.as_slice().iter().map(|d| d * width).collect();
            crate::stroke::dash(&lines, &pat, pen.dash_offset as f32 * width, &mut dashed);
            lines = dashed;
        }
        let mut tris = crate::stroke::TriSink::default();
        let style = crate::stroke::StrokeParams {
            half_width: width * 0.5,
            cap: pen.cap,
            join: pen.join,
            miter_limit: pen.miter_limit as f32,
            tolerance: 0.05,
        };
        crate::stroke::stroke_polylines(&lines, &style, &mut tris);
        let mut out = Path::with_capacity(tris.indices.len() / 3 * 4);
        out.set_fill_rule(FillRule::Winding);
        for t in tris.indices.as_chunks::<3>().0 {
            let (a, mut b, mut c) = (tris.verts[t[0] as usize], tris.verts[t[1] as usize], tris.verts[t[2] as usize]);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area.abs() < 1e-9 {
                continue;
            }
            if area < 0.0 {
                std::mem::swap(&mut b, &mut c);
            }
            out.move_to(a[0] as f64, a[1] as f64);
            out.line_to(b[0] as f64, b[1] as f64);
            out.line_to(c[0] as f64, c[1] as f64);
            out.close_subpath();
        }
        out
    }

    /// Calls `f(start, end, closed)` with each flattened subpath as a point list.
    pub fn to_subpath_polygons(&self, tolerance: f64) -> Vec<(Vec<Vec2>, bool)> {
        let mut pl = Polylines::default();
        pl.flatten(self.as_ref(), tolerance as f32);
        pl.contours
            .iter()
            .map(|c| {
                (
                    pl.pts[c.start as usize..c.end as usize]
                        .iter()
                        .map(|p| Vec2::new(p[0] as f64, p[1] as f64))
                        .collect(),
                    c.closed,
                )
            })
            .collect()
    }
}

fn normalized(r: Rect) -> Rect {
    let (mut x, mut y, mut w, mut h) = (r.x, r.y, r.w, r.h);
    if w < 0.0 {
        x += w;
        w = -w;
    }
    if h < 0.0 {
        y += h;
        h = -h;
    }
    Rect::new(x, y, w, h)
}

/// Point on the ellipse inscribed in `rect` at a Qt angle.
pub fn arc_point(rect: Rect, angle_deg: f64) -> (f64, f64) {
    let (s, c) = sin_cos_deg(angle_deg);
    (rect.x + rect.w / 2.0 + rect.w / 2.0 * c, rect.y + rect.h / 2.0 - rect.h / 2.0 * s)
}

/// Fluent builder over [`Path`].
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    path: Path,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn move_to(mut self, x: f64, y: f64) -> Self {
        self.path.move_to(x, y);
        self
    }
    pub fn line_to(mut self, x: f64, y: f64) -> Self {
        self.path.line_to(x, y);
        self
    }
    pub fn quad_to(mut self, cx: f64, cy: f64, x: f64, y: f64) -> Self {
        self.path.quad_to(cx, cy, x, y);
        self
    }
    pub fn cubic_to(mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) -> Self {
        self.path.cubic_to(c1x, c1y, c2x, c2y, x, y);
        self
    }
    pub fn arc_to(mut self, rect: Rect, start_deg: f64, sweep_deg: f64) -> Self {
        self.path.arc_to(rect, start_deg, sweep_deg);
        self
    }
    pub fn rect(mut self, r: Rect) -> Self {
        self.path.add_rect(r);
        self
    }
    pub fn ellipse(mut self, r: Rect) -> Self {
        self.path.add_ellipse(r);
        self
    }
    pub fn rounded_rect(mut self, r: Rect, xr: f64, yr: f64) -> Self {
        self.path.add_rounded_rect(r, xr, yr);
        self
    }
    pub fn polygon<P: Into<Vec2> + Copy>(mut self, pts: &[P]) -> Self {
        self.path.add_polygon(pts);
        self
    }
    pub fn close(mut self) -> Self {
        self.path.close_subpath();
        self
    }
    pub fn fill_rule(mut self, rule: FillRule) -> Self {
        self.path.set_fill_rule(rule);
        self
    }
    pub fn build(self) -> Path {
        self.path
    }
}

// ---------------------------------------------------------------------------
// Borrowed path view, flattening and geometry queries (shared with the scene arena).

#[derive(Clone, Copy)]
pub(crate) struct PathRef<'a> {
    pub verbs: &'a [Verb],
    pub points: &'a [[f32; 2]],
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FlatEv {
    Begin([f32; 2]),
    Line([f32; 2]),
    End { closed: bool },
}

impl<'a> PathRef<'a> {
    pub fn new(verbs: &'a [Verb], points: &'a [[f32; 2]]) -> Self {
        Self { verbs, points }
    }

    pub fn elements(self) -> impl Iterator<Item = PathEl> + 'a {
        let mut i = 0usize;
        self.verbs.iter().map(move |v| {
            let p = self.points;
            let el = match v {
                Verb::MoveTo => PathEl::MoveTo(p[i]),
                Verb::LineTo => PathEl::LineTo(p[i]),
                Verb::QuadTo => PathEl::QuadTo(p[i], p[i + 1]),
                Verb::CubicTo => PathEl::CubicTo(p[i], p[i + 1], p[i + 2]),
                Verb::Close => PathEl::Close,
            };
            i += v.point_count();
            el
        })
    }

    /// Flattens curves to lines within `tol` and reports contours.
    pub fn flatten_visit(self, tol: f32, f: &mut impl FnMut(FlatEv)) {
        let tol = tol.max(1e-4);
        let mut open = false;
        let mut cur = [0.0f32; 2];
        let mut start = [0.0f32; 2];
        for el in self.elements() {
            match el {
                PathEl::MoveTo(p) => {
                    if open {
                        f(FlatEv::End { closed: false });
                    }
                    f(FlatEv::Begin(p));
                    open = true;
                    cur = p;
                    start = p;
                }
                PathEl::LineTo(p) => {
                    if !open {
                        f(FlatEv::Begin(cur));
                        open = true;
                        start = cur;
                    }
                    f(FlatEv::Line(p));
                    cur = p;
                }
                PathEl::QuadTo(c, p) => {
                    if !open {
                        f(FlatEv::Begin(cur));
                        open = true;
                        start = cur;
                    }
                    flatten_quad(cur, c, p, tol, f);
                    cur = p;
                }
                PathEl::CubicTo(a, b, p) => {
                    if !open {
                        f(FlatEv::Begin(cur));
                        open = true;
                        start = cur;
                    }
                    flatten_cubic(cur, a, b, p, tol, f);
                    cur = p;
                }
                PathEl::Close => {
                    if open {
                        f(FlatEv::End { closed: true });
                        open = false;
                    }
                    cur = start;
                }
            }
        }
        if open {
            f(FlatEv::End { closed: false });
        }
    }

    pub fn bounding_rect(self) -> Rect {
        let mut b = BoundsAcc::default();
        let mut cur = [0.0f32; 2];
        let mut start = [0.0f32; 2];
        for el in self.elements() {
            match el {
                PathEl::MoveTo(p) => {
                    b.add(p);
                    cur = p;
                    start = p;
                }
                PathEl::LineTo(p) => {
                    b.add(p);
                    cur = p;
                }
                PathEl::QuadTo(c, p) => {
                    b.add(p);
                    for axis in 0..2 {
                        let d = cur[axis] - 2.0 * c[axis] + p[axis];
                        if d.abs() > 1e-12 {
                            let t = (cur[axis] - c[axis]) / d;
                            if t > 0.0 && t < 1.0 {
                                b.add(quad_at(cur, c, p, t));
                            }
                        }
                    }
                    cur = p;
                }
                PathEl::CubicTo(c1, c2, p) => {
                    b.add(p);
                    for axis in 0..2 {
                        let (p0, p1, p2, p3) = (cur[axis], c1[axis], c2[axis], p[axis]);
                        let a = p3 - 3.0 * p2 + 3.0 * p1 - p0;
                        let bb = 2.0 * (p2 - 2.0 * p1 + p0);
                        let c = p1 - p0;
                        for t in quadratic_roots(a, bb, c).into_iter().flatten() {
                            if t > 0.0 && t < 1.0 {
                                b.add(cubic_at(cur, c1, c2, p, t));
                            }
                        }
                    }
                    cur = p;
                }
                PathEl::Close => cur = start,
            }
        }
        b.rect()
    }

    pub fn contains(self, x: f32, y: f32, rule: FillRule) -> bool {
        let mut winding = 0i32;
        let mut first = [0.0f32; 2];
        let mut prev = [0.0f32; 2];
        let edge = |a: [f32; 2], b: [f32; 2], w: &mut i32| {
            if a[1] <= y {
                if b[1] > y && cross(a, b, [x, y]) > 0.0 {
                    *w += 1;
                }
            } else if b[1] <= y && cross(a, b, [x, y]) < 0.0 {
                *w -= 1;
            }
        };
        self.flatten_visit(0.05, &mut |ev| match ev {
            FlatEv::Begin(p) => {
                first = p;
                prev = p;
            }
            FlatEv::Line(p) => {
                edge(prev, p, &mut winding);
                prev = p;
            }
            FlatEv::End { .. } => edge(prev, first, &mut winding),
        });
        match rule {
            FillRule::OddEven => winding % 2 != 0,
            FillRule::Winding => winding != 0,
        }
    }

    pub fn any_edge_hits_rect(self, r: Rect) -> bool {
        let (l, t, rr, b) = (r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32);
        let mut hit = false;
        let mut first = [0.0f32; 2];
        let mut prev = [0.0f32; 2];
        let test = |a: [f32; 2], c: [f32; 2]| segment_hits_rect(a, c, l, t, rr, b);
        self.flatten_visit(0.05, &mut |ev| {
            if hit {
                return;
            }
            match ev {
                FlatEv::Begin(p) => {
                    first = p;
                    prev = p;
                    hit = test(p, p);
                }
                FlatEv::Line(p) => {
                    hit = test(prev, p);
                    prev = p;
                }
                FlatEv::End { .. } => hit = test(prev, first),
            }
        });
        hit
    }
}

#[inline]
fn cross(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1])
}

fn segments_cross(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> bool {
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Liang–Barsky segment/rectangle overlap.
fn segment_hits_rect(a: [f32; 2], b: [f32; 2], l: f32, t: f32, r: f32, bo: f32) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;
    for (p, q) in [(-dx, a[0] - l), (dx, r - a[0]), (-dy, a[1] - t), (dy, bo - a[1])] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let u = q / p;
            if p < 0.0 {
                if u > t1 {
                    return false;
                }
                t0 = t0.max(u);
            } else {
                if u < t0 {
                    return false;
                }
                t1 = t1.min(u);
            }
        }
    }
    t0 <= t1
}

fn quadratic_roots(a: f32, b: f32, c: f32) -> [Option<f32>; 2] {
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return [None, None];
        }
        return [Some(-c / b), None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    let s = disc.sqrt();
    [Some((-b + s) / (2.0 * a)), Some((-b - s) / (2.0 * a))]
}

#[inline]
fn quad_at(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], t: f32) -> [f32; 2] {
    let mt = 1.0 - t;
    [mt * mt * p0[0] + 2.0 * mt * t * p1[0] + t * t * p2[0], mt * mt * p0[1] + 2.0 * mt * t * p1[1] + t * t * p2[1]]
}

#[inline]
fn cubic_at(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    [a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]
}

#[inline]
fn len2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn flatten_quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], tol: f32, f: &mut impl FnMut(FlatEv)) {
    let dd = len2(p0[0] - 2.0 * p1[0] + p2[0], p0[1] - 2.0 * p1[1] + p2[1]);
    let n = ((dd / (4.0 * tol)).sqrt().ceil() as usize).clamp(1, 256);
    for i in 1..n {
        f(FlatEv::Line(quad_at(p0, p1, p2, i as f32 / n as f32)));
    }
    f(FlatEv::Line(p2));
}

fn flatten_cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], tol: f32, f: &mut impl FnMut(FlatEv)) {
    let d1 = len2(p0[0] - 2.0 * p1[0] + p2[0], p0[1] - 2.0 * p1[1] + p2[1]);
    let d2 = len2(p1[0] - 2.0 * p2[0] + p3[0], p1[1] - 2.0 * p2[1] + p3[1]);
    let dd = d1.max(d2);
    let n = ((0.75 * dd / tol).sqrt().ceil() as usize).clamp(1, 256);
    for i in 1..n {
        f(FlatEv::Line(cubic_at(p0, p1, p2, p3, i as f32 / n as f32)));
    }
    f(FlatEv::Line(p3));
}

#[derive(Default)]
struct BoundsAcc {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
    any: bool,
}

impl BoundsAcc {
    fn add(&mut self, p: [f32; 2]) {
        if !self.any {
            *self = BoundsAcc { l: p[0], t: p[1], r: p[0], b: p[1], any: true };
        } else {
            self.l = self.l.min(p[0]);
            self.t = self.t.min(p[1]);
            self.r = self.r.max(p[0]);
            self.b = self.b.max(p[1]);
        }
    }
    fn rect(&self) -> Rect {
        if !self.any {
            return Rect::default();
        }
        Rect::from_ltrb(self.l as f64, self.t as f64, self.r as f64, self.b as f64)
    }
}

fn bounds_of(it: impl Iterator<Item = [f32; 2]>) -> Rect {
    let mut b = BoundsAcc::default();
    for p in it {
        b.add(p);
    }
    b.rect()
}

/// Flattened contours with reusable buffers.
#[derive(Clone, Debug, Default)]
pub(crate) struct Polylines {
    pub pts: Vec<[f32; 2]>,
    pub contours: Vec<Contour>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Contour {
    pub start: u32,
    pub end: u32,
    pub closed: bool,
}

impl Polylines {
    pub fn clear(&mut self) {
        self.pts.clear();
        self.contours.clear();
    }

    /// Appends the flattened path (does not clear).
    pub fn flatten(&mut self, path: PathRef<'_>, tol: f32) {
        let pts = &mut self.pts;
        let contours = &mut self.contours;
        let mut start = 0u32;
        path.flatten_visit(tol, &mut |ev| match ev {
            FlatEv::Begin(p) => {
                start = pts.len() as u32;
                pts.push(p);
            }
            FlatEv::Line(p) => {
                if pts.last() != Some(&p) {
                    pts.push(p);
                }
            }
            FlatEv::End { closed } => {
                let end = pts.len() as u32;
                let mut closed = closed;
                // A closing point equal to the start is dropped; the contour is closed.
                if end - start > 1 && pts[end as usize - 1] == pts[start as usize] {
                    pts.pop();
                    closed = true;
                }
                contours.push(Contour { start, end: pts.len() as u32, closed });
            }
        });
    }

    pub fn begin(&mut self, p: [f32; 2]) {
        self.contours.push(Contour { start: self.pts.len() as u32, end: self.pts.len() as u32 + 1, closed: false });
        self.pts.push(p);
    }

    pub fn push(&mut self, p: [f32; 2]) {
        self.pts.push(p);
        if let Some(c) = self.contours.last_mut() {
            c.end = self.pts.len() as u32;
        }
    }

    pub fn close(&mut self) {
        if let Some(c) = self.contours.last_mut() {
            c.closed = true;
        }
    }

    pub fn contour(&self, c: &Contour) -> &[[f32; 2]] {
        &self.pts[c.start as usize..c.end as usize]
    }

    /// Every edge, closing edges included for closed contours.
    pub fn edges(&self) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
        self.contours.iter().flat_map(move |c| {
            let s = self.contour(c);
            let n = s.len();
            let count = if c.closed && n > 2 { n } else { n.saturating_sub(1) };
            (0..count).map(move |i| (s[i], s[(i + 1) % n]))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_move_and_dedup() {
        let mut p = Path::new();
        p.line_to(10.0, 0.0);
        assert_eq!(p.verbs(), &[Verb::MoveTo, Verb::LineTo]);
        assert_eq!(p.points()[0], [0.0, 0.0]);
        p.line_to(10.0, 0.0);
        assert_eq!(p.verb_count(), 2);
        let mut q = Path::new();
        q.move_to(1.0, 1.0);
        q.move_to(2.0, 2.0);
        assert_eq!(q.verb_count(), 1);
        assert!(q.is_empty());
    }

    #[test]
    fn close_then_line_starts_at_subpath_start() {
        let mut p = Path::new();
        p.move_to(5.0, 5.0);
        p.line_to(10.0, 5.0);
        p.line_to(10.0, 10.0);
        p.close_subpath();
        assert_eq!(p.current_position(), Vec2::new(5.0, 5.0));
        p.line_to(0.0, 0.0);
        assert_eq!(p.verbs()[4], Verb::MoveTo);
        assert_eq!(p.points()[3], [5.0, 5.0]);
    }

    #[test]
    fn rect_bounds_and_contains() {
        let mut p = Path::new();
        p.add_rect(Rect::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(p.bounding_rect(), Rect::new(10.0, 20.0, 30.0, 40.0));
        assert!(p.contains(15.0, 25.0));
        assert!(!p.contains(5.0, 25.0));
        assert!(p.intersects_rect(Rect::new(0.0, 0.0, 12.0, 22.0)));
        assert!(!p.intersects_rect(Rect::new(0.0, 0.0, 5.0, 5.0)));
        assert!(p.contains_rect(Rect::new(12.0, 22.0, 5.0, 5.0)));
        assert!(!p.contains_rect(Rect::new(5.0, 22.0, 10.0, 5.0)));
    }

    #[test]
    fn ellipse_is_tight_and_round() {
        let mut p = Path::new();
        p.add_ellipse_c(50.0, 50.0, 20.0, 10.0);
        let b = p.bounding_rect();
        assert!((b.x - 30.0).abs() < 1e-3 && (b.w - 40.0).abs() < 1e-3 && (b.h - 20.0).abs() < 1e-3);
        // starts at 3 o'clock, goes clockwise (down) first
        assert_eq!(p.points()[0], [70.0, 50.0]);
        assert_eq!(p.points()[3], [50.0, 60.0]);
        assert!(p.contains(50.0, 50.0) && p.contains(68.0, 50.0) && !p.contains(69.0, 58.0));
        // max radial error of the cubic approximation is tiny
        let polys = p.to_subpath_polygons(0.01);
        for q in &polys[0].0 {
            let r = (((q.x - 50.0) / 20.0).powi(2) + ((q.y - 50.0) / 10.0).powi(2)).sqrt();
            assert!((r - 1.0).abs() < 1e-3, "{r}");
        }
    }

    #[test]
    fn arc_to_follows_qt_angles() {
        let mut p = Path::new();
        let r = Rect::new(0.0, 0.0, 20.0, 20.0);
        p.arc_move_to(r, 0.0);
        p.arc_to(r, 0.0, 90.0);
        let end = p.current_position();
        assert!((end.x - 10.0).abs() < 1e-5 && end.y.abs() < 1e-5, "{end:?}");
        let b = p.bounding_rect();
        assert!((b.right() - 20.0).abs() < 1e-4 && b.y.abs() < 1e-4);
    }

    #[test]
    fn odd_even_vs_winding() {
        let mut p = Path::new();
        p.add_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        p.add_rect(Rect::new(2.0, 2.0, 6.0, 6.0));
        assert!(!p.contains(5.0, 5.0));
        p.set_fill_rule(FillRule::Winding);
        assert!(p.contains(5.0, 5.0));
    }

    #[test]
    fn cubic_bounds_include_extrema() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0);
        p.cubic_to(0.0, 10.0, 10.0, 10.0, 10.0, 0.0);
        let b = p.bounding_rect();
        assert!((b.bottom() - 7.5).abs() < 1e-4, "{b:?}");
        assert!((p.control_point_rect().bottom() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn rounded_rect_and_stroker() {
        let mut p = Path::new();
        p.add_rounded_rect(Rect::new(0.0, 0.0, 40.0, 20.0), 5.0, 5.0);
        let b = p.bounding_rect();
        assert!((b.w - 40.0).abs() < 1e-3 && (b.h - 20.0).abs() < 1e-3);
        assert!(!p.contains(0.3, 0.3));
        assert!(p.contains(2.0, 10.0));
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(100.0, 0.0);
        let s = line.stroked(&crate::paint::Pen::new(dc_core::Color::BLACK, 6.0).cap(crate::paint::Cap::Round));
        assert!(s.contains(50.0, 2.9) && !s.contains(50.0, 3.2));
        assert!(s.contains(-2.5, 0.0) && !s.contains(-3.5, 0.0));
    }

    #[test]
    fn path_intersections() {
        let mut a = Path::new();
        a.add_ellipse_c(0.0, 0.0, 10.0, 10.0);
        let mut b = Path::new();
        b.add_rect(Rect::new(8.0, -1.0, 10.0, 2.0));
        assert!(a.intersects_path(&b));
        let c = b.translated(10.0, 0.0);
        assert!(!a.intersects_path(&c));
    }
}
