//! Port of `world/webs.py`: spider silk the creatures weave, walk on,
//! bounce-test, tear and mend.
//!
//! A [`Web`] is a shared world object. One spider builds it strand by strand,
//! any spider may later walk onto a finished web, and any weaver may adopt an
//! abandoned one. [`WebWorld`] holds every web.
//!
//! Python held creature objects in `Web.builder` / `Web.repairer`; here they
//! are [`CreatureId`]s, and webs are addressed by [`WebId`].
//!
//! The torn-segment set `cut` is a `BTreeSet` (sorted) where Python used a
//! hash set. `repair_near` observes the iteration order when two torn
//! segments are exactly the same distance from the mender; Python then mends
//! in hash-table order and Rust in index order. The count mended, and so
//! `is_damaged`, is identical. The float sums in `damaged_centroid` and
//! `intact_fraction` run in that order too (last-bit differences).
//!
//! Drawing (`Web.draw`) is out of scope; see [`Web::strand_portion`],
//! [`Web::temp_alpha`] and [`Web::wobble_offset`] for what a painter needs.

use std::collections::BTreeSet;

use dc_core::PyRandom;
use dc_core::math::{clamp, lerp};
use serde_json::{Map, Value, json};

use crate::creature_view::{CreatureId, WebId};
use crate::pymath::{self, distance, py_int, py_pow, py_sum, round_n};
use crate::{Bbox, Point, Xywh};

/// Total webs allowed on screen at once.
pub const MAX_WEBS: usize = 7;
/// How close the cursor must pass to a strand to snap it.
pub const TEAR_RADIUS: f64 = 9.0;
/// Minimum cursor speed that counts as a deliberate swipe.
pub const TEAR_MIN_CURSOR_SPEED: f64 = 90.0;
/// Torn segments mended per second while a webber works.
pub const REPAIR_RATE: f64 = 9.0;
/// How close the spider must trail the silk tip before the build advances.
pub const DEFAULT_LEAD_MAX: f64 = 26.0;

const PI: f64 = std::f64::consts::PI;

// ======================================================================
// Strand
// ======================================================================

/// `Strand.kind`. Unknown kinds from a state file are kept verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrandKind {
    Frame,
    Bridge,
    Radius,
    Hub,
    Aux,
    Capture,
    Sheet,
    Funnel,
    Scaffold,
    Tangle,
    Gumfoot,
    Move,
    Other(String),
}

impl StrandKind {
    pub fn parse(s: &str) -> StrandKind {
        match s {
            "frame" => StrandKind::Frame,
            "bridge" => StrandKind::Bridge,
            "radius" => StrandKind::Radius,
            "hub" => StrandKind::Hub,
            "aux" => StrandKind::Aux,
            "capture" => StrandKind::Capture,
            "sheet" => StrandKind::Sheet,
            "funnel" => StrandKind::Funnel,
            "scaffold" => StrandKind::Scaffold,
            "tangle" => StrandKind::Tangle,
            "gumfoot" => StrandKind::Gumfoot,
            "move" => StrandKind::Move,
            other => StrandKind::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            StrandKind::Frame => "frame",
            StrandKind::Bridge => "bridge",
            StrandKind::Radius => "radius",
            StrandKind::Hub => "hub",
            StrandKind::Aux => "aux",
            StrandKind::Capture => "capture",
            StrandKind::Sheet => "sheet",
            StrandKind::Funnel => "funnel",
            StrandKind::Scaffold => "scaffold",
            StrandKind::Tangle => "tangle",
            StrandKind::Gumfoot => "gumfoot",
            StrandKind::Move => "move",
            StrandKind::Other(s) => s,
        }
    }
}

/// One traversal segment of the build: a polyline the spider walks along,
/// laying silk when `draw` is true.
#[derive(Clone, Debug, PartialEq)]
pub struct Strand {
    pub points: Vec<Point>,
    pub draw: bool,
    pub kind: StrandKind,
    pub temporary: bool,
    cum: Vec<f64>,
    pub total: f64,
}

impl Strand {
    pub fn new(points: Vec<Point>, draw: bool, kind: StrandKind, temporary: bool) -> Strand {
        let points = if points.is_empty() { vec![(0.0, 0.0)] } else { points };
        let mut cum = Vec::with_capacity(points.len());
        cum.push(0.0);
        let mut total = 0.0;
        for i in 1..points.len() {
            total += distance(points[i - 1].0, points[i - 1].1, points[i].0, points[i].1);
            cum.push(total);
        }
        Strand { points, draw, kind, temporary, cum, total }
    }

    fn silk(points: Vec<Point>, kind: StrandKind) -> Strand {
        Strand::new(points, true, kind, false)
    }

    pub fn start(&self) -> Point {
        self.points[0]
    }

    pub fn end(&self) -> Point {
        *self.points.last().unwrap()
    }

    /// Position a fraction `t` (0..1) along the strand by arc length.
    pub fn point_at(&self, t: f64) -> Point {
        if self.points.len() == 1 || self.total <= 1e-6 {
            return self.end();
        }
        let target = clamp(t, 0.0, 1.0) * self.total;
        for i in 1..self.points.len() {
            if self.cum[i] >= target {
                let seg = self.cum[i] - self.cum[i - 1];
                let local = if seg <= 1e-6 { 0.0 } else { (target - self.cum[i - 1]) / seg };
                let (ax, ay) = self.points[i - 1];
                let (bx, by) = self.points[i];
                return (lerp(ax, bx, local), lerp(ay, by, local));
            }
        }
        self.end()
    }

    /// All vertices up to fraction `t` plus the interpolated tip.
    pub fn polyline_upto(&self, t: f64) -> Vec<Point> {
        let mut out = Vec::new();
        self.polyline_upto_into(t, &mut out);
        out
    }

    /// [`Strand::polyline_upto`] into a reused buffer.
    pub fn polyline_upto_into(&self, t: f64, out: &mut Vec<Point>) {
        out.clear();
        if t >= 1.0 || self.total <= 1e-6 {
            out.extend_from_slice(&self.points);
            return;
        }
        if t <= 0.0 {
            out.push(self.points[0]);
            return;
        }
        let target = t * self.total;
        out.push(self.points[0]);
        for i in 1..self.points.len() {
            if self.cum[i] <= target {
                out.push(self.points[i]);
            } else {
                let seg = self.cum[i] - self.cum[i - 1];
                let local = if seg <= 1e-6 { 0.0 } else { (target - self.cum[i - 1]) / seg };
                let (ax, ay) = self.points[i - 1];
                let (bx, by) = self.points[i];
                out.push((lerp(ax, bx, local), lerp(ay, by, local)));
                break;
            }
        }
    }

    pub fn segment_count(&self) -> usize {
        self.points.len().saturating_sub(1)
    }

    pub fn segment_length(&self, i: usize) -> f64 {
        if i + 1 >= self.points.len() {
            return 0.0;
        }
        let (a, b) = (self.points[i], self.points[i + 1]);
        distance(a.0, a.1, b.0, b.1)
    }

    /// Distance from `(x, y)` to segment `i`.
    pub fn segment_dist(&self, i: usize, x: f64, y: f64) -> f64 {
        if i + 1 >= self.points.len() {
            return f64::INFINITY;
        }
        let (ax, ay) = self.points[i];
        let (bx, by) = self.points[i + 1];
        let (dx, dy) = (bx - ax, by - ay);
        let d2 = dx * dx + dy * dy;
        if d2 <= 1e-9 {
            return distance(ax, ay, x, y);
        }
        let t = clamp(((x - ax) * dx + (y - ay) * dy) / d2, 0.0, 1.0);
        distance(ax + dx * t, ay + dy * t, x, y)
    }
}

