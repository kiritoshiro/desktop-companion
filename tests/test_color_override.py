"""A preset may recolour a model, without reaching into the model itself."""

from __future__ import annotations

import copy
import json

import pytest
from desktop_bug.creature import Creature
from desktop_bug.preset_io import validate_preset
from support import ROOT

PALETTE = {"body": [220, 40, 80], "leg_band": [40, 200, 160]}
PERSONALITY = {
    "id": "curious",
    "speed_multiplier": 1.0,
    "reaction_radius": 120,
    "boldness": 0.5,
    "wander_frequency": 0.0,
}


def preset(colors=None) -> dict:
    slot = {"model": "tarantula", "personality": "curious", "count": 1}
    if colors is not None:
        slot["colors"] = colors
    return {"name": "Color smoke", "slots": [slot]}


def test_a_slot_may_omit_colors_entirely():
    validate_preset(preset())


def test_a_well_formed_palette_is_accepted():
    validate_preset(preset(PALETTE))


@pytest.mark.parametrize(
    "colors",
    [
        pytest.param({"body": [1, 2]}, id="too-few-channels"),
        pytest.param({"body": [1, True, 3]}, id="a-bool-is-not-a-channel"),
        pytest.param({"body": [1, 2, 300]}, id="out-of-range"),
    ],
)
def test_a_malformed_palette_is_rejected(colors):
    with pytest.raises(ValueError):
        validate_preset(preset(colors))


def test_an_override_reaches_the_creature_without_being_shared():
    model = json.loads((ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    original = copy.deepcopy(model["colors"])
    first = Creature(model, PERSONALITY, 800, 600, color_overrides=PALETTE)
    second = Creature(model, PERSONALITY, 800, 600, color_overrides=PALETTE)

    assert first.colors["body"] == PALETTE["body"], "override did not reach Creature.colors"
    assert first.colors["leg_band"] == PALETTE["leg_band"]
    assert model["colors"] == original, "color override mutated shared model metadata"

    # Two creatures from the same palette must not share the list behind it.
    first.colors["body"][0] = 0
    assert second.colors["body"][0] == PALETTE["body"][0], "creatures share a mutable palette"
