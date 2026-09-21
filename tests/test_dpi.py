"""DC-14: high-DPI Qt attributes and physical/logical coordinate conversions.

Qt.AA_EnableHighDpiScaling makes Qt report coordinates in device-independent
("logical") pixels, scaled per monitor. Every raw Win32 call made through
ctypes in this codebase (SetCursorPos, GetWindowRect, ...) still deals in
native ("physical") pixels regardless. `desktop_bug.dpi` holds the pure
conversion between the two, and this module proves both the conversion math
itself (round trips at 1.0/1.5/2.0, the ratios named in the plan's acceptance
line) and the two call sites in `engine.py` that actually need it.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from desktop_bug.support import dpi

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object."""


# ----------------------------------------------------------------------
# Pure conversion math
# ----------------------------------------------------------------------

_POINTS = [(0.0, 0.0), (123.0, 45.0), (-10.0, 999.5), (1920.0, 1080.0), (3000.25, 10.75)]


@pytest.mark.parametrize("ratio", [1.0, 1.5, 2.0])
def test_logical_to_physical_then_back_round_trips(ratio):
    for x, y in _POINTS:
        px, py = dpi.logical_to_physical(x, y, ratio)
        assert px == pytest.approx(x * ratio)
        assert py == pytest.approx(y * ratio)
        lx, ly = dpi.physical_to_logical(px, py, ratio)
        assert lx == pytest.approx(x)
        assert ly == pytest.approx(y)


@pytest.mark.parametrize("ratio", [1.0, 1.5, 2.0])
def test_physical_to_logical_then_back_round_trips(ratio):
    for x, y in _POINTS:
        lx, ly = dpi.physical_to_logical(x, y, ratio)
        px, py = dpi.logical_to_physical(lx, ly, ratio)
        assert px == pytest.approx(x)
        assert py == pytest.approx(y)


@pytest.mark.parametrize("ratio", [1.0, 1.5, 2.0])
def test_ratio_above_one_makes_physical_the_larger_number(ratio):
    # Sanity check on direction, not just round-trip symmetry: a scaled-up
    # monitor's native pixel grid is bigger than its logical one, so a wrong
    # multiply/divide swap would still round-trip correctly but move points
    # the wrong way, which none of the round-trip tests above would catch.
    px, py = dpi.logical_to_physical(100.0, 100.0, ratio)
    if ratio > 1.0:
        assert px > 100.0 and py > 100.0
    else:
        assert px == pytest.approx(100.0)


@pytest.mark.parametrize("bad_ratio", [0.0, -1.0, -2.5])
def test_non_positive_ratio_degrades_to_identity(bad_ratio):
    """A failed DPR lookup must not divide by zero or flip a sign."""
    assert dpi.logical_to_physical(10.0, 20.0, bad_ratio) == (10.0, 20.0)
    assert dpi.physical_to_logical(10.0, 20.0, bad_ratio) == (10.0, 20.0)


def test_screen_device_pixel_ratio_at_has_a_safe_fallback():
    # Under the offscreen QPA there is exactly one screen and no real
    # multi-monitor arrangement, so this only has to return a sane positive
    # ratio rather than raise. The two engine tests below prove the call
    # sites actually use whatever ratio this returns.
    ratio = dpi.screen_device_pixel_ratio_at(0.0, 0.0)
    assert ratio > 0.0


def test_enable_high_dpi_scaling_sets_the_qt_attributes():
    from PyQt5.QtCore import QCoreApplication, Qt

    dpi.enable_high_dpi_scaling()
    assert QCoreApplication.testAttribute(Qt.AA_EnableHighDpiScaling) is True
    assert QCoreApplication.testAttribute(Qt.AA_UseHighDpiPixmaps) is True


