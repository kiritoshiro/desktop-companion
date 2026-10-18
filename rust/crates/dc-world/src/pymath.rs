//! CPython numeric and value semantics the world modules depend on.
//!
//! `math.hypot` in CPython 3.8+ is `vector_norm`, not the CRT `hypot`; the
//! worlds make threshold decisions on these distances every frame (a fly
//! deciding whether a spider is close enough to fear, a web strand within
//! tearing reach), so [`hypot`] reproduces it bit for bit. This is a copy of
//! `dc-kinematics::pymath::hypot` (checked there against CPython 3.13 on two
//! million pairs); it lives here too because `dc-world` may not depend on
//! `dc-kinematics`. Both belong in `dc-core` eventually.
//!
//! The JSON helpers mirror `float(x)`, `int(x)`, `str(x)`, `bool(x)` and
//! `x[i]` on values loaded from the runtime state file, so `from_dict` accepts
//! and rejects exactly what the Python accepted.

use serde_json::Value;

#[inline(always)]
fn split(a: f64) -> (f64, f64) {
    const FACTOR: f64 = 134_217_729.0; // 2^27 + 1
    let c = FACTOR * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

#[inline(always)]
fn dl_mul(x: f64, y: f64) -> (f64, f64) {
    let z = x * y;
    let (xh, xl) = split(x);
    let (yh, yl) = split(y);
    let zz = ((xh * yh - z) + xh * yl + xl * yh) + xl * yl;
    (z, zz)
}

#[inline(always)]
fn dl_fast_sum(a: f64, b: f64) -> (f64, f64) {
    let x = a + b;
    let y = (a - x) + b;
    (x, y)
}

#[inline(always)]
fn frexp_exp(x: f64) -> i32 {
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        let y = x * f64::from_bits(0x4350_0000_0000_0000); // 2^54
        let b2 = ((y.to_bits() >> 52) & 0x7ff) as i32;
        b2 - 1022 - 54
    } else {
        biased - 1022
    }
}

#[inline(always)]
fn pow2(e: i32) -> f64 {
    if e >= -1022 { f64::from_bits(((e + 1023) as u64) << 52) } else { f64::from_bits(1u64 << (e + 1074)) }
}

/// CPython's `math.hypot(x, y)`.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    let a = x.abs();
    let b = y.abs();
    if a.is_nan() || b.is_nan() {
        if a.is_infinite() || b.is_infinite() {
            return f64::INFINITY;
        }
        return f64::NAN;
    }
    let max = if a >= b { a } else { b };
    vector_norm2(a, b, max)
}

fn vector_norm2(mut a: f64, mut b: f64, max: f64) -> f64 {
    if max.is_infinite() || max == 0.0 {
        return max;
    }
    let max_e = frexp_exp(max);
    if max_e < -1023 {
        a /= f64::MIN_POSITIVE;
        b /= f64::MIN_POSITIVE;
        return f64::MIN_POSITIVE * vector_norm2(a, b, max / f64::MIN_POSITIVE);
    }
    let scale = pow2(-max_e);
    let mut csum = 1.0f64;
    let mut frac1 = 0.0f64;
    let mut frac2 = 0.0f64;
    for v in [a, b] {
        let x = v * scale;
        let (hi, lo) = dl_mul(x, x);
        let (s_hi, s_lo) = dl_fast_sum(csum, hi);
        csum = s_hi;
        frac1 += lo;
        frac2 += s_lo;
    }
    let mut h = (csum - 1.0 + (frac1 + frac2)).sqrt();
    let (hi, lo) = dl_mul(-h, h);
    let (s_hi, s_lo) = dl_fast_sum(csum, hi);
    csum = s_hi;
    frac1 += lo;
    frac2 += s_lo;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    h / scale
}

/// `support.math_utils.distance` (`math.hypot(bx - ax, by - ay)`).
#[inline]
pub fn distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    hypot(bx - ax, by - ay)
}

