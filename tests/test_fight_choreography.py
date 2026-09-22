"""A fight that looks like a fight (DC-59).

The owner: *"next thing i want a good looking fight between spiders. how they
move their legs and body as in an attack stance and focus, and so on."*

Before this every number about a fight was right -- blows landed on a
cooldown, silk pinned, the loser died -- and none of it was visible, because
nothing about a fighting spider was drawn differently from a walking one.

The interesting part is what the first real-hardware session called "a fight
is one clump of overlapping bodies". That was recorded as a *spacing* problem
and it is not one. A pounce at a foe was launched with ``after="outcome"``,
which is the **social** resolution -- catch, cuddle or flee -- written for
pouncing on a friend. Measured over a 364-frame brawl, the two spiders spent
170 frames in ``Cuddle``: glued together, and unreachable by any spacing rule,
because ``Cuddle`` is a committed state and ``_pursue_foe`` hands the tick
back whenever it is in one. Two spiders fighting to the death were cuddling.

With a combat landing of its own, the same measured brawl went from 171 frames
closer than one combined size to 35 -- and the 35 are the pounces, which are
supposed to close.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.creature.constants import (
    COMBAT_SPACING,
    COMBAT_SPACING_SLACK,
    LUNGE_REACH,
    RECOIL_REACH,
)
from desktop_bug.manager import CreatureManager
from desktop_bug.manager.combat import CONTACT_REACH

DT = 1.0 / 60.0
SCREEN = (420, 300)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


@pytest.fixture
def brawl(monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="dc59-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "f.json"
    preset.write_text(json.dumps({
        "name": "f",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "a", "team": "hunters"},
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "b", "team": "rivals"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False},
                     "conflict": True,
                     "team_relations": {"hunters": {"rivals": "foe"}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=6)
    left = [c for c in manager.creatures if c.progression.team_id == "hunters"][0]
    right = [c for c in manager.creatures if c.progression.team_id == "rivals"][0]
    left.x, left.y = 150.0, 165.0
    right.x, right.y = 270.0, 165.0
    # Unkillable, so the whole run is a fight rather than a funeral.
    for creature in (left, right):
        creature.max_hp = 100000.0
        creature.hp = 100000.0
    return manager, left, right


def _run(brawl, frames):
    manager, left, right = brawl
    gaps = []
    for _ in range(frames):
        manager.update(DT, *AWAY)
        for creature in (left, right):
            creature.hp = creature.max_hp
        gaps.append(math.hypot(left.x - right.x, left.y - right.y)
                    / (left.size + right.size))
    return gaps


# ------------------------------------------------------------- the spacing

def test_a_fight_is_not_one_clump_of_bodies(brawl):
    """The measurement the whole package turns on.

    Not an average: a mean of 1.44 hid two spiders sitting on top of each
    other for half the fight and drifting apart for the other half. What
    matters is how much of the fight is spent overlapping.
    """
    gaps = _run(brawl, 364)
    overlapping = sum(1 for gap in gaps if gap < 1.0)
    assert overlapping < len(gaps) * 0.2, (
        f"{overlapping}/{len(gaps)} frames spent overlapping")


def test_two_spiders_fighting_do_not_cuddle(brawl):
    """The actual cause, pinned by name.

    `_resolve_pounce_outcome` is the social resolution and a pounce at a foe
    used to go through it. Keeping this test honest matters more than the
    spacing one: the spacing rule was written first, changed nothing, and the
    measurement said so.
    """
    manager, left, right = brawl
    states = []
    for _ in range(364):
        manager.update(DT, *AWAY)
        for creature in (left, right):
            creature.hp = creature.max_hp
        states.extend((left.state, right.state))
    assert "Cuddle" not in states, "two spiders fighting to the death cuddled"
    assert "Play" not in states


def test_a_spider_walks_out_again_when_it_ends_up_too_close(brawl):
    """Holding still is not enough: a pounce or a shove puts them on top of
    each other and something has to walk back out."""
    manager, left, right = brawl
    # Let the fight start properly first: a foe has to be published before
    # anything about fighting applies.
    for _ in range(30):
        manager.update(DT, *AWAY)
    left.x, left.y = right.x - 6.0, right.y
    start = math.hypot(left.x - right.x, left.y - right.y) / (left.size + right.size)
    best = start
    for _ in range(360):
        manager.update(DT, *AWAY)
        for creature in (left, right):
            creature.hp = creature.max_hp
        best = max(best, math.hypot(left.x - right.x, left.y - right.y)
                   / (left.size + right.size))
    assert best > COMBAT_SPACING * 0.8, (start, best)


def test_a_blow_still_lands_at_the_distance_they_stand_at():
    """The trap in the first version: a standoff of 1.35 combined sizes with
    a contact reach of 0.85 would have meant two spiders squared up at
    exactly the right distance could never hit each other."""
    assert CONTACT_REACH > COMBAT_SPACING * (1.0 + COMBAT_SPACING_SLACK)


def test_they_do_still_reach_each_other(brawl):
    """Spacing that stopped the fight would be worse than the clump."""
    manager, left, right = brawl
    before = left.hp + right.hp
    for _ in range(int(20.0 * 60)):
        manager.update(DT, *AWAY)
        if left.hp + right.hp < before:
            break
    assert left.hp + right.hp < before, "nobody landed a blow in twenty seconds"


# -------------------------------------------------------------- the stance

def test_a_spider_with_a_foe_squares_up(brawl):
    manager, left, right = brawl
    assert left.combat_stance == 0.0
    for _ in range(60):
        manager.update(DT, *AWAY)
        for creature in (left, right):
            creature.hp = creature.max_hp
    assert left.combat_stance > 0.5
    assert left.rear > 0.4, "squaring up did not rear the body"


def test_the_stance_drops_when_the_fight_does(brawl):
    manager, left, right = brawl
    for _ in range(60):
        manager.update(DT, *AWAY)
    assert left.combat_stance > 0.5
    right.dead = True
    left._foe = None
    for _ in range(120):
        manager.update(DT, *AWAY)
    assert left.combat_stance < 0.05


def test_a_fleeing_spider_does_not_hold_a_threat_pose(brawl):
    """It is running away; a threat display would read as the opposite."""
    manager, left, right = brawl
    for _ in range(60):
        manager.update(DT, *AWAY)
    left.flee_timer = 5.0
    left.flee_from = right
    for _ in range(60):
        manager.update(DT, *AWAY)
        left.flee_timer = 5.0
    assert left.combat_stance < 0.5


def test_a_squared_up_spider_faces_its_foe(brawl):
    manager, left, right = brawl
    for _ in range(60):
        manager.update(DT, *AWAY)
        for creature in (left, right):
            creature.hp = creature.max_hp
    dx, dy = right.x - left.x, right.y - left.y
    span = math.hypot(dx, dy) or 1.0
    dot = left.combat_face_x * dx / span + left.combat_face_y * dy / span
    assert dot > 0.98, dot


# -------------------------------------------------------- the blow itself

def test_a_landed_blow_throws_the_body_forward(brawl):
    manager, left, right = brawl
    manager.update(DT, *AWAY)
    left.strike_landed(right)
    assert left.lunge == pytest.approx(1.0)
    offset_x, offset_y = left.combat_body_offset()
    assert math.hypot(offset_x, offset_y) == pytest.approx(LUNGE_REACH * left.size)
    # Forward, towards the foe.
    assert offset_x * (right.x - left.x) + offset_y * (right.y - left.y) > 0.0


def test_a_blow_taken_throws_the_body_back(brawl):
    manager, left, right = brawl
    manager.update(DT, *AWAY)
    left.blow_taken(right)
    assert left.lunge == pytest.approx(-1.0)
    offset_x, offset_y = left.combat_body_offset()
    assert math.hypot(offset_x, offset_y) == pytest.approx(RECOIL_REACH * left.size)
    assert offset_x * (right.x - left.x) + offset_y * (right.y - left.y) < 0.0


def test_the_lunge_decays_rather_than_sticking(brawl):
    """With the fight called off, so the decay is what is measured.

    Left running, the two keep landing blows and keep re-setting the lunge,
    which is correct and makes "it decayed" unmeasurable.
    """
    manager, left, right = brawl
    manager.update(DT, *AWAY)
    right.dead = True
    left._foe = None
    left.strike_landed(right)
    assert left.combat_body_offset() != (0.0, 0.0)
    for _ in range(90):
        manager.update(DT, *AWAY)
    assert left.combat_body_offset() == (0.0, 0.0)


def test_the_body_moves_and_the_feet_do_not(brawl):
    """The whole animation: the legs are solved in world space, so throwing
    the body over planted feet stretches them for free."""
    manager, left, right = brawl
    for _ in range(30):
        manager.update(DT, *AWAY)
    feet = [(leg.foot_x, leg.foot_y) for leg in left.legs]
    left.strike_landed(right)
    assert [(leg.foot_x, leg.foot_y) for leg in left.legs] == feet
    assert left.combat_body_offset() != (0.0, 0.0)


def test_the_pose_changes_nothing_that_decides_a_fight(brawl):
    """A rendering change must not become a balance change."""
    manager, left, right = brawl
    manager.update(DT, *AWAY)
    before = (left.hp, left.damage, left.armor, left.attack_cooldown,
              left.x, left.y, left.speed)
    left.strike_landed(right)
    left.blow_taken(right)
    after = (left.hp, left.damage, left.armor, left.attack_cooldown,
             left.x, left.y, left.speed)
    assert before == after
