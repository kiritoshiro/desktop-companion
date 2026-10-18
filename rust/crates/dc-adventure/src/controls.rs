//! Port of app/controls.py (and `virtual_key_for` from app/engine.py).
//!
//! Adventure controls: which key or mouse button does what, and how aiming
//! works.
//!
//! - Movement is keys only. By default the spider no longer turns to face
//!   the pointer while standing still.
//! - The pointer aims inside a cone around the facing (`aim_cone`, total
//!   degrees; 90 by default, 360 for free aim).
//! - Bindings are stored by name ("W", "Shift", "Space", "Mouse Left") in
//!   `controls.json` in the state folder.
//!
//! Qt key codes are replaced by [`Key`]; its names are exactly the strings
//! Qt's `QKeySequence` writes, so stored files stay compatible.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::pyjson::{atomic_write, dumps_indent2, py_int, read_json, truthy};

// -- actions -------------------------------------------------------------------

/// Every rebindable action, in list order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Sprint,
    Jump,
    Shoot,
    Bite,
    CompanionFollow,
    CompanionDefend,
    CompanionAttack,
    Skills,
    Pause,
    MapView,
    Release,
}

/// `(id, label, what it does)`. The order is the order they are listed in.
pub const ACTIONS: [(Action, &str, &str); 15] = [
    (Action::MoveUp, "Move up", "Walk up the screen."),
    (Action::MoveDown, "Move down", "Walk down the screen."),
    (Action::MoveLeft, "Move left", "Walk left."),
    (Action::MoveRight, "Move right", "Walk right."),
    (Action::Sprint, "Sprint", "Hold while moving to run faster. Drains stamina."),
    (Action::Jump, "Jump", "Pounce forward in the direction you are walking."),
    (
        Action::Shoot,
        "Shoot silk",
        "Fire one silk charge along your aim, even if it misses. Refill at home or the Silk loom.",
    ),
    (Action::Bite, "Bite", "Hold to repeatedly bite a foe in front of you, inside the aim cone."),
    (Action::CompanionFollow, "Scout: follow", "Your companion follows and fights nearby enemies."),
    (Action::CompanionDefend, "Scout: defend here", "Your companion holds its current position and protects the area."),
    (Action::CompanionAttack, "Scout: attack target", "Point at an enemy and order your companion to attack it."),
    (Action::Skills, "Character & armour", "Open your character: armour, skills and upgrades."),
    (Action::Pause, "Pause menu", "Pause, save, change settings or release the spider. Esc always pauses too."),
    (
        Action::MapView,
        "Map / desktop",
        "Show the mission's painted map instead of your desktop; press again for the desktop.",
    ),
    (
        Action::Release,
        "Release / take controls",
        "Hand the mouse and keyboard back to the computer and pause the raid; press again, from anywhere, \
         to take control of your spider again.",
    ),
];

pub const ACTION_IDS: [Action; 15] = [
    Action::MoveUp,
    Action::MoveDown,
    Action::MoveLeft,
    Action::MoveRight,
    Action::Sprint,
    Action::Jump,
    Action::Shoot,
    Action::Bite,
    Action::CompanionFollow,
    Action::CompanionDefend,
    Action::CompanionAttack,
    Action::Skills,
    Action::Pause,
    Action::MapView,
    Action::Release,
];

impl Action {
    /// The id stored on disk ("move_up", ...).
    pub fn id(self) -> &'static str {
        match self {
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::MoveLeft => "move_left",
            Action::MoveRight => "move_right",
            Action::Sprint => "sprint",
            Action::Jump => "jump",
            Action::Shoot => "shoot",
            Action::Bite => "bite",
            Action::CompanionFollow => "companion_follow",
            Action::CompanionDefend => "companion_defend",
            Action::CompanionAttack => "companion_attack",
            Action::Skills => "skills",
            Action::Pause => "pause",
            Action::MapView => "map_view",
            Action::Release => "release",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        ACTION_IDS.iter().copied().find(|a| a.id() == id)
    }

    /// A movement action (what a webbed spider struggles with).
    pub fn is_movement(self) -> bool {
        matches!(self, Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight)
    }
}

