"""Shipped presets must not silently strip a spider's abilities.

A slot that never customised its abilities is personality-driven: it keeps the
common ``web_walk`` plus whatever specialty its personality grants, and adds
whatever its job requires.  Only an explicit (possibly empty) ``abilities`` list
in the preset overrides that.
"""

import json
import random

from desktop_bug.manager import CreatureManager
from desktop_bug.skills import (
    ABILITY_SKILL_IDS,
    COMMON_ABILITY_IDS,
    default_ability_ids,
    skills_with_default_abilities,
    skills_with_selected_abilities,
)
from desktop_bug.discovery import discover_personalities
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """


def abilities_of(creature):
    return [skill_id for skill_id in creature.skill_ids() if skill_id in ABILITY_SKILL_IDS]


PRESETS = sorted((ROOT / "presets").glob("*.json"))


@pytest.fixture(scope="module")
def personalities():
    random.seed(11)
    found, warnings = discover_personalities(ROOT)
    assert not warnings, warnings
    return found


def test_no_choice_keeps_the_personalitys_own_abilities(personalities):
    assert default_ability_ids(personalities["webber"]) == ["weave_web", "web_walk"]
    assert set(skills_with_default_abilities(personalities["trapper"])) >= {
        "web_walk", "shoot_web", "wall_web",
    }
    assert "drift" in skills_with_default_abilities(personalities["drifter"])


def test_an_explicit_empty_choice_still_means_none(personalities):
    assert not set(ABILITY_SKILL_IDS).intersection(
        skills_with_selected_abilities(personalities["webber"], [])
    )


def test_a_job_adds_the_ability_it_requires(personalities):
    webber_job = skills_with_default_abilities(personalities["curious"], ("weave_web",))
    assert {"weave_web", "web_walk"}.issubset(webber_job), webber_job


@pytest.mark.parametrize("preset_path", PRESETS, ids=lambda p: p.stem)
def test_a_shipped_preset_launches_spiders_that_can_act(preset_path):
    preset = json.loads(preset_path.read_text(encoding="utf-8"))
    explicit = {
        index for index, slot in enumerate(preset.get("slots", []))
        if slot.get("abilities") is not None or slot.get("skills") is not None
    }
    manager = CreatureManager(preset_path, 1280, 720)
    assert manager.creatures, preset_path
    if explicit:
        # Mixed presets: only the personality-driven spiders say anything here.
        return
    for creature in manager.creatures:
        abilities = abilities_of(creature)
        for common in COMMON_ABILITY_IDS:
            assert common in abilities, (preset_path, creature.personality["id"], abilities)
        for expected in default_ability_ids(creature.personality):
            assert expected in abilities, (preset_path, creature.personality["id"], abilities)
