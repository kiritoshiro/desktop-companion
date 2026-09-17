"""The five added procedural models must really differ from one another."""

from __future__ import annotations

import pytest
from desktop_bug.discovery import discover_models
from support import ROOT

EXPECTED = {
    "mini_marble": {"size": "small", "fluffy": False},
    "velvet_cloud": {"size": "medium", "fluffy": True},
    "giant_copper": {"size": "large", "fluffy": True},
    "sunset_fuzzball": {"size": "large", "fluffy": True},
    "blue_jewel": {"size": "large", "fluffy": False},
}


@pytest.fixture(scope="module")
def models():
    found, warnings = discover_models(ROOT)
    assert not warnings, "model discovery warnings: " + " | ".join(warnings)
    return found


@pytest.mark.parametrize("model_id", sorted(EXPECTED))
def test_a_variation_is_procedural_and_fully_legged(models, model_id):
    model = models.get(model_id)
    assert model is not None, f"missing new model: {model_id}"
    assert model.get("render_mode") == "procedural", (
        f"{model_id} must use the recolorable procedural renderer"
    )
    assert len(model.get("legs", [])) == 8, f"{model_id} must have eight articulated legs"
    appearance = model.get("appearance", {})
    assert appearance.get("segmented_legs"), f"{model_id} is missing segmented legs"
    assert appearance.get("leg_connections", {}).get("enabled"), (
        f"{model_id} is missing visible segmented leg roots"
    )


@pytest.mark.parametrize("model_id", sorted(EXPECTED))
def test_a_variation_is_as_fluffy_as_it_claims(models, model_id):
    fluffiness = float(models[model_id].get("appearance", {}).get("fluffiness", 0.0))
    if EXPECTED[model_id]["fluffy"]:
        assert fluffiness >= 0.75, f"{model_id} should be a fluffy variation"
    else:
        assert fluffiness <= 0.40, f"{model_id} should be a sleeker variation"


def test_the_set_covers_a_range_of_sizes(models):
    sizes = {model_id: models[model_id]["base_size"] for model_id in EXPECTED}
    assert sizes["mini_marble"] < sizes["blue_jewel"] < sizes["giant_copper"], (
        f"size range is not represented: {sizes}"
    )