/// Player one's default bindings, in action order.
pub const DEFAULT_BINDINGS: [(Action, &str); 15] = [
    (Action::MoveUp, "W"),
    (Action::MoveDown, "S"),
    (Action::MoveLeft, "A"),
    (Action::MoveRight, "D"),
    (Action::Sprint, "Shift"),
    (Action::Jump, "Space"),
    (Action::Shoot, "Q"),
    (Action::Bite, "F"),
    (Action::CompanionFollow, "1"),
    (Action::CompanionDefend, "2"),
    (Action::CompanionAttack, "3"),
    (Action::Skills, "K"),
    (Action::Pause, "Esc"),
    (Action::MapView, "`"),
    (Action::Release, "F8"),
];

/// Player two's fixed keys: `{**DEFAULT_BINDINGS, ...}`, in action order.
pub const SECOND_PLAYER_BINDINGS: [(Action, &str); 15] = [
    (Action::MoveUp, "Up"),
    (Action::MoveDown, "Down"),
    (Action::MoveLeft, "Left"),
    (Action::MoveRight, "Right"),
    (Action::Sprint, "O"),
    (Action::Jump, "I"),
    (Action::Shoot, "U"),
    (Action::Bite, "J"),
    (Action::CompanionFollow, "7"),
    (Action::CompanionDefend, "8"),
    (Action::CompanionAttack, "9"),
    (Action::Skills, "P"),
    (Action::Pause, "Esc"),
    (Action::MapView, "`"),
    (Action::Release, "F8"),
];

/// `DEFAULT_BINDINGS[action]`.
pub fn default_binding(action: Action) -> &'static str {
    DEFAULT_BINDINGS.iter().find(|(a, _)| *a == action).map(|(_, n)| *n).expect("every action has a default")
}

fn bindings_from(table: &[(Action, &str)]) -> Vec<(Action, String)> {
    table.iter().map(|(a, n)| (*a, n.to_string())).collect()
}

/// Player 2's fixed keys. Up walks forward, Down backs up and Left/Right
/// turn, unless `movement` says screen directions.
pub fn second_player_controls(movement: Option<Movement>) -> ControlSettings {
    ControlSettings {
        bindings: bindings_from(&SECOND_PLAYER_BINDINGS),
        movement: movement.unwrap_or(DEFAULT_MOVEMENT),
        ..ControlSettings::default()
    }
}

// -- movement and aim ------------------------------------------------------------

/// How W/A/S/D move the spider. "screen": up/left/down/right on the screen.
/// "turn": forward, back up, turn left, turn right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Movement {
    Screen,
    Turn,
}

impl Movement {
    pub fn id(self) -> &'static str {
        match self {
            Movement::Screen => "screen",
            Movement::Turn => "turn",
        }
    }

    pub fn from_id(id: &str) -> Option<Movement> {
        match id {
            "screen" => Some(Movement::Screen),
            "turn" => Some(Movement::Turn),
            _ => None,
        }
    }
}

pub const MOVEMENT_MODES: [(Movement, &str); 2] =
    [(Movement::Screen, "Screen directions"), (Movement::Turn, "Turn and walk")];
pub const MOVEMENT_IDS: [&str; 2] = ["screen", "turn"];
/// Turn and walk by default, for both players.
pub const DEFAULT_MOVEMENT: Movement = Movement::Turn;
pub const TURN_ACTION_TEXT: [(Action, &str, &str); 4] = [
    (Action::MoveUp, "Forward", "Walk forward, the way your spider is facing."),
    (Action::MoveDown, "Back up", "Walk slowly backwards, still facing forward. No sprint."),
    (Action::MoveLeft, "Turn left", "Turn anticlockwise, on the spot or while walking."),
    (Action::MoveRight, "Turn right", "Turn clockwise, on the spot or while walking."),
];

pub const AIM_CONES: [i64; 5] = [60, 90, 120, 180, 360];
pub const DEFAULT_AIM_CONE: i64 = 90;

// -- inputs --------------------------------------------------------------------

/// A mouse button, with Qt's `MouseButton` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left = 1,
    Right = 2,
    Middle = 4,
    Back = 8,
    Forward = 16,
}

pub const MOUSE_NAMES: [(&str, MouseButton); 5] = [
    ("Mouse Left", MouseButton::Left),
    ("Mouse Right", MouseButton::Right),
    ("Mouse Middle", MouseButton::Middle),
    ("Mouse Back", MouseButton::Back),
    ("Mouse Forward", MouseButton::Forward),
];

/// The name stored for a mouse button.
pub fn mouse_name(button: MouseButton) -> &'static str {
    MOUSE_NAMES.iter().find(|(_, b)| *b == button).map(|(n, _)| *n).expect("every button has a name")
}

