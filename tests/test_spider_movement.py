"""Deterministic headless checks for the Snowpuff-2 spider gait.

The helpers moved to `movement.py`; what is left here is what the old
`main()` asserted, one named check at a time. Each reseeds first: the original
did too, and the walk is only reproducible from a known seed.
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import (
    build_creature,
    run_causality_checks,
    run_heading_filter_check,
    run_quick_turn_check,
    run_roll_recovery_check,
    run_walk,
)
from support import ROOT

SEED = 19
SECONDS = 8.0
SPEED = 80.0
DT = 1.0 / 60.0


@pytest.fixture(scope="module")
def gait():
    """The model, personality and solved gait configuration under test."""
    random.seed(SEED)
    model = json.loads((ROOT / "models/plush_snow_hybrid_2/model.json").read_text())
    personality = json.loads((ROOT / "personalities/cuddly.json").read_text())
    config = build_creature(model, personality)._spider_gait_config()
    return model, personality, config


def walk(gait, turn_rate: float = 0.0, dt: float = DT) -> dict:
    model, personality, config = gait
    random.seed(SEED)
    return run_walk(model, personality, config, SECONDS, SPEED, turn_rate, dt)


def test_motion_is_leg_first(gait):
    """Guards specifically against reintroducing body-first motion."""
    model, personality, config = gait
    random.seed(SEED)
    run_causality_checks(model, personality, config)


def test_heading_filter_settles(gait):
    model, personality, _config = gait
    random.seed(SEED)
    run_heading_filter_check(model, personality)


def test_legs_replant_after_a_roll(gait):
    model, personality, _config = gait
    random.seed(SEED)
    run_roll_recovery_check(model, personality)


def test_a_quick_turn_stays_bounded(gait):
    model, personality, _config = gait
    random.seed(SEED)
    run_quick_turn_check(model, personality)


def test_a_walk_stays_inside_its_limits(gait):
    _model, _personality, config = gait
    result = walk(gait)
    assert result["starts"] > 0, "no spider gait steps started"
    assert result["max_airborne"] <= config["max_airborne"], result["max_airborne"]
    assert result["planted_displacements"] == 0, result["planted_displacements"]
    assert result["outside"] == 0, (result["outside"], result["starts"])
    assert result["max_chain_stretch"] <= 1.35, result["max_chain_stretch"]
    assert result["max_segment_ratio"] <= 1.001, result["max_segment_ratio"]
    assert result["min_supports"] >= len(gait[0]["legs"]) - config["max_airborne"]


def test_the_body_never_teleports(gait):
    """Pose changes come from the support solve, not from moving the body."""
    result = walk(gait)
    assert result["max_pose_jump"] < max(12.0, SPEED * 0.05)
    assert result["max_heading_jump"] < 0.16


def test_the_body_turns_when_it_is_asked_to(gait):
    """The turning case the old runner only reached with a command-line flag.

    `main()` took `--turn-rate`, defaulted it to zero and only asserted when it
    was non-zero, so the assertion below never ran in CI -- the one check in
    the file that needed an argument was the one nobody passed.
    """
    result = walk(gait, turn_rate=0.6)
    final_turn = abs(((result["creature"].heading + math.pi) % math.tau) - math.pi)
    assert final_turn > 0.50, final_turn


@pytest.mark.parametrize("frame_dt", [1.0 / 30.0, 1.0 / 120.0])
def test_the_walk_agrees_across_frame_rates(gait, frame_dt):
    """Half and double the frame rate must end up in roughly the same place."""
    base = walk(gait)["creature"]
    other = walk(gait, dt=frame_dt)["creature"]
    assert math.hypot(other.x - base.x, other.y - base.y) < 32.0
    assert abs(((other.heading - base.heading + math.pi) % math.tau) - math.pi) < 0.35
