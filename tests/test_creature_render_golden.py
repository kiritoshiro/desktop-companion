"""A seeded colony must match the committed creature rendering reference.

The reference was refreshed for uniform baseline spider size and level-based
name styling. Any later rendering change should be reviewed before replacing it.
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
    # This frame pins how spiders are drawn. Labels became on by default at
    # the owner's request; off here, their boxes would cover the spiders
    # (and headless Qt has no fonts, so they come out as wide empty bars).
    for setter in (manager.set_always_show_names, manager.set_always_show_levels,
                   manager.set_always_show_health, manager.set_always_show_xp,
                   manager.set_always_show_stamina):
        setter(False)
    # Webs and the cursor-silk world used not to be part of DC-09's seeding,
    # so silk drawn during the run made this golden frame depend on the
    # module-level random state instead of only on `seed`. Turning off every
    # web-related skill kept the one thing this test cares about -- did the
    # split change the picture -- decoupled from that gap.
    #
    # DC-68 closed the gap: each world now draws from a stream derived from
    # the run seed. Measured afterwards, two in-process renders differ by
    # 0/255 with this workaround removed as well as with it. It is kept
    # because dropping it would mean regenerating the reference for no gain
    # here, not because it is still load-bearing -- a later package that
    # wants this frame to cover silk can simply delete these two lines and
    # regenerate.
    for creature in manager.creatures:
        for skill_id in ("weave_web", "web_walk", "shoot_web", "wall_web", "drift"):
            creature.skills.set_enabled(skill_id, False)
    for _ in range(FRAMES):
        manager.update(DT, -100000.0, -100000.0)
    # FlyWorld.ensure_spawner() plants a nest marker at screen-centre even
    # with flies off, and FlySpawner carried a pulse phase from flies.py's own
    # module-level random -- confirmed at the time by comparing two in-process
    # runs with identical creature state that still disagreed on a ~44x40
    # patch centred on (screen_w/2, screen_h/2). That was the same gap DC-68
    # closed; the spawner now draws from the fly world's seeded stream and two
    # runs agree to 0/255 with this line removed. Kept for the same reason as
    # the skills above.
    manager.fly_world.spawners.clear()
    # A fresh cache per process is deterministic; a *reused* one is not once
    # more than one manager has been built in the same run (its keys are
    # asset paths, not instance ids, but this sidesteps needing to know why).
    Creature.SPRITE_CACHE.clear()
    # Damage numbers are text, like the labels switched off above: glyphs
    # depend on the fonts a machine has, and this frame pins how spiders are
    # drawn. Two spiders here are mid-fight, so clear their numbers.
    for creature in manager.creatures:
        creature.damage_numbers.clear()
    image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    manager.render(painter)
    painter.end()
    return image


# How far a live render may be from a committed PNG. The PNG round trip is not
# quite lossless (see `max_channel_difference`), and a CI runner can round an
# anti-aliased edge one step differently from the machine that wrote the
# reference: three variant checks that demanded 0 failed on CI at 1/255 twice
# on 2026-09-25 (PRs #75 and #80) and passed on rerun of the same commit. Two
# live renders in one process are still compared exactly.
PNG_TOLERANCE = 2


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


def test_creature_render_matches_reference(monkeypatch):
    # A fresh, private state directory: otherwise this reads whatever
    # progression a real launch happened to save (or nothing, non-deterministic
    # either way), and a spider's size and health bar both depend on its level.
    # Plain tempfile rather than pytest's tmp_path fixture, which numbers its
    # directories by scanning a shared parent that can be locked by another
    # process (antivirus, a leftover handle) on a machine this never ran on
    # before -- a rendering test should not fail over that.
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="golden-render-"))
    live = render_reference_frame()
    assert GOLDEN.exists(), "no creature rendering reference image exists"
    reference = QImage(str(GOLDEN))
    assert not reference.isNull(), f"could not load {GOLDEN}"
    worst = max_channel_difference(live, reference)
    assert worst <= PNG_TOLERANCE, (
        f"the rendered colony differs from the committed reference by up to "
        f"{worst}/255 on one channel"
    )
    # And the comparison has to be capable of failing, or it proves nothing.
    blank = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    blank.fill(0xFFFFFFFF)
    assert max_channel_difference(live, blank) > 2, "comparison is not comparing anything"
