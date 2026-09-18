"""Any run can be replayed: DC-09.

`Creature` made 185 calls against the module-level `random`, so no run
could be seeded or reproduced -- a bug report could carry no repro, and a
golden-image test could never be stable. `CreatureManager` and `Creature`
now accept a `seed`; every one of those calls goes through a `random.Random`
built from it instead, and a manager built with the same seed for the same
preset must trace the same path every time.
"""

from __future__ import annotations

from desktop_bug.manager import CreatureManager
from support import ROOT

PRESET = ROOT / "presets" / "colony.json"
FRAMES = 600
DT = 1.0 / 60.0


def run(seed: int) -> list[tuple]:
    """One state sequence: (x, y, heading, state) per spider, per frame."""
    manager = CreatureManager(PRESET, 1600, 900, seed=seed)
    manager.base_world.clear()
    # Flies are seeded too since DC-40 (see tests/test_fly_world_seeded.py),
    # but from their own `Random` instance, independent of this manager's
    # `_rng` by design -- so a fly nest existing does not shift this trace.
    # Webs are still unseeded (DC-09 named Creature/CreatureManager; DC-40
    # named flies.py only), and turning flies off here as well keeps this
    # test about exactly what it traces: creature state, not fly state.
    manager.set_flies_enabled(False)
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
    assert len(manager.creatures) == 4