/// A keyboard key, replacing Qt's key codes. [`Key::name`] is the string Qt's
/// `QKeySequence(key).toString()` gives, which is what `controls.json` stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable key, stored upper-case for letters ('W', '1', '`', ...).
    Char(char),
    /// F1..F35.
    F(u8),
    Shift,
    Ctrl,
    Alt,
    Meta,
    Space,
    Esc,
    Tab,
    Backtab,
    Backspace,
    Return,
    Enter,
    Ins,
    Del,
    Pause,
    Print,
    SysReq,
    Clear,
    Home,
    End,
    Left,
    Up,
    Right,
    Down,
    PgUp,
    PgDown,
    CapsLock,
    NumLock,
    ScrollLock,
    Menu,
    Help,
    /// Qt's `Key_unknown`: a name Qt could not read. Qt still accepts it as a
    /// binding, and writes it back as "Key 33554431".
    Unknown,
}

/// Named keys as Qt writes them, plus the aliases Qt reads (case-insensitively).
const NAMED_KEYS: [(&str, Key); 31] = [
    ("Shift", Key::Shift),
    ("Ctrl", Key::Ctrl),
    ("Alt", Key::Alt),
    ("Meta", Key::Meta),
    ("Space", Key::Space),
    ("Esc", Key::Esc),
    ("Tab", Key::Tab),
    ("Backtab", Key::Backtab),
    ("Backspace", Key::Backspace),
    ("Return", Key::Return),
    ("Enter", Key::Enter),
    ("Ins", Key::Ins),
    ("Del", Key::Del),
    ("Pause", Key::Pause),
    ("Print", Key::Print),
    ("SysReq", Key::SysReq),
    ("Clear", Key::Clear),
    ("Home", Key::Home),
    ("End", Key::End),
    ("Left", Key::Left),
    ("Up", Key::Up),
    ("Right", Key::Right),
    ("Down", Key::Down),
    ("PgUp", Key::PgUp),
    ("PgDown", Key::PgDown),
    ("CapsLock", Key::CapsLock),
    ("NumLock", Key::NumLock),
    ("ScrollLock", Key::ScrollLock),
    ("Menu", Key::Menu),
    ("Help", Key::Help),
    ("Unknown", Key::Unknown),
];

const KEY_ALIASES: [(&str, Key); 9] = [
    ("Escape", Key::Esc),
    ("Insert", Key::Ins),
    ("Delete", Key::Del),
    ("Page Up", Key::PgUp),
    ("Page Down", Key::PgDown),
    ("PageUp", Key::PgUp),
    ("PageDown", Key::PgDown),
    ("Control", Key::Ctrl),
    ("Caps Lock", Key::CapsLock),
];

impl Key {
    /// The stored, readable name ("W", "Space", "Shift", "F8").
    pub fn name(self) -> String {
        match self {
            Key::Char(c) => c.to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Unknown => "Key 33554431".to_string(),
            other => NAMED_KEYS.iter().find(|(_, k)| *k == other).map(|(n, _)| n.to_string()).unwrap_or_default(),
        }
    }

    /// Qt's `QKeySequence.fromString(name)[0]` with modifier bits dropped:
    /// None only for a name with no key at all.
    pub fn from_name(name: &str) -> Option<Key> {
        if name.is_empty() {
            return None;
        }
        // "Ctrl+A", "Shift+W", "Num+1": the last part is the key.
        let mut rest = name;
        while let Some((head, tail)) = rest.split_once('+') {
            if tail.is_empty() {
                break;
            }
            let modifier = ["ctrl", "shift", "alt", "meta", "num"].contains(&head.trim().to_ascii_lowercase().as_str());
            if !modifier {
                return Some(Key::Unknown);
            }
            rest = tail;
        }
        Some(Self::single(rest.trim_start()))
    }

    fn single(text: &str) -> Key {
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.clone().next()) {
            return Key::Char(c.to_uppercase().next().filter(|_| c.to_uppercase().count() == 1).unwrap_or(c));
        }
        let lower = text.to_lowercase();
        for (n, k) in NAMED_KEYS.iter().chain(KEY_ALIASES.iter()) {
            if n.to_lowercase() == lower {
                return *k;
            }
        }
        if let Some(digits) = lower.strip_prefix('f')
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = digits.parse::<u8>()
            && (1..=35).contains(&n)
        {
            return Key::F(n);
        }
        Key::Unknown
    }
}

