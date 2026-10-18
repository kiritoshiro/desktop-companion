//! Python value semantics for hand-edited JSON: `int(x)`, `float(x)`,
//! truthiness, `max(low, min(high, x))`, and `json.dumps(..., indent=2)`
//! output (ASCII-escaped, like Python's default `ensure_ascii=True`).
//!
//! Not a port of one module: the helpers every loader in `app/` relies on.

use serde_json::Value;
use std::io;
use std::path::Path;

/// Python's `int(value)`: ints, floats (truncated), bools and integer
/// strings; None where Python raises `TypeError`/`ValueError`.
pub fn py_int(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else if let Some(u) = n.as_u64() {
                Some(i64::try_from(u).unwrap_or(i64::MAX))
            } else {
                let f = n.as_f64()?;
                if f.is_finite() { Some(f.trunc() as i64) } else { None }
            }
        }
        Value::String(s) => parse_py_int(s),
        _ => None,
    }
}

fn parse_py_int(text: &str) -> Option<i64> {
    let t = text.trim();
    let cleaned: String = t.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() || t.starts_with('_') || t.ends_with('_') || t.contains("__") {
        return None;
    }
    cleaned.parse::<i64>().ok()
}

/// Python's `float(value)`.
pub fn py_float(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim().to_ascii_lowercase();
            match t.as_str() {
                "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
                "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
                "nan" | "+nan" | "-nan" => Some(f64::NAN),
                _ => t.replace('_', "").parse::<f64>().ok().filter(|_| !t.is_empty()),
            }
        }
        _ => None,
    }
}

/// Python's `bool(value)`.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `bool(d.get(key, default))`.
pub fn get_bool(map: &serde_json::Map<String, Value>, key: &str, default: bool) -> bool {
    map.get(key).map_or(default, truthy)
}

/// Python's `max(low, min(high, value))` for floats, NaN included.
pub fn clamp_f(value: f64, low: f64, high: f64) -> f64 {
    let m = if value < high { value } else { high };
    if m > low { m } else { low }
}

/// `max(low, min(high, int(value)))`, `low` when `int()` fails (the
/// `_int` helper of custom_maps).
pub fn clamped_int(value: &Value, low: i64, high: i64) -> i64 {
    py_int(value).map_or(low, |v| v.min(high).max(low))
}

/// `max(low, min(high, float(value)))`, `low` when `float()` fails.
pub fn clamped_float(value: &Value, low: f64, high: f64) -> f64 {
    py_float(value).map_or(low, |v| clamp_f(v, low, high))
}

/// Python's `str(value)` for a JSON value (what `str(raw.get(k))` gives).
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => {
            if n.is_f64() {
                py_float_repr(n.as_f64().unwrap_or(0.0))
            } else {
                n.to_string()
            }
        }
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Python's `repr(float)` for the common cases.
pub fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if f == f.trunc() && f.abs() < 1e16 {
        return format!("{f:.1}");
    }
    format!("{f}")
}

/// `str(value or default)`: the default when the value is falsy.
pub fn str_or(value: Option<&Value>, default: &str) -> String {
    match value {
        Some(v) if truthy(v) => py_str(v),
        _ => default.to_string(),
    }
}

/// Python's `" ".join(text.split())`.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first `n` characters (Python's `text[:n]`).
pub fn take_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// `json.dumps(value, indent=2)`: two-space indent, `": "` and `","`
/// separators, non-ASCII escaped as `\uXXXX`.
pub fn dumps_indent2(value: &Value) -> String {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into());
    escape_non_ascii(&pretty)
}

fn escape_non_ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            // Only string contents can hold non-ASCII in serde_json output.
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// Write `text` to `<path>.tmp` and rename it over `path` (Python's
/// `temp.write_text(...); temp.replace(path)`), creating the folder first.
pub fn atomic_write(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// Read and parse a JSON file; None for a missing, unreadable or broken one.
pub fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text);
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn int_and_float_follow_python() {
        assert_eq!(py_int(&json!(3.9)), Some(3));
        assert_eq!(py_int(&json!(-3.9)), Some(-3));
        assert_eq!(py_int(&json!(" 12 ")), Some(12));
        assert_eq!(py_int(&json!("1.5")), None);
        assert_eq!(py_int(&json!(true)), Some(1));
        assert_eq!(py_int(&json!(null)), None);
        assert_eq!(py_float(&json!("2.5")), Some(2.5));
        assert_eq!(py_float(&json!("x")), None);
        assert_eq!(clamp_f(f64::NAN, 0.0, 1.0), 1.0);
        assert_eq!(clamped_int(&json!("x"), 4, 30), 4);
    }

    #[test]
    fn dumps_escapes_like_python() {
        assert_eq!(dumps_indent2(&json!({"a": "é", "b": []})), "{\n  \"a\": \"\\u00e9\",\n  \"b\": []\n}");
    }
}
