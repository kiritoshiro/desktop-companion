"""Offscreen regression check for stable inspector combo-box popups."""

import json
import random


from PyQt5.QtWidgets import QApplication, QWidget

from desktop_bug.creature import Creature
from desktop_bug.app.inspector_ui import CreatureInspectorDialog
from desktop_bug.state.teams import normalize_teams
from desktop_bug.content.body_plans import resolve_body_plan
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
    model = resolve_body_plan(json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text()))
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
    """The note has to track the overlay, in whichever direction it moved.

    This check used to assert the opposite -- that the note said "no combat" --
    because for a long time marking teams as foes really did nothing but raise
    an alert, and the risk was over-promising a fight. DC-22, DC-45 and DC-47
    reversed that: foes fight, they use their skills, and a loser dies for
    good. The note went on saying "nothing takes damage" the whole time, so the
    assertion that was guarding against over-promising was quietly guarding a
    lie instead. Under-promising a permanent death is the worse of the two.
    """
    _app, _manager, dialog, _first, _second = inspector
    from desktop_bug.state.teams import HOSTILITY_NOTE

    lowered = HOSTILITY_NOTE.lower()
    assert "no combat" not in lowered, HOSTILITY_NOTE
    assert "nothing takes damage" not in lowered, HOSTILITY_NOTE
    assert "fight" in lowered, HOSTILITY_NOTE
    assert "dies" in lowered or "death" in lowered, (
        "a spider now dies permanently; the note has to say so", HOSTILITY_NOTE,
    )
    assert dialog.team_combo.toolTip() == HOSTILITY_NOTE


# -- the redesigned window (the owner: "spaced way too wide ... make nicer
#    interface ... inventory / armor page should also have the bag ... and the
#    spider anatomy and could be equipped") -------------------------------------

def test_the_armour_tab_is_the_anatomy_doll_and_the_bag(inspector):
    from desktop_bug.app.armour_ui import InventoryBag, SpiderDoll

    _app, _manager, dialog, first, _second = inspector
    assert dialog.findChildren(SpiderDoll) and dialog.findChildren(InventoryBag)
    assert dialog.doll.state is first.progression and dialog.bag.tiles.keys() <= set(first.progression.inventory)


def test_the_overview_shows_painted_vitals_and_stat_tiles(inspector):
    _app, _manager, dialog, first, _second = inspector
    assert dialog.health_bar.maximum == pytest.approx(first.max_hp)
    assert dialog.stamina_bar.maximum == pytest.approx(first.max_energy)
    assert set(dialog.tiles) == {"armor", "damage", "points", "worn"}
    assert not dialog.portrait.pixmap().isNull(), "a live portrait of the spider"


def test_the_skill_tree_has_a_picture_on_every_skill(inspector):
    from desktop_bug.state.progression import ABILITY_TREE

    _app, _manager, dialog, _first, _second = inspector
    for node in ABILITY_TREE:
        assert not dialog.tree.buttons[node.id].icon().isNull(), node.id


def test_a_pinned_health_bar_no_longer_brings_the_name_with_it(inspector):
    """The owner: "name is unchecked but it still appears on the spider"."""
    _app, _manager, _dialog, first, _second = inspector
    first.set_health_label_pinned(True)
    first._hovered = False
    assert first.label_visible(False), "the bar still shows"
    assert not first.name_visible(False), "but not the name"
    assert first.name_visible(True), "unless names are switched on"
