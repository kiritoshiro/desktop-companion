//! Python-semantics helpers shared by the content and state ports.
//!
//! The Python code reads JSON as plain dicts and coerces values on the way
//! (`float(x)`, `int(x)`, `str(x).strip().lower()`, truthiness). These helpers
//! reproduce those coercions on `serde_json::Value`, and `OrderedMap` keeps a
//! JSON object's key order the way a Python dict does.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub type JsonMap = serde_json::Map<String, Value>;

/// Python `repr(float)`: shortest round-trip digits, always with a `.0` or an
/// exponent, scientific outside `1e-4 <= |x| < 1e16`.
pub fn py_float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let ax = x.abs();
    if !(1e-4..1e16).contains(&ax) {
        // Rust: "1e-5" / "1.5e16"; Python: "1e-05" / "1.5e+16".
        let s = format!("{x:e}");
        let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let exp: i32 = exp.parse().unwrap_or(0);
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mant}e{sign}{:02}", exp.abs());
    }
    let s = format!("{x}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

/// Python `str(value)` for a JSON value.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                py_float_repr(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `str(value).strip().lower()`.
pub fn py_key(v: &Value) -> String {
    py_str(v).trim().to_lowercase()
}

/// `str(value or "").strip().lower()`.
pub fn py_key_or_empty(v: Option<&Value>) -> String {
    match v {
        Some(v) if py_truthy(v) => py_key(v),
        _ => String::new(),
    }
}

/// Python truthiness of a JSON value.
pub fn py_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python `float(value)`; `None` where Python raises.
pub fn py_float(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_py_float(s),
        _ => None,
    }
}

/// `float(text)` for a string (whitespace stripped, `inf`/`nan` accepted).
pub fn parse_py_float(s: &str) -> Option<f64> {
    let t = s.trim().replace('_', "");
    if t.is_empty() {
        return None;
    }
    match t.to_lowercase().as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => return Some(f64::INFINITY),
        "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => return Some(f64::NAN),
        _ => {}
    }
    t.parse::<f64>().ok()
}

/// Python `int(value)`; `None` where Python raises. Floats truncate, strings
/// must spell an integer.
pub fn py_int(v: &Value) -> Option<i64> {
    match v {
        Value::Bool(b) => Some(*b as i64),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else {
                let f = n.as_f64()?;
                if f.is_finite() { Some(f.trunc() as i64) } else { None }
            }
        }
        Value::String(s) => s.trim().replace('_', "").parse::<i64>().ok(),
        _ => None,
    }
}

/// `isinstance(value, int)` (bool counts, as in Python).
pub fn is_py_int(v: &Value) -> bool {
    matches!(v, Value::Bool(_)) || v.as_i64().is_some() || v.as_u64().is_some()
}

/// `isinstance(value, (int, float)) and not isinstance(value, bool)`.
pub fn is_number(v: &Value) -> bool {
    matches!(v, Value::Number(_))
}

/// Python `round(x, ndigits)` (correctly rounded, ties to even).
pub fn py_round_n(x: f64, ndigits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.ndigits$}").parse().unwrap_or(x)
}

/// Python `str.title()` for ASCII-ish text.
pub fn py_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_cased = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if prev_cased {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            prev_cased = true;
        } else {
            out.push(c);
            prev_cased = false;
        }
    }
    out
}

/// The first `n` characters (Python `text[:n]`).
pub fn py_prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Python `str.casefold()` approximation (full Unicode lowercase, `ß` -> `ss`).
pub fn casefold(s: &str) -> String {
    s.to_lowercase().replace('ß', "ss")
}

/// JSON equality that treats `1` and `1.0` as equal, the way Python's `==`
/// does once a typed Rust struct has turned every int into an `f64`.
pub fn json_eq_numeric(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => x == y || (x - y).abs() <= 1e-12 * x.abs().max(y.abs()),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq_numeric(a, b)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_eq_numeric(v, w)))
        }
        _ => a == b,
    }
}

