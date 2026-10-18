//! Port of `creature/kinematics/geometry.py`: body/world frames, leg sectors,
//! reach limits and constraints.
//!
//! Every method takes the leg by index into [`Kinematics::legs`] (and the
//! matching [`crate::config::Rig::legs`] definition) instead of a `LegState`
//! object, so the borrow of the leg never overlaps the borrow of the body.

use crate::core::{Kinematics, ReachCache, Sector};
use crate::pymath::hypot;
use dc_core::math::clamp;

/// `_leg_reach_metrics` result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReachMetrics {
    pub dist: f64,
    pub max_reach: f64,
    pub too_far: bool,
    pub very_far: bool,
}

/// `_leg_alignment_metrics` result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentMetrics {
    pub local_f: f64,
    pub local_s: f64,
    pub min_side: f64,
    pub rest_f: f64,
    pub wrong_side: bool,
    /// `severe_wrong_side or forward_wrong`
    pub severe_wrong: bool,
}

/// The five world-space point pairs `_soft_limit_leg_state` may move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegPoint {
    Foot,
    StepStart,
    StepTarget,
    StepControl,
    PendingTarget,
}

impl Kinematics {
    /// `_basis()`: forward and right unit vectors `(fx, fy, rx, ry)` for the
    /// current heading. Cached on the heading value itself.
    #[inline]
    pub fn basis(&self) -> [f64; 4] {
        let (cached_heading, cached) = self.basis_cache.get();
        if cached_heading == self.heading {
            return cached;
        }
        let fx = self.heading.cos();
        let fy = self.heading.sin();
        let basis = [fx, fy, -fy, fx];
        self.basis_cache.set((self.heading, basis));
        basis
    }

    /// `_side_sign(leg.definition["side"])`
    #[inline]
    pub fn side_sign(&self, i: usize) -> f64 {
        self.rig.legs[i].side_sign
    }

    #[inline]
    pub fn world_to_body_local(&self, x: f64, y: f64) -> (f64, f64) {
        let [fx, fy, rx, ry] = self.basis();
        let dx = x - self.x;
        let dy = y - self.y;
        (dx * fx + dy * fy, dx * rx + dy * ry)
    }

    #[inline]
    pub fn body_local_to_world(&self, forward: f64, side: f64) -> (f64, f64) {
        let [fx, fy, rx, ry] = self.basis();
        (self.x + fx * forward + rx * side, self.y + fy * forward + ry * side)
    }

    /// `_compute_leg_sectors()`: each leg's angular territory around the body.
    pub fn compute_leg_sectors(&mut self) {
        let n = self.rig.legs.len();
        self.sectors.clear();
        self.sectors.resize(n, None);
        let rest_polar = |i: usize| -> (f64, f64) {
            let d = &self.rig.legs[i];
            let (ang, dist) = if d.has_rest() {
                let rf = d.rest_forward_or();
                let rs = d.rest_side_or().abs();
                (rs.atan2(rf).to_degrees(), hypot(rf, rs))
            } else {
                (d.rest_angle.unwrap_or(90.0).abs(), d.reach_or() * 0.62)
            };
            (ang, 0.6f64.max(dist))
        };
        let mut out = vec![None; n];
        for side in ['l', 'r'] {
            let mut idxs: Vec<usize> = (0..n).filter(|&i| self.rig.legs[i].side_initial == Some(side)).collect();
            // Stable sort by rest angle, as `list.sort(key=...)`.
            idxs.sort_by(|&a, &b| rest_polar(a).0.partial_cmp(&rest_polar(b).0).unwrap_or(std::cmp::Ordering::Equal));
            let m = idxs.len();
            for (k, &i) in idxs.iter().enumerate() {
                let (ang, dist) = rest_polar(i);
                let prev_ang = if k > 0 { Some(rest_polar(idxs[k - 1]).0) } else { None };
                let next_ang = if k + 1 < m { Some(rest_polar(idxs[k + 1]).0) } else { None };
                let mut lo = match prev_ang {
                    Some(p) => (ang + p) * 0.5,
                    None => 6.0f64.max(ang - 26.0),
                };
                let mut hi = match next_ang {
                    Some(nx) => (ang + nx) * 0.5,
                    None => 174.0f64.min(ang + 26.0),
                };
                lo += 2.5;
                hi -= 2.5;
                if hi <= lo {
                    let mid = (lo + hi) * 0.5;
                    lo = mid;
                    hi = mid;
                }
                out[i] =
                    Some(Sector { lo: lo.to_radians(), hi: hi.to_radians(), rmin_m: dist * 0.42, rmax_m: dist * 1.30 });
            }
        }
        self.sectors = out;
    }

    /// The leg's sector, if its side starts with `l` or `r`.
    #[inline]
    pub fn leg_sector(&self, i: usize) -> Option<Sector> {
        self.sectors.get(i).copied().flatten()
    }

