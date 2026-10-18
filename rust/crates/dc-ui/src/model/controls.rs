//! The Adventure bindings as the controls editor edits them. Mirrors the
//! Qt-free half of app/controls.py: `ACTIONS`, `DEFAULT_BINDINGS`,
//! `SECOND_PLAYER_BINDINGS`, `MOVEMENT_MODES`, `AIM_CONES` and
//! `ControlSettings` (`binding`, `action_text`, `rebind`, `reset`,
//! `to_dict`/`from_dict`).
//!
//! [`crate::bridge`] converts to and from `dc_adventure::ControlSettings`
//! both ways (same JSON on disk). Binding names stay the Qt spellings ("W",
//! "Space", "Shift", "Esc", "Mouse Left") so `controls.json` is shared with
//! the Python build; [`key_name`] maps egui keys onto them.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// `(id, label, what it does)`, in list order.
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("move_up", "Move up", "Walk up the screen."),
    ("move_down", "Move down", "Walk down the screen."),
    ("move_left", "Move left", "Walk left."),
    ("move_right", "Move right", "Walk right."),
    ("sprint", "Sprint", "Hold while moving to run faster. Drains stamina."),
    ("jump", "Jump", "Pounce forward in the direction you are walking."),
    ("shoot", "Shoot silk", "Fire one silk charge along your aim, even if it misses. Refill at home or the Silk loom."),
    ("bite", "Bite", "Hold to repeatedly bite a foe in front of you, inside the aim cone."),
    ("companion_follow", "Scout: follow", "Your companion follows and fights nearby enemies."),
    ("companion_defend", "Scout: defend here", "Your companion holds its current position and protects the area."),
    ("companion_attack", "Scout: attack target", "Point at an enemy and order your companion to attack it."),
    ("skills", "Character & armour", "Open your character: armour, skills and upgrades."),
    ("pause", "Pause menu", "Pause, save, change settings or release the spider. Esc always pauses too."),
    (
        "map_view",
        "Map / desktop",
        "Show the mission's painted map instead of your desktop; press again for the desktop.",
    ),
    (
        "release",
        "Release / take controls",
        "Hand the mouse and keyboard back to the computer and pause the raid; press again, from anywhere, to take control of your spider again.",
    ),
];

pub const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("move_up", "W"),
    ("move_down", "S"),
    ("move_left", "A"),
    ("move_right", "D"),
    ("sprint", "Shift"),
    ("jump", "Space"),
    ("shoot", "Q"),
    ("bite", "F"),
    ("companion_follow", "1"),
    ("companion_defend", "2"),
    ("companion_attack", "3"),
    ("skills", "K"),
    ("pause", "Esc"),
    ("map_view", "`"),
    ("release", "F8"),
];

/// Player 2's fixed keys.
pub const SECOND_PLAYER_OVERRIDES: &[(&str, &str)] = &[
    ("move_up", "Up"),
    ("move_down", "Down"),
    ("move_left", "Left"),
    ("move_right", "Right"),
    ("bite", "J"),
    ("shoot", "U"),
    ("jump", "I"),
    ("sprint", "O"),
    ("skills", "P"),
    ("companion_follow", "7"),
    ("companion_defend", "8"),
    ("companion_attack", "9"),
];

pub const MOVEMENT_MODES: &[(&str, &str)] = &[("screen", "Screen directions"), ("turn", "Turn and walk")];
pub const DEFAULT_MOVEMENT: &str = "turn";
pub const TURN_ACTION_TEXT: &[(&str, &str, &str)] = &[
    ("move_up", "Forward", "Walk forward, the way your spider is facing."),
    ("move_down", "Back up", "Walk slowly backwards, still facing forward. No sprint."),
    ("move_left", "Turn left", "Turn anticlockwise, on the spot or while walking."),
    ("move_right", "Turn right", "Turn clockwise, on the spot or while walking."),
];
pub const AIM_CONES: [u32; 5] = [60, 90, 120, 180, 360];
pub const DEFAULT_AIM_CONE: u32 = 90;
pub const MOUSE_NAMES: [&str; 5] = ["Mouse Left", "Mouse Right", "Mouse Middle", "Mouse Back", "Mouse Forward"];

/// `controls_ui.CONE_LABELS`.
pub fn cone_label(degrees: u32) -> &'static str {
    match degrees {
        60 => "60\u{b0}  narrow",
        90 => "90\u{b0}  (recommended)",
        120 => "120\u{b0}  wide",
        180 => "180\u{b0}  half circle",
        _ => "360\u{b0}  free aim",
    }
}

fn default_binding(action: &str) -> &'static str {
    DEFAULT_BINDINGS.iter().find(|(a, _)| *a == action).map(|(_, b)| *b).unwrap_or("")
}

/// `ControlSettings`.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlSettings {
    pub bindings: BTreeMap<String, String>,
    pub aim_cone: u32,
    pub face_mouse_when_still: bool,
    pub movement: String,
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self {
            bindings: DEFAULT_BINDINGS.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect(),
            aim_cone: DEFAULT_AIM_CONE,
            face_mouse_when_still: false,
            movement: DEFAULT_MOVEMENT.into(),
        }
    }
}

