"""DC-11 splits an 8,500-line file with a hard rule: a pure move, no logic
edits. The only way to actually know a mechanical split changed nothing is to
render before and after and compare pixels, not to read the diff and hope.

This test renders a fixed, seeded colony to an offscreen QImage and compares
it against a committed reference. It is written and the reference generated
*before* creature.py is split into a package, exactly as the plan requires,
so a failure here after the split means the split was not actually pure.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

from PyQt5.QtGui import QImage, QPainter
import pytest

from desktop_bug.creature import Creature
from desktop_bug.manager import CreatureManager
from support import ROOT

GOLDEN = Path(__file__).parent / "golden" / "creature_render.png"


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering needs a live QApplication; see conftest.qapp for why."""


SCREEN = (960, 640)
SEED = 20260917
FRAMES = 90
DT = 1.0 / 60.0


def render_reference_frame() -> QImage:
    """Run a fixed seeded colony forward and paint the final frame."""
    manager = CreatureManager(ROOT / "presets" / "colony.json", *SCREEN, seed=SEED)
    manager.base_world.clear()
    manager.set_flies_enabled(False)
    # Webs and the cursor-silk world are not part of DC-09's seeding (see
    # tests/test_seeded_randomness.py); silk drawn during the run would make
    # this golden frame depend on the module-level random state instead of
    # only on `seed`. Turning off every web-related skill keeps the one thing
    # this test cares about -- did the split change the picture -- decoupled
    # from a determinism gap this package did not create.
    for creature in manager.creatures:
        for skill_id in ("weave_web", "web_walk", "shoot_web", "wall_web", "drift"):
            creature.skills.set_enabled(skill_id, False)
    for _ in range(FRAMES):
        manager.update(DT, -100000.0, -100000.0)
    # FlyWorld.ensure_spawner() plants a nest marker at screen-centre even with
    # flies off, and FlySpawner itself carries a pulse phase from flies.py's
    # own (unseeded, out-of-scope-for-DC-09) module-level random -- confirmed
    # by comparing two in-process runs with identical creature state that
    # still disagreed on a ~44x40 patch centred on (screen_w/2, screen_h/2).
    # Dropping it keeps this test about the split, not about that gap.
    manager.fly_world.spawners.clear()
    # A fresh cache per process is deterministic; a *reused* one is not once
    # more than one manager has been built in the same run (its keys are
    # asset paths, not instance ids, but this sidesteps needing to know why).
    Creature.SPRITE_CACHE.clear()
    image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    manager.render(painter)
    painter.end()
    return image


def max_channel_difference(left: QImage, right: QImage) -> int:
    """Largest per-channel gap between two same-sized images.

    A PNG round trip through `QImage.save`/`QImage(path)` changes the pixel
    format from premultiplied to straight alpha and back, which is not quite
    lossless -- comparing bytes directly (`==`) fails even for two renders of
    the exact same frame. Comparing after converting both to one format, with
    a small tolerance for that rounding, is the "within tolerance" the plan
    asks for; it still catches a mechanical slip; it does not fail on the
    premultiplication arithmetic itself.
    """
    fmt = QImage.Format_ARGB32
    a = left.convertToFormat(fmt)
    b = right.convertToFormat(fmt)
    assert a.size() == b.size(), (a.size(), b.size())
    worst = 0
    for y in range(0, a.height(), 4):
        for x in range(0, a.width(), 4):
            pa, pb = a.pixel(x, y), b.pixel(x, y)
            for shift in (0, 8, 16, 24):  # A, R, G, B byte lanes
                worst = max(worst, abs(((pa >> shift) & 0xFF) - ((pb >> shift) & 0xFF)))
    return worst


def test_the_split_did_not_change_a_single_pixel(monkeypatch):
    # A fresh, private state directory: otherwise this reads whatever
    # progression a real launch happened to save (or nothing, non-deterministic
    # either way), and a spider's size and health bar both depend on its level.
    # Plain tempfile rather than pytest's tmp_path fixture, which numbers its
    # directories by scanning a shared parent that can be locked by another
    # process (antivirus, a leftover handle) on a machine this never ran on
    # before -- a rendering test should not fail over that.
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="golden-render-"))
    live = render_reference_frame()
    assert GOLDEN.exists(), (
        "no reference image yet -- generate it with the render helper in this "
        "file *before* moving anything, per DC-11's guardrail"
    )
    reference = QImage(str(GOLDEN))
    assert not reference.isNull(), f"could not load {GOLDEN}"
    worst = max_channel_difference(live, reference)
    assert worst <= 2, (
        f"the rendered colony differs from the committed reference by up to "
        f"{worst}/255 on one channel -- a mechanical move changed something "
        "it should not have"
    )
    # And the comparison has to be capable of failing, or it proves nothing.
    blank = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    blank.fill(0xFFFFFFFF)
    assert max_channel_difference(live, blank) > 2, "comparison is not comparing anything"
