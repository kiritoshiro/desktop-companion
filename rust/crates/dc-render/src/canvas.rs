//! [`Canvas`]: the `QPainter` replacement. Calls record into a [`Scene`].
//!
//! Method names follow QPainter in snake_case; overloads get suffixes
//! (`draw_ellipse(rect)` / `draw_ellipse_at(cx, cy, rx, ry)`). Defaults are
//! Qt's: black 1-px square-cap pen, no brush, SourceOver, opacity 1,
//! **antialiasing off** until `set_render_hint(RenderHint::Antialiasing, true)`
//! (the GPU backend always antialiases with MSAA).

use crate::image::Image;
use crate::paint::{Brush, Cap, CompositionMode, Pen, RenderHint};
use crate::path::{FillRule, Path};
use crate::region::Region;
use crate::scene::{Clip, ClipShape, Command, Op, SOLID, Scene, Shape, StrokeStyle};
use crate::text::{Font, FontMetrics, TextFlags, layout_block};
use crate::transform::Transform;
use dc_core::{Color, Rect, Vec2};

/// `Qt::ClipOperation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClipOp {
    NoClip,
    #[default]
    Replace,
    Intersect,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct State {
    t: Transform,
    t32: [f32; 6],
    pen: Pen,
    brush: Brush,
    font: Font,
    opacity: f64,
    comp: CompositionMode,
    clip: u32,
    clip_on: bool,
    aa: bool,
    smooth: bool,
    pen_paint: Option<u32>,
    brush_paint: Option<u32>,
    stroke: Option<u32>,
}

impl Default for State {
    fn default() -> Self {
        State {
            t: Transform::IDENTITY,
            t32: Transform::IDENTITY.to_f32(),
            pen: Pen::default(),
            brush: Brush::None,
            font: Font::default(),
            opacity: 1.0,
            comp: CompositionMode::SourceOver,
            clip: 0,
            clip_on: false,
            aa: false,
            smooth: false,
            pen_paint: None,
            brush_paint: None,
            stroke: None,
        }
    }
}

pub struct Canvas<'s> {
    scene: &'s mut Scene,
    st: State,
    stack: Vec<State>,
}

impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        self.stack.clear();
        self.scene.stack_pool = std::mem::take(&mut self.stack);
    }
}

impl<'s> Canvas<'s> {
    pub fn new(scene: &'s mut Scene) -> Self {
        let stack = std::mem::take(&mut scene.stack_pool);
        if scene.clips.is_empty() {
            scene.clips.push(Clip { scissor: None, shapes: (0, 0) });
        }
        Canvas { scene, st: State::default(), stack }
    }

    // ---------------------------------------------------------------- state

