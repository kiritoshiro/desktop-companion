"""DC-19 (C5): behaviour modules replace `_is_*_personality` id branches.

`creature/behaviour.py` used to decide whether a spider acted like a Hunter,
Jumper, Observer, Nope, Drifter, Webber or web-shooting Trapper by checking a
raw personality id or a legacy boolean flag directly inside seven
`_is_*_personality` methods. `personality_profiles.behaviour_modules_for()`
is now the single place that resolves those modules -- from an explicit
`behaviour_modules` list in the personality JSON, or, for an older
personality file, the same legacy flags/id equality the old methods checked
-- and the creature-side `_acts_as_*` helpers just ask it plus the dynamic
`job_id`/`has_skill` gate that has to stay per-creature.

These tests are the regression backstop the plan asks for: each was proved
to fail by reverting the corresponding line back to always returning an
empty set / ignoring the explicit list, then restored.
"""

from __future__ import annotations

import json

import pytest
from desktop_bug.creature import Creature
from desktop_bug.content.discovery import discover_personalities
from desktop_bug.content.personality_profiles import (
    BEHAVIOUR_MODULE_IDS,
    behaviour_modules_for,
    selectable_personality_ids,
)
from support import PERSONALITIES, ROOT, load_model


@pytest.fixture(scope="module")
def personalities():
    found, warnings = discover_personalities(ROOT)
    assert not warnings, warnings
    return found


@pytest.mark.parametrize(
    "personality_id,expected",
    [
        ("hunter", {"hunter"}),
        ("trapper", {"hunter", "web_shooter"}),
        ("jumper", {"jumper"}),
        ("observer", {"observer"}),
        ("nope", {"nope"}),
        ("drifter", {"drifter"}),
        ("webber", {"webber"}),
        # Generic/compact personalities select no specialist module.
        ("balanced", set()),
        ("bold", set()),
        ("curious", set()),
        ("playful", set()),
    ],
)
def test_shipped_personality_selects_its_module(personalities, personality_id, expected):
    assert behaviour_modules_for(personalities[personality_id]) == frozenset(expected)


def test_explicit_behaviour_modules_list_wins_over_legacy_flags():
    """An explicit list is authoritative, even if it disagrees with flags."""
    personality = {
        "id": "hunter",
        "mouse_hunter": True,
        "behaviour_modules": ["drifter"],
    }
    assert behaviour_modules_for(personality) == frozenset({"drifter"})


def test_explicit_list_ignores_unknown_module_ids():
    personality = {"id": "balanced", "behaviour_modules": ["hunter", "not-a-module"]}
    assert behaviour_modules_for(personality) == frozenset({"hunter"})


def test_an_unmigrated_personality_falls_back_to_legacy_flags():
    """A custom personality file with no `behaviour_modules` still works."""
    personality = {"id": "custom", "web_weaver": True}
    assert behaviour_modules_for(personality) == frozenset({"webber"})


def test_every_module_id_is_reachable():
    ids = set()
    for path in sorted(PERSONALITIES.glob("*.json")):
        ids |= behaviour_modules_for(json.loads(path.read_text(encoding="utf-8")))
    assert ids == set(BEHAVIOUR_MODULE_IDS)


def test_no_is_personality_branches_remain():
    """The literal acceptance check from the plan, run from inside pytest too."""
    import re

    src_root = ROOT / "src"
    pattern = re.compile(r"_is_.*_personality")
    hits = []
    for path in src_root.rglob("*.py"):
        text = path.read_text(encoding="utf-8")
        if pattern.search(text):
            hits.append(str(path))
    assert hits == []


LEGACY_IDS = ("hunter", "jumper", "observer", "nope", "drifter", "webber", "trapper")


def test_the_menu_offers_the_six_temperaments(personalities):
    """DC-60 narrows the list; it does not delete anything.

    This reverses DC-19's C5 fix, deliberately and at the owner's request:
    *"some of them could be consolidated and left only a few since they kinda
    look the same."* Twenty-one personality files already shared eleven
    movement profiles between them, so most of the list was distinctions
    without a visible difference.

    C5's actual complaint was that the UI and the data disagreed about what
    personalities existed. They agree again, from the other direction: the
    menu offers what a person should pick from, and `include_id` (below)
    keeps it honest about what is already in use.
    """
    selectable = selectable_personality_ids(personalities)
    for compact_id in ("balanced", "playful", "curious", "bold", "cautious", "social"):
        assert compact_id in selectable, compact_id
    for legacy_id in LEGACY_IDS:
        assert legacy_id not in selectable, legacy_id


def test_a_legacy_personality_already_in_use_is_still_offered(personalities):
    """The half that stops this being a deletion.

    A preset or a saved spider that names one keeps it, and the row showing
    it can still show it -- otherwise loading a preset and saving it again
    would silently rewrite somebody's choice.
    """
    for legacy_id in LEGACY_IDS:
        assert legacy_id in selectable_personality_ids(personalities, legacy_id), legacy_id


def test_a_personality_that_does_not_exist_is_not_offered(personalities):
    """`include_id` is "what this row already uses", not "anything asked for"."""
    assert "not_a_personality" not in selectable_personality_ids(personalities, "not_a_personality")


def test_every_legacy_personality_still_loads_and_still_differs(personalities):
    """Consolidating the *menu* must not consolidate the behaviour."""
    from desktop_bug.content.personality_profiles import movement_profile_for

    for legacy_id in LEGACY_IDS:
        assert legacy_id in personalities, legacy_id
    profiles = {movement_profile_for(personalities[legacy_id]) for legacy_id in LEGACY_IDS}
    assert len(profiles) > 3, profiles


@pytest.mark.parametrize(
    "personality_id,method,module_id",
    [
        ("hunter", "_acts_as_hunter", "hunter"),
        ("jumper", "_acts_as_jumper", "jumper"),
        ("observer", "_acts_as_observer", "observer"),
        ("nope", "_acts_as_nope", "nope"),
        ("drifter", "_acts_as_drifter", "drifter"),
        ("webber", "_acts_as_webber", "webber"),
        ("trapper", "_acts_as_web_shooter", "web_shooter"),
    ],
)
def test_a_creature_actually_wires_its_module_through(personality_id, method, module_id):
    """The module resolves all the way to the live per-creature helper.

    Reverting `_has_behaviour_module` to always return `False` makes every
    one of these fail, proving the wiring is exercised, not just the
    stand-alone `personality_profiles` function above.
    """
    model = load_model("spider")
    personality = json.loads((PERSONALITIES / f"{personality_id}.json").read_text(encoding="utf-8"))
    creature = Creature(model, personality, 2400, 1400)
    assert module_id in creature._behaviour_modules()
    assert getattr(creature, method)() is True


def test_a_balanced_creature_acts_as_no_specialist(personalities):
    model = load_model("spider")
    creature = Creature(model, personalities["balanced"], 2400, 1400)
    for method in (
        "_acts_as_hunter", "_acts_as_jumper", "_acts_as_observer",
        "_acts_as_nope", "_acts_as_drifter", "_acts_as_webber", "_acts_as_web_shooter",
    ):
        assert getattr(creature, method)() is False, method
