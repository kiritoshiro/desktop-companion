//! Port of `content/preset_io.py`: validate, load and save presets.
//!
//! Validation runs on raw JSON so every message matches the Python. Saving
//! goes to the user's own preset folder and refuses a path inside a shipped
//! preset folder, so a preset called `Default` can never overwrite the
//! shipped `default.json` on a case-insensitive filesystem.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ContentError;
use crate::discovery::{is_shipped_preset, user_presets_dir};
use crate::jobs::job_by_id;
use crate::preset::{LABEL_SWITCH_DEFAULTS, Preset};
use crate::skills::skill_by_id;
use crate::util::{py_float, py_int, py_round_n, py_str, write_json_atomic};

/// `parse_color` from `state/teams.py` (needed here to validate a preset's
/// team colours; `dc_state::teams::parse_color` is this function).
/// Accepts `#rrggbb`, `rrggbb`, `#rgb`, or a sequence of three numbers.
pub fn parse_color(value: Option<&Value>) -> Option<(u8, u8, u8)> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    if let Value::Array(parts) = value
        && parts.len() >= 3
    {
        let mut out = [0u8; 3];
        for (slot, part) in out.iter_mut().zip(parts) {
            let f = py_float(part)?;
            if !f.is_finite() {
                return None;
            }
            *slot = py_round_n(f, 0).clamp(0.0, 255.0) as u8;
        }
        return Some((out[0], out[1], out[2]));
    }
    let text = py_str(value);
    let text = text.trim().trim_start_matches('#');
    let chars: Vec<char> = text.chars().collect();
    let chars: Vec<char> = if chars.len() == 3 { chars.iter().flat_map(|c| [*c, *c]).collect() } else { chars };
    if chars.len() != 6 {
        return None;
    }
    let pair = |i: usize| -> Option<u8> {
        let s: String = chars[i..i + 2].iter().collect();
        if s.starts_with('-') {
            return None;
        }
        u8::from_str_radix(&s, 16).ok()
    };
    Some((pair(0)?, pair(2)?, pair(4)?))
}

fn validate_rgb_overrides(value: &Value, label: &str) -> Result<(), String> {
    let Some(map) = value.as_object() else {
        return Err(format!("{label} must be an object"));
    };
    for (key, rgb) in map {
        if key.trim().is_empty() {
            return Err(format!("{label} keys must be non-empty strings"));
        }
        let Some(channels) = rgb.as_array().filter(|a| a.len() == 3) else {
            return Err(format!("{label}.{key} must be an RGB triplet"));
        };
        for channel in channels {
            let ok = !channel.is_boolean() && py_float(channel).is_some_and(|f| (0.0..=255.0).contains(&f));
            if !ok {
                return Err(format!("{label}.{key} channels must be numbers from 0 to 255"));
            }
        }
    }
    Ok(())
}

/// `safe_preset_filename`: `[^A-Za-z0-9_. -]+` -> `_`, trimmed of ` ._`.
pub fn safe_preset_filename(name: &str) -> String {
    let mut clean = String::new();
    let mut in_bad = false;
    for c in name.trim().chars() {
        let ok = c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ' ' | '-');
        if ok {
            clean.push(c);
            in_bad = false;
        } else if !in_bad {
            clean.push('_');
            in_bad = true;
        }
    }
    let clean = clean.trim_matches(|c| c == ' ' || c == '.' || c == '_');
    let clean = if clean.is_empty() { "preset" } else { clean };
    format!("{clean}.json")
}

fn is_nonempty_str(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty())
}

