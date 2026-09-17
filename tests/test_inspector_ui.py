"""Offscreen regression check for stable inspector combo-box popups."""

import json
import random


from PyQt5.QtWidgets import QApplication, QWidget

from desktop_bug.creature import Creature
from desktop_bug.engine import CreatureInspectorDialog
from desktop_bug.teams import normalize_teams
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """


class FakeManager:
    def __init__(self, creatures):
        self.creatures = creatures
        self.team_calls = []
        # The scene's teams, the way a real manager carries them. The inspector
        # offers these, so it and the settings window describe the same groups.
        self.team_profiles = normalize_teams(
            {"porch_guard": {"name": "Porch guard", "color": "#3fa9d9"},
             "hunters": {"name": "Hunters"}})

    def set_creature_team(self, creature, team_id):
        self.team_calls.append((creature, str(team_id)))
        creature.set_team(team_id)
        return "ok"


class FakeWindow(QWidget):
    def __init__(self, manager):
        super().__init__()
        self.manager = manager

    def _announce(self, _message):
        return None


@pytest.fixture
def inspector():
    """A fresh inspector over two spiders, shown offscreen.

    Fresh per test rather than shared: these checks assign teams, and one that
    inherited the team another had just set would pass or fail on the order
    pytest happened to run them in.
    """
    random.seed(31)
    model = json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text())
    personality = json.loads((ROOT / "personalities" / "cuddly.json").read_text())
    app = QApplication.instance()
    first = Creature(model, personality, 1200, 800)
    second = Creature(model, personality, 1200, 800, index=1)
    manager = FakeManager([first, second])
    window = FakeWindow(manager)
    dialog = CreatureInspectorDialog(window, first)
    dialog.show()
    app.processEvents()
    yield app, manager, dialog, first, second
    dialog.close()
    window.close()
    app.processEvents()


def test_a_refresh_leaves_an_open_relationship_popup_alone(inspector):
    app, _manager, dialog, _first, second = inspector
    combo = dialog._relation_controls[id(second)][2]
    combo.showPopup()
    app.processEvents()
    popup_was_visible = combo.view().isVisible()

    dialog.refresh()
    app.processEvents()
    assert dialog._relation_controls[id(second)][2] is combo
    if popup_was_visible:
        assert combo.view().isVisible(), "refresh closed an open relationship popup"


def test_the_picker_offers_this_scenes_teams_by_name(inspector):
    """"Porch guard" means nothing to the code and everything to its owner."""
    _app, _manager, dialog, _first, _second = inspector
    labels = [dialog.team_combo.itemText(i) for i in range(dialog.team_combo.count())]
    assert labels == ["Neutral / solo", "Hunters", "Porch guard"], labels
    ids = [dialog.team_combo.itemData(i) for i in range(dialog.team_combo.count())]
    assert ids == ["neutral", "hunters", "porch_guard"], ids


def test_choosing_a_team_by_name_assigns_the_id_behind_it(inspector):
    app, manager, dialog, first, _second = inspector
    dialog.team_combo.setCurrentIndex(dialog.team_combo.findData("porch_guard"))
    dialog._commit_team()
    app.processEvents()
    assert manager.team_calls[-1][1] == "porch_guard", manager.team_calls
    assert first.progression.team_id == "porch_guard"


def test_typing_a_name_does_not_assign_every_prefix_along_the_way(inspector):
    """The editable combo commits on a chosen entry or a finished edit, only."""
    app, manager, dialog, first, _second = inspector
    before = len(manager.team_calls)
    line_edit = dialog.team_combo.lineEdit()
    for index in range(1, len("hunters") + 1):
        line_edit.setText("hunters"[:index])
        app.processEvents()
    assert len(manager.team_calls) == before, manager.team_calls[before:]

    line_edit.editingFinished.emit()
    app.processEvents()
    assert [team for _creature, team in manager.team_calls[before:]] == ["hunters"], (
        manager.team_calls
    )
    assert first.progression.team_id == "hunters"

    # A finished edit that changes nothing must not rewrite the state file.
    line_edit.editingFinished.emit()
    app.processEvents()
    assert len(manager.team_calls) == before + 1, manager.team_calls


def test_the_inspector_says_what_hostility_actually_does(inspector):
    """"Foe" and "rivals" promise a fight the overlay cannot have yet."""
    _app, _manager, dialog, _first, _second = inspector
    from desktop_bug.teams import HOSTILITY_NOTE

    assert "no combat" in HOSTILITY_NOTE.lower()
    assert dialog.team_combo.toolTip() == HOSTILITY_NOTE