# ----------------------------------------------------------------------
# Every place a QApplication/QGuiApplication gets constructed must enable
# scaling first -- Qt reads the attribute while building the platform
# integration for the *first* one in the process and ignores a later change,
# so this can only be proved by inspecting the source, not by importing and
# calling main() (which would open a real window/event loop). Matches the
# grep-assertion style DC-17's perception-snapshot test already uses for the
# same kind of "wired into the right place" claim.
# ----------------------------------------------------------------------

_ENTRY_POINTS = {
    "src/desktop_bug/app/engine.py": "QApplication.instance() or QApplication(sys.argv[:1])",
    "src/desktop_bug/app/config_ui.py": "QApplication.instance() or QApplication(sys.argv[:1])",
    "tools/benchmark.py": "QGuiApplication.instance() or QGuiApplication(sys.argv[:1])",
}


@pytest.mark.parametrize("relative_path,construct_line", sorted(_ENTRY_POINTS.items()))
def test_high_dpi_scaling_enabled_before_application_constructed(relative_path, construct_line):
    source = (ROOT / relative_path).read_text(encoding="utf-8")
    assert "enable_high_dpi_scaling()" in source, (
        f"{relative_path} constructs an application but never calls enable_high_dpi_scaling()"
    )
    construct_at = source.index(construct_line)
    enable_at = source.rindex("enable_high_dpi_scaling()", 0, construct_at)
    # rindex raises ValueError (failing the test) if the call does not appear
    # anywhere before the construction line at all.
    assert enable_at < construct_at


def test_high_dpi_scaling_enabled_before_the_shared_test_qapp():
    """`tests/conftest.py` builds the one QApplication the whole suite shares."""
    conftest_source = (ROOT / "tests" / "conftest.py").read_text(encoding="utf-8")
    construct_at = conftest_source.index("QApplication(sys.argv[:1])")
    enable_at = conftest_source.rindex("enable_high_dpi_scaling()", 0, construct_at)
    assert enable_at < construct_at


# ----------------------------------------------------------------------
# The two engine.py call sites that cross from Qt logical into raw Win32
# physical pixels (or back). Proved by reverting: see the comment on each.
# ----------------------------------------------------------------------

def test_cursor_trap_converts_logical_target_to_physical_pixels(monkeypatch):
    """SetCursorPos is a raw Win32 call and always wants native pixels.

    Revert by changing the `tick()` cursor-trap block back to
    `set_cursor_pos(origin.x() + desired[0], origin.y() + desired[1])`
    (no DPR conversion) and this fails for every ratio other than 1.0.
    """
    from desktop_bug.app import engine

    window = engine.OverlayWindow(ROOT / "presets" / "default.json")
    try:
        # Freeze the simulation for this tick so a deterministic desired
        # cursor position survives into the code under test instead of
        # whatever the real (un-trapped) mouse-web world computes. DC-16
        # made manager.update()'s return value the source of truth (C9),
        # so the mock returns it directly rather than setting the
        # now-unread _desired_cursor attribute.
        monkeypatch.setattr(window.manager, "update", lambda *a, **k: (100.0, 50.0))

        monkeypatch.setattr(engine, "screen_device_pixel_ratio_at", lambda x, y: 2.0)
        calls = []
        monkeypatch.setattr(engine, "set_cursor_pos", lambda x, y: calls.append((x, y)))

        window.tick()

        origin = window.geometry_rect.topLeft()
        expected_x = int(round((origin.x() + 100.0) * 2.0))
        expected_y = int(round((origin.y() + 50.0) * 2.0))
        assert calls == [(expected_x, expected_y)]
    finally:
        window.timer.stop()
        window.style_timer.stop()


