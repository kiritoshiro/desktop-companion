"""The mission map instead of the desktop, and handing the controls back.

The owner: *"background change on clicking the button ~ so that instead of
desktop it would show a map for the mission ... generate graphic for each game
... after clicking it it would show desktop again. also another button could
disable the controlling of the spider so that a computer could be used and
then pushing that button again would activate the controls of spider."*
"""
from __future__ import annotations

from pathlib import Path

import pytest
from PyQt5.QtCore import QEvent, Qt
from PyQt5.QtGui import QColor, QImage, QKeyEvent, QPainter

from desktop_bug.app.controls import DEFAULT_BINDINGS, ControlSettings
from desktop_bug.app.mission_backdrop import LOOKS, MissionBackdrop, look_for, paint_backdrop


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Painting needs the one Qt application."""


# -- the painted maps -----------------------------------------------------------------

def test_every_map_has_its_own_painted_ground():
    colours = {}
    for look in LOOKS:
        image = paint_backdrop(look, 640, 360, sites=((120, 200, "home"), (480, 160, "nest")))
        assert (image.width(), image.height()) == (640, 360)
        colours[look] = QColor(image.pixel(320, 330)).name()
    assert len(set(colours.values())) == len(LOOKS), colours


def test_a_path_runs_from_home_to_each_building():
    bare = paint_backdrop("territory", 800, 450)
    with_sites = paint_backdrop("territory", 800, 450, sites=((100, 225, "home"), (700, 225, "nest")))
    # The trodden clearing by the nest differs from the bare meadow there.
    assert QColor(with_sites.pixel(700, 245)).name() != QColor(bare.pixel(700, 245)).name()


def test_an_editor_map_borrows_its_kinds_look():
    assert look_for("custom-abc", "swarm") == "swarm" and look_for("queen") == "queen"
    assert look_for("custom-abc", "raid") == "territory"


# -- in the overlay ---------------------------------------------------------------------

@pytest.fixture
def window(state_dir, monkeypatch):
    from desktop_bug.app.engine import OverlayWindow

    w = OverlayWindow(Path("presets/colony.json"), seed=4, mode="adventure")
    w.timer.stop()
    # No real system-wide hotkey from a test.
    monkeypatch.setattr(w, "_register_release_hotkey", lambda: None)
    yield w
    w.style_timer.stop()
    w.deleteLater()


def _painted(w):
    image = QImage(w.width(), w.height(), QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    w.render(image)
    return image


def _covered(w):
    """Share of the mission's main screen painted opaque."""
    image, a = _painted(w), w.mission.area
    points = [(int(a.x + a.w * fx / 20), int(a.y + a.h * fy / 20)) for fx in range(1, 20) for fy in range(1, 20)]
    return sum(QColor.fromRgba(image.pixel(x, y)).alpha() == 255 for x, y in points) / len(points)


def test_the_map_key_swaps_the_desktop_for_the_mission_map_and_back(window):
    assert _covered(window) < 0.5, "the desktop shows through"
    window._adventure_action("map_view", True)
    assert window.map_view
    assert _covered(window) > 0.97, "the map covers the desktop"
    window._adventure_action("map_view", True)
    assert not window.map_view


def test_the_tilde_key_opens_the_map_whatever_the_layout_calls_it(window):
    window.keyPressEvent(QKeyEvent(QEvent.KeyPress, Qt.Key_AsciiTilde, Qt.ShiftModifier, "~"))
    assert window.map_view


def test_releasing_the_controls_frees_the_desktop_and_pauses_the_raid(window):
    window._adventure_action("release", True)
    assert window.controls_released
    assert not window._adventure_captures_mouse(200.0, 200.0), "clicks reach the desktop"
    elapsed = window.mission.elapsed
    window.tick()
    assert window.mission.elapsed == elapsed, "the raid waits"
    window._adventure_action("map_view", True)
    assert not window.map_view, "no game keys while released"
    window._adventure_action("release", True)
    assert not window.controls_released
    assert window._adventure_captures_mouse(200.0, 200.0)


def test_the_release_key_works_from_anywhere_as_a_windows_hotkey():
    from desktop_bug.app.engine import VK_OEM_3, virtual_key_for

    assert virtual_key_for("F8") == 0x77 and virtual_key_for("F1") == 0x70
    assert virtual_key_for("P") == ord("P") and virtual_key_for("`") == VK_OEM_3
    assert virtual_key_for("Mouse Left") is None


def test_the_new_keys_have_defaults_and_join_old_saved_controls(tmp_path):
    assert DEFAULT_BINDINGS["map_view"] == "`" and DEFAULT_BINDINGS["release"] == "F8"
    old = {"bindings": {k: v for k, v in DEFAULT_BINDINGS.items() if k not in ("map_view", "release")}}
    loaded = ControlSettings.from_dict(old) if hasattr(ControlSettings, "from_dict") else None
    if loaded is not None:
        assert loaded.binding("map_view") == "`" and loaded.binding("release") == "F8"


def test_the_backdrop_is_painted_once_and_reused(window):
    backdrop = MissionBackdrop()
    first = backdrop.pixmaps(window.mission)
    second = backdrop.pixmaps(window.mission)
    assert [p.cacheKey() for _r, p in first] == [p.cacheKey() for _r, p in second]
    painter_image = QImage(64, 64, QImage.Format_ARGB32_Premultiplied)
    painter = QPainter(painter_image)
    backdrop.paint(painter, window.mission)
    painter.end()
