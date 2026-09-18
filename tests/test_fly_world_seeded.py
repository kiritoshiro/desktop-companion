"""Flies draw the same picture on a replayed run too: DC-40.

DC-09 seeded `Creature`/`CreatureManager` but deliberately left `flies.py`
drawing from the module-level `random` (see tests/test_seeded_randomness.py).
Building DC-11's golden-image test surfaced a concrete consequence: two
headless colony runs with the same seed and identical final creature state
still rendered different pixels in a small patch centred on the screen, because
`FlySpawner`'s pulse animation -- and every other visual roll in `flies.py` --
was seeded from `random` directly, not from `CreatureManager`'s `seed`.

`FlyWorld` (and everything it owns: `Fly`, `FlySpawner`, `FlyRemains`) now
accepts an injected `random.Random`, wired from `CreatureManager` the same way
`BaseWorld` already was. A manager built with the same seed for the same
preset, flies included, must now retrace the same fly-world state -- and
render the same pixels -- every time.

`webs.py` and `mouse_webs.py` still draw from the module-level `random`
directly; that gap is unchanged by this package (recording it here for the
same reason DC-09 recorded the flies.py gap: naming a disclosed limit rather
than leaving it to be rediscovered).
"""

from __future__ import annotations

from PyQt5.QtGui import QImage, QPainter
import pytest

from desktop_bug.manager import CreatureManager
from support import ROOT

PRESET = ROOT / "presets" / "colony.json"
FRAMES = 600
DT = 1.0 / 60.0
SCREEN = (960, 640)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering needs a live QApplication; see conftest.qapp for why."""


def fly_world_trace(seed: int) -> list[tuple]:
    """One state sequence for everything `flies.py` draws, per frame."""
    manager = CreatureManager(PRESET, *SCREEN, seed=seed)
    manager.base_world.clear()
    trace: list[tuple] = []
    for _ in range(FRAMES):
        manager.update(DT, -5000.0, -5000.0)
        fw = manager.fly_world
        frame = (
            tuple(round(sp.pulse, 6) for sp in fw.spawners),
            tuple(
                (round(f.x, 6), round(f.y, 6), round(f.heading, 6),
                 f.motion_mode, round(f.wing_phase, 6))
                for f in fw.flies
            ),
        )
        trace.append(frame)
    return trace


def render_frame(seed: int) -> QImage:
    """Run a fixed seeded colony (flies included) and paint the final frame."""
    manager = CreatureManager(PRESET, *SCREEN, seed=seed)
    manager.base_world.clear()
    for _ in range(FRAMES):
        manager.update(DT, -5000.0, -5000.0)
    image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    manager.fly_world.render(painter)
    painter.end()
    return image


def images_differ(left: QImage, right: QImage) -> bool:
    fmt = QImage.Format_ARGB32
    a, b = left.convertToFormat(fmt), right.convertToFormat(fmt)
    assert a.size() == b.size()
    for y in range(0, a.height(), 2):
        for x in range(0, a.width(), 2):
            if a.pixel(x, y) != b.pixel(x, y):
                return True
    return False


def test_the_same_seed_retraces_the_same_fly_world_state():
    first = fly_world_trace(20260918)
    second = fly_world_trace(20260918)
    assert first == second


def test_a_different_seed_diverges():
    baseline = fly_world_trace(20260918)
    other = fly_world_trace(1)
    assert baseline != other


def test_the_same_seed_renders_the_same_pixels():
    """The bug this package fixes: same seed, same state, different pixels."""
    first = render_frame(20260918)
    second = render_frame(20260918)
    assert not images_differ(first, second)


def test_an_unseeded_run_still_works():
    """No seed must not crash or hang; it only forfeits replay."""
    manager = CreatureManager(PRESET, *SCREEN)
    for _ in range(120):
        manager.update(DT, -5000.0, -5000.0)
    assert len(manager.creatures) == 4
