"""The Adventure hero: named, starting at level 1, keeping its progress.

The owner: *"after the victory or defeat should put a bigger title on the
middle of screen whether victory or not. and then should navigate to the
adventure mode again. and check that the mission is victorious. also on
adventure mode should allow to name the spider. and the spider should begin
with the first level. and as progress keep it level xp and skills chosen
saved. also in adventure window create the character whole skill tree, and
armor, character name."*
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from PyQt5.QtWidgets import QLabel

from desktop_bug.app.adventure_profile import (DEFAULT_HERO_NAME, clean_name, fresh_profile,
                                               load_profile, mission_record, profile_path,
                                               record_result, save_profile)
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.manager import CreatureManager


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and widgets need the one Qt application."""


def _mission():
    manager = CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4)
    return TerritoryMission(manager, ControlSettings())


def test_the_profile_round_trips_and_reads_the_first_format(state_dir):
    profile = fresh_profile()
    profile["name"] = "  Crimson   Knee "
    profile["progression"] = {"level": 4, "xp": 12, "skill_points": 1}
    record_result(profile, "territory", True, 200.0)
    record_result(profile, "territory", False, 50.0)
    assert save_profile(profile)
    loaded = load_profile()
    assert loaded["name"] == "Crimson Knee"
    assert loaded["progression"]["level"] == 4
    assert mission_record(loaded, "territory") == {"victories": 1, "defeats": 1, "best_seconds": 200.0}
    profile_path().write_text(json.dumps({"version": 1, "progression": {"level": 3}}), encoding="utf-8")
    old = load_profile()
    assert old["name"] == DEFAULT_HERO_NAME and old["progression"]["level"] == 3
    assert clean_name("") == DEFAULT_HERO_NAME and len(clean_name("x" * 99)) == 24


def test_a_new_hero_starts_at_level_one_under_its_name(state_dir):
    profile = fresh_profile()
    profile["name"] = "Crimson Knee"
    save_profile(profile)
    m = _mission()
    assert m.hero.level == 1 and m.hero.progression.xp == 0
    assert m.hero.display_name == "Crimson Knee"
    assert m.hero.chooses_own_skills


def test_the_hero_banks_its_skill_points_for_the_player(state_dir):
    m = _mission()
    m.hero.gain_experience(2000, "test")
    assert m.hero.level > 3
    assert m.hero.progression.skill_points == m.hero.level - 1
    assert m.hero.progression.unlocked_abilities == []


def test_victory_is_recorded_and_the_end_screen_counts_down(state_dir):
    m = _mission()
    m.hero.gain_experience(150, "test")
    m.state = "victory"
    m.finish(won=True)
    profile = load_profile()
    assert mission_record(profile, "territory")["victories"] == 1
    assert profile["progression"]["total_xp"] == m.hero.progression.total_xp
    assert not m.end_screen_done
    m.update(m.END_SCREEN_SECONDS / 2)
    assert not m.end_screen_done
    m.update(m.END_SCREEN_SECONDS)
    assert m.end_screen_done
    m.finish(won=True)
    assert mission_record(load_profile(), "territory")["victories"] == 1, "recorded once"


def test_the_overlay_closes_after_the_end_screen(state_dir, monkeypatch):
    from desktop_bug.app.engine import OverlayWindow

    window = OverlayWindow(Path("presets/colony.json"), seed=4, mode="adventure")
    stops = []
    try:
        window.timer.stop()
        monkeypatch.setattr(window, "_do_graceful_stop", lambda: stops.append(1))
        window.mission.hero.take_damage(100000)
        window.tick()
        assert window.mission.state == "defeat" and not stops
        window.mission.end_clock = window.mission.END_SCREEN_SECONDS
        window.tick()
        assert stops, "the overlay should close so the Adventure page comes back"
        assert mission_record(load_profile(), "territory")["defeats"] == 1
    finally:
        window.style_timer.stop()
        window.deleteLater()


def test_the_character_window_names_levels_and_equips(state_dir):
    from desktop_bug.app.character_ui import CharacterDialog

    profile = fresh_profile()
    profile["progression"] = {"level": 5, "skill_points": 2}
    save_profile(profile)
    dialog = CharacterDialog()
    dialog.name_edit.setText("Night Weaver")
    dialog._rename()
    assert dialog.tree.buttons["vitality"].property("state") == "available"
    assert dialog.tree.buttons["apex_predator"].property("state") == "locked"
    dialog.tree.buttons["vitality"].click()
    dialog._equip("silk_carapace")
    saved = load_profile()
    assert saved["name"] == "Night Weaver"
    assert saved["progression"]["unlocked_abilities"] == ["vitality"]
    assert saved["progression"]["skill_points"] == 1
    assert saved["progression"]["equipped"] == {"carapace": "silk_carapace"}
    assert dialog.tree.buttons["vitality"].property("state") == "unlocked"
    dialog.close()


def test_the_adventure_page_shows_the_hero_and_the_win(state_dir):
    from desktop_bug.app.mode_menu import ModeShell

    profile = fresh_profile()
    profile["name"] = "Crimson Knee"
    profile["progression"] = {"level": 6, "skill_points": 2}
    save_profile(profile)
    shell = ModeShell(QLabel("editor"), lambda: None)
    assert "Crimson Knee" in shell.hero_label.text() and "Level 6" in shell.hero_label.text()
    assert shell.mission_status["territory"].text() == ""
    record_result(profile, "territory", True, 125.0)
    save_profile(profile)
    shell.refresh_adventure()
    assert "Won" in shell.mission_status["territory"].text()
    assert "2:05" in shell.mission_status["territory"].text()