/// `("mouse", button)` or `("key", code)` for a stored binding name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    Mouse(MouseButton),
    Key(Key),
}

/// A readable, storable name for a key ("W", "Space", "Shift").
pub fn key_name(key: Key) -> String {
    key.name()
}

/// What a stored binding name means; None for one that names nothing.
pub fn name_to_input(name: &str) -> Option<Input> {
    if let Some((_, button)) = MOUSE_NAMES.iter().find(|(n, _)| *n == name) {
        return Some(Input::Mouse(*button));
    }
    Key::from_name(name).map(Input::Key)
}

/// Windows' VK_OEM_3: the key under Esc (` and ~ on a US layout).
pub const VK_OEM_3: u32 = 0xC0;

/// The Windows virtual-key code for a binding name, for `RegisterHotKey`:
/// F1-F24, letters, digits and the ` key. None for anything else.
/// (Port of `virtual_key_for` in app/engine.py.)
pub fn virtual_key_for(name: &str) -> Option<u32> {
    let upper = name.to_uppercase();
    if upper.starts_with('F') {
        let digits: String = name.chars().skip(1).collect();
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = digits.parse::<u32>()
                && (1..=24).contains(&n)
            {
                return Some(0x6F + n);
            }
            return None;
        }
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_alphanumeric()
    {
        let up = c.to_uppercase().next().unwrap_or(c);
        return Some(up as u32);
    }
    if name == "`" || name == "~" {
        return Some(VK_OEM_3);
    }
    None
}

// -- settings --------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSettings {
    /// Action -> binding name, in Python's dict order.
    pub bindings: Vec<(Action, String)>,
    pub aim_cone: i64,
    pub face_mouse_when_still: bool,
    pub movement: Movement,
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self {
            bindings: bindings_from(&DEFAULT_BINDINGS),
            aim_cone: DEFAULT_AIM_CONE,
            face_mouse_when_still: false,
            movement: DEFAULT_MOVEMENT,
        }
    }
}

impl ControlSettings {
    // -- lookups ---------------------------------------------------------

    fn action_bound_to(&self, name: &str) -> Option<Action> {
        self.bindings.iter().find(|(_, bound)| bound == name).map(|(a, _)| *a)
    }

    pub fn action_for_key(&self, key: Key) -> Option<Action> {
        self.action_bound_to(&key_name(key))
    }

    pub fn action_for_mouse(&self, button: MouseButton) -> Option<Action> {
        self.action_bound_to(mouse_name(button))
    }

    /// The binding name of `action`, its default when unbound.
    pub fn binding(&self, action: Action) -> &str {
        self.bindings.iter().find(|(a, _)| *a == action).map(|(_, n)| n.as_str()).unwrap_or(default_binding(action))
    }

    pub fn turn_movement(&self) -> bool {
        self.movement == Movement::Turn
    }

    /// (label, what it does) for `action` under the current movement mode.
    pub fn action_text(&self, action: Action) -> (&'static str, &'static str) {
        if self.turn_movement()
            && let Some((_, label, text)) = TURN_ACTION_TEXT.iter().find(|(a, _, _)| *a == action)
        {
            return (label, text);
        }
        ACTIONS.iter().find(|(a, _, _)| *a == action).map(|(_, l, t)| (*l, *t)).expect("every action is listed")
    }

    // -- changes ---------------------------------------------------------

