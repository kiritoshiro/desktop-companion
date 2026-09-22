"""A seeded run replays exactly, flies and silk included (DC-68).

DC-09 promised that a seeded colony replays. It did not, and the gap has been
disclosed in this repository for several packages -- `WebWorld.claim_site`
still carried a docstring calling webs.py *"a disclosed, out-of-scope gap in
DC-09/DC-40's seeding pass"*. The three world modules drew straight from the
module-level ``random``, so nothing the manager's seed did could reach them.

The cost was not theoretical. **Five measurements in this project have been
invalidated by it**, four of them colony-economy numbers and one -- in the
session that produced this package -- a leg-rendering measurement that
produced a clean, plausible, entirely false story before three repeat runs
disagreed about which leg was worst.

Measured before this package: three spiders, one seed, 900 frames, run twice
in the same process.

    flies off : identical
    flies on  : spiders hundreds of pixels apart
                (51.7, 84.4) vs (63.6, 60.6)
                (45.7, 78.6) vs (604.0, 489.1)

Each world now takes an ``rng``. With no seed it is handed ``random`` itself,
which is exactly what the files used before, so an unseeded launch behaves as
it always has -- that is what `test_an_unseeded_world_still_uses_the_module`
below pins.

The streams are derived **by name** (``f"{seed}:flies"``), not drawn from one
shared generator. DC-09 recorded what happens otherwise: adding a single draw
in a constructor shifted every later value and broke a leg-geometry invariant
two packages away. Naming them means a world added later cannot disturb the
numbers an existing one draws.
"""

from __future__ import annotations

import json
import random
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.world.flies import FlyWorld
from desktop_bug.world.mouse_webs import MouseWebWorld
from desktop_bug.world.webs import WebWorld

DT = 1.0 / 60.0
SCREEN = (1200, 800)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


# ------------------------------------------------------------------ flies

def _fly_world(seed):
    world = FlyWorld(*SCREEN, enabled=True, min_interval=0.2, max_interval=0.4,
                     max_flies=8, rng=random.Random(seed))
    for _ in range(int(20.0 * 60)):
        world.update(DT, [], None)
    return [(round(f.x, 5), round(f.y, 5), round(f.heading, 6)) for f in world.flies]


def test_a_seeded_fly_world_replays():
    assert _fly_world(11) == _fly_world(11)
    assert len(_fly_world(11)) > 0, "no flies were spawned; the test proves nothing"


def test_two_seeds_give_two_different_flocks():
    """A stream that ignores its seed would pass the test above."""
    assert _fly_world(11) != _fly_world(12)


# ------------------------------------------------------------------- webs

class _Weaver:
    """The least a `claim_site` caller can be: a position and a stream."""

    def __init__(self, x, y, rng):
        self.x, self.y = x, y
        self.rng = rng


def _web_world(seed):
    world = WebWorld(*SCREEN, rng=random.Random(seed))
    picker = random.Random(seed + 500)
    for i in range(6):
        weaver = _Weaver(120.0 + i * 130.0, 110.0 + (i % 3) * 190.0, picker)
        world.claim_site(weaver, prefer_corner=(i % 2 == 0), rng=picker)
    return [(web.pattern,
             (round(web.hub[0], 5), round(web.hub[1], 5)),
             len(web.strands),
             tuple(round(v, 5) for v in web.walkable_point(*SCREEN)))
            for web in world.webs]


def test_a_seeded_web_world_replays():
    """The silk *geometry* was the part nobody seeded: `claim_site` already
    took an rng for which site and which pattern, but the strands themselves,
    and `walkable_point` -- which decides where a spider steps onto a web --
    came from the module."""
    assert _web_world(5) == _web_world(5)
    assert len(_web_world(5)) > 0, "no webs were built; the test proves nothing"


def test_two_seeds_give_two_different_webs():
    assert _web_world(5) != _web_world(6)


def test_where_a_spider_steps_onto_a_web_is_replayable():
    """Singled out because it is the one web roll that changes behaviour
    rather than appearance."""
    a = WebWorld(*SCREEN, rng=random.Random(3))
    b = WebWorld(*SCREEN, rng=random.Random(3))
    wa = a.claim_site(_Weaver(200.0, 200.0, random.Random(9)), rng=random.Random(9))
    wb = b.claim_site(_Weaver(200.0, 200.0, random.Random(9)), rng=random.Random(9))
    assert wa is not None and wb is not None
    assert [wa.walkable_point(*SCREEN) for _ in range(12)] == \
           [wb.walkable_point(*SCREEN) for _ in range(12)]


