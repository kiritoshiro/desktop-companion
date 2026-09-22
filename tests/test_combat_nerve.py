"""Fights that last, and spiders with nerve (DC-50).

Everything here comes from the owner watching a colony run and reporting
what it actually looked like: that silk on another spider did not hold it
the way silk on the pointer does, that fights did not last and both sides
"just move their own ways", that hunters "just circle around their base and
barely fight other team", and that a spider should run when badly hurt or
outnumbered and should run home, because the base is the only thing that
puts health back on.

The last three are one mechanism, which is why they are tested together.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.creature.constants import (
    FLEE_HEALTH_FRACTION,
    RALLY_HEALTH_FRACTION,
)
from desktop_bug.manager import CreatureManager
from desktop_bug.manager.combat import DISENGAGE_RADIUS_MULT

DT = 1.0 / 60.0
SCREEN = (1400, 900)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _scene(monkeypatch, left=1, right=1, job="none", apart=120.0):
    scratch = Path(tempfile.mkdtemp(prefix="dc50-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": left,
             "slot_id": "a", "team": "pack_a", "job": job},
            {"model": "tarantula", "personality": "bold", "count": right,
             "slot_id": "b", "team": "pack_b", "job": job},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False},
                     "conflict": True,
                     "team_relations": {"pack_a": {"pack_b": "foe"}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=6)
    ours = [c for c in manager.creatures if c.progression.team_id == "pack_a"]
    theirs = [c for c in manager.creatures if c.progression.team_id == "pack_b"]
    for index, creature in enumerate(ours):
        creature.x, creature.y = 700.0 - apart / 2.0, 450.0 + index * 26.0
    for index, creature in enumerate(theirs):
        creature.x, creature.y = 700.0 + apart / 2.0, 450.0 + index * 26.0
    return manager, ours, theirs


# --------------------------------------------------------------- fights last

def test_a_fight_survives_the_foe_stepping_out_of_engage_range(monkeypatch):
    """The engage radius starts a fight; a much wider one ends it.

    Without that gap a fight lasted a second or two and both sides wandered
    off, which is what "they just move their own ways" looks like.
    """
    manager, ours, theirs = _scene(monkeypatch)
    me, them = ours[0], theirs[0]
    manager.update(DT, *AWAY)
    assert me._foe is them

    reaction = float(me.personality.get("reaction_radius", 360))
    # Well past the radius that would have started it, well inside the one
    # that ends it.
    them.x = me.x + reaction * 1.1
    manager.update(DT, *AWAY)
    assert me._foe is them, "the fight was dropped the moment it moved"


def test_a_foe_that_truly_leaves_is_dropped(monkeypatch):
    """Sticky is not the same as permanent.

    Worth stating in numbers, because the stickiness is deliberately large
    and it is the sort of tuning that should be visible: a `bold` spider has
    a reaction radius of 430, so it starts a fight at 267px and holds on out
    to 817px. The first version of this test put the foe 767px away and was
    surprised the fight continued -- the test was wrong, not the code.
    """
    manager, ours, theirs = _scene(monkeypatch)
    me, them = ours[0], theirs[0]
    manager.update(DT, *AWAY)
    assert me._foe is them

    reaction = float(me.personality.get("reaction_radius", 360))
    # Both moved to the left edge first: the gap needed is wider than half
    # the test screen, and a foe placed past the edge is simply clamped back
    # inside the radius it was supposed to leave.
    me.x, me.y = 60.0, 450.0
    them.x = me.x + reaction * DISENGAGE_RADIUS_MULT + 40.0
    them.y = me.y
    assert them.x < SCREEN[0], "the test screen is too small for this gap"
    me.engagement_timer = 0.0
    for _ in range(5):
        manager.update(DT, *AWAY)
    assert me._foe is None


def test_a_hunter_is_allowed_to_pick_a_fight(monkeypatch):
    """A Hunter was on duty almost continuously and so was gated out of
    every fight, which is why it only ever circled its own base."""
    manager, ours, _theirs = _scene(monkeypatch, job="hunter")
    for creature in manager.creatures:
        creature.job_mode = "working"
    assert manager._may_pick_a_fight(ours[0]) is True


# ---------------------------------------------------------------------- silk

def test_fresh_silk_holds_a_spider_still(monkeypatch):
    manager, ours, theirs = _scene(monkeypatch)
    victim = theirs[0]
    victim.web_pinned("trap", ours[0])
    assert victim.webbed_held
    assert victim._speed_mult() == 0.0

    start_x, start_y = victim.x, victim.y
    for _ in range(30):
        manager.update(DT, *AWAY)
        if not victim.webbed_held:
            break
    moved = math.hypot(victim.x - start_x, victim.y - start_y)
    assert moved < 6.0, f"a held spider walked {moved:.1f}px"


# --------------------------------------------------------------------- nerve

def test_a_badly_hurt_spider_runs(monkeypatch):
    manager, ours, theirs = _scene(monkeypatch, apart=90.0)
    me, them = ours[0], theirs[0]
    manager.update(DT, *AWAY)
    me.hp = me.max_hp * (FLEE_HEALTH_FRACTION - 0.05)
    manager.update(DT, *AWAY)
    assert me.fleeing
    assert me._foe is None, "it is running and still holding a target"
    assert me.flee_from is them


def test_running_opens_the_distance(monkeypatch):
    manager, ours, theirs = _scene(monkeypatch, apart=90.0)
    me, them = ours[0], theirs[0]
    manager.update(DT, *AWAY)
    me.hp = me.max_hp * 0.2
    start = math.hypot(me.x - them.x, me.y - them.y)
    best = start
    for _ in range(int(4.0 * 60)):
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)   # keep it frightened
        if me.dead or them.dead:
            pytest.skip("the fight ended before the retreat could be measured")
        best = max(best, math.hypot(me.x - them.x, me.y - them.y))
    assert best > start * 1.5, (start, best)


def test_a_healthy_spider_does_not_run(monkeypatch):
    manager, ours, theirs = _scene(monkeypatch, apart=90.0)
    for _ in range(20):
        manager.update(DT, *AWAY)
        if ours[0].dead or theirs[0].dead:
            break
        assert not ours[0].fleeing


def test_being_outnumbered_is_enough_on_its_own(monkeypatch):
    """Full health, but three to one."""
    manager, ours, _theirs = _scene(monkeypatch, left=1, right=3, apart=70.0)
    me = ours[0]
    me.hp = me.max_hp
    manager.update(DT, *AWAY)
    assert me.fleeing, "a lone spider faced three foes and stood its ground"


def test_an_even_match_is_not_outnumbered(monkeypatch):
    manager, _ours, _theirs = _scene(monkeypatch, left=2, right=2, apart=70.0)
    for creature in manager.creatures:
        creature.hp = creature.max_hp
    manager.update(DT, *AWAY)
    assert not any(c.fleeing for c in manager.creatures), "an even fight scared someone"


def test_it_stops_running_once_patched_up(monkeypatch):
    manager, ours, _theirs = _scene(monkeypatch, apart=90.0)
    me = ours[0]
    me.hp = me.max_hp * 0.2
    manager.update(DT, *AWAY)
    assert me.fleeing
    me.hp = me.max_hp * (RALLY_HEALTH_FRACTION + 0.05)
    for _ in range(10):
        manager.update(DT, *AWAY)
    assert not me.fleeing


def test_the_rally_point_is_higher_than_the_flee_point():
    """Otherwise a spider bounces in and out of a fight on a single hit."""
    assert RALLY_HEALTH_FRACTION > FLEE_HEALTH_FRACTION + 0.2


def test_a_hurt_spider_runs_towards_its_own_base(monkeypatch):
    """The base is the only thing that heals, so a retreat should aim at it."""
    manager, ours, theirs = _scene(monkeypatch, apart=90.0)
    me, them = ours[0], theirs[0]
    # A base behind it, away from the foe.
    me.x, me.y = 700.0, 450.0
    them.x, them.y = 900.0, 450.0
    site = manager.base_world.ensure_site(me)
    site.x, site.y = 200.0, 450.0
    site.level = 1

    me.hp = me.max_hp * 0.2
    before = math.hypot(me.x - site.x, me.y - site.y)
    for _ in range(int(3.0 * 60)):
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)
        if me.dead:
            pytest.skip("it died before reaching home")
    after = math.hypot(me.x - site.x, me.y - site.y)
    assert after < before * 0.7, (before, after)


def test_a_spider_with_no_base_still_runs_away(monkeypatch):
    """A team that has built nothing must not stand there instead."""
    manager, ours, theirs = _scene(monkeypatch, apart=90.0)
    manager.base_world.clear()
    me, them = ours[0], theirs[0]
    me.hp = me.max_hp * 0.2
    start = math.hypot(me.x - them.x, me.y - them.y)
    best = start
    for _ in range(int(3.0 * 60)):
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)
        if me.dead or them.dead:
            pytest.skip("the fight ended first")
        best = max(best, math.hypot(me.x - them.x, me.y - them.y))
    assert best > start * 1.4, (start, best)
