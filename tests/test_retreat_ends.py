"""A retreat has to end (DC-64).

The owner, watching a colony on a real desktop: *"after initiating runing
after low health, they get stuck in that position where they runinng, and not
stoping, so end up mostly to top right corner."*

DC-50 gave a spider exactly one way out of a retreat -- heal back up to
``RALLY_HEALTH_FRACTION`` -- and the only thing that heals is a base. A spider
whose team has built nothing, or that runs away from its base rather than
towards it, can never satisfy that condition. It runs until it hits an edge,
and then runs on the spot for the rest of the session.

Measured before the fix, with the foe pinned so the question was only whether
the runner ever stops: **7200 of 7200 frames fleeing over two minutes**, final
position (20, 880) -- a corner -- 856px from a threat whose scan radius is
260, still holding it as ``flee_from``. After: 204 frames.

Two things were wrong and both are tested here:

* **Escaping was not an exit.** Running away is *for* getting away; not
  noticing that it worked is the bug.
* **``flee_from`` outlived the retreat.** ``Creature.update`` clears it when
  the timer lapses, but ``_update_nerve`` wrote it back from the stale
  reference every frame, so a spider kept running from something on the far
  side of the screen.

The second half of the package is the calm counterpart: once safe but still
hurt, a spider walks home to heal instead of resuming its rounds at a tenth of
its hp. That is the behaviour the sprint was standing in for.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.creature.constants import (
    ESCAPED_RADIUS,
    ESCAPED_SECONDS,
    RALLY_HEALTH_FRACTION,
    THREAT_SCAN_RADIUS,
)
from desktop_bug.manager import CreatureManager

DT = 1.0 / 60.0
SCREEN = (1400, 900)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _scene(monkeypatch, apart=120.0):
    scratch = Path(tempfile.mkdtemp(prefix="dc64-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "a", "team": "pack_a", "job": "none"},
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "b", "team": "pack_b", "job": "none"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False},
                     "conflict": True,
                     "team_relations": {"pack_a": {"pack_b": "foe"}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=6)
    me = [c for c in manager.creatures if c.progression.team_id == "pack_a"][0]
    them = [c for c in manager.creatures if c.progression.team_id == "pack_b"][0]
    me.x, me.y = 700.0 - apart / 2.0, 450.0
    them.x, them.y = 700.0 + apart / 2.0, 450.0
    return manager, me, them


def _run(manager, me, them, seconds, hold_hp=0.2, pin_foe=True):
    """Step the scene, optionally pinning the foe so only the runner moves.

    Pinning matters: without it the foe chases, the runner dies, and the
    measurement becomes "how long until it loses a fight" rather than "does
    it ever stop running".
    """
    fleeing_frames = 0
    frames = int(seconds / DT)
    for _ in range(frames):
        if pin_foe:
            them.x, them.y = 700.0 + 60.0, 450.0
            them.motion_paused = True
        manager.update(DT, *AWAY)
        if pin_foe:
            them.x, them.y = 700.0 + 60.0, 450.0
        if me.dead:
            pytest.skip("it died before the measurement finished")
        if hold_hp is not None:
            me.hp = min(me.hp, me.max_hp * hold_hp)
        if me.fleeing:
            fleeing_frames += 1
    return fleeing_frames, frames


# ------------------------------------------------------- the reported bug

def test_a_retreat_ends_even_when_nothing_ever_heals(monkeypatch):
    """The bug as reported, with the numbers that were measured.

    Held at 20% hp for two minutes so healing can never be the exit. Before
    DC-64 this was 7200/7200.
    """
    manager, me, them = _scene(monkeypatch)
    me.hp = me.max_hp * 0.2
    fleeing, frames = _run(manager, me, them, seconds=120.0)
    assert fleeing < frames * 0.25, (
        f"still running for {fleeing} of {frames} frames")
    assert not me.fleeing


def test_it_does_not_end_up_pinned_in_a_corner(monkeypatch):
    """What the owner actually saw. A corner is where a permanent retreat
    accumulates: the away-vector stops changing once an edge is reached."""
    manager, me, them = _scene(monkeypatch)
    me.hp = me.max_hp * 0.2
    _run(manager, me, them, seconds=60.0)
    margin = 60.0
    in_corner = ((me.x < margin or me.x > SCREEN[0] - margin)
                 and (me.y < margin or me.y > SCREEN[1] - margin))
    assert not in_corner, f"parked in a corner at ({me.x:.0f}, {me.y:.0f})"


def test_the_thing_it_was_running_from_is_let_go(monkeypatch):
    """``flee_from`` outliving the retreat is half the bug: the creature
    clears it, the manager wrote it straight back."""
    manager, me, them = _scene(monkeypatch)
    me.hp = me.max_hp * 0.2
    manager.update(DT, *AWAY)
    assert me.flee_from is them
    _run(manager, me, them, seconds=30.0)
    assert me.flee_from is None
    assert me.escaped_timer == 0.0


# ------------------------------------------------------------- escaping

def test_getting_away_is_what_ends_it_not_getting_well(monkeypatch):
    """Explicitly: hp never rises, and the retreat still ends."""
    manager, me, them = _scene(monkeypatch)
    me.hp = me.max_hp * 0.2
    start_hp = me.hp
    _run(manager, me, them, seconds=30.0)
    assert me.hp <= start_hp, "the test healed it; that is not what is being tested"
    assert not me.fleeing


def test_it_keeps_running_while_the_foe_is_still_close(monkeypatch):
    """The exit must not be so eager that a retreat stops inside arm's reach."""
    manager, me, them = _scene(monkeypatch, apart=60.0)
    me.hp = me.max_hp * 0.2
    # The question is when a retreat ends, not whether it is survived: with
    # a foe glued at 70px the pair simply fight and the runner dies before
    # the measurement finishes.
    them.damage = 0.0
    for _ in range(int(3.0 * 60)):
        them.x, them.y = me.x + 70.0, me.y   # chase it, stay inside the radius
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)
        if me.dead:
            pytest.skip("it died first")
    assert me.fleeing, "it stopped running with a foe 70px away"


