//! Port of `state/teams.py`: teams a person can name, colour and understand.
//!
//! A team is a stable (case-folded) id, a name its owner chose, and a colour
//! for its base ring and member marker. Invented teams get a colour hashed
//! from the id, nudged apart so two teams in one scene never look alike.

use dc_content::OrderedMap;
use dc_content::palettes::hsv_to_rgb;
use dc_content::util::{py_str, py_title, py_truthy};
use dc_core::Color;
use dc_core::math::py_round;
use serde_json::{Value, json};

use crate::progression::{DEFAULT_HOSTILE_TEAMS, RELATIONS, Stances, normalize_team_id, team_stance};

pub const NEUTRAL: &str = "neutral";

/// What hostility actually does today; shown by the settings window, the
/// inspector and the README.
pub const HOSTILITY_NOTE: &str = "Foes fight. Two hostile spiders that meet will attack, using whatever \
skills they have -- silk to pin, a pounce to close -- and a Guard still \
raises an alert and intercepts. A beaten spider dies and leaves a carcass \
that is eaten away; it does not come back. Turn Conflict off to go back to \
alerts without damage.";

/// `STANCE_LABELS`.
pub const STANCE_LABELS: [(&str, &str); 3] = [("friend", "Allies"), ("neutral", "Ignore each other"), ("foe", "Foes")];

/// `BUILT_IN_TEAMS`: id -> (label, colour).
pub const BUILT_IN_TEAMS: [(&str, &str, Color); 4] = [
    ("pack_a", "Pack A", Color::rgb(79, 163, 209)),
    ("pack_b", "Pack B", Color::rgb(122, 196, 129)),
    ("hunters", "Hunters", Color::rgb(226, 166, 74)),
    ("rivals", "Rivals", Color::rgb(209, 83, 79)),
];

fn built_in(team_id: &str) -> Option<(&'static str, Color)> {
    BUILT_IN_TEAMS.iter().find(|(id, _, _)| *id == team_id).map(|(_, name, color)| (*name, *color))
}

/// One team (`TeamProfile`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamProfile {
    pub id: String,
    pub name: String,
    pub color: Color,
}

impl TeamProfile {
    pub fn new(id: impl Into<String>, name: impl Into<String>, color: Color) -> Self {
        Self { id: id.into(), name: name.into(), color }
    }
    pub fn hex_color(&self) -> String {
        color_to_hex(self.color)
    }
}

/// Team table: id -> profile, in the order teams were declared.
pub type TeamProfiles = OrderedMap<TeamProfile>;

/// `default_color`: built-in colour, white for neutral, else hashed from the id.
pub fn default_color(team_id: &str) -> Color {
    let team_id = normalize_team_id(team_id);
    if let Some((_, color)) = built_in(&team_id) {
        return color;
    }
    if team_id == NEUTRAL {
        return Color::rgb(255, 255, 255);
    }
    color_at_hue(fnv1a(&team_id), 0)
}

/// Minimum plain RGB distance between two team colours.
pub const MIN_COLOR_SEPARATION: f64 = 60.0;

fn color_at_hue(digest: u32, hue_offset: u32) -> Color {
    let digest = digest as u64;
    let hue = (((digest % 360) + hue_offset as u64) % 360) as f64 / 360.0;
    let saturation = 0.46 + ((digest >> 9) % 5) as f64 * 0.06;
    let value = 0.78 + ((digest >> 17) % 4) as f64 * 0.05;
    let (r, g, b) = hsv_to_rgb(hue, saturation, value);
    let c = |x: f64| py_round(x * 255.0) as u8;
    Color::rgb(c(r), c(g), c(b))
}

/// `_distance`: plain RGB distance.
pub fn color_distance(left: Color, right: Color) -> f64 {
    let d = |a: u8, b: u8| (a as f64 - b as f64).powi(2);
    (d(left.r, right.r) + d(left.g, right.g) + d(left.b, right.b)).sqrt()
}

