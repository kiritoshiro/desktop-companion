"""Deterministic geometry and locomotion checks for the Chosen One model."""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import build_creature, run_causality_checks, run_walk
from support import ROOT

SEED = 19
SECONDS = 5.0
DT = 1.0 / 60.0


@pytest.fixture(scope="module")
def chosen():
    model = json.loads((ROOT / "models/chosen_one/model.json").read_text())
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    return model, personality


@pytest.fixture(scope="module")
def config(chosen):
    random.seed(SEED)
    return build_creature(*chosen)._spider_gait_config()


def test_the_model_is_shaped_as_the_gait_expects(chosen):
    model, _personality = chosen
    legs = model["legs"]
    appearance = model["appearance"]
    gait = appearance["spider_gait"]
    chain = appearance["leg_chain"]

    assert model["id"] == "chosen_one"
    assert model["render_mode"] == "procedural"
    assert len(legs) == 8
    assert len({round(float(leg["phase_offset"]), 4) for leg in legs}) == 8
    assert chain["enabled"] and chain["segment_count"] == 5
    assert len(chain["joint_phase_offsets"]) == 4
    assert len(chain["bend_directions"]) == 4
    assert chain["bend_directions"][-1] < 0.0
    assert gait["profile"] == "chosen_one"
    assert gait["max_airborne"] == 2


def test_motion_is_leg_first(chosen, config):
    random.seed(SEED)
    run_causality_checks(*chosen, config)


def test_the_drawn_chain_has_every_joint(chosen):
    random.seed(SEED)
    probe = build_creature(*chosen)
    preview_chain = probe._sprite_leg_chain_config()
    preview_points = probe._sprite_leg_chain_points(
        probe.legs[0], *probe._leg_attach(probe.legs[0]),
        *probe._visual_foot_for_render(probe.legs[0]), preview_chain
    )
    assert len(preview_points) == 6, len(preview_points)


@pytest.mark.parametrize(
    "speed,turn_rate",
    [(40.0, 0.0), (90.0, 0.0), (180.0, 0.0), (90.0, 1.2), (180.0, -1.2)],
)
def test_a_walk_stays_inside_its_limits(chosen, config, speed, turn_rate):
    model, personality = chosen
    random.seed(SEED)
    result = run_walk(model, personality, config, SECONDS, speed, turn_rate, DT)
    assert result["starts"] > 0
    assert result["outside"] == 0
    assert result["max_airborne"] <= config["max_airborne"]
    assert result["min_supports"] >= 6
    assert result["planted_displacements"] == 0
    assert result["max_chain_stretch"] <= 1.30
    assert result["max_segment_ratio"] <= 1.001
    assert result["max_joint_bend_range"] > 0.08
    assert result["max_joint_motion_spread"] > 1e-5
    assert result["max_pose_jump"] < max(14.0, speed * 0.05)
    assert result["max_heading_jump"] < 0.11
    if turn_rate:
        final_turn = abs(((result["creature"].heading + math.pi) % math.tau) - math.pi)
        assert final_turn > 0.50


@pytest.mark.parametrize("frame_dt", [1.0 / 30.0, 1.0 / 120.0])
def test_the_walk_agrees_across_frame_rates(chosen, config, frame_dt):
    model, personality = chosen

    def end_of_walk(dt):
        random.seed(SEED)
        return run_walk(model, personality, config, SECONDS, 90.0, 0.0, dt)["creature"]

    base = end_of_walk(DT)
    other = end_of_walk(frame_dt)
    assert math.hypot(other.x - base.x, other.y - base.y) < 8.0
    assert abs(((other.heading - base.heading + math.pi) % math.tau) - math.pi) < 0.12
