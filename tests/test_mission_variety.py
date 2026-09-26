"""Maps with their own buildings and places, new buildings, armour by map,
fly levels, and missions that use every screen.

The owner: *"missions are all the same. make different configuration of the
buildings. and in different places. also in first levels no armor on enemy
... only in higher maps should they wear better armor. and bosses wear best
armor. also add levels with flies. and also utilise multiple screens for
missions. and build some other buildings that would have some other functions
too."*
"""
from __future__ import annotations

import random
from pathlib import Path
from types import SimpleNamespace

import pytest
from PyQt5.QtGui import QImage, QPainter

from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.adventure_profile import fresh_profile, load_profile, record_result, save_profile
from desktop_bug.app.campaign import MAP_BY_ID, MAPS, enemy_loadout
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.map_layouts import LAYOUTS
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.app.mission_factory import create_mission
from desktop_bug.app.swarm import RULES, FlySwarmMission
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import ARMOR_SETS, MAX_ITEM_LEVEL
from desktop_bug.world.playfield import ScreenRect
from desktop_bug.world.screen_layout import ScreenLayout


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures need the one Qt application."""


SIDE = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1600, 0, 1600, 1000)]


def mission(map_id, rects=SIDE):
    profile = fresh_profile()
    for earlier in ("territory", "ember", "obsidian"):
        record_result(profile, earlier, True, 100.0)
    profile["selected_map"] = map_id
    save_profile(profile)
    manager = CreatureManager(Path("presets/colony.json"), 3200, 1000, seed=4)
    m = create_mission(manager, ControlSettings(), rects, 0)
    m.rng = random.Random(6)
    return m


# -- layouts -----------------------------------------------------------------

def test_every_raid_map_has_its_own_layout_with_one_home_hatchery_and_nest():
    raids = [m.id for m in MAPS if m.kind == "raid"]
    assert set(raids) <= set(LAYOUTS)
    shapes = set()
    for map_id in raids:
        kinds = [p.kind for p in LAYOUTS[map_id]]
        assert kinds[0] == "home" and kinds.count("home") == 1
        assert kinds.count("hatchery") == 1 and kinds.count("nest") == 1
        shapes.add(tuple((p.kind, p.fx, p.fy, p.far) for p in LAYOUTS[map_id]))
    assert len(shapes) == len(raids), "no two maps are laid out alike"


def test_later_maps_bring_the_new_buildings():
    for map_id in ("ember", "obsidian", "queen"):
        kinds = {p.kind for p in LAYOUTS[map_id]}
        assert kinds & {"venom", "lookout", "nursery", "amber"}, map_id
    assert {"venom", "lookout", "nursery", "amber"} <= {p.kind for p in LAYOUTS["queen"]}


def test_the_nest_stands_on_the_far_screen_when_there_is_one(state_dir):
    two = mission("ember")
    assert two.nest.screen == 1 and SIDE[1].contains(two.nest.x, two.nest.y)
    one = mission("ember", rects=SIDE[:1])
    assert one.nest.screen == 0 and SIDE[0].contains(one.nest.x, one.nest.y)


def test_a_second_screen_adds_buildings_one_screen_does_not_have(state_dir):
    two = mission("territory")
    one = mission("territory", rects=SIDE[:1])
    assert "amber" in {s.kind for s in two.sites} and "amber" not in {s.kind for s in one.sites}
    assert [s.kind for s in one.sites] == ["home", "food", "silk", "hatchery", "nest"], "the first map as it was"


def test_buildings_stand_in_different_places_on_different_maps(state_dir):
    homes = {map_id: (round(mission(map_id).sites[0].x), round(mission(map_id).sites[0].y))
             for map_id in ("territory", "ember", "obsidian", "queen")}
    assert len(set(homes.values())) == 4


def test_guards_stand_at_their_buildings(state_dir):
    m = mission("ember")
    venom = m._site("venom")
    near = [a for a in m.actors if a.role != "ally"
            and ((a.creature.x - venom.x) ** 2 + (a.creature.y - venom.y) ** 2) ** 0.5 < 160]
    assert near, "the Venom den is guarded"


def test_any_building_is_a_foothold_on_the_later_maps(state_dir):
    m = mission("ember")
    assert m.objective == "01 / Capture any building for a foothold"
    m.capture(m._site("venom"))
    assert m.counter_started and m.nest.warning > 0
    assert m.objective.startswith("02")


# -- armour by map ---------------------------------------------------------------

def test_the_first_maps_enemies_wear_no_armour_and_later_ones_more():
    rng = random.Random(2)
    first, last = MAP_BY_ID["territory"], MAP_BY_ID["queen"]
    assert all(enemy_loadout(first, role, rng)[0] == [] for role in ("guard", "weaver", "hunter") for _ in range(30))
    assert min(len(enemy_loadout(last, "guard", rng)[0]) for _ in range(30)) >= 3
    assert max(len(enemy_loadout(MAP_BY_ID["ember"], "guard", rng)[0]) for _ in range(60)) <= 2


def test_bosses_wear_their_whole_set_a_level_above_their_guard():
    rng = random.Random(3)
    for info in MAPS:
        worn, level = enemy_loadout(info, "guardian", rng)
        assert sorted(worn) == sorted(ARMOR_SETS[info.guardian_set].pieces)
        _, guard_level = enemy_loadout(info, "guard", rng)
        assert level > guard_level or level == MAX_ITEM_LEVEL


def test_first_map_enemies_drop_nothing_but_the_boss_still_does(state_dir):
    m = mission("territory", rects=SIDE[:1])
    assert all(c.mission_loot == [] for c in m.manager.creatures if m.hero.relation_to(c) == "foe")
    boss = m._spawn("guardian", (m.nest.x, m.nest.y))
    assert boss.mission_loot


# -- the new buildings -----------------------------------------------------------

def test_the_venom_den_strengthens_whoever_holds_it(state_dir):
    m = mission("ember")
    enemy = next(a.creature for a in m.actors if a.role != "ally")
    base_hero, base_enemy = m.hero._venom_base, enemy._venom_base
    assert enemy.damage == pytest.approx(base_enemy * m.VENOM_BONUS), "the enemy holds it at the start"
    assert m.hero.damage == pytest.approx(base_hero)
    m.capture(m._site("venom"))
    assert m.hero.damage == pytest.approx(base_hero * m.VENOM_BONUS)
    assert enemy.damage == pytest.approx(base_enemy)


def test_the_lookout_lengthens_the_holders_range(state_dir):
    m = mission("ember")
    assert m.player.WEB_RANGE == pytest.approx(PlayerController.WEB_RANGE)
    m.capture(m._site("lookout"))
    assert m.player.WEB_RANGE == pytest.approx(PlayerController.WEB_RANGE * m.LOOKOUT_BONUS)


def test_the_amber_mine_pays_while_held(state_dir):
    m = mission("territory")
    before = int(m.profile["armoury"].get("amber", 0))
    m.capture(m._site("amber"))
    for _ in range(int(13 / 0.05)):
        m._building_work(0.05)
    assert m.amber_mined == 2 and m.profile["armoury"]["amber"] == before + 2
    m.save_progress()
    assert load_profile()["armoury"]["amber"] == before + 2


def test_the_nursery_hatches_for_its_holder_two_at_a_time(state_dir):
    m = mission("obsidian", rects=SIDE[:1])
    nursery = m._site("nursery")
    for c in list(m.manager.creatures):
        if m.hero.relation_to(c) == "foe":
            c.take_damage(10 ** 6, m.hero)
    m.manager._bury_the_dead()
    for _ in range(int(130 / 0.5)):
        m._nursery(nursery, 0.5)
    foes = [c for c in m.manager.creatures if not c.dead and getattr(c, "spiderling", False)]
    assert len(foes) == 2 and all(m.hero.relation_to(c) == "foe" for c in foes)
    assert all(c.size < m.hero.size for c in foes)
    nursery.owned = True
    for _ in range(int(130 / 0.5)):
        m._nursery(nursery, 0.5)
    ours = [c for c in m.manager.creatures if not c.dead and getattr(c, "spiderling", False)
            and m.hero.relation_to(c) != "foe"]
    assert len(ours) == 2 and all(c.display_name == "Spiderling" for c in ours)


# -- fly levels --------------------------------------------------------------------

def swarm(rects=SIDE, map_id="swarm"):
    m = mission(map_id, rects)
    assert isinstance(m, FlySwarmMission)
    return m


def test_the_fly_swarm_is_open_from_the_start_and_puts_a_nest_on_every_screen(state_dir):
    save_profile(fresh_profile())
    from desktop_bug.app.campaign import map_unlocked

    assert map_unlocked({}, "swarm") and not map_unlocked({}, "storm")
    m = swarm()
    assert sorted(s.screen for s in m.nests) == [0, 1]
    assert m.manager.fly_world.flies, "flies are out from the start"
    assert m.objective.startswith("Catch flies 0/")


def test_a_pinned_fly_is_eaten_for_silk_and_counts(state_dir):
    m = swarm()
    fly = m._release_fly() or m.manager.fly_world.flies[0]
    fly.x, fly.y = m.hero.x + 5, m.hero.y
    fly.pin_with_web_shot((m.hero.x, m.hero.y))
    m.player.silk = 0
    for c in m.manager.creatures:
        if c is not m.hero:
            c.x, c.y = 3000, 900
    m._flies(0.016)
    assert m.caught == 1 and m.player.silk >= 1


def test_rivals_hunt_the_flies(state_dir):
    m = swarm()
    rival = next(a for a in m.actors if a.role != "ally")
    fly = m.manager.fly_world.flies[0]
    goal = m.actor_goal(rival)
    assert goal is not None
    fly.x, fly.y = rival.creature.x, rival.creature.y
    for c in m.manager.creatures:
        if c is not rival.creature:
            c.x, c.y = 50, 50
    m._flies(0.016)
    assert m.rival_caught >= 1


def test_catching_the_swarm_wins_and_running_out_of_time_loses(state_dir):
    m = swarm()
    m.caught = m.rules.target
    m.update(0.016)
    assert m.state == "victory"
    assert load_profile()["missions"]["swarm"]["victories"] == 1
    m = swarm()
    m.time_left = 0.01
    m.update(0.05)
    assert m.state == "defeat" and "got away" in m.objective


def test_a_fly_left_alone_escapes(state_dir):
    m = swarm()
    for c in m.manager.creatures:
        c.x, c.y = 3100, 950
    count = len(m.manager.fly_world.flies)
    for fly in m.manager.fly_world.flies:
        m.fly_age[id(fly)] = m.rules.escape_after
    m._flies(0.016)
    assert m.escaped >= count - 1


def test_the_storm_sends_rival_dens_on_the_other_screens(state_dir):
    m = swarm(map_id="storm")
    assert m.outposts and all(s.screen == 1 for s in m.outposts)
    assert m.rules.target > RULES["swarm"].target and m.rules.rivals > RULES["swarm"].rivals


def test_every_new_scene_paints(state_dir):
    from desktop_bug.app.mission_ui import draw_buildings, draw_mission_hud

    window = SimpleNamespace(width=lambda: 3200, height=lambda: 1000, player=None, controls=ControlSettings())
    for map_id in ("ember", "obsidian", "queen", "swarm", "storm"):
        m = mission(map_id)
        m.update(0.05)
        image = QImage(3200, 1000, QImage.Format_ARGB32_Premultiplied)
        p = QPainter(image)
        draw_buildings(p, m)
        m.manager.render(p)
        draw_mission_hud(p, window, m)
        p.end()


def test_a_raid_on_one_screen_still_plays_the_map(state_dir):
    m = mission("queen", rects=SIDE[:1])
    assert type(m) is TerritoryMission and not m.layout.multi
    assert all(SIDE[0].contains(s.x, s.y) for s in m.sites)
    assert isinstance(m.layout, ScreenLayout)
