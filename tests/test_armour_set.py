"""The Warden set, the anatomy doll and what skills say on hover.

The owner: *"for armor inventory, i want it to look like in those games where a
whole person anatomy is showed and on each part of it some armor type to be
equipped. so create an inventory with a list of items at bottom that could be
dragged and equipped on the anatomical drawing of spider. and create one armor
set that would actually be logical anatomically for tarantula. and when
equipped would be visible in game. so create assets. and what stats they give.
also for skills also write what each skill gives when hovering mouse about it."*
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from PyQt5.QtCore import QMimeData, QPointF, Qt
from PyQt5.QtGui import QColor, QDropEvent, QImage, QPainter

from desktop_bug.app.adventure_profile import (fresh_profile, hero_progression, load_profile,
                                               profile_path, save_profile)
from desktop_bug.app.armour_ui import MIME, SLOT_ORDER, SpiderDoll
from desktop_bug.app.stat_text import item_tooltip, skill_tooltip
from desktop_bug.creature import armour_art
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import (ABILITY_TREE, ARMOR_BY_ID, ARMOR_CATALOG, ARMOR_SETS,
                                           ProgressionState, set_bonus_effects)

WARDEN = ARMOR_SETS["warden"].pieces


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Painting and widgets need the one Qt application."""


def test_the_warden_set_covers_each_part_of_a_tarantula_once():
    slots = [ARMOR_BY_ID[piece].slot for piece in WARDEN]
    assert sorted(slots) == sorted(SLOT_ORDER), "one piece per anatomy slot"
    for piece in WARDEN:
        item = ARMOR_BY_ID[piece]
        assert item.armor > 0 and item.description
        assert armour_art.look_for(piece).style == "plate"
    # Every slot the catalogue uses has a place on the doll.
    assert {item.slot for item in ARMOR_CATALOG} <= set(SLOT_ORDER)


def test_the_set_bonus_needs_all_five_pieces():
    state = ProgressionState(inventory=list(WARDEN))
    for piece in WARDEN[:-1]:
        state.equip(piece)
    assert set_bonus_effects(state) == {}
    state.equip(WARDEN[-1])
    assert set_bonus_effects(state) == ARMOR_SETS["warden"].effects


def test_the_worn_set_changes_the_spiders_stats(state_dir):
    manager = CreatureManager(Path("presets/tarantula.json"), 800, 600, seed=3)
    spider = manager.creatures[0]
    bare_hp, bare_armor = spider.max_hp, spider.armor
    for piece in WARDEN:
        spider.add_inventory_item(piece)
        assert spider.equip_item(piece)[0]
    pieces_hp = sum(ARMOR_BY_ID[p].max_hp for p in WARDEN)
    pieces_armor = sum(ARMOR_BY_ID[p].armor for p in WARDEN)
    bonus = ARMOR_SETS["warden"].effects
    assert spider.max_hp == pytest.approx(bare_hp + pieces_hp + bonus["max_hp"])
    assert spider.armor == pytest.approx(bare_armor + pieces_armor + bonus["armor"])


def own_everything():
    """A saved profile that owns every piece (armour is found as loot now)."""
    profile = fresh_profile()
    profile["armoury"]["owned"] = [item.id for item in ARMOR_CATALOG]
    save_profile(profile)
    return profile


def test_an_old_saves_armour_moves_into_the_armoury(state_dir):
    # A new hero starts with the common silk pieces; the rest is loot.
    fresh = hero_progression(fresh_profile())
    assert not set(WARDEN) & set(fresh.inventory)
    # A version 2 file: the hero's own inventory, no armoury yet.
    profile_path().write_text(json.dumps({
        "version": 2, "name": "Old", "missions": {},
        "progression": {"level": 3, "inventory": ["silk_carapace", *WARDEN],
                        "equipped": {"carapace": "warden_carapace"}}}), encoding="utf-8")
    loaded = load_profile()
    state = hero_progression(loaded)
    assert state.equipped == {"carapace": "warden_carapace"}, "what it wore stays on"
    assert loaded["companions"].keys() == {"scout"} and loaded["selected_map"] == "territory"
    assert "silk_carapace" in state.inventory and set(WARDEN) <= set(state.inventory)


def _render(spider) -> QImage:
    image = QImage(400, 400, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(90, 110, 84))
    painter = QPainter(image)
    painter.translate(200, 200)
    painter.scale(4.0, 4.0)
    painter.translate(-spider.x, -spider.y)
    spider.render(painter)
    painter.end()
    return image


def _differs(a: QImage, b: QImage) -> int:
    return sum(a.pixel(x, y) != b.pixel(x, y)
               for y in range(0, a.height(), 2) for x in range(0, a.width(), 2))


def test_each_warden_piece_is_drawn_on_the_spider_in_game(state_dir):
    manager = CreatureManager(Path("presets/tarantula.json"), 800, 600, seed=3)
    manager.set_flies_enabled(False)
    for setter in (manager.set_always_show_names, manager.set_always_show_levels,
                   manager.set_always_show_health, manager.set_always_show_xp,
                   manager.set_always_show_stamina):
        setter(False)
    spider = manager.creatures[0]
    for _ in range(20):
        manager.update(1 / 60, -1e5, -1e5)
    bare = _render(spider)
    for piece in WARDEN:
        spider.progression.add_item(piece)
        spider.progression.equipped.clear()
        spider.progression.equip(piece)
        assert _differs(bare, _render(spider)) > 40, piece
    spider.progression.equipped.clear()
    assert _differs(bare, _render(spider)) == 0, "taking it off leaves the spider as it was"


