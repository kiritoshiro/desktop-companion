"""The settings window has to fit, and nothing may sit on top of anything.

The owner reported that the menu "doesnt look good, everything tends to
overlap". Measuring it rather than squinting at it found four separate
causes, and each one is pinned here because each would come back the same
way -- by someone adding a control with a long label and not noticing what
it does to the window's *minimum* size:

* A `QComboBox` asks for the width of its widest **item**, not of what it is
  showing. Single combos holding "Skitter - rapid bursts + tiny stops" and
  "Coppercurl Soft Tarantula" were demanding 522 and 420 pixels.
* `QHeaderView.ResizeToContents` sizes a column to the wider of its header
  and its contents, so "How many" and "Abilities" claimed 129 and 192 pixels
  for a spin box and a short label -- and between them the fixed columns took
  980 of 1022, collapsing Skin and Temperament to 18px each.
* The four quick-set buttons were 894px of the Creatures group's 1080px
  minimum. The table itself only asks for 71.
* The colour swatch was a 34px icon inside a 26px-tall button, so it
  overflowed and painted across the skin name in the column to its left.

Together those put the window's minimum at 1241x1016. It is now 896x836.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest
from desktop_bug.app.config_ui import (
    COL_COLORS,
    COL_SKIN,
    COL_TEMPERAMENT,
    SLOT_HEADERS,
    ConfigWindow,
)
from PyQt5.QtWidgets import QGroupBox

# A 1280x800 laptop, less room for the frame and the taskbar. The window has
# to be usable there, which is the whole point of the exercise.
MODEST_SCREEN = (1180, 860)


@pytest.fixture
def window(qapp, monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="dc-layout-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    win = ConfigWindow()
    win.mode_shell.show_mode("companion")
    yield win
    win.close()


def _laid_out(window, width, height, qapp):
    window.resize(width, height)
    window.show()
    qapp.processEvents()
    qapp.processEvents()
    return window


def test_the_window_fits_a_modest_screen(window):
    hint = window.centralWidget().minimumSizeHint()
    assert hint.width() <= MODEST_SCREEN[0], hint.width()
    assert hint.height() <= MODEST_SCREEN[1], hint.height()


@pytest.mark.parametrize("size", [(640, 520), (820, 700), (1066, 914), (1400, 1100)])
def test_nothing_is_drawn_across_the_creature_table(window, qapp, size):
    """The failure the panel check below did not catch.

    Comparing only top-level panels said the layout was clean while the "Add
    slot" button was being painted across the third row of the table: the
    collision was between two children *inside* one panel. It happened
    because the Creatures group was handed 238px against a 250px minimum,
    and Qt resolves that shortfall by overlapping rather than by refusing.

    Clamping the window's minimum does not fix it -- a QMainWindow does not
    propagate its central widget's minimum height, so the window shrank
    anyway. The panels scroll instead, which cannot overlap at any size.
    """
    _laid_out(window, *size, qapp)
    table = window.table.geometry()
    for button in (window.add_slot_btn, window.clear_slots_btn):
        overlap = table.intersected(button.geometry())
        assert overlap.width() <= 2 or overlap.height() <= 2, (
            size, button.text(), overlap,
        )


@pytest.mark.parametrize("size", [(900, 700), (1000, 780), (1240, 1020)])
def test_no_panel_sits_on_top_of_another(window, qapp, size):
    _laid_out(window, *size, qapp)
    groups = window.findChildren(QGroupBox)
    assert len(groups) >= 5, len(groups)
    for index, first in enumerate(groups):
        for second in groups[index + 1:]:
            if first.parent() is not second.parent():
                continue
            overlap = first.geometry().intersected(second.geometry())
            assert overlap.width() <= 2 or overlap.height() <= 2, (
                first.title(), second.title(), overlap,
            )


def test_no_control_spills_out_of_its_cell(window, qapp):
    """The swatch painting over the skin name was exactly this."""
    _laid_out(window, 1000, 780, qapp)
    table = window.table
    assert table.rowCount() > 0, "no slots to check"
    for row in range(table.rowCount()):
        for column in range(table.columnCount()):
            widget = table.cellWidget(row, column)
            if widget is None:
                continue
            cell = table.visualRect(table.model().index(row, column))
            assert widget.width() <= cell.width() + 1, (SLOT_HEADERS[column], row)
            assert widget.height() <= cell.height() + 1, (SLOT_HEADERS[column], row)


def test_the_colour_swatch_fits_its_button(window, qapp):
    _laid_out(window, 1000, 780, qapp)
    button = window.table.cellWidget(0, COL_COLORS)
    assert button is not None
    assert button.iconSize().height() <= button.height(), (
        button.iconSize().height(), button.height(),
    )
    assert button.iconSize().width() <= button.width()


def test_the_columns_fit_without_scrolling(window, qapp):
    _laid_out(window, 1000, 780, qapp)
    table = window.table
    total = sum(table.columnWidth(i) for i in range(table.columnCount()))
    assert total <= table.viewport().width() + 2, (total, table.viewport().width())


def test_the_skin_column_gets_the_room(window, qapp):
    """It collapsed to 18px once, then to 239 while sharing the slack with
    Temperament -- which is more than "Balanced" needs and not enough for
    "Coppercurl Soft Tarantula". Skin is the column a person reads."""
    _laid_out(window, 1000, 780, qapp)
    table = window.table
    assert table.columnWidth(COL_SKIN) >= 200, table.columnWidth(COL_SKIN)
    assert table.columnWidth(COL_TEMPERAMENT) >= 120
    widest = max(range(table.columnCount()), key=table.columnWidth)
    assert widest == COL_SKIN, SLOT_HEADERS[widest]


def test_a_dropdown_does_not_demand_its_longest_entry(window, qapp):
    """The single biggest cause, and the least obvious one."""
    _laid_out(window, 1000, 780, qapp)
    # The two worst offenders, Movement and Mood, were retired by DC-52 --
    # they were scene-wide overrides of what the Temperament column already
    # decides. The rule they proved still has to hold for the combos that
    # are left, including the per-row ones, which are the long ones now.
    combos = [window.size_combo,
              window.table.cellWidget(0, COL_SKIN),
              window.table.cellWidget(0, COL_TEMPERAMENT)]
    for combo in combos:
        longest = max((combo.itemText(i) for i in range(combo.count())), key=len)
        assert len(longest) > 12, "pick a combo that actually has a long entry"
        assert combo.minimumSizeHint().width() < 320, (
            longest, combo.minimumSizeHint().width(),
        )


def test_every_shortened_control_still_explains_itself(window, qapp):
    """Labels were cut to fit; the meaning moved to the tooltip, not away."""
    _laid_out(window, 1000, 780, qapp)
    for control in (window.interferable_check, window.always_names_check,
                    window.always_levels_check, window.always_health_check,
                    window.add_slot_btn):
        # The Randomize buttons were here until the owner asked for that row
        # to be removed.
        assert control.toolTip().strip(), control.text()
        assert len(control.toolTip()) > len(control.text()), control.text()

    from desktop_bug.app.config_ui import COL_ABILITIES
    skills_btn = window.table.cellWidget(0, COL_ABILITIES)
    assert skills_btn.toolTip().strip()
    assert len(skills_btn.text()) <= 16, skills_btn.text()


def test_the_teams_panel_leaves_nothing_behind_when_it_rebuilds(window, qapp):
    """The stray vertical line through the Teams panel, explained.

    `_refresh_teams_panel` used to clear itself with `deleteLater()` alone.
    That defers destruction to the event loop, so until the loop runs each
    old widget is still a visible child of the panel -- and, having just
    been taken out of the layout, it has reverted to a default 640x480 at
    (0, 0) and is painting over everything. The panel rebuilds three times
    while the window is being constructed, before the loop has ever spun, so
    it left seventeen of them stacked up, and the right edge of one 640px
    QLineEdit was the line visible in the owner's screenshot.

    Counting orphans rather than looking for the line: a pixel test would
    pass the moment the stacking happened to be hidden behind something.
    """
    from PyQt5.QtWidgets import QWidget

    _laid_out(window, 980, 820, qapp)
    for _ in range(3):
        window._teams_signature = None
        window._refresh_teams_panel()
    qapp.processEvents()

    laid = {id(window.teams_layout.itemAt(i).widget())
            for i in range(window.teams_layout.count())}
    orphans = [child for child in window.teams_panel.findChildren(QWidget)
               if child.parentWidget() is window.teams_panel and id(child) not in laid]
    assert orphans == [], [
        (type(o).__name__, o.geometry()) for o in orphans]


def test_the_size_dropdown_does_not_fill_its_whole_row(window, qapp):
    """With Mood and Movement retired it was the only thing left in the row
    and grew to 700px to fill it, which reads as a mistake."""
    _laid_out(window, 980, 820, qapp)
    assert window.size_combo.width() <= 220, window.size_combo.width()
