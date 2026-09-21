"""Headless checks for the compact personality/ability catalog."""

from __future__ import annotations

import json

import pytest
from desktop_bug.content.discovery import discover_personalities
from desktop_bug.content.personality_profiles import (
    BEHAVIOUR_PHASE_IDS,
    COMPACT_TEMPERAMENT_IDS,
    MOVEMENT_PROFILES,
    TEMPERAMENT_TRAIT_IDS,
)
from desktop_bug.content.skills import (
    ABILITY_SKILL_IDS,
    COMMON_ABILITY_IDS,
    COMMON_BEHAVIOUR_IDS,
    default_skills_for_personality,
    skills_with_selected_abilities,
)
from support import ROOT

PERSONALITY_FILES = sorted((ROOT / "personalities").glob("*.json"))


@pytest.fixture(scope="module")
def personalities():
    found, warnings = discover_personalities(ROOT)
    assert not warnings, warnings
    return found


def test_every_legacy_personality_still_loads(personalities):
    """The launch menu shrank to six; the files behind it did not go away."""
    assert len(personalities) >= 21, len(personalities)


def test_the_menu_is_six_temperaments(personalities):
    assert set(COMPACT_TEMPERAMENT_IDS).issubset(personalities)
    assert len(COMPACT_TEMPERAMENT_IDS) == 6


def test_the_ability_skills_are_the_five_that_can_be_chosen():
    assert set(ABILITY_SKILL_IDS) == {"drift", "weave_web", "web_walk", "shoot_web", "wall_web"}
    assert COMMON_ABILITY_IDS == ("web_walk",)
    assert "web_walk" not in COMMON_BEHAVIOUR_IDS


@pytest.mark.parametrize("path", PERSONALITY_FILES, ids=lambda p: p.stem)
def test_a_personality_file_composes_into_a_profile(personalities, path):
    data = json.loads(path.read_text(encoding="utf-8"))
    assert "skills" not in data, path
    composed = personalities[data["id"]]
    assert composed["movement_profile"] in MOVEMENT_PROFILES
    assert tuple(composed["phase_scores"]) == BEHAVIOUR_PHASE_IDS
    assert all(0.0 <= float(value) <= 10.0 for value in composed["phase_scores"].values())


@pytest.mark.parametrize("personality_id", COMPACT_TEMPERAMENT_IDS)
def test_a_temperament_scores_every_trait(personalities, personality_id):
    temperament = personalities[personality_id]["temperament"]
    assert tuple(temperament) == TEMPERAMENT_TRAIT_IDS
    assert all(0.0 <= float(value) <= 10.0 for value in temperament.values())


def test_a_hunter_chases_and_nothing_else(personalities):
    scores = personalities["hunter"]["phase_scores"]
    assert scores["chase"] == 10.0
    assert scores["prepare_jump_attack"] >= 7.0
    assert all(
        scores[phase] == 0.0
        for phase in ("jump", "roll", "zoomies", "cuddle", "social_play")
    )


def test_a_personality_keeps_its_common_behaviours(personalities):
    normal = default_skills_for_personality(personalities["hunter"])
    assert set(COMMON_BEHAVIOUR_IDS).issubset(normal)
    assert "web_walk" in normal


def test_choosing_no_abilities_leaves_only_behaviours(personalities):
    chosen = skills_with_selected_abilities(personalities["hunter"], [])
    assert set(COMMON_BEHAVIOUR_IDS).issubset(chosen)
    assert not set(ABILITY_SKILL_IDS).intersection(chosen)


@pytest.mark.parametrize(
    "personality_id,expected",
    [
        ("drifter", {"drift", "web_walk"}),
        ("trapper", {"shoot_web", "wall_web", "web_walk"}),
        ("webber", {"weave_web", "web_walk"}),
    ],
)
def test_a_personality_brings_its_own_abilities(personalities, personality_id, expected):
    assert expected.issubset(default_skills_for_personality(personalities[personality_id]))


def test_nope_brings_nothing_but_the_basics(personalities):
    assert set(default_skills_for_personality(personalities["nope"])) == {
        "wander", "jump", "run_away",
    }
