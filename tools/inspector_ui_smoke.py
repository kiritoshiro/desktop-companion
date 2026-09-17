"""Offscreen regression check for stable inspector combo-box popups."""

import json
import os
import random
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtWidgets import QApplication, QWidget

from desktop_bug.creature import Creature
from desktop_bug.engine import CreatureInspectorDialog
from desktop_bug.teams import normalize_teams


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


def main() -> int:
    random.seed(31)
    model = json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text())
    personality = json.loads((ROOT / "personalities" / "cuddly.json").read_text())
    app = QApplication.instance() or QApplication([])
    first = Creature(model, personality, 1200, 800)
    second = Creature(model, personality, 1200, 800, index=1)
    manager = FakeManager([first, second])
    window = FakeWindow(manager)
    dialog = CreatureInspectorDialog(window, first)
    dialog.show()
    app.processEvents()

    entry = dialog._relation_controls[id(second)]
    combo = entry[2]
    combo.showPopup()
    app.processEvents()
    popup_was_visible = combo.view().isVisible()

    dialog.refresh()
    app.processEvents()
    assert dialog._relation_controls[id(second)][2] is combo
    if popup_was_visible:
        assert combo.view().isVisible(), "refresh closed an open relationship popup"

    # The picker offers this scene's teams under their own names, not a fixed
    # list of ids. "Porch guard" means nothing to the code and everything to the
    # person who named it, which is the point of DC-33.
    labels = [dialog.team_combo.itemText(i) for i in range(dialog.team_combo.count())]
    assert labels == ["Neutral / solo", "Hunters", "Porch guard"], labels
    ids = [dialog.team_combo.itemData(i) for i in range(dialog.team_combo.count())]
    assert ids == ["neutral", "hunters", "porch_guard"], ids

    # Choosing by name assigns the id behind it.
    dialog.team_combo.setCurrentIndex(dialog.team_combo.findData("porch_guard"))
    dialog._commit_team()
    app.processEvents()
    assert manager.team_calls[-1][1] == "porch_guard", manager.team_calls
    assert first.progression.team_id == "porch_guard"

    # Typing a team name must not assign every prefix along the way. The
    # editable combo commits on a chosen entry or a finished edit instead.
    before = len(manager.team_calls)
    line_edit = dialog.team_combo.lineEdit()
    for index in range(1, len("hunters") + 1):
        line_edit.setText("hunters"[:index])
        app.processEvents()
    assert len(manager.team_calls) == before, manager.team_calls[before:]
    line_edit.editingFinished.emit()
    app.processEvents()
    assert [team for _creature, team in manager.team_calls[before:]] == ["hunters"], manager.team_calls
    assert first.progression.team_id == "hunters"
    # A finished edit that changes nothing must not rewrite the state file.
    line_edit.editingFinished.emit()
    app.processEvents()
    assert len(manager.team_calls) == before + 1, manager.team_calls

    # Whatever hostility means, the inspector has to say so where a team is
    # chosen, because the words "foe" and "rivals" promise a fight that the
    # overlay cannot have.
    from desktop_bug.teams import HOSTILITY_NOTE

    assert "no combat" in HOSTILITY_NOTE.lower()
    assert dialog.team_combo.toolTip() == HOSTILITY_NOTE

    dialog.close()
    app.processEvents()
    print(
        f"inspector-ui-smoke: stable relationship controls, team commits={len(manager.team_calls)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
