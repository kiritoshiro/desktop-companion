"""Adventure controls: the mouse aims inside a cone, and every button is rebindable.

The owner: *"mouse would not control the movement just the direction for
shooting in that viewing direction lets say 90degree angle ... add the
controls there to change. and instructions what button does what. and
ability to change the settings buttons."*
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest
from PyQt5.QtCore import Qt

from desktop_bug.app import controls as ctl
from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.controls import (ACTION_IDS, DEFAULT_BINDINGS, ControlSettings, clamp_to_cone,
                                      load_controls, name_to_input, save_controls)
from desktop_bug.app.engine import OverlayWindow
from desktop_bug.creature import Creature
from support import load_pair


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Key names and the editor need the one Qt application object."""


def _scratch() -> Path:
    return Path(tempfile.mkdtemp(prefix="dc-controls-")) / "controls.json"


def _spider(heading: float = 0.0) -> Creature:
    model, personality = load_pair()
    spider = Creature(model, personality, 900, 700, index=0, seed=7)
    spider.x, spider.y = 450.0, 350.0
    spider.heading = spider.target_heading = heading
    return spider


# ------------------------------------------------------------ the settings

def test_every_action_has_its_own_button_by_default():
    assert set(DEFAULT_BINDINGS) == set(ACTION_IDS)
    assert len(set(DEFAULT_BINDINGS.values())) == len(DEFAULT_BINDINGS)
    assert ControlSettings().aim_cone == 90
    assert ControlSettings().face_mouse_when_still is False


def test_binding_names_survive_the_round_trip_to_qt():
    for name, expected in (("W", ("key", Qt.Key_W)), ("Space", ("key", Qt.Key_Space)),
                           ("Shift", ("key", Qt.Key_Shift)), ("Esc", ("key", Qt.Key_Escape)),
                           ("Mouse Left", ("mouse", int(Qt.LeftButton))),
                           ("Mouse Right", ("mouse", int(Qt.RightButton)))):
        assert name_to_input(name) == expected, name
        kind, code = expected
        back = ctl.key_name(code) if kind == "key" else ctl.mouse_name(code)
        assert back == name


def test_taking_a_button_in_use_swaps_rather_than_leaving_a_gap():
    settings = ControlSettings()
    displaced = settings.rebind("jump", "W")          # W was move up
    assert displaced == "move_up"
    assert settings.binding("jump") == "W"
    assert settings.binding("move_up") == "Space"
    assert len(set(settings.bindings.values())) == len(ACTION_IDS)


def test_controls_are_saved_and_read_back():
    path = _scratch()
    settings = ControlSettings()
    settings.rebind("shoot", "F")
    settings.aim_cone = 120
    settings.face_mouse_when_still = True
    assert save_controls(settings, path)
    loaded = load_controls(path)
    assert loaded.binding("shoot") == "F"
    assert loaded.binding("jump") == "Space"
    assert loaded.aim_cone == 120 and loaded.face_mouse_when_still is True


def test_a_broken_or_hand_edited_file_still_gives_working_controls():
    path = _scratch()
    path.write_text("{not json", encoding="utf-8")
    assert load_controls(path).bindings == DEFAULT_BINDINGS
    path.write_text(json.dumps({"bindings": {"jump": "W", "shoot": "???"}, "aim_cone": "x"}),
                    encoding="utf-8")
    loaded = load_controls(path)
    assert len(set(loaded.bindings.values())) == len(ACTION_IDS), loaded.bindings
    assert loaded.aim_cone == 90


# --------------------------------------------------------------- aiming

def test_the_aim_is_held_to_the_cone():
    half = math.radians(45)
    assert clamp_to_cone(0.0, 0.3, half) == pytest.approx(0.3)
    assert clamp_to_cone(0.0, -math.pi / 2, half) == pytest.approx(-half)
    assert clamp_to_cone(0.0, math.pi * 0.9, half) == pytest.approx(half)

    player = PlayerController(_spider(heading=0.0))
    player.aim = (450.0, 100.0)                       # straight up
    assert player.aim_angle() == pytest.approx(-half)
    player.controls.aim_cone = 360
    assert player.aim_angle() == pytest.approx(-math.pi / 2)


def _target(spider, angle_deg, dist=150.0):
    a = math.radians(angle_deg)
    return SimpleNamespace(x=spider.x + math.cos(a) * dist, y=spider.y + math.sin(a) * dist, size=12.0)


