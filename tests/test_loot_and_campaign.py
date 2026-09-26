"""Enemies wear armour, drop it; stacking, selling; companions; maps and bosses.

The owner: *"i want the enemies to wear the armor. and on other maps better
armor that way user could get new armor dropped from killing enemies. also he
could stack these armor to upgrade them for 5 levels. any other duplicates
could be sold and at some point a shop could be made ... also ability to put
armor on companion/s. and more companions could be obtained from further maps.
also will need bosses who will wear legendary armor."*
"""
from __future__ import annotations

import random
from pathlib import Path

import pytest

from desktop_bug.app import armoury
from desktop_bug.app.adventure_profile import (HERO, companion_progression, fresh_profile,
                                               hero_progression, load_profile, record_result,
                                               save_profile, unlock_companion)
from desktop_bug.app.campaign import MAP_BY_ID, MAPS, SELL_PRICES, enemy_loadout
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission import LOOT_REACH, TerritoryMission
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import (ARMOR_BY_ID, ARMOR_SETS, MAX_ITEM_LEVEL, ProgressionState,
                                           equipped_items, level_multiplier)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and widgets need the one Qt application."""


def mission_on(map_id="territory", won=(), profile=None):
    profile = profile or fresh_profile()
    for earlier in won:
        record_result(profile, earlier, True, 100.0)
    profile["selected_map"] = map_id
    save_profile(profile)
    manager = CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4)
    m = TerritoryMission(manager, ControlSettings())
    m.rng = random.Random(3)
    return m


def enemies(m):
    return [a for a in m.actors if a.role != "ally"]


# -- enemies wear the map's armour --------------------------------------------

def test_enemies_wear_armour_of_their_maps_quality():
    rng = random.Random(1)
    for info in MAPS:
        for _ in range(20):
            worn, level = enemy_loadout(info, "guard", rng)
            assert len({ARMOR_BY_ID[i].slot for i in worn}) == len(worn), "one piece per slot"
            assert all(ARMOR_BY_ID[i].tier in info.enemy_tiers for i in worn)
            assert 1 <= level <= MAX_ITEM_LEVEL
    first, last = MAPS[0], MAPS[-1]
    assert max(len(enemy_loadout(first, "guard", rng)[0]) for _ in range(30)) <= 2
    assert min(len(enemy_loadout(last, "guard", rng)[0]) for _ in range(30)) >= 3


def test_the_last_boss_wears_legendary_armour_and_is_named_for_its_map(state_dir):
    m = mission_on("queen", won=("territory", "ember", "obsidian"))
    assert m.map_info.id == "queen"
    boss = m._spawn("guardian", (m.sites[4].x, m.sites[4].y))
    worn = [item.id for item in equipped_items(boss.progression)]
    assert sorted(worn) == sorted(ARMOR_SETS["sun"].pieces)
    assert all(ARMOR_BY_ID[i].tier == "legendary" for i in worn)
    assert boss.display_name == "Queen of thorns"


def test_a_locked_map_cannot_be_played_yet(state_dir):
    m = mission_on("queen")
    assert m.map_info.id == "territory", "falls back to the first map"


# -- loot ---------------------------------------------------------------------

def test_a_killed_enemy_drops_armour_you_pick_up_by_walking_over_it(state_dir):
    m = mission_on("ember", won=("territory",))
    guard = next(a for a in enemies(m) if a.creature.mission_loot)
    c = guard.creature
    worn = list(c.mission_loot)
    m.rng = random.Random()
    m.rng.random = lambda: 0.0          # this one always drops
    c.take_damage(100000, m.hero)
    m.update(1 / 60)
    assert len(m.loot) == 1 and m.loot[0].item_id in worn
    loot = m.loot[0]
    m.hero.x, m.hero.y = loot.x + LOOT_REACH * 2, loot.y
    m.update(1 / 60)
    assert m.loot, "not picked up from afar"
    m.hero.x, m.hero.y = loot.x, loot.y
    m._collect_loot(1 / 60)
    assert not m.loot and m.found == [(loot.item_id, "new")]
    assert loot.item_id in m.profile["armoury"]["owned"]
    m.save_progress()
    assert loot.item_id in load_profile()["armoury"]["owned"]


def test_a_guardian_always_leaves_two_pieces(state_dir):
    m = mission_on()
    boss = m._spawn("guardian", (m.sites[4].x, m.sites[4].y))
    m.guardian = boss
    boss.take_damage(100000, m.hero)
    m._drop_loot()
    assert len(m.loot) == 2
    assert {loot.item_id for loot in m.loot} <= set(ARMOR_SETS["forager"].pieces)


# -- stacking, selling ----------------------------------------------------------

def test_duplicates_stack_to_upgrade_up_to_five_levels():
    profile = fresh_profile()
    assert armoury.add_loot(profile, "warden_carapace") == "new"
    for _ in range(6):
        assert armoury.add_loot(profile, "warden_carapace") == "spare"
    for level in range(2, MAX_ITEM_LEVEL + 1):
        assert armoury.upgrade(profile, "warden_carapace")
        assert armoury.level_of(profile, "warden_carapace") == level
    assert not armoury.upgrade(profile, "warden_carapace"), "five is the most"
    assert armoury.spares_of(profile, "warden_carapace") == 2
    state = hero_progression(profile)
    state.equip("warden_carapace")
    worn = next(equipped_items(state))
    base = ARMOR_BY_ID["warden_carapace"]
    assert worn.armor == pytest.approx(base.armor * level_multiplier(5))
    assert level_multiplier(5) == pytest.approx(1.8)


def test_spares_sell_for_amber_and_the_piece_is_kept():
    profile = fresh_profile()
    armoury.add_loot(profile, "sun_crown")
    assert armoury.sell_spares(profile, "sun_crown") == 0, "the piece itself is not for sale"
    armoury.add_loot(profile, "sun_crown")
    armoury.add_loot(profile, "sun_crown")
    assert armoury.sell_spares(profile, "sun_crown", 99) == 2 * SELL_PRICES["legendary"]
    assert profile["armoury"]["amber"] == 2 * SELL_PRICES["legendary"]
    assert "sun_crown" in profile["armoury"]["owned"]


# -- companions -----------------------------------------------------------------

def test_armour_goes_on_one_spider_at_a_time_companions_included():
    profile = fresh_profile()
    armoury.add_loot(profile, "warden_tergites")
    assert armoury.wear(profile, HERO, "warden_tergites")
    assert armoury.wear(profile, "scout", "warden_tergites")
    assert "warden_tergites" not in hero_progression(profile).equipped.values()
    assert companion_progression(profile, "scout").equipped == {"abdomen": "warden_tergites"}
    assert armoury.worn_by(profile) == {"warden_tergites": "scout"}


def test_a_companion_wears_its_armour_in_the_mission_and_keeps_its_xp(state_dir):
    profile = fresh_profile()
    armoury.add_loot(profile, "warden_tergites")
    armoury.wear(profile, "scout", "warden_tergites")
    m = mission_on(profile=profile)
    assert [i.id for i in equipped_items(m.ally.progression)] == ["warden_tergites"]
    m.ally.gain_experience(500, "test")
    level = m.ally.level
    m.save_progress()
    assert companion_progression(load_profile(), "scout").level == level > 1


def test_winning_a_map_the_first_time_brings_a_new_companion(state_dir):
    m = mission_on()
    m.state = "victory"
    m.finish(won=True)
    profile = load_profile()
    assert "weaver" in profile["companions"]
    assert m.reward_text == "Silk weaver joins you"
    m2 = mission_on(profile=profile)
    styles = sorted(a.style for a in m2.actors if a.role == "ally")
    assert styles == ["scout", "weaver"]
    m2.state = "victory"
    m2.finish(won=True)
    assert m2.reward_text == "", "only the first win brings one"


def test_the_party_is_at_most_three_companions(state_dir):
    profile = fresh_profile()
    for cid in ("weaver", "hunter", "sentinel"):
        unlock_companion(profile, cid)
    m = mission_on(profile=profile)
    assert len(m.allies) == 3
    assert len({(round(a.x), round(a.y)) for a in m.allies}) == 3


# -- the character window --------------------------------------------------------

def test_the_character_window_dresses_companions_and_upgrades_and_sells(state_dir):
    from desktop_bug.app.character_ui import CharacterDialog

    profile = fresh_profile()
    unlock_companion(profile, "weaver")
    for _ in range(3):
        armoury.add_loot(profile, "frost_aegis")
    save_profile(profile)
    dialog = CharacterDialog()
    assert list(dialog.who_buttons) == [HERO, "scout", "weaver"]
    dialog.choose_spider("weaver")
    dialog._equip("frost_aegis")
    assert companion_progression(load_profile(), "weaver").equipped == {"carapace": "frost_aegis"}
    assert "frost_aegis" not in dialog.bag.tiles, "worn by the weaver, so out of the bag"
    dialog._select("frost_aegis")
    assert dialog.item_panel.isVisibleTo(dialog) and dialog.upgrade_button.isEnabled()
    dialog._upgrade()
    assert armoury.level_of(load_profile(), "frost_aegis") == 2
    dialog._sell(1)
    saved = load_profile()["armoury"]
    assert saved["amber"] == SELL_PRICES["epic"] and "frost_aegis" not in saved["spares"]
    assert "Level 2" in dialog.item_label.text()
    dialog.close()


def test_every_map_names_a_boss_set_and_later_maps_are_tougher():
    tiers = [m.tier for m in MAPS]
    assert tiers == sorted(tiers)
    assert MAP_BY_ID["queen"].guardian_set == "sun"
    for m in MAPS:
        assert m.guardian_set in ARMOR_SETS
    assert ProgressionState().inventory == ["silk_carapace", "leg_guard_set"]
