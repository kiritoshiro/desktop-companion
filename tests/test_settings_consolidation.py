"""Three scene-wide overrides fold back into the per-slot columns (DC-52).

The owner, looking at the settings window:

* *"as for skills, why does it show 13 abilites if cant select them. only
  show count for how many can be selectable by our design."* The button
  counted the whole `skill_ids` list -- behaviours included, all thirteen --
  beside a dialog offering five checkboxes.
* *"teams also could be indicated just by the color. no need for titles.
  white would be neutral."* The Team column was a dot plus "Hunters",
  "Rivals", "Neutral / solo", taking 116px to say what a 14px dot says.
* *"also moods and movements, and social play should be consolidated in the
  temperaments skills jobs."* Three scene-wide overrides sat on top of the
  per-slot columns that already decide the same things.

The last one is the load-bearing change, because each override had somewhere
real to go rather than simply being deleted:

* Mood was already "Auto - use each personality" by default, and a
  temperament already names its mood.
* Movement was classic/lively/skitter for the whole scene. A temperament
  already names a movement profile, so `personality_gait_style` maps the
  eleven profiles onto the two gaits.
* Social play was a master switch over a `social_play` skill that every
  temperament carries, weighted by its sociability -- 0 for a hunter, 10 for
  a cuddly one. It could only ever make a sociable spider antisocial.

What took their place in that panel is the three "Always show" switches,
which the owner asked for in the same message and which had only ever
existed in the overlay's own menus.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.app.config_ui import (
    COL_ABILITIES,
    COL_CATEGORY,
    COL_TEAM,
    DEFAULT_CATEGORY_ID,
    ConfigWindow,
)
from desktop_bug.content.personality_profiles import (
    GAIT_BY_MOVEMENT,
    MOVEMENT_PROFILES,
    personality_gait_style,
)
from desktop_bug.content.skills import ABILITY_SKILL_IDS, SKILLS
from desktop_bug.manager import CreatureManager

SCREEN = (1400, 900)


@pytest.fixture
def scratch():
    return Path(tempfile.mkdtemp(prefix="dc52-"))


@pytest.fixture
def window(qapp, monkeypatch, scratch):
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    win = ConfigWindow()
    yield win
    win.close()


# ------------------------------------------------------------- the skills count

def test_the_skills_button_counts_what_can_be_selected(window):
    """It said "13 abilities" over a dialog that offers five checkboxes."""
    window.table.setRowCount(0)
    window.add_slot(None, "balanced", 1, False)
    button = window.table.cellWidget(0, COL_ABILITIES)
    assert button.text().endswith(f"of {len(ABILITY_SKILL_IDS)}"), button.text()
    assert "13" not in button.text(), button.text()


def test_the_count_matches_the_dialog_that_opens(window):
    """The number and the checkboxes have to be counting the same things."""
    selectable = [skill for skill in SKILLS if skill.category == "Ability"]
    assert len(selectable) == len(ABILITY_SKILL_IDS)

    window.table.setRowCount(0)
    window.add_slot(None, "balanced", 1, False, abilities=["shoot_web", "web_walk"])
    button = window.table.cellWidget(0, COL_ABILITIES)
    assert button.text() == f"2 of {len(selectable)}", button.text()

    window.table.setRowCount(0)
    window.add_slot(None, "balanced", 1, False, abilities=[])
    assert window.table.cellWidget(0, COL_ABILITIES).text() == f"0 of {len(selectable)}"


def test_the_full_list_is_still_one_hover_away(window):
    window.table.setRowCount(0)
    window.add_slot(None, "balanced", 1, False, abilities=["shoot_web"])
    button = window.table.cellWidget(0, COL_ABILITIES)
    assert "web trap" in button.toolTip()


# ------------------------------------------------------------------- the teams

def test_the_team_column_shows_a_colour_and_no_words(window):
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False, team_id="hunters")
    box = window.table.cellWidget(0, COL_TEAM)
    for index in range(box.count()):
        data = box.itemData(index)
        if data == window.NEW_TEAM_SENTINEL or data is None:
            continue  # the action entry, and the separator
        assert box.itemText(index) == "", (index, box.itemText(index))
        assert not box.itemIcon(index).isNull(), index


def test_the_team_is_still_named_in_words_somewhere(window):
    """A colour-only control has to stay usable by someone who cannot tell
    two of them apart, or who simply has three teams."""
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False, team_id="hunters")
    box = window.table.cellWidget(0, COL_TEAM)
    assert "Hunters" in box.toolTip()
    assert "Hunters" in box.accessibleName()
    assert "Hunters" in box.itemData(box.currentIndex(), 3)  # Qt.ToolTipRole


def test_the_tooltip_follows_the_choice(window):
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False, team_id="hunters")
    box = window.table.cellWidget(0, COL_TEAM)
    box.setCurrentIndex(box.findData("neutral"))
    assert "Neutral" in box.toolTip(), box.toolTip()


def test_neutral_is_white():
    from desktop_bug.state.teams import default_color, distinct_color

    assert default_color("neutral") == (255, 255, 255)
    assert distinct_color("neutral") == (255, 255, 255)


def test_the_team_column_got_narrower():
    """The point of dropping the words. It held 116px for "Neutral / solo"."""
    from desktop_bug.app.config_ui import FIXED_COLUMN_WIDTHS

    assert FIXED_COLUMN_WIDTHS[COL_TEAM] <= 60


# ------------------------------------------------- what the panel offers now

def test_the_three_overrides_are_gone(window):
    for retired in ("mood_combo", "movement_combo", "social_play_check"):
        assert not hasattr(window, retired), retired


def test_the_label_switches_took_their_place(window):
    for check in (window.always_names_check, window.always_levels_check,
                  window.always_health_check):
        assert check.toolTip().strip()
        assert check.isChecked() is False


def test_the_label_switches_round_trip_through_a_preset(window):
    window.always_names_check.setChecked(True)
    window.always_health_check.setChecked(True)
    saved = window.current_settings_data()
    assert saved["always_show_names"] is True
    assert saved["always_show_levels"] is False
    assert saved["always_show_health"] is True

    window.always_names_check.setChecked(False)
    window.apply_settings_to_ui(saved)
    assert window.always_names_check.isChecked() is True


def test_a_save_drops_the_retired_overrides(window):
    """`_loaded_settings` keeps everything this window has no widget for, so
    without dropping them by name an old preset would carry its scene-wide
    mood and gait forward forever and the temperament would never get a
    say."""
    window.apply_settings_to_ui({
        "mood_mode": "playful", "gait_style": "classic", "social_play": False,
        "size_scale": 1.0,
    })
    saved = window.current_settings_data()
    for retired in ("mood_mode", "gait_style", "social_play"):
        assert retired not in saved, retired


def test_a_new_slot_starts_as_a_tarantula(window):
    """"lets focus mainly on the tarantula model from now on." Every other
    kind is still one click away."""
    window.table.setRowCount(0)
    window.add_slot(None, None, 1, False)
    assert window.table.cellWidget(0, COL_CATEGORY).currentData() == DEFAULT_CATEGORY_ID


def test_a_slot_that_names_a_model_still_follows_that_model(window):
    """The default must not overwrite a loaded preset's choice."""
    window.table.setRowCount(0)
    window.add_slot("silk_peacock_jumper", None, 1, False)
    assert window.table.cellWidget(0, COL_CATEGORY).currentData() == "jumper"


