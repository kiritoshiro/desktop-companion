"""Flies stay on the monitors, and spiders don't grind against the gap (DC-88).

The owner, on two monitors of different sizes: *"flies can still run on
extended screen beyond what is the screen size. and spiders glitch if they
try to go in there."*

DC-65 kept spiders on the real screens, but only by pulling a spider's body
back after it had moved. Flies knew nothing about monitors: they bounced off
the edges of the window around both screens, so they wandered into the corner
no monitor shows. A spider chasing one there walked into that corner every
frame and was shoved back out every frame, its legs thrashing. Flies now use
the same Playfield, and a spider aims for the nearest point on a real screen
*before* it steps, so the shove behind it has nothing left to do.

The layout is the scaled-down one from ``test_two_screens.py``: a big monitor
with a smaller one to its right, leaving a hole below the small one.
"""

from __future__ import annotations

import json
import random
import tempfile
from pathlib import Path

import pytest

from desktop_bug.manager import CreatureManager
from desktop_bug.world.flies import FlyWorld
from desktop_bug.world.playfield import Playfield, ScreenRect

DT = 1.0 / 60.0
AWAY = (-9000.0, -9000.0)
BIG = ScreenRect(0.0, 0.0, 800.0, 600.0)
SMALL = ScreenRect(800.0, 0.0, 500.0, 360.0)
SCREEN = (1300, 600)
DEAD = (1100.0, 500.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _flies(playfield) -> FlyWorld:
    world = FlyWorld(*SCREEN, enabled=True, min_interval=0.2, max_interval=0.5,
                     max_flies=8, rng=random.Random(5))
    world.playfield = playfield
    world.use_spawner = False
    return world


def _fly_frames_off_screen(world, field, seconds=60.0) -> int:
    off = 0
    for _ in range(int(seconds * 60)):
        world.update(DT, [], None)
        off += sum(1 for f in world.flies if f.alive and not field.contains(f.x, f.y))
    return off


def test_flies_stay_on_the_monitors():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    assert _fly_frames_off_screen(_flies(field), field) == 0


def test_without_the_layout_flies_do_reach_the_hole():
    """The bug, so the test above is known to be measuring something."""
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    assert _fly_frames_off_screen(_flies(None), field) > 0


def test_a_fly_put_in_the_hole_comes_out_of_it():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    world = _flies(field)
    world.enabled = False
    fly = world.spawn(force=True)
    fly.x, fly.y = DEAD
    world.update(DT, [], None)
    assert field.contains(fly.x, fly.y), (fly.x, fly.y)


def test_a_nest_in_the_hole_is_moved_onto_a_monitor():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    world = _flies(field)
    nest = world.add_spawner(DEAD)
    world.update(DT, [], None)
    assert field.contains(nest.x, nest.y), (nest.x, nest.y)


def test_new_flies_arrive_on_a_monitor():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    world = _flies(field)
    for _ in range(200):
        fly = world.spawn(force=True)
        assert field.contains(fly.x, fly.y), (fly.x, fly.y)


def _hunting_colony(monkeypatch, seed):
    scratch = Path(tempfile.mkdtemp(prefix="dc88-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": 4,
                   "slot_id": "a", "job": "hunter"}],
        "settings": {"flies": {"enabled": True, "spawner": True, "max_flies": 8,
                               "min_interval": 0.5, "max_interval": 1.0}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=seed)
    manager.set_screen_rects([BIG, SMALL])
    return manager


def test_walking_spiders_are_never_shoved_back_onto_a_screen(monkeypatch):
    """Count how often the manager's backstop has to move a spider's body.

    Each time it does, the body jumps and the planted feet do not: the glitch
    the owner saw. Four hunters with flies about, twenty simulated seconds.
    Measured before this package, over a minute: 2351 to 3794 shoves of up to
    233px. After: none while walking. A leap from one monitor to the other
    can still cut across the corner mid-air and be nudged by up to ~13px,
    which this allows for.
    """
    manager = _hunting_colony(monkeypatch, seed=11)
    backstop = manager._keep_on_a_real_screen
    walking = []

    def counting(creature):
        before = (creature.x, creature.y)
        backstop(creature)
        moved = abs(creature.x - before[0]) + abs(creature.y - before[1])
        if moved and not creature.airborne:
            walking.append((creature.state, round(moved, 1)))

    manager._keep_on_a_real_screen = counting
    for _ in range(20 * 60):
        manager.update(DT, *AWAY)
    assert not walking, f"shoved back {len(walking)} times while walking: {walking[:8]}"