/// `_connect`: insert short `move` strands so the route is one unbroken path.
fn connect(strands: Vec<Strand>) -> Vec<Strand> {
    if strands.is_empty() {
        return strands;
    }
    let mut out: Vec<Strand> = Vec::with_capacity(strands.len() * 2);
    for (n, strand) in strands.into_iter().enumerate() {
        if n > 0 {
            let (px, py) = out.last().unwrap().end();
            let (sx, sy) = strand.start();
            if distance(px, py, sx, sy) > 1.5 {
                out.push(Strand::new(vec![(px, py), (sx, sy)], false, StrandKind::Move, false));
            }
        }
        out.push(strand);
    }
    out
}

// ======================================================================
// Geometry helpers
// ======================================================================

/// `_unit`.
pub fn unit(dx: f64, dy: f64) -> Point {
    let m = pymath::hypot(dx, dy);
    if m <= 1e-9 {
        return (1.0, 0.0);
    }
    (dx / m, dy / m)
}

fn sag_points(points: &[Point], anchors: &[Point], droop: f64) -> Vec<Point> {
    if droop <= 0.0 || anchors.is_empty() {
        return points.to_vec();
    }
    points
        .iter()
        .map(|&(x, y)| {
            let mut nearest = f64::INFINITY;
            for (n, &(ax, ay)) in anchors.iter().enumerate() {
                let d = distance(x, y, ax, ay);
                if n == 0 || d < nearest {
                    nearest = d;
                }
            }
            (x, y + droop * py_pow(nearest, 0.5))
        })
        .collect()
}

fn apply_sag(strands: Vec<Strand>, anchors: &[Point], droop: f64) -> Vec<Strand> {
    strands
        .into_iter()
        .map(|s| {
            let sagged = sag_points(&s.points, anchors, droop);
            Strand::new(sagged, s.draw, s.kind, s.temporary)
        })
        .collect()
}

fn clamp_screen(p: Point, sw: f64, sh: f64) -> Point {
    (clamp(p.0, 2.0, sw - 2.0), clamp(p.1, 2.0, sh - 2.0))
}

/// A planned web: `(strands, hub, wobble_dir)`.
pub type Plan = (Vec<Strand>, Point, Point);

/// `_plan_corner_orb`: the sector orb tucked into a screen corner.
pub fn plan_corner_orb(
    vertex: Point,
    din: Point,
    span: f64,
    _reach_inset: f64,
    screen_w: f64,
    screen_h: f64,
    rng: &mut PyRandom,
) -> Plan {
    let (vx, vy) = vertex;
    let hub_off = clamp(span * 0.20, 38.0, 80.0);
    let hub = (vx + din.0 * hub_off, vy + din.1 * hub_off);
    let r = span;
    let base = din.1.atan2(din.0);
    let sector = 1.55;
    let nr = rng.randint(9, 13) as usize;

    let mut tips = Vec::with_capacity(nr);
    let mut radii_angles = Vec::with_capacity(nr);
    for i in 0..nr {
        let a = base - sector + (2.0 * sector) * (i as f64 / (nr - 1) as f64);
        let tip = clamp_screen((hub.0 + a.cos() * r, hub.1 + a.sin() * r), screen_w, screen_h);
        tips.push(tip);
        radii_angles.push((tip.1 - hub.1).atan2(tip.0 - hub.0));
    }

    let mut strands = vec![Strand::silk(vec![hub, vertex], StrandKind::Bridge)];
    let mut frame_pts = vec![vertex, tips[0]];
    frame_pts.extend_from_slice(&tips);
    frame_pts.push(*tips.last().unwrap());
    frame_pts.push(vertex);
    strands.push(Strand::silk(frame_pts, StrandKind::Frame));
    let hub_r = r * 0.10;
    let hub_loop: Vec<Point> = (0..=nr)
        .map(|i| {
            let a = base - sector + (2.0 * sector) * (i as f64 / nr as f64);
            (hub.0 + a.cos() * hub_r, hub.1 + a.sin() * hub_r)
        })
        .collect();
    strands.push(Strand::silk(hub_loop, StrandKind::Hub));

    let aux_loops = 3;
    let aux_inner = hub_r * 1.4;
    let reaches: Vec<f64> = tips.iter().map(|t| distance(hub.0, hub.1, t.0, t.1)).collect();
    let (aux_pts, aux_cross) =
        spiral_on_radii(hub, &radii_angles, aux_inner, r * 0.96, aux_loops, false, Some(&reaches));
    let cap_loops = 5.max(nr as i64 - 4);
    let rev_angles: Vec<f64> = radii_angles.iter().rev().copied().collect();
    let rev_reaches: Vec<f64> = reaches.iter().rev().copied().collect();
    let (cap_pts, cap_cross) =
        spiral_on_radii(hub, &rev_angles, r * 0.92, aux_inner * 1.2, cap_loops, false, Some(&rev_reaches));
    let last = radii_angles.len() - 1;
    let cap_cross: Vec<(usize, f64)> = cap_cross.into_iter().map(|(i, rr)| (last - i, rr)).collect();
    strands.extend(radii_through(hub, &radii_angles, &tips, &[&aux_cross, &cap_cross]));
    strands.push(Strand::new(aux_pts, true, StrandKind::Aux, true));
    strands.push(Strand::silk(cap_pts, StrandKind::Capture));

    let strands = connect(strands);
    let strands = apply_sag(strands, &[vertex, tips[0], *tips.last().unwrap()], span * 0.012);
    (strands, hub, din)
}

/// `_plan_orb`: a full circular orb spun in the open, hub lifted for gravity.
pub fn plan_orb(center: Point, span: f64, screen_w: f64, screen_h: f64, rng: &mut PyRandom) -> Plan {
    let (cx, cy) = center;
    let hub = (cx, cy - span * 0.12);
    let r = span;
    let nr = rng.randint(11, 15) as usize;
    let mut tips = Vec::with_capacity(nr);
    let mut radii_angles = Vec::with_capacity(nr);
    for i in 0..nr {
        let a = -PI + (2.0 * PI) * (i as f64 / nr as f64);
        let tip = clamp_screen((hub.0 + a.cos() * r, hub.1 + a.sin() * r), screen_w, screen_h);
        tips.push(tip);
        radii_angles.push((tip.1 - hub.1).atan2(tip.0 - hub.0));
    }

    let mut strands = vec![Strand::silk(vec![tips[0], hub], StrandKind::Bridge)];
    let mut frame = tips.clone();
    frame.push(tips[0]);
    strands.push(Strand::silk(frame, StrandKind::Frame));
    let hub_r = r * 0.09;
    let hub_loop: Vec<Point> = (0..=nr)
        .map(|i| {
            let a = -PI + 2.0 * PI * i as f64 / nr as f64;
            (hub.0 + a.cos() * hub_r, hub.1 + a.sin() * hub_r)
        })
        .collect();
    strands.push(Strand::silk(hub_loop, StrandKind::Hub));
    let reaches: Vec<f64> = tips.iter().map(|t| distance(hub.0, hub.1, t.0, t.1)).collect();
    let (aux_pts, aux_cross) = spiral_on_radii(hub, &radii_angles, hub_r * 1.5, r * 0.95, 4, true, Some(&reaches));
    let rev_angles: Vec<f64> = radii_angles.iter().rev().copied().collect();
    let rev_reaches: Vec<f64> = reaches.iter().rev().copied().collect();
    let (cap_pts, cap_cross) =
        spiral_on_radii(hub, &rev_angles, r * 0.92, hub_r * 1.8, 7.max(nr as i64 - 3), true, Some(&rev_reaches));
    let last = radii_angles.len() - 1;
    let cap_cross: Vec<(usize, f64)> = cap_cross.into_iter().map(|(i, rr)| (last - i, rr)).collect();
    strands.extend(radii_through(hub, &radii_angles, &tips, &[&aux_cross, &cap_cross]));
    strands.push(Strand::new(aux_pts, true, StrandKind::Aux, true));
    strands.push(Strand::silk(cap_pts, StrandKind::Capture));

    let strands = connect(strands);
    let strands = apply_sag(strands, &tips, span * 0.01);
    (strands, hub, (0.0, 1.0))
}

