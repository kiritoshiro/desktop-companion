//! Teams a person can name, colour, and understand, as the settings window
//! and the inspector use them. Mirrors state/teams.py and the team helpers
//! of state/progression.py (`normalize_team_id`, `normalize_team_stances`,
//! `team_stance`).
//!
//! TODO(content): could delegate to `dc_state::teams` outright (it uses
//! `Color` and an ordered map where these use `Rgb` and `BTreeMap`); these
//! are the same rules, and `bridge`'s tests check they agree.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::model::Rgb;

pub const NEUTRAL: &str = "neutral";
pub const RELATIONS: [&str; 3] = ["friend", "neutral", "foe"];
pub const DEFAULT_HOSTILE_TEAMS: [&str; 1] = ["rivals"];

/// What hostility actually does today (`HOSTILITY_NOTE`).
pub const HOSTILITY_NOTE: &str = "Foes fight. Two hostile spiders that meet will attack, using whatever \
skills they have -- silk to pin, a pounce to close -- and a Guard still \
raises an alert and intercepts. A beaten spider dies and leaves a carcass \
that is eaten away; it does not come back. Turn Conflict off to go back to \
alerts without damage.";

/// `STANCE_LABELS`.
pub fn stance_label(relation: &str) -> &'static str {
    match relation {
        "friend" => "Allies",
        "foe" => "Foes",
        _ => "Ignore each other",
    }
}

/// `BUILT_IN_TEAMS`.
const BUILT_IN_TEAMS: [(&str, &str, Rgb); 4] = [
    ("pack_a", "Pack A", [79, 163, 209]),
    ("pack_b", "Pack B", [122, 196, 129]),
    ("hunters", "Hunters", [226, 166, 74]),
    ("rivals", "Rivals", [209, 83, 79]),
];

/// `MIN_COLOR_SEPARATION`.
pub const MIN_COLOR_SEPARATION: f64 = 60.0;

/// One team: its id, a name its owner chose, and a colour.
#[derive(Clone, Debug, PartialEq)]
pub struct TeamProfile {
    pub id: String,
    pub name: String,
    pub color: Rgb,
}

impl TeamProfile {
    pub fn hex_color(&self) -> String {
        color_to_hex(self.color)
    }
}

pub type TeamProfiles = BTreeMap<String, TeamProfile>;
/// `{team: {team: relation}}`, stored in both directions.
pub type Stances = BTreeMap<String, BTreeMap<String, String>>;

/// `normalize_team_id`: case-folded, whitespace to `_`, at most 32 chars.
pub fn normalize_team_id(value: &str) -> String {
    let text = value.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("_");
    let text: String = text.chars().take(32).collect();
    if text.is_empty() { NEUTRAL.into() } else { text }
}

fn fnv1a(text: &str) -> u32 {
    let mut digest: u32 = 2166136261;
    for byte in text.as_bytes() {
        digest = (digest ^ *byte as u32).wrapping_mul(16777619);
    }
    digest
}

fn color_at_hue(digest: u32, hue_offset: u32) -> Rgb {
    let hue = (((digest % 360) + hue_offset) % 360) as f64 / 360.0;
    let saturation = 0.46 + ((digest >> 9) % 5) as f64 * 0.06;
    let value = 0.78 + ((digest >> 17) % 4) as f64 * 0.05;
    let i = (hue * 6.0).floor();
    let f = hue * 6.0 - i;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * f);
    let t = value * (1.0 - saturation * (1.0 - f));
    let (r, g, b) = match (i as i64).rem_euclid(6) {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    let c = |x: f64| dc_core::math::py_round(x * 255.0) as u8;
    [c(r), c(g), c(b)]
}

fn distance(a: Rgb, b: Rgb) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum::<f64>().sqrt()
}

/// `default_color`: built-ins hand-picked, neutral white, anything else
/// from its id's hash.
pub fn default_color(team_id: &str) -> Rgb {
    let id = normalize_team_id(team_id);
    if let Some((_, _, c)) = BUILT_IN_TEAMS.iter().find(|(t, _, _)| *t == id) {
        return *c;
    }
    if id == NEUTRAL {
        return [255, 255, 255];
    }
    color_at_hue(fnv1a(&id), 0)
}