def test_cursor_trap_is_a_no_op_conversion_at_ratio_one(monkeypatch):
    """At the common 100% scale, physical and logical pixels coincide."""
    from desktop_bug.app import engine

    window = engine.OverlayWindow(ROOT / "presets" / "default.json")
    try:
        # See the sibling test above: DC-16 made the return value of
        # manager.update() the source of truth for the desired cursor (C9).
        monkeypatch.setattr(window.manager, "update", lambda *a, **k: (7.0, 9.0))

        monkeypatch.setattr(engine, "screen_device_pixel_ratio_at", lambda x, y: 1.0)
        calls = []
        monkeypatch.setattr(engine, "set_cursor_pos", lambda x, y: calls.append((x, y)))

        window.tick()

        origin = window.geometry_rect.topLeft()
        assert calls == [(int(round(origin.x() + 7.0)), int(round(origin.y() + 9.0)))]
    finally:
        window.timer.stop()
        window.style_timer.stop()


def test_desktop_surface_refresh_converts_between_physical_and_logical(monkeypatch):
    """snapshot_desktop_surfaces reads raw Win32 rectangles (native pixels).

    `_refresh_desktop_surfaces` must hand it a native-pixel origin/size and
    then scale whatever it returns back down into the logical pixels the
    manager and every creature position use. Revert either half of that
    conversion in `_refresh_desktop_surfaces` and this fails for ratio 2.0.
    """
    from desktop_bug.app import engine
    from desktop_bug.world.desktop_environment import DesktopSurface

    window = engine.OverlayWindow(ROOT / "presets" / "default.json")
    try:
        monkeypatch.setattr(engine, "screen_device_pixel_ratio_at", lambda x, y: 2.0)

        captured_kwargs = {}

        def fake_snapshot(**kwargs):
            captured_kwargs.update(kwargs)
            # A native-pixel window rectangle, 20 native px right of and
            # below the (native) origin the call was given.
            return [DesktopSurface(20.0, 20.0, 200.0, 100.0, "Notepad", "Notepad", "window")]

        monkeypatch.setattr(engine, "snapshot_desktop_surfaces", fake_snapshot)

        recorded = {}
        monkeypatch.setattr(
            window.manager, "set_desktop_surfaces", lambda surfaces: recorded.setdefault("surfaces", surfaces)
        )

        window._refresh_desktop_surfaces()

        top_left = window.geometry_rect.topLeft()
        assert captured_kwargs["origin_x"] == int(round(top_left.x() * 2.0))
        assert captured_kwargs["origin_y"] == int(round(top_left.y() * 2.0))
        assert captured_kwargs["screen_w"] == int(round(window.width() * 2.0))
        assert captured_kwargs["screen_h"] == int(round(window.height() * 2.0))

        surfaces = recorded["surfaces"]
        assert len(surfaces) == 1
        surface = surfaces[0]
        # The fake native-pixel rectangle (20, 20, 200x100) scaled back down
        # by the ratio 2.0 the screen lookup was told to report.
        assert surface.x == pytest.approx(10.0)
        assert surface.y == pytest.approx(10.0)
        assert surface.w == pytest.approx(100.0)
        assert surface.h == pytest.approx(50.0)
    finally:
        window.timer.stop()
        window.style_timer.stop()


def test_desktop_surface_refresh_is_a_no_op_conversion_at_ratio_one(monkeypatch):
    from desktop_bug.app import engine
    from desktop_bug.world.desktop_environment import DesktopSurface

    window = engine.OverlayWindow(ROOT / "presets" / "default.json")
    try:
        monkeypatch.setattr(engine, "screen_device_pixel_ratio_at", lambda x, y: 1.0)

        def fake_snapshot(**kwargs):
            return [DesktopSurface(20.0, 20.0, 200.0, 100.0, "Notepad", "Notepad", "window")]

        monkeypatch.setattr(engine, "snapshot_desktop_surfaces", fake_snapshot)

        recorded = {}
        monkeypatch.setattr(
            window.manager, "set_desktop_surfaces", lambda surfaces: recorded.setdefault("surfaces", surfaces)
        )

        window._refresh_desktop_surfaces()

        surface = recorded["surfaces"][0]
        assert (surface.x, surface.y, surface.w, surface.h) == (20.0, 20.0, 200.0, 100.0)
    finally:
        window.timer.stop()
        window.style_timer.stop()
