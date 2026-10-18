//! Port of the drawing half of `world/webs.py`: `Strand.polyline_upto`,
//! `Web.temp_alpha`, `Web._wobble_offset`, `Web.draw` and `WebWorld.render`.

use super::Clip;
use dc_core::Color;
use dc_core::math::clamp;
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen};
use serde::{Deserialize, Serialize};

type Point = (f64, f64);

/// `Strand`: one polyline of the build.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct StrandDraw {
    pub points: Vec<Point>,
    pub draw: bool,
    pub kind: String,
    pub temporary: bool,
}

impl StrandDraw {
    fn cum(&self) -> (Vec<f64>, f64) {
        let mut cum = Vec::with_capacity(self.points.len());
        cum.push(0.0);
        let mut total = 0.0;
        for w in self.points.windows(2) {
            total += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            cum.push(total);
        }
        (cum, total)
    }

    /// `polyline_upto(t)`: every vertex up to fraction `t` plus the tip.
    pub fn polyline_upto(&self, t: f64) -> Vec<Point> {
        if self.points.is_empty() {
            return vec![(0.0, 0.0)];
        }
        let (cum, total) = self.cum();
        if t >= 1.0 || total <= 1e-6 {
            return self.points.clone();
        }
        if t <= 0.0 {
            return vec![self.points[0]];
        }
        let target = t * total;
        let mut out = vec![self.points[0]];
        for i in 1..self.points.len() {
            if cum[i] <= target {
                out.push(self.points[i]);
            } else {
                let seg = cum[i] - cum[i - 1];
                let local = if seg <= 1e-6 { 0.0 } else { (target - cum[i - 1]) / seg };
                let (ax, ay) = self.points[i - 1];
                let (bx, by) = self.points[i];
                out.push((ax + (bx - ax) * local, ay + (by - ay) * local));
                break;
            }
        }
        out
    }
}

/// `Web`: what `Web.draw` reads.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WebDraw {
    pub strands: Vec<StrandDraw>,
    pub hub: Point,
    /// Unit vector (`_unit(*wobble_dir)` already applied).
    pub wobble_dir: Point,
    pub built: usize,
    pub active_t: f64,
    /// `"building"` or `"complete"`.
    pub state: String,
    /// `cut`: snapped `(strand_index, segment_index)` pairs.
    pub cut: Vec<(usize, usize)>,
    pub bounce_amp: f64,
    pub bounce_phase: f64,
    pub bounce_origin: Point,
}

impl Default for WebDraw {
    fn default() -> Self {
        WebDraw {
            strands: Vec::new(),
            hub: (0.0, 0.0),
            wobble_dir: (1.0, 0.0),
            built: 0,
            active_t: 0.0,
            state: "building".into(),
            cut: Vec::new(),
            bounce_amp: 0.0,
            bounce_phase: 0.0,
            bounce_origin: (0.0, 0.0),
        }
    }
}

