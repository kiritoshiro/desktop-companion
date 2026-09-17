"""Decide what frame rate the overlay deserves right now.

A desktop pet that keeps painting at 60 FPS behind a fullscreen game, or on a
laptop running off its battery, is being a bad guest. The decision is a pure
function of two facts about the machine so it can be tested without Windows;
the probes that establish those facts are separate, cheap, and return a safe
answer on anything that is not Windows or when a call fails.
"""

from __future__ import annotations

import ctypes
import sys
from typing import Optional

# A fullscreen window has the user's whole attention, so the overlay is not even
# visible; drop to a rate that keeps the simulation sane and costs almost
# nothing. Battery is a softer case: still watchable, just cheaper.
FULLSCREEN_FPS = 10.0
BATTERY_FPS = 30.0
# Probing the foreground window and the power source every frame would defeat
# the point. Twice a second reacts faster than a person can notice.
POLL_MS = 500
# A window counts as fullscreen only if it covers essentially the whole screen.
# Some maximised windows sit a pixel or two outside the work area, so allow a
# small slack rather than demanding an exact match.
FULLSCREEN_SLACK_PX = 2


def decide_fps(target_fps: float, fullscreen: bool = False,
               on_battery: bool = False) -> float:
    """Return the frame rate to run at, never above what was asked for.

    Fullscreen wins over battery because it is the stronger signal: nothing the
    overlay draws can be seen at all.
    """
    target = max(1.0, float(target_fps))
    if fullscreen:
        return min(target, FULLSCREEN_FPS)
    if on_battery:
        return min(target, BATTERY_FPS)
    return target


class _Rect(ctypes.Structure):
    _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                ("right", ctypes.c_long), ("bottom", ctypes.c_long)]


class _MonitorInfo(ctypes.Structure):
    _fields_ = [("cbSize", ctypes.c_ulong), ("rcMonitor", _Rect),
                ("rcWork", _Rect), ("dwFlags", ctypes.c_ulong)]


class _PowerStatus(ctypes.Structure):
    _fields_ = [("ACLineStatus", ctypes.c_ubyte),
                ("BatteryFlag", ctypes.c_ubyte),
                ("BatteryLifePercent", ctypes.c_ubyte),
                ("SystemStatusFlag", ctypes.c_ubyte),
                ("BatteryLifeTime", ctypes.c_ulong),
                ("BatteryFullLifeTime", ctypes.c_ulong)]


# Class names that are the desktop itself rather than an application. The shell
# owns the whole screen permanently, so without this every idle desktop would
# look like a fullscreen app.
DESKTOP_CLASSES = frozenset({"progman", "workerw", "shell_traywnd", "windows.ui.core.corewindow"})


def foreground_window_is_fullscreen(exclude_hwnd: Optional[int] = None) -> bool:
    """True when the focused window covers a whole monitor.

    Returns False rather than raising anywhere it cannot tell, including on a
    platform without these calls, because guessing "fullscreen" would silently
    slow the overlay down for no reason.
    """
    if not sys.platform.startswith("win"):
        return False
    try:
        user32 = ctypes.windll.user32
        hwnd = int(user32.GetForegroundWindow() or 0)
        if not hwnd or (exclude_hwnd is not None and hwnd == int(exclude_hwnd)):
            return False

        buf = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(ctypes.c_void_p(hwnd), buf, len(buf))
        if buf.value.strip().casefold() in DESKTOP_CLASSES:
            return False

        rect = _Rect()
        if not user32.GetWindowRect(ctypes.c_void_p(hwnd), ctypes.byref(rect)):
            return False

        # MONITOR_DEFAULTTONEAREST: compare against the monitor the window is
        # actually on, not the primary one, so this is correct on a multi-head
        # desktop where the fullscreen app is on the second screen.
        monitor = user32.MonitorFromWindow(ctypes.c_void_p(hwnd), 2)
        if not monitor:
            return False
        info = _MonitorInfo()
        info.cbSize = ctypes.sizeof(_MonitorInfo)
        if not user32.GetMonitorInfoW(monitor, ctypes.byref(info)):
            return False
        screen = info.rcMonitor

        slack = FULLSCREEN_SLACK_PX
        return (rect.left <= screen.left + slack and rect.top <= screen.top + slack
                and rect.right >= screen.right - slack and rect.bottom >= screen.bottom - slack)
    except Exception:
        return False


def on_battery() -> bool:
    """True when the machine is running from its battery.

    False when plugged in, when the state is unknown, and on any platform
    without this call, so an unknown answer never throttles anything.
    """
    if not sys.platform.startswith("win"):
        return False
    try:
        status = _PowerStatus()
        if not ctypes.windll.kernel32.GetSystemPowerStatus(ctypes.byref(status)):
            return False
        # 0 offline, 1 online, 255 unknown.
        return int(status.ACLineStatus) == 0
    except Exception:
        return False


class FramePolicy:
    """Tracks the machine state and reports the frame rate that follows from it.

    Keeps the last decision so a caller can cheaply ask whether anything needs
    changing, and only re-probes when `POLL_MS` has passed.
    """

    def __init__(self, target_fps: float, poll_ms: int = POLL_MS,
                 fullscreen_probe=foreground_window_is_fullscreen,
                 battery_probe=on_battery) -> None:
        self.target_fps = max(1.0, float(target_fps))
        self.poll_ms = max(0, int(poll_ms))
        self._fullscreen_probe = fullscreen_probe
        self._battery_probe = battery_probe
        self._last_poll_ms: float | None = None
        self.fullscreen = False
        self.battery = False
        self.current_fps = self.target_fps
        self.reason = "target"

    def set_target_fps(self, fps: float) -> float:
        """The user chose a rate; re-derive from it immediately."""
        self.target_fps = max(1.0, float(fps))
        return self._decide()

    def poll(self, now_ms: float, exclude_hwnd: Optional[int] = None) -> Optional[float]:
        """Re-probe if it is time to, returning a new FPS only when it changed."""
        if self._last_poll_ms is not None and now_ms - self._last_poll_ms < self.poll_ms:
            return None
        self._last_poll_ms = now_ms
        self.fullscreen = bool(self._fullscreen_probe(exclude_hwnd))
        self.battery = bool(self._battery_probe())
        previous = self.current_fps
        decided = self._decide()
        return decided if abs(decided - previous) > 0.01 else None

    def _decide(self) -> float:
        self.current_fps = decide_fps(self.target_fps, self.fullscreen, self.battery)
        if self.fullscreen:
            self.reason = "fullscreen window in front"
        elif self.battery:
            self.reason = "on battery"
        else:
            self.reason = "target"
        return self.current_fps
