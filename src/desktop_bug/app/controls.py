"""Adventure controls: which key or mouse button does what, and how aiming works.

The owner asked for the mouse to stop steering the spider and only aim its
silk, within the direction the spider is looking -- "lets say 90degree
angle" -- and for every button to be listed, explained and changeable.

- Movement is keys only. The spider faces where it walks; by default it no
  longer turns to face the pointer while standing still.
- The pointer aims inside a cone around that facing (``aim_cone``, total
  degrees; 90 by default, 360 for free aim). A pointer outside the cone is
  clamped to its nearest edge, so a shot always leaves the spider's front.
- Bindings are stored by name ("W", "Shift", "Space", "Mouse Left"), in
  ``controls.json`` in the state folder: they belong to the player, not to a
  preset, and both the settings window and a running overlay read the same
  file.
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, field
from pathlib import Path

from ..content.discovery import state_dir

# (id, label, what it does). The order is the order they are listed in.
ACTIONS = (
    ("move_up", "Move up", "Walk up the screen."),
    ("move_down", "Move down", "Walk down the screen."),
    ("move_left", "Move left", "Walk left."),
    ("move_right", "Move right", "Walk right."),
    ("sprint", "Sprint", "Hold while moving to run faster. Drains stamina."),
    ("jump", "Jump", "Pounce forward in the direction you are walking."),
    ("shoot", "Shoot silk", "Fire one silk charge along your aim, even if it misses. Refill at home or the Silk loom."),
    ("bite", "Bite", "Bite a foe in front of you, inside the aim cone."),
    ("companion_follow", "Scout: follow", "Your companion follows and fights nearby enemies."),
    ("companion_defend", "Scout: defend here", "Your companion holds its current position and protects the area."),
    ("companion_attack", "Scout: attack target", "Point at an enemy and order your companion to attack it."),
    ("skills", "Character & armour", "Open your character: armour, skills and upgrades."),
    ("pause", "Pause menu", "Pause, save, change settings or release the spider. Esc always pauses too."),
)
ACTION_IDS = tuple(action for action, _, _ in ACTIONS)

DEFAULT_BINDINGS = {
    "move_up": "W",
    "move_down": "S",
    "move_left": "A",
    "move_right": "D",
    "sprint": "Shift",
    "jump": "Space",
    "shoot": "Mouse Left",
    "bite": "Mouse Right",
    "companion_follow": "1",
    "companion_defend": "2",
    "companion_attack": "3",
    "skills": "K",
    "pause": "Esc",
}

# How W/A/S/D move the spider. "screen": up/left/down/right on the screen.
# "turn": forward, back up, turn left, turn right -- the owner wanted to walk
# backwards ("S would walk backwards"). A tarantula does back up, slowly and
# for short distances, so backing is slow and cannot sprint.
MOVEMENT_MODES = (("screen", "Screen directions"), ("turn", "Turn and walk"))
MOVEMENT_IDS = tuple(mode for mode, _ in MOVEMENT_MODES)
DEFAULT_MOVEMENT = "screen"
TURN_ACTION_TEXT = {
    "move_up": ("Forward", "Walk forward, the way your spider is facing."),
    "move_down": ("Back up", "Walk slowly backwards, still facing forward. No sprint."),
    "move_left": ("Turn left", "Turn anticlockwise, on the spot or while walking."),
    "move_right": ("Turn right", "Turn clockwise, on the spot or while walking."),
}

AIM_CONES = (60, 90, 120, 180, 360)
DEFAULT_AIM_CONE = 90

MOUSE_NAMES = {"Mouse Left": 1, "Mouse Right": 2, "Mouse Middle": 4,
               "Mouse Back": 8, "Mouse Forward": 16}      # Qt.MouseButton values
_MODIFIER_NAMES = {0x01000020: "Shift", 0x01000021: "Ctrl", 0x01000023: "Alt",
                   0x01000022: "Meta"}                    # Qt.Key_Shift etc.


def key_name(key: int) -> str:
    """A readable, storable name for a Qt key code ("W", "Space", "Shift")."""
    if key in _MODIFIER_NAMES:
        return _MODIFIER_NAMES[key]
    from PyQt5.QtGui import QKeySequence

    return QKeySequence(int(key)).toString() or f"Key {int(key)}"


def mouse_name(button: int) -> str | None:
    for name, value in MOUSE_NAMES.items():
        if int(button) == value:
            return name
    return None


def name_to_input(name: str):
    """("mouse", button) or ("key", code) for a stored binding name."""
    if name in MOUSE_NAMES:
        return "mouse", MOUSE_NAMES[name]
    for code, text in _MODIFIER_NAMES.items():
        if name == text:
            return "key", code
    from PyQt5.QtGui import QKeySequence

    sequence = QKeySequence.fromString(name)
    if sequence.count() == 0:
        return None
    return "key", int(sequence[0]) & ~0xFE000000   # drop any modifier bits


@dataclass
class ControlSettings:
    bindings: dict = field(default_factory=lambda: dict(DEFAULT_BINDINGS))
    aim_cone: int = DEFAULT_AIM_CONE
    face_mouse_when_still: bool = False
    movement: str = DEFAULT_MOVEMENT

    # -- lookups ---------------------------------------------------------
    def action_for_key(self, key: int) -> str | None:
        name = key_name(key)
        for action, bound in self.bindings.items():
            if bound == name:
                return action
        return None

    def action_for_mouse(self, button: int) -> str | None:
        name = mouse_name(button)
        if name is None:
            return None
        for action, bound in self.bindings.items():
            if bound == name:
                return action
        return None

    def binding(self, action: str) -> str:
        return self.bindings.get(action, DEFAULT_BINDINGS[action])

    @property
    def turn_movement(self) -> bool:
        return self.movement == "turn"

    def action_text(self, action: str) -> tuple[str, str]:
        """(label, what it does) for ``action`` under the current movement mode."""
        if self.turn_movement and action in TURN_ACTION_TEXT:
            return TURN_ACTION_TEXT[action]
        for known, label, text in ACTIONS:
            if known == action:
                return label, text
        raise KeyError(action)

    # -- changes ---------------------------------------------------------
    def rebind(self, action: str, name: str) -> str | None:
        """Bind ``action`` to ``name``. Returns the action that lost it, if any.

        A button can only do one thing, so an action already using ``name``
        takes this action's old binding instead: a swap, never a gap.
        """
        if action not in ACTION_IDS:
            raise KeyError(action)
        previous = self.binding(action)
        displaced = None
        for other, bound in self.bindings.items():
            if other != action and bound == name:
                self.bindings[other] = previous
                displaced = other
        self.bindings[action] = name
        return displaced

    def reset(self) -> None:
        self.bindings = dict(DEFAULT_BINDINGS)
        self.aim_cone = DEFAULT_AIM_CONE
        self.face_mouse_when_still = False
        self.movement = DEFAULT_MOVEMENT

    @property
    def half_cone(self) -> float:
        return math.radians(max(1, min(360, int(self.aim_cone)))) / 2.0

    # -- saving ----------------------------------------------------------
    def to_dict(self) -> dict:
        return {"bindings": dict(self.bindings), "aim_cone": int(self.aim_cone),
                "face_mouse_when_still": bool(self.face_mouse_when_still),
                "movement": self.movement}

    @classmethod
    def from_dict(cls, data) -> "ControlSettings":
        settings = cls()
        if not isinstance(data, dict):
            return settings
        bindings = data.get("bindings")
        if isinstance(bindings, dict):
            # Explicit saved bindings win over defaults introduced by an update.
            # In particular, a player may already use 1/2/3 for another action.
            assigned = {}
            used = set()
            for action in ACTION_IDS:
                value = bindings.get(action)
                if (isinstance(value, str) and value and value not in used
                        and name_to_input(value) is not None):
                    assigned[action] = value
                    used.add(value)
            for action in ACTION_IDS:
                if action in assigned:
                    continue
                choices = [DEFAULT_BINDINGS[action]] + [f"F{i}" for i in range(1, 25)]
                value = next(key for key in choices if key not in used)
                assigned[action] = value
                used.add(value)
            settings.bindings = assigned
        try:
            cone = int(data.get("aim_cone", DEFAULT_AIM_CONE))
        except (TypeError, ValueError):
            cone = DEFAULT_AIM_CONE
        settings.aim_cone = max(30, min(360, cone))
        settings.face_mouse_when_still = bool(data.get("face_mouse_when_still", False))
        movement = data.get("movement", DEFAULT_MOVEMENT)
        settings.movement = movement if movement in MOVEMENT_IDS else DEFAULT_MOVEMENT
        return settings


def controls_path() -> Path:
    return state_dir() / "controls.json"


def load_controls(path: Path | None = None) -> ControlSettings:
    path = Path(path) if path is not None else controls_path()
    try:
        with path.open("r", encoding="utf-8") as handle:
            return ControlSettings.from_dict(json.load(handle))
    except (OSError, ValueError):
        return ControlSettings()


def save_controls(settings: ControlSettings, path: Path | None = None) -> bool:
    path = Path(path) if path is not None else controls_path()
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        temp = path.with_suffix(".tmp")
        with temp.open("w", encoding="utf-8") as handle:
            json.dump(settings.to_dict(), handle, indent=2)
            handle.write("\n")
        temp.replace(path)
        return True
    except OSError:
        return False


def clamp_to_cone(heading: float, target_angle: float, half_cone: float) -> float:
    """The aim angle, pulled to the nearest edge of the cone around ``heading``."""
    delta = (target_angle - heading + math.pi) % math.tau - math.pi
    if abs(delta) <= half_cone:
        return target_angle
    return heading + math.copysign(half_cone, delta)


def instructions(settings: ControlSettings) -> list[tuple[str, str, str]]:
    """(button, label, what it does) for every action, in list order."""
    return [(settings.binding(action), *settings.action_text(action)) for action in ACTION_IDS]
