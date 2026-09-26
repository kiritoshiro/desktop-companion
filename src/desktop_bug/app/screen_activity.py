"""Which monitors are really in use, so missions only populate those.

The owner: *"sometimes a second screen is connected however is not active,
as in either sleeping or connected to another pc and active there. so make
sure that only when the second/or other screens are active only then
populate them."*

Windows keeps a monitor in the desktop layout in both cases, so Qt lists it
like any other. Two checks, both read-only:

1. **Windows' own flags** (``EnumDisplayDevices``): a monitor Windows no
   longer drives is not marked active.
2. **The monitor itself**, over DDC/CI (the channel monitors use for their
   brightness controls): its power mode (VCP code 0xD6). A monitor in standby
   or off says so. Many monitors answer; some do not, and those count as
   active.

A monitor showing another PC's input is usually still "on" by both checks,
and Windows cannot see which input it shows. For that the Adventure page
lists the screens and the player can switch any off by hand
(``disabled_screens`` in the profile). The main screen is always used.
"""
from __future__ import annotations

import sys

# VCP 0xD6 power mode: 1 on, 2 standby, 3 suspend, 4 off (DPM), 5 off (hard).
POWER_ON = 1


def monitor_states() -> dict[str, str]:
    """Qt screen name (``\\\\.\\DISPLAY2``) -> "active", "asleep" or "off".
    Monitors that cannot be asked are left out, meaning active."""
    if not sys.platform.startswith("win"):
        return {}
    states = {}
    try:
        states.update(_windows_flags())
    except Exception:
        pass
    try:
        for name, mode in _ddc_power_modes().items():
            if mode != POWER_ON and states.get(name, "active") == "active":
                states[name] = "asleep" if mode in (2, 3) else "off"
            else:
                states.setdefault(name, "active")
    except Exception:
        pass
    return states


def playable_screens(screens, primary, disabled=(), states=None):
    """The QScreens a mission may populate: the main one always; the others
    when active and not switched off by the player."""
    states = monitor_states() if states is None else states
    disabled = set(disabled or ())
    out = []
    for screen in screens:
        name = screen.name()
        if screen is primary:
            out.append(screen)
        elif name not in disabled and states.get(name, "active") == "active":
            out.append(screen)
    return out


def _windows_flags() -> dict[str, str]:
    import ctypes
    from ctypes import wintypes

    class DISPLAY_DEVICEW(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("DeviceName", wintypes.WCHAR * 32),
                    ("DeviceString", wintypes.WCHAR * 128), ("StateFlags", wintypes.DWORD),
                    ("DeviceID", wintypes.WCHAR * 128), ("DeviceKey", wintypes.WCHAR * 128)]

    ATTACHED = 0x00000001        # adapter: part of the desktop; monitor: active
    user32 = ctypes.windll.user32
    states = {}
    i = 0
    while True:
        adapter = DISPLAY_DEVICEW()
        adapter.cb = ctypes.sizeof(adapter)
        if not user32.EnumDisplayDevicesW(None, i, ctypes.byref(adapter), 0):
            break
        i += 1
        if not adapter.StateFlags & ATTACHED:
            continue
        j, any_active, any_monitor = 0, False, False
        while True:
            monitor = DISPLAY_DEVICEW()
            monitor.cb = ctypes.sizeof(monitor)
            if not user32.EnumDisplayDevicesW(adapter.DeviceName, j, ctypes.byref(monitor), 0):
                break
            j += 1
            any_monitor = True
            any_active = any_active or bool(monitor.StateFlags & ATTACHED)
        if any_monitor:
            states[adapter.DeviceName] = "active" if any_active else "off"
    return states


def _ddc_power_modes() -> dict[str, int]:
    import ctypes
    from ctypes import wintypes

    user32, dxva2 = ctypes.windll.user32, ctypes.windll.dxva2

    class MONITORINFOEXW(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT), ("rcWork", wintypes.RECT),
                    ("dwFlags", wintypes.DWORD), ("szDevice", wintypes.WCHAR * 32)]

    class PHYSICAL_MONITOR(ctypes.Structure):
        _fields_ = [("hPhysicalMonitor", wintypes.HANDLE), ("szPhysicalMonitorDescription", wintypes.WCHAR * 128)]

    modes = {}

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMONITOR, wintypes.HDC, ctypes.POINTER(wintypes.RECT),
                        wintypes.LPARAM)
    def visit(hmonitor, _hdc, _rect, _lparam):
        info = MONITORINFOEXW()
        info.cbSize = ctypes.sizeof(info)
        if not user32.GetMonitorInfoW(hmonitor, ctypes.byref(info)):
            return True
        count = wintypes.DWORD()
        if not dxva2.GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, ctypes.byref(count)) or not count.value:
            return True
        physical = (PHYSICAL_MONITOR * count.value)()
        if not dxva2.GetPhysicalMonitorsFromHMONITOR(hmonitor, count.value, physical):
            return True
        try:
            for monitor in physical:
                kind, current, maximum = wintypes.DWORD(), wintypes.DWORD(), wintypes.DWORD()
                if dxva2.GetVCPFeatureAndVCPFeatureReply(monitor.hPhysicalMonitor, 0xD6, ctypes.byref(kind),
                                                         ctypes.byref(current), ctypes.byref(maximum)):
                    modes[info.szDevice] = int(current.value)
        finally:
            dxva2.DestroyPhysicalMonitors(count.value, physical)
        return True

    user32.EnumDisplayMonitors(None, None, visit, 0)
    return modes