# ------------------------------------------------------- the gait, per spider

def test_every_movement_profile_maps_to_a_gait():
    """A profile nobody mapped would silently fall back, so the gait would
    stop following the temperament without anything saying so."""
    assert set(MOVEMENT_PROFILES) == set(GAIT_BY_MOVEMENT)


def test_a_brisk_temperament_and_a_calm_one_walk_differently():
    assert personality_gait_style("bold") == "skitter"
    assert personality_gait_style("hunter") == "skitter"
    assert personality_gait_style("mellow") == "lively"
    assert personality_gait_style("observer") == "lively"


def test_classic_is_not_reachable_from_a_temperament():
    """It is the pre-DC-16 gait, kept only so an old preset still loads."""
    assert "classic" not in set(GAIT_BY_MOVEMENT.values())


def _colony(scratch, monkeypatch, settings=None):
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / f"gait-{len(list(scratch.glob('*.json')))}.json"
    preset.write_text(json.dumps({
        "name": "gait",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1, "slot_id": "a"},
            {"model": "tarantula", "personality": "mellow", "count": 1, "slot_id": "b"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False}, **(settings or {})},
    }), encoding="utf-8")
    return CreatureManager(preset, *SCREEN, seed=5)


def test_two_temperaments_in_one_scene_walk_differently(qapp, scratch, monkeypatch):
    """The whole point of retiring the dropdown: it was one gait for all."""
    manager = _colony(scratch, monkeypatch)
    gaits = {c.personality["id"]: c.gait_style for c in manager.creatures}
    assert gaits == {"bold": "skitter", "mellow": "lively"}, gaits


def test_a_preset_that_names_a_gait_still_wins(qapp, scratch, monkeypatch):
    """An old preset must keep behaving the way its author left it."""
    manager = _colony(scratch, monkeypatch, {"gait_style": "classic"})
    assert {c.gait_style for c in manager.creatures} == {"classic"}


def test_social_play_is_on_by_default_now(qapp, scratch, monkeypatch):
    """It defaulted to False with a checkbox defaulting to True in front of
    it; with the checkbox gone the manager's own default is what runs."""
    manager = _colony(scratch, monkeypatch)
    assert manager.social_play is True
    assert all(c.has_skill("social_play") for c in manager.creatures)


def test_a_preset_can_still_switch_social_play_off(qapp, scratch, monkeypatch):
    manager = _colony(scratch, monkeypatch, {"social_play": False})
    assert manager.social_play is False


def test_the_label_switches_reach_a_launched_colony(qapp, scratch, monkeypatch):
    manager = _colony(scratch, monkeypatch, {
        "always_show_levels": True, "always_show_health": True,
        "always_show_names": True,
    })
    assert manager.always_show_names is True
    assert all(c.level_label_pinned and c.health_label_pinned
               for c in manager.creatures)