/// `distinct_color`: the team's own colour, rotated until it is clear of `taken`.
pub fn distinct_color(team_id: &str, taken: &[Color]) -> Color {
    let team_id = normalize_team_id(team_id);
    if let Some((_, color)) = built_in(&team_id) {
        return color;
    }
    if team_id == NEUTRAL {
        return Color::rgb(255, 255, 255);
    }
    let digest = fnv1a(&team_id);
    for step in 0..8 {
        let candidate = color_at_hue(digest, step * 47);
        if taken.iter().all(|other| color_distance(candidate, *other) >= MIN_COLOR_SEPARATION) {
            return candidate;
        }
    }
    color_at_hue(digest, 0)
}

/// `_fnv1a`: a stable 32-bit hash of the UTF-8 bytes.
pub fn fnv1a(text: &str) -> u32 {
    let mut digest: u32 = 2_166_136_261;
    for byte in text.as_bytes() {
        digest = (digest ^ *byte as u32).wrapping_mul(16_777_619);
    }
    digest
}

/// `default_name`: the name a team has before anybody renames it.
pub fn default_name(team_id: &str) -> String {
    let team_id = normalize_team_id(team_id);
    if let Some((name, _)) = built_in(&team_id) {
        return name.into();
    }
    if team_id == NEUTRAL {
        return "Neutral / solo".into();
    }
    let title = py_title(team_id.replace(['_', '-'], " ").trim());
    if title.is_empty() { "Team".into() } else { title }
}

/// `parse_color`: `#rrggbb`, `rrggbb`, `#rgb`, or three numbers; `None` if unusable.
pub fn parse_color(value: Option<&Value>) -> Option<Color> {
    dc_content::preset_io::parse_color(value).map(|(r, g, b)| Color::rgb(r, g, b))
}

/// `color_to_hex`.
pub fn color_to_hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// `normalize_team_name`: never empty, at most 32 characters.
pub fn normalize_team_name(value: Option<&Value>, team_id: &str) -> String {
    let text = match value {
        Some(v) if py_truthy(v) => py_str(v).trim().to_string(),
        _ => String::new(),
    };
    if text.is_empty() {
        return default_name(team_id);
    }
    text.chars().take(32).collect()
}

/// `normalize_teams(raw, used_ids)`: the team table from a preset's `teams`
/// block plus every id a slot refers to. Chosen colours are never moved;
/// invented ones are assigned in sorted id order, clear of each other.
pub fn normalize_teams<S: AsRef<str>>(raw: Option<&Value>, used_ids: &[S]) -> TeamProfiles {
    let mut profiles = TeamProfiles::new();
    let mut taken: Vec<Color> = Vec::new();
    let mut pending: Vec<(String, String)> = Vec::new();

    if let Some(Value::Object(raw)) = raw {
        for (key, value) in raw {
            let team_id = normalize_team_id(key);
            if team_id == NEUTRAL {
                continue;
            }
            let (name, chosen) = match value {
                Value::Object(entry) => {
                    (normalize_team_name(entry.get("name"), &team_id), parse_color(entry.get("color")))
                }
                other => (normalize_team_name(Some(other), &team_id), None),
            };
            match chosen {
                None => {
                    pending.push((team_id.clone(), name.clone()));
                    profiles.insert(team_id.clone(), TeamProfile::new(team_id, name, Color::rgb(0, 0, 0)));
                }
                Some(color) => {
                    profiles.insert(team_id.clone(), TeamProfile::new(team_id, name, color));
                    taken.push(color);
                }
            }
        }
    }

    for value in used_ids {
        let team_id = normalize_team_id(value.as_ref());
        if team_id == NEUTRAL || profiles.contains_key(&team_id) {
            continue;
        }
        let name = default_name(&team_id);
        pending.push((team_id.clone(), name.clone()));
        profiles.insert(team_id.clone(), TeamProfile::new(team_id, name, Color::rgb(0, 0, 0)));
    }

    pending.sort();
    for (team_id, name) in pending {
        let color = distinct_color(&team_id, &taken);
        taken.push(color);
        profiles.insert(team_id.clone(), TeamProfile::new(team_id, name, color));
    }
    profiles
}