impl ControlSettings {
    /// `second_player_controls`.
    pub fn second_player(movement: Option<&str>) -> Self {
        let mut settings = Self::default();
        for (action, key) in SECOND_PLAYER_OVERRIDES {
            settings.bindings.insert((*action).into(), (*key).into());
        }
        settings.movement = movement.unwrap_or(DEFAULT_MOVEMENT).into();
        settings
    }

    pub fn binding(&self, action: &str) -> String {
        self.bindings.get(action).cloned().unwrap_or_else(|| default_binding(action).to_owned())
    }

    pub fn turn_movement(&self) -> bool {
        self.movement == "turn"
    }

    /// `(label, what it does)` under the current movement mode.
    pub fn action_text(&self, action: &str) -> (String, String) {
        if self.turn_movement()
            && let Some((_, label, text)) = TURN_ACTION_TEXT.iter().find(|(a, _, _)| *a == action)
        {
            return ((*label).into(), (*text).into());
        }
        ACTIONS
            .iter()
            .find(|(a, _, _)| *a == action)
            .map(|(_, l, t)| ((*l).into(), (*t).into()))
            .unwrap_or_else(|| (action.to_owned(), String::new()))
    }

    /// `rebind`: an action already using `name` takes this action's old key,
    /// a swap, never a gap. Returns the action that lost it.
    pub fn rebind(&mut self, action: &str, name: &str) -> Option<String> {
        if !ACTIONS.iter().any(|(a, _, _)| *a == action) {
            return None;
        }
        let previous = self.binding(action);
        let mut displaced = None;
        for (other, bound) in self.bindings.iter_mut() {
            if other != action && bound == name {
                *bound = previous.clone();
                displaced = Some(other.clone());
            }
        }
        self.bindings.insert(action.into(), name.into());
        displaced
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `to_dict`, the shape of `controls.json`.
    pub fn to_json(&self) -> Value {
        json!({
            "version": 2,
            "bindings": self.bindings,
            "aim_cone": self.aim_cone,
            "face_mouse_when_still": self.face_mouse_when_still,
            "movement": self.movement,
        })
    }

    /// `from_dict`, tolerant of hand edits: duplicates and unknown keys fall
    /// back to a free default (then F1..F24), the legacy mouse layout migrates.
    pub fn from_json(data: &Value) -> Self {
        let mut settings = Self::default();
        let Some(obj) = data.as_object() else { return settings };
        let mut bindings: Option<BTreeMap<String, String>> = obj
            .get("bindings")
            .and_then(|b| b.as_object())
            .map(|b| b.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned()))).collect());
        if let Some(b) = bindings.as_mut()
            && !obj.contains_key("version")
            && b.get("shoot").map(String::as_str) == Some("Mouse Left")
            && b.get("bite").map(String::as_str) == Some("Mouse Right")
            && !b.iter().any(|(k, v)| k != "shoot" && k != "bite" && (v == "F" || v == "Q"))
        {
            b.insert("shoot".into(), "Q".into());
            b.insert("bite".into(), "F".into());
        }
        if let Some(b) = bindings {
            let mut assigned = BTreeMap::new();
            let mut used: Vec<String> = Vec::new();
            for (action, _, _) in ACTIONS {
                if let Some(value) = b.get(*action)
                    && !value.is_empty()
                    && !used.contains(value)
                    && is_known_name(value)
                {
                    assigned.insert((*action).to_owned(), value.clone());
                    used.push(value.clone());
                }
            }
            for (action, _, _) in ACTIONS {
                if assigned.contains_key(*action) {
                    continue;
                }
                let mut choices = vec![default_binding(action).to_owned()];
                choices.extend((1..=24).map(|i| format!("F{i}")));
                if let Some(value) = choices.into_iter().find(|k| !used.contains(k)) {
                    used.push(value.clone());
                    assigned.insert((*action).to_owned(), value);
                }
            }
            settings.bindings = assigned;
        }
        let cone = obj.get("aim_cone").and_then(Value::as_i64).unwrap_or(DEFAULT_AIM_CONE as i64);
        settings.aim_cone = cone.clamp(30, 360) as u32;
        settings.face_mouse_when_still = obj.get("face_mouse_when_still").and_then(Value::as_bool).unwrap_or(false);
        let movement = obj.get("movement").and_then(Value::as_str).unwrap_or(DEFAULT_MOVEMENT);
        settings.movement =
            if MOVEMENT_MODES.iter().any(|(m, _)| *m == movement) { movement.into() } else { DEFAULT_MOVEMENT.into() };
        settings
    }

    /// `instructions`: (button, label, what it does) for every action.
    pub fn instructions(&self) -> Vec<(String, String, String)> {
        ACTIONS
            .iter()
            .map(|(a, _, _)| {
                let (label, text) = self.action_text(a);
                (self.binding(a), label, text)
            })
            .collect()
    }
}

/// Whether a stored name is one the overlay can read back (`name_to_input`).
pub fn is_known_name(name: &str) -> bool {
    if MOUSE_NAMES.contains(&name) {
        return true;
    }
    matches!(name, "Shift" | "Ctrl" | "Alt" | "Meta") || egui_key_for(name).is_some()
}