    /// `_clamp_to_sector`: pull a foot into its leg's wedge and distance range.
    pub fn clamp_to_sector(&self, i: usize, x: f64, y: f64) -> (f64, f64) {
        let Some(sec) = self.leg_sector(i) else {
            return (x, y);
        };
        let sign = self.side_sign(i);
        let (f, s_signed) = self.world_to_body_local(x, y);
        let s = s_signed * sign;
        let r = clamp(hypot(f, s), self.size * sec.rmin_m, self.size * sec.rmax_m);
        let ang = s.max(1e-4).atan2(f);
        let ang = clamp(ang, sec.lo, sec.hi);
        let f2 = r * ang.cos();
        let s2 = r * ang.sin();
        self.body_local_to_world(f2, s2 * sign)
    }

    /// `_leg_max_reach(leg, visual)`: the generous safety reach limit.
    /// Cached per leg on the body size.
    pub fn leg_max_reach(&self, i: usize, visual: bool) -> f64 {
        let cell = &self.reach_cache[i];
        let cached = cell.get();
        if cached.size == self.size {
            return cached.values[visual as usize];
        }
        let values = [self.compute_leg_max_reach(i, false), self.compute_leg_max_reach(i, true)];
        cell.set(ReachCache { size: self.size, values });
        values[visual as usize]
    }

    fn compute_leg_max_reach(&self, i: usize, visual: bool) -> f64 {
        let d = &self.rig.legs[i];
        let size = self.size;
        let reach = (size * 0.85).max(d.reach_or() * size);
        let upper = (size * 0.22).max(d.upper_len_or() * size);
        let lower = (size * 0.22).max(d.lower_len_or() * size);
        let rest_f = d.rest_forward_or() * size;
        let rest_s = (d.rest_side_or() * size).abs();
        let attach_f = d.attach_forward_or() * size;
        let attach_s = (d.attach_side_or() * size).abs();
        let rest_dist = hypot(rest_f - attach_f, rest_s - attach_s);
        let chain_limit = (upper + lower) * (if visual { 1.08 } else { 1.18 });
        let target_limit = reach * (if visual { 1.04 } else { 1.14 });
        let rest_allowance = rest_dist * (if visual { 1.18 } else { 1.30 });
        chain_limit.min(target_limit).max(rest_allowance).max(size * 0.92)
    }

    /// `_leg_reach_metrics(leg, x, y, visual)`
    pub fn leg_reach_metrics(&self, i: usize, x: f64, y: f64, visual: bool) -> ReachMetrics {
        let (ax, ay) = self.leg_attach(i);
        let (a_f, a_s) = self.world_to_body_local(ax, ay);
        let (f_f, f_s) = self.world_to_body_local(x, y);
        let dist = hypot(f_f - a_f, f_s - a_s);
        let max_reach = self.leg_max_reach(i, visual);
        ReachMetrics { dist, max_reach, too_far: dist > max_reach, very_far: dist > max_reach * 1.14 }
    }

    /// `_limit_world_point_to_leg_reach`: clamp to the coxa-centred reach circle.
    pub fn limit_world_point_to_leg_reach(&self, i: usize, x: f64, y: f64, visual: bool) -> (f64, f64) {
        let (ax, ay) = self.leg_attach(i);
        let (a_f, a_s) = self.world_to_body_local(ax, ay);
        let (mut f_f, mut f_s) = self.world_to_body_local(x, y);
        let dv_f = f_f - a_f;
        let dv_s = f_s - a_s;
        let dist = hypot(dv_f, dv_s);
        let max_reach = self.leg_max_reach(i, visual);
        if dist > max_reach {
            let scale = max_reach / dist.max(1e-5);
            f_f = a_f + dv_f * scale;
            f_s = a_s + dv_s * scale;
            return self.body_local_to_world(f_f, f_s);
        }
        (x, y)
    }

    fn leg_point(&self, i: usize, which: LegPoint) -> (f64, f64) {
        let l = &self.legs[i];
        match which {
            LegPoint::Foot => (l.foot_x, l.foot_y),
            LegPoint::StepStart => (l.step_start_x, l.step_start_y),
            LegPoint::StepTarget => (l.step_target_x, l.step_target_y),
            LegPoint::StepControl => (l.step_control_x, l.step_control_y),
            LegPoint::PendingTarget => (l.pending_target_x, l.pending_target_y),
        }
    }

    fn set_leg_point(&mut self, i: usize, which: LegPoint, x: f64, y: f64) {
        let l = &mut self.legs[i];
        match which {
            LegPoint::Foot => (l.foot_x, l.foot_y) = (x, y),
            LegPoint::StepStart => (l.step_start_x, l.step_start_y) = (x, y),
            LegPoint::StepTarget => (l.step_target_x, l.step_target_y) = (x, y),
            LegPoint::StepControl => (l.step_control_x, l.step_control_y) = (x, y),
            LegPoint::PendingTarget => (l.pending_target_x, l.pending_target_y) = (x, y),
        }
    }

