"""Every window opens in the middle of the main monitor.

The owner: *"the default position of all windows should be on the main
monitor middle."* The overlay spans every monitor, so anything placed
against it -- its dialogs, the Adventure HUD, the take-control hint -- was
placed against the whole desktop, which on two monitors is the seam between
them or a corner no screen shows. Also removed at the owner's request: the
aim wedge and line drawn in front of the controlled spider.
"""

from __future__ import annotations

from types import SimpleNamespace

import pytest
from PyQt5.QtCore import QRect, Qt
from PyQt5.QtWidgets import QDialog, QMenu, QWidget

from desktop_bug.app import adventure_ui, window_placement
from desktop_bug.app.adventure_ui import HUD_HEIGHT, HUD_WIDTH, hud_rect


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    window_placement.install(qapp)


def _centre_of(widget):
    return widget.frameGeometry().center()


def test_a_dialog_opens_in_the_middle_of_the_main_monitor():
    area = window_placement.primary_rect()
    assert not area.isEmpty(), "no main monitor in this Qt platform"
    dialog = QDialog()
    dialog.resize(300, 200)
    dialog.move(area.left() + 3, area.top() + 3)
    dialog.show()
    try:
        centre = _centre_of(dialog)
        assert abs(centre.x() - area.center().x()) <= 2, (centre, area)
        assert abs(centre.y() - area.center().y()) <= 2, (centre, area)
    finally:
        dialog.close()


def test_only_the_first_opening_is_placed():
    """Where the player drags a window afterwards is theirs."""
    area = window_placement.primary_rect()
    dialog = QDialog()
    dialog.resize(200, 120)
    dialog.show()
    dialog.move(area.left() + 5, area.top() + 7)
    dialog.hide()
    dialog.show()
    try:
        assert dialog.pos().x() == area.left() + 5 and dialog.pos().y() == area.top() + 7
    finally:
        dialog.close()


def test_menus_and_tool_windows_are_left_alone():
    menu = QMenu()
    assert not window_placement.should_place(menu)
    tool = QWidget(None, Qt.Tool | Qt.FramelessWindowHint)
    assert not window_placement.should_place(tool), "the overlay is a tool window"
    parent = QDialog()
    child = QWidget(parent)
    assert not window_placement.should_place(child), "only top-level windows move"


def test_the_hud_starts_at_the_bottom_middle_of_the_main_monitor():
    area = window_placement.primary_rect()
    # An overlay spanning a second monitor to the left of the main one.
    left_extra = 1200
    overlay = SimpleNamespace(
        width=lambda: area.width() + left_extra, height=lambda: area.height(),
        geometry_rect=QRect(area.left() - left_extra, area.top(),
                            area.width() + left_extra, area.height()),
        _adventure_hud_position=None)
    rect = hud_rect(overlay)
    main_local_centre = left_extra + area.width() // 2
    assert abs(rect.center().x() - main_local_centre) <= 2, (rect, main_local_centre)
    assert rect.width() == HUD_WIDTH and rect.height() == HUD_HEIGHT
    assert rect.bottom() <= area.height() and rect.bottom() >= area.height() - HUD_HEIGHT - 30


def test_the_aim_indicator_is_gone():
    assert not hasattr(adventure_ui, "draw_aim")
    assert not hasattr(adventure_ui, "aim_region")
