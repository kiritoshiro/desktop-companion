"""Headless checks for weighted personality behaviour phases."""

from __future__ import annotations

import random
from collections import Counter

import pytest
from desktop_bug.creature import Creature
from desktop_bug.discovery import validate_personality
from desktop_bug.personality_profiles import BEHAVIOUR_PHASE_IDS, phase_scores_for
from desktop_bug.phase_scheduler import (
    PHASE_DURATION_RANGES,
    BehaviourPhaseScheduler,
    normalize_focus,
)
from support import ROOT

ENABLED = list(BEHAVIOUR_PHASE_IDS)
SCHEMA_BASE = {
    "id": "schema-test",
    "display_name": "Schema test",
    "speed_multiplier": 1.0,
    "reaction_radius": 100,
    "boldness": 0.5,
    "wander_frequency": 0.5,
}


def scores_with(**weights) -> dict:
    scores = {phase_id: 0 for phase_id in BEHAVIOUR_PHASE_IDS}
    scores.update(weights)
    return scores


@pytest.fixture
def personality() -> dict:
    return {"id": "phase-test", "phase_scores": scores_with(roll=10, jump=1)}


def test_the_deck_holds_only_what_is_scored(personality):
    deck = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41)).deck
    assert Counter(deck) == Counter({"roll": 10, "jump": 1})
    assert "wander" not in deck


def test_the_same_seed_deals_the_same_deck(personality):
    first = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    second = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    assert first.deck == second.deck


def test_only_scored_phases_are_allowed(personality):
    scheduler = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    assert scheduler.is_allowed("roll")
    assert not scheduler.is_allowed("wander")


def test_two_names_for_the_same_focus_plan_alike(personality):
    """"mouse" and "cursor" are the same thing, and must not diverge."""
    first = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    second = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    first_plan = first.choose("mouse")
    second_plan = second.choose("cursor")
    assert first_plan is not None and second_plan is not None
    assert first_plan.phase_id == second_plan.phase_id
    assert first_plan.focus == "cursor"
    assert first_plan.duration == second_plan.duration


def test_a_duration_stays_inside_its_scored_range(personality):
    scheduler = BehaviourPhaseScheduler(personality, ENABLED, random.Random(41))
    plan = scheduler.choose("mouse")
    low, high = PHASE_DURATION_RANGES[plan.phase_id]
    scale = scheduler.duration_multiplier * (0.72 + plan.score / 10.0 * 0.56)
    assert low * scale <= plan.duration <= high * scale


def test_a_gated_scheduler_offers_only_what_is_enabled(personality):
    gated = BehaviourPhaseScheduler(personality, ("jump",), random.Random(2))
    assert gated.choose("none").phase_id == "jump"
    assert gated.choose("none").phase_id == "jump"


def test_a_scheduler_with_nothing_enabled_offers_nothing(personality):
    empty = BehaviourPhaseScheduler(personality, (), random.Random(2))
    assert empty.choose("none") is None


def test_a_phase_that_needs_a_target_waits_for_one():
    target_only = BehaviourPhaseScheduler(
        {"id": "target-only", "phase_scores": scores_with(approach=10)},
        ENABLED,
        random.Random(3),
    )
    assert target_only.choose("none") is None
    assert target_only.choose("cursor").phase_id == "approach"


def test_focus_names_are_normalised():
    assert normalize_focus("fly") == "prey"
    assert normalize_focus("another spider") == "none"


def test_unscored_phases_read_as_zero(personality):
    assert phase_scores_for(personality)["roll"] == 10.0
    assert phase_scores_for(personality)["wander"] == 0.0


def test_a_creature_actually_follows_its_scheduler():
    runtime_personality = {
        "id": "phase-runtime",
        "speed_multiplier": 1.0,
        "reaction_radius": 80,
        "boldness": 0.5,
        "wander_frequency": 1.0,
        "idle_time": [0.0, 0.0],
        "phase_scores": scores_with(wander=10),
    }
    runtime = Creature({"base_size": 25, "legs": []}, runtime_personality, 800, 600)
    runtime.x, runtime.y = 400.0, 300.0
    runtime.state_timer = 0.0
    runtime.decision_timer = 0.0
    runtime.update(1.0 / 60.0, 780.0, 580.0, 800, 600)
    assert runtime.state == "Wander"
    assert runtime.phase_scheduler.current_phase_id == "wander"


@pytest.mark.parametrize(
    "extra,accepted",
    [
        pytest.param({"phase_scores": {"wander": 0, "chase": 10}}, True, id="in-range"),
        pytest.param({"phase_scores": {"wander": 11}}, False, id="score-above-ten"),
        pytest.param({"behaviours": ["weave_web"]}, False, id="behaviours-are-gone"),
    ],
)
def test_the_personality_schema_holds_the_line(extra, accepted):
    valid, _ = validate_personality({**SCHEMA_BASE, **extra}, ROOT / "phase-test.json")
    assert valid is accepted