/// The first path where two JSON values differ under [`json_eq_numeric`].
pub fn json_first_difference(a: &Value, b: &Value) -> Option<String> {
    fn walk(a: &Value, b: &Value, path: &str) -> Option<String> {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for (k, v) in x {
                    match y.get(k) {
                        None => return Some(format!("{path}.{k}: missing on the right")),
                        Some(w) => {
                            if let Some(d) = walk(v, w, &format!("{path}.{k}")) {
                                return Some(d);
                            }
                        }
                    }
                }
                for k in y.keys() {
                    if !x.contains_key(k) {
                        return Some(format!("{path}.{k}: missing on the left"));
                    }
                }
                None
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return Some(format!("{path}: length {} != {}", x.len(), y.len()));
                }
                x.iter().zip(y).enumerate().find_map(|(i, (v, w))| walk(v, w, &format!("{path}[{i}]")))
            }
            _ if json_eq_numeric(a, b) => None,
            _ => Some(format!("{path}: {a} != {b}")),
        }
    }
    walk(a, b, "")
}

/// Python `repr()` for the values validation messages quote (`{plan!r}`).
pub fn py_repr(v: &Value) -> String {
    match v {
        Value::String(s) => {
            if s.contains('\'') && !s.contains('"') {
                format!("\"{s}\"")
            } else {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
            }
        }
        Value::Array(items) => format!("[{}]", items.iter().map(py_repr).collect::<Vec<_>>().join(", ")),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}: {}", py_repr(&Value::String(k.clone())), py_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => py_str(other),
    }
}

/// `json.dumps(value, indent=indent)` with Python's defaults
/// (`ensure_ascii=True`, `", "`/`": "` separators, float `repr`), so a file
/// this port writes is byte-identical to the one Python would.
pub fn python_json_dumps(value: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_json(&mut out, value, indent, 0);
    out
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

fn write_json(out: &mut String, value: &Value, indent: Option<usize>, level: usize) {
    let newline = |out: &mut String, level: usize| {
        if let Some(n) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(n * level));
        }
    };
    let item_sep = if indent.is_some() { "," } else { ", " };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                out.push_str(&n.to_string());
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.is_nan() {
                    out.push_str("NaN");
                } else if f.is_infinite() {
                    out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" });
                } else {
                    out.push_str(&py_float_repr(f));
                }
            }
        }
        Value::String(s) => write_json_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, level + 1);
                write_json(out, item, indent, level + 1);
            }
            newline(out, level);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, level + 1);
                write_json_string(out, k);
                out.push_str(": ");
                write_json(out, v, indent, level + 1);
            }
            newline(out, level);
            out.push('}');
        }
    }
}

/// Write `json.dump(value, handle, indent=2); handle.write("\n")` atomically:
/// to `<path>.tmp` (the suffix replaced, like `Path.with_suffix(".tmp")`)
/// and then renamed over `path`.
pub fn write_json_atomic(path: &std::path::Path, value: &Value) -> std::io::Result<()> {
    let mut text = python_json_dumps(value, Some(2));
    text.push('\n');
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text.as_bytes())?;
    std::fs::rename(&temp, path)
}

// ---------------------------------------------------------------------------
// Lenient deserializers: Python reads these keys with int()/float(), so a
// value written as 4.0 or "4" still works there and must load here.
// ---------------------------------------------------------------------------

pub mod de {
    use super::*;

    pub fn opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        let v = Option::<Value>::deserialize(d)?;
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                py_int(&v).map(Some).ok_or_else(|| serde::de::Error::custom(format!("expected an integer, got {v}")))
            }
        }
    }

    pub fn i64_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        let v = Value::deserialize(d)?;
        py_int(&v).ok_or_else(|| serde::de::Error::custom(format!("expected an integer, got {v}")))
    }

    pub fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        let v = Option::<Value>::deserialize(d)?;
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                py_float(&v).map(Some).ok_or_else(|| serde::de::Error::custom(format!("expected a number, got {v}")))
            }
        }
    }

    pub fn f64_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let v = Value::deserialize(d)?;
        py_float(&v).ok_or_else(|| serde::de::Error::custom(format!("expected a number, got {v}")))
    }

    /// A list whose items are coerced with `float()`.
    pub fn opt_vec_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<f64>>, D::Error> {
        let v = Option::<Value>::deserialize(d)?;
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Array(items)) => items
                .iter()
                .map(|x| py_float(x).ok_or_else(|| serde::de::Error::custom(format!("expected a number, got {x}"))))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(v) => Err(serde::de::Error::custom(format!("expected a list of numbers, got {v}"))),
        }
    }

    /// A list whose items are coerced with `str()`.
    pub fn opt_vec_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
        let v = Option::<Value>::deserialize(d)?;
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Array(items)) => Ok(Some(items.iter().map(py_str).collect())),
            Some(v) => Err(serde::de::Error::custom(format!("expected a list of strings, got {v}"))),
        }
    }

    pub fn opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
        let v = Option::<Value>::deserialize(d)?;
        Ok(match v {
            None | Some(Value::Null) => None,
            Some(v) => Some(py_truthy(&v)),
        })
    }
}

