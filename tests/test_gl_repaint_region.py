"""The GL overlay does not build a dirty region it cannot use.

`paintGL` redraws the whole framebuffer whatever region `update` is given,
so on the GL path (the default since DC-78) building the region every frame
was wasted work. The software path still needs it: there the region decides
what is cleared and redrawn, and the manager culls spiders against it.
"""

from __future__ import annotations

import pytest
from PyQt5.QtCore import QRect
from PyQt5.QtGui import QRegion

from desktop_bug.app import engine


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Importing engine touches Qt types."""


class _Overlay:
    """Just enough of OverlayWindow for the repaint request."""

    request_repaint = engine.OverlayWindow.request_repaint

    def __init__(self):
        self._prev_region = QRegion()
        self._full_repaint_pending = False
        self._frames_since_full_repaint = 0
        self.region_builds = 0
        self.updates = []

    def _current_paint_region(self):
        self.region_builds += 1
        return QRegion(QRect(10, 10, 40, 40))

    def update(self, *region):
        self.updates.append(region)


def test_gl_overlay_repaints_everything_without_building_a_region(monkeypatch):
    monkeypatch.setattr(engine, "GL_OVERLAY", True)
    overlay = _Overlay()
    for _ in range(engine.FULL_REPAINT_SAFETY_FRAMES + 5):
        overlay.request_repaint()
    assert overlay.region_builds == 0
    assert overlay.updates and all(args == () for args in overlay.updates)


def test_software_overlay_still_repaints_only_the_dirty_region(monkeypatch):
    monkeypatch.setattr(engine, "GL_OVERLAY", False)
    overlay = _Overlay()
    overlay.request_repaint()
    overlay.request_repaint()
    assert overlay.region_builds == 2
    assert overlay.updates[-1] and isinstance(overlay.updates[-1][0], QRegion)
