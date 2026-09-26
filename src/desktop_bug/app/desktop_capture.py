"""A frozen picture of the desktop for "Reclaim the desktop".

The owner: *"will need to screen shot the desktop and freeze the controls on
the desktop for it ... by leaving the raid it would retake the control of the
screen."*

Nothing here touches a real window. It takes, once, before the overlay shows:

- a screenshot of every monitor, at its native resolution;
- where the open windows are (rectangles only; no titles, no contents
  beyond the screenshot itself), so acid can tell a window from the desktop;
- the wallpaper, to show through a hole melted in a window.

The snapshot lives in memory only. It is never written to disk or sent
anywhere: whatever was on screen -- messages, mail -- stays on this machine.
"""
from __future__ import annotations

import os
import sys
from dataclasses import dataclass, field

from PyQt5.QtCore import QPoint, QRectF, QSize, Qt
from PyQt5.QtGui import QColor, QFont, QGuiApplication, QImage, QLinearGradient, QPainter

from ..world.playfield import ScreenRect


@dataclass
class ScreenShot:
    rect: ScreenRect                 # overlay-local, logical pixels
    image: QImage                    # native pixels, devicePixelRatio set
    wallpaper: QImage | None = None  # the same size, or None


@dataclass
class DesktopSnapshot:
    screens: list[ScreenShot]
    windows: list[QRectF] = field(default_factory=list)   # overlay-local, logical
    primary: int = 0


def capture_desktop(origin: QPoint) -> DesktopSnapshot | None:
    """Every monitor as it is now, in overlay-local coordinates. None when Qt
    reports no screens (nothing to freeze)."""
    screens = QGuiApplication.screens()
    if not screens:
        return None
    primary = QGuiApplication.primaryScreen()
    wallpaper = _wallpaper_image()
    shots = []
    for screen in screens:
        g = screen.geometry()
        pixmap = screen.grabWindow(0)
        image = pixmap.toImage().convertToFormat(QImage.Format_ARGB32_Premultiplied)
        ratio = image.width() / max(1, g.width())
        image.setDevicePixelRatio(ratio if ratio > 0 else 1.0)
        rect = ScreenRect(float(g.x() - origin.x()), float(g.y() - origin.y()), float(g.width()), float(g.height()))
        shots.append(ScreenShot(rect, image, _fill(wallpaper, image.size(), image.devicePixelRatio())))
    index = screens.index(primary) if primary in screens else 0
    return DesktopSnapshot(shots, list_windows(origin, screens), index)


_WORDS = ("silk", "spider", "desktop", "reclaim", "window", "tarantula", "amber", "web", "nest",
          "hunter", "the", "of", "burrow", "venom", "glass", "crawl", "weaver", "scout")


def synthetic_snapshot(rects: list[ScreenRect], windows: list[QRectF] = (), primary: int = 0,
                       text_rows: int = 0) -> DesktopSnapshot:
    """A made-up desktop for tests and previews: a blue desktop, pale windows,
    and optionally rows of real text on them."""
    shots = []
    for rect in rects:
        image = QImage(int(rect.w), int(rect.h), QImage.Format_ARGB32_Premultiplied)
        image.fill(QColor(38, 92, 150))
        p = QPainter(image)
        for win in windows:
            local = win.translated(-rect.x, -rect.y)
            p.fillRect(local, QColor(236, 236, 232))
            p.fillRect(QRectF(local.x(), local.y(), local.width(), 28), QColor(52, 56, 64))
            p.setPen(QColor(30, 30, 30))
            p.setFont(QFont("Segoe UI", 10))
            for row in range(text_rows):
                y = local.y() + 50 + row * 26
                if y + 16 > local.bottom():
                    break
                line = " ".join(_WORDS[(row * 5 + i) % len(_WORDS)] for i in range(14))
                p.drawText(QRectF(local.x() + 20, y, local.width() - 40, 20),
                           Qt.AlignLeft | Qt.AlignVCenter, line)
        p.end()
        paper = QImage(image.size(), QImage.Format_ARGB32_Premultiplied)
        gradient = QLinearGradient(0, 0, 0, image.height())
        gradient.setColorAt(0, QColor(40, 110, 90))
        gradient.setColorAt(1, QColor(20, 50, 70))
        wp = QPainter(paper)
        wp.fillRect(paper.rect(), gradient)
        wp.end()
        shots.append(ScreenShot(rect, image, paper))
    return DesktopSnapshot(shots, [QRectF(w) for w in windows], primary)


