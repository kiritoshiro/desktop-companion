"""The Adventure page lists skirmish missions; closing saves and stops spiders.

The owner:
- *"after selecting the adventure mode, it should list more skirmish
  missions in smaller rectangles and player would choose one. for now only
  one active others make placeholders."*
- *"when exiting the program it gives me choice to close the app without
  closing the spiders ... make it so that after closing it it would
  automatically save and close all the spiders too. without asking."*
"""

from __future__ import annotations

import pytest
from PyQt5.QtWidgets import QLabel, QMessageBox

from desktop_bug.app.mode_menu import SKIRMISH_MISSIONS, ModeShell


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Widgets need the one Qt application."""


def test_missions_are_listed_and_the_maps_open_one_by_one(state_dir):
    from desktop_bug.app.adventure_profile import load_profile, record_result, save_profile
    from desktop_bug.app.campaign import MAPS

    started = []
    shell = ModeShell(QLabel("editor"), lambda: started.append(1))
    assert len(shell.mission_cards) == len(SKIRMISH_MISSIONS) >= 4
    playable = [mid for mid, _, _, ok in SKIRMISH_MISSIONS if ok]
    assert playable == [m.id for m in MAPS]
    # A new player has only the first map open; the rest wait on a win.
    for mission_id, card in shell.mission_cards.items():
        assert card.property("locked") is (mission_id != "territory")
    assert shell.adventure_launch.text() == "Play"
    assert shell.adventure_launch.parent() is shell.mission_cards["territory"]
    shell.adventure_launch.click()
    assert started == [1]
    assert load_profile()["selected_map"] == "territory"
    profile = load_profile()
    record_result(profile, "territory", True, 100.0)
    save_profile(profile)
    shell.refresh_adventure()
    assert shell.mission_cards["ember"].property("locked") is False
    assert shell.mission_cards["obsidian"].property("locked") is True
    shell.map_buttons["ember"].click()
    assert started == [1, 1] and load_profile()["selected_map"] == "ember"


class _Running:
    def poll(self):
        return None


def test_closing_saves_and_stops_the_overlay_without_asking(state_dir, monkeypatch):
    from desktop_bug.app.config_ui import ConfigWindow

    window = ConfigWindow()
    stopped = []
    asked = []
    monkeypatch.setattr(window, "stop_overlay", lambda: stopped.append(1))
    monkeypatch.setattr(QMessageBox, "question", lambda *a, **k: asked.append(1) or QMessageBox.No)
    window.overlay_process = _Running()
    window.close()
    assert stopped == [1], "closing should stop (and so save) the running spiders"
    assert asked == [], "and not ask first"