/// `_plan_funnel`: a dense sheet over the corner with a retreat tunnel.
pub fn plan_funnel(vertex: Point, din: Point, span: f64, screen_w: f64, screen_h: f64, rng: &mut PyRandom) -> Plan {
    let (vx, vy) = vertex;
    let base = din.1.atan2(din.0);
    let r = span;
    let mouth = (vx + din.0 * (r * 0.30), vy + din.1 * (r * 0.30));
    let (a0, a1) = (base - 1.5, base + 1.5);
    let edge0 = clamp_screen((vx + a0.cos() * r, vy + a0.sin() * r), screen_w, screen_h);
    let edge1 = clamp_screen((vx + a1.cos() * r, vy + a1.sin() * r), screen_w, screen_h);
    let apex = clamp_screen((vx + din.0 * r, vy + din.1 * r), screen_w, screen_h);

    let mut strands = vec![
        Strand::silk(vec![vertex, edge0], StrandKind::Frame),
        Strand::silk(vec![edge0, apex, edge1], StrandKind::Frame),
        Strand::silk(vec![edge1, vertex], StrandKind::Frame),
    ];
    let fan = 13;
    for i in 0..fan {
        let f = i as f64 / (fan - 1) as f64;
        let a = lerp(a0, a1, f);
        let fx = vx + a.cos() * r * rng.uniform(0.9, 1.0);
        let fy = vy + a.sin() * r * rng.uniform(0.9, 1.0);
        let far = clamp_screen((fx, fy), screen_w, screen_h);
        strands.push(Strand::silk(vec![mouth, far], StrandKind::Sheet));
    }
    for ring in [0.55, 0.78] {
        let steps = 9;
        let cross: Vec<Point> = (0..steps)
            .map(|i| {
                let a = lerp(a0, a1, i as f64 / (steps - 1) as f64);
                (vx + a.cos() * r * ring, vy + a.sin() * r * ring)
            })
            .collect();
        strands.push(Strand::silk(cross, StrandKind::Sheet));
    }
    let throat = 6;
    for i in 0..throat {
        let a = lerp(base - 0.7, base + 0.7, i as f64 / (throat - 1) as f64);
        let m = (vx + a.cos() * r * 0.30, vy + a.sin() * r * 0.30);
        strands.push(Strand::silk(vec![m, vertex], StrandKind::Funnel));
    }

    let strands = connect(strands);
    let strands = apply_sag(strands, &[vertex, edge0, edge1, apex], span * 0.006);
    (strands, mouth, din)
}

/// `_plan_tangle`: a cobweb mesh with gumfoot lines.
pub fn plan_tangle(vertex: Point, din: Point, span: f64, screen_w: f64, screen_h: f64, rng: &mut PyRandom) -> Plan {
    let (vx, vy) = vertex;
    let base = din.1.atan2(din.0);
    let r = span;
    let down = if din.1 >= 0.0 { 1.0 } else { -1.0 };

    let mut scaffold_ends: Vec<Point> = Vec::new();
    let mut strands: Vec<Strand> = Vec::new();
    let n_scaffold = rng.randint(4, 5);
    for i in 0..n_scaffold {
        let a = lerp(base - 1.45, base + 1.45, i as f64 / (n_scaffold - 1) as f64);
        let ex = vx + a.cos() * r * rng.uniform(0.82, 1.0);
        let ey = vy + a.sin() * r * rng.uniform(0.82, 1.0);
        let end = clamp_screen((ex, ey), screen_w, screen_h);
        scaffold_ends.push(end);
        strands.push(Strand::silk(vec![vertex, end], StrandKind::Scaffold));
    }

    let on_scaffold = |rng: &mut PyRandom, ends: &[Point]| -> Point {
        let end = *rng.choice(ends);
        let t = rng.uniform(0.25, 0.95);
        (lerp(vx, end.0, t), lerp(vy, end.1, t))
    };

    for _ in 0..rng.randint(14, 20) {
        let a = on_scaffold(rng, &scaffold_ends);
        let b = on_scaffold(rng, &scaffold_ends);
        strands.push(Strand::silk(vec![a, b], StrandKind::Tangle));
    }
    for _ in 0..rng.randint(2, 3) {
        let top = on_scaffold(rng, &scaffold_ends);
        let drop = r * rng.uniform(0.35, 0.6);
        let foot = clamp_screen((top.0 + rng.uniform(-6.0, 6.0), top.1 + down * drop), screen_w, screen_h);
        strands.push(Strand::silk(vec![top, foot], StrandKind::Gumfoot));
    }

    let strands = connect(strands);
    let mut anchors = vec![vertex];
    anchors.extend_from_slice(&scaffold_ends);
    let strands = apply_sag(strands, &anchors, span * 0.004);
    let hub = (vx + din.0 * r * 0.4, vy + din.1 * r * 0.4);
    (strands, hub, din)
}

/// `_spiral_on_radii`: a spiral whose every vertex sits on a radius (DC-70).
/// Returns the points and every `(spoke index, radius)` it touched.
pub fn spiral_on_radii(
    hub: Point,
    angles: &[f64],
    r_start: f64,
    r_end: f64,
    loops: i64,
    full: bool,
    reaches: Option<&[f64]>,
) -> (Vec<Point>, Vec<(usize, f64)>) {
    let n = angles.len();
    if n < 2 || loops < 1 {
        return (Vec::new(), Vec::new());
    }
    let n_i = n as i64;
    let (steps, period) = if full {
        (1.max(loops * n_i), 0)
    } else {
        let period = 2 * (n_i - 1);
        (1.max(loops * (n_i - 1)), period)
    };
    let index_at = |k: i64| -> usize {
        if full {
            (k % n_i) as usize
        } else {
            let pos = k % period;
            (if pos < n_i { pos } else { period - pos }) as usize
        }
    };
    let mut pts = Vec::with_capacity(steps as usize + 1);
    let mut crossings = Vec::with_capacity(steps as usize + 1);
    for k in 0..=steps {
        let mut rr = lerp(r_start, r_end, k as f64 / steps as f64);
        let i = index_at(k);
        if let Some(reaches) = reaches
            && i < reaches.len()
        {
            let cap = reaches[i] * 0.97;
            if cap < rr {
                rr = cap;
            }
        }
        let a = angles[i];
        crossings.push((i, rr));
        pts.push((hub.0 + a.cos() * rr, hub.1 + a.sin() * rr));
    }
    (pts, crossings)
}

/// `_radii_through`: radius strands with a vertex wherever a spiral is
/// knotted to them.
pub fn radii_through(hub: Point, angles: &[f64], tips: &[Point], crossing_lists: &[&[(usize, f64)]]) -> Vec<Strand> {
    let mut per_spoke: Vec<Vec<f64>> = vec![Vec::new(); angles.len()];
    for crossings in crossing_lists {
        for &(index, radius) in crossings.iter() {
            if index < per_spoke.len() {
                per_spoke[index].push(round_n(radius, 4));
            }
        }
    }
    let mut strands = Vec::with_capacity(angles.len());
    for (index, (&angle, &tip)) in angles.iter().zip(tips.iter()).enumerate() {
        let spoke = &mut per_spoke[index];
        spoke.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        spoke.dedup();
        let reach = distance(hub.0, hub.1, tip.0, tip.1);
        let mut points = vec![hub];
        for &radius in spoke.iter() {
            if 1e-6 < radius && radius < reach - 1e-6 {
                points.push((hub.0 + angle.cos() * radius, hub.1 + angle.sin() * radius));
            }
        }
        points.push(tip);
        strands.push(Strand::silk(points, StrandKind::Radius));
    }
    strands
}

