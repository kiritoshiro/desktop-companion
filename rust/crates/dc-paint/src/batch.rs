//! Port of `creature/render_batch.py`: one spider's leg pass, collected and
//! issued as a few paths.
//!
//! Strokes group by `(layer, colour, width)`, fills by `(layer, colour)`,
//! points by `(layer, colour, width)`, each in first-seen order; the flush
//! walks the layers in ascending order and issues strokes, then fills, then
//! points for each, exactly as the Python did. With `direct` set every call
//! draws immediately, in collection order (`BATCH_LEGS = False`).

use dc_core::Color;
use dc_core::math::py_round;
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen, PenStyle};
use std::collections::HashMap;

pub const LAYER_SHADOW: i32 = -1;
pub const LAYER_FUZZ: i32 = 0;
pub const LAYER_HAIR: i32 = 1;
pub const LAYER_SEGMENT: i32 = 2;
pub const LAYER_JOINT: i32 = 3;
pub const LAYER_CORE: i32 = 4;
pub const LAYER_FOOT: i32 = 5;

/// `LEG_WIDTH_STEP`: 0 leaves every stroke width exactly as computed.
pub const LEG_WIDTH_STEP: f64 = 0.0;
/// `BATCH_LEGS`.
pub const BATCH_LEGS: bool = true;

fn rgba_key(c: Color) -> u32 {
    u32::from_be_bytes([c.a, c.r, c.g, c.b])
}

/// `LegBatch`.
pub struct LegBatch {
    width_step: f64,
    direct: bool,
    strokes: Vec<(i32, Color, f64, Path)>,
    stroke_index: HashMap<(i32, u32, u64), usize>,
    fills: Vec<(i32, Color, Path)>,
    fill_index: HashMap<(i32, u32), usize>,
    points: Vec<(i32, Color, f64, Vec<(f64, f64)>)>,
    point_index: HashMap<(i32, u32, u64), usize>,
}

impl LegBatch {
    pub fn new(width_step: f64, direct: bool) -> Self {
        LegBatch {
            width_step,
            direct,
            strokes: Vec::new(),
            stroke_index: HashMap::new(),
            fills: Vec::new(),
            fill_index: HashMap::new(),
            points: Vec::new(),
            point_index: HashMap::new(),
        }
    }

    fn quantize(&self, width: f64) -> f64 {
        let step = self.width_step;
        if step <= 0.0 {
            return width;
        }
        step.max(py_round(width / step) * step)
    }

    fn stroke_path(&mut self, layer: i32, color: Color, width: f64) -> &mut Path {
        let width = self.quantize(width);
        let key = (layer, rgba_key(color), width.to_bits());
        let idx = match self.stroke_index.get(&key) {
            Some(&i) => i,
            None => {
                self.strokes.push((layer, color, width, Path::new()));
                self.stroke_index.insert(key, self.strokes.len() - 1);
                self.strokes.len() - 1
            }
        };
        &mut self.strokes[idx].3
    }

    fn fill_path(&mut self, layer: i32, color: Color) -> &mut Path {
        let key = (layer, rgba_key(color));
        let idx = match self.fill_index.get(&key) {
            Some(&i) => i,
            None => {
                let mut p = Path::new();
                // QPainterPath's default fill rule.
                p.set_fill_rule(dc_render::FillRule::OddEven);
                self.fills.push((layer, color, p));
                self.fill_index.insert(key, self.fills.len() - 1);
                self.fills.len() - 1
            }
        };
        &mut self.fills[idx].2
    }

    #[allow(clippy::too_many_arguments)]
    pub fn line(&mut self, c: &mut Canvas, layer: i32, color: Color, width: f64, x1: f64, y1: f64, x2: f64, y2: f64) {
        if self.direct {
            c.set_pen(Pen::full(color, width, PenStyle::Solid, Cap::Round, Join::Round));
            c.draw_line(x1, y1, x2, y2);
            return;
        }
        let path = self.stroke_path(layer, color, width);
        path.move_to(x1, y1);
        path.line_to(x2, y2);
    }

    /// `add_path`: in direct mode the brush is left as it was, exactly as
    /// the shipped renderer did (see the Python docstring).
    pub fn add_path(&mut self, c: &mut Canvas, layer: i32, color: Color, width: f64, sub: &Path) {
        if self.direct {
            c.set_pen(Pen::full(color, width, PenStyle::Solid, Cap::Round, Join::Round));
            c.draw_path(sub);
            return;
        }
        self.stroke_path(layer, color, width).add_path(sub);
    }

    pub fn point(&mut self, c: &mut Canvas, layer: i32, color: Color, width: f64, x: f64, y: f64) {
        if self.direct {
            c.set_pen(Pen::full(color, width, PenStyle::Solid, Cap::Round, Join::Bevel));
            c.draw_point(x, y);
            return;
        }
        let width = self.quantize(width);
        let key = (layer, rgba_key(color), width.to_bits());
        let idx = match self.point_index.get(&key) {
            Some(&i) => i,
            None => {
                self.points.push((layer, color, width, Vec::new()));
                self.point_index.insert(key, self.points.len() - 1);
                self.points.len() - 1
            }
        };
        self.points[idx].3.push((x, y));
    }

    pub fn dot(&mut self, c: &mut Canvas, layer: i32, color: Color, x: f64, y: f64, radius: f64) {
        if self.direct {
            c.set_pen(Pen::NONE);
            c.set_brush(color);
            c.draw_ellipse_at(x, y, radius, radius);
            return;
        }
        self.fill_path(layer, color).add_ellipse_c(x, y, radius, radius);
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.fills.is_empty() && self.points.is_empty()
    }

    /// Issue the collected work; returns the number of draws made.
    pub fn flush(&mut self, c: &mut Canvas) -> usize {
        let mut draws = 0;
        if self.direct {
            return draws;
        }
        let mut layers: Vec<i32> = self
            .strokes
            .iter()
            .map(|e| e.0)
            .chain(self.fills.iter().map(|e| e.0))
            .chain(self.points.iter().map(|e| e.0))
            .collect();
        layers.sort_unstable();
        layers.dedup();
        for layer in layers {
            for (l, color, width, path) in &self.strokes {
                if *l != layer {
                    continue;
                }
                c.set_pen(Pen::full(*color, *width, PenStyle::Solid, Cap::Round, Join::Round));
                c.set_brush(Brush::None);
                c.draw_path(path);
                draws += 1;
            }
            for (l, color, path) in &self.fills {
                if *l != layer {
                    continue;
                }
                c.set_pen(Pen::NONE);
                c.set_brush(*color);
                c.draw_path(path);
                draws += 1;
            }
            for (l, color, width, pts) in &self.points {
                if *l != layer {
                    continue;
                }
                c.set_pen(Pen::full(*color, *width, PenStyle::Solid, Cap::Round, Join::Bevel));
                c.draw_points(pts);
                draws += 1;
            }
        }
        self.strokes.clear();
        self.stroke_index.clear();
        self.fills.clear();
        self.fill_index.clear();
        self.points.clear();
        self.point_index.clear();
        draws
    }
}
