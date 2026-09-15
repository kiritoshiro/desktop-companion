from __future__ import annotations

"""Lightweight desktop/window awareness helpers.

The companion overlay is always-on-top and transparent. Real application
windows cannot actually occlude pixels drawn by the overlay, so the animation
layer uses this snapshot to *simulate* depth: when a spider deliberately walks
into a real top-visible window rectangle the renderer clips it as though it
passed behind the window. File Explorer windows and visible desktop folder icons
can be used as occasional portal entrances/exits.

The Win32 code is intentionally defensive. On non-Windows platforms, or if the
API calls fail, callers simply get an empty list and the overlay behaves as it
always did.
"""

from dataclasses import dataclass
import ctypes
import os
import sys
from ctypes import wintypes
from pathlib import Path
from typing import List, Set


@dataclass(frozen=True)
class DesktopSurface:
    """A rectangular desktop object that spiders can react to."""

    x: float
    y: float
    w: float
    h: float
    title: str = ""
    class_name: str = ""
    kind: str = "window"  # "window" or "folder"

    @property
    def left(self) -> float:
        return self.x

    @property
    def top(self) -> float:
        return self.y

    @property
    def right(self) -> float:
        return self.x + self.w

    @property
    def bottom(self) -> float:
        return self.y + self.h


# Top-level windows from these classes are desktop infrastructure rather than
# useful hide-behind surfaces. Keeping them out avoids every spider vanishing
# behind the taskbar/desktop worker windows.
_SKIP_CLASSES = {
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "DV2ControlHost",
    "MsgrIMEWindowClass",
    "IME",
    "ToolTips_Class32",
    "NotifyIconOverflowWindow",
    "ApplicationFrameInputSinkWindow",
    "Windows.UI.Composition.DesktopWindowContentBridge",
    "Xaml_WindowedPopupClass",
}

_FOLDER_CLASSES = {
    "CabinetWClass",   # File Explorer folder windows on modern Windows.
    "ExploreWClass",   # Older Explorer folder window class.
}


class _LVITEMW(ctypes.Structure):
    """Subset-compatible LVITEMW layout for reading desktop icon captions."""

    _fields_ = [
        ("mask", wintypes.UINT),
        ("iItem", ctypes.c_int),
        ("iSubItem", ctypes.c_int),
        ("state", wintypes.UINT),
        ("stateMask", wintypes.UINT),
        ("pszText", ctypes.c_void_p),
        ("cchTextMax", ctypes.c_int),
        ("iImage", ctypes.c_int),
        ("lParam", wintypes.LPARAM),
        ("iIndent", ctypes.c_int),
        ("iGroupId", ctypes.c_int),
        ("cColumns", wintypes.UINT),
        ("puColumns", ctypes.c_void_p),
        ("piColFmt", ctypes.c_void_p),
        ("iGroup", ctypes.c_int),
    ]


def _rect_from_dwm(hwnd: int):
    """Return the extended frame bounds for hwnd, or None on failure."""
    try:
        dwmapi = ctypes.windll.dwmapi
        rect = wintypes.RECT()
        DWMWA_EXTENDED_FRAME_BOUNDS = 9
        if dwmapi.DwmGetWindowAttribute(
            wintypes.HWND(hwnd),
            wintypes.DWORD(DWMWA_EXTENDED_FRAME_BOUNDS),
            ctypes.byref(rect),
            ctypes.sizeof(rect),
        ) == 0:
            return rect
    except Exception:
        return None
    return None


def _rect_from_user32(hwnd: int):
    try:
        rect = wintypes.RECT()
        if ctypes.windll.user32.GetWindowRect(wintypes.HWND(hwnd), ctypes.byref(rect)):
            return rect
    except Exception:
        return None
    return None


def _window_text(hwnd: int) -> str:
    try:
        user32 = ctypes.windll.user32
        length = int(user32.GetWindowTextLengthW(wintypes.HWND(hwnd)))
        if length <= 0:
            return ""
        buf = ctypes.create_unicode_buffer(length + 1)
        user32.GetWindowTextW(wintypes.HWND(hwnd), buf, length + 1)
        return buf.value or ""
    except Exception:
        return ""


def _class_name(hwnd: int) -> str:
    try:
        buf = ctypes.create_unicode_buffer(256)
        ctypes.windll.user32.GetClassNameW(wintypes.HWND(hwnd), buf, len(buf))
        return buf.value or ""
    except Exception:
        return ""