/// The four planners, by pattern name (`_PATTERN_PLANNERS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    CornerOrb,
    Orb,
    Funnel,
    Tangle,
}

impl Pattern {
    pub fn as_str(self) -> &'static str {
        match self {
            Pattern::CornerOrb => "corner_orb",
            Pattern::Orb => "orb",
            Pattern::Funnel => "funnel",
            Pattern::Tangle => "tangle",
        }
    }
}

// ======================================================================
// Web
// ======================================================================

/// `Web.state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebState {
    Building,
    Complete,
}

/// A single web: silk geometry, build progress, bounce physics, tears.
#[derive(Clone, Debug)]
pub struct Web {
    pub id: WebId,
    pub pattern: String,
    pub strands: Vec<Strand>,
    pub hub: Point,
    pub screen_w: f64,
    pub screen_h: f64,
    pub wobble_dir: Point,
    pub spec_id: String,
    pub reach_inset: f64,
    /// Index of the strand being laid; everything before it is complete.
    pub built: usize,
    /// 0..1 along the strand being laid.
    pub active_t: f64,
    pub state: WebState,
    pub builder: Option<CreatureId>,
    pub just_completed: i32,
    pub capture_start: Option<usize>,
    pub capture_count: usize,
    pub bounce_amp: f64,
    pub bounce_phase: f64,
    pub bounce_omega: f64,
    pub bounce_origin: Point,
    /// Torn `(strand, segment)` pairs.
    pub cut: BTreeSet<(usize, usize)>,
    pub repairer: Option<CreatureId>,
    dmg_frames: i32,
    tearable_total: f64,
    /// `(strand index, segment index, length)` of every segment that can tear.
    tearable: Vec<(usize, usize, f64)>,
    bbox: Bbox,
    repair_accum: f64,
}