def test_the_escape_radius_is_wider_than_the_one_that_starts_a_retreat():
    """Leaving by the line you entered by makes a spider oscillate on it --
    the same reason RALLY sits well above FLEE."""
    assert ESCAPED_RADIUS > THREAT_SCAN_RADIUS
    assert ESCAPED_SECONDS > 0.0


def test_one_frame_of_respite_is_not_an_escape(monkeypatch):
    """It has to stay clear for ESCAPED_SECONDS, so a foe clipping out of
    range for an instant does not call off a retreat mid-stride."""
    manager, me, them = _scene(monkeypatch)
    me.hp = me.max_hp * 0.2
    manager.update(DT, *AWAY)
    assert me.fleeing
    them.x, them.y = 20.0, 20.0            # suddenly far away
    manager.update(DT, *AWAY)
    assert me.fleeing, "one clear frame called off the retreat"


# ----------------------------------------------- the calm half: walking home

def test_once_safe_but_still_hurt_it_walks_home(monkeypatch):
    manager, me, them = _scene(monkeypatch)
    site = manager.base_world.ensure_site(me)
    site.x, site.y = 220.0, 200.0
    site.level = 1
    them.x, them.y = 1300.0, 850.0          # nowhere near either
    me.x, me.y = 900.0, 620.0
    me.hp = me.max_hp * 0.2

    before = math.hypot(me.x - site.x, me.y - site.y)
    for _ in range(int(12.0 * 60)):
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)
        if me.dead:
            pytest.skip("it died first")
    after = math.hypot(me.x - site.x, me.y - site.y)
    assert after < before * 0.6, (before, after)


def test_it_stops_recovering_once_patched_up(monkeypatch):
    manager, me, them = _scene(monkeypatch)
    them.x, them.y = 1300.0, 850.0
    me.hp = me.max_hp * 0.2
    manager.update(DT, *AWAY)
    me.hp = me.max_hp * (RALLY_HEALTH_FRACTION + 0.05)
    for _ in range(10):
        manager.update(DT, *AWAY)
    assert me.recovering is False


def test_it_will_not_limp_home_into_the_enemy(monkeypatch):
    """Home is sometimes where the enemy is. Walking in would put the spider
    straight back over the flee threshold, and it would bounce between
    running and limping home for as long as the foe stayed -- measured at
    2104 of 7200 frames before this guard, against 204 with it."""
    manager, me, them = _scene(monkeypatch)
    site = manager.base_world.ensure_site(me)
    site.x, site.y = 700.0, 450.0
    site.level = 1
    them.x, them.y = site.x + 40.0, site.y   # camped on the base
    me.x, me.y = 1200.0, 800.0
    me.hp = me.max_hp * 0.2

    before = math.hypot(me.x - site.x, me.y - site.y)
    for _ in range(int(8.0 * 60)):
        them.x, them.y = site.x + 40.0, site.y
        manager.update(DT, *AWAY)
        me.hp = min(me.hp, me.max_hp * 0.2)
        if me.dead:
            pytest.skip("it died first")
    after = math.hypot(me.x - site.x, me.y - site.y)
    assert after > ESCAPED_RADIUS * 0.5, (
        f"limped to {after:.0f}px of a base with a foe on it (from {before:.0f})")


def test_a_spider_with_nowhere_to_heal_simply_gets_on_with_things(monkeypatch):
    """There is nowhere to go, and standing still would swap one stuck
    spider for another.

    Note what this test had to become. The obvious setup -- clear the bases
    and watch -- does not work any more: since DC-54 the manager re-founds a
    base for every *named* team on every update, so ``base_world.clear()``
    is undone on the next frame. A spider with no base is now only reachable
    through "Neutral / solo", which DC-54 deliberately leaves out. That is
    worth stating rather than working around, because it means the "a team
    that has built nothing" case the original retreat was written for no
    longer exists for a team with a name.
    """
    manager, me, them = _scene(monkeypatch)
    me.set_team("neutral")
    manager.base_world.clear()
    manager.update(DT, *AWAY)
    assert me._own_base_point() is None, "neutral was given a base after all"

    me.hp = me.max_hp * 0.2
    me.recovering = True
    assert me._walk_home_to_heal(DT) is False
    assert me.recovering is False


def test_every_named_team_still_has_somewhere_to_run_to(monkeypatch):
    """The other half of the note above, stated as a fact that can fail."""
    manager, me, _them = _scene(monkeypatch)
    manager.base_world.clear()
    manager.update(DT, *AWAY)
    assert me._own_base_point() is not None