/// `validate_preset(data)`: `Err(message)` exactly as Python raises it.
pub fn validate_preset(data: &Value) -> Result<(), String> {
    let Some(obj) = data.as_object() else {
        return Err("Preset must be a JSON object".into());
    };
    if !is_nonempty_str(obj.get("name")) {
        return Err("Preset requires a non-empty name".into());
    }
    let Some(slots) = obj.get("slots").and_then(Value::as_array) else {
        return Err("Preset requires a slots list".into());
    };

    let empty = Value::Object(Default::default());
    let settings = obj.get("settings").unwrap_or(&empty);
    if !settings.is_null() {
        let Some(settings) = settings.as_object() else {
            return Err("Preset settings must be an object".into());
        };
        if let Some(v) = settings.get("size_scale") {
            let Some(size) = py_float(v) else {
                return Err("Preset settings.size_scale must be a number".into());
            };
            if size < 0.45 || size > 2.25 {
                return Err("Preset settings.size_scale must be between 0.45 and 2.25".into());
            }
        }
        for key in ["interferable", "social_play"] {
            if settings.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err(format!("Preset settings.{key} must be true or false"));
            }
        }
        for (switch, _) in LABEL_SWITCH_DEFAULTS {
            if settings.get(switch).is_some_and(|v| !v.is_boolean()) {
                return Err(format!("Preset settings.{switch} must be true or false"));
            }
        }
        if let Some(mood) = settings.get("mood_mode") {
            let ok = mood
                .as_str()
                .is_some_and(|m| ["auto", "playful", "cuddly", "curious", "calm"].contains(&m.to_lowercase().as_str()));
            if !ok {
                return Err("Preset settings.mood_mode must be auto, playful, cuddly, curious, or calm".into());
            }
        }
        if let Some(teams) = settings.get("teams").filter(|v| !v.is_null()) {
            let Some(teams) = teams.as_object() else {
                return Err("Preset settings.teams must be an object".into());
            };
            for (team_id, entry) in teams {
                if team_id.trim().is_empty() {
                    return Err("Preset settings.teams keys must be team ids".into());
                }
                if entry.is_string() {
                    continue;
                }
                let Some(entry) = entry.as_object() else {
                    return Err(format!("Preset settings.teams.{team_id} must be an object or a name"));
                };
                if entry.get("name").is_some_and(|n| !n.is_string()) {
                    return Err(format!("Preset settings.teams.{team_id}.name must be text"));
                }
                if let Some(color) = entry.get("color").filter(|c| !c.is_null())
                    && parse_color(Some(color)).is_none()
                {
                    return Err(format!("Preset settings.teams.{team_id}.color must be #rrggbb or [r, g, b]"));
                }
            }
        }
        if let Some(relations) = settings.get("team_relations").filter(|v| !v.is_null()) {
            let Some(relations) = relations.as_object() else {
                return Err("Preset settings.team_relations must be an object".into());
            };
            for (left, row) in relations {
                if left.trim().is_empty() {
                    return Err("Preset settings.team_relations keys must be team names".into());
                }
                let Some(row) = row.as_object() else {
                    return Err(format!("Preset settings.team_relations.{left} must be an object"));
                };
                for (right, relation) in row {
                    if right.trim().is_empty() {
                        return Err(format!("Preset settings.team_relations.{left} keys must be team names"));
                    }
                    let ok = relation
                        .as_str()
                        .is_some_and(|r| ["friend", "neutral", "foe"].contains(&r.trim().to_lowercase().as_str()));
                    if !ok {
                        return Err(format!(
                            "Preset settings.team_relations.{left}.{right} must be friend, neutral, or foe"
                        ));
                    }
                }
            }
        }
        if let Some(flies) = settings.get("flies").filter(|v| !v.is_null()) {
            let Some(flies) = flies.as_object() else {
                return Err("Preset settings.flies must be an object".into());
            };
            if flies.get("enabled").is_some_and(|v| !v.is_boolean()) {
                return Err("Preset settings.flies.enabled must be true or false".into());
            }
            for key in ["min_interval", "max_interval"] {
                if let Some(v) = flies.get(key) {
                    let Some(value) = py_float(v) else {
                        return Err(format!("Preset settings.flies.{key} must be a number"));
                    };
                    if value <= 0.0 || value > 600.0 {
                        return Err(format!("Preset settings.flies.{key} must be between 0 and 600 seconds"));
                    }
                }
            }
            if let Some(v) = flies.get("max_flies") {
                let Some(max_flies) = py_int(v) else {
                    return Err("Preset settings.flies.max_flies must be an integer".into());
                };
                if !(0..=40).contains(&max_flies) {
                    return Err("Preset settings.flies.max_flies must be between 0 and 40".into());
                }
            }
            if flies.get("spawner").is_some_and(|v| !v.is_boolean()) {
                return Err("Preset settings.flies.spawner must be true or false".into());
            }
        }
    }

    for (index, slot) in slots.iter().enumerate() {
        let n = index + 1;
        let Some(slot) = slot.as_object() else {
            return Err(format!("Slot {n} must be an object"));
        };
        for key in ["model", "personality", "count"] {
            if !slot.contains_key(key) {
                return Err(format!("Slot {n} missing {key}"));
            }
        }
        if !is_nonempty_str(slot.get("model")) {
            return Err(format!("Slot {n} model must be a non-empty string"));
        }
        if !is_nonempty_str(slot.get("personality")) {
            return Err(format!("Slot {n} personality must be a non-empty string"));
        }
        if slot.contains_key("slot_id") && !is_nonempty_str(slot.get("slot_id")) {
            return Err(format!("Slot {n} slot_id must be a non-empty string"));
        }
        for team_key in ["team", "team_id"] {
            if slot.contains_key(team_key) && !is_nonempty_str(slot.get(team_key)) {
                return Err(format!("Slot {n} {team_key} must be a non-empty string"));
            }
        }
        if let Some(job) = slot.get("job") {
            if !is_nonempty_str(Some(job)) {
                return Err(format!("Slot {n} job must be a non-empty string"));
            }
            let job = job.as_str().unwrap();
            if job_by_id(&job.trim().to_lowercase()).is_none() {
                return Err(format!("Slot {n} has unknown job: {job}"));
            }
        }
        if slot.get("count_random").is_some_and(|v| !v.is_boolean()) {
            return Err(format!("Slot {n} count_random must be true or false"));
        }
        if let Some(skills) = slot.get("skills") {
            let Some(skills) = skills.as_array() else {
                return Err(format!("Slot {n} skills must be a list"));
            };
            for skill in skills {
                let Some(skill) = skill.as_str() else {
                    return Err(format!("Slot {n} skills must contain only strings"));
                };
                if skill_by_id(&skill.trim().to_lowercase()).is_none() {
                    return Err(format!("Slot {n} has unknown skill: {skill}"));
                }
            }
        }
        if let Some(abilities) = slot.get("abilities") {
            let Some(abilities) = abilities.as_array() else {
                return Err(format!("Slot {n} abilities must be a list"));
            };
            for ability in abilities {
                let Some(ability) = ability.as_str() else {
                    return Err(format!("Slot {n} abilities must contain only strings"));
                };
                let Some(skill) = skill_by_id(&ability.trim().to_lowercase()) else {
                    return Err(format!("Slot {n} has unknown ability: {ability}"));
                };
                if !skill.is_ability() {
                    return Err(format!("Slot {n} entry is not an ability: {ability}"));
                }
            }
        }
        if let Some(colors) = slot.get("colors") {
            validate_rgb_overrides(colors, &format!("Slot {n} colors"))?;
        }
        let Some(count) = slot.get("count").and_then(py_int) else {
            return Err(format!("Slot {n} count must be an integer"));
        };
        if !(1..=50).contains(&count) {
            return Err(format!("Slot {n} count must be between 1 and 50"));
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, ContentError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| ContentError::Io { path: path.display().to_string(), source })?;
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|source| ContentError::Json { path: path.display().to_string(), source })
}

