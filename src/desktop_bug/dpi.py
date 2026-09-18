"""High-DPI / multi-monitor coordinate correctness helpers (DC-14).

Two different coordinate spaces meet at the edges of this Windows overlay:

* Qt's own coordinate system -- widget geometry, ``QCursor.pos()``,
  ``QScreen.geometry()`` -- which, once ``Qt.AA_EnableHighDpiScaling`` is
  set, reports **device-independent pixels** ("logical" pixels, one unit per
  96 DPI, scaled per screen).
* Raw Win32 calls made directly through ``ctypes`` (``SetCursorPos``,
  ``GetWindowRect``, ...) which always operate in **native pixels**
  ("physical" pixels): the real pixel grid of the monitor.

On a 100% (device pixel ratio 1.0) monitor the two coincide and mixing them
is invisible. On a scaled monitor they differ by that screen's device pixel
ratio, and code that silently mixes the two spaces places the cursor in the
wrong spot or hit-tests the wrong pixel. This module holds the one-time
enable step plus the small pure conversion helpers so every place the
overlay crosses between the two spaces does it the same way.

Before this package, no ``AA_EnableHighDpiScaling``/``AA_UseHighDpiPixmaps``
attribute was set anywhere, so Qt reported native pixels throughout and,
because every raw Win32 call in this codebase also deals in native pixels,
the two happened to already agree (at the cost of Qt never rescaling its own
drawing for a monitor that runs above or below the system's reference DPI,
which is D2's blurry-scaling half of the finding). Turning scaling on fixes
that, but it also means every point that used to silently pass a Qt
coordinate into a raw Win32 call (or the reverse) now needs the explicit
conversion below.
"""
from __future__ import annotations


def enable_high_dpi_scaling() -> None:
    """Turn on Qt's automatic per-monitor DPI scaling.

    Must run before any ``QApplication``/``QGuiApplication`` is constructed:
    Qt reads ``AA_EnableHighDpiScaling`` while it builds the platform
    integration and ignores a change made afterwards. Safe to call more than
    once, and safe to call in a process that already has an application
    instance (Qt no-ops the redundant `setAttribute` rather than raising) --
    the value only actually matters for whichever call happened first.
    """
    from PyQt5.QtCore import QCoreApplication, Qt

    QCoreApplication.setAttribute(Qt.AA_EnableHighDpiScaling, True)
    # Rasterizes pixmaps (icons, the settings-window preview) at the same
    # per-monitor ratio instead of leaving them native-sized and blurry when
    # Qt has to up-scale them for a high-DPI screen.
    QCoreApplication.setAttribute(Qt.AA_UseHighDpiPixmaps, True)


def logical_to_physical(x: float, y: float, dpr: float) -> tuple[float, float]:
    """Convert a Qt logical (device-independent) point to native pixels.

    ``dpr`` is the target screen's device pixel ratio
    (``QScreen.devicePixelRatio()``). A non-positive ratio is treated as 1.0
    so a bad lookup degrades to a no-op conversion instead of dividing by
    zero or flipping a sign.
    """
    ratio = dpr if dpr and dpr > 0 else 1.0
    return (x * ratio, y * ratio)


def physical_to_logical(x: float, y: float, dpr: float) -> tuple[float, float]:
    """Convert a native-pixel point to Qt logical (device-independent) coordinates.

    Inverse of :func:`logical_to_physical`; see its docstring for ``dpr``.
    """
    ratio = dpr if dpr and dpr > 0 else 1.0
    return (x / ratio, y / ratio)


def screen_device_pixel_ratio_at(x: float, y: float) -> float:
    """Best-effort device pixel ratio of the screen under a Qt logical point.

    Falls back to the first available screen, then to 1.0 (no scaling) when
    Qt reports no screens at all -- true of a fresh offscreen QPA before any
    window has been shown, and in unit tests that call this directly without
    a real window on screen.
    """
    try:
        from PyQt5.QtCore import QPoint
        from PyQt5.QtGui import QGuiApplication

        screen = QGuiApplication.screenAt(QPoint(int(round(x)), int(round(y))))
        if screen is not None:
            return float(screen.devicePixelRatio())
        screens = QGuiApplication.screens()
        if screens:
            return float(screens[0].devicePixelRatio())
    except Exception:
        pass
    return 1.0
