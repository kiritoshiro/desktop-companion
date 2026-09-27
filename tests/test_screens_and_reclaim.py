"""Multi-screen missions and Reclaim the desktop.

The owner: *"make multi screen missions too. to recognise automatically where
are the screens and allow to move to them and even create additional enemy
based on the type of mission"*, and *"enemies spit acid ... it would also melt
a hole in any open window, and a desktop part would be visible and later even
desktop would be broken bit by bit. also some spiders, big ones, when walking
or jumping could crack the desktop ... spiders could use [text] to create
something maybe like a silk or just eat the text."*
"""
from __future__ import annotations

import math
import random
from pathlib import Path
from types import SimpleNamespace

import pytest
from PyQt5.QtCore import QRectF
from PyQt5.QtGui import QColor, QImage, QPainter

from desktop_bug.app.adventure_profile import fresh_profile, load_profile, record_result, save_profile
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.desktop_capture import synthetic_snapshot
from desktop_bug.app.desktop_surface import DesktopSurface
from desktop_bug.app.encounters import PROFILES, EncounterDirector
from desktop_bug.app.mission import MissionActor, TerritoryMission
from desktop_bug.app.mission_factory import build_layout, create_mission
from desktop_bug.app.reclaim import ReclaimMission
from desktop_bug.app.screen_text import find_text_boxes
from desktop_bug.manager import CreatureManager
from desktop_bug.world.playfield import ScreenRect
from desktop_bug.world.screen_layout import ScreenLayout


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Images, painters and creatures need the one Qt application."""


# The owner's own monitors: a 2560x1440 main screen and a 4K one to its
# right, raised 718 px -- overlay-local, so everything shifts down by 718.
OWNER = [ScreenRect(0, 718, 2560, 1440), ScreenRect(2560, 0, 3840, 2160)]
SIDE = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1600, 0, 1600, 1000)]
APART = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1900, 200, 1280, 800)]


# -- the screens -------------------------------------------------------------

def test_the_owners_monitors_join_by_a_door_on_their_shared_edge():
    layout = ScreenLayout(OWNER, 0)
    assert layout.multi and layout.primary.index == 0
    assert [link.kind for link in layout.links] == ["door"]
    door = layout.links[0]
    assert door.a_point[0] < 2560 < door.b_point[0]
    assert 718 <= door.a_point[1] <= 2158, "the door is on the part of the edge both screens share"
    assert layout.screen_at(100, 900).index == 0 and layout.screen_at(4000, 100).index == 1
    # From the main screen to the 4K one: first to the door, then through it.
    x, y = layout.route(300, 1400, 5000, 300)
    assert (x, y) == door.a_point
    assert layout.route(*door.a_point, 5000, 300) == door.b_point
    assert layout.route(300, 1400, 900, 1500) == (900, 1500), "same screen: straight there"


def test_screens_that_do_not_touch_are_joined_by_a_tunnel():
    layout = ScreenLayout(APART, 0)
    assert [link.kind for link in layout.links] == ["tunnel"]
    tunnel = layout.links[0]
    assert layout.screen_at(*tunnel.a_point).index == 0
    assert layout.screen_at(*tunnel.b_point).index == 1
    found = layout.tunnel_at(*tunnel.a_point)
    assert found is not None and found[1] == tunnel.b_point
    assert layout.route(200, 200, 2500, 600) == tunnel.a_point


def test_three_screens_all_connect_and_a_path_crosses_them_in_order():
    rects = SIDE + [ScreenRect(4000, 0, 1200, 900)]
    layout = ScreenLayout(rects, 1)
    assert layout.primary.index == 1 and layout.primary.name == "Main screen"
    steps = layout.path(0, 2)
    assert len(steps) == 2 and steps[0].kind == "door" and steps[1].kind == "tunnel"


def test_the_layout_follows_the_setting_and_the_monitors():
    assert not build_layout(SIDE, 0, all_screens=False).multi
    assert build_layout(SIDE, 1, all_screens=True).primary.rect == SIDE[1]
    assert not build_layout(SIDE[:1], 0, all_screens=True).multi


# -- extra enemies by the kind of mission --------------------------------------

def test_a_raid_puts_an_outpost_on_every_other_screen_and_none_on_one_screen():
    one = EncounterDirector(PROFILES["raid"], ScreenLayout(SIDE[:1]), 1, random.Random(1))
    assert one.outposts() == []
    two = EncounterDirector(PROFILES["raid"], ScreenLayout(SIDE, 0), 1, random.Random(1))
    plans = two.outposts()
    assert [p.screen for p in plans] == [1]
    assert plans[0].kind == "outpost" and SIDE[1].contains(plans[0].x, plans[0].y)


def test_reclaim_infests_every_screen_including_the_main_one():
    director = EncounterDirector(PROFILES["reclaim"], ScreenLayout(SIDE, 0), 2, random.Random(1))
    plans = director.outposts()
    assert sorted(p.screen for p in plans) == [0, 1]
    assert all(p.kind == "infestation" and "spitter" in p.defenders for p in plans)


def test_later_maps_and_bigger_screens_bring_more_defenders():
    layout = ScreenLayout(OWNER, 0)
    first = EncounterDirector(PROFILES["raid"], layout, 1).defenders_for(layout.screens[1])
    later = EncounterDirector(PROFILES["raid"], layout, 3).defenders_for(layout.screens[1])
    small = EncounterDirector(PROFILES["raid"], ScreenLayout(SIDE, 0), 1).defenders_for(ScreenLayout(SIDE, 0).screens[1])
    assert len(later) > len(first) > len(small), "a 4K screen holds more than a 1600x1000 one"


def test_outposts_warn_then_send_waves_until_taken():
    director = EncounterDirector(PROFILES["raid"], ScreenLayout(SIDE, 0), 1)
    director.outposts()
    site = SimpleNamespace(owned=False, reserves=2, warning=0.0)
    due, warned = [], False
    for _ in range(int(80 / 0.1)):
        got = director.update(0.1, [site])
        warned = warned or site.warning > 0
        due += got
        if got:
            site.reserves -= 1
    assert warned and [role for _, role in due] == ["hunter", "guard"], "two reserves, two waves, warned first"
    site.reserves, site.owned = 5, True
    assert all(not director.update(0.5, [site]) for _ in range(200)), "a taken outpost sends nothing"


# -- missions across screens ---------------------------------------------------

def raid_mission(rects, map_id="territory"):
    profile = fresh_profile()
    profile["selected_map"] = map_id
    save_profile(profile)
    manager = CreatureManager(Path("presets/colony.json"), 3400, 1100, seed=4)
    m = TerritoryMission(manager, ControlSettings(), layout=ScreenLayout(rects, 0))
    m.rng = random.Random(3)
    return m


def test_a_two_screen_raid_keeps_its_buildings_home_and_guards_the_other_screen(state_dir):
    m = raid_mission(SIDE)
    assert [s.kind for s in m.sites[:5]] == ["home", "food", "silk", "hatchery", "nest"]
    assert all(SIDE[0].contains(s.x, s.y) for s in m.sites[:5])
    assert len(m.outposts) == 1 and m.outposts[0].screen == 1
    far = [a for a in m.actors if a.role != "ally" and SIDE[1].contains(a.creature.x, a.creature.y)]
    assert len(far) >= 2, "the outpost is guarded"
    assert m.objective.startswith("01"), "the outposts are optional; the objective is unchanged"
    assert "other screen" in m.notice


def test_spiders_may_stand_on_either_screen_and_cross_a_tunnel(state_dir):
    m = raid_mission(APART)
    m.hero.x, m.hero.y = 2500, 600
    m.hero.x, m.hero.y = m.layout.clamp(m.hero.x, m.hero.y, m.hero.margin)
    assert APART[1].contains(m.hero.x, m.hero.y), "the second screen is part of the arena"
    tunnel = m.layout.tunnels[0]
    m.hero.x, m.hero.y = tunnel.a_point
    m._through_tunnel(m.hero, 0.016)
    assert m.layout.screen_at(m.hero.x, m.hero.y).index == 1
    assert "tunnel" in m.notice.lower()
    before = (m.hero.x, m.hero.y)
    m._through_tunnel(m.hero, 0.016)
    assert (m.hero.x, m.hero.y) == before, "no bouncing straight back"


def test_outpost_raiders_do_not_hold_up_the_thorn_nest(state_dir):
    m = raid_mission(SIDE)
    for _ in range(int(40 / 0.05)):
        for c in m.manager.creatures:
            if c is not m.hero:
                c.motion_paused = True
        m.director.update(0.05, [])      # keep the clock honest without spawning
        m._outpost_waves(0.05)
        if any(a.from_outpost for a in m.actors):
            break
    raiders = [a for a in m.actors if a.from_outpost]
    assert raiders and all(a.raider for a in raiders)
    assert m.objective.startswith("01"), "not '03 / Defeat the counterattack'"


def test_taking_an_outpost_stops_its_raids_and_pays(state_dir):
    m = raid_mission(SIDE)
    site = m.outposts[0]
    xp = m.hero.progression.xp
    m.capture(site)
    assert site.owned and site.reserves == 0
    assert m.hero.progression.xp > xp or m.hero.level > 1
    assert "no more raids" in m.notice


# -- acid and spitters -----------------------------------------------------------

def test_a_spitter_keeps_the_cone_and_stamina_rules(state_dir):
    m = raid_mission(SIDE)
    c = m._spawn("spitter", (900, 500))
    actor = next(a for a in m.actors if a.creature is c)
    assert actor.style == "spitter"
    c.heading = 0.0
    m.hero.x, m.hero.y = c.x + 150, c.y
    actor.aim = (m.hero.x, m.hero.y)
    assert actor._choose_strike(m.hero, 150) == "spit"
    m.hero.x = c.x - 150                       # behind it
    assert actor._choose_strike(m.hero, 150) is None
    m.hero.x = c.x + 150
    c.energy = 1.0
    assert actor._choose_strike(m.hero, 150) is None, "no stamina, no spit"


def test_acid_lands_where_aimed_and_burns_only_foes(state_dir):
    m = raid_mission(SIDE)
    spitter = m._spawn("spitter", (800, 500))
    m.hero.x, m.hero.y = 1000, 500
    ally_hp = [a.hp for a in m.allies]
    hp = m.hero.hp
    m.spit_acid(spitter, (m.hero.x, m.hero.y))
    assert len(m.hazards) == 1
    for _ in range(40):
        m._update_hazards(0.02)
    assert not m.hazards and m.splashes
    assert m.hero.hp < hp
    assert [a.hp for a in m.allies] == ally_hp or all(math.hypot(a.x-1000, a.y-500) < 60 for a in m.allies)


# -- the frozen desktop --------------------------------------------------------------

WINDOW = QRectF(200, 150, 700, 500)


def surface(text_rows=0, rects=None):
    snap = synthetic_snapshot(rects or [ScreenRect(0, 0, 1600, 1000)], [WINDOW], text_rows=text_rows)
    return DesktopSurface(snap, seed=2, find_text=text_rows > 0)


def test_acid_melts_a_window_then_the_desktop_behind_it_then_nothing_is_left():
    s = surface()
    x, y = 500, 400
    assert s.what_is_at(x, y) == "window"
    full = s.integrity
    assert s.melt(x, y, 30) == "window"
    assert s.what_is_at(x, y) == "desktop", "the wallpaper shows through the hole"
    assert s.integrity == full, "a hole in a window does not eat the desktop"
    assert s.melt(x, y, 30) == "desktop"
    assert s.what_is_at(x, y) == "void"
    assert s.melt(x, y, 30) is None
    assert s.integrity < full


def test_acid_on_the_bare_desktop_breaks_it_at_once():
    s = surface()
    assert s.what_is_at(1300, 800) == "desktop"
    assert s.melt(1300, 800, 30) == "desktop"
    assert s.what_is_at(1300, 800) == "void"


def test_the_picture_shows_the_hole():
    s = surface()
    image = QImage(1600, 1000, QImage.Format_ARGB32_Premultiplied)
    before = QImage(image)
    p = QPainter(before)
    s.paint(p)
    p.end()
    s.melt(500, 400, 30)
    p = QPainter(image)
    s.paint(p)
    p.end()
    assert QColor(before.pixel(500, 400)) != QColor(image.pixel(500, 400))
    assert QColor(before.pixel(1400, 900)) == QColor(image.pixel(1400, 900)), "the rest is untouched"


def test_heavy_landings_crack_the_glass_until_it_shatters():
    s = surface()
    layer = s.layers[0]
    assert not s.crack(800, 500, 0.5) and 0 < layer.stress < 1
    shattered = False
    for i in range(40):
        shattered = s.crack(300 + i * 25, 500, 1.0) or shattered
        if shattered:
            break
    assert shattered and layer.shattered and layer.shards
    assert s.what_is_at(500, 400) == "desktop", "the glass and windows fell away; the desktop is bare"
    for _ in range(200):
        s.update(0.02)
    assert not layer.shards, "the falling shards finish"
    assert not s.crack(800, 500, 1.0), "broken glass cannot crack again"


def test_words_are_found_by_their_shape_and_eaten_to_the_background():
    s = surface(text_rows=8)
    assert len(s.words) >= 10
    assert all(WINDOW.adjusted(-4, -4, 4, 4).contains(w.rect) for w in s.words), "only the window has text"
    word = s.words[0]
    cx, cy = word.centre
    assert s.word_near(cx, cy, 5) is word
    while not s.eat(word, 0.3):
        pass
    after = QColor(s.layers[0].top.pixel(int(cx), int(cy)))
    assert abs(after.red() - word.colour.red()) < 10, "the word is erased to the colour around it"
    assert s.word_near(cx, cy, 5) is not word


def test_a_plain_picture_has_no_words():
    image = QImage(800, 600, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(60, 90, 140))
    assert find_text_boxes(image) == []


# -- Reclaim the desktop -------------------------------------------------------------

def reclaim(rects=SIDE, text_rows=6):
    profile = fresh_profile()
    record_result(profile, "territory", True, 100.0)
    profile["selected_map"] = "reclaim"
    save_profile(profile)
    manager = CreatureManager(Path("presets/colony.json"), 3200, 1000, seed=4)
    windows = [QRectF(200, 150, 700, 500)]
    m = create_mission(manager, ControlSettings(), rects, 0,
                       capture=lambda: synthetic_snapshot(rects, windows, text_rows=text_rows))
    m.rng = random.Random(4)
    return m


def test_reclaim_freezes_every_screen_and_infests_each(state_dir):
    m = reclaim()
    assert isinstance(m, ReclaimMission) and m.FREEZES_DESKTOP
    assert m.layout.count == 2 and len(m.surface.layers) == 2
    assert sorted(s.screen for s in m.outposts) == [0, 1]
    assert any(a.style == "spitter" for a in m.actors if a.role != "ally")
    assert m.objective == "01 / Destroy the nests  (0/2)"
    assert m.intro_time > 0


def test_a_plain_raid_never_freezes_the_desktop(state_dir):
    profile = fresh_profile()
    save_profile(profile)
    m = create_mission(CreatureManager(Path("presets/colony.json"), 3200, 1000, seed=4),
                       ControlSettings(), SIDE, 0, capture=lambda: pytest.fail("no picture for a raid"))
    assert type(m) is TerritoryMission and not m.FREEZES_DESKTOP and m.layout.multi


def test_reclaim_acid_melts_the_frozen_desktop(state_dir):
    m = reclaim()
    spitter = next(a.creature for a in m.actors if a.style == "spitter")
    assert m.surface.what_is_at(500, 400) == "window"
    spitter.x, spitter.y = 700, 450
    m.spit_acid(spitter, (500, 400))
    for _ in range(60):
        m._update_hazards(0.02)
    assert not m.hazards
    assert m.surface.what_is_at(500, 400) == "desktop"


def test_your_spiders_never_eat_words_but_enemies_devour_them(state_dir):
    m = reclaim()
    word = m.surface.living_words()[0]
    enemies = [a for a in m.actors if a.role != "ally"]
    for a in enemies:
        a.creature.x, a.creature.y = 3100, 950
    m.hero.x, m.hero.y = word.centre
    for ally in m.allies:
        ally.x, ally.y = word.centre
    for _ in range(40):
        m._eat_words(0.05)
    assert not word.done and m.words_eaten == 0, "your spiders leave the text alone"
    enemy = enemies[0].creature
    enemy.x, enemy.y = word.centre
    enemy.hp = enemy.max_hp * 0.5
    hp = enemy.hp
    for _ in range(40):
        m._eat_words(0.05)
    assert word.done and m.words_eaten >= 1 and enemy.hp > hp
    assert m.threads, "the letters are torn away"


def test_an_enemy_with_no_one_to_fight_goes_for_a_word(state_dir):
    m = reclaim()
    raider = next(a for a in m.actors if a.role != "ally")
    raider.raider = True
    word = m.surface.living_words()[0]
    raider.creature.x, raider.creature.y = word.centre[0] + 120, word.centre[1]
    goal = m.actor_goal(raider)
    assert goal is not None
    assert m.actor_goal(next(a for a in m.actors if a.role == "ally")) is None


def test_big_spiders_crack_the_glass_and_small_ones_do_not(state_dir):
    m = reclaim()
    small = m.hero
    m.on_landing(small)
    assert all(layer.stress == 0 for layer in m.surface.layers)
    big = m._spawn("guard", (1200, 500))
    big.set_size_scale(1.7)
    m.on_landing(big)
    assert m.surface.layers[0].stress > 0
    before = m.surface.layers[0].stress
    for _ in range(3):
        m._heavy_steps(big, 60)
    assert m.surface.layers[0].stress > before, "a boss's walk cracks it too"


def test_destroying_every_nest_raises_the_devourer_and_killing_it_wins(state_dir):
    m = reclaim()
    for site in m.outposts:
        m.capture(site)
    assert m.objective == "02 / Defeat the Devourer"
    for c in list(m.manager.creatures):
        if m.hero.relation_to(c) == "foe":
            c.take_damage(10 ** 6, m.hero)
    m.manager._bury_the_dead()
    m.actors = [a for a in m.actors if not a.creature.dead]
    m.hero.x, m.hero.y = m.layout.primary.anchor(0.1, 0.5, 60)
    for _ in range(int(5 / 0.05)):
        m._spawning(0.05)
    assert m.guardian is not None and m.guardian.display_name == "The Devourer"
    assert m.layout.screen_at(m.guardian.x, m.guardian.y).index == 1, "it rises on the other screen"
    m.guardian.take_damage(10 ** 7, m.hero)
    m.update(0.016)
    assert m.state == "victory"
    assert load_profile()["missions"]["reclaim"]["victories"] == 1


def test_too_much_desktop_eaten_loses_the_raid(state_dir):
    m = reclaim(rects=SIDE[:1], text_rows=0)
    for x in range(40, 1600, 50):
        for y in range(40, 1000, 50):
            m.surface.melt(x, y, 40)
            m.surface.melt(x, y, 40)
    assert m.surface.integrity < m.LOST_BELOW
    m.update(0.016)
    assert m.state == "defeat" and m.lost_desktop
    assert "acid" in m.objective.lower()


def test_retry_starts_a_fresh_desktop_from_the_same_picture(state_dir):
    m = reclaim()
    m.surface.melt(500, 400, 30)
    again = m.restarted()
    assert isinstance(again, ReclaimMission) and again.surface is not m.surface
    assert again.surface.what_is_at(500, 400) == "window"


def test_the_whole_reclaim_scene_paints(state_dir):
    """Surface, buildings, links, acid, minimap, desktop meter, intro card and
    end title: a paint error here would crash the overlay (the #90 lesson)."""
    from desktop_bug.app.mission_ui import draw_buildings, draw_mission_hud

    m = reclaim()
    spitter = next(a.creature for a in m.actors if a.style == "spitter")
    m.spit_acid(spitter, (m.hero.x, m.hero.y))
    m._update_hazards(0.1)
    window = SimpleNamespace(width=lambda: 3200, height=lambda: 1000, player=None, controls=ControlSettings())
    image = QImage(3200, 1000, QImage.Format_ARGB32_Premultiplied)
    p = QPainter(image)
    m.surface.paint(p)
    draw_buildings(p, m)
    for c in m.manager.creatures:
        c.render(p)
    draw_mission_hud(p, window, m)
    m.state, m.lost_desktop = "defeat", True
    draw_mission_hud(p, window, m)
    p.end()


def test_the_adventure_page_offers_every_screen(state_dir):
    from PyQt5.QtWidgets import QLabel

    from desktop_bug.app.mode_menu import ModeShell

    save_profile(fresh_profile())
    shell = ModeShell(QLabel("editor"), lambda: None)
    shell.refresh_adventure()
    assert shell.all_screens_check.isChecked()
    shell.all_screens_check.setEnabled(True)
    shell.all_screens_check.setChecked(False)
    assert load_profile()["all_screens"] is False
    assert "reclaim" in shell.mission_cards


def test_an_actor_heads_for_the_door_when_its_target_is_on_the_other_screen(state_dir):
    m = raid_mission(SIDE)
    ally = next(a for a in m.actors if a.role == "ally")
    ally.creature.x, ally.creature.y = 800, 500
    m.hero.x, m.hero.y = 2600, 500
    ally.think = 0
    ally.update(0.016)
    door = m.layout.links[0]
    assert math.hypot(ally.creature.target_x - door.a_point[0], ally.creature.target_y - door.a_point[1]) < 60
    assert isinstance(ally, MissionActor)
