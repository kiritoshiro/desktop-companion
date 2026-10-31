//! Polyline stroking and dashing shared by the GPU backend and
//! [`Path::stroked`](crate::Path::stroked).
//!
//! The stroker emits *overlapping* triangles (segment quads plus join and
//! cap fans). The GPU backend draws every command at its own depth with a
//! strict depth test, so overlaps never blend twice; the result matches Qt's
//! single-coverage stroke region. `Path::stroked` turns the triangles into a
//! winding-rule path, whose union is the same region.

use crate::paint::{Cap, Join};
use crate::path::Polylines;

pub(crate) trait TriOut {
    fn vertex(&mut self, p: [f32; 2]) -> u32;
    fn tri(&mut self, a: u32, b: u32, c: u32);
}

#[derive(Default)]
pub(crate) struct TriSink {
    pub verts: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl TriOut for TriSink {
    fn vertex(&mut self, p: [f32; 2]) -> u32 {
        self.verts.push(p);
        self.verts.len() as u32 - 1
    }
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StrokeParams {
    pub half_width: f32,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f32,
    /// Flattening tolerance in the same units as the points.
    pub tolerance: f32,
}

#[inline]
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
#[inline]
fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
#[inline]
fn mul(a: [f32; 2], s: f32) -> [f32; 2] {
    [a[0] * s, a[1] * s]
}
#[inline]
fn norm(a: [f32; 2]) -> Option<[f32; 2]> {
    let l = (a[0] * a[0] + a[1] * a[1]).sqrt();
    (l > 1e-12).then(|| [a[0] / l, a[1] / l])
}
#[inline]
fn perp(d: [f32; 2]) -> [f32; 2] {
    [-d[1], d[0]]
}

/// Angular step for round joins/caps of radius `r` at tolerance `tol`.
#[inline]
fn round_step(r: f32, tol: f32) -> f32 {
    if r <= tol {
        std::f32::consts::FRAC_PI_2
    } else {
        (2.0 * (1.0 - tol / r).acos()).clamp(0.05, std::f32::consts::FRAC_PI_2)
    }
}

/// Triangle fan around `c`, starting at offset `v0`, rotating by `sweep`
/// radians (positive = from x towards y).
pub(crate) fn fan(out: &mut impl TriOut, c: [f32; 2], v0: [f32; 2], sweep: f32, r: f32, tol: f32) {
    let n = ((sweep.abs() / round_step(r, tol)).ceil() as usize).clamp(1, 512);
    let step = sweep / n as f32;
    let (s, co) = step.sin_cos();
    let ci = out.vertex(c);
    let mut v = v0;
    let mut prev = out.vertex(add(c, v));
    for _ in 0..n {
        v = [v[0] * co - v[1] * s, v[0] * s + v[1] * co];
        let cur = out.vertex(add(c, v));
        out.tri(ci, prev, cur);
        prev = cur;
    }
}

/// A filled disc (round dot).
pub(crate) fn disc(out: &mut impl TriOut, c: [f32; 2], r: f32, tol: f32) {
    fan(out, c, [r, 0.0], std::f32::consts::TAU, r, tol);
}

/// The dot Qt draws for a zero-length stroke: a disc for round caps, a
/// square for square caps, nothing for flat caps.
pub(crate) fn dot(out: &mut impl TriOut, c: [f32; 2], st: &StrokeParams) {
    let hw = st.half_width;
    match st.cap {
        Cap::Round => disc(out, c, hw, st.tolerance),
        Cap::Square => {
            quad(out, [c[0] - hw, c[1] - hw], [c[0] + hw, c[1] - hw], [c[0] + hw, c[1] + hw], [c[0] - hw, c[1] + hw])
        }
        Cap::Flat => {}
    }
}

#[inline]
fn quad(out: &mut impl TriOut, a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) {
    let ia = out.vertex(a);
    let ib = out.vertex(b);
    let ic = out.vertex(c);
    let id = out.vertex(d);
    out.tri(ia, ib, ic);
    out.tri(ia, ic, id);
}

pub(crate) fn stroke_polylines(lines: &Polylines, st: &StrokeParams, out: &mut impl TriOut) {
    for c in &lines.contours {
        stroke_contour(lines.contour(c), c.closed, st, out);
    }
}

pub(crate) fn stroke_contour(pts: &[[f32; 2]], closed: bool, st: &StrokeParams, out: &mut impl TriOut) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let closed = closed && n > 2;
    let hw = st.half_width;
    let seg_count = if closed { n } else { n - 1 };
    // Segment directions; zero-length segments are skipped by carrying the
    // previous direction.
    let dir = |i: usize| norm(sub(pts[(i + 1) % n], pts[i]));
    let mut first_dir = None;
    let mut last_dir = None;
    let mut prev_dir: Option<[f32; 2]> = None;
    let mut first_joint: Option<usize> = None;
    for i in 0..seg_count {
        let Some(d) = dir(i) else { continue };
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let nrm = mul(perp(d), hw);
        let mut a2 = a;
        let mut b2 = b;
        if !closed && st.cap == Cap::Square {
            if first_dir.is_none() {
                a2 = sub(a, mul(d, hw));
            }
            if i == seg_count - 1 {
                b2 = add(b, mul(d, hw));
            }
        }
        quad(out, add(a2, nrm), add(b2, nrm), sub(b2, nrm), sub(a2, nrm));
        if let Some(pd) = prev_dir {
            join(out, a, pd, d, st);
        } else {
            first_joint = Some(i);
        }
        if first_dir.is_none() {
            first_dir = Some(d);
        }
        prev_dir = Some(d);
        last_dir = Some(d);
    }
    let (Some(fd), Some(ld)) = (first_dir, last_dir) else {
        // Every segment had zero length: Qt draws the cap as a dot.
        let st2 = *st;
        dot(out, pts[0], &st2);
        return;
    };
    if closed {
        let j = first_joint.unwrap_or(0);
        join(out, pts[j], ld, fd, st);
    } else if st.cap == Cap::Round {
        let start = pts[0];
        let end = pts[n - 1];
        fan(out, start, mul(perp(fd), hw), std::f32::consts::PI, hw, st.tolerance);
        fan(out, end, mul(perp(ld), -hw), std::f32::consts::PI, hw, st.tolerance);
    }
}

fn join(out: &mut impl TriOut, p: [f32; 2], d0: [f32; 2], d1: [f32; 2], st: &StrokeParams) {
    let hw = st.half_width;
    let cr = d0[0] * d1[1] - d0[1] * d1[0];
    let dt = d0[0] * d1[0] + d0[1] * d1[1];
    if cr.abs() < 1e-6 && dt > 0.0 {
        return;
    }
    let s = if cr > 0.0 { -hw } else { hw };
    let v0 = mul(perp(d0), s);
    let v1 = mul(perp(d1), s);
    let o0 = add(p, v0);
    let o1 = add(p, v1);
    // Inner wedge: fills the gap on very short segments.
    {
        let a = out.vertex(p);
        let b = out.vertex(sub(p, v0));
        let c = out.vertex(sub(p, v1));
        out.tri(a, b, c);
    }
    let theta = cr.abs().atan2(dt);
    match st.join {
        Join::Round => {
            if hw * (1.0 - (theta * 0.5).cos()) < st.tolerance * 0.5 {
                bevel(out, p, o0, o1);
            } else {
                let sweep = (v0[0] * v1[1] - v0[1] * v1[0]).atan2(v0[0] * v1[0] + v0[1] * v1[1]);
                fan(out, p, v0, sweep, hw, st.tolerance);
            }
        }
        Join::Bevel => bevel(out, p, o0, o1),
        Join::Miter | Join::SvgMiter => {
            let half_cos = ((1.0 + dt) * 0.5).max(0.0).sqrt();
            if half_cos > 1e-6
                && 1.0 / half_cos <= 2.0 * st.miter_limit.max(0.0)
                && let Some(bis) = norm(add(v0, v1))
            {
                let tip = add(p, mul(bis, hw / half_cos));
                let a = out.vertex(p);
                let b = out.vertex(o0);
                let c = out.vertex(tip);
                let d = out.vertex(o1);
                out.tri(a, b, c);
                out.tri(a, c, d);
                return;
            }
            bevel(out, p, o0, o1);
        }
    }
}

#[inline]
fn bevel(out: &mut impl TriOut, p: [f32; 2], o0: [f32; 2], o1: [f32; 2]) {
    let a = out.vertex(p);
    let b = out.vertex(o0);
    let c = out.vertex(o1);
    out.tri(a, b, c);
}

/// Splits contours into dashes. `pattern` alternates on/off lengths (already
/// in path units); each subpath restarts the pattern at `offset`, like Qt.
pub(crate) fn dash(input: &Polylines, pattern: &[f32], offset: f32, out: &mut Polylines) {
    out.clear();
    let total: f32 = pattern.iter().sum();
    if pattern.len() < 2 || total <= 1e-6 {
        out.pts.extend_from_slice(&input.pts);
        out.contours.extend_from_slice(&input.contours);
        return;
    }
    for c in &input.contours {
        let s = input.contour(c);
        let n = s.len();
        if n < 2 {
            continue;
        }
        let edge_count = if c.closed { n } else { n - 1 };
        let mut length = 0.0f32;
        for i in 0..edge_count {
            let (a, b) = (s[i], s[(i + 1) % n]);
            length += (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        if length / total > 200_000.0 {
            out.contours.push(crate::path::Contour {
                start: out.pts.len() as u32,
                end: (out.pts.len() + n) as u32,
                closed: c.closed,
            });
            out.pts.extend_from_slice(s);
            continue;
        }
        // Position within the pattern.
        let mut idx = 0usize;
        let mut rem = pattern[0];
        let mut off = offset.rem_euclid(total);
        while off > 0.0 {
            if off >= rem {
                off -= rem;
                idx = (idx + 1) % pattern.len();
                rem = pattern[idx];
            } else {
                rem -= off;
                off = 0.0;
            }
        }
        let mut on = idx.is_multiple_of(2);
        if on {
            out.begin(s[0]);
        }
        for i in 0..edge_count {
            let (a, b) = (s[i], s[(i + 1) % n]);
            let seg = (b[0] - a[0]).hypot(b[1] - a[1]);
            if seg <= 0.0 {
                continue;
            }
            let mut pos = 0.0f32;
            while seg - pos > rem {
                pos += rem;
                let t = pos / seg;
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                if on {
                    out.push(p);
                } else {
                    out.begin(p);
                }
                on = !on;
                idx = (idx + 1) % pattern.len();
                rem = pattern[idx];
            }
            rem -= seg - pos;
            if on {
                out.push(b);
            }
        }
    }
    // Drop single-point dashes produced by zero-length "on" parts.
    out.contours.retain(|c| c.end > c.start + 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(t: &TriSink) -> f32 {
        t.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|i| {
                let (a, b, c) = (t.verts[i[0] as usize], t.verts[i[1] as usize], t.verts[i[2] as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }

    fn params(hw: f32, cap: Cap, join: Join) -> StrokeParams {
        StrokeParams { half_width: hw, cap, join, miter_limit: 2.0, tolerance: 0.01 }
    }

    #[test]
    fn straight_line_area() {
        let mut t = TriSink::default();
        stroke_contour(&[[0.0, 0.0], [10.0, 0.0]], false, &params(1.0, Cap::Flat, Join::Bevel), &mut t);
        assert!((area(&t) - 20.0).abs() < 1e-4);
        let mut t = TriSink::default();
        stroke_contour(&[[0.0, 0.0], [10.0, 0.0]], false, &params(1.0, Cap::Square, Join::Bevel), &mut t);
        assert!((area(&t) - 24.0).abs() < 1e-4);
        let mut t = TriSink::default();
        stroke_contour(&[[0.0, 0.0], [10.0, 0.0]], false, &params(1.0, Cap::Round, Join::Bevel), &mut t);
        assert!((area(&t) - (20.0 + std::f32::consts::PI)).abs() < 0.06);
    }

    #[test]
    fn dashes_split_lines() {
        let mut pl = Polylines::default();
        pl.begin([0.0, 0.0]);
        pl.push([10.0, 0.0]);
        let mut out = Polylines::default();
        dash(&pl, &[2.0, 1.0], 0.0, &mut out);
        // 0-2, 3-5, 6-8, 9-10
        assert_eq!(out.contours.len(), 4);
        assert_eq!(out.contour(&out.contours[1]), &[[3.0, 0.0], [5.0, 0.0]]);
        assert_eq!(out.contour(&out.contours[3]), &[[9.0, 0.0], [10.0, 0.0]]);
        let mut out2 = Polylines::default();
        dash(&pl, &[2.0, 1.0], 1.0, &mut out2);
        assert_eq!(out2.contour(&out2.contours[0]), &[[0.0, 0.0], [1.0, 0.0]]);
    }
}