def test_every_piece_has_an_inventory_picture():
    for item in ARMOR_CATALOG:
        icon = armour_art.armour_icon(item.id, item.slot, 64)
        opaque = sum(icon.pixelColor(x, y).alpha() > 40 for y in range(0, 64, 2) for x in range(0, 64, 2))
        # A leg icon is one slim diagonal leg, so it covers less than a shell.
        assert opaque > (100 if item.slot == "legs" else 150), item.id


def test_each_part_of_the_doll_is_where_its_callout_points():
    doll = SpiderDoll()
    for slot in SLOT_ORDER:
        assert doll.slot_at(doll.anchors[slot].toPoint()) == slot
        assert doll.slot_at(doll.plaques[slot].center().toPoint()) == slot


def _drop(widget, text):
    mime = QMimeData()
    mime.setData(MIME, text.encode("utf-8"))
    event = QDropEvent(QPointF(300, 220), Qt.MoveAction, mime, Qt.LeftButton, Qt.NoModifier)
    widget.dropEvent(event)


def test_dragging_a_piece_onto_the_doll_wears_it_and_back_to_the_bag_takes_it_off(state_dir):
    from desktop_bug.app.character_ui import CharacterDialog

    own_everything()
    dialog = CharacterDialog()
    assert "warden_tergites" in dialog.bag.tiles
    _drop(dialog.doll, "warden_tergites")
    assert dialog.state.equipped["abdomen"] == "warden_tergites"
    assert "warden_tergites" not in dialog.bag.tiles, "a worn piece leaves the bag"
    assert load_profile()["progression"]["equipped"] == {"abdomen": "warden_tergites"}
    _drop(dialog.bag, "worn:abdomen")
    assert "abdomen" not in dialog.state.equipped
    assert "warden_tergites" in dialog.bag.tiles
    for piece in WARDEN:
        _drop(dialog.doll, piece)
    assert "Warden set 5/5" in dialog.stats_label.text()
    dialog.close()


def test_skill_and_armour_tooltips_say_exactly_what_they_give():
    state = ProgressionState(level=30, skill_points=5)
    for node in ABILITY_TREE:
        tip = skill_tooltip(node, state)
        assert node.name in tip and node.description in tip and "Gives" in tip
        for key, value in node.effects.items():
            if key == "speed":
                assert f"+{value * 100:.0f}% speed" in tip, node.id
            elif key == "web_homing":
                assert "steers" in tip
            else:
                assert f"+{value:g}" in tip, (node.id, key)
    tip = item_tooltip(ARMOR_BY_ID["warden_carapace"], state)
    assert "+2.5 armour" in tip and "+12 max health" in tip and "-2% speed" in tip
    assert "Warden set 0/5" in tip


# -- tiers, more sets, whole-leg armour ----------------------------------------
# The owner: "on the whole leg an armor would be nice. also make some more
# models of armors on various tier and quality material, looking epic some."

def _set_total(set_id, key="armor"):
    return sum(getattr(ARMOR_BY_ID[p], key) for p in ARMOR_SETS[set_id].pieces)


def test_every_set_is_a_full_suit_with_its_own_material_and_tier():
    from desktop_bug.state.progression import ARMOR_TIERS

    looks = set()
    for set_id, armor_set in ARMOR_SETS.items():
        pieces = [ARMOR_BY_ID[p] for p in armor_set.pieces]
        assert sorted(p.slot for p in pieces) == sorted(SLOT_ORDER), set_id
        assert len({p.tier for p in pieces}) == 1, "a set is one quality"
        assert pieces[0].tier in ARMOR_TIERS
        looks.add(armour_art.look_for(pieces[0].id))
    assert len(looks) == len(ARMOR_SETS), "each set has its own material"
    tiers = {ARMOR_BY_ID[s.pieces[0]].tier for s in ARMOR_SETS.values()}
    assert {"uncommon", "rare", "epic", "legendary"} <= tiers


def test_better_tiers_protect_more():
    assert _set_total("forager") < _set_total("warden") < _set_total("sun")
    assert _set_total("warden") <= _set_total("brood") < _set_total("sun")


def test_leg_armour_runs_down_the_whole_leg_but_leaves_the_claw_bare():
    points = [(20, 100), (80, 100), (110, 100), (170, 100), (230, 100), (270, 100)]
    widths = [10, 9, 8, 6, 5]
    image = QImage(300, 200, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    armour_art.paint_leg_armour(painter, points, widths, armour_art.look_for("warden_greaves"))
    painter.end()

    def covered(x):
        return any(image.pixelColor(x, y).alpha() > 100 for y in range(90, 111))

    for x in (55, 95, 140, 200):   # femur, knee, tibia, metatarsus
        assert covered(x), x
    assert not covered(255), "the tarsus and its claws stay bare"


def test_the_bag_puts_the_best_first(state_dir):
    from desktop_bug.app.character_ui import CharacterDialog
    from desktop_bug.state.progression import ARMOR_TIERS

    own_everything()
    dialog = CharacterDialog()
    order = [ARMOR_TIERS.index(ARMOR_BY_ID[i].tier) for i in dialog.bag.tiles]
    assert order == sorted(order, reverse=True)
    assert dialog.bag.tiles["sun_crown"].property("tier") == "legendary"
    assert "Legendary" in item_tooltip(ARMOR_BY_ID["sun_crown"])
    dialog.close()