def _is_dwm_cloaked(hwnd: int) -> bool:
    """True for hidden/virtualized DWM windows that still enumerate as visible."""
    try:
        dwmapi = ctypes.windll.dwmapi
        cloaked = ctypes.c_int(0)
        DWMWA_CLOAKED = 14
        if dwmapi.DwmGetWindowAttribute(
            wintypes.HWND(hwnd),
            wintypes.DWORD(DWMWA_CLOAKED),
            ctypes.byref(cloaked),
            ctypes.sizeof(cloaked),
        ) == 0:
            return bool(cloaked.value)
    except Exception:
        pass
    return False


def _extended_style(hwnd: int) -> int:
    try:
        user32 = ctypes.windll.user32
        GWL_EXSTYLE = -20
        if ctypes.sizeof(ctypes.c_void_p) == 8:
            fn = user32.GetWindowLongPtrW
            fn.restype = ctypes.c_longlong
        else:
            fn = user32.GetWindowLongW
            fn.restype = ctypes.c_long
        fn.argtypes = [wintypes.HWND, ctypes.c_int]
        return int(fn(wintypes.HWND(hwnd), GWL_EXSTYLE))
    except Exception:
        return 0


def _layered_window_is_effectively_invisible(hwnd: int, ex_style: int) -> bool:
    """Skip alpha-0 helper overlays that otherwise create invisible walls."""
    WS_EX_LAYERED = 0x00080000
    LWA_ALPHA = 0x00000002
    if not (ex_style & WS_EX_LAYERED):
        return False
    try:
        user32 = ctypes.windll.user32
        color_key = wintypes.DWORD()
        alpha = ctypes.c_ubyte(255)
        flags = wintypes.DWORD()
        if user32.GetLayeredWindowAttributes(
            wintypes.HWND(hwnd),
            ctypes.byref(color_key),
            ctypes.byref(alpha),
            ctypes.byref(flags),
        ):
            return bool(flags.value & LWA_ALPHA) and int(alpha.value) <= 8
    except Exception:
        pass
    return False


def _desktop_folder_names() -> Set[str]:
    """Best-effort set of folder names that are likely shown on the desktop."""
    roots = []
    for value in (os.environ.get("USERPROFILE"), os.environ.get("OneDrive")):
        if value:
            roots.append(Path(value) / "Desktop")
    try:
        roots.append(Path.home() / "Desktop")
    except Exception:
        pass
    public = os.environ.get("PUBLIC") or r"C:\Users\Public"
    roots.append(Path(public) / "Desktop")

    names: Set[str] = set()
    for root in roots:
        try:
            if not root.exists() or not root.is_dir():
                continue
            for child in root.iterdir():
                try:
                    if child.is_dir():
                        names.add(child.name.casefold())
                except Exception:
                    continue
        except Exception:
            continue
    return names


def _desktop_listview_hwnd() -> int:
    """Return the desktop icon SysListView32 handle, or 0 if unavailable."""
    try:
        user32 = ctypes.windll.user32
        try:
            user32.FindWindowW.restype = wintypes.HWND
            user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
            user32.FindWindowExW.restype = wintypes.HWND
            user32.FindWindowExW.argtypes = [wintypes.HWND, wintypes.HWND, wintypes.LPCWSTR, wintypes.LPCWSTR]
        except Exception:
            pass
        progman = int(user32.FindWindowW("Progman", None) or 0)
        if progman:
            defview = int(user32.FindWindowExW(wintypes.HWND(progman), 0, "SHELLDLL_DefView", None) or 0)
            if defview:
                listview = int(user32.FindWindowExW(wintypes.HWND(defview), 0, "SysListView32", None) or 0)
                if listview:
                    return listview

        found = ctypes.c_void_p(0)
        EnumWindowsProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        def callback(hwnd, _lparam):
            try:
                defview = int(user32.FindWindowExW(hwnd, 0, "SHELLDLL_DefView", None) or 0)
                if defview:
                    listview = int(user32.FindWindowExW(wintypes.HWND(defview), 0, "SysListView32", None) or 0)
                    if listview:
                        found.value = listview
                        return False
            except Exception:
                pass
            return True

        user32.EnumWindows(EnumWindowsProc(callback), 0)
        return int(found.value or 0)
    except Exception:
        return 0


def _read_remote(process, address: int, obj) -> bool:
    try:
        nread = ctypes.c_size_t(0)
        return bool(ctypes.windll.kernel32.ReadProcessMemory(
            process,
            ctypes.c_void_p(address),
            ctypes.byref(obj),
            ctypes.sizeof(obj),
            ctypes.byref(nread),
        ))
    except Exception:
        return False