# ------------------------------------------------------------- mouse webs

def _mouse_world(seed):
    """Fire a glob at the pointer and record what the seed actually drives.

    The pointer position the world asks for is *not* it: that is leash
    physics and comes out the same whatever the seed. What the stream rolls
    is the silk -- the anchor pegs and the wobble phase. The first version of
    this test watched the desired pointer position, which made
    `test_two_seeds_give_two_different_globs` compare two identical lists of
    Nones and pass while measuring nothing.
    """
    world = MouseWebWorld(*SCREEN, can_control=True, rng=random.Random(seed))
    world.shoot((100.0, 100.0), (600.0, 400.0), kind="trap")
    for i in range(int(12.0 * 60)):
        world.update(DT, 600.0 + (i % 17), 400.0 + (i % 13))
        hold = getattr(world.capture, "hold", None)
        if hold is not None:
            return (round(hold.wob_phase, 6),
                    [((round(px, 6), round(py, 6)), round(w, 6))
                     for (px, py), w in hold.pegs])
    raise AssertionError("the glob never caught the pointer; nothing measured")


def test_a_seeded_mouse_web_world_replays():
    assert _mouse_world(4) == _mouse_world(4)


def test_two_seeds_give_two_different_globs():
    assert _mouse_world(4) != _mouse_world(7)


# -------------------------------------------------------- the whole colony

def _colony(seed, frames=900, flies=True):
    scratch = Path(tempfile.mkdtemp(prefix="dc68-"))
    import os
    os.environ["DESKTOP_BUG_STATE_DIR"] = str(scratch / "state")
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": 3,
                   "slot_id": "a", "team": "hunters"}],
        "settings": {"flies": {"enabled": flies, "spawner": flies}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=seed)
    for _ in range(frames):
        manager.update(DT, *AWAY)
    return [(round(c.x, 4), round(c.y, 4), round(c.heading, 5))
            for c in manager.creatures]


def test_a_seeded_colony_replays_with_flies_on():
    """The measurement that started this: identical with flies off, hundreds
    of pixels apart with flies on."""
    assert _colony(42) == _colony(42)


def test_a_seeded_colony_still_depends_on_its_seed():
    assert _colony(42) != _colony(43)


def test_the_colony_replays_across_the_three_worlds_together():
    """Run long enough that flies spawn, get hunted and leave remains."""
    assert _colony(8, frames=1800) == _colony(8, frames=1800)


# --------------------------------------------------- and nothing else moved

def test_an_unseeded_world_still_uses_the_module():
    """The whole compatibility promise in one line. An unseeded launch must
    behave exactly as it did: `random` itself, not a private generator that
    `random.seed()` cannot reach."""
    assert FlyWorld(*SCREEN).rng is random
    assert WebWorld(*SCREEN).rng is random
    assert MouseWebWorld(*SCREEN, can_control=False).rng is random


def test_an_unseeded_manager_hands_its_worlds_the_module(qapp, monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="dc68u-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": 1,
                   "slot_id": "a"}],
        "settings": {},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN)          # no seed
    assert manager.fly_world.rng is random
    assert manager.web_world.rng is random
    assert manager.mouse_web_world.rng is random


def test_each_world_gets_its_own_named_stream(qapp, monkeypatch):
    """Derived by name, not taken from one shared generator. DC-09 recorded
    what the shared kind costs: one extra draw in a constructor shifted every
    later value and broke a leg-geometry invariant two packages away."""
    scratch = Path(tempfile.mkdtemp(prefix="dc68n-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": 1,
                   "slot_id": "a"}],
        "settings": {},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=77)
    streams = [manager.fly_world.rng, manager.web_world.rng,
               manager.mouse_web_world.rng]
    assert all(s is not random for s in streams)
    assert len({id(s) for s in streams}) == 3
    # Different names, so different sequences -- not three copies of one seed.
    assert len({s.random() for s in streams}) == 3