/// `distinct_color`: the team's own colour, moved aside when another team
/// already looks like it.
pub fn distinct_color(team_id: &str, taken: &[Rgb]) -> Rgb {
    let id = normalize_team_id(team_id);
    if let Some((_, _, c)) = BUILT_IN_TEAMS.iter().find(|(t, _, _)| *t == id) {
        return *c;
    }
    if id == NEUTRAL {
        return [255, 255, 255];
    }
    let digest = fnv1a(&id);
    for step in 0..8 {
        let candidate = color_at_hue(digest, step * 47);
        if taken.iter().all(|other| distance(candidate, *other) >= MIN_COLOR_SEPARATION) {
            return candidate;
        }
    }
    color_at_hue(digest, 0)
}

/// `default_name`.
pub fn default_name(team_id: &str) -> String {
    let id = normalize_team_id(team_id);
    if let Some((_, name, _)) = BUILT_IN_TEAMS.iter().find(|(t, _, _)| *t == id) {
        return (*name).into();
    }
    if id == NEUTRAL {
        return "Neutral / solo".into();
    }
    let spaced = id.replace(['_', '-'], " ");
    let titled = title_case(spaced.trim());
    if titled.is_empty() { "Team".into() } else { titled }
}

/// Python's `str.title()`.
pub fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            if start {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            start = false;
        } else {
            out.push(ch);
            start = true;
        }
    }
    out
}

/// `parse_color`: `#rrggbb`, `rrggbb`, `#rgb` or three numbers.
pub fn parse_color(value: &Value) -> Option<Rgb> {
    match value {
        Value::Array(parts) if parts.len() >= 3 => {
            let mut out = [0u8; 3];
            for (i, part) in parts.iter().take(3).enumerate() {
                out[i] = part.as_f64()?.round().clamp(0.0, 255.0) as u8;
            }
            Some(out)
        }
        Value::String(text) => {
            let mut t = text.trim().trim_start_matches('#').to_owned();
            if t.len() == 3 {
                t = t.chars().flat_map(|c| [c, c]).collect();
            }
            if t.len() != 6 {
                return None;
            }
            let p = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
            Some([p(0)?, p(2)?, p(4)?])
        }
        _ => None,
    }
}

pub fn color_to_hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

/// `normalize_team_name`: never empty, at most 32 characters.
pub fn normalize_team_name(value: &str, team_id: &str) -> String {
    let text = value.trim();
    if text.is_empty() { default_name(team_id) } else { text.chars().take(32).collect() }
}

/// `normalize_teams`: the team table from a preset's `teams` block, plus
/// every id a slot refers to.
pub fn normalize_teams(raw: Option<&Value>, used_ids: &[String]) -> TeamProfiles {
    let mut profiles = TeamProfiles::new();
    let mut taken: Vec<Rgb> = Vec::new();
    let mut pending: Vec<(String, String)> = Vec::new();
    if let Some(Value::Object(map)) = raw {
        for (key, value) in map {
            let id = normalize_team_id(key);
            if id == NEUTRAL {
                continue;
            }
            let (name, chosen) = match value {
                Value::Object(obj) => (
                    normalize_team_name(obj.get("name").and_then(Value::as_str).unwrap_or(""), &id),
                    obj.get("color").and_then(parse_color),
                ),
                Value::String(s) => (normalize_team_name(s, &id), None),
                other => (normalize_team_name(&other.to_string(), &id), None),
            };
            match chosen {
                Some(color) => {
                    taken.push(color);
                    profiles.insert(id.clone(), TeamProfile { id, name, color });
                }
                None => {
                    pending.push((id.clone(), name.clone()));
                    profiles.insert(id.clone(), TeamProfile { id, name, color: [0, 0, 0] });
                }
            }
        }
    }
    for value in used_ids {
        let id = normalize_team_id(value);
        if id == NEUTRAL || profiles.contains_key(&id) {
            continue;
        }
        let name = default_name(&id);
        pending.push((id.clone(), name.clone()));
        profiles.insert(id.clone(), TeamProfile { id, name, color: [0, 0, 0] });
    }
    pending.sort();
    for (id, name) in pending {
        let color = distinct_color(&id, &taken);
        taken.push(color);
        profiles.insert(id.clone(), TeamProfile { id, name, color });
    }
    profiles
}