def _read_remote_text(process, address: int, chars: int) -> str:
    try:
        buf = ctypes.create_unicode_buffer(chars)
        nread = ctypes.c_size_t(0)
        ok = ctypes.windll.kernel32.ReadProcessMemory(
            process,
            ctypes.c_void_p(address),
            buf,
            ctypes.sizeof(buf),
            ctypes.byref(nread),
        )
        return (buf.value or "") if ok else ""
    except Exception:
        return ""


def _write_remote(process, address: int, obj) -> bool:
    try:
        nwritten = ctypes.c_size_t(0)
        return bool(ctypes.windll.kernel32.WriteProcessMemory(
            process,
            ctypes.c_void_p(address),
            ctypes.byref(obj),
            ctypes.sizeof(obj),
            ctypes.byref(nwritten),
        ))
    except Exception:
        return False


def _desktop_folder_icon_surfaces(
    *,
    origin_x: int = 0,
    origin_y: int = 0,
    screen_w: int | None = None,
    screen_h: int | None = None,
) -> List[DesktopSurface]:
    """Best-effort visible desktop folder icon rectangles.

    Windows stores desktop icons in Explorer's SysListView32, so reading positions
    requires a tiny remote-memory round trip. It is intentionally limited and
    failure-safe; if anything looks wrong we simply return no folder icons.
    """
    if not sys.platform.startswith("win"):
        return []
    try:
        user32 = ctypes.windll.user32
        kernel32 = ctypes.windll.kernel32
        try:
            user32.SendMessageW.restype = ctypes.c_ssize_t
            user32.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
            kernel32.OpenProcess.restype = wintypes.HANDLE
            kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            kernel32.VirtualAllocEx.restype = ctypes.c_void_p
            kernel32.VirtualAllocEx.argtypes = [wintypes.HANDLE, ctypes.c_void_p, ctypes.c_size_t, wintypes.DWORD, wintypes.DWORD]
            kernel32.VirtualFreeEx.argtypes = [wintypes.HANDLE, ctypes.c_void_p, ctypes.c_size_t, wintypes.DWORD]
            kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
        except Exception:
            pass
        hwnd = _desktop_listview_hwnd()
        if not hwnd:
            return []

        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(wintypes.HWND(hwnd), ctypes.byref(pid))
        if not pid.value:
            return []

        PROCESS_QUERY_INFORMATION = 0x0400
        PROCESS_VM_OPERATION = 0x0008
        PROCESS_VM_READ = 0x0010
        PROCESS_VM_WRITE = 0x0020
        process = kernel32.OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE,
            False,
            int(pid.value),
        )
        if not process:
            return []

        MEM_COMMIT = 0x1000
        MEM_RESERVE = 0x2000
        MEM_RELEASE = 0x8000
        PAGE_READWRITE = 0x04
        LVM_FIRST = 0x1000
        LVM_GETITEMCOUNT = LVM_FIRST + 4
        LVM_GETITEMPOSITION = LVM_FIRST + 16
        LVM_GETITEMTEXTW = LVM_FIRST + 115
        LVIF_TEXT = 0x0001

        count = int(user32.SendMessageW(wintypes.HWND(hwnd), LVM_GETITEMCOUNT, 0, 0))
        count = max(0, min(count, 96))
        if count <= 0:
            kernel32.CloseHandle(process)
            return []

        point_size = ctypes.sizeof(wintypes.POINT)
        lvitem_size = ctypes.sizeof(_LVITEMW)
        text_chars = 260
        text_bytes = text_chars * ctypes.sizeof(ctypes.c_wchar)
        block_size = max(point_size, lvitem_size + text_bytes)
        remote = kernel32.VirtualAllocEx(process, None, block_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
        if not remote:
            kernel32.CloseHandle(process)
            return []
        remote_text = int(remote) + lvitem_size

        client_origin = wintypes.POINT(0, 0)
        user32.ClientToScreen(wintypes.HWND(hwnd), ctypes.byref(client_origin))
        folder_names = _desktop_folder_names()
        include_unknown_when_no_folder_names = not folder_names
        surfaces: List[DesktopSurface] = []

        try:
            for i in range(count):
                pt = wintypes.POINT()
                if not user32.SendMessageW(wintypes.HWND(hwnd), LVM_GETITEMPOSITION, i, int(remote)):
                    continue
                if not _read_remote(process, int(remote), pt):
                    continue

                item = _LVITEMW()
                item.mask = LVIF_TEXT
                item.iItem = i
                item.iSubItem = 0
                item.pszText = ctypes.c_void_p(remote_text)
                item.cchTextMax = text_chars
                if _write_remote(process, int(remote), item):
                    user32.SendMessageW(wintypes.HWND(hwnd), LVM_GETITEMTEXTW, i, int(remote))
                    name = _read_remote_text(process, remote_text, text_chars)
                else:
                    name = ""
                if folder_names and name.casefold() not in folder_names:
                    continue
                if not folder_names and not include_unknown_when_no_folder_names:
                    continue

                # Desktop ListView positions are the upper-left of each item cell.
                # Use a generous icon+caption footprint so the portal is easy to see.
                x = int(client_origin.x) + int(pt.x) - int(origin_x) - 6
                y = int(client_origin.y) + int(pt.y) - int(origin_y) - 6
                w = 86
                h = 86
                if screen_w is not None and screen_h is not None:
                    if x + w <= 0 or y + h <= 0 or x >= screen_w or y >= screen_h:
                        continue
                surfaces.append(DesktopSurface(float(x), float(y), float(w), float(h), name, "DesktopFolderIcon", "folder"))
                if len(surfaces) >= 32:
                    break
        finally:
            kernel32.VirtualFreeEx(process, ctypes.c_void_p(int(remote)), 0, MEM_RELEASE)
            kernel32.CloseHandle(process)
        return surfaces
    except Exception:
        return []


def snapshot_desktop_surfaces(
    *,
    exclude_hwnd: int | None = None,
    origin_x: int = 0,
    origin_y: int = 0,
    screen_w: int | None = None,
    screen_h: int | None = None,
    include_desktop_icons: bool = True,
) -> List[DesktopSurface]:
    """Return visible top-level windows in overlay-local coordinates.

    ``exclude_hwnd`` should be the overlay window handle. All windows belonging
    to the current overlay process are also skipped so menus/tool windows do not
    become occluders.
    """
    if not sys.platform.startswith("win"):
        return []

    surfaces: List[DesktopSurface] = []
    try:
        user32 = ctypes.windll.user32
        current_pid = os.getpid()
        excluded = int(exclude_hwnd or 0)

        EnumWindowsProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        def callback(hwnd, _lparam):
            try:
                hwnd_int = int(hwnd)
                if hwnd_int == excluded:
                    return True
                if not user32.IsWindowVisible(hwnd):
                    return True
                if user32.IsIconic(hwnd):
                    return True

                pid = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if int(pid.value) == current_pid:
                    return True

                cls = _class_name(hwnd_int)
                if not cls or cls in _SKIP_CLASSES:
                    return True
                if _is_dwm_cloaked(hwnd_int):
                    return True
                ex_style = _extended_style(hwnd_int)
                WS_EX_TOOLWINDOW = 0x00000080
                WS_EX_TRANSPARENT = 0x00000020
                if ex_style & WS_EX_TOOLWINDOW:
                    return True
                if ex_style & WS_EX_TRANSPARENT and cls not in _FOLDER_CLASSES:
                    return True
                if _layered_window_is_effectively_invisible(hwnd_int, ex_style):
                    return True

                title = _window_text(hwnd_int)
                # Untitled utility windows are frequently invisible helpers. File
                # Explorer can sometimes have an empty caption briefly, so keep known
                # folder classes even without a title.
                if not title and cls not in _FOLDER_CLASSES:
                    return True

                rect = _rect_from_dwm(hwnd_int) or _rect_from_user32(hwnd_int)
                if rect is None:
                    return True
                left = int(rect.left) - int(origin_x)
                top = int(rect.top) - int(origin_y)
                right = int(rect.right) - int(origin_x)
                bottom = int(rect.bottom) - int(origin_y)
                w = right - left
                h = bottom - top
                if w < 90 or h < 70:
                    return True

                if screen_w is not None and screen_h is not None:
                    # No intersection with the virtual desktop covered by the overlay.
                    if right <= 0 or bottom <= 0 or left >= screen_w or top >= screen_h:
                        return True

                kind = "folder" if cls in _FOLDER_CLASSES else "window"
                surfaces.append(DesktopSurface(float(left), float(top), float(w), float(h), title, cls, kind))
            except Exception:
                pass
            return True

        user32.EnumWindows(EnumWindowsProc(callback), 0)
    except Exception:
        return []

    windows = surfaces[:40]
    icons: List[DesktopSurface] = []
    if include_desktop_icons:
        icons = _desktop_folder_icon_surfaces(
            origin_x=origin_x,
            origin_y=origin_y,
            screen_w=screen_w,
            screen_h=screen_h,
        )

    # EnumWindows yields top-level windows in Z order from front to back. Desktop
    # icons are appended last because they live behind every real window.
    return windows + icons[:32]
