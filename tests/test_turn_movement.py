"""Turn-and-walk controls: S backs the spider up.

The owner: *"i want my spider ability to walk backwards. so maybe lets add
another option in controls instead of arrows walk direction it would be turn
based. so S would walk backwards."* A tarantula does back up, slowly and for
short distances, so backing is slower than walking and cannot sprint.
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import build_creature
from support import ROOT
from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.controls import ControlSettings, instructions
from desktop_bug.content.body_plans import resolve_body_plan

DT = 1.0 / 60.0


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and the controls editor need the one Qt application."""


def _player(movement="turn"):
    model = resolve_body_plan(json.loads((ROOT / "models/tarantula/model.json").read_text()))
    personality = json.loads((ROOT / "personalities/bold.json").read_text())
    random.seed(11)
    spider = build_creature(model, personality)
    spider.target_x, spider.target_y = spider.x, spider.y
    player = PlayerController(spider, ControlSettings(movement=movement))
    for _ in range(30):
        player.update(DT)
    return spider, player


def _hold(player, actions, seconds):
    player.held = set(actions)
    for _ in range(round(seconds / DT)):
        player.update(DT)
    player.held = set()


def _turned(a, b):
    return abs((b - a + math.pi) % math.tau - math.pi)


def test_w_walks_forward_the_way_it_faces():
    spider, player = _player()
    x0, y0, h0 = spider.x, spider.y, spider.heading
    _hold(player, {"move_up"}, 1.5)
    ahead = (spider.x - x0) * math.cos(h0) + (spider.y - y0) * math.sin(h0)
    assert ahead > 60.0, ahead
    assert _turned(h0, spider.heading) < 0.1


def test_s_backs_up_slowly_still_facing_forward():
    spider, player = _player()
    x0, y0, h0 = spider.x, spider.y, spider.heading
    _hold(player, {"move_down"}, 2.0)
    along = (spider.x - x0) * math.cos(h0) + (spider.y - y0) * math.sin(h0)
    assert along < -25.0, f"it should have backed up, moved {along:.1f} along its heading"
    assert _turned(h0, spider.heading) < 0.15, "it turned round instead of backing up"
    player.update(DT)
    assert not spider.reverse_walk, "the flag clears once S is let go"

    forward, _ = _player()
    fx0 = forward.x
    _hold(_, {"move_up"}, 2.0)
    assert abs(along) < (forward.x - fx0) * 0.6, "backing up should be slower than walking"


def test_backing_up_cannot_sprint():
    spider, player = _player()
    energy = spider.energy
    _hold(player, {"move_down", "sprint"}, 1.0)
    assert spider.energy >= energy - 1e-6


def test_a_and_d_turn_on_the_spot():
    spider, player = _player()
    x0, y0, h0 = spider.x, spider.y, spider.heading
    _hold(player, {"move_left"}, 1.0)
    assert _turned(h0, spider.heading) > 0.5
    left = ((spider.heading - h0 + math.pi) % math.tau) - math.pi
    assert left < 0.0, "A turns anticlockwise on screen"
    assert math.hypot(spider.x - x0, spider.y - y0) < spider.size * 1.5
    h1 = spider.heading
    _hold(player, {"move_right"}, 1.0)
    assert ((spider.heading - h1 + math.pi) % math.tau) - math.pi > 0.3


def test_w_and_d_walk_an_arc():
    spider, player = _player()
    h0 = spider.heading
    x0, y0 = spider.x, spider.y
    _hold(player, {"move_up", "move_right"}, 1.5)
    assert _turned(h0, spider.heading) > 0.6
    assert math.hypot(spider.x - x0, spider.y - y0) > 40.0


def test_screen_directions_are_unchanged():
    spider, player = _player(movement="screen")
    y0 = spider.y
    _hold(player, {"move_down"}, 1.5)
    assert spider.y - y0 > 60.0, "S walks down the screen in screen mode"
    assert not spider.reverse_walk


def test_the_setting_is_saved_and_labels_follow_it():
    settings = ControlSettings.from_dict({"movement": "turn"})
    assert settings.turn_movement
    assert ControlSettings.from_dict(settings.to_dict()).movement == "turn"
    assert ControlSettings.from_dict({"movement": "sideways"}).movement == "screen"
    labels = {label for _, label, _ in instructions(settings)}
    assert {"Forward", "Back up", "Turn left", "Turn right"} <= labels
    settings.reset()
    assert settings.movement == "screen"


def test_the_controls_editor_offers_it(tmp_path_factory):
    from desktop_bug.app.controls_ui import ControlsEditor

    path = tmp_path_factory.mktemp("controls") / "controls.json"
    editor = ControlsEditor(ControlSettings(), path=path)
    editor.movement.setCurrentIndex(editor.movement.findData("turn"))
    assert editor.settings.movement == "turn"
    assert editor.action_labels["move_down"][0].text() == "<b>Back up</b>"
    assert not editor.face_mouse.isEnabled()
    assert json.loads(path.read_text())["movement"] == "turn"


def test_backing_up_keeps_its_feet_planted():
    """Measured: 53 steps in 3 s of backing, 3 of them emergencies, no slides."""
    spider, player = _player()
    player.held = {"move_down"}
    was = [leg.stepping or leg.pending_step for leg in spider.legs]
    starts = emergencies = slides = 0
    for _ in range(180):
        planted = {id(leg): (leg.foot_x, leg.foot_y) for leg in spider.legs
                   if leg.contact_state == "stance" and not leg.stepping and not leg.pending_step}
        player.update(DT)
        for i, leg in enumerate(spider.legs):
            now = leg.stepping or leg.pending_step
            if now and not was[i]:
                starts += 1
                emergencies += int(bool(leg.last_step_emergency))
            if id(leg) in planted and not now and not was[i] and leg.contact_state == "stance":
                old = planted[id(leg)]
                slides += math.hypot(leg.foot_x - old[0], leg.foot_y - old[1]) > 1e-6
            was[i] = now
    assert starts > 20, "backing up should step"
    assert slides == 0, "a planted foot slid"
    assert emergencies <= starts * 0.15, (emergencies, starts)