const KEY_NAMES: &[(egui::Key, &str)] = &[
    (egui::Key::Space, "Space"),
    (egui::Key::Escape, "Esc"),
    (egui::Key::Tab, "Tab"),
    (egui::Key::Backspace, "Backspace"),
    (egui::Key::Enter, "Return"),
    (egui::Key::Insert, "Ins"),
    (egui::Key::Delete, "Del"),
    (egui::Key::Home, "Home"),
    (egui::Key::End, "End"),
    (egui::Key::PageUp, "PgUp"),
    (egui::Key::PageDown, "PgDown"),
    (egui::Key::ArrowUp, "Up"),
    (egui::Key::ArrowDown, "Down"),
    (egui::Key::ArrowLeft, "Left"),
    (egui::Key::ArrowRight, "Right"),
    (egui::Key::Backtick, "`"),
    (egui::Key::Minus, "-"),
    (egui::Key::Equals, "="),
    (egui::Key::OpenBracket, "["),
    (egui::Key::CloseBracket, "]"),
    (egui::Key::Backslash, "\\"),
    (egui::Key::Semicolon, ";"),
    (egui::Key::Quote, "'"),
    (egui::Key::Comma, ","),
    (egui::Key::Period, "."),
    (egui::Key::Slash, "/"),
];

/// `key_name`: the name `QKeySequence` gives a key ("W", "Space", "F8").
pub fn key_name(key: egui::Key) -> String {
    if let Some((_, name)) = KEY_NAMES.iter().find(|(k, _)| *k == key) {
        return (*name).to_owned();
    }
    let name = key.name();
    // Letters, digits ("1"), and F-keys come out as Qt spells them.
    name.to_owned()
}

/// The egui key a stored name means, if it is a key.
pub fn egui_key_for(name: &str) -> Option<egui::Key> {
    if let Some((k, _)) = KEY_NAMES.iter().find(|(_, n)| *n == name) {
        return Some(*k);
    }
    egui::Key::from_name(name)
}

/// `mouse_name` for an egui pointer button.
pub fn mouse_name(button: egui::PointerButton) -> &'static str {
    match button {
        egui::PointerButton::Primary => "Mouse Left",
        egui::PointerButton::Secondary => "Mouse Right",
        egui::PointerButton::Middle => "Mouse Middle",
        egui::PointerButton::Extra1 => "Mouse Back",
        egui::PointerButton::Extra2 => "Mouse Forward",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_its_own_button_by_default() {
        let settings = ControlSettings::default();
        let mut keys: Vec<String> = ACTIONS.iter().map(|(a, _, _)| settings.binding(a)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ACTIONS.len());
        for key in &keys {
            assert!(is_known_name(key), "{key}");
        }
    }

    #[test]
    fn taking_a_button_in_use_swaps_rather_than_leaving_a_gap() {
        let mut settings = ControlSettings::default();
        let displaced = settings.rebind("jump", "W");
        assert_eq!(displaced.as_deref(), Some("move_up"));
        assert_eq!(settings.binding("jump"), "W");
        assert_eq!(settings.binding("move_up"), "Space");
    }

    #[test]
    fn a_broken_file_still_gives_working_controls() {
        let data = json!({"bindings": {"move_up": "Q", "shoot": "Q", "bite": "Nonsense key"}, "aim_cone": 5, "movement": "fly"});
        let settings = ControlSettings::from_json(&data);
        assert_eq!(settings.binding("move_up"), "Q");
        assert_ne!(settings.binding("shoot"), "Q");
        assert_eq!(settings.aim_cone, 30);
        assert_eq!(settings.movement, "turn");
        let mut keys: Vec<String> = settings.bindings.values().cloned().collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ACTIONS.len());
    }

    #[test]
    fn controls_round_trip() {
        let mut settings = ControlSettings::default();
        settings.rebind("bite", "Mouse Right");
        settings.aim_cone = 120;
        settings.movement = "screen".into();
        let back = ControlSettings::from_json(&settings.to_json());
        assert_eq!(back, settings);
        assert_eq!(back.action_text("move_up").0, "Move up");
        let turn = ControlSettings::default();
        assert_eq!(turn.action_text("move_up").0, "Forward");
    }

    #[test]
    fn keys_are_named_as_qt_names_them() {
        assert_eq!(key_name(egui::Key::W), "W");
        assert_eq!(key_name(egui::Key::Space), "Space");
        assert_eq!(key_name(egui::Key::F8), "F8");
        assert_eq!(key_name(egui::Key::Num1), "1");
        assert_eq!(key_name(egui::Key::Backtick), "`");
        assert_eq!(egui_key_for("Up"), Some(egui::Key::ArrowUp));
        assert_eq!(egui_key_for("1"), Some(egui::Key::Num1));
    }

    #[test]
    fn the_second_player_has_fixed_keys() {
        let p2 = ControlSettings::second_player(None);
        assert_eq!(p2.binding("move_up"), "Up");
        assert_eq!(p2.binding("skills"), "P");
        assert_eq!(p2.binding("pause"), "Esc");
    }
}