def test_only_what_is_in_front_can_be_hit():
    spider = _spider(heading=0.0)
    player = PlayerController(spider)
    ahead, off, behind = _target(spider, 0), _target(spider, 65), _target(spider, 180)
    for target in (ahead, off, behind):
        player.aim = (target.x, target.y)
        hit = player._in_front(target, player.WEB_RANGE, 30.0)
        assert (hit is not None) == (target is ahead), target
    player.controls.aim_cone = 180
    player.aim = (off.x, off.y)
    assert player._in_front(off, player.WEB_RANGE, 30.0) is not None, "a wider cone should reach it"


def test_the_mouse_does_not_turn_a_standing_spider_unless_asked():
    spider = _spider(heading=0.0)
    player = PlayerController(spider)
    player.aim = (spider.x, spider.y - 200.0)
    for _ in range(60):
        spider.update(1 / 60, 100, 100, 900, 700)
    assert abs(spider.heading) < 0.2, f"it turned to the mouse: {spider.heading:.2f}"
    player.controls.face_mouse_when_still = True
    for _ in range(90):
        spider.update(1 / 60, 100, 100, 900, 700)
    assert spider.heading < -0.8, f"with the option on it should face up: {spider.heading:.2f}"


def test_rebound_keys_move_the_spider():
    settings = ControlSettings()
    settings.rebind("move_right", "L")
    spider = _spider()
    player = PlayerController(spider, settings)
    player.set_held(Qt.Key_D, True)                   # D no longer does anything
    assert not player.held
    player.set_held(Qt.Key_L, True)
    start = spider.x
    for _ in range(90):
        spider.update(1 / 60, 100, 100, 900, 700)
    assert spider.x > start + 5


# ------------------------------------------------------ the overlay's keys

def _fake_overlay(settings):
    calls = []
    fake = SimpleNamespace(
        mode="adventure", controls=settings, _adventure_paused=False, manager=None,
        player=SimpleNamespace(set_held=lambda a, d: calls.append(("held", a, d)),
                               jump=lambda: calls.append(("jump",)),
                               shoot=lambda m: calls.append(("shoot",)),
                               bite=lambda m: calls.append(("bite",))),
        _show_adventure_pause=lambda: calls.append(("pause",)),
        _show_adventure_skills=lambda: calls.append(("skills",)),
    )
    fake._adventure_action = lambda *a, **k: OverlayWindow._adventure_action(fake, *a, **k)
    return fake, calls


def _key(code, repeat=False):
    return SimpleNamespace(key=lambda: code, isAutoRepeat=lambda: repeat, accept=lambda: None)


def test_the_overlay_follows_the_bindings_and_esc_always_pauses():
    settings = ControlSettings()
    settings.rebind("jump", "J")
    settings.rebind("pause", "P")
    fake, calls = _fake_overlay(settings)
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_J))
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_J, repeat=True))
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_Space))
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_P))
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_Escape))
    OverlayWindow.keyPressEvent(fake, _key(Qt.Key_W))
    OverlayWindow.keyReleaseEvent(fake, _key(Qt.Key_W))
    assert calls == [("jump",), ("pause",), ("pause",),
                     ("held", "move_up", True), ("held", "move_up", False)], calls


# ---------------------------------------------------------- the settings UI

def test_the_editor_rebinds_saves_and_resets():
    from desktop_bug.app.controls_ui import ControlsEditor

    path = _scratch()
    editor = ControlsEditor(path=path)
    assert set(editor.buttons) == set(ACTION_IDS)
    editor.buttons["shoot"].captured.emit("shoot", "Mouse Middle")
    assert load_controls(path).binding("shoot") == "Mouse Middle"
    assert editor.buttons["shoot"].text() == "Mouse Middle"
    editor.cone.setCurrentIndex(editor.cone.findData(60))
    assert load_controls(path).aim_cone == 60
    editor.reset()
    assert load_controls(path).bindings == DEFAULT_BINDINGS
    assert load_controls(path).aim_cone == 90


def test_the_adventure_page_shows_the_essential_buttons(monkeypatch):
    """One short line from the saved bindings; the full list is in Controls.

    The owner: "the instructions could be smaller too, and maybe
    unnecessary at all."
    """
    from PyQt5.QtWidgets import QLabel

    from desktop_bug.app.mode_menu import ModeShell

    scratch = Path(tempfile.mkdtemp(prefix="dc-controls-state-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch))
    settings = ControlSettings()
    settings.rebind("shoot", "F")
    save_controls(settings)
    shell = ModeShell(QLabel("editor"), lambda: None)
    text = shell.controls_line.text()
    assert "<b>F</b> shoot silk" in text
    assert "<b>WASD</b> walk" in text
    for label in ("sprint", "jump", "bite", "pause menu", "90° cone"):
        assert label in text, label
    assert shell.controls_button.text().startswith("Controls")