    pub fn save(&mut self) {
        self.stack.push(self.st);
    }

    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.st = s;
        } else {
            log::warn!("Canvas::restore with empty stack");
        }
    }

    pub fn set_pen(&mut self, pen: impl Into<Pen>) {
        self.st.pen = pen.into();
        self.st.pen_paint = None;
        self.st.stroke = None;
    }

    pub fn pen(&self) -> Pen {
        self.st.pen
    }

    pub fn set_brush(&mut self, brush: impl Into<Brush>) {
        self.st.brush = brush.into();
        self.st.brush_paint = None;
    }

    pub fn brush(&self) -> Brush {
        self.st.brush
    }

    pub fn set_font(&mut self, font: Font) {
        self.st.font = font;
    }

    pub fn font(&self) -> Font {
        self.st.font
    }

    pub fn font_metrics(&self) -> FontMetrics {
        FontMetrics::new(&self.st.font)
    }

    /// `setOpacity` (absolute, saved by `save`).
    pub fn set_opacity(&mut self, o: f64) {
        self.st.opacity = o.clamp(0.0, 1.0);
    }

    pub fn opacity(&self) -> f64 {
        self.st.opacity
    }

    pub fn set_composition_mode(&mut self, m: CompositionMode) {
        self.st.comp = m;
    }

    /// Short alias of [`Canvas::set_composition_mode`].
    pub fn set_composition(&mut self, m: CompositionMode) {
        self.st.comp = m;
    }

    pub fn composition_mode(&self) -> CompositionMode {
        self.st.comp
    }

    pub fn set_render_hint(&mut self, hint: RenderHint, on: bool) {
        match hint {
            RenderHint::Antialiasing => self.st.aa = on,
            RenderHint::SmoothPixmapTransform => self.st.smooth = on,
            RenderHint::TextAntialiasing => {}
        }
    }

    /// `setRenderHint(Antialiasing, on)` shorthand.
    pub fn set_antialiasing(&mut self, on: bool) {
        self.st.aa = on;
    }

    // ------------------------------------------------------------ transform

    fn sync_t(&mut self) {
        self.st.t32 = self.st.t.to_f32();
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.st.t.translate(dx, dy);
        self.sync_t();
    }

    /// `QPainter::rotate` (degrees, clockwise on screen).
    pub fn rotate(&mut self, deg: f64) {
        self.st.t.rotate(deg);
        self.sync_t();
    }

    pub fn rotate_deg(&mut self, deg: f64) {
        self.rotate(deg);
    }

    pub fn rotate_rad(&mut self, rad: f64) {
        self.rotate(rad.to_degrees());
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.st.t.scale(sx, sy);
        self.sync_t();
    }

    pub fn shear(&mut self, sh: f64, sv: f64) {
        self.st.t.shear(sh, sv);
        self.sync_t();
    }

    /// `setTransform(t, combine)`: with `combine`, `t` applies before the current transform.
    pub fn set_transform(&mut self, t: &Transform, combine: bool) {
        self.st.t = if combine { t.then(&self.st.t) } else { *t };
        self.sync_t();
    }

    pub fn reset_transform(&mut self) {
        self.st.t = Transform::IDENTITY;
        self.sync_t();
    }

    pub fn world_transform(&self) -> Transform {
        self.st.t
    }

    pub fn transform(&self) -> Transform {
        self.st.t
    }

    // ----------------------------------------------------------------- clip

    fn new_clip(&mut self, op: ClipOp, scissor: Option<[f32; 4]>, shape: Option<ClipShape>) {
        if op == ClipOp::NoClip {
            self.st.clip_on = false;
            self.st.clip = 0;
            return;
        }
        let mut clip = Clip { scissor, shapes: (0, 0) };
        let base = self.scene.clip_shapes.len() as u32;
        if op == ClipOp::Intersect && self.st.clip_on && self.st.clip != 0 {
            let old = self.scene.clips[self.st.clip as usize];
            clip.scissor = match (old.scissor, scissor) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (a, b) => a.or(b),
            };
            for i in old.shapes.0..old.shapes.1 {
                let s = self.scene.clip_shapes[i as usize];
                self.scene.clip_shapes.push(s);
            }
        }
        if let Some(s) = shape {
            self.scene.clip_shapes.push(s);
        }
        clip.shapes = (base, self.scene.clip_shapes.len() as u32);
        self.scene.clips.push(clip);
        self.st.clip = self.scene.clips.len() as u32 - 1;
        self.st.clip_on = true;
    }

    /// `setClipRect(rect)` (replaces the clip).
    pub fn set_clip_rect(&mut self, r: Rect) {
        self.set_clip_rect_op(r, ClipOp::Replace);
    }

    pub fn set_clip_rect_op(&mut self, r: Rect, op: ClipOp) {
        if self.st.t.is_axis_aligned() {
            let d = self.st.t.map_rect(r);
            self.new_clip(op, Some([d.x as f32, d.y as f32, d.right() as f32, d.bottom() as f32]), None);
        } else {
            let shape = Shape::Rect { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32 };
            self.new_clip(op, None, Some(ClipShape { shape, rule: FillRule::Winding, transform: self.st.t32 }));
        }
    }

    pub fn set_clip_path(&mut self, p: &Path) {
        self.set_clip_path_op(p, ClipOp::Replace);
    }

    pub fn set_clip_path_op(&mut self, p: &Path, op: ClipOp) {
        let shape = self.scene.push_path(p);
        self.new_clip(op, None, Some(ClipShape { shape, rule: p.fill_rule(), transform: self.st.t32 }));
    }

    pub fn set_clip_region(&mut self, r: &Region) {
        self.set_clip_region_op(r, ClipOp::Replace);
    }

    pub fn set_clip_region_op(&mut self, r: &Region, op: ClipOp) {
        match r.rects() {
            [] => self.new_clip(op, Some([0.0; 4]), None),
            [one] => self.set_clip_rect_op(*one, op),
            _ => self.set_clip_path_op(&r.to_path(), op),
        }
    }

    /// `setClipping(on)`: toggles the current clip without forgetting it.
    pub fn set_clipping(&mut self, on: bool) {
        self.st.clip_on = on && self.st.clip != 0;
    }

    pub fn has_clipping(&self) -> bool {
        self.st.clip_on
    }

    // ------------------------------------------------------------ recording

    fn current_clip(&self) -> Option<u32> {
        if !self.st.clip_on {
            return Some(0);
        }
        (!self.scene.clips[self.st.clip as usize].is_empty()).then_some(self.st.clip)
    }

    fn push(&mut self, op: Op, paint: u32, color: Color) {
        self.push_t(op, paint, color, self.st.t32);
    }

    fn push_t(&mut self, op: Op, mut paint: u32, mut color: Color, transform: [f32; 6]) {
        let Some(clip) = self.current_clip() else { return };
        let mut comp = self.st.comp;
        match comp {
            CompositionMode::Destination => return,
            // Qt's Clear: destination × (1 − coverage × opacity).
            CompositionMode::Clear => {
                comp = CompositionMode::DestinationOut;
                paint = SOLID;
                color = Color::BLACK;
            }
            _ => {}
        }
        if self.st.opacity <= 0.0 {
            return;
        }
        if paint == SOLID
            && color.a == 0
            && matches!(comp, CompositionMode::SourceOver | CompositionMode::DestinationOut)
        {
            return;
        }
        self.scene.cmds.push(Command {
            op,
            transform,
            paint,
            color,
            opacity: self.st.opacity as f32,
            clip,
            comp,
            aa: self.st.aa,
        });
    }

    fn paint_of(&mut self, b: Brush) -> (u32, Color) {
        match b {
            Brush::Solid(c) => (SOLID, c),
            Brush::None => (SOLID, Color::TRANSPARENT),
            other => {
                self.scene.paints.push(other);
                (self.scene.paints.len() as u32 - 1, Color::BLACK)
            }
        }
    }

    fn brush_paint(&mut self) -> (u32, Color) {
        if let Brush::Solid(c) = self.st.brush {
            return (SOLID, c);
        }
        if let Some(i) = self.st.brush_paint {
            return (i, Color::BLACK);
        }
        let r = self.paint_of(self.st.brush);
        self.st.brush_paint = Some(r.0);
        r
    }

    fn pen_paint(&mut self) -> (u32, Color) {
        if let Brush::Solid(c) = self.st.pen.brush {
            return (SOLID, c);
        }
        if let Some(i) = self.st.pen_paint {
            return (i, Color::BLACK);
        }
        let r = self.paint_of(self.st.pen.brush);
        self.st.pen_paint = Some(r.0);
        r
    }

    fn stroke_idx(&mut self) -> u32 {
        if let Some(i) = self.st.stroke {
            return i;
        }
        let p = &self.st.pen;
        let s = StrokeStyle {
            width: p.width.max(0.0) as f32,
            cap: p.cap,
            join: p.join,
            miter_limit: p.miter_limit as f32,
            cosmetic: p.is_cosmetic(),
        };
        let i = if self.scene.strokes.last() == Some(&s) {
            self.scene.strokes.len() as u32 - 1
        } else {
            self.scene.strokes.push(s);
            self.scene.strokes.len() as u32 - 1
        };
        self.st.stroke = Some(i);
        i
    }

    fn fill_shape(&mut self, shape: Shape, rule: FillRule) {
        if self.st.brush.is_none() {
            return;
        }
        let (paint, color) = self.brush_paint();
        self.push(Op::Fill { shape, rule }, paint, color);
    }

    fn stroke_shape(&mut self, shape: Shape) {
        if self.st.pen.is_none() {
            return;
        }
        let shape = if self.st.pen.is_dashed() { self.dash_shape(shape) } else { shape };
        let stroke = self.stroke_idx();
        let (paint, color) = self.pen_paint();
        self.push(Op::Stroke { shape, stroke }, paint, color);
    }

    fn dash_shape(&mut self, shape: Shape) -> Shape {
        let pen = self.st.pen;
        let scale = self.st.t.scale_factor().max(1e-6);
        let unit = if pen.is_cosmetic() { (1.0 / scale) as f32 } else { pen.width as f32 };
        let mut lines = std::mem::take(&mut self.scene.tmp_lines);
        let mut dashed = std::mem::take(&mut self.scene.tmp_dash);
        lines.clear();
        self.scene.flatten_shape(&shape, (0.2 / scale) as f32, &mut lines);
        let mut pat = [0f32; crate::paint::MAX_DASHES];
        let src = pen.dash.as_slice();
        for (d, s) in pat.iter_mut().zip(src) {
            *d = s * unit;
        }
        crate::stroke::dash(&lines, &pat[..src.len()], pen.dash_offset as f32 * unit, &mut dashed);
        let out = self.scene.push_polylines(&dashed);
        self.scene.tmp_lines = lines;
        self.scene.tmp_dash = dashed;
        out
    }

    fn with_tmp_path(&mut self, f: impl FnOnce(&mut Path)) -> Shape {
        let mut p = std::mem::take(&mut self.scene.tmp_path);
        p.clear();
        p.set_fill_rule(FillRule::OddEven);
        f(&mut p);
        let s = self.scene.push_path(&p);
        self.scene.tmp_path = p;
        s
    }

    // ---------------------------------------------------------------- shapes

    /// `drawPath`: fill with the brush (path's fill rule), then stroke with the pen.
    pub fn draw_path(&mut self, path: &Path) {
        if path.verbs.is_empty() || (self.st.brush.is_none() && self.st.pen.is_none()) {
            return;
        }
        let shape = self.scene.push_path(path);
        self.fill_shape(shape, path.fill_rule());
        self.stroke_shape(shape);
    }

    /// `fillPath(path, brush)`.
    pub fn fill_path(&mut self, path: &Path, brush: impl Into<Brush>) {
        let b = brush.into();
        if b.is_none() || path.verbs.is_empty() {
            return;
        }
        let shape = self.scene.push_path(path);
        let (paint, color) = self.paint_of(b);
        self.push(Op::Fill { shape, rule: path.fill_rule() }, paint, color);
    }

    /// `strokePath(path, pen)`.
    pub fn stroke_path(&mut self, path: &Path, pen: impl Into<Pen>) {
        self.save();
        self.set_pen(pen);
        if !path.verbs.is_empty() {
            let shape = self.scene.push_path(path);
            self.stroke_shape(shape);
        }
        self.restore();
    }

    /// `drawEllipse(QRectF)`.
    pub fn draw_ellipse(&mut self, r: Rect) {
        self.draw_ellipse_at(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
    }

    /// `drawEllipse(QPointF center, rx, ry)`.
    pub fn draw_ellipse_at(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        let shape = Shape::Ellipse { cx: cx as f32, cy: cy as f32, rx: rx.abs() as f32, ry: ry.abs() as f32 };
        self.fill_shape(shape, FillRule::Winding);
        self.stroke_shape(shape);
    }

    pub fn draw_ellipse_p(&mut self, c: impl Into<Vec2>, rx: f64, ry: f64) {
        let c = c.into();
        self.draw_ellipse_at(c.x, c.y, rx, ry);
    }

    /// `drawRect`.
    pub fn draw_rect(&mut self, r: Rect) {
        let shape = Shape::Rect { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32 };
        self.fill_shape(shape, FillRule::Winding);
        self.stroke_shape(shape);
    }

    /// `fillRect(rect, brush)` (no pen).
    pub fn fill_rect(&mut self, r: Rect, brush: impl Into<Brush>) {
        let b = brush.into();
        if b.is_none() {
            return;
        }
        let shape = Shape::Rect { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32 };
        let (paint, color) = self.paint_of(b);
        self.push(Op::Fill { shape, rule: FillRule::Winding }, paint, color);
    }

    /// `eraseRect` on a transparent device: clears to transparent.
    pub fn erase_rect(&mut self, r: Rect) {
        let comp = self.st.comp;
        self.st.comp = CompositionMode::Clear;
        self.fill_rect(r, Color::BLACK);
        self.st.comp = comp;
    }

    /// `drawRoundedRect(rect, xr, yr)` (absolute radii).
    pub fn draw_rounded_rect(&mut self, r: Rect, xr: f64, yr: f64) {
        let shape = self.with_tmp_path(|p| p.add_rounded_rect(r, xr, yr));
        self.fill_shape(shape, FillRule::OddEven);
        self.stroke_shape(shape);
    }

    /// `drawRoundedRect(rect, xr, yr, Qt::RelativeSize)`.
    pub fn draw_rounded_rect_relative(&mut self, r: Rect, xr: f64, yr: f64) {
        let shape = self.with_tmp_path(|p| p.add_rounded_rect_relative(r, xr, yr));
        self.fill_shape(shape, FillRule::OddEven);
        self.stroke_shape(shape);
    }

    /// `drawLine(x1, y1, x2, y2)`.
    pub fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        if self.st.pen.is_none() {
            return;
        }
        self.stroke_shape(Shape::Line { x1: x1 as f32, y1: y1 as f32, x2: x2 as f32, y2: y2 as f32 });
    }

    /// `drawLine(QPointF, QPointF)`.
    pub fn draw_line_p(&mut self, a: impl Into<Vec2>, b: impl Into<Vec2>) {
        let (a, b) = (a.into(), b.into());
        self.draw_line(a.x, a.y, b.x, b.y);
    }

    /// `drawLines(lines)`: one stroke covering every line.
    pub fn draw_lines<P: Into<Vec2> + Copy>(&mut self, lines: &[(P, P)]) {
        if self.st.pen.is_none() || lines.is_empty() {
            return;
        }
        let shape = self.with_tmp_path(|p| {
            for &(a, b) in lines {
                let (a, b) = (a.into(), b.into());
                p.move_to(a.x, a.y);
                p.line_to(b.x, b.y);
            }
        });
        self.stroke_shape(shape);
    }

    /// `drawPolyline(points)`.
    pub fn draw_polyline<P: Into<Vec2> + Copy>(&mut self, pts: &[P]) {
        if self.st.pen.is_none() || pts.len() < 2 {
            return;
        }
        let shape = self.with_tmp_path(|p| p.add_polygon(pts));
        self.stroke_shape(shape);
    }

    /// `drawPolygon(points)` with Qt's default odd-even rule.
    pub fn draw_polygon<P: Into<Vec2> + Copy>(&mut self, pts: &[P]) {
        self.draw_polygon_rule(pts, FillRule::OddEven);
    }

    pub fn draw_polygon_rule<P: Into<Vec2> + Copy>(&mut self, pts: &[P], rule: FillRule) {
        if pts.len() < 2 {
            return;
        }
        let shape = self.with_tmp_path(|p| {
            p.add_polygon(pts);
            p.close_subpath();
        });
        self.fill_shape(shape, rule);
        self.stroke_shape(shape);
    }

    /// `drawConvexPolygon`.
    pub fn draw_convex_polygon<P: Into<Vec2> + Copy>(&mut self, pts: &[P]) {
        self.draw_polygon_rule(pts, FillRule::Winding);
    }

    /// `drawPoint`: a round dot for round caps, otherwise a square; a
    /// one-pixel (or cosmetic) pen paints exactly one device pixel.
    pub fn draw_point(&mut self, x: f64, y: f64) {
        if self.st.pen.is_none() {
            return;
        }
        let pen = self.st.pen;
        let scale = self.st.t.scale_factor();
        let dev_w = if pen.is_cosmetic() { 1.0 } else { pen.width * scale };
        let (paint, color) = self.pen_paint();
        if dev_w <= 1.0 {
            let (dx, dy) = self.st.t.map(x, y);
            let shape = Shape::Rect { x: dx.floor() as f32, y: dy.floor() as f32, w: 1.0, h: 1.0 };
            let saved = self.st.aa;
            self.st.aa = false;
            self.push_t(Op::Fill { shape, rule: FillRule::Winding }, paint, color, Transform::IDENTITY.to_f32());
            self.st.aa = saved;
            return;
        }
        let hw = (pen.width / 2.0) as f32;
        let shape = if pen.cap == Cap::Round {
            Shape::Ellipse { cx: x as f32, cy: y as f32, rx: hw, ry: hw }
        } else {
            Shape::Rect { x: x as f32 - hw, y: y as f32 - hw, w: hw * 2.0, h: hw * 2.0 }
        };
        self.push(Op::Fill { shape, rule: FillRule::Winding }, paint, color);
    }

    pub fn draw_points<P: Into<Vec2> + Copy>(&mut self, pts: &[P]) {
        for &p in pts {
            let p = p.into();
            self.draw_point(p.x, p.y);
        }
    }

    /// `drawArc(rect, startAngle, spanAngle)` with angles in 1/16 degree.
    pub fn draw_arc(&mut self, r: Rect, start16: i32, span16: i32) {
        self.draw_arc_deg(r, start16 as f64 / 16.0, span16 as f64 / 16.0);
    }

    pub fn draw_arc_deg(&mut self, r: Rect, start: f64, span: f64) {
        if self.st.pen.is_none() {
            return;
        }
        let shape = self.with_tmp_path(|p| {
            p.arc_move_to(r, start);
            p.arc_to(r, start, span);
        });
        self.stroke_shape(shape);
    }

    /// `drawChord` (1/16 degree angles).
    pub fn draw_chord(&mut self, r: Rect, start16: i32, span16: i32) {
        let (s, a) = (start16 as f64 / 16.0, span16 as f64 / 16.0);
        let shape = self.with_tmp_path(|p| {
            p.arc_move_to(r, s);
            p.arc_to(r, s, a);
            p.close_subpath();
        });
        self.fill_shape(shape, FillRule::OddEven);
        self.stroke_shape(shape);
    }

    /// `drawPie` (1/16 degree angles).
    pub fn draw_pie(&mut self, r: Rect, start16: i32, span16: i32) {
        let (s, a) = (start16 as f64 / 16.0, span16 as f64 / 16.0);
        let shape = self.with_tmp_path(|p| {
            let (cx, cy) = r.center();
            p.move_to(cx, cy);
            p.arc_to(r, s, a);
            p.close_subpath();
        });
        self.fill_shape(shape, FillRule::OddEven);
        self.stroke_shape(shape);
    }

    // ---------------------------------------------------------------- images

    fn image_index(&mut self, img: &Image) -> u32 {
        let n = self.scene.images.len();
        for i in (n.saturating_sub(8)..n).rev() {
            let e = &self.scene.images[i];
            if e.id() == img.id() && e.version() == img.version() {
                return i as u32;
            }
        }
        self.scene.images.push(img.clone());
        n as u32
    }

    /// `drawImage(QRectF target, image, QRectF source)` (source in image pixels).
    pub fn draw_image(&mut self, target: Rect, img: &Image, source: Rect) {
        if img.is_null() || target.w == 0.0 || target.h == 0.0 || source.w <= 0.0 || source.h <= 0.0 {
            return;
        }
        let image = self.image_index(img);
        let op = Op::Image {
            image,
            src: [source.x as f32, source.y as f32, source.w as f32, source.h as f32],
            dst: [target.x as f32, target.y as f32, target.w as f32, target.h as f32],
            smooth: self.st.smooth,
        };
        self.push(op, SOLID, Color::WHITE);
    }

    /// `drawImage(QRectF target, image)`: the whole image scaled into `target`.
    pub fn draw_image_rect(&mut self, target: Rect, img: &Image) {
        self.draw_image(target, img, Rect::new(0.0, 0.0, img.width() as f64, img.height() as f64));
    }

    /// `drawImage(QPointF, image)`: at its logical size (honours the device pixel ratio).
    pub fn draw_image_at(&mut self, x: f64, y: f64, img: &Image) {
        let (w, h) = img.logical_size();
        self.draw_image_rect(Rect::new(x, y, w, h), img);
    }

    /// `drawImage(QPointF, image, QRectF source)`.
    pub fn draw_image_at_src(&mut self, x: f64, y: f64, img: &Image, source: Rect) {
        let d = img.device_pixel_ratio() as f64;
        self.draw_image(Rect::new(x, y, source.w / d, source.h / d), img, source);
    }

    /// `drawPixmap(target, pixmap, source)` – pixmaps are images here.
    pub fn draw_pixmap(&mut self, target: Rect, img: &Image, source: Rect) {
        self.draw_image(target, img, source);
    }

    pub fn draw_pixmap_at(&mut self, x: f64, y: f64, img: &Image) {
        self.draw_image_at(x, y, img);
    }

    // ------------------------------------------------------------------ text

    /// `drawText(x, y, text)`: baseline at `y`, drawn with the pen's brush.
    pub fn draw_text(&mut self, x: f64, y: f64, text: &str) {
        if self.st.pen.is_none() || text.is_empty() {
            return;
        }
        let fm = FontMetrics::new(&self.st.font);
        let start = self.scene.glyphs.len() as u32;
        fm.layout_line(text, x as f32, y as f32, Some(&mut self.scene.glyphs));
        self.push_glyphs(start);
    }

    pub fn draw_text_p(&mut self, p: impl Into<Vec2>, text: &str) {
        let p = p.into();
        self.draw_text(p.x, p.y, text);
    }

    fn push_glyphs(&mut self, start: u32) {
        let end = self.scene.glyphs.len() as u32;
        if end == start {
            return;
        }
        let px = self.st.font.pixel_size_f();
        let (paint, color) = self.pen_paint();
        self.push(Op::Glyphs { start, end, px }, paint, color);
    }

    /// `drawText(rect, flags, text)`; returns the bounding rect. Text that
    /// overflows `rect` is clipped unless `TEXT_DONT_CLIP` is set, as in Qt.
    pub fn draw_text_in(&mut self, rect: Rect, flags: TextFlags, text: &str) -> Rect {
        let fm = FontMetrics::new(&self.st.font);
        if self.st.pen.is_none() || text.is_empty() {
            return layout_block(&fm, rect, flags, text, None);
        }
        let start = self.scene.glyphs.len() as u32;
        let bounds = layout_block(&fm, rect, flags, text, Some(&mut self.scene.glyphs));
        let contained = bounds.x >= rect.x - 0.01
            && bounds.y >= rect.y - 0.01
            && bounds.right() <= rect.right() + 0.01
            && bounds.bottom() <= rect.bottom() + 0.01;
        if !flags.has(TextFlags::TEXT_DONT_CLIP) && !contained {
            self.save();
            self.set_clip_rect_op(rect, ClipOp::Intersect);
            self.push_glyphs(start);
            self.restore();
        } else {
            self.push_glyphs(start);
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{LinearGradient, PenStyle};

    #[test]
    fn records_flattened_state() {
        let mut scene = Scene::new();
        {
            let mut c = scene.canvas();
            c.set_brush(Color::rgb(255, 0, 0));
            c.set_pen(Pen::NONE);
            c.save();
            c.translate(10.0, 20.0);
            c.draw_ellipse_at(0.0, 0.0, 5.0, 5.0);
            c.restore();
            c.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0));
            c.set_pen(Pen::new(Color::BLACK, 2.0));
            c.draw_line(0.0, 0.0, 10.0, 10.0);
        }
        let s = scene.stats();
        assert_eq!((s.fills, s.strokes), (2, 1));
        assert_eq!(scene.cmds[0].transform[4], 10.0);
        assert_eq!(scene.cmds[1].transform[4], 0.0);
        scene.clear();
        assert!(scene.is_empty());
    }

    #[test]
    fn paints_are_shared_until_changed() {
        let mut scene = Scene::new();
        {
            let mut c = scene.canvas();
            c.set_pen(Pen::NONE);
            c.set_brush(LinearGradient::new(0.0, 0.0, 1.0, 0.0).with_stop(0.0, Color::WHITE));
            for i in 0..10 {
                c.draw_rect(Rect::new(i as f64, 0.0, 1.0, 1.0));
            }
        }
        assert_eq!(scene.paints.len(), 1);
    }

    #[test]
    fn clips_and_clear_mode() {
        let mut scene = Scene::new();
        {
            let mut c = scene.canvas();
            c.set_pen(Pen::NONE);
            c.set_brush(Color::WHITE);
            c.set_clip_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
            c.set_clip_rect_op(Rect::new(5.0, 5.0, 10.0, 10.0), ClipOp::Intersect);
            c.set_composition_mode(CompositionMode::Clear);
            c.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0));
            c.set_clip_rect(Rect::new(50.0, 50.0, 0.0, 0.0));
            c.draw_rect(Rect::new(0.0, 0.0, 20.0, 20.0)); // empty clip: dropped
        }
        assert_eq!(scene.len(), 1);
        let cmd = scene.cmds[0];
        assert_eq!(cmd.comp, CompositionMode::DestinationOut);
        assert_eq!(scene.clips[cmd.clip as usize].scissor, Some([5.0, 5.0, 10.0, 10.0]));
    }

    #[test]
    fn dashed_pen_records_dashes() {
        let mut scene = Scene::new();
        {
            let mut c = scene.canvas();
            c.set_pen(Pen::new(Color::BLACK, 1.0).style(PenStyle::Dash));
            c.draw_line(0.0, 0.0, 12.0, 0.0);
        }
        let Op::Stroke { shape: Shape::Path { verbs, .. }, .. } = scene.cmds[0].op else { panic!() };
        // 4 on, 2 off: dashes at 0-4 and 6-10 (+ 12..12 dropped)
        assert_eq!(verbs.1 - verbs.0, 4);
    }
}
