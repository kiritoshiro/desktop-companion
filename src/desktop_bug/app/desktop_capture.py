"""A frozen picture of the desktop for "Reclaim the desktop".

The owner: *"will need to screen shot the desktop and freeze the controls on
the desktop for it ... by leaving the raid it would retake the control of the
screen."*

Nothing here touches a real window. It takes, once, before the overlay shows:

- a screenshot of every monitor, at its native resolution;
- where the open windows are, front to back, so acid can tell a window from
  the desktop -- and a picture of each (Windows' PrintWindow), so when the
  glass of the front one breaks the one behind it can show (the owner:
  "there could be another folder open beneath it therefore the desktop is
  not seen until it is broken too"). No titles are read;
- the wallpaper, to show through a hole melted in a window.

The snapshot lives in memory only. It is never written to disk or sent
anywhere: whatever was on screen -- messages, mail -- stays on this machine.
"""
from __future__ import annotations

import os
import sys
from dataclasses import dataclass, field

from PyQt5.QtCore import QPoint, QRectF, QSize, Qt
from PyQt5.QtGui import QColor, QGuiApplication, QImage, QLinearGradient, QPainter

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
    # Each window's own picture, in step with ``windows``; None where it could
    # not be taken (then it is drawn as a plain window).
    window_images: list = field(default_factory=list)


def capture_desktop(origin: QPoint, screens=None) -> DesktopSnapshot | None:
    """Every monitor in use as it is now, in overlay-local coordinates. None
    when Qt reports no screens (nothing to freeze)."""
    screens = list(screens) if screens is not None else QGuiApplication.screens()
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
    found = _windows(origin, screens, pictures=True)
    return DesktopSnapshot(shots, [rect for rect, _ in found], index, [image for _, image in found])


def _fake_word(p, x, y, letters, seed):
    """Letter-like strokes a text line tall, drawn without a font: a test
    machine may have no fonts at all, and the word finder only needs shape."""
    for i in range(letters):
        lx = x + i * 7
        tall = 10 if (seed + i) % 3 == 0 else 7
        p.fillRect(QRectF(lx, y + 10 - tall, 1.4, tall), QColor(30, 30, 30))
        p.fillRect(QRectF(lx + 3.6, y + 3, 1.4, 7), QColor(30, 30, 30))
        p.fillRect(QRectF(lx, y + 3 + (seed + i) % 2 * 6, 5, 1.2), QColor(30, 30, 30))
    return letters * 7


def synthetic_snapshot(rects: list[ScreenRect], windows: list[QRectF] = (), primary: int = 0,
                       text_rows: int = 0) -> DesktopSnapshot:
    """A made-up desktop for tests and previews: a blue desktop, pale windows,
    and optionally rows of word-like marks on them."""
    shots = []
    for rect in rects:
        image = QImage(int(rect.w), int(rect.h), QImage.Format_ARGB32_Premultiplied)
        image.fill(QColor(38, 92, 150))
        p = QPainter(image)
        for win in windows:
            local = win.translated(-rect.x, -rect.y)
            p.fillRect(local, QColor(236, 236, 232))
            p.fillRect(QRectF(local.x(), local.y(), local.width(), 28), QColor(52, 56, 64))
            for row in range(text_rows):
                y = local.y() + 50 + row * 26
                if y + 16 > local.bottom():
                    break
                x = local.x() + 20
                word = 0
                while True:
                    letters = 3 + (row * 7 + word * 5) % 6
                    if x + letters * 7 > local.right() - 20:
                        break
                    x += _fake_word(p, x, y, letters, row + word) + 8
                    word += 1
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
    return [rect for rect, _ in _windows(origin, screens, pictures=False)]


def _windows(origin, screens, pictures: bool) -> list:
    """(rect, picture or None) for each window, front to back."""
    if not sys.platform.startswith("win"):
        return []
    try:
        return _win32_windows(origin, screens, pictures)
    except Exception:
        return []


# Pictures are taken of at most this many windows, front first.
MAX_PICTURES = 12


def _win32_windows(origin, screens, pictures: bool = False) -> list:
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
        picture = None
        if pictures and len(found) < MAX_PICTURES and not user32.IsHungAppWindow(hwnd):
            try:
                picture = _print_window(hwnd, rect)
            except Exception:
                picture = None
        found.append((QRectF(x0, y0, x1 - x0, y1 - y0), picture))
        return True

    user32.EnumWindows(visit, 0)
    return found


def _print_window(hwnd, frame) -> QImage | None:
    """A window's own picture, even where other windows cover it, cropped to
    its visible frame (``frame``, physical pixels). None if Windows gives back
    nothing (some windows draw only to the screen)."""
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    gdi32 = ctypes.windll.gdi32
    whole = wintypes.RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(whole)):
        return None
    w, h = whole.right - whole.left, whole.bottom - whole.top
    if w <= 0 or h <= 0 or w * h > 40_000_000:
        return None

    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
                    ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD),
                    ("biCompression", wintypes.DWORD), ("biSizeImage", wintypes.DWORD),
                    ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
                    ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD)]

    screen_dc = user32.GetDC(None)
    memory_dc = gdi32.CreateCompatibleDC(screen_dc)
    bitmap = gdi32.CreateCompatibleBitmap(screen_dc, w, h)
    old = gdi32.SelectObject(memory_dc, bitmap)
    try:
        PW_RENDERFULLCONTENT = 0x2
        if not user32.PrintWindow(hwnd, memory_dc, PW_RENDERFULLCONTENT):
            return None
        header = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
        buffer = ctypes.create_string_buffer(w * h * 4)
        if not gdi32.GetDIBits(memory_dc, bitmap, 0, h, buffer, ctypes.byref(header), 0):
            return None
    finally:
        gdi32.SelectObject(memory_dc, old)
        gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(memory_dc)
        user32.ReleaseDC(None, screen_dc)
    image = QImage(buffer.raw, w, h, w * 4, QImage.Format_RGB32).copy()
    left, top = frame.left - whole.left, frame.top - whole.top
    image = image.copy(max(0, left), max(0, top), frame.right - frame.left, frame.bottom - frame.top)
    return None if _blank(image) else image


def _blank(image: QImage) -> bool:
    """True when every sampled pixel is the same colour: nothing was drawn."""
    if image.isNull() or image.width() < 2 or image.height() < 2:
        return True
    first = image.pixel(0, 0)
    for j in range(1, 12):
        for i in range(1, 12):
            if image.pixel(image.width() * i // 12, image.height() * j // 12) != first:
                return False
    return True
