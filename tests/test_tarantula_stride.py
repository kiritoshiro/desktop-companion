"""A tarantula goes faster by stepping more often, not farther.

The reference note (Tarantula Reference -- Brachypelma hamorii): *Aphonopelma
hentzi* raised sprint speed 2.5-fold by changing stride frequency while stride
length stayed roughly constant. The gait did the opposite. Measured on main
before this change, walking straight:

    speed  steps/s/leg  stride
       60         2.67   22.5 px
      100         3.65   27.4 px
      150         4.88   30.7 px
      200         5.00   40.0 px   (every step an emergency replant)

The model now sets a stride length, and the cadence follows the speed.
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import advance_controller, build_creature
from support import ROOT
from desktop_bug.content.body_plans import resolve_body_plan

DT = 1.0 / 60.0


@pytest.fixture(scope="module")
def tarantula():
    model = resolve_body_plan(json.loads((ROOT / "models/tarantula/model.json").read_text()))
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    return model, personality


def _walk(tarantula, speed: float) -> dict:
    random.seed(19)
    creature = build_creature(*tarantula)
    creature.speed = speed
    creature.target_x = 20000.0
    config = creature._spider_gait_config()
    for _ in range(90):  # up to speed
        advance_controller(creature, DT, config)
    x0 = creature.x
    was = [leg.stepping or leg.pending_step for leg in creature.legs]
    starts = emergencies = 0
    frames = 360
    for _ in range(frames):
        advance_controller(creature, DT, config)
        for i, leg in enumerate(creature.legs):
            now = leg.stepping or leg.pending_step
            if now and not was[i]:
                starts += 1
                emergencies += int(bool(leg.last_step_emergency))
            was[i] = now
    seconds = frames * DT
    actual = (creature.x - x0) / seconds
    frequency = starts / len(creature.legs) / seconds
    return {"speed": actual, "frequency": frequency,
            "stride": actual / frequency, "emergency": emergencies / max(1, starts)}


def test_speed_comes_from_step_frequency_not_stride(tarantula):
    walks = [_walk(tarantula, speed) for speed in (60.0, 100.0, 150.0, 200.0)]
    for walk, asked in zip(walks, (60.0, 100.0, 150.0, 200.0)):
        assert walk["speed"] > asked * 0.97, f"asked {asked}, walked {walk['speed']:.1f}"
    strides = [walk["stride"] for walk in walks]
    assert max(strides) < min(strides) * 1.10, f"stride changed with speed: {strides}"
    # A third of the speed took over three times the steps.
    assert walks[-1]["frequency"] > walks[0]["frequency"] * 3.0, walks
    # The steps come from the cadence, not from feet running out of reach.
    for walk in walks:
        assert walk["emergency"] <= 0.05, walk


def test_other_spiders_keep_their_gait(tarantula):
    """The new settings default to the old gait for every other model."""
    model, personality = tarantula
    config = build_creature(model, personality)._spider_gait_config()
    assert config["stride_length"] > 0.0
    for path in ROOT.glob("models/*/model.json"):
        if path.parent.name == "tarantula":
            continue
        other = resolve_body_plan(json.loads(path.read_text()))
        gait = (other.get("appearance") or {}).get("spider_gait") or {}
        assert "stride_length" not in gait, path.parent.name
    assert math.isclose(config["min_swing_time"], 0.04)
