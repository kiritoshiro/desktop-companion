"""The overlay can paint onto an OpenGL surface, opt-in (DC-74).

Asked to take the renderer to the GPU, and the honest answer turned out to be
"it works, and it is worth 9%".

**Worth 9%.** Twenty tarantulas at 1600x1000, the same QPainter calls onto a
software QImage and onto a GPU framebuffer, interleaved offscreen in one
process: 8.7%, 8.6% and 10.1% over three runs, and 8.7% with both sides
clipped to the dirty region the overlay actually repaints. Not the
transformation it sounds like, because **Qt's OpenGL paint engine still turns
every stroked path into triangles on the CPU** -- the GPU takes the fill and
the antialiasing, which DC-71 measured at roughly 27% and 8% of render, and
leaves the per-primitive geometry alone.

Before DC-73 the same comparison gave 20-24%. DC-73 removed the fill-heavy
primitives, so most of that win had already been taken by cheaper means. The
GPU and the detail levels were largely after the same money.

**It works.** The risk worth checking first was that `apply_click_through`
sets WS_EX_LAYERED, and a layered window is composited by the DWM from a
CPU-side bitmap, which is historically the one thing an OpenGL swap chain
will not show through. Probed on this machine with a control: a plain
translucent topmost widget and a QOpenGLWidget with the same flags both
composited, both let the desktop through. Then the real `OverlayWindow` was
run on the GL path and a spider was grabbed off the actual screen, over the
desktop, matching the software path.

A mistake worth keeping from that: the first version of the on-screen check
added no window offset, grabbed a completely different window on a
multi-monitor desktop, found plenty of non-background pixels and reported
success. Creature coordinates are overlay-local and this overlay's top-left
is at (0, -718).

**Off by default**, because 9% does not justify making every launch depend on
a working GL driver, and because what it changes is the whole window -- its
behaviour on other machines, other drivers and a packaged build is not
something one machine settles.

The tests below cannot render GL under a headless run, so they pin the switch
and the surface format, and check the class actually changes shape. The
timings and the on-screen proof are recorded above rather than asserted; a
stopwatch here would be flaky.
"""

from __future__ import annotations

import subprocess
import sys

import pytest
from PyQt5.QtGui import QSurfaceFormat
from PyQt5.QtWidgets import QWidget

from desktop_bug.app import engine
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Importing engine touches Qt types."""


# ------------------------------------------------------------------ the switch

def test_it_is_off_unless_asked_for(monkeypatch):
    monkeypatch.delenv("DESKTOP_BUG_GL", raising=False)
    assert engine.gl_overlay_enabled() is False


@pytest.mark.parametrize("value", ["1", "true", "TRUE", "yes", "on", " 1 "])
def test_the_ways_of_saying_yes(monkeypatch, value):
    monkeypatch.setenv("DESKTOP_BUG_GL", value)
    assert engine.gl_overlay_enabled() is True


@pytest.mark.parametrize("value", ["", "0", "no", "off", "false", "maybe"])
def test_everything_else_is_no(monkeypatch, value):
    """A typo must not silently move every launch onto a different surface."""
    monkeypatch.setenv("DESKTOP_BUG_GL", value)
    assert engine.gl_overlay_enabled() is False


def test_the_shipped_default_is_the_software_path():
    """The module-level decision, not just the helper. This is what a launch
    with no environment actually gets."""
    assert engine.GL_OVERLAY is False
    assert engine._OverlayBase is QWidget


# ---------------------------------------------------------- the surface format

def test_no_surface_format_is_touched_when_it_is_off(monkeypatch):
    monkeypatch.delenv("DESKTOP_BUG_GL", raising=False)
    before = QSurfaceFormat.defaultFormat()
    assert engine.configure_gl_surface() is False
    assert QSurfaceFormat.defaultFormat().alphaBufferSize() == before.alphaBufferSize()


def test_it_asks_for_an_alpha_channel_when_it_is_on(monkeypatch):
    """A QOpenGLWidget cannot be translucent without one, and an overlay that
    is not translucent is an opaque rectangle over the desktop."""
    monkeypatch.setenv("DESKTOP_BUG_GL", "1")
    restore = QSurfaceFormat.defaultFormat()
    try:
        assert engine.configure_gl_surface() is True
        fmt = QSurfaceFormat.defaultFormat()
        assert fmt.alphaBufferSize() >= 8
        assert fmt.samples() >= 4
    finally:
        QSurfaceFormat.setDefaultFormat(restore)


# ------------------------------------------------- the class really changes shape

_PROBE = """
import sys
sys.path.insert(0, {src!r})
from PyQt5.QtWidgets import QOpenGLWidget, QWidget
from desktop_bug.app import engine
own = engine.OverlayWindow.__dict__
print(engine.GL_OVERLAY,
      engine._OverlayBase is QOpenGLWidget,
      engine._OverlayBase is QWidget,
      "paintGL" in own,
      "paintEvent" in own,
      sep="|")
"""


def _probe(gl: bool) -> list:
    import os
    env = dict(os.environ)
    if gl:
        env["DESKTOP_BUG_GL"] = "1"
    else:
        env.pop("DESKTOP_BUG_GL", None)
    out = subprocess.run(
        [sys.executable, "-c", _PROBE.format(src=str(ROOT / "src"))],
        capture_output=True, text=True, env=env, timeout=120,
    )
    assert out.returncode == 0, out.stderr
    return out.stdout.strip().splitlines()[-1].split("|")


def test_asking_for_gl_gives_a_qopenglwidget_that_paints_through_paintgl():
    """A fresh interpreter, because the base class is chosen at import. Qt
    drives a QOpenGLWidget through paintGL and a QWidget through paintEvent,
    so defining the wrong one would leave the overlay blank -- which no
    in-process test of the helper would notice."""
    flag, is_gl, is_widget, has_paint_gl, has_paint_event = _probe(True)
    assert flag == "True"
    assert is_gl == "True" and is_widget == "False"
    assert has_paint_gl == "True", "QOpenGLWidget needs paintGL"
    assert has_paint_event == "False", (
        "overriding paintEvent on a QOpenGLWidget stops paintGL being called"
    )


def test_without_it_the_overlay_is_a_plain_widget_painting_through_paintevent():
    flag, is_gl, is_widget, has_paint_gl, has_paint_event = _probe(False)
    assert flag == "False"
    assert is_gl == "False" and is_widget == "True"
    assert has_paint_gl == "False"
    assert has_paint_event == "True"


# ---------------------------------------------------------------- the painting

def test_the_paint_helper_takes_a_region_not_an_event():
    """Both paths share `_paint`, and only one of them has an event to take a
    region from: paintGL has no exposed region at all."""
    import inspect
    params = list(inspect.signature(engine.OverlayWindow._paint).parameters)
    assert params == ["self", "region"], params
