"""The drawn base does not change by accident (DC-41).

`tests/test_base_mounds.py` checks the mound *model* -- how many exist, where
they sit, that none of them move. This checks the picture, which is what the
package is actually about: a part-built base and a finished one, rendered to
an offscreen image and compared against a committed reference.

The reference was generated from this file's own helper, the same way DC-11
generated the creature one. Regenerate it only for a change to how a base
looks that you intend, and say so in the work-log entry.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest
from PyQt5.QtGui import QColor, QImage, QPainter

from desktop_bug.world.jobs import MAX_BUILD_PROGRESS, BaseSite, BaseWorld

GOLDEN = Path(__file__).parent / "golden" / "base_render.png"
SCREEN = (620, 260)
# Two bases, one part-way through and one finished, at different levels so the
# reference covers both the growing pile and a full one.
CASES = ((0.4, 2), (1.0, 5))


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering needs a live QApplication; see conftest.qapp for why."""


def render_reference_frame() -> QImage:
    """Paint the fixed set of bases this test compares."""
    world = BaseWorld(*SCREEN)
    for index, (completion, level) in enumerate(CASES):
        site = BaseSite(
            id=f"golden|base-{index}", owner_id="owner", team_id="hunters",
            x=160.0 + index * 300.0, y=130.0,
            build_progress=MAX_BUILD_PROGRESS * completion, level=level,
        )
        world.bases[site.id] = site

    image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(18, 20, 26))
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    world.render(painter)
    painter.end()
    return image


def max_channel_difference(a: QImage, b: QImage) -> int:
    if a.size() != b.size():
        return 255
    worst = 0
    for y in range(0, a.height(), 2):
        for x in range(0, a.width(), 2):
            pa, pb = a.pixel(x, y), b.pixel(x, y)
            for shift in (0, 8, 16, 24):  # A, R, G, B byte lanes
                worst = max(worst, abs(((pa >> shift) & 0xFF) - ((pb >> shift) & 0xFF)))
    return worst


def test_the_base_renders_the_same_way_it_did(monkeypatch):
    # A private state directory, so this never reads a real saved colony.
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="base-golden-"))
    live = render_reference_frame()
    assert GOLDEN.exists(), (
        "no reference image yet -- generate it with render_reference_frame() "
        "in this file before changing how a base is drawn"
    )
    reference = QImage(str(GOLDEN))
    assert not reference.isNull(), f"could not load {GOLDEN}"
    worst = max_channel_difference(live, reference)
    assert worst <= 2, (
        f"the rendered base differs from the committed reference by up to "
        f"{worst}/255 on one channel"
    )

    # And the comparison has to be capable of failing, or it proves nothing.
    blank = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    blank.fill(QColor(18, 20, 26))
    assert max_channel_difference(blank, reference) > 2, (
        "an empty frame matches the reference, so this test cannot fail"
    )


def test_the_drawn_base_is_deterministic():
    """Two renders in one process must agree, or the reference means nothing."""
    assert max_channel_difference(render_reference_frame(), render_reference_frame()) == 0