/// CPython 3.12+ `sum()` of floats (Neumaier compensated).
pub fn py_sum<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else {
        return 0.0;
    };
    let mut f = 0.0 + first;
    let mut c = 0.0f64;
    for x in iter {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// `x ** y` through the C library, never strength-reduced by the optimiser.
#[inline]
pub fn py_pow(x: f64, y: f64) -> f64 {
    x.powf(std::hint::black_box(y))
}

/// Python `round(x, ndigits)` (correctly rounded, ties to even).
pub fn round_n(x: f64, ndigits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.ndigits$}").parse().unwrap_or(x)
}

/// `f"{x:.0f}"`.
pub fn fmt0(x: f64) -> String {
    format!("{x:.0}")
}

// -- JSON values, Python style --------------------------------------------

/// `float(value)`; `None` where Python raises.
pub fn py_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => {
            let t = s.trim().replace('_', "");
            let lower = t.to_ascii_lowercase();
            match lower.trim_start_matches(['+', '-']) {
                "inf" | "infinity" => Some(if lower.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY }),
                "nan" => Some(f64::NAN),
                _ => t.parse::<f64>().ok(),
            }
        }
        _ => None,
    }
}

/// `int(value)`; `None` where Python raises.
pub fn py_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else {
                let f = n.as_f64()?;
                if f.is_finite() { Some(f.trunc() as i64) } else { None }
            }
        }
        Value::Bool(b) => Some(*b as i64),
        Value::String(s) => s.trim().replace('_', "").parse::<i64>().ok(),
        _ => None,
    }
}

/// `bool(value)` (Python truthiness).
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(value)` for the JSON scalars a state file can hold.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.fract() == 0.0 && f.abs() < 1e16 { format!("{f:.1}") } else { format!("{f}") }
            }
        }
        other => other.to_string(),
    }
}

/// `value[i]` for a list or a string; `None` where Python raises.
pub fn py_index(value: &Value, i: usize) -> Option<Value> {
    match value {
        Value::Array(a) => a.get(i).cloned(),
        Value::String(s) => s.chars().nth(i).map(|c| Value::String(c.to_string())),
        _ => None,
    }
}

/// `float(value[i])`.
pub fn float_at(value: &Value, i: usize) -> Option<f64> {
    py_float(&py_index(value, i)?)
}

/// `value.get(key, default)` then `float()`; `None` where Python raises.
pub fn get_float(obj: &Value, key: &str, default: f64) -> Option<f64> {
    match obj.get(key) {
        None => Some(default),
        Some(v) => py_float(v),
    }
}

/// `value.get(key, default)` then `int()`.
pub fn get_int(obj: &Value, key: &str, default: i64) -> Option<i64> {
    match obj.get(key) {
        None => Some(default),
        Some(v) => py_int(v),
    }
}

/// `str(value.get(key, default))`.
pub fn get_str(obj: &Value, key: &str, default: &str) -> String {
    match obj.get(key) {
        None => default.to_string(),
        Some(v) => py_str(v),
    }
}

/// A JSON number for a Python float.
pub fn num(x: f64) -> Value {
    Value::from(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_matches_known_cpython_values() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(-1e-320, 0.0), 1e-320);
    }

    #[test]
    fn python_rounding() {
        assert_eq!(round_n(0.125, 2), 0.12);
        assert_eq!(round_n(2.675, 2), 2.67);
        assert_eq!(round_n(1.25, 1), 1.2);
        assert_eq!(fmt0(2.5), "2");
        assert_eq!(fmt0(3.5), "4");
    }

    #[test]
    fn python_values() {
        assert_eq!(py_float(&Value::from("12")), Some(12.0));
        assert_eq!(py_int(&Value::from(3.7)), Some(3));
        assert_eq!(py_int(&Value::from("3.5")), None);
        assert_eq!(py_str(&Value::from(1.0)), "1.0");
        assert_eq!(py_str(&Value::Null), "None");
        assert!(!truthy(&Value::from(0)));
    }
}
