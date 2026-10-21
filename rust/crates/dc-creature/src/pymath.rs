//! `support/math_utils.py` with CPython-exact `math.hypot`, plus the
//! dict-style JSON reads the creature code does on its personality and model
//! (`float(personality.get(key, default))`, `rand_range(pair, ...)`).

use dc_core::PyRandom;
pub use dc_core::math::{angle_lerp, angle_to, clamp, clamp_point, normalize_angle, py_mod, py_round, smoothstep};
pub use dc_kinematics::pymath::{hypot, py_float, py_int, py_pow, py_str, py_truthy};
use serde_json::Value;

/// `distance(ax, ay, bx, by)` = `math.hypot(bx - ax, by - ay)`.
#[inline]
pub fn distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    hypot(bx - ax, by - ay)
}

/// `cursor_is_threatening(...)` with the default `toward_score_threshold=0.6`.
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
) -> bool {
    let dx = spider_x - cursor_x;
    let dy = spider_y - cursor_y;
    let dist = 1.0f64.max(hypot(dx, dy));
    if dist > threat_radius {
        return false;
    }
    let cursor_speed = hypot(cursor_vx, cursor_vy);
    if cursor_speed < speed_threshold {
        return false;
    }
    let score = (cursor_vx * dx + cursor_vy * dy) / (1.0f64.max(cursor_speed) * dist);
    score > 0.6
}

/// A `[lo, hi]` pair as `rand_range` accepts it: a list of at least two
/// numbers (`isinstance(pair, (list, tuple)) and len(pair) >= 2`).
pub fn pair_of(value: Option<&Value>) -> Option<[f64; 2]> {
    match value {
        Some(Value::Array(items)) if items.len() >= 2 => Some([py_float(&items[0])?, py_float(&items[1])?]),
        _ => None,
    }
}

/// `rand_range(pair, low, high, rng=rng)`: always exactly one `uniform` draw.
#[inline]
pub fn rand_range(pair: Option<&Value>, low: f64, high: f64, rng: &mut PyRandom) -> f64 {
    match pair_of(pair) {
        Some([a, b]) => rng.uniform(a, b),
        None => rng.uniform(low, high),
    }
}

/// `random.choice((-1.0, 1.0))`
#[inline]
pub fn choice_sign(rng: &mut PyRandom) -> f64 {
    if rng.choice_index(2) == 0 { -1.0 } else { 1.0 }
}

/// Read-only view of a JSON object with Python `dict.get` semantics.
#[derive(Clone, Copy)]
pub struct Dict<'a>(pub Option<&'a serde_json::Map<String, Value>>);

impl<'a> Dict<'a> {
    pub fn of(value: &'a Value) -> Self {
        Dict(value.as_object())
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&'a Value> {
        self.0.and_then(|m| m.get(key))
    }

    /// `float(d.get(key, default))`; the default also when the value is not
    /// a number (where Python would raise).
    #[inline]
    pub fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.get(key).and_then(py_float).unwrap_or(default)
    }

    /// `d.get(key)` as a nested object (`{}` when absent or not an object).
    pub fn dict(&self, key: &str) -> Dict<'a> {
        Dict(self.get(key).and_then(Value::as_object))
    }

    /// `str(d.get(key, default))`
    pub fn str_or(&self, key: &str, default: &str) -> String {
        match self.get(key) {
            Some(v) => py_str(v),
            None => default.to_string(),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}
