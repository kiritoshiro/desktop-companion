"""In-game Character window, quieter names, fighting before flies, building art.

The owner: *"when in game and opened inventory it should also allow to change
armor and show that visual of spider anatomy as in pregame ... the in game
spider names are taking too much space and hides what is beneath them. make
them less having presence. only my own should be highlighted. when flies are
present enemy spiders prioritise them over the enemy (me). fix that ...
make nicer graphics of those buildings."*
"""
from __future__ import annotations

import math
import random
from pathlib import Path

import pytest
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QImage, QPainter

from desktop_bug.app import armoury
from desktop_bug.app.adventure_profile import (fresh_profile, hero_progression, load_profile, save_profile,
                                               store_progression)
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission_art import SCALE, building_art
from desktop_bug.app.mission_factory import create_mission
from desktop_bug.manager import CreatureManager
from desktop_bug.world.playfield import ScreenRect


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and painting need the one Qt application."""


SIDE = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1600, 0, 1600, 1000)]


def mission(map_id="territory", rects=SIDE[:1]):
    profile = fresh_profile()
    profile["selected_map"] = map_id
    save_profile(profile)
    m = create_mission(CreatureManager(Path("presets/colony.json"), 3200, 1000, seed=4),
                       ControlSettings(), rects, 0)
    m.rng = random.Random(2)
    return m


def _ink(image):
    return sum(QColor.fromRgba(image.pixel(x, y)).alpha() > 30 for y in range(image.height()) for x in range(image.width()))


# -- names -----------------------------------------------------------------------

def test_only_your_own_spider_has_a_highlighted_name(state_dir):
    m = mission()
    assert m.hero.label_style == "hero"
    others = [c for c in m.manager.creatures if c is not m.hero]
    assert others and all(c.label_style == "quiet" for c in others)


def test_a_quiet_name_takes_far_less_room_than_a_full_one(state_dir):
    m = mission()
    enemy = next(c for c in m.manager.creatures if m.hero.relation_to(c) == "foe")

    def label_ink(style):
        enemy.label_style = style
        image = QImage(600, 400, QImage.Format_ARGB32_Premultiplied)
        image.fill(Qt.transparent)
        p = QPainter(image)
        p.translate(300 - enemy.x, 250 - enemy.y)
        enemy._draw_name_label(p, True)
        p.end()
        return _ink(image)

    assert label_ink("quiet") < label_ink("full") * 0.45


# -- enemies fight before they chase flies ------------------------------------------

def swarm():
    m = mission("swarm", SIDE)
    assert m.manager.fly_world.flies
    return m


def rival_of(m):
    return next(a for a in m.actors if a.aggro_range is not None)


def test_a_rival_attacks_you_before_any_fly(state_dir):
    m = swarm()
    rival = rival_of(m)
    c = rival.creature
    fly = m.manager.fly_world.flies[0]
    fly.x, fly.y = c.x + 30, c.y                        # a fly right beside it
    m.hero.x, m.hero.y = c.x + 200, c.y                 # and you close enough to fight
    rival.think = 0
    rival.update(0.016)
    assert rival.target is m.hero
    assert math.hypot(c.target_x - m.hero.x, c.target_y - m.hero.y) < 40, "it heads for you, not the fly"


def test_with_you_far_away_a_rival_hunts_flies(state_dir):
    m = swarm()
    rival = rival_of(m)
    c = rival.creature
    m.hero.x, m.hero.y = 3100, 950
    for ally in m.allies:
        ally.x, ally.y = 3100, 950
    fly = m.manager.fly_world.flies[0]
    fly.x, fly.y = c.x + 120, c.y + 40
    rival.think = 0
    rival.update(0.016)
    assert rival.target is None
    assert math.hypot(c.target_x - fly.x, c.target_y - fly.y) < 60


def test_a_defender_hunts_only_flies_near_its_post(state_dir):
    m = swarm()
    defender = next(a for a in m.actors if a.role != "ally" and a.aggro_range is None)
    post = defender.defend_point
    near = m.manager.fly_world.flies[0]
    near.x, near.y = post[0] + 60, post[1]
    assert m.actor_goal(defender) == (near.x, near.y)
    for fly in m.manager.fly_world.flies:
        fly.x, fly.y = post[0] + 900, post[1]
    assert m.actor_goal(defender) is None, "it does not leave its post for a far fly"


# -- the Character window in a raid ----------------------------------------------------

def test_armour_changed_mid_raid_goes_on_the_living_hero(state_dir):
    m = mission()
    before_armor = m.hero.armor
    m.hero.take_damage(m.hero.max_hp * 0.5)
    share = m.hero.hp / m.hero.max_hp
    m.save_progress()
    profile = load_profile()
    armoury.add_loot(profile, "frost_aegis")
    state = hero_progression(profile)
    state.equipped["carapace"] = "frost_aegis"
    store_progression(profile, "hero", state)
    save_profile(profile)
    m.refresh_from_profile()
    assert m.hero.progression.equipped.get("carapace") == "frost_aegis"
    assert m.hero.armor > before_armor
    assert m.hero.hp / m.hero.max_hp == pytest.approx(share, abs=0.02), "wounds are kept"
    assert m.hero.progression.team_id == "adventurers", "still on your side"


def test_the_in_game_window_is_the_pregame_character_window(state_dir):
    from desktop_bug.app.character_ui import CharacterDialog

    m = mission()
    m.save_progress()
    from desktop_bug.app.armour_ui import InventoryBag, SpiderDoll

    dialog = CharacterDialog(path=m.progress_path)
    assert dialog.findChildren(SpiderDoll) and dialog.findChildren(InventoryBag), "the doll and the bag"


# -- building art ----------------------------------------------------------------------

def test_buildings_are_painted_at_twice_the_size_for_crisp_edges():
    art = building_art("home", False)
    assert art.devicePixelRatio() == SCALE and art.width() == 240 * SCALE


def test_every_building_stands_on_an_earth_base_with_a_soil_face():
    for kind in ("home", "amber", "flynest"):
        art = building_art(kind, False)
        # Below the site's point, the soil face is painted in dark earth.
        x, y = 120 * SCALE, (139 + 14) * SCALE
        colour = QColor.fromRgba(art.pixel(x, y))
        assert colour.alpha() > 200 and colour.red() < 140, (kind, colour.name())