impl Web {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: WebId,
        pattern: &str,
        strands: Vec<Strand>,
        hub: Point,
        wobble_dir: Point,
        spec_id: &str,
        reach_inset: f64,
        screen_w: f64,
        screen_h: f64,
    ) -> Web {
        let mut capture_start = None;
        let mut capture_count = 0;
        for (i, s) in strands.iter().enumerate() {
            if s.kind == StrandKind::Capture {
                if capture_start.is_none() {
                    capture_start = Some(i);
                }
                capture_count += 1;
            }
        }
        let mut tearable = Vec::new();
        let mut tearable_total = 0.0;
        for (si, s) in strands.iter().enumerate() {
            if !s.draw || s.kind == StrandKind::Move {
                continue;
            }
            for segi in 0..s.segment_count() {
                let length = s.segment_length(segi);
                if length <= 1e-6 {
                    continue;
                }
                tearable.push((si, segi, length));
                tearable_total += length;
            }
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &strands {
            for &(x, y) in &s.points {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        let pad = 8.0;
        Web {
            id,
            pattern: pattern.to_string(),
            strands,
            hub,
            screen_w,
            screen_h,
            wobble_dir: unit(wobble_dir.0, wobble_dir.1),
            spec_id: spec_id.to_string(),
            reach_inset,
            built: 0,
            active_t: 0.0,
            state: WebState::Building,
            builder: None,
            just_completed: 0,
            capture_start,
            capture_count,
            bounce_amp: 0.0,
            bounce_phase: 0.0,
            bounce_omega: 17.0,
            bounce_origin: hub,
            cut: BTreeSet::new(),
            repairer: None,
            dmg_frames: 0,
            tearable_total,
            tearable,
            bbox: (x0 - pad, y0 - pad, x1 + pad, y1 + pad),
            repair_accum: 0.0,
        }
    }

    // -- progress ---------------------------------------------------------
    pub fn is_complete(&self) -> bool {
        self.state == WebState::Complete
    }

    pub fn progress(&self) -> f64 {
        let n = self.strands.len();
        if n == 0 {
            return 1.0;
        }
        clamp((self.built as f64 + self.active_t) / n as f64, 0.0, 1.0)
    }

    /// Opacity of the temporary auxiliary spiral.
    pub fn temp_alpha(&self) -> f64 {
        let Some(start) = self.capture_start else {
            return 1.0;
        };
        if self.built < start {
            return 1.0;
        }
        let done = (self.built - start) as f64 + self.active_t;
        clamp(1.0 - done / 1.0f64.max(self.capture_count as f64), 0.0, 1.0)
    }

    fn reachable(&self, p: Point, sw: f64, sh: f64) -> Point {
        let ins = self.reach_inset;
        (clamp(p.0, ins, sw - ins), clamp(p.1, ins, sh - ins))
    }

    /// `working_point(clamp_reachable=True)` on the web's own screen size.
    pub fn working_point(&self) -> (Point, bool) {
        self.working_point_on(true, self.screen_w, self.screen_h)
    }

    /// Where the silk tip is right now, and whether the spider is drawing.
    pub fn working_point_on(&self, clamp_reachable: bool, sw: f64, sh: f64) -> (Point, bool) {
        if self.built >= self.strands.len() {
            return (self.hub, false);
        }
        let strand = &self.strands[self.built];
        let mut tip = strand.point_at(self.active_t);
        if clamp_reachable {
            tip = self.reachable(tip, sw, sh);
        }
        (tip, strand.draw)
    }

    /// Lay a little more silk, but only if the spider has kept up.
    pub fn advance(&mut self, dt: f64, weave_speed: f64, spider_xy: Point, lead_max: f64) {
        if self.built >= self.strands.len() {
            if self.state != WebState::Complete {
                self.state = WebState::Complete;
                self.just_completed = 3;
            }
            return;
        }
        let strand = &self.strands[self.built];
        let tip = strand.point_at(self.active_t);
        let rtip = self.reachable(tip, self.screen_w, self.screen_h);
        if distance(rtip.0, rtip.1, spider_xy.0, spider_xy.1) > lead_max {
            return;
        }
        let eff = weave_speed * if !strand.draw { 3.0 } else { 1.0 };
        let length = 1.0f64.max(strand.total);
        self.active_t += dt * eff / length;
        while self.active_t >= 1.0 && self.built < self.strands.len() {
            self.built += 1;
            self.active_t -= 1.0;
            if self.built >= self.strands.len() {
                self.active_t = 0.0;
                self.state = WebState::Complete;
                self.just_completed = 3;
                break;
            }
        }
    }

    // -- wobble -------------------------------------------------------------
    pub fn pluck(&mut self, at: Point, strength: f64) {
        self.bounce_origin = at;
        self.bounce_amp = self.bounce_amp.max(clamp(strength, 0.2, 1.6) * 6.5);
        self.bounce_phase = 0.0;
    }

    pub fn update(&mut self, dt: f64) {
        if self.bounce_amp > 0.01 {
            self.bounce_phase += dt * self.bounce_omega;
            self.bounce_amp *= (-3.0 * dt).exp();
            if self.bounce_amp <= 0.01 {
                self.bounce_amp = 0.0;
            }
        }
        if self.just_completed > 0 {
            self.just_completed -= 1;
        }
        if self.dmg_frames > 0 {
            self.dmg_frames -= 1;
        }
    }

    pub fn is_animating(&self) -> bool {
        self.state == WebState::Building || self.bounce_amp > 0.01 || self.just_completed > 0 || self.dmg_frames > 0
    }

    // -- tearing / repair -------------------------------------------------
    pub fn is_damaged(&self) -> bool {
        !self.cut.is_empty()
    }

    /// The segments that can tear (and that a fly can be caught on).
    pub fn tearable(&self) -> &[(usize, usize, f64)] {
        &self.tearable
    }

    /// Fraction of the web's silk length still present.
    pub fn intact_fraction(&self) -> f64 {
        if self.tearable_total <= 1e-6 {
            return 1.0;
        }
        let mut torn = 0.0;
        for &(si, segi) in &self.cut {
            if si < self.strands.len() {
                torn += self.strands[si].segment_length(segi);
            }
        }
        clamp(1.0 - torn / self.tearable_total, 0.0, 1.0)
    }

    /// Snap the segments within `radius` of the point. Returns how many.
    pub fn damage_near(&mut self, x: f64, y: f64, radius: f64) -> usize {
        if !self.is_complete() {
            return 0;
        }
        let (x0, y0, x1, y1) = self.bbox;
        if x < x0 - radius || x > x1 + radius || y < y0 - radius || y > y1 + radius {
            return 0;
        }
        let mut newly = 0;
        for &(si, segi, _len) in &self.tearable {
            if self.cut.contains(&(si, segi)) {
                continue;
            }
            if self.strands[si].segment_dist(segi, x, y) <= radius {
                self.cut.insert((si, segi));
                newly += 1;
            }
        }
        if newly > 0 {
            self.dmg_frames = self.dmg_frames.max(3);
            self.pluck((x, y), 0.7);
        }
        newly
    }

    pub fn damaged_centroid(&self) -> Option<Point> {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0usize);
        for &(si, segi) in &self.cut {
            if si < self.strands.len() {
                let s = &self.strands[si];
                if segi + 1 < s.points.len() {
                    let (ax, ay) = s.points[segi];
                    let (bx, by) = s.points[segi + 1];
                    sx += (ax + bx) * 0.5;
                    sy += (ay + by) * 0.5;
                    n += 1;
                }
            }
        }
        if n == 0 { None } else { Some((sx / n as f64, sy / n as f64)) }
    }

    /// Re-knit torn segments, nearest to `(x, y)` first. Returns how many.
    pub fn repair_near(&mut self, x: f64, y: f64, dt: f64, rate: f64, radius: f64) -> usize {
        if self.cut.is_empty() {
            return 0;
        }
        self.repair_accum += rate * dt;
        let n = self.repair_accum.trunc() as i64;
        if n <= 0 {
            return 0;
        }
        self.repair_accum -= n as f64;
        let mut candidates: Vec<(f64, (usize, usize))> = Vec::with_capacity(self.cut.len());
        for &(si, segi) in &self.cut {
            if si < self.strands.len() {
                let d = self.strands[si].segment_dist(segi, x, y);
                if d <= radius {
                    candidates.push((d, (si, segi)));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut mended = 0;
        for (_d, key) in candidates.into_iter().take(n as usize) {
            self.cut.remove(&key);
            mended += 1;
        }
        if mended > 0 {
            self.dmg_frames = self.dmg_frames.max(3);
        }
        mended
    }

    // -- geometry ---------------------------------------------------------
    /// `(x0, y0, x1, y1)` footprint over the true geometry, padded.
    pub fn bbox(&self) -> Bbox {
        self.bbox
    }

    pub fn footprint_xywh(&self) -> Xywh {
        let (x0, y0, x1, y1) = self.bbox;
        (x0, y0, x1 - x0, y1 - y0)
    }

    pub fn contains_region(&self, sw: f64, sh: f64) -> bool {
        let (x0, y0, x1, y1) = self.bbox;
        x1 >= 0.0 && y1 >= 0.0 && x0 <= sw && y0 <= sh
    }

    /// A reachable spot on the web for another spider to stand and pluck.
    pub fn walkable_point(&self, rng: &mut PyRandom, screen_w: f64, screen_h: f64) -> Point {
        let candidates: Vec<usize> = (0..self.strands.len())
            .filter(|&i| {
                let s = &self.strands[i];
                s.draw
                    && matches!(
                        s.kind,
                        StrandKind::Capture
                            | StrandKind::Radius
                            | StrandKind::Frame
                            | StrandKind::Sheet
                            | StrandKind::Scaffold
                            | StrandKind::Aux
                    )
            })
            .collect();
        let ins = self.reach_inset;
        let p = if candidates.is_empty() {
            self.hub
        } else {
            let s = &self.strands[*rng.choice(&candidates)];
            s.point_at(rng.uniform(0.3, 0.8))
        };
        (clamp(p.0, ins, screen_w - ins), clamp(p.1, ins, screen_h - ins))
    }

    /// Whether the wobble is visible this frame (`bounce_amp > 0.01`).
    pub fn is_wobbling(&self) -> bool {
        self.bounce_amp > 0.01
    }

    /// `_wobble_offset`: where a silk point is drawn while the web quivers.
    pub fn wobble_offset(&self, p: Point) -> Point {
        if self.bounce_amp <= 0.01 {
            return p;
        }
        let d = distance(p.0, p.1, self.bounce_origin.0, self.bounce_origin.1);
        let fall = (-d / 90.0).exp();
        let amt = self.bounce_amp * fall * (self.bounce_phase - d * 0.035).cos();
        (p.0 + self.wobble_dir.0 * amt, p.1 + self.wobble_dir.1 * amt)
    }

    /// How much of strand `i` is drawn (the `portion` in `Web.draw`), or
    /// `None` when nothing of it shows. Move strands never draw.
    pub fn strand_portion(&self, i: usize) -> Option<f64> {
        let s = self.strands.get(i)?;
        if !s.draw {
            return None;
        }
        let portion = if i < self.built {
            1.0
        } else if i == self.built && self.state != WebState::Complete {
            self.active_t
        } else if self.state == WebState::Complete {
            1.0
        } else {
            return None;
        };
        if portion <= 0.0 { None } else { Some(portion) }
    }

    /// Whether `(strand, segment)` has been torn (draw only the gaps).
    pub fn is_cut(&self, si: usize, segi: usize) -> bool {
        self.cut.contains(&(si, segi))
    }

    // -- persistence ------------------------------------------------------
    /// `Web.to_dict`.
    pub fn to_dict(&self) -> Value {
        let strands: Vec<Value> = self
            .strands
            .iter()
            .map(|s| {
                json!({
                    "kind": s.kind.as_str(),
                    "draw": s.draw,
                    "temporary": s.temporary,
                    "points": s.points.iter().map(|&(x, y)| json!([x, y])).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut m = Map::new();
        m.insert("pattern".into(), Value::from(self.pattern.clone()));
        m.insert("spec_id".into(), Value::from(self.spec_id.clone()));
        m.insert("hub".into(), json!([self.hub.0, self.hub.1]));
        m.insert("wobble_dir".into(), json!([self.wobble_dir.0, self.wobble_dir.1]));
        m.insert("reach_inset".into(), Value::from(self.reach_inset));
        m.insert("built".into(), Value::from(self.built as i64));
        m.insert("active_t".into(), Value::from(self.active_t));
        m.insert("state".into(), Value::from(self.state_str()));
        m.insert("cut".into(), self.cut.iter().map(|&(a, b)| json!([a, b])).collect::<Vec<_>>().into());
        m.insert("strands".into(), Value::Array(strands));
        Value::Object(m)
    }

    pub fn state_str(&self) -> &'static str {
        match self.state {
            WebState::Building => "building",
            WebState::Complete => "complete",
        }
    }
}

/// `web_from_dict`: rebuild a saved web, or `None` if the entry is unusable.
pub fn web_from_dict(id: WebId, data: &Value, screen_w: f64, screen_h: f64) -> Option<Web> {
    if !data.is_object() {
        return None;
    }
    let mut strands = Vec::new();
    if let Some(raws) = data.get("strands").filter(|v| pymath::truthy(v)).and_then(Value::as_array) {
        for raw in raws {
            if !raw.is_object() {
                continue;
            }
            let mut points = Vec::new();
            if let Some(pts) = raw.get("points").filter(|v| pymath::truthy(v)).and_then(Value::as_array) {
                for point in pts {
                    if let (Some(x), Some(y)) = (pymath::float_at(point, 0), pymath::float_at(point, 1)) {
                        points.push((x, y));
                    }
                }
            }
            if points.len() < 2 {
                continue;
            }
            let draw = raw.get("draw").map(pymath::truthy).unwrap_or(true);
            let kind = pymath::get_str(raw, "kind", "frame");
            let temporary = raw.get("temporary").map(pymath::truthy).unwrap_or(false);
            strands.push(Strand::new(points, draw, StrandKind::parse(&kind), temporary));
        }
    }
    if strands.is_empty() {
        return None;
    }
    let hub_v = data.get("hub")?;
    let wob_v = data.get("wobble_dir")?;
    let hub = (pymath::float_at(hub_v, 0)?, pymath::float_at(hub_v, 1)?);
    let mut wobble = (pymath::float_at(wob_v, 0)?, pymath::float_at(wob_v, 1)?);
    if wobble == (0.0, 0.0) {
        wobble = (0.0, -1.0);
    }
    let reach_inset = pymath::get_float(data, "reach_inset", 38.0).unwrap_or(38.0);
    let pattern = pymath::get_str(data, "pattern", "orb");
    let spec_id = pymath::get_str(data, "spec_id", "");
    let mut web = Web::new(id, &pattern, strands, hub, wobble, &spec_id, reach_inset, screen_w, screen_h);
    let n = web.strands.len() as i64;
    web.built = match pymath::get_int(data, "built", 0) {
        Some(b) => b.min(n).max(0) as usize,
        None => 0,
    };
    web.active_t = pymath::get_float(data, "active_t", 0.0).map(|t| clamp(t, 0.0, 1.0)).unwrap_or(0.0);
    let state = data.get("state").map(pymath::py_str).unwrap_or_default();
    web.state = if state == "complete" { WebState::Complete } else { WebState::Building };
    if web.state == WebState::Complete {
        web.built = web.strands.len();
    }
    if let Some(pairs) = data.get("cut").filter(|v| pymath::truthy(v)).and_then(Value::as_array) {
        for pair in pairs {
            let (Some(si), Some(segi)) = (
                pymath::py_index(pair, 0).and_then(|v| py_int(&v)),
                pymath::py_index(pair, 1).and_then(|v| py_int(&v)),
            ) else {
                continue;
            };
            if si >= 0 && (si as usize) < web.strands.len() && segi >= 0 {
                let (si, segi) = (si as usize, segi as usize);
                if segi < web.strands[si].segment_count() {
                    web.cut.insert((si, segi));
                }
            }
        }
    }
    web.builder = None;
    web.repairer = None;
    Some(web)
}

// ======================================================================
// WebWorld
// ======================================================================

/// `spec["kind"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecKind {
    Corner,
    Edge,
    Open,
}

/// `_random_open_spec`: a site out in the open, away from the walls.
pub fn random_open_spec(w: f64, h: f64, rng: &mut PyRandom) -> SiteSpec {
    let mx = w * 0.16;
    let my = h * 0.16;
    let vx = rng.uniform(mx, w - mx);
    let vy = rng.uniform(my, h - my);
    let din = unit((w * 0.5) - vx, (h * 0.5) - vy);
    SiteSpec {
        id: format!("open_{}_{}", pymath::fmt0(vx), pymath::fmt0(vy)),
        kind: SpecKind::Open,
        vertex: (vx, vy),
        din,
    }
}

/// One candidate anchor site (`_specs` / `_random_open_spec`).
#[derive(Clone, Debug, PartialEq)]
pub struct SiteSpec {
    pub id: String,
    pub kind: SpecKind,
    pub vertex: Point,
    pub din: Point,
}

/// Holds every web; hands sites to weavers; finds webs to finish or walk on.
#[derive(Debug)]
pub struct WebWorld {
    pub rng: PyRandom,
    pub screen_w: f64,
    pub screen_h: f64,
    pub webs: Vec<Web>,
    dirty: Vec<Xywh>,
    removed: Vec<Xywh>,
    prev_cursor: Option<Point>,
    next_id: u64,
}

impl WebWorld {
    pub fn new(screen_w: f64, screen_h: f64, rng: PyRandom) -> WebWorld {
        WebWorld {
            rng,
            screen_w,
            screen_h,
            webs: Vec::new(),
            dirty: Vec::new(),
            removed: Vec::new(),
            prev_cursor: None,
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> WebId {
        let id = WebId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn web(&self, id: WebId) -> Option<&Web> {
        self.webs.iter().find(|w| w.id == id)
    }

    pub fn web_mut(&mut self, id: WebId) -> Option<&mut Web> {
        self.webs.iter_mut().find(|w| w.id == id)
    }

    pub fn contains(&self, id: WebId) -> bool {
        self.webs.iter().any(|w| w.id == id)
    }

    /// Add a web built elsewhere (tests, editors), giving it a fresh id.
    pub fn push_web(&mut self, mut web: Web) -> WebId {
        let id = self.alloc_id();
        web.id = id;
        self.webs.push(web);
        id
    }

    /// Build a web from a plan with a fresh id (not added).
    #[allow(clippy::too_many_arguments)]
    pub fn make_web(&mut self, pattern: &str, plan: Plan, spec_id: &str, reach_inset: f64) -> Web {
        let id = self.alloc_id();
        let (strands, hub, wob) = plan;
        Web::new(id, pattern, strands, hub, wob, spec_id, reach_inset, self.screen_w, self.screen_h)
    }

    // -- screen / lifecycle ---------------------------------------------
    pub fn set_screen(&mut self, screen_w: f64, screen_h: f64) {
        self.screen_w = screen_w;
        self.screen_h = screen_h;
        let mut kept = Vec::with_capacity(self.webs.len());
        for mut web in self.webs.drain(..) {
            web.screen_w = screen_w;
            web.screen_h = screen_h;
            if web.contains_region(screen_w, screen_h) {
                kept.push(web);
            } else {
                self.removed.push(web.footprint_xywh());
            }
        }
        self.webs = kept;
    }

    pub fn clear(&mut self) {
        for web in &self.webs {
            self.removed.push(web.footprint_xywh());
        }
        self.webs.clear();
    }

    // -- persistence ------------------------------------------------------
    /// `to_dict`: every web, for `Scene.webs`.
    pub fn to_dict(&self) -> Vec<Value> {
        self.webs.iter().map(Web::to_dict).collect()
    }

    /// `restore`: replace the webs with saved ones; returns how many came back.
    pub fn restore(&mut self, entries: &Value) -> usize {
        let Some(list) = entries.as_array() else {
            return 0;
        };
        self.restore_list(list)
    }

    pub fn restore_list(&mut self, list: &[Value]) -> usize {
        self.clear();
        for entry in list.iter().take(MAX_WEBS) {
            let id = WebId(self.next_id);
            let Some(web) = web_from_dict(id, entry, self.screen_w, self.screen_h) else {
                continue;
            };
            if !web.contains_region(self.screen_w, self.screen_h) {
                continue;
            }
            self.next_id += 1;
            self.dirty.push(web.footprint_xywh());
            self.webs.push(web);
        }
        self.webs.len()
    }

    // -- anchor specs ---------------------------------------------------
    /// `_specs`: the four corners and two edges.
    pub fn specs(&self) -> Vec<SiteSpec> {
        let (w, h) = (self.screen_w, self.screen_h);
        let inv = 1.0 / 2.0f64.sqrt();
        let spec = |id: &str, kind, vertex, din| SiteSpec { id: id.to_string(), kind, vertex, din };
        vec![
            spec("corner_nw", SpecKind::Corner, (0.0, 0.0), (inv, inv)),
            spec("corner_ne", SpecKind::Corner, (w, 0.0), (-inv, inv)),
            spec("corner_sw", SpecKind::Corner, (0.0, h), (inv, -inv)),
            spec("corner_se", SpecKind::Corner, (w, h), (-inv, -inv)),
            spec("edge_top", SpecKind::Edge, (w * 0.5, 0.0), (0.0, 1.0)),
            spec("edge_bottom", SpecKind::Edge, (w * 0.5, h), (0.0, -1.0)),
        ]
    }

    fn occupied(&self, spec_id: &str) -> bool {
        self.webs.iter().any(|w| w.spec_id == spec_id)
    }

    fn span_for(&self, base_size: f64) -> f64 {
        let small = self.screen_w.min(self.screen_h);
        clamp(base_size * 6.0, 90.0, small * 0.34)
    }

    /// `_pick_pattern`. `weights` are already-parsed `(pattern, weight)`
    /// pairs; unknown names are ignored, as in Python.
    pub fn pick_pattern(spec: &SiteSpec, weights: Option<&[(&str, f64)]>, rng: &mut PyRandom) -> Pattern {
        if spec.kind == SpecKind::Edge {
            return if rng.random() < 0.7 { Pattern::Orb } else { Pattern::CornerOrb };
        }
        let mut w = [(Pattern::CornerOrb, 0.62), (Pattern::Funnel, 0.2), (Pattern::Tangle, 0.18)];
        if let Some(weights) = weights {
            for &(k, v) in weights {
                for entry in w.iter_mut() {
                    if entry.0.as_str() == k {
                        entry.1 = v.max(0.0);
                    }
                }
            }
        }
        let mut total = py_sum(w.iter().map(|e| e.1));
        if total == 0.0 {
            total = 1.0;
        }
        let roll = rng.random() * total;
        let mut acc = 0.0;
        for (k, v) in w {
            acc += v;
            if roll <= acc {
                return k;
            }
        }
        Pattern::CornerOrb
    }

    fn too_close_to_existing(&self, p: Point, min_sep: f64) -> bool {
        self.webs.iter().any(|w| distance(p.0, p.1, w.hub.0, w.hub.1) < min_sep)
    }

    // -- weaver API -------------------------------------------------------
    /// `claim_site`: start a fresh web for `builder` at a free corner, edge or
    /// open spot. `rng` chooses the site and pattern (the caller's stream;
    /// `None` uses the world's own, as an unseeded Python world did); the
    /// silk geometry always comes from the world's stream.
    pub fn claim_site(
        &mut self,
        builder: CreatureId,
        size: f64,
        prefer_corner: bool,
        weights: Option<&[(&str, f64)]>,
        mut caller_rng: Option<&mut PyRandom>,
    ) -> Option<WebId> {
        macro_rules! pick_rng {
            () => {
                match caller_rng.as_deref_mut() {
                    Some(r) => r,
                    None => &mut self.rng,
                }
            };
        }
        if self.webs.len() >= MAX_WEBS {
            return None;
        }
        let (sw, sh) = (self.screen_w, self.screen_h);
        let mut specs = self.specs();
        for _ in 0..4 {
            specs.push(random_open_spec(sw, sh, pick_rng!()));
        }
        pick_rng!().shuffle(&mut specs);
        if prefer_corner {
            specs.sort_by_key(|s| if s.kind == SpecKind::Corner { 0 } else { 1 });
        }
        let base_size = size;
        let reach_inset = base_size * 0.7 + 22.0;
        for spec in &specs {
            if matches!(spec.kind, SpecKind::Corner | SpecKind::Edge) && self.occupied(&spec.id) {
                continue;
            }
            let span = self.span_for(base_size);
            let (plan, pattern) = if spec.kind == SpecKind::Open {
                if self.too_close_to_existing(spec.vertex, span * 1.25) {
                    continue;
                }
                (plan_orb(spec.vertex, span * 0.8, sw, sh, &mut self.rng), Pattern::Orb)
            } else {
                let pattern = Self::pick_pattern(spec, weights, pick_rng!());
                let plan = if spec.kind == SpecKind::Edge && pattern == Pattern::Orb {
                    let center = (spec.vertex.0, spec.vertex.1 + spec.din.1 * span);
                    plan_orb(center, span, sw, sh, &mut self.rng)
                } else if pattern == Pattern::Orb {
                    let center = (spec.vertex.0 + spec.din.0 * span, spec.vertex.1 + spec.din.1 * span);
                    plan_orb(center, span * 0.8, sw, sh, &mut self.rng)
                } else if pattern == Pattern::CornerOrb {
                    plan_corner_orb(spec.vertex, spec.din, span, reach_inset, sw, sh, &mut self.rng)
                } else if pattern == Pattern::Funnel {
                    plan_funnel(spec.vertex, spec.din, span, sw, sh, &mut self.rng)
                } else {
                    plan_tangle(spec.vertex, spec.din, span, sw, sh, &mut self.rng)
                };
                (plan, pattern)
            };
            let mut web = self.make_web(pattern.as_str(), plan, &spec.id, reach_inset);
            web.builder = Some(builder);
            let id = web.id;
            self.webs.push(web);
            return Some(id);
        }
        None
    }

    fn nearest(&self, x: f64, y: f64, max_dist: f64, pred: impl Fn(&Web) -> bool) -> Option<WebId> {
        let mut best = None;
        let mut best_d = max_dist;
        for web in &self.webs {
            if !pred(web) {
                continue;
            }
            let d = distance(x, y, web.hub.0, web.hub.1);
            if d <= best_d {
                best_d = d;
                best = Some(web.id);
            }
        }
        best
    }

    /// Nearest unfinished web nobody is building.
    pub fn find_adoptable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.nearest(x, y, max_dist, |w| w.state == WebState::Building && w.builder.is_none())
    }

    /// Nearest finished web to walk onto.
    pub fn find_walkable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.nearest(x, y, max_dist, |w| w.is_complete())
    }

    /// Take over an abandoned unfinished web.
    pub fn adopt(&mut self, web: WebId, creature: CreatureId) -> bool {
        match self.web_mut(web) {
            Some(w) if w.state == WebState::Building && w.builder.is_none() => {
                w.builder = Some(creature);
                true
            }
            _ => false,
        }
    }

    /// Nearest finished, torn web nobody is mending.
    pub fn find_repairable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId> {
        self.nearest(x, y, max_dist, |w| w.is_complete() && w.is_damaged() && w.repairer.is_none())
    }

    pub fn claim_repair(&mut self, web: WebId, creature: CreatureId) -> bool {
        match self.web_mut(web) {
            Some(w) if w.is_complete() && w.repairer.is_none() => {
                w.repairer = Some(creature);
                true
            }
            _ => false,
        }
    }

    pub fn release_repair(&mut self, web: Option<WebId>) {
        if let Some(w) = web.and_then(|id| self.web_mut(id)) {
            w.repairer = None;
        }
    }

    /// Finished webs that are not torn.
    pub fn intact_complete_count(&self) -> usize {
        self.webs.iter().filter(|w| w.is_complete() && !w.is_damaged()).count()
    }

    /// Leave an in-progress web for anyone to finish; a barely started one
    /// is dropped.
    pub fn abandon(&mut self, web: Option<WebId>) {
        let Some(pos) = web.and_then(|id| self.webs.iter().position(|w| w.id == id)) else {
            return;
        };
        let w = &mut self.webs[pos];
        w.builder = None;
        if w.state == WebState::Building && w.built <= 1 && w.active_t < 0.2 {
            self.removed.push(w.footprint_xywh());
            self.webs.remove(pos);
        }
    }

    pub fn remove(&mut self, web: WebId) {
        if let Some(pos) = self.webs.iter().position(|w| w.id == web) {
            self.removed.push(self.webs[pos].footprint_xywh());
            self.webs.remove(pos);
        }
    }

    /// `walkable_point` on one web, from the world's stream.
    pub fn walkable_point(&mut self, web: WebId) -> Option<Point> {
        let (sw, sh) = (self.screen_w, self.screen_h);
        let pos = self.webs.iter().position(|w| w.id == web)?;
        Some(self.webs[pos].walkable_point(&mut self.rng, sw, sh))
    }

    // -- per-frame --------------------------------------------------------
    /// Tear webs along the moving cursor, advance wobble, collect dirty rects.
    pub fn update(&mut self, dt: f64, cursor: Option<Point>) {
        if let Some((cx, cy)) = cursor
            && dt > 1e-6
        {
            if let Some((px, py)) = self.prev_cursor {
                let speed = distance(cx, cy, px, py) / dt;
                if speed >= TEAR_MIN_CURSOR_SPEED && !self.webs.is_empty() {
                    let steps = 1.max(6.min((distance(cx, cy, px, py) / TEAR_RADIUS) as i64));
                    for web in &mut self.webs {
                        if !web.is_complete() {
                            continue;
                        }
                        for s in 0..=steps {
                            let f = s as f64 / steps as f64;
                            web.damage_near(lerp(px, cx, f), lerp(py, cy, f), TEAR_RADIUS);
                        }
                    }
                }
            }
            self.prev_cursor = Some((cx, cy));
        }
        self.dirty.clear();
        for web in &mut self.webs {
            web.update(dt);
            if web.is_animating() {
                self.dirty.push(web.footprint_xywh());
            }
        }
        self.dirty.append(&mut self.removed);
    }

    pub fn dirty_rects(&self) -> &[Xywh] {
        &self.dirty
    }

    pub fn count(&self) -> usize {
        self.webs.len()
    }

    pub fn count_building(&self) -> usize {
        self.webs.iter().filter(|w| w.state == WebState::Building).count()
    }

    /// Webs a fly can be caught on (`webs` by index), for the fly world.
    pub fn webs_mut(&mut self) -> &mut [Web] {
        &mut self.webs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    #[test]
    fn spiral_visits_each_spoke_in_turn() {
        let angles: Vec<f64> = (0..8).map(|i| i as f64 * TAU / 8.0).collect();
        let (pts, crossings) = spiral_on_radii((0.0, 0.0), &angles, 100.0, 10.0, 3, true, None);
        assert_eq!(pts.len(), crossings.len());
        let visited: Vec<usize> = crossings.iter().map(|c| c.0).collect();
        assert_eq!(&visited[..9], &[0, 1, 2, 3, 4, 5, 6, 7, 0]);
    }

    #[test]
    fn sector_spiral_folds_back() {
        let angles: Vec<f64> = (0..6).map(|i| i as f64 * 0.2).collect();
        let (_p, crossings) = spiral_on_radii((0.0, 0.0), &angles, 100.0, 10.0, 2, false, None);
        let visited: Vec<usize> = crossings.iter().map(|c| c.0).collect();
        assert_eq!(&visited[..6], &[0, 1, 2, 3, 4, 5]);
        assert_eq!(&visited[6..11], &[4, 3, 2, 1, 0]);
    }

    #[test]
    fn radius_shrinks_all_the_way_in() {
        let angles: Vec<f64> = (0..10).map(|i| i as f64 * TAU / 10.0).collect();
        let (_p, crossings) = spiral_on_radii((0.0, 0.0), &angles, 200.0, 20.0, 4, true, None);
        let radii: Vec<f64> = crossings.iter().map(|c| c.1).collect();
        assert!((radii[0] - 200.0).abs() < 1e-9);
        assert!((radii.last().unwrap() - 20.0).abs() < 1e-9);
        assert!(radii.windows(2).all(|w| w[0] >= w[1] - 1e-9));
    }

    #[test]
    fn a_clipped_spoke_carries_no_silk_past_its_tip() {
        let reaches = [300.0, 40.0, 300.0];
        let (_p, crossings) = spiral_on_radii((0.0, 0.0), &[0.0, 0.5, 1.0], 250.0, 20.0, 3, true, Some(&reaches));
        for (i, r) in crossings {
            assert!(r <= reaches[i] + 1e-6);
        }
    }

    fn plans() -> Vec<(String, Vec<Strand>)> {
        let mut out = Vec::new();
        for seed in [4u64, 9, 17, 23] {
            let mut rng = PyRandom::from_u64(seed);
            out.push((format!("orb/{seed}"), plan_orb((600.0, 400.0), 260.0, 1200.0, 800.0, &mut rng).0));
            let mut rng = PyRandom::from_u64(seed);
            out.push((
                format!("corner/{seed}"),
                plan_corner_orb((40.0, 40.0), (0.707, 0.707), 300.0, 38.0, 1200.0, 800.0, &mut rng).0,
            ));
        }
        out
    }

    #[test]
    fn every_spiral_vertex_sits_on_a_radius() {
        for (label, strands) in plans() {
            let knots: Vec<Point> = strands
                .iter()
                .filter(|s| s.kind == StrandKind::Radius)
                .flat_map(|s| s.points.iter().map(|&(x, y)| (round_n(x, 2), round_n(y, 2))))
                .collect();
            assert!(!knots.is_empty(), "{label}");
            for s in strands.iter().filter(|s| matches!(s.kind, StrandKind::Capture | StrandKind::Aux)) {
                for &(px, py) in &s.points {
                    let gap =
                        knots.iter().map(|&(kx, ky)| pymath::hypot(px - kx, py - ky)).fold(f64::INFINITY, f64::min);
                    assert!(gap <= 0.75, "{label}: loose vertex {gap}");
                }
            }
        }
    }

    #[test]
    fn webs_have_spirals_and_bent_spokes() {
        for (label, strands) in plans() {
            let radii: Vec<&Strand> = strands.iter().filter(|s| s.kind == StrandKind::Radius).collect();
            assert!(radii.len() >= 9, "{label}");
            assert!(radii.iter().all(|s| s.points.len() > 2), "{label}");
            let capture: usize = strands.iter().filter(|s| s.kind == StrandKind::Capture).map(|s| s.points.len()).sum();
            assert!(capture > 40, "{label}");
        }
    }

    #[test]
    fn a_web_round_trips_through_its_dict() {
        let mut world = WebWorld::new(1200.0, 800.0, PyRandom::from_u64(3));
        let id = world.claim_site(CreatureId(1), 24.0, true, None, Some(&mut PyRandom::from_u64(9))).unwrap();
        let web = world.web_mut(id).unwrap();
        web.built = web.strands.len();
        web.state = WebState::Complete;
        web.cut.insert((1, 0));
        let saved = world.to_dict();
        let mut other = WebWorld::new(1200.0, 800.0, PyRandom::from_u64(3));
        assert_eq!(other.restore(&Value::Array(saved.clone())), 1);
        assert_eq!(other.to_dict(), saved);
        assert!(other.webs[0].builder.is_none());
    }

    #[test]
    fn a_cursor_swipe_tears_and_a_mender_repairs() {
        let mut world = WebWorld::new(1200.0, 800.0, PyRandom::from_u64(3));
        let plan = plan_orb((600.0, 400.0), 200.0, 1200.0, 800.0, &mut PyRandom::from_u64(1));
        let mut web = world.make_web("orb", plan, "t", 30.0);
        web.state = WebState::Complete;
        web.built = web.strands.len();
        let id = web.id;
        world.webs.push(web);
        world.update(1.0 / 60.0, Some((300.0, 380.0)));
        world.update(1.0 / 60.0, Some((900.0, 380.0)));
        let web = world.web_mut(id).unwrap();
        assert!(web.is_damaged());
        assert!(web.intact_fraction() < 1.0);
        for _ in 0..600 {
            web.repair_near(600.0, 380.0, 1.0 / 60.0, REPAIR_RATE, 1e9);
        }
        assert!(!web.is_damaged());
    }
}
