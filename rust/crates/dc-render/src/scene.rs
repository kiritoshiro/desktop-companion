//! The retained display list.
//!
//! A [`Scene`] holds *flattened* commands: every command carries its final
//! transform, paint, opacity, composition mode and clip, so backends never
//! replay `save`/`restore`. Geometry lives in shared arenas (`verbs`,
//! `points`, `glyphs`), and [`Scene::clear`] keeps every buffer's capacity, so
//! recording a frame allocates nothing once the buffers have grown.

use crate::canvas::{Canvas, State};
use crate::image::Image;
use crate::paint::{Brush, Cap, CompositionMode, Join};
use crate::path::{FillRule, Path, PathRef, Polylines, Verb};
use crate::text::GlyphPos;
use crate::transform::Transform;
use dc_core::{Color, Rect};

/// `paint` value meaning "use the command's inline `color`".
pub(crate) const SOLID: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shape {
    /// Ranges into `Scene::verbs` / `Scene::points`.
    Path {
        verbs: (u32, u32),
        points: (u32, u32),
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StrokeStyle {
    pub width: f32,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f32,
    pub cosmetic: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
    Fill {
        shape: Shape,
        rule: FillRule,
    },
    Stroke {
        shape: Shape,
        stroke: u32,
    },
    /// `src` in image pixels, `dst` in local units, both `[x, y, w, h]`.
    Image {
        image: u32,
        src: [f32; 4],
        dst: [f32; 4],
        smooth: bool,
    },
    /// Range into `Scene::glyphs`; `px` is the em size in local units.
    Glyphs {
        start: u32,
        end: u32,
        px: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Command {
    pub op: Op,
    pub transform: [f32; 6],
    pub paint: u32,
    pub color: Color,
    pub opacity: f32,
    pub clip: u32,
    pub comp: CompositionMode,
    pub aa: bool,
}

/// A clip state: an optional device-space scissor `[l, t, r, b]` intersected
/// with every shape in `shapes` (a range of `Scene::clip_shapes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Clip {
    pub scissor: Option<[f32; 4]>,
    pub shapes: (u32, u32),
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.scissor.is_some_and(|s| s[2] <= s[0] || s[3] <= s[1])
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipShape {
    pub shape: Shape,
    pub rule: FillRule,
    pub transform: [f32; 6],
}

/// Counts for tests and profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStats {
    pub commands: usize,
    pub fills: usize,
    pub strokes: usize,
    pub images: usize,
    pub text_runs: usize,
    pub glyphs: usize,
    pub clips: usize,
    pub path_points: usize,
}

#[derive(Default)]
pub struct Scene {
    pub(crate) cmds: Vec<Command>,
    pub(crate) verbs: Vec<Verb>,
    pub(crate) points: Vec<[f32; 2]>,
    pub(crate) paints: Vec<Brush>,
    pub(crate) strokes: Vec<StrokeStyle>,
    pub(crate) images: Vec<Image>,
    pub(crate) glyphs: Vec<GlyphPos>,
    pub(crate) clips: Vec<Clip>,
    pub(crate) clip_shapes: Vec<ClipShape>,
    pub(crate) stack_pool: Vec<State>,
    pub(crate) tmp_path: Path,
    pub(crate) tmp_lines: Polylines,
    pub(crate) tmp_dash: Polylines,
}

impl Scene {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.clips.push(Clip { scissor: None, shapes: (0, 0) });
        s
    }

    /// Drops every command but keeps buffer capacity.
    pub fn clear(&mut self) {
        self.cmds.clear();
        self.verbs.clear();
        self.points.clear();
        self.paints.clear();
        self.strokes.clear();
        self.images.clear();
        self.glyphs.clear();
        self.clips.clear();
        self.clips.push(Clip { scissor: None, shapes: (0, 0) });
        self.clip_shapes.clear();
    }

    /// Starts (or continues) recording into this scene with a fresh painter
    /// state, like `QPainter(device)`.
    pub fn canvas(&mut self) -> Canvas<'_> {
        Canvas::new(self)
    }

    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    pub fn stats(&self) -> SceneStats {
        let mut s = SceneStats {
            commands: self.cmds.len(),
            glyphs: self.glyphs.len(),
            clips: self.clips.len() - 1,
            path_points: self.points.len(),
            ..Default::default()
        };
        for c in &self.cmds {
            match c.op {
                Op::Fill { .. } => s.fills += 1,
                Op::Stroke { .. } => s.strokes += 1,
                Op::Image { .. } => s.images += 1,
                Op::Glyphs { .. } => s.text_runs += 1,
            }
        }
        s
    }

    /// Appends every command of `src` drawn through `t` (applied after each
    /// command's own transform) and clipped to `clip` (this scene's device
    /// pixels), like replaying a `QPicture` under a painter transform and
    /// clip rect. Paints, strokes, glyphs and images are shared; cosmetic
    /// pens stay one device pixel wide. The overlay draws one logical-pixel
    /// scene once per monitor this way, each at its own DPI scale.
    pub fn append_transformed(&mut self, src: &Scene, t: &Transform, clip: Option<Rect>) {
        let (v0, p0) = (self.verbs.len() as u32, self.points.len() as u32);
        let (paint0, stroke0) = (self.paints.len() as u32, self.strokes.len() as u32);
        let (image0, glyph0) = (self.images.len() as u32, self.glyphs.len() as u32);
        self.verbs.extend_from_slice(&src.verbs);
        self.points.extend_from_slice(&src.points);
        self.paints.extend_from_slice(&src.paints);
        self.strokes.extend_from_slice(&src.strokes);
        self.images.extend(src.images.iter().cloned());
        self.glyphs.extend_from_slice(&src.glyphs);
        let shape = |s: Shape| match s {
            Shape::Path { verbs, points } => {
                Shape::Path { verbs: (verbs.0 + v0, verbs.1 + v0), points: (points.0 + p0, points.1 + p0) }
            }
            other => other,
        };
        let then = |m: [f32; 6]| Transform::from_f32(m).then(t).to_f32();
        let outer = clip.map(|r| [r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32]);
        // Where each of `src`'s clips went; an unclipped one stays clip 0
        // (backends read a clip without scissor or shapes as empty).
        let mut clips = Vec::with_capacity(src.clips.len());
        for c in &src.clips {
            let base = self.clip_shapes.len() as u32;
            for i in c.shapes.0..c.shapes.1 {
                let cs = src.clip_shapes[i as usize];
                self.clip_shapes.push(ClipShape {
                    shape: shape(cs.shape),
                    rule: cs.rule,
                    transform: then(cs.transform),
                });
            }
            let mut scissor = None;
            if let Some(s) = c.scissor {
                let r = Rect::from_ltrb(s[0] as f64, s[1] as f64, s[2] as f64, s[3] as f64);
                if t.is_axis_aligned() {
                    let d = t.map_rect(r);
                    scissor = Some([d.x as f32, d.y as f32, d.right() as f32, d.bottom() as f32]);
                } else {
                    let shape = Shape::Rect { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32 };
                    self.clip_shapes.push(ClipShape { shape, rule: FillRule::Winding, transform: t.to_f32() });
                }
            }
            let scissor = match (scissor, outer) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (a, b) => a.or(b),
            };
            let shapes = (base, self.clip_shapes.len() as u32);
            if scissor.is_none() && shapes.0 == shapes.1 {
                clips.push(0);
            } else {
                clips.push(self.clips.len() as u32);
                self.clips.push(Clip { scissor, shapes });
            }
        }
        for cmd in &src.cmds {
            let op = match cmd.op {
                Op::Fill { shape: s, rule } => Op::Fill { shape: shape(s), rule },
                Op::Stroke { shape: s, stroke } => Op::Stroke { shape: shape(s), stroke: stroke + stroke0 },
                Op::Image { image, src: from, dst, smooth } => {
                    Op::Image { image: image + image0, src: from, dst, smooth }
                }
                Op::Glyphs { start, end, px } => Op::Glyphs { start: start + glyph0, end: end + glyph0, px },
            };
            let clip = clips[cmd.clip as usize];
            if self.clips[clip as usize].is_empty() {
                continue;
            }
            let paint = if cmd.paint == SOLID { SOLID } else { cmd.paint + paint0 };
            self.cmds.push(Command { op, transform: then(cmd.transform), paint, clip, ..*cmd });
        }
    }

    pub(crate) fn push_path(&mut self, p: &Path) -> Shape {
        let v0 = self.verbs.len() as u32;
        let p0 = self.points.len() as u32;
        self.verbs.extend_from_slice(&p.verbs);
        self.points.extend_from_slice(&p.points);
        Shape::Path { verbs: (v0, self.verbs.len() as u32), points: (p0, self.points.len() as u32) }
    }

    pub(crate) fn push_polylines(&mut self, pl: &Polylines) -> Shape {
        let v0 = self.verbs.len() as u32;
        let p0 = self.points.len() as u32;
        for c in &pl.contours {
            let pts = pl.contour(c);
            if pts.is_empty() {
                continue;
            }
            self.verbs.push(Verb::MoveTo);
            self.points.push(pts[0]);
            for p in &pts[1..] {
                self.verbs.push(Verb::LineTo);
                self.points.push(*p);
            }
            if c.closed {
                self.verbs.push(Verb::Close);
            }
        }
        Shape::Path { verbs: (v0, self.verbs.len() as u32), points: (p0, self.points.len() as u32) }
    }

    pub(crate) fn path_ref(&self, verbs: (u32, u32), points: (u32, u32)) -> PathRef<'_> {
        PathRef::new(
            &self.verbs[verbs.0 as usize..verbs.1 as usize],
            &self.points[points.0 as usize..points.1 as usize],
        )
    }

    /// Flattens any shape (in its local coordinates) into `out` (appending).
    pub(crate) fn flatten_shape(&self, shape: &Shape, tol: f32, out: &mut Polylines) {
        flatten_shape_into(self, shape, tol, out);
    }
}