    /// Bind `action` to `name`. Returns the action that lost it, if any: an
    /// action already using `name` takes this action's old binding instead.
    pub fn rebind(&mut self, action: Action, name: &str) -> Option<Action> {
        let previous = self.binding(action).to_string();
        let mut displaced = None;
        for (other, bound) in self.bindings.iter_mut() {
            if *other != action && bound == name {
                *bound = previous.clone();
                displaced = Some(*other);
            }
        }
        if let Some(slot) = self.bindings.iter_mut().find(|(a, _)| *a == action) {
            slot.1 = name.to_string();
        } else {
            self.bindings.push((action, name.to_string()));
        }
        displaced
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Half the aim cone, in radians.
    pub fn half_cone(&self) -> f64 {
        (self.aim_cone.clamp(1, 360) as f64).to_radians() / 2.0
    }

    // -- saving ----------------------------------------------------------

    pub fn to_dict(&self) -> Value {
        let mut bindings = Map::new();
        for (action, name) in &self.bindings {
            bindings.insert(action.id().to_string(), Value::String(name.clone()));
        }
        json!({
            "version": 2,
            "bindings": bindings,
            "aim_cone": self.aim_cone,
            "face_mouse_when_still": self.face_mouse_when_still,
            "movement": self.movement.id(),
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        let mut settings = Self::default();
        let Some(data) = data.as_object() else {
            return settings;
        };
        let mut bindings = data.get("bindings").and_then(Value::as_object).cloned();
        if let Some(b) = &mut bindings {
            // Migration: bindings saved before version 2 put silk and bite on
            // the mouse; they move to Q and F unless those keys are taken.
            let is = |k: &str, v: &str| b.get(k).and_then(Value::as_str) == Some(v);
            let taken =
                b.iter().any(|(k, v)| k != "shoot" && k != "bite" && matches!(v.as_str(), Some("F") | Some("Q")));
            if !data.contains_key("version") && is("shoot", "Mouse Left") && is("bite", "Mouse Right") && !taken {
                b.insert("shoot".into(), Value::String("Q".into()));
                b.insert("bite".into(), Value::String("F".into()));
            }
        }
        if let Some(bindings) = bindings {
            // Explicit saved bindings win over defaults introduced by an update.
            let mut assigned: Vec<(Action, String)> = Vec::new();
            let mut used: Vec<String> = Vec::new();
            for action in ACTION_IDS {
                if let Some(value) = bindings.get(action.id()).and_then(Value::as_str)
                    && !value.is_empty()
                    && !used.iter().any(|u| u == value)
                    && name_to_input(value).is_some()
                {
                    assigned.push((action, value.to_string()));
                    used.push(value.to_string());
                }
            }
            for action in ACTION_IDS {
                if assigned.iter().any(|(a, _)| *a == action) {
                    continue;
                }
                let choices =
                    std::iter::once(default_binding(action).to_string()).chain((1..25).map(|i| format!("F{i}")));
                let value = choices.into_iter().find(|key| !used.contains(key)).expect("a free key");
                used.push(value.clone());
                assigned.push((action, value));
            }
            // Python's dict keeps the order actions were assigned in.
            settings.bindings = assigned;
        }
        let cone = match data.get("aim_cone") {
            None => DEFAULT_AIM_CONE,
            Some(v) => py_int(v).unwrap_or(DEFAULT_AIM_CONE),
        };
        settings.aim_cone = cone.clamp(30, 360);
        settings.face_mouse_when_still = data.get("face_mouse_when_still").is_some_and(truthy);
        settings.movement = data
            .get("movement")
            .map_or(Some(DEFAULT_MOVEMENT), |v| v.as_str().and_then(Movement::from_id))
            .unwrap_or(DEFAULT_MOVEMENT);
        settings
    }
}

pub fn controls_path(state_dir: &Path) -> PathBuf {
    state_dir.join("controls.json")
}

/// The saved controls, or the defaults when the file is missing or broken.
pub fn load_controls(path: &Path) -> ControlSettings {
    match read_json(path) {
        Some(data) => ControlSettings::from_dict(&data),
        None => ControlSettings::default(),
    }
}

/// Save atomically (`.tmp` then rename). False on an OS error.
pub fn save_controls(settings: &ControlSettings, path: &Path) -> bool {
    let text = dumps_indent2(&settings.to_dict()) + "\n";
    atomic_write(path, &text).is_ok()
}

/// The aim angle, pulled to the nearest edge of the cone around `heading`.
pub fn clamp_to_cone(heading: f64, target_angle: f64, half_cone: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let delta = dc_core::math::py_mod(target_angle - heading + PI, TAU) - PI;
    if delta.abs() <= half_cone {
        return target_angle;
    }
    heading + half_cone.copysign(delta)
}

/// (button, label, what it does) for every action, in list order.
pub fn instructions(settings: &ControlSettings) -> Vec<(String, &'static str, &'static str)> {
    ACTION_IDS
        .iter()
        .map(|&action| {
            let (label, text) = settings.action_text(action);
            (settings.binding(action).to_string(), label, text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Port of the data parts of tests/test_adventure_controls.py,
    //! test_map_view_and_release.py, test_turn_movement.py and
    //! test_local_coop.py.
    use super::*;
    use std::collections::HashSet;
    use std::f64::consts::PI;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dc-controls-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("controls.json")
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn every_action_has_its_own_button_by_default() {
        let actions: HashSet<_> = DEFAULT_BINDINGS.iter().map(|(a, _)| *a).collect();
        assert_eq!(actions, ACTION_IDS.iter().copied().collect());
        let names: HashSet<_> = DEFAULT_BINDINGS.iter().map(|(_, n)| *n).collect();
        assert_eq!(names.len(), DEFAULT_BINDINGS.len());
        assert_eq!(ControlSettings::default().aim_cone, 90);
        assert!(!ControlSettings::default().face_mouse_when_still);
    }

    #[test]
    fn binding_names_survive_the_round_trip() {
        for (name, expected) in [
            ("W", Input::Key(Key::Char('W'))),
            ("Space", Input::Key(Key::Space)),
            ("Shift", Input::Key(Key::Shift)),
            ("Esc", Input::Key(Key::Esc)),
            ("Mouse Left", Input::Mouse(MouseButton::Left)),
            ("Mouse Right", Input::Mouse(MouseButton::Right)),
        ] {
            assert_eq!(name_to_input(name), Some(expected), "{name}");
            let back = match expected {
                Input::Key(k) => key_name(k),
                Input::Mouse(b) => mouse_name(b).to_string(),
            };
            assert_eq!(back, name);
        }
    }

    #[test]
    fn key_names_read_like_qt() {
        // Probed from PyQt5: QKeySequence.fromString(name) then toString().
        for (name, back) in [
            ("w", "W"),
            ("space", "Space"),
            ("Escape", "Esc"),
            ("`", "`"),
            ("~", "~"),
            ("F24", "F24"),
            ("F35", "F35"),
            ("Up", "Up"),
            ("Foo", "Key 33554431"),
            ("Ctrl+A", "A"),
            ("Return", "Return"),
            ("Delete", "Del"),
            ("Page Up", "PgUp"),
            ("Num+1", "1"),
            ("\\", "\\"),
            ("+", "+"),
            ("Plus", "Key 33554431"),
            ("A+B", "Key 33554431"),
            ("Shift+W", "W"),
            ("Ctrl", "Ctrl"),
        ] {
            match name_to_input(name) {
                Some(Input::Key(k)) => assert_eq!(key_name(k), back, "{name}"),
                other => panic!("{name}: {other:?}"),
            }
        }
        assert_eq!(name_to_input(""), None);
    }

    #[test]
    fn taking_a_button_in_use_swaps_rather_than_leaving_a_gap() {
        let mut settings = ControlSettings::default();
        let displaced = settings.rebind(Action::Jump, "W");
        assert_eq!(displaced, Some(Action::MoveUp));
        assert_eq!(settings.binding(Action::Jump), "W");
        assert_eq!(settings.binding(Action::MoveUp), "Space");
        let names: HashSet<_> = settings.bindings.iter().map(|(_, n)| n.clone()).collect();
        assert_eq!(names.len(), ACTION_IDS.len());
    }

    #[test]
    fn controls_are_saved_and_read_back() {
        let path = scratch("save");
        let mut settings = ControlSettings::default();
        settings.rebind(Action::Shoot, "F");
        settings.aim_cone = 120;
        settings.face_mouse_when_still = true;
        assert!(save_controls(&settings, &path));
        let loaded = load_controls(&path);
        assert_eq!(loaded.binding(Action::Shoot), "F");
        assert_eq!(loaded.binding(Action::Jump), "Space");
        assert_eq!(loaded.aim_cone, 120);
        assert!(loaded.face_mouse_when_still);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn a_broken_or_hand_edited_file_still_gives_working_controls() {
        let path = scratch("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load_controls(&path).bindings, bindings_from(&DEFAULT_BINDINGS));
        std::fs::write(&path, r#"{"bindings": {"jump": "W", "shoot": "???"}, "aim_cone": "x"}"#).unwrap();
        let loaded = load_controls(&path);
        let names: HashSet<_> = loaded.bindings.iter().map(|(_, n)| n.clone()).collect();
        assert_eq!(names.len(), ACTION_IDS.len(), "{:?}", loaded.bindings);
        assert_eq!(loaded.aim_cone, 90);
        // "W" went to jump, so move_up takes the first free F key.
        assert_eq!(loaded.binding(Action::Jump), "W");
        assert_eq!(loaded.binding(Action::MoveUp), "F1");
    }

    #[test]
    fn the_aim_is_held_to_the_cone() {
        let half = 45f64.to_radians();
        assert!(approx(clamp_to_cone(0.0, 0.3, half), 0.3));
        assert!(approx(clamp_to_cone(0.0, -PI / 2.0, half), -half));
        assert!(approx(clamp_to_cone(0.0, PI * 0.9, half), half));
    }

    #[test]
    fn the_release_key_works_from_anywhere_as_a_windows_hotkey() {
        assert_eq!(virtual_key_for("F8"), Some(0x77));
        assert_eq!(virtual_key_for("F1"), Some(0x70));
        assert_eq!(virtual_key_for("P"), Some('P' as u32));
        assert_eq!(virtual_key_for("p"), Some('P' as u32));
        assert_eq!(virtual_key_for("`"), Some(VK_OEM_3));
        assert_eq!(virtual_key_for("Mouse Left"), None);
        assert_eq!(virtual_key_for("F25"), None);
        assert_eq!(virtual_key_for("F"), Some('F' as u32));
    }

    #[test]
    fn the_new_keys_have_defaults_and_join_old_saved_controls() {
        assert_eq!(default_binding(Action::MapView), "`");
        assert_eq!(default_binding(Action::Release), "F8");
        let mut old = Map::new();
        for (a, n) in DEFAULT_BINDINGS {
            if a != Action::MapView && a != Action::Release {
                old.insert(a.id().into(), Value::String(n.into()));
            }
        }
        let loaded = ControlSettings::from_dict(&json!({ "bindings": old }));
        assert_eq!(loaded.binding(Action::MapView), "`");
        assert_eq!(loaded.binding(Action::Release), "F8");
    }

    #[test]
    fn the_setting_is_saved_and_labels_follow_it() {
        let mut settings = ControlSettings::from_dict(&json!({"movement": "turn"}));
        assert!(settings.turn_movement());
        assert_eq!(ControlSettings::from_dict(&settings.to_dict()).movement, Movement::Turn);
        assert_eq!(ControlSettings::from_dict(&json!({"movement": "sideways"})).movement, Movement::Turn);
        let labels: HashSet<_> = instructions(&settings).into_iter().map(|(_, l, _)| l).collect();
        for want in ["Forward", "Back up", "Turn left", "Turn right"] {
            assert!(labels.contains(want));
        }
        settings.movement = Movement::Screen;
        settings.reset();
        assert_eq!(settings.movement, Movement::Turn);
        let screen = ControlSettings::from_dict(&json!({"movement": "screen"}));
        assert_eq!(screen.action_text(Action::MoveUp).0, "Move up");
    }

    #[test]
    fn defaults_and_legacy_migration() {
        let mut old = Map::new();
        for (a, n) in DEFAULT_BINDINGS {
            old.insert(a.id().into(), Value::String(n.into()));
        }
        old.insert("shoot".into(), json!("Mouse Left"));
        old.insert("bite".into(), json!("Mouse Right"));
        let migrated = ControlSettings::from_dict(&json!({ "bindings": old.clone() }));
        assert_eq!(migrated.binding(Action::Bite), "F");
        assert_eq!(migrated.binding(Action::Shoot), "Q");
        let explicit = ControlSettings::from_dict(&json!({"version": 2, "bindings": old}));
        assert_eq!(explicit.binding(Action::Bite), "Mouse Right");
        assert_eq!(explicit.binding(Action::Shoot), "Mouse Left");
        let p2 = second_player_controls(None);
        assert_eq!(p2.action_for_key(Key::Up), Some(Action::MoveUp));
        assert_eq!(p2.action_for_key(Key::Char('J')), Some(Action::Bite));
        let one: HashSet<_> = DEFAULT_BINDINGS.iter().map(|(_, n)| n.to_string()).collect();
        let two: HashSet<_> = p2.bindings.iter().map(|(_, n)| n.clone()).collect();
        let shared: HashSet<_> = one.intersection(&two).cloned().collect();
        assert_eq!(shared, ["Esc", "`", "F8"].iter().map(|s| s.to_string()).collect());
        assert!(p2.turn_movement(), "both players turn and walk by default");
    }

    #[test]
    fn mouse_bindings_find_their_action() {
        let mut s = ControlSettings::default();
        s.rebind(Action::Shoot, "Mouse Left");
        assert_eq!(s.action_for_mouse(MouseButton::Left), Some(Action::Shoot));
        assert_eq!(s.action_for_mouse(MouseButton::Middle), None);
        assert_eq!(s.half_cone(), 45f64.to_radians());
    }
}
