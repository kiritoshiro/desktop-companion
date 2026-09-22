"""Regression checks for Snowpuff-2's Drifter motion and leg envelope."""

from __future__ import annotations

import json
import math
import random

import pytest
from desktop_bug.creature import Creature
from desktop_bug.content.body_plans import resolve_body_plan
from support import ROOT

SEED = 7


@pytest.fixture
def drifter() -> Creature:
    model = resolve_body_plan(json.loads(
        (ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text(encoding="utf-8")
    ))
    personality = json.loads(
        (ROOT / "personalities" / "drifter.json").read_text(encoding="utf-8")
    )
    random.seed(SEED)
    spider = Creature(model, personality, 2400, 1400, gait_style="skitter")
    spider.x, spider.y = 1200.0, 700.0
    spider.heading = spider.target_heading = 0.0
    spider._initialize_legs()
    return spider


def test_every_socket_sits_on_the_lateral_edge(drifter):
    """The fluffy shell is intentionally broad, so no socket may sit inside it."""
    for leg in drifter.legs:
        assert float(leg.definition.get("attach_side", 0.0)) >= 0.50


def test_the_neutral_stance_stays_long(drifter):
    min_stance_radius = min(
        math.hypot(leg.foot_x - drifter.x, leg.foot_y - drifter.y)
        for leg in drifter.legs
    )
    assert min_stance_radius >= drifter.size * 1.35, min_stance_radius


def test_a_drifter_never_moonwalks(drifter):
    """It may carry momentum through a curve, but not travel behind its facing."""
    drifter.enter_drift_run("circle")
    minimum_forward_component = 1.0
    for _ in range(480):
        dt = 1.0 / 60.0
        drifter._update_drift_run(dt, -1000.0, -1000.0)
        drifter._move_body(dt)
        drifter._update_legs(dt)
        speed = math.hypot(drifter.vel_x, drifter.vel_y)
        if speed > 35.0:
            fx, fy, _, _ = drifter._basis()
            minimum_forward_component = min(
                minimum_forward_component,
                (drifter.vel_x * fx + drifter.vel_y * fy) / speed,
            )
    assert minimum_forward_component >= math.cos(math.radians(78.0)) - 1e-6, (
        minimum_forward_component
    )
