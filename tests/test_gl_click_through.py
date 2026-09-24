"""The OpenGL overlay lets the desktop keep its mouse (DC-79).

The owner, the first time DC-78's GPU default ran: *"the overlay takes over
the control of whole screen nothing works after it."*

A layered window drawn through Qt's software backing store is hit-tested per
pixel by Windows, so its transparent pixels pass input to other applications
without the app doing anything. An OpenGL window is not, and the
HTTRANSPARENT answer in `OverlayWindow.nativeEvent` only forwards to windows
of the same thread. Measured with a real mouse over a window underneath the
overlay: the software overlay passed 245 moves through, the GL overlay 0.

The fix sets WS_EX_TRANSPARENT on the GL overlay whenever the cursor is not
over something interactive, from the same `wants_mouse` question the hit-test
asks, on its own 20 Hz timer (not the frame, which can drop to 1 FPS), only on
change, and starting transparent the moment the window is shown. Measured the same way afterwards: 184
moves through on empty space; hovering a spider, 0 leaked through on the run
that was checked with diagnostics. One earlier hover run leaked, and was not
re-run because the owner asked for the real-mouse tests to stop. That
residual is recorded, not claimed fixed.

Nothing here moves the mouse.
"""

from __future__ import annotations

import subprocess
import sys

import pytest
from PyQt5.QtCore import QPoint, QRect

from desktop_bug.app import engine
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Importing engine touches Qt types."""


class _Manager:
    def __init__(self):
        self.wanted = set()

    def wants_mouse(self, mx, my):
        return (int(mx), int(my)) in self.wanted


class _Overlay:
    """Just enough of OverlayWindow for the per-frame decision."""

    _update_input_transparency = engine.OverlayWindow._update_input_transparency

    def __init__(self):
        self.geometry_rect = QRect(100, -50, 1600, 900)
        self.manager = _Manager()


@pytest.fixture
def rig(monkeypatch):
    writes = []
    cursor = {"pos": QPoint(0, 0)}
    monkeypatch.setattr(engine, "GL_OVERLAY", True)
    monkeypatch.setattr(engine.QCursor, "pos", staticmethod(lambda: cursor["pos"]))

    def record(widget, transparent):
        widget._input_transparent = transparent
        writes.append(transparent)
        return True

    monkeypatch.setattr(engine, "set_input_transparent", record)
    return _Overlay(), cursor, writes


def test_empty_space_is_transparent_to_input(rig):
    overlay, cursor, writes = rig
    cursor["pos"] = QPoint(600, 300)
    overlay._update_input_transparency()
    assert writes == [True]


def test_a_spider_under_the_cursor_takes_the_mouse(rig):
    overlay, cursor, writes = rig
    # overlay-local (500, 350) is global (600, 300): the question is asked in
    # overlay coordinates, which is where creatures live
    overlay.manager.wanted.add((500, 350))
    cursor["pos"] = QPoint(600, 300)
    overlay._update_input_transparency()
    assert writes == [False]


def test_the_style_is_written_only_when_the_answer_changes(rig):
    overlay, cursor, writes = rig
    cursor["pos"] = QPoint(600, 300)
    for _ in range(5):
        overlay._update_input_transparency()
    overlay.manager.wanted.add((500, 350))
    for _ in range(5):
        overlay._update_input_transparency()
    assert writes == [True, False]


def test_the_software_overlay_is_left_alone(rig, monkeypatch):
    overlay, cursor, writes = rig
    monkeypatch.setattr(engine, "GL_OVERLAY", False)
    overlay._update_input_transparency()
    assert writes == []


def test_the_question_is_asked_on_its_own_timer_not_the_frame():
    """The frame rate can drop to 1 FPS. Asked per frame, a cursor last seen
    over a spider would hold the whole screen for up to a second."""
    import inspect
    init = inspect.getsource(engine.OverlayWindow.__init__)
    assert "self.input_timer.timeout.connect(self._update_input_transparency)" in init
    assert "self.input_timer.start(INPUT_CHECK_MS)" in init
    assert engine.INPUT_CHECK_MS <= 50
    assert "_update_input_transparency" not in inspect.getsource(engine.OverlayWindow.tick)


def test_the_gl_overlay_starts_out_letting_the_mouse_through():
    import inspect
    show = inspect.getsource(engine.OverlayWindow.showEvent)
    assert "set_input_transparent(self, True)" in show


# ------------------------------------------- the real window style, no mouse

_PROBE = """
import sys
sys.path.insert(0, {src!r})
from PyQt5.QtWidgets import QApplication, QWidget
from PyQt5.QtCore import Qt
import ctypes
from ctypes import wintypes
from desktop_bug.app.overlay_win32 import apply_click_through, set_input_transparent
app = QApplication([])
w = QWidget(None, Qt.FramelessWindowHint | Qt.Tool)
w.setGeometry(-2000, -2000, 50, 50)   # off every screen
w.show()
u = ctypes.windll.user32
u.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
u.GetWindowLongPtrW.restype = ctypes.c_longlong
bit = lambda: bool(u.GetWindowLongPtrW(int(w.winId()), -20) & 0x20)
out = []
apply_click_through(w); out.append(bit())            # default: off
set_input_transparent(w, True); out.append(bit())    # on
apply_click_through(w); out.append(bit())            # periodic refresh keeps it
set_input_transparent(w, False); out.append(bit())   # off
apply_click_through(w); out.append(bit())            # and stays off
print("|".join(str(v) for v in out))
"""


@pytest.mark.skipif(not sys.platform.startswith("win"), reason="Windows window styles")
def test_windows_really_stores_the_flag_and_the_refresh_keeps_it():
    """On the real Windows platform, not the offscreen one the suite uses:
    the periodic `apply_click_through` used to clear WS_EX_TRANSPARENT
    unconditionally, which would undo this once a second."""
    import os
    env = dict(os.environ)
    env.pop("QT_QPA_PLATFORM", None)
    out = subprocess.run(
        [sys.executable, "-c", _PROBE.format(src=str(ROOT / "src"))],
        capture_output=True, text=True, env=env, timeout=120,
    )
    assert out.returncode == 0, out.stderr
    assert out.stdout.strip().splitlines()[-1] == "False|True|True|False|False"