/// `teams_payload`: the `teams` block to save.
pub fn teams_payload(profiles: &TeamProfiles) -> Value {
    let mut out = Map::new();
    for (id, p) in profiles {
        out.insert(id.clone(), json!({"name": p.name, "color": p.hex_color()}));
    }
    Value::Object(out)
}

/// `team_label`.
pub fn team_label(team_id: &str, profiles: &TeamProfiles) -> String {
    let id = normalize_team_id(team_id);
    profiles.get(&id).map(|p| p.name.clone()).unwrap_or_else(|| default_name(&id))
}

/// `team_color`.
pub fn team_color(team_id: &str, profiles: &TeamProfiles) -> Rgb {
    let id = normalize_team_id(team_id);
    profiles.get(&id).map(|p| p.color).unwrap_or_else(|| default_color(&id))
}

/// `normalize_team_stances`: mirrored into both directions.
pub fn normalize_team_stances(raw: Option<&Value>) -> Stances {
    let mut stances = Stances::new();
    let Some(Value::Object(map)) = raw else { return stances };
    for (left, row) in map {
        let Value::Object(row) = row else { continue };
        let left_id = normalize_team_id(left);
        for (right, relation) in row {
            let rel = relation.as_str().unwrap_or("").trim().to_lowercase();
            if !RELATIONS.contains(&rel.as_str()) {
                continue;
            }
            let right_id = normalize_team_id(right);
            stances.entry(left_id.clone()).or_default().insert(right_id.clone(), rel.clone());
            stances.entry(right_id).or_default().insert(left_id.clone(), rel);
        }
    }
    stances
}

/// Stances back into JSON (the shape `normalize_team_stances` reads).
pub fn stances_to_json(stances: &Stances) -> Value {
    serde_json::to_value(stances).unwrap_or(Value::Null)
}

/// `team_stance`: declared, then same-team friendship, then the default
/// hostility of Rivals; None when unrelated.
pub fn team_stance(left: &str, right: &str, stances: &Stances) -> Option<String> {
    let l = normalize_team_id(left);
    let r = normalize_team_id(right);
    if l == NEUTRAL || r == NEUTRAL {
        return None;
    }
    if let Some(rel) = stances.get(&l).and_then(|row| row.get(&r)) {
        return Some(rel.clone());
    }
    if l == r {
        return Some("friend".into());
    }
    if DEFAULT_HOSTILE_TEAMS.contains(&l.as_str()) || DEFAULT_HOSTILE_TEAMS.contains(&r.as_str()) {
        return Some("foe".into());
    }
    None
}

/// `stance_pairs`: every unordered pair, ordered by id, with its relation.
pub fn stance_pairs(profiles: &TeamProfiles, stances: &Stances) -> Vec<(String, String, String)> {
    let ids: Vec<&String> = profiles.keys().collect();
    let mut pairs = Vec::new();
    for (i, left) in ids.iter().enumerate() {
        for right in &ids[i + 1..] {
            let relation = team_stance(left, right, stances).unwrap_or_else(|| "neutral".into());
            pairs.push(((*left).clone(), (*right).clone(), relation));
        }
    }
    pairs
}

/// `minimal_stances`: each pair once, keeping an explicit `neutral`.
pub fn minimal_stances(stances: &Stances) -> Value {
    let mut payload: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for (left, row) in stances {
        let left_id = normalize_team_id(left);
        if left_id == NEUTRAL {
            continue;
        }
        for (right, relation) in row {
            let right_id = normalize_team_id(right);
            let relation = relation.trim().to_lowercase();
            if !RELATIONS.contains(&relation.as_str()) || right_id == NEUTRAL || right_id == left_id {
                continue;
            }
            let pair = if left_id <= right_id {
                (left_id.clone(), right_id.clone())
            } else {
                (right_id.clone(), left_id.clone())
            };
            if seen.contains(&pair) {
                continue;
            }
            seen.push(pair.clone());
            payload.entry(pair.0).or_default().insert(pair.1, relation);
        }
    }
    serde_json::to_value(payload).unwrap_or(Value::Null)
}

