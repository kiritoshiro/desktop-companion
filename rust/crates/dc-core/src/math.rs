//! Port of `support/math_utils.py`. Scalars are `f64` throughout the
//! simulation so results stay close to the Python originals.

use crate::rng::PyRandom;
use std::f64::consts::{PI, TAU};

pub use glam::DVec2 as Vec2;

pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    // `max(low, min(high, value))` — keeps Python's behaviour when low > high.
    low.max(high.min(value))
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * clamp(t, 0.0, 1.0)
}

pub fn smoothstep(t: f64) -> f64 {
    let t = clamp(t, 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    (bx - ax).hypot(by - ay)
}

pub fn normalize_angle(mut angle: f64) -> f64 {
    if !angle.is_finite() {
        return angle;
    }
    while angle <= -PI {
        angle += TAU;
    }
    while angle > PI {
        angle -= TAU;
    }
    angle
}

pub fn angle_to(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    (by - ay).atan2(bx - ax)
}

pub fn angle_lerp(current: f64, target: f64, max_step: f64) -> f64 {
    let delta = clamp(normalize_angle(target - current), -max_step.abs(), max_step.abs());
    normalize_angle(current + delta)
}

pub fn vec_from_angle(angle: f64) -> (f64, f64) {
    (angle.cos(), angle.sin())
}

/// `rand_range(pair, low, high, rng)`: uniform within `pair` when present.
pub fn rand_range(pair: Option<[f64; 2]>, default_low: f64, default_high: f64, rng: &mut PyRandom) -> f64 {
    match pair {
        Some([lo, hi]) => rng.uniform(lo, hi),
        None => rng.uniform(default_low, default_high),
    }
}

pub fn clamp_point(x: f64, y: f64, margin: f64, width: f64, height: f64) -> (f64, f64) {
    (clamp(x, margin, width - margin), clamp(y, margin, height - margin))
}

#[allow(clippy::too_many_arguments)]
pub fn cursor_is_threatening(
    spider_x: f64,
    spider_y: f64,
    cursor_x: f64,
    cursor_y: f64,
    cursor_vx: f64,
    cursor_vy: f64,
    threat_radius: f64,
    speed_threshold: f64,
    toward_score_threshold: f64,
) -> bool {
    let dx = spider_x - cursor_x;
    let dy = spider_y - cursor_y;
    let dist = dx.hypot(dy).max(1.0);
    if dist > threat_radius {
        return false;
    }
    let cursor_speed = cursor_vx.hypot(cursor_vy);
    if cursor_speed < speed_threshold {
        return false;
    }
    let score = (cursor_vx * dx + cursor_vy * dy) / (cursor_speed.max(1.0) * dist);
    score > toward_score_threshold
}

/// Python's `round()` (banker's rounding to even) for porting `round(x)`.
pub fn py_round(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }
}

/// Python's `%` on floats (result takes the sign of the divisor).
pub fn py_mod(a: f64, b: f64) -> f64 {
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) { r + b } else { r }
}

/// Python's `//` on floats.
pub fn py_floordiv(a: f64, b: f64) -> f64 {
    (a / b).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_wrap_like_python() {
        assert!((normalize_angle(3.0 * PI) - PI).abs() < 1e-12);
        assert!((normalize_angle(-PI) - PI).abs() < 1e-12);
        assert_eq!(py_mod(-1.0, 3.0), 2.0);
        assert_eq!(py_round(2.5), 2.0);
        assert_eq!(py_round(3.5), 4.0);
    }
}
