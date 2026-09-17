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


class FakeManager:
    def __init__(self, creatures):
        self.creatures = creatures
        self.team_calls = []

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

    # Both team pickers must offer the same ids.
    from desktop_bug.config_ui import TEAM_OPTIONS as SETTINGS_TEAMS

    dialog_teams = [dialog.team_combo.itemText(i) for i in range(dialog.team_combo.count())]
    assert dialog_teams == [value for _label, value in SETTINGS_TEAMS], dialog_teams

    dialog.close()
    app.processEvents()
    print(
        f"inspector-ui-smoke: stable relationship controls, team commits={len(manager.team_calls)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