// ---------------------------------------------------------------------------
// OrderedMap: a JSON object with Python dict ordering.
// ---------------------------------------------------------------------------

/// An insertion-ordered string-keyed map that (de)serialises as a JSON object.
///
/// Python dicts keep insertion order and several outputs depend on it
/// (`tuple(composed["phase_scores"]) == BEHAVIOUR_PHASE_IDS`), so a `BTreeMap`
/// would not do. Re-inserting an existing key keeps its position.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, key: &str) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    /// Insert or replace (in place); returns the previous value.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        if let Some(slot) = self.get_mut(&key) {
            return Some(std::mem::replace(slot, value));
        }
        self.entries.push((key, value));
        None
    }
    /// `dict.setdefault(key, value)`.
    pub fn entry_or_insert_with(&mut self, key: &str, make: impl FnOnce() -> V) -> &mut V {
        if let Some(i) = self.entries.iter().position(|(k, _)| k == key) {
            return &mut self.entries[i].1;
        }
        self.entries.push((key.to_string(), make()));
        &mut self.entries.last_mut().unwrap().1
    }
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(i).1)
    }
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut V)> {
        self.entries.iter_mut().map(|(k, v)| (k.as_str(), v))
    }
    pub fn into_entries(self) -> Vec<(String, V)> {
        self.entries
    }
    /// Sort by key (Python `sorted(d.items())` then rebuild).
    pub fn sort_keys(&mut self) {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

impl<V> FromIterator<(String, V)> for OrderedMap<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut m = OrderedMap::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<'a, V> IntoIterator for &'a OrderedMap<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (String, V)>, fn(&'a (String, V)) -> (&'a String, &'a V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

impl<V: Serialize> Serialize for OrderedMap<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.entries.len()))?;
        for (k, v) in &self.entries {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for OrderedMap<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V2<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for V2<V> {
            type Value = OrderedMap<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut out = OrderedMap::new();
                while let Some((k, v)) = access.next_entry::<String, V>()? {
                    out.insert(k, v);
                }
                Ok(out)
            }
        }
        d.deserialize_map(V2(PhantomData))
    }
}

/// Model and slot palettes: colour key -> RGB(A), in file order.
pub type ColorMap = OrderedMap<dc_core::Color>;

/// Generates a struct whose every field is optional on disk and omitted when
/// absent, plus a flattened `extra` map so unknown keys round-trip.
#[macro_export]
macro_rules! optional_fields {
    (
        $(#[$m:meta])*
        pub struct $name:ident {
            $( $(#[$fm:meta])* $field:ident : $ty:ty ),* $(,)?
        }
    ) => {
        $(#[$m])*
        #[derive(Clone, Debug, Default, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name {
            $(
                $(#[$fm])*
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )*
            /// Keys this port does not know, kept for a lossless round trip.
            #[serde(flatten)]
            pub extra: ::serde_json::Map<String, ::serde_json::Value>,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn python_reprs() {
        assert_eq!(py_float_repr(3.0), "3.0");
        assert_eq!(py_float_repr(0.1), "0.1");
        assert_eq!(py_float_repr(1e-5), "1e-05");
        assert_eq!(py_float_repr(1.5e16), "1.5e+16");
        assert_eq!(py_str(&json!(7)), "7");
        assert_eq!(py_str(&json!(true)), "True");
        assert_eq!(py_str(&Value::Null), "None");
    }

    #[test]
    fn python_rounding_to_digits() {
        assert_eq!(py_round_n(0.125, 2), 0.12);
        assert_eq!(py_round_n(2.675, 2), 2.67);
        assert_eq!(py_round_n(4.4499999, 2), 4.45);
    }

    #[test]
    fn title_case() {
        assert_eq!(py_title("attic watch"), "Attic Watch");
        assert_eq!(py_title("x2b team"), "X2B Team");
    }

    #[test]
    fn ordered_map_keeps_order() {
        let m: OrderedMap<i64> = serde_json::from_str(r#"{"b":1,"a":2,"b":3}"#).unwrap();
        assert_eq!(m.keys().collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(m.get("b"), Some(&3));
        assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"b":3,"a":2}"#);
    }
}
