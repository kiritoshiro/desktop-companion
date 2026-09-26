"""Adventure enemies are the enemy kinds of their map; Companion never sees them.

The owner asked for "more skins colors for enemies, and the way they look
like, maybe even custom looking not just like the tarantula"; the kinds are
content/enemy_kinds (PR #95), the missions choose among them here.
"""
from __future__ import annotations

import random
from pathlib import Path

import pytest

from desktop_bug.app.adventure_profile import fresh_profile, record_result, save_profile
from desktop_bug.app.campaign import MAPS
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.content.enemy_kinds import ENEMY_KINDS, is_enemy_model
from desktop_bug.manager import CreatureManager


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures need the one Qt application."""


def mission_on(map_id):
    profile = fresh_profile()
    for info in MAPS:
        if info.id == map_id:
            break
        record_result(profile, info.id, True, 100.0)
    profile["selected_map"] = map_id
    save_profile(profile)
    m = TerritoryMission(CreatureManager(Path("presets/tarantula.json"), 1600, 1000, seed=4),
                         ControlSettings())
    m.rng = random.Random(9)
    return m


@pytest.mark.parametrize("info", MAPS, ids=lambda i: i.id)
def test_each_map_fields_its_own_enemy_kinds_and_a_bigger_boss(state_dir, info):
    m = mission_on(info.id)
    for _ in range(12):
        c = m._spawn("guard", (900, 500))
        if c is None:
            break
        kind = ENEMY_KINDS[c.enemy_kind]
        assert not kind.boss and kind.tier <= info.tier
        assert c.display_name == kind.name and c.model["id"] == kind.model_id
    boss_spider = m._spawn("guardian", (900, 300))
    if boss_spider is None:        # the arena is full; make room and retry
        for c in list(m.manager.creatures):
            if m.hero.relation_to(c) == "foe":
                c.take_damage(10 ** 6, m.hero)
        m.manager._bury_the_dead()
        boss_spider = m._spawn("guardian", (900, 300))
    boss = ENEMY_KINDS[boss_spider.enemy_kind]
    assert boss.boss and boss_spider.display_name == info.guardian
    assert boss_spider.size > m.hero.size * 1.4, "a boss reads as a boss"
    assert len(boss_spider.progression.equipped) == 5, "in its full set"


def test_skins_vary_between_spawns(state_dir):
    m = mission_on("territory")
    skins = set()
    for _ in range(9):
        c = m._spawn("hunter", (900, 500))
        if c is None:
            break
        skins.add((c.enemy_kind, tuple(c.colors["body"])))
        c.take_damage(10 ** 6, m.hero)
        m.manager._bury_the_dead()
    assert len(skins) >= 3


def test_a_random_companion_spider_is_never_an_enemy_kind(state_dir):
    manager = CreatureManager(Path("presets/colony.json"), 800, 600, seed=1)
    assert any(is_enemy_model(mid) for mid in manager.models), "the enemies are discovered"
    picks = {manager._random_model_id() for _ in range(400)}
    assert picks and not any(is_enemy_model(mid) for mid in picks)


def test_the_companion_skin_list_leaves_out_enemies(state_dir, qapp):
    from desktop_bug.app.config_ui import ConfigWindow

    window = ConfigWindow()
    try:
        window.add_slot("spider", None, 1, False)
        row = window.table.rowCount() - 1
        category_box = window.table.cellWidget(row, 0)
        plans = [category_box.itemData(i) for i in range(category_box.count())]
        assert "enemy_crab" not in plans and "enemy_orb" not in plans
        model_ids = []
        for column in range(window.table.columnCount()):
            box = window.table.cellWidget(row, column)
            if hasattr(box, "itemData") and box is not category_box:
                model_ids = [box.itemData(i) for i in range(box.count())]
                if "spider" in model_ids:
                    break
        assert "spider" in model_ids
        assert not any(is_enemy_model(mid) for mid in model_ids)
    finally:
        window.close()
