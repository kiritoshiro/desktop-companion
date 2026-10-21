//! CPython numeric semantics the kinematics depend on bit for bit.
//!
//! `math.hypot` in CPython 3.8+ is not the C library's `hypot`: it is
//! `vector_norm` in `Modules/mathmodule.c`, a scaled, compensated sum with a
//! final differential correction. It is correctly rounded in almost every
//! case, and it is *not* what `f64::hypot` (the CRT `_hypot`) returns in the
//! last bit. The gait makes threshold decisions on these distances every
//! frame, so a one-ulp difference eventually picks a different leg to lift.
//! [`hypot`] reproduces `vector_norm` for two arguments exactly; it was
//! checked against CPython 3.13 on two million random pairs (see
//! `tests/pymath_parity.rs`).
//!
//! `sum()` of floats in CPython 3.12+ is Neumaier-compensated, not a naive
//! left fold; [`py_sum`] mirrors it.
//!
//! The JSON helpers mirror `float(x)`, `int(x)`, `bool(x)` and `str(x)` on
//! values that came out of a model or personality file, so the config
//! builders accept exactly what the Python accepted.
//!
//! These belong in `dc-core` eventually (see `PORT_STATUS.md`); they live
//! here because this crate may not edit `dc-core` during the parallel port.

use dc_core::math::py_mod;
use serde_json::Value;
use std::f64::consts::{PI, TAU};

/// `((a + math.pi) % math.tau) - math.pi`: the wrapped angle difference used
/// throughout the gait. Equal to Python's expression bit for bit.
#[inline]
pub fn wrap_pi(a: f64) -> f64 {
    py_mod(a + PI, TAU) - PI
}

/// Veltkamp split for Dekker's exact product.
#[inline(always)]
fn split(a: f64) -> (f64, f64) {
    const FACTOR: f64 = 134_217_729.0; // 2^27 + 1
    let c = FACTOR * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

/// `dl_mul`: the exact product `x * y` as an unevaluated sum `(hi, lo)`.
/// CPython uses `fma(x, y, -z)` for `lo`; for operands that cannot overflow
/// or underflow (they are scaled into `[0, 1)` first) Dekker's algorithm
/// yields the same, unique, exact error term without a libm call.
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

/// `frexp(x).1` for a finite, positive `x`.
#[inline(always)]
fn frexp_exp(x: f64) -> i32 {
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        // Subnormal: normalise by 2^54 first.
        let y = x * f64::from_bits(0x4350_0000_0000_0000); // 2^54
        let b2 = ((y.to_bits() >> 52) & 0x7ff) as i32;
        b2 - 1022 - 54
    } else {
        biased - 1022
    }
}

/// `ldexp(1.0, e)` for `e` in `[-1074, 1023]`.
#[inline(always)]
fn pow2(e: i32) -> f64 {
    if e >= -1022 { f64::from_bits(((e + 1023) as u64) << 52) } else { f64::from_bits(1u64 << (e + 1074)) }
}

/// CPython's `math.hypot(x, y)` (`vector_norm` with `n == 2`).
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
    if max.is_infinite() {
        return max;
    }
    if max == 0.0 {
        return max;
    }
    let max_e = frexp_exp(max);
    if max_e < -1023 {
        // Subnormal inputs: convert to normals and recurse, as CPython does.
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

/// CPython 3.12+ `sum()` over floats: Neumaier compensated summation
/// starting from the first element (`0 + x0 == x0`).
pub fn py_sum<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else {
        return 0.0;
    };
    // `0 + first` (an int start plus a float) is exactly `first`, except
    // that `0 + -0.0` is `+0.0`.
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

/// `pow(x, y)` through the C library, never strength-reduced by the
/// optimiser (LLVM rewrites `powf(x, 2.0)` as `x * x`, which is not what
/// CPython's `x ** 2` computes).
#[inline]
pub fn py_pow(x: f64, y: f64) -> f64 {
    x.powf(std::hint::black_box(y))
}

/// `float(value)`: numbers, booleans and numeric strings.
pub fn py_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => parse_py_float_str(s),
        _ => None,
    }
}

fn parse_py_float_str(s: &str) -> Option<f64> {
    let t = s.trim().replace('_', "");
    let lower = t.to_ascii_lowercase();
    match lower.trim_start_matches(['+', '-']) {
        "inf" | "infinity" => {
            return Some(if lower.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY });
        }
        "nan" => return Some(f64::NAN),
        _ => {}
    }
    t.parse::<f64>().ok()
}

/// `float(d.get(key, default))`; `None` where Python would raise.
pub fn get_float(obj: &Value, key: &str, default: f64) -> Option<f64> {
    match obj.get(key) {
        None => Some(default),
        Some(v) => py_float(v),
    }
}

/// `int(value)`: truncates floats, parses integer strings.
pub fn py_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else {
                n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)
            }
        }
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(s) => s.trim().replace('_', "").parse::<i64>().ok(),
        _ => None,
    }
}

/// `bool(value)` (Python truthiness).
pub fn py_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(value)` for scalar JSON values.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// `[float(v) for v in value]`; `None` where Python would raise (not a list,
/// or a non-numeric element).
pub fn py_float_list(value: &Value) -> Option<Vec<f64>> {
    match value {
        Value::Array(items) => items.iter().map(py_float).collect(),
        Value::String(s) => s.chars().map(|c| parse_py_float_str(&c.to_string())).collect(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_basics() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(-2.0, 0.0), 2.0);
        assert_eq!(hypot(f64::INFINITY, f64::NAN), f64::INFINITY);
        assert!(hypot(f64::NAN, 1.0).is_nan());
        let tiny = f64::from_bits(5);
        assert!(hypot(tiny, tiny) > 0.0);
    }

    #[test]
    fn neumaier_sum() {
        assert_eq!(py_sum([0.1, 0.2, 0.3]), 0.6);
        assert_eq!(py_sum([1e100, 1.0, -1e100]), 1.0);
        assert_eq!(py_sum(std::iter::empty()), 0.0);
    }

    #[test]
    fn json_conversions() {
        assert_eq!(py_float(&Value::from("35")), Some(35.0));
        assert_eq!(py_float(&Value::from(true)), Some(1.0));
        assert_eq!(py_int(&Value::from(4.9)), Some(4));
        assert_eq!(py_int(&Value::from("4.0")), None);
        assert!(py_truthy(&Value::from(1)));
        assert!(!py_truthy(&Value::from("")));
        assert!((wrap_pi(0.5 + TAU) - 0.5).abs() < 1e-12);
        assert!((wrap_pi(-0.5 - TAU) + 0.5).abs() < 1e-12);
    }
}
