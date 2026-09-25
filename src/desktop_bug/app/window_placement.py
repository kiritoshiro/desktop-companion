"""Every window opens in the middle of the main monitor.

The owner: *"the default position of all windows should be on the main
monitor middle."* The overlay spans every monitor, so a dialog it owns was
centred on the whole desktop -- on two monitors that is the seam between
them, or a corner no screen shows. The settings window was left wherever
Windows put it.

One application-wide event filter handles them all -- the settings window,
the pause menu, the skill tree, message boxes, input dialogs -- the first
time each is shown. Menus, tooltips, combo popups and the overlay itself are
not dialogs (they are popups or tool windows) and are left alone. Moving a
window afterwards is the player's business, so it only happens once.
"""

from __future__ import annotations

import atexit

from PyQt5 import sip
from PyQt5.QtCore import QEvent, QObject, QPoint, QRect, Qt
from PyQt5.QtGui import QGuiApplication

_PLACED = "_desktopBugPlacedOnPrimary"
_PLACEABLE = (Qt.Window, Qt.Dialog, Qt.Sheet)


def primary_rect() -> QRect:
    """The main monitor's usable area (without the taskbar), in desktop pixels."""
    screen = QGuiApplication.primaryScreen()
    if screen is None:
        return QRect()
    return screen.availableGeometry()


def primary_rect_local(origin: QPoint) -> QRect:
    """The main monitor's usable area in a window's own coordinates."""
    rect = primary_rect()
    return rect.translated(-origin.x(), -origin.y()) if not rect.isEmpty() else rect


def center_on_primary(widget) -> None:
    area = primary_rect()
    if area.isEmpty():
        return
    frame = widget.frameGeometry()
    width = min(frame.width(), area.width())
    height = min(frame.height(), area.height())
    x = area.left() + (area.width() - width) // 2
    y = area.top() + (area.height() - height) // 2
    widget.move(x + (widget.x() - frame.x()), y + (widget.y() - frame.y()))


def should_place(widget) -> bool:
    return (widget.isWindow()
            and widget.windowType() in _PLACEABLE
            and not widget.property(_PLACED))


class PrimaryCenterFilter(QObject):
    def eventFilter(self, obj, event):  # noqa: N802 - Qt API name
        if event.type() == QEvent.Show and obj.isWidgetType() and should_place(obj):
            obj.setProperty(_PLACED, True)
            center_on_primary(obj)
        return False


_filter = None


def install(app) -> None:
    """Centre every window and dialog on the main monitor when it first opens."""
    global _filter
    if _filter is None:
        _filter = PrimaryCenterFilter(app)
        app.installEventFilter(_filter)
        app.aboutToQuit.connect(uninstall)


def uninstall() -> None:
    """Detach Python callbacks before Qt destroys windows during interpreter exit.

    Larger Adventure dialogs exposed a Windows access violation when the global
    Show-event filter survived into QApplication's native destruction. Normal
    app exit uses aboutToQuit; atexit also covers headless tools and tests which
    do not enter the event loop.
    """
    global _filter
    app = QGuiApplication.instance()
    if app is not None and _filter is not None and not sip.isdeleted(_filter):
        app.removeEventFilter(_filter)
    _filter = None


atexit.register(uninstall)