/// `load_preset` as raw JSON (validated).
pub fn load_preset_value(path: &Path) -> Result<Value, ContentError> {
    let data = read_json(path)?;
    validate_preset(&data).map_err(ContentError::Invalid)?;
    Ok(data)
}

/// `load_preset`: read, validate and type a preset.
pub fn load_preset(path: &Path) -> Result<Preset, ContentError> {
    let data = load_preset_value(path)?;
    Preset::from_value(data).map_err(|source| ContentError::Json { path: path.display().to_string(), source })
}

/// `save_preset(data, path=None)` on raw JSON: validate, then write to the
/// user's preset folder (a relative `path` is taken inside it). Refuses a
/// shipped preset. Written like `json.dump(indent=2)` plus a newline.
pub fn save_preset_value(data: &Value, path: Option<&Path>) -> Result<PathBuf, ContentError> {
    validate_preset(data).map_err(ContentError::Invalid)?;
    let presets_dir = user_presets_dir();
    let io = |p: &Path, source| ContentError::Io { path: p.display().to_string(), source };
    std::fs::create_dir_all(&presets_dir).map_err(|e| io(&presets_dir, e))?;
    let name = data["name"].as_str().unwrap_or("");
    let mut path = match path {
        Some(p) => p.to_path_buf(),
        None => presets_dir.join(safe_preset_filename(name)),
    };
    if !path.is_absolute() {
        path = presets_dir.join(path);
    }
    if is_shipped_preset(&path) {
        let file = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(ContentError::Invalid(format!(
            "{file} ships with the application and is not writable. Saved presets go to your own preset folder instead."
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
    }
    let mut text = crate::util::python_json_dumps(data, Some(2));
    text.push('\n');
    std::fs::write(&path, text).map_err(|e| io(&path, e))?;
    Ok(path)
}

/// `save_preset` for a typed preset.
pub fn save_preset(preset: &Preset, path: Option<&Path>) -> Result<PathBuf, ContentError> {
    save_preset_value(&preset.to_value(), path)
}

/// Atomic JSON write helper re-exported for callers that persist presets
/// elsewhere (the Python writes presets directly; state files atomically).
pub fn write_json_file_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    write_json_atomic(path, value)
}