pub(crate) fn flatten_shape_into(scene: &Scene, shape: &Shape, tol: f32, out: &mut Polylines) {
    match *shape {
        Shape::Path { verbs, points } => out.flatten(scene.path_ref(verbs, points), tol),
        Shape::Ellipse { cx, cy, rx, ry } => {
            let r = rx.abs().max(ry.abs()).max(1e-6);
            let step =
                if r <= tol { std::f32::consts::FRAC_PI_2 } else { (2.0 * (1.0 - tol / r).acos()).clamp(0.01, 1.0) };
            let n = ((std::f32::consts::TAU / step).ceil() as usize).clamp(8, 1024);
            // Clockwise from 3 o'clock like QPainterPath::addEllipse.
            out.begin([cx + rx, cy]);
            for i in 1..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                out.push([cx + rx * a.cos(), cy + ry * a.sin()]);
            }
            out.close();
        }
        Shape::Rect { x, y, w, h } => {
            out.begin([x, y]);
            out.push([x + w, y]);
            out.push([x + w, y + h]);
            out.push([x, y + h]);
            out.close();
        }
        Shape::Line { x1, y1, x2, y2 } => {
            out.begin([x1, y1]);
            if [x1, y1] != [x2, y2] {
                out.push([x2, y2]);
            }
        }
    }
}
