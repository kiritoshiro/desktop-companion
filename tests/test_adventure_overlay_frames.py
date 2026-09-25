"""The real Adventure overlay must survive its own frames.

The take-control hint's rectangle was once marked a static method while it
still read `self`, so every Adventure frame raised a TypeError in the repaint
region and in painting. The unit tests built stand-in windows and never ran a
real Adventure tick, so nothing noticed. This runs real ticks and a real paint.
"""

import random

import pytest
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Needs the one Qt application object (see `conftest.qapp`)."""


def test_adventure_overlay_ticks_and_paints() -> None:
    from desktop_bug.app.engine import OverlayWindow

    random.seed(3)
    window = OverlayWindow(ROOT / "presets" / "colony.json", mode="adventure")
    try:
        assert window.mode == "adventure"
        for _ in range(4):
            window.tick()
        region = window._current_paint_region()
        hint = window._adventure_hint_rect()
        assert not hint.isEmpty()
        assert region.contains(hint.center()), "the hint is never repainted"
        window.resize(900, 600)
        image = window.grab().toImage()
        assert not image.isNull()
    finally:
        window.timer.stop()
        window.style_timer.stop()
        window.deleteLater()