    /// `_soft_limit_leg_state(leg, blend)`: gently pull impossible stored
    /// footfalls back after an external displacement.
    pub fn soft_limit_leg_state(&mut self, i: usize, blend: f64) {
        let blend = clamp(blend, 0.0, 1.0);
        if blend <= 0.0 {
            return;
        }
        let mut pairs = [LegPoint::Foot; 5];
        let mut n = 1;
        if !self.legs[i].stepping {
            pairs[1] = LegPoint::StepStart;
            pairs[2] = LegPoint::StepTarget;
            pairs[3] = LegPoint::StepControl;
            n = 4;
        }
        if !self.legs[i].pending_step {
            pairs[n] = LegPoint::PendingTarget;
            n += 1;
        }
        for &which in &pairs[..n] {
            let (x, y) = self.leg_point(i, which);
            let r = self.leg_reach_metrics(i, x, y, false);
            if !r.very_far {
                continue;
            }
            let (sx, sy) = self.limit_world_point_to_leg_reach(i, x, y, false);
            let (sx, sy) = self.constrain_leg_point(i, sx, sy);
            let use_blend = if which == LegPoint::Foot { blend } else { blend.min(0.55) };
            self.set_leg_point(i, which, x + (sx - x) * use_blend, y + (sy - y) * use_blend);
        }
    }

    /// `_leg_alignment_metrics(leg, x, y)`
    pub fn leg_alignment_metrics(&self, i: usize, x: f64, y: f64) -> AlignmentMetrics {
        let d = &self.rig.legs[i];
        let size = self.size;
        let sign = d.side_sign;
        let (local_f, local_s) = self.world_to_body_local(x, y);
        let rest_f = d.rest_forward_or() * size;
        let rest_s = (d.rest_side_or() * size).abs();
        let attach_f = d.attach_forward_or() * size;
        let attach_s = (d.attach_side_or() * size).abs();

        let min_side = (attach_s + size * 0.10).max(rest_s * 0.48);
        let side_mag = sign * local_s;
        let wrong_side = side_mag < min_side;
        let severe_wrong_side = side_mag < (size * 0.05).max(min_side * 0.36);

        let forward_wrong = if rest_f.abs() >= size * 0.22 {
            if rest_f >= 0.0 { local_f < attach_f - size * 0.22 } else { local_f > attach_f + size * 0.22 }
        } else {
            (local_f - rest_f).abs() > size * 0.95
        };
        AlignmentMetrics {
            local_f,
            local_s,
            min_side,
            rest_f,
            wrong_side,
            severe_wrong: severe_wrong_side || forward_wrong,
        }
    }

    /// `_constrain_leg_point`: keep a foot target on the anatomically correct
    /// side of the body and within a heading-relative envelope.
    pub fn constrain_leg_point(&self, i: usize, x: f64, y: f64) -> (f64, f64) {
        let d = &self.rig.legs[i];
        let size = self.size;
        let sign = d.side_sign;
        let (mut local_f, mut local_s) = self.world_to_body_local(x, y);

        let rest_f = d.rest_forward_or() * size;
        let rest_s = (d.rest_side_or() * size).abs();
        let attach_f = d.attach_forward_or() * size;
        let attach_s = (d.attach_side_or() * size).abs();
        let reach = (size * 0.7).max(d.reach_or() * size);

        let min_side = (attach_s + size * 0.10).max(rest_s * 0.48);
        let max_side = (min_side + size * 0.28).max(rest_s * 1.38);
        let clamped_side_mag = clamp(sign * local_s, min_side, max_side);
        local_s = sign * clamped_side_mag;

        let (forward_min, forward_max) = if rest_f.abs() >= size * 0.22 {
            if rest_f >= 0.0 {
                (attach_f.min(rest_f) - size * 0.55, attach_f.max(rest_f) + size * 0.95)
            } else {
                (attach_f.min(rest_f) - size * 0.95, attach_f.max(rest_f) + size * 0.55)
            }
        } else {
            (rest_f.min(attach_f) - size * 0.70, rest_f.max(attach_f) + size * 0.70)
        };
        local_f = clamp(local_f, forward_min, forward_max);

        let (ax, ay) = self.leg_attach(i);
        let (a_f, a_s) = self.world_to_body_local(ax, ay);
        let mut dv_f = local_f - a_f;
        let mut dv_s = local_s - a_s;
        let dist = hypot(dv_f, dv_s);
        let max_reach = reach * 1.08;
        let min_reach = (size * 0.20).max(reach * 0.24);
        if dist > max_reach {
            let scale = max_reach / dist.max(1e-5);
            dv_f *= scale;
            dv_s *= scale;
        } else if dist < min_reach {
            let scale = min_reach / dist.max(1e-5);
            dv_f *= scale;
            dv_s *= scale;
        }
        local_f = a_f + dv_f;
        local_s = a_s + dv_s;
        let final_side_mag = sign * local_s;
        if final_side_mag < min_side {
            local_s = sign * min_side;
        }
        self.body_local_to_world(local_f, local_s)
    }
}
