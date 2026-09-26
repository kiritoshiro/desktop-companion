"""The settings window's creature table, and the column that came out of it.

The project owner asked for the "Pick 1-10" column to go. It was a per-slot
checkbox that deferred the count roll to launch time, which meant the number
shown in "How many" beside it was not the number that would spawn -- in a row
that already carries a model, a temperament, a count, abilities, colours, a
team and a job, it was both the least-used control and the most misleading.

Removing a column renumbers every widget lookup in the table, so most of
what is checked here is that the remaining controls still land in the right
place. The preset field itself is untouched: the overlay still honours
``count_random`` and still offers "Randomize count (1-10)" on right-click, so
a preset that uses it has to survive a load-and-save through a window that no
longer shows it.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.app.config_ui import (
    COL_ABILITIES,
    COL_COLORS,
    COL_COUNT,
    COL_CATEGORY,
    COL_JOB,
    COL_SKIN,
    COL_REMOVE,
    COL_TEAM,
    COL_TEMPERAMENT,
    RANDOM_CATEGORY_ID,
    RANDOM_MODEL_ID,
    SLOT_COLUMNS,
    SLOT_HEADERS,
    ConfigWindow,
)
from PyQt5.QtWidgets import QCheckBox, QComboBox, QPushButton, QSpinBox


@pytest.fixture
def scratch():
    """mkdtemp rather than tmp_path: the shared pytest-of-win root on this
    machine is intermittently locked, which fails the fixture before a test
    body ever runs. Established convention in this suite."""
    return Path(tempfile.mkdtemp(prefix="dc-settings-"))


@pytest.fixture
def window(qapp, monkeypatch, scratch):
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    win = ConfigWindow()
    yield win
    win.close()


def test_the_table_has_no_pick_1_10_column(window):
    headers = [window.table.horizontalHeaderItem(i).text()
               for i in range(window.table.columnCount())]
    assert headers == SLOT_HEADERS
    assert "Pick 1-10" not in headers
    assert window.table.columnCount() == SLOT_COLUMNS


def test_no_row_still_carries_the_checkbox(window):
    """The widget is gone, not merely hidden behind a removed header."""
    window.add_slot(None, None, 3, False)
    row = window.table.rowCount() - 1
    for col in range(SLOT_COLUMNS):
        assert not isinstance(window.table.cellWidget(row, col), QCheckBox)


def test_every_remaining_control_is_in_its_named_column(window):
    """Removing a column renumbers the lookups; this is what that breaks."""
    window.add_slot(None, None, 2, False)
    row = window.table.rowCount() - 1
    assert isinstance(window.table.cellWidget(row, COL_CATEGORY), QComboBox)
    assert isinstance(window.table.cellWidget(row, COL_SKIN), QComboBox)
    assert isinstance(window.table.cellWidget(row, COL_TEMPERAMENT), QComboBox)
    assert isinstance(window.table.cellWidget(row, COL_COUNT), QSpinBox)
    assert isinstance(window.table.cellWidget(row, COL_ABILITIES), QPushButton)
    assert isinstance(window.table.cellWidget(row, COL_COLORS), QPushButton)
    assert isinstance(window.table.cellWidget(row, COL_TEAM), QComboBox)
    assert isinstance(window.table.cellWidget(row, COL_JOB), QComboBox)
    assert isinstance(window.table.cellWidget(row, COL_REMOVE), QPushButton)


def test_a_slot_still_collects_its_count_and_job(window):
    window.table.setRowCount(0)
    window.add_slot(None, None, 4, False, job_id="builder")
    slots = window.collect_slots(silent=True)
    assert len(slots) == 1
    assert slots[0]["count"] == 4
    assert slots[0]["job"] == "builder"


def test_count_random_survives_a_load_and_save(window):
    """The column is gone; the preset field it wrote is not.

    A preset built elsewhere -- or by an older build -- can still set this,
    and the overlay still acts on it. Dropping it on save would silently
    rewrite the owner's file.
    """
    window.table.setRowCount(0)
    window.add_slot(None, None, 5, True)
    slots = window.collect_slots(silent=True)
    assert slots[0]["count_random"] is True
    assert slots[0]["count"] == 5

    window.table.setRowCount(0)
    window.add_slot(None, None, 5, False)
    assert window.collect_slots(silent=True)[0]["count_random"] is False


def test_a_random_count_slot_is_still_announced_in_the_summary(window):
    """With no column to show it, the summary line is the only place left."""
    window.table.setRowCount(0)
    window.add_slot(None, None, 5, True)
    window.update_summary()
    assert "random-count" in window.summary.text()


def test_random_counts_now_rolls_a_real_number(window):
    """It used to tick the box and leave the visible count lying.

    With the box gone the button has to put a number the table actually shows
    into the spin box, and must not leave the deferred flag set.
    """
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False)
    window.add_slot(None, None, 1, False)
    window.set_random_count_options()

    slots = window.collect_slots(silent=True)
    assert len(slots) == 2
    for slot in slots:
        assert slot["count_random"] is False
        assert 1 <= slot["count"] <= 10
    spins = [window.table.cellWidget(r, COL_COUNT) for r in range(window.table.rowCount())]
    assert [s.value() for s in spins] == [s["count"] for s in slots]


def test_a_saved_preset_still_loads_every_slot_field(window, scratch):
    """End to end through the file, since the table indices moved."""
    window.table.setRowCount(0)
    window.add_slot(None, None, 3, False, team_id="hunters", job_id="guard")
    slots = window.collect_slots(silent=True)
    preset = scratch / "roundtrip.json"
    preset.write_text(json.dumps({"name": "roundtrip", "slots": slots, "settings": {}}),
                      encoding="utf-8")

    window.table.setRowCount(0)
    window.load_preset_path(preset)

    reloaded = window.collect_slots(silent=True)
    assert len(reloaded) == 1
    assert reloaded[0]["count"] == 3
    assert reloaded[0]["team"] == "hunters"
    assert reloaded[0]["job"] == "guard"


# ----------------------------------------------------------------------
# Category and Skin
#
# DC-49 reduced forty-one leg rigs to four body plans, which made a single
# dropdown of forty-nine models the wrong shape: picking a spider is really
# picking a *kind* and then a *look*. The owner asked for the menu to list
# "only those few categories we left, and in other collumns just the skns
# column and/or color picker".
# ----------------------------------------------------------------------

def test_the_category_column_lists_only_the_body_plans(window):
    from desktop_bug.content.body_plans import BODY_PLAN_IDS
    from desktop_bug.content.enemy_kinds import ENEMY_BODY_PLANS

    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False)
    box = window.table.cellWidget(0, COL_CATEGORY)
    offered = [box.itemData(i) for i in range(box.count())]
    # Adventure's enemy-only body plans are not Companion categories.
    companion_plans = set(BODY_PLAN_IDS) - ENEMY_BODY_PLANS
    assert offered[0] == RANDOM_CATEGORY_ID
    assert set(offered[1:]) == companion_plans
    assert len(offered) == len(companion_plans) + 1


def test_the_skin_column_only_offers_that_category(window):
    window.table.setRowCount(0)
    window.add_slot("tarantula", None, 1, False)
    category = window.table.cellWidget(0, COL_CATEGORY)
    skin = window.table.cellWidget(0, COL_SKIN)
    assert category.currentData() == "tarantula", "the category did not follow the skin"

    category.setCurrentIndex(category.findData("jumper"))
    offered = [skin.itemData(i) for i in range(skin.count()) if skin.itemData(i) != RANDOM_MODEL_ID]
    assert offered, "no skins offered for the jumper category"
    for model_id in offered:
        assert window.models[model_id]["body_plan"] == "jumper", model_id


def test_switching_category_never_leaves_the_slot_empty(window):
    """It must land on a real skin, not silently become a random one."""
    window.table.setRowCount(0)
    window.add_slot("spider", None, 1, False)
    category = window.table.cellWidget(0, COL_CATEGORY)
    skin = window.table.cellWidget(0, COL_SKIN)
    for plan in ("segmented", "jumper", "tarantula", "bug"):
        category.setCurrentIndex(category.findData(plan))
        assert skin.currentData() not in (None, RANDOM_MODEL_ID), plan
        assert window.models[skin.currentData()]["body_plan"] == plan


def test_any_kind_offers_every_skin(window):
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False)
    category = window.table.cellWidget(0, COL_CATEGORY)
    skin = window.table.cellWidget(0, COL_SKIN)
    category.setCurrentIndex(category.findData(RANDOM_CATEGORY_ID))
    from desktop_bug.content.enemy_kinds import is_enemy_model

    offered = {skin.itemData(i) for i in range(skin.count())} - {RANDOM_MODEL_ID}
    # Every Companion skin; Adventure's enemy kinds are left out.
    wanted = {mid for mid in window.models if not is_enemy_model(mid)}
    assert offered == wanted, sorted(wanted ^ offered)


def test_the_preset_still_stores_a_model_not_a_category(window):
    """The split is a way of choosing; it must not change what is saved."""
    window.table.setRowCount(0)
    window.add_slot("giant_copper", None, 1, False)
    slot = window.collect_slots(silent=True)[0]
    assert slot["model"] == "giant_copper"
    assert "category" not in slot and "body_plan" not in slot


def test_loading_a_preset_puts_the_category_back(window, scratch):
    window.table.setRowCount(0)
    window.add_slot("silk_peacock_jumper", None, 1, False)
    preset = scratch / "cat.json"
    preset.write_text(json.dumps({
        "name": "cat", "slots": window.collect_slots(silent=True), "settings": {}}),
        encoding="utf-8")

    window.table.setRowCount(0)
    window.load_preset_path(preset)
    assert window.table.cellWidget(0, COL_CATEGORY).currentData() == "jumper"
    assert window.table.cellWidget(0, COL_SKIN).currentData() == "silk_peacock_jumper"