/// `describe_stance`: one sentence naming both teams and what follows.
pub fn describe_stance(left: &str, right: &str, relation: &str, profiles: &TeamProfiles) -> String {
    let l = team_label(left, profiles);
    let r = team_label(right, profiles);
    match relation {
        "friend" => format!("{l} and {r} are allies and keep each other company."),
        "foe" => format!(
            "{l} and {r} are foes: they fight on sight and a Guard of either team intercepts the other near its base. Damage is real and the loser dies."
        ),
        _ => format!("{l} and {r} ignore each other."),
    }
}

/// `ConfigWindow._unique_team_id`: a stable, case-folded id nobody uses yet.
pub fn unique_team_id(name: &str, profiles: &TeamProfiles) -> String {
    let lowered = name.trim().to_lowercase();
    let slug: String = lowered.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let mut slug: String =
        slug.split('_').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_").chars().take(32).collect();
    if slug.is_empty() {
        slug = "team".into();
    }
    if slug == NEUTRAL {
        slug = "team".into();
    }
    let mut candidate = slug.clone();
    let mut suffix = 2;
    while profiles.contains_key(&candidate) {
        candidate = format!("{slug}_{suffix}").chars().take(32).collect();
        suffix += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_white_and_built_ins_keep_their_colours() {
        assert_eq!(default_color("neutral"), [255, 255, 255]);
        assert_eq!(default_color("Rivals"), [209, 83, 79]);
        assert_eq!(default_name("porch_guard"), "Porch Guard");
        assert_eq!(default_name("neutral"), "Neutral / solo");
    }

    #[test]
    fn ids_are_case_folded() {
        assert_eq!(normalize_team_id(" Porch  Guard "), "porch_guard");
        assert_eq!(normalize_team_id(""), "neutral");
    }

    #[test]
    fn profiles_from_a_preset_block() {
        let raw =
            json!({"porch_guard": {"name": "Porch guard", "color": "#112233"}, "Shed": "Shed crew", "neutral": "x"});
        let teams = normalize_teams(Some(&raw), &["rivals".into(), "neutral".into()]);
        assert_eq!(teams.len(), 3);
        assert_eq!(teams["porch_guard"].color, [0x11, 0x22, 0x33]);
        assert_eq!(teams["shed"].name, "Shed crew");
        assert_eq!(teams["rivals"].color, [209, 83, 79]);
        let payload = teams_payload(&teams);
        let back = normalize_teams(Some(&payload), &[]);
        assert_eq!(back, teams);
    }

    #[test]
    fn invented_colours_differ() {
        let a = distinct_color("porch_guard", &[]);
        let b = distinct_color("shed_crew", &[a]);
        assert!(distance(a, b) >= MIN_COLOR_SEPARATION);
    }

    #[test]
    fn stances_round_trip_minimally() {
        let raw = json!({"hunters": {"pack_a": "foe"}, "pack_a": {"hunters": "foe", "rivals": "neutral"}});
        let stances = normalize_team_stances(Some(&raw));
        assert_eq!(stances["hunters"]["pack_a"], "foe");
        assert_eq!(stances["rivals"]["pack_a"], "neutral");
        let minimal = minimal_stances(&stances);
        assert_eq!(minimal, json!({"hunters": {"pack_a": "foe"}, "pack_a": {"rivals": "neutral"}}));
    }

    #[test]
    fn rivals_are_foes_unless_declared() {
        let teams = normalize_teams(None, &["rivals".into(), "hunters".into(), "pack_a".into()]);
        let pairs = stance_pairs(&teams, &Stances::new());
        assert!(pairs.contains(&("hunters".into(), "rivals".into(), "foe".into())));
        assert!(pairs.contains(&("hunters".into(), "pack_a".into(), "neutral".into())));
        assert!(describe_stance("hunters", "rivals", "foe", &teams).contains("the loser dies"));
    }

    #[test]
    fn unique_ids_never_collide() {
        let teams = normalize_teams(None, &["porch_guard".into()]);
        assert_eq!(unique_team_id("Porch guard!", &teams), "porch_guard_2");
        assert_eq!(unique_team_id("Neutral", &teams), "team");
        assert_eq!(unique_team_id("???", &teams), "team");
    }
}
