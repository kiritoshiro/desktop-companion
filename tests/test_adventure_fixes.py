"""Two fixes the owner asked for after the first Adventure mission.

- *"when attacking, the body detaches from back four legs. it also should
  move connected"*: the bite shifts the drawn body by `combat_body_offset`,
  and the leg roots stayed where the undisplaced body was.
- *"once clicking start adventure that whole window should hide"*.
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import advance_controller, build_creature
from support import ROOT
from desktop_bug.content.body_plans import resolve_body_plan
from desktop_bug.creature.constants import STRIKE_SNAP, STRIKE_WINDUP


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Widgets and sprites need the one Qt application object."""


def _tarantula():
    model = resolve_body_plan(json.loads((ROOT / "models/tarantula/model.json").read_text()))
    personality = json.loads((ROOT / "personalities/bold.json").read_text())
    random.seed(4)
    spider = build_creature(model, personality)
    spider.speed = 0.0
    spider.target_x, spider.target_y = spider.x, spider.y
    config = spider._spider_gait_config()
    for _ in range(60):
        advance_controller(spider, 1.0 / 60.0, config)
    return spider


def test_the_legs_stay_on_the_body_through_a_bite():
    spider = _tarantula()
    spider.strike_face = (1.0, 0.0)
    spider.strike_clock = STRIKE_WINDUP + STRIKE_SNAP   # the snap's furthest reach
    ox, oy = spider.combat_body_offset()
    assert math.hypot(ox, oy) > spider.size * 0.5, "the bite should move the body"
    feet_before = [(leg.foot_x, leg.foot_y) for leg in spider.legs]
    for leg in spider.legs:
        ax, ay = spider._leg_attach(leg)
        drawn_x, drawn_y = spider._leg_draw_points(leg)[:2]
        # Each root keeps its place on the body, which is drawn at x + ox.
        assert math.isclose(drawn_x - (spider.x + ox), ax - spider.x, abs_tol=1e-6)
        assert math.isclose(drawn_y - (spider.y + oy), ay - spider.y, abs_tol=1e-6)
    assert feet_before == [(leg.foot_x, leg.foot_y) for leg in spider.legs], "feet stay planted"


def test_without_a_bite_nothing_moves():
    spider = _tarantula()
    spider.strike_clock = None
    spider.lunge = 0.0
    for leg in spider.legs:
        assert spider._leg_draw_points(leg)[:2] == spider._leg_attach(leg)


# ------------------------------------------------------------ the window

@pytest.fixture
def window(monkeypatch):
    from desktop_bug.app.config_ui import ConfigWindow

    window = ConfigWindow()
    running = {"value": False}
    monkeypatch.setattr(window, "_overlay_running", lambda: running["value"])
    window.show()
    yield window, running
    window._hidden_for_adventure = False
    window.close()


def test_starting_adventure_hides_the_window_and_it_returns_after(window, monkeypatch):
    window, running = window

    def launched(mode="companion"):
        running["value"] = True
        window._overlay_mode = mode

    monkeypatch.setattr(window, "launch_engine", launched)
    window.start_adventure()
    assert not window.isVisible(), "the window should hide for Adventure"
    window.update_process_status()
    assert not window.isVisible(), "it stays hidden while Adventure runs"
    running["value"] = False                     # the Adventure overlay exits
    window.update_process_status()
    assert window.isVisible(), "the window should come back"


def test_a_failed_launch_leaves_the_window_showing(window, monkeypatch):
    window, running = window
    monkeypatch.setattr(window, "launch_engine", lambda mode="companion": None)
    window.start_adventure()
    assert window.isVisible()