# -- the wallpaper ---------------------------------------------------------

def _wallpaper_image() -> QImage | None:
    if not sys.platform.startswith("win"):
        return None
    try:
        import ctypes

        buffer = ctypes.create_unicode_buffer(520)
        SPI_GETDESKWALLPAPER = 0x0073
        if not ctypes.windll.user32.SystemParametersInfoW(SPI_GETDESKWALLPAPER, len(buffer), buffer, 0):
            return None
        path = buffer.value
    except Exception:
        return None
    if not path or not os.path.exists(path):
        return None
    image = QImage(path)
    return None if image.isNull() else image


def _fill(wallpaper: QImage | None, size: QSize, ratio: float) -> QImage | None:
    """The wallpaper scaled to cover ``size`` and cropped to it, as Windows' Fill."""
    if wallpaper is None:
        return None
    scaled = wallpaper.scaled(size, Qt.KeepAspectRatioByExpanding, Qt.SmoothTransformation)
    x = max(0, (scaled.width() - size.width()) // 2)
    y = max(0, (scaled.height() - size.height()) // 2)
    image = scaled.copy(x, y, size.width(), size.height()).convertToFormat(QImage.Format_ARGB32_Premultiplied)
    image.setDevicePixelRatio(ratio)
    return image


# -- the windows -----------------------------------------------------------

def list_windows(origin: QPoint, screens) -> list[QRectF]:
    """Rectangles of the visible top-level windows, front to back, overlay-local.

    Windows only; elsewhere the whole screen counts as desktop. Minimised,
    cloaked (hidden UWP) and tool windows are skipped, as are this process's
    own windows and the desktop itself.
    """
    if not sys.platform.startswith("win"):
        return []
    try:
        return _win32_windows(origin, screens)
    except Exception:
        return []


def _win32_windows(origin, screens) -> list[QRectF]:
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    dwmapi = ctypes.windll.dwmapi
    GWL_EXSTYLE = -20
    WS_EX_TOOLWINDOW = 0x00000080
    DWMWA_EXTENDED_FRAME_BOUNDS = 9
    DWMWA_CLOAKED = 14
    own = os.getpid()
    found = []

    def physical_to_local(px, py):
        for screen in screens:
            g = screen.geometry()
            dpr = screen.devicePixelRatio() or 1.0
            if g.x() <= px < g.x() + g.width() * dpr and g.y() <= py < g.y() + g.height() * dpr:
                return (g.x() + (px - g.x()) / dpr - origin.x(), g.y() + (py - g.y()) / dpr - origin.y())
        return px - origin.x(), py - origin.y()

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _lparam):
        if not user32.IsWindowVisible(hwnd) or user32.IsIconic(hwnd):
            return True
        if user32.GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW:
            return True
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == own:
            return True
        name = ctypes.create_unicode_buffer(64)
        user32.GetClassNameW(hwnd, name, 64)
        if name.value in ("Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"):
            return True
        cloaked = wintypes.DWORD()
        if (dwmapi.DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, ctypes.byref(cloaked), ctypes.sizeof(cloaked)) == 0
                and cloaked.value):
            return True
        rect = wintypes.RECT()
        if dwmapi.DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, ctypes.byref(rect),
                                        ctypes.sizeof(rect)) != 0:
            if not user32.GetWindowRect(hwnd, ctypes.byref(rect)):
                return True
        if rect.right - rect.left < 40 or rect.bottom - rect.top < 30:
            return True
        x0, y0 = physical_to_local(rect.left, rect.top)
        x1, y1 = physical_to_local(rect.right, rect.bottom)
        found.append(QRectF(x0, y0, x1 - x0, y1 - y0))
        return True

    user32.EnumWindows(visit, 0)
    return found
