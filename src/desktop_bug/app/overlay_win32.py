"""Windows-only helpers for desktop overlay behavior.

Qt handles per-pixel transparency through WA_TranslucentBackground.  Windows
still often needs extended styles applied after show() to keep the overlay
layered/topmost.  Mouse pass-through is handled by OverlayWindow.nativeEvent so
spider pixels can still be draggable.
"""
import sys

from ..support.logging_setup import get_logger

log = get_logger("overlay")


def is_windows() -> bool:
    return sys.platform.startswith("win")


def apply_click_through(widget) -> bool:
    """Apply layered/topmost styles to a QWidget on Windows.

    The historical function name is kept for compatibility.  We deliberately do
    not set WS_EX_TRANSPARENT anymore; OverlayWindow.nativeEvent returns
    HTTRANSPARENT only when the cursor is not over a spider, which preserves
    desktop click-through while allowing direct dragging on spider pixels.
    """
    if not is_windows():
        return False
    try:
        import ctypes
        from ctypes import wintypes

        hwnd = int(widget.winId())
        if not hwnd:
            return False

        user32 = ctypes.windll.user32
        GWL_EXSTYLE = -20
        WS_EX_LAYERED = 0x00080000
        WS_EX_TRANSPARENT = 0x00000020
        WS_EX_TOOLWINDOW = 0x00000080

        if ctypes.sizeof(ctypes.c_void_p) == 8:
            get_window_long = user32.GetWindowLongPtrW
            set_window_long = user32.SetWindowLongPtrW
        else:
            get_window_long = user32.GetWindowLongW
            set_window_long = user32.SetWindowLongW

        get_window_long.argtypes = [wintypes.HWND, ctypes.c_int]
        get_window_long.restype = ctypes.c_longlong if ctypes.sizeof(ctypes.c_void_p) == 8 else ctypes.c_long
        set_window_long.argtypes = [wintypes.HWND, ctypes.c_int, get_window_long.restype]
        set_window_long.restype = get_window_long.restype

        style = get_window_long(hwnd, GWL_EXSTYLE)
        style |= WS_EX_LAYERED | WS_EX_TOOLWINDOW
        style &= ~WS_EX_TRANSPARENT
        set_window_long(hwnd, GWL_EXSTYLE, style)

        # Keep the window available above normal apps without activating it.
        HWND_TOPMOST = -1
        SWP_NOMOVE = 0x0002
        SWP_NOSIZE = 0x0001
        SWP_NOACTIVATE = 0x0010
        SWP_SHOWWINDOW = 0x0040
        user32.SetWindowPos(
            wintypes.HWND(hwnd),
            wintypes.HWND(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        return True
    except Exception:  # pragma: no cover - platform-specific safety
        log.warning("Failed to apply Windows overlay styles", exc_info=True)
        return False


def set_cursor_pos(x: int, y: int) -> bool:
    """Move the OS pointer to global screen pixel ``(x, y)``.

    Used by the cursor-trapping silk so a spider can pin or shove the pointer.
    Returns True on success.  On non-Windows platforms, or if the call fails,
    returns False; the trap logic treats an unmovable pointer as "the user
    escaped", so a failure degrades safely rather than locking the cursor.
    """
    if not is_windows():
        return False
    try:
        import ctypes

        return bool(ctypes.windll.user32.SetCursorPos(int(x), int(y)))
    except Exception:
        log.debug("SetCursorPos failed", exc_info=True)
        return False
