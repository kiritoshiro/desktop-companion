"""The map editor, custom maps in play, admin mode, and screens in use.

The owner: *"create editor tool. so i could create my self the map. with
spiders/bosses buildings and the settings in the buildings and spider
enemies armor and everything else. also add admin mode. so i could also
reset my own progress or enter all the maps too."* And: *"sometimes a second
screen is connected however is not active ... only when the second/or other
screens are active only then populate them."*
"""
from __future__ import annotations

import random
from pathlib import Path
from types import SimpleNamespace

import pytest

from desktop_bug.app import admin_ui, custom_maps as cm
from desktop_bug.app.adventure_profile import fresh_profile, load_profile, record_result, save_profile
from desktop_bug.app.campaign import MAPS, chosen_map, map_unlocked
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.app.mission_factory import create_mission
from desktop_bug.app.screen_activity import playable_screens
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import ARMOR_SETS, equipped_items
from desktop_bug.world.playfield import ScreenRect


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Widgets and creatures need the one Qt application."""


SIDE = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1600, 0, 1600, 1000)]


def my_map():
    data = cm.new_map("Spider fort")
    data["tier"] = 2
    data["cap"] = 16
    data["wave_every"] = 10.0
    food = next(b for b in data["buildings"] if b["kind"] == "food")
    food["supply"] = 999.0
    food["guards"] = [cm.spider_spec("weaver", kind="thorn_weaver", level_bonus=3, hp=2.0,
                                     armor=["frost_aegis"], item_level=4)]
    data["buildings"].append({"kind": "venom", "name": "Poison well", "fx": 0.3, "fy": 0.4, "far": True,
                              "owned": False, "reserves": 0, "supply": 180.0,
                              "guards": [cm.spider_spec("spitter")]})
    extra = cm.spider_spec("hunter", kind="ash_wolf")
    extra.update({"fx": 0.7, "fy": 0.8, "far": False})
    data["enemies"].append(extra)
    data["boss"] = cm.spider_spec("guardian", name="Fort lord", kind="thorn_crown",
                                  armor=list(ARMOR_SETS["sun"].pieces), item_level=5, hp=1.5)
    return data


def play(map_id, rects=SIDE):
    profile = load_profile()
    profile["selected_map"] = map_id
    save_profile(profile)
    m = create_mission(CreatureManager(Path("presets/colony.json"), 3200, 1000, seed=4),
                       ControlSettings(), rects, 0)
    m.rng = random.Random(1)
    return m


# -- custom maps ---------------------------------------------------------------

def test_a_new_map_is_playable_and_saves_and_loads_back(state_dir):
    data = cm.new_map()
    assert cm.problems(data) == []
    map_id = cm.save_map(my_map())
    assert map_id.startswith("custom-spider-fort")
    again = cm.load_map(map_id)
    assert again["title"] == "Spider fort" and again["cap"] == 16
    assert again["boss"]["name"] == "Fort lord" and again["boss"]["kind"] == "thorn_crown"
    assert [m["id"] for m in cm.list_maps()] == [map_id]
    assert cm.save_map(my_map()) != map_id, "a second map of the same title gets its own file"


def test_a_map_without_its_essentials_says_why():
    data = cm.new_map()
    data["buildings"] = [b for b in data["buildings"] if b["kind"] != "nest"]
    data["buildings"].append(dict(data["buildings"][0]))      # a second home
    found = " ".join(cm.problems(data))
    assert "Enemy nest" in found and "one Home burrow" in found


def test_bad_values_in_a_file_are_cleaned():
    data = cm.clean_map({"title": "", "tier": 99, "cap": -3,
                         "buildings": [{"kind": "castle"}, {"kind": "food", "fx": 5, "guards": [
                             {"role": "wizard", "kind": "dragon", "armor": ["nope", "frost_aegis", "frost_aegis"]}]}]})
    assert data["title"] == "My map" and data["tier"] == 6 and data["cap"] == 4
    assert [b["kind"] for b in data["buildings"]] == ["food"]
    guard = data["buildings"][0]["guards"][0]
    assert guard["role"] == "guard" and guard["kind"] == "auto" and guard["armor"] == ["frost_aegis"]
    assert data["buildings"][0]["fx"] == 1.0


# -- a custom map in play --------------------------------------------------------

def test_a_custom_map_plays_with_its_own_buildings_settings_and_spiders(state_dir):
    save_profile(fresh_profile())
    map_id = cm.save_map(my_map())
    assert map_unlocked({}, map_id), "your own maps are always open"
    m = play(map_id)
    assert type(m) is TerritoryMission and m.map_info.id == map_id and m.map_info.title == "Spider fort"
    assert m.CAP == 16 and m.WAVE_EVERY == 10.0
    well = m._site("venom")
    assert well.name == "Poison well" and well.screen == 1, "placed on the second screen"
    assert m._site("food").supply == 999.0
    weaver = next(c for c in m.manager.creatures if c.enemy_kind == "thorn_weaver")
    assert [i.id for i in equipped_items(weaver.progression)] == ["frost_aegis"]
    assert weaver.progression.item_levels["frost_aegis"] == 4
    assert weaver.level >= m.hero.level + 1 + 3
    assert any(c.enemy_kind == "ash_wolf" for c in m.manager.creatures), "the lone enemy is there"
    boss = m._spawn("guardian", (m.nest.x, m.nest.y))
    assert boss.display_name == "Fort lord" and boss.enemy_kind == "thorn_crown"
    assert sorted(i.id for i in equipped_items(boss.progression)) == sorted(ARMOR_SETS["sun"].pieces)


def test_with_one_screen_the_far_buildings_stand_on_the_main_one(state_dir):
    map_id = cm.save_map(my_map())
    m = play(map_id, rects=SIDE[:1])
    assert m._site("venom").screen == 0 and SIDE[0].contains(m._site("venom").x, m._site("venom").y)


def test_a_deleted_map_falls_back_to_the_first(state_dir):
    map_id = cm.save_map(my_map())
    profile = load_profile()
    profile["selected_map"] = map_id
    save_profile(profile)
    assert cm.delete_map(map_id)
    assert chosen_map(load_profile()).id == MAPS[0].id


# -- the editor ------------------------------------------------------------------

def test_the_editor_builds_a_map_edits_it_and_saves_it(state_dir):
    from desktop_bug.app.map_editor import MapEditor

    played = []
    editor = MapEditor(play=played.append)
    editor.title.setText("Web city")
    editor.title.textEdited.emit("Web city")
    editor.tier.setValue(3)
    editor.new_kind.setCurrentIndex(editor.new_kind.findData("nursery"))
    editor.add_building()
    b = editor.data["buildings"][-1]
    assert b["kind"] == "nursery" and editor.selected == ("building", len(editor.data["buildings"]) - 1)
    editor.b_far.setChecked(True)
    editor.b_name.setText("Brood hall")
    editor.b_name.textEdited.emit("Brood hall")
    editor.add_guard()
    guard_editor = editor.guard_editor
    guard_editor.kind.setCurrentIndex(guard_editor.kind.findData("crab_reaver"))
    guard_editor.auto_armour.setChecked(False)
    slot = next(iter(guard_editor.slots))
    guard_editor.slots[slot].setCurrentIndex(1)
    assert b["far"] and b["name"] == "Brood hall" and len(b["guards"]) == 2
    assert b["guards"][1]["kind"] == "crab_reaver" and isinstance(b["guards"][1]["armor"], list)
    editor.add_enemy()
    editor.boss_name.setText("Web tyrant")
    editor.boss_name.textEdited.emit("Web tyrant")
    assert editor.play_button.isEnabled()
    editor.play()
    assert played and played[0].startswith("custom-web-city")
    saved = cm.load_map(played[0])
    assert saved["tier"] == 3 and saved["boss"]["name"] == "Web tyrant" and len(saved["enemies"]) == 1
    assert any(x["name"] == "Brood hall" and x["far"] for x in saved["buildings"])


def test_the_editor_will_not_play_a_broken_map(state_dir):
    from desktop_bug.app.map_editor import MapEditor

    editor = MapEditor(play=lambda mid: pytest.fail("played a broken map"))
    nest = next(i for i, b in enumerate(editor.data["buildings"]) if b["kind"] == "nest")
    editor.select("building", nest)
    editor.remove_building()
    assert not editor.play_button.isEnabled() and "Enemy nest" in editor.problems.text()
    editor.play()


def test_dragging_on_the_canvas_moves_a_building_to_the_other_screen(state_dir):
    from PyQt5.QtCore import QPointF

    from desktop_bug.app.map_editor import MapEditor

    editor = MapEditor()
    editor.canvas.resize(800, 360)
    main, far = editor.canvas.screens()
    fx, fy, on_far = editor.canvas.from_canvas(QPointF(far.center()))
    assert on_far and abs(fx - 0.5) < 0.02
    b = editor.data["buildings"][1]
    at = editor.canvas.to_canvas(b["fx"], b["fy"], b["far"])
    assert editor.canvas.item_at(at) == ("building", 1)


def test_the_adventure_page_lists_your_maps_and_opens_the_tools(state_dir):
    from PyQt5.QtWidgets import QLabel

    from desktop_bug.app.mode_menu import ModeShell

    save_profile(fresh_profile())
    map_id = cm.save_map(my_map())
    started = []
    shell = ModeShell(QLabel("editor"), lambda: started.append(1))
    shell.refresh_adventure()
    assert map_id in shell.custom_cards
    assert shell.editor_button.text().startswith("Map editor") and shell.admin_button.text().startswith("Admin")
    shell.play_map(map_id, shell._start_adventure)
    assert started == [1] and load_profile()["selected_map"] == map_id


# -- admin mode ------------------------------------------------------------------

def test_admin_mode_opens_every_map(state_dir):
    save_profile(fresh_profile())
    assert not map_unlocked({}, "queen")
    admin_ui.set_admin(True)
    profile = load_profile()
    assert profile["admin"] and map_unlocked({}, "queen", True)
    profile["selected_map"] = "queen"
    save_profile(profile)
    assert chosen_map(load_profile()).id == "queen"


def test_reset_progress_starts_again_but_keeps_the_settings(state_dir):
    profile = fresh_profile()
    record_result(profile, "territory", True, 90.0)
    profile["admin"] = True
    profile["disabled_screens"] = ["\\\\.\\DISPLAY2"]
    profile["armoury"]["amber"] = 500
    save_profile(profile)
    admin_ui.reset_progress()
    after = load_profile()
    assert after["missions"] == {} and after["armoury"]["amber"] == 0
    assert after["admin"] and after["disabled_screens"] == ["\\\\.\\DISPLAY2"]


def test_the_admin_gifts(state_dir):
    save_profile(fresh_profile())
    assert admin_ui.give_amber(1000) == 1000
    assert admin_ui.give_all_armour() > 20
    assert admin_ui.unlock_all_companions() >= 3
    assert admin_ui.set_hero_level(12) == 12
    from desktop_bug.app.adventure_profile import hero_progression

    state = hero_progression(load_profile())
    assert state.level == 12 and state.skill_points >= 11


def test_the_admin_window(state_dir):
    save_profile(fresh_profile())
    dialog = admin_ui.AdminDialog()
    dialog.admin.setChecked(True)
    assert load_profile()["admin"]
    dialog.buttons["Give 1000 amber"].click()
    assert "1000" in dialog.status.text()
    dialog._reset(confirm=False)
    assert load_profile()["armoury"]["amber"] == 0 and load_profile()["admin"]


# -- screens in use ----------------------------------------------------------------

def screen(name):
    return SimpleNamespace(name=lambda: name)


def test_only_screens_in_use_are_populated():
    main, second, third = screen("\\\\.\\DISPLAY1"), screen("\\\\.\\DISPLAY2"), screen("\\\\.\\DISPLAY3")
    everything = [main, second, third]
    assert playable_screens(everything, main, states={}) == everything
    asleep = {"\\\\.\\DISPLAY2": "asleep"}
    assert playable_screens(everything, main, states=asleep) == [main, third]
    assert playable_screens(everything, main, disabled=["\\\\.\\DISPLAY3"], states={}) == [main, second]
    assert playable_screens(everything, main, disabled=["\\\\.\\DISPLAY1"],
                            states={"\\\\.\\DISPLAY1": "off"}) == [main, second, third], "the main screen always"