/// `teams_payload`: the `teams` block to save, sorted by id.
pub fn teams_payload(profiles: &TeamProfiles) -> Value {
    let mut items: Vec<(&str, &TeamProfile)> = profiles.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = serde_json::Map::new();
    for (id, profile) in items {
        out.insert(id.to_string(), json!({"name": profile.name, "color": profile.hex_color()}));
    }
    Value::Object(out)
}

/// `team_profile`: the defined profile, or one invented from the id.
pub fn team_profile(team_id: &str, profiles: Option<&TeamProfiles>) -> TeamProfile {
    let team_id = normalize_team_id(team_id);
    if let Some(found) = profiles.filter(|p| !p.is_empty()).and_then(|p| p.get(&team_id)) {
        return found.clone();
    }
    TeamProfile::new(team_id.clone(), default_name(&team_id), default_color(&team_id))
}

pub fn team_label(team_id: &str, profiles: Option<&TeamProfiles>) -> String {
    team_profile(team_id, profiles).name
}

pub fn team_color(team_id: &str, profiles: Option<&TeamProfiles>) -> Color {
    team_profile(team_id, profiles).color
}

/// `stance_label`.
pub fn stance_label(relation: &str) -> &'static str {
    let relation = if relation.is_empty() { "neutral" } else { relation };
    STANCE_LABELS.iter().find(|(k, _)| *k == relation).map(|(_, v)| *v).unwrap_or("Ignore each other")
}

/// `describe_stance`: one readable sentence naming both teams.
pub fn describe_stance(left_id: &str, right_id: &str, relation: &str, profiles: Option<&TeamProfiles>) -> String {
    let left = team_label(left_id, profiles);
    let right = team_label(right_id, profiles);
    match relation {
        "friend" => format!("{left} and {right} are allies and keep each other company."),
        "foe" => format!(
            "{left} and {right} are foes: they fight on sight and a Guard of either team intercepts the other near its base. Damage is real and the loser dies."
        ),
        _ => format!("{left} and {right} ignore each other."),
    }
}

/// `stance_pairs`: every unordered pair of teams with its current relation.
pub fn stance_pairs(profiles: &TeamProfiles, stances: Option<&Stances>) -> Vec<(String, String, String)> {
    let mut ids: Vec<&str> = profiles.keys().collect();
    ids.sort();
    let mut pairs = Vec::new();
    for (index, left) in ids.iter().enumerate() {
        for right in &ids[index + 1..] {
            let relation = team_stance(left, right, stances).unwrap_or_else(|| "neutral".into());
            pairs.push((left.to_string(), right.to_string(), relation));
        }
    }
    pairs
}

/// `minimal_stances`: the `team_relations` block to save, each pair once.
pub fn minimal_stances(stances: Option<&Stances>) -> Stances {
    let mut payload = Stances::new();
    let Some(stances) = stances else { return payload };
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut rows: Vec<(&str, &OrderedMap<String>)> = stances.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    for (left, row) in rows {
        let left_id = normalize_team_id(left);
        if left_id == NEUTRAL {
            continue;
        }
        let mut cells: Vec<(&str, &String)> = row.iter().collect();
        cells.sort();
        for (right, relation) in cells {
            let right_id = normalize_team_id(right);
            let relation = relation.trim().to_lowercase();
            if !RELATIONS.contains(&relation.as_str()) || right_id == NEUTRAL || right_id == left_id {
                continue;
            }
            let mut pair = [left_id.clone(), right_id];
            pair.sort();
            let [a, b] = pair;
            if seen.contains(&(a.clone(), b.clone())) {
                continue;
            }
            seen.push((a.clone(), b.clone()));
            payload.entry_or_insert_with(&a, OrderedMap::new).insert(b, relation);
        }
    }
    payload
}

/// `is_default_hostile`.
pub fn is_default_hostile(team_id: &str) -> bool {
    DEFAULT_HOSTILE_TEAMS.contains(&normalize_team_id(team_id).as_str())
}