impl WebDraw {
    /// `_bbox`: the true geometry plus 8 px.
    pub fn bbox(&self) -> Clip {
        let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &self.strands {
            for &(x, y) in &s.points {
                b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
        let pad = 8.0;
        (b.0 - pad, b.1 - pad, b.2 + pad, b.3 + pad)
    }

    /// `temp_alpha()`.
    pub fn temp_alpha(&self) -> f64 {
        let capture_start = self.strands.iter().position(|s| s.kind == "capture");
        let Some(capture_start) = capture_start else { return 1.0 };
        if self.built < capture_start {
            return 1.0;
        }
        let capture_count = self.strands.iter().filter(|s| s.kind == "capture").count();
        let done = (self.built - capture_start) as f64 + self.active_t;
        clamp(1.0 - done / (capture_count as f64).max(1.0), 0.0, 1.0)
    }

    /// `_wobble_offset(p)`.
    pub fn wobble_offset(&self, p: Point) -> Point {
        if self.bounce_amp <= 0.01 {
            return p;
        }
        let d = (p.0 - self.bounce_origin.0).hypot(p.1 - self.bounce_origin.1);
        let fall = (-d / 90.0).exp();
        let amt = self.bounce_amp * fall * (self.bounce_phase - d * 0.035).cos();
        (p.0 + self.wobble_dir.0 * amt, p.1 + self.wobble_dir.1 * amt)
    }
}

fn silk_pen(color: Color, width: f64) -> Pen {
    let mut pen = Pen::new(color, width);
    pen.set_cap_style(Cap::Round);
    pen.set_join_style(Join::Round);
    pen
}

/// `pen_for(kind, temporary)` inside `Web.draw`.
fn pen_for(kind: &str, temporary: bool, temp_a: f64) -> Option<Pen> {
    let pen = if temporary {
        let a = (120.0 * temp_a) as i64;
        if a <= 3 {
            return None;
        }
        let mut pen = silk_pen(Color::rgba(210, 224, 238, a as u8), 0.9);
        pen.set_dash_pattern(&[2.0, 3.0]);
        pen
    } else {
        match kind {
            "capture" => silk_pen(Color::rgba(238, 245, 255, 205), 1.15),
            "bridge" | "scaffold" => silk_pen(Color::rgba(225, 234, 246, 180), 1.35),
            "gumfoot" => silk_pen(Color::rgba(232, 240, 250, 195), 1.1),
            "sheet" | "funnel" => silk_pen(Color::rgba(222, 232, 244, 120), 0.85),
            _ => silk_pen(Color::rgba(224, 233, 245, 165), 1.0),
        }
    };
    let mut pen = pen;
    pen.set_cap_style(Cap::Round);
    pen.set_join_style(Join::Round);
    Some(pen)
}

fn run_path(run: &[Point]) -> Path {
    let mut path = Path::new();
    path.move_to(run[0].0, run[0].1);
    for p in &run[1..] {
        path.line_to(p.0, p.1);
    }
    path
}

/// `Web.draw(painter, clip)`.
pub fn draw_web(p: &mut Canvas, web: &WebDraw, clip: Option<Clip>) {
    if let Some(clip) = clip {
        let (x0, y0, x1, y1) = web.bbox();
        if x1 < clip.0 || x0 > clip.2 || y1 < clip.1 || y0 > clip.3 {
            return;
        }
    }
    let wobbling = web.bounce_amp > 0.01;
    let temp_a = web.temp_alpha();
    let complete = web.state == "complete";
    for (i, s) in web.strands.iter().enumerate() {
        if !s.draw {
            continue;
        }
        let portion = if i < web.built {
            1.0
        } else if i == web.built && !complete {
            web.active_t
        } else if complete {
            1.0
        } else {
            continue;
        };
        if portion <= 0.0 {
            continue;
        }
        let Some(pen) = pen_for(&s.kind, s.temporary, temp_a) else { continue };
        let mut pts = s.polyline_upto(portion);
        if pts.len() < 2 {
            continue;
        }
        if wobbling {
            pts = pts.into_iter().map(|q| web.wobble_offset(q)).collect();
        }
        p.set_pen(pen);
        p.set_brush(Brush::None);
        let is_cut = |segi: usize| web.cut.contains(&(i, segi));
        let has_cuts = !web.cut.is_empty() && complete && (0..pts.len() - 1).any(is_cut);
        if has_cuts {
            let mut run: Vec<Point> = vec![pts[0]];
            for segi in 0..pts.len() - 1 {
                if is_cut(segi) {
                    if run.len() >= 2 {
                        p.draw_path(&run_path(&run));
                    }
                    run = vec![pts[segi + 1]];
                } else {
                    run.push(pts[segi + 1]);
                }
            }
            if run.len() >= 2 {
                p.draw_path(&run_path(&run));
            }
        } else {
            p.draw_path(&run_path(&pts));
        }
        if s.kind == "capture" && pts.len() > 4 && !has_cuts {
            p.set_pen(Pen::NONE);
            p.set_brush(Color::rgba(245, 250, 255, 150));
            let step = (pts.len() / 10).max(2);
            for &(bx, by) in pts.iter().step_by(step) {
                p.draw_ellipse_at(bx, by, 0.9, 0.9);
            }
        }
    }
    if web.built > 1 {
        let hub = if wobbling { web.wobble_offset(web.hub) } else { web.hub };
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(240, 247, 255, 150));
        p.draw_ellipse_at(hub.0, hub.1, 1.6, 1.6);
    }
}

/// `WebWorld.render(painter, clip)`.
pub fn render_webs(p: &mut Canvas, webs: &[WebDraw], clip: Option<Clip>) {
    for web in webs {
        draw_web(p, web, clip);
    }
}
