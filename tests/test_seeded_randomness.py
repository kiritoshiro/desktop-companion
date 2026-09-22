"""Any run can be replayed: DC-09.

`Creature` made 185 calls against the module-level `random`, so no run
could be seeded or reproduced -- a bug report could carry no repro, and a
golden-image test could never be stable. `CreatureManager` and `Creature`
now accept a `seed`; every one of those calls goes through a `random.Random`
built from it instead, and a manager built with the same seed for the same
preset must trace the same path every time.
"""

from __future__ import annotations

import os
import tempfile
from contextlib import contextmanager

from desktop_bug.manager import CreatureManager
from support import ROOT

PRESET = ROOT / "presets" / "colony.json"
FRAMES = 600
DT = 1.0 / 60.0


@contextmanager
def _own_state_dir():
    """Run with an empty, private state directory, restored afterwards.

    The suite shares one state directory by default, and a run of this length
    auto-saves partway through. Since DC-23 that save carries the scene -- the
    webs these spiders wove -- so a second run would start from the first
    run's desktop rather than a bare one, and the two traces would differ for
    a reason that has nothing to do with seeding. conftest's own `state_dir`
    fixture states the rule; this test needs a fresh directory per *run*, not
    per test, so it does the same thing inline.
    """
    previous = os.environ.get("DESKTOP_BUG_STATE_DIR")
    os.environ["DESKTOP_BUG_STATE_DIR"] = tempfile.mkdtemp(prefix="dc09-replay-")
    try:
        yield
    finally:
        if previous is None:
            os.environ.pop("DESKTOP_BUG_STATE_DIR", None)
        else:
            os.environ["DESKTOP_BUG_STATE_DIR"] = previous


def run(seed: int) -> list[tuple]:
    """One state sequence: (x, y, heading, state) per spider, per frame."""
    with _own_state_dir():
        return _trace(seed)


def _trace(seed: int) -> list[tuple]:
    manager = CreatureManager(PRESET, 1600, 900, seed=seed)
    manager.base_world.clear()
    # Flies and webs used to be their own worlds with their own module-level
    # randomness, so this test turned flies off to stay inside what DC-09
    # actually promised rather than fail on a gap it did not create.
    #
    # DC-68 closed that gap -- each world now draws from a stream derived from
    # the run seed -- so the flies stay on and this test finally covers the
    # whole claim its docstring makes. That is the point of the package: the
    # promise and the test now say the same thing.
    trace: list[tuple] = []
    for _ in range(FRAMES):
        # Off screen, so nothing here depends on the mouse nudging a spider
        # onto a different branch between the two runs being compared.
        manager.update(DT, -5000.0, -5000.0)
        trace.append(
            tuple(
                (round(c.x, 6), round(c.y, 6), round(c.heading, 6), c.state)
                for c in manager.creatures
            )
        )
    return trace


def test_the_same_seed_retraces_the_same_path():
    first = run(11)
    second = run(11)
    assert first == second


def test_a_different_seed_diverges():
    baseline = run(11)
    other = run(12)
    assert baseline != other


def test_an_unseeded_run_still_works():
    """No seed must not crash or hang; it only forfeits replay."""
    manager = CreatureManager(PRESET, 1600, 900)
    for _ in range(120):
        manager.update(DT, -5000.0, -5000.0)
    assert len(manager.creatures) == 5
