"""Every spider is built on one of four body plans (DC-49).

Forty-nine models carried forty-one leg rigs, nearly all hand-tuned copies
that nothing kept in step. Four plans now cover forty-six of them, and three
models keep rigs of their own because they genuinely do not fit one.

The first attempt claimed *three* plans covering all forty-nine, from a
metric that compared raw parameter differences. That was wrong, and the way
it was wrong is the reason for `test_the_classification_metric_is_scale_free`
below: across the fleet `attach_angle` has a standard deviation of about 98
and `coxa_len` about 0.022, so an unnormalised comparison measures the angles
and essentially ignores everything else. Under it `plush_snow_hybrid_2`
looked like an ordinary bug, when normalised it is the most distinctive model
in the set.
"""

from __future__ import annotations

import json
import statistics
from collections import defaultdict
from pathlib import Path

from desktop_bug.content.body_plans import (
    BODY_PLAN_IDS,
    BODY_PLANS,
    body_plan_gait,
    body_plan_legs,
    resolve_body_plan,
)
from desktop_bug.content.discovery import discover_models

ROOT = Path(__file__).resolve().parents[1]
GEOMETRY = ("attach_angle", "rest_angle", "reach", "upper_len", "lower_len",
            "attach_forward", "attach_side", "rest_forward", "rest_side", "coxa_len")
# Models deliberately kept off the shared rigs. Each was hand-tuned and has a
# test or a gait profile that encodes its own anatomy.
KEEPS_OWN_RIG = {"plush_snow_hybrid_2", "knuckle_skitter_stalker", "chosen_one"}


def _raw_models() -> dict:
    out = {}
    for path in sorted((ROOT / "models").glob("*/model.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        out[data["id"]] = data
    return out


def test_every_model_names_a_known_plan():
    for model_id, data in _raw_models().items():
        plan = data.get("body_plan")
        assert plan in BODY_PLAN_IDS, (model_id, plan)


def test_almost_no_model_still_carries_its_own_rig():
    """The point of the package: forty-one rigs down to a handful."""
    own = {mid for mid, data in _raw_models().items() if data.get("legs")}
    assert own == KEEPS_OWN_RIG, sorted(own)
    total_rigs = len(BODY_PLANS) + len(own)
    assert total_rigs <= 8, total_rigs


def test_a_model_that_states_a_rig_keeps_it():
    """Consolidation must not mean 'no model may ever be special'."""
    custom = [{"name": "only", "side": "left", "gait_group": 0, "attach_angle": -11,
               "rest_angle": -11, "reach": 2.0, "upper_len": 1.0, "lower_len": 1.0}]
    resolved = resolve_body_plan({"body_plan": "bug", "legs": list(custom)})
    assert resolved["legs"] == custom


def test_a_model_that_states_a_gait_keeps_it():
    stated = {"enabled": True, "profile": "mine", "cycle_hz": 1.11}
    resolved = resolve_body_plan(
        {"body_plan": "bug", "appearance": {"spider_gait": dict(stated)}})
    assert resolved["appearance"]["spider_gait"] == stated


def test_each_plan_walks_differently():
    """An archetype is a way of moving, not only a shape.

    Before this package only three of forty-nine models enabled the
    anatomical gait controller at all, so almost every spider walked on the
    same generic fallback whatever its body looked like.
    """
    profiles = {plan: body_plan_gait(plan) for plan in BODY_PLAN_IDS}
    assert all(p and p.get("enabled") for p in profiles.values()), profiles
    cadences = {plan: p["cycle_hz"] for plan, p in profiles.items()}
    assert len(set(cadences.values())) == len(cadences), cadences
    # The heavy forward-legged body must not out-step the jumper.
    assert cadences["tarantula"] < cadences["bug"] < cadences["jumper"], cadences


def test_a_plan_hands_out_copies_not_the_shared_list():
    """One spider's tuning must not leak into every other of its plan."""
    first = body_plan_legs("bug")
    first[0]["reach"] = 99.0
    assert body_plan_legs("bug")[0]["reach"] != 99.0
    assert BODY_PLANS["bug"]["legs"][0].get("reach") != 99.0


def test_an_unknown_plan_is_left_alone_rather_than_emptied():
    data = {"body_plan": "nonsense", "legs": [{"name": "a"}]}
    assert resolve_body_plan(data)["legs"] == [{"name": "a"}]
    assert body_plan_legs("nonsense") == []
    assert body_plan_gait("nonsense") is None


def test_every_shipped_model_still_loads_with_eight_legs():
    models, warnings = discover_models()
    assert not warnings, warnings
    assert len(models) == 49, len(models)
    for model_id, data in models.items():
        assert len(data["legs"]) == 8, (model_id, len(data["legs"]))
        for leg in data["legs"]:
            for key in ("name", "side", "gait_group", "attach_angle",
                        "rest_angle", "reach", "upper_len", "lower_len"):
                assert key in leg, (model_id, key)


def test_the_classification_metric_is_scale_free():
    """Guards the mistake that produced the wrong answer the first time.

    Leg parameters are measured in wildly different units -- degrees and
    body-size multiples -- so any comparison that treats them as one space is
    dominated by the widest. This asserts the disparity is real, so that the
    normalisation in the plan's own documentation is not removed later as
    over-engineering.
    """
    spreads = defaultdict(list)
    for data in discover_models()[0].values():
        for leg in data["legs"]:
            for key in GEOMETRY:
                spreads[key].append(float(leg.get(key, 0.0)))
    sd = {key: statistics.pstdev(values) for key, values in spreads.items()}
    widest, narrowest = max(sd.values()), min(v for v in sd.values() if v > 0)
    assert widest / narrowest > 100, sd
