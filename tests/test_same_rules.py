"""Every mission spider plays by the player's rules.

The owner: *"i noticed enemy can shoot webs from any angle he is facing not
just from the front 90 degrees. make sure all spiders have the same rules,
stamina, shooting, xp hp regeneration on base and so on. its just that i can
control mine."*
"""
from __future__ import annotations

import math
from pathlib import Path

from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.manager import CreatureManager


def make_mission():
    manager = CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4)
    return TerritoryMission(manager, ControlSettings())


def actor(m, role):
    return next(a for a in m.actors if a.role == role)


def place(spider, x, y, heading):
    spider.x, spider.y = x, y
    spider.target_x, spider.target_y = x, y
    spider.heading = spider.target_heading = heading
    spider._initialize_legs()


def only(m, *keep):
    """Remove every enemy but ``keep`` so no one else joins in."""
    for a in list(m.actors):
        if a.role != "ally" and a not in keep:
            a.creature.take_damage(100000, m.hero)
    m.manager._bury_the_dead()
    m.actors = [a for a in m.actors if not a.creature.dead]


def test_an_enemy_cannot_shoot_or_bite_what_is_behind_it(state_dir):
    m = make_mission()
    weaver, guard = actor(m, "weaver"), actor(m, "guard")
    only(m, weaver, guard)
    x, y = m.area.x + m.area.w / 2, m.area.y + m.area.h / 2
    place(m.hero, x, y, 0.0)
    for a, gap in ((weaver, 200.0), (guard, weaver.bite_reach() * 0.5)):
        # Facing straight away from the hero, which is in range.
        place(a.creature, x + gap, y, 0.0)
        a.target = m.hero
        assert not a.in_cone(m.hero)
        assert a._choose_strike(m.hero, gap) is None, a.role
        # Turned to face it, the same spider may strike.
        a.creature.heading = math.pi
        assert a._choose_strike(m.hero, gap) is not None, a.role


def test_an_enemy_shot_costs_stamina_and_silk_like_the_players(state_dir):
    m = make_mission()
    weaver = actor(m, "weaver")
    only(m, weaver)
    c = weaver.creature
    x, y = m.area.x + m.area.w / 2, m.area.y + m.area.h / 2
    place(m.hero, x - 200, y, 0.0)
    place(c, x, y, math.pi)
    energy, silk = c.energy, weaver.silk
    weaver.aim = (m.hero.x, m.hero.y)
    assert weaver.shoot(m.manager)
    assert c.energy == energy - PlayerController.WEB_ENERGY
    assert weaver.silk == silk - 1
    assert weaver.web_cooldown == PlayerController.WEB_COOLDOWN
    weaver.web_cooldown = 0.0
    weaver.silk = 0.0
    assert weaver._choose_strike(m.hero, 200.0) is None, "no silk, no shot"
    weaver.silk = 3.0
    c.energy = PlayerController.WEB_ENERGY - 1
    assert weaver._choose_strike(m.hero, 200.0) is None, "no stamina, no shot"


def test_a_shot_goes_where_the_enemy_faces_not_behind_it(state_dir):
    m = make_mission()
    weaver = actor(m, "weaver")
    c = weaver.creature
    place(c, m.area.x + 600, m.area.y + 400, 0.0)
    weaver.aim = (c.x - 200, c.y)          # straight behind it
    assert abs(weaver.aim_angle()) <= m.controls.half_cone + 1e-9


def test_an_enemy_pounce_costs_stamina_and_carries_as_far_as_the_players(state_dir):
    m = make_mission()
    hunter = actor(m, "hunter")
    only(m, hunter)
    c = hunter.creature
    x, y = m.area.x + m.area.w / 2, m.area.y + m.area.h / 2
    place(m.hero, x, y, 0.0)
    far = hunter.bite_reach() + PlayerController.POUNCE_DISTANCE + 40
    place(c, x + far, y, math.pi)
    assert hunter._choose_strike(m.hero, far) is None, "out of pounce range"
    near = hunter.bite_reach() + PlayerController.POUNCE_DISTANCE * 0.5
    place(c, x + near, y, math.pi)
    assert hunter._choose_strike(m.hero, near) == "pounce"
    energy = c.energy
    hunter.strike_kind, hunter.strike_point = "pounce", (m.hero.x, m.hero.y)
    hunter._release_strike()
    assert c.airborne and c.energy == energy - PlayerController.JUMP_ENERGY


def test_enemies_heal_and_refill_silk_at_their_own_bases(state_dir):
    m = make_mission()
    weaver = actor(m, "weaver")
    only(m, weaver)
    c = weaver.creature
    food = next(s for s in m.sites if s.kind == "food")
    loom = next(s for s in m.sites if s.kind == "silk")
    m.hero.x, m.hero.y = m.sites[0].x, m.sites[0].y
    m.ally.x, m.ally.y = m.sites[0].x, m.sites[0].y
    c.hp = c.max_hp * 0.4
    place(c, food.x + 30, food.y, 0.0)
    hurt, supply = c.hp, food.supply
    m._heal_at(food, 1.0)
    assert c.hp == hurt + m.HEAL_RATES["food"] and food.supply < supply
    weaver.silk = 1.0
    place(c, loom.x + 30, loom.y, 0.0)
    m._refill_silk_at(loom, 1.0)
    assert weaver.silk == 1.0 + m.SILK_RATES["silk"]
    # Holding the loom lets a side carry more silk -- the enemy starts with it.
    assert weaver.silk_capacity == PlayerController.LOOM_SILK_CAPACITY
    assert m.player.silk_capacity == PlayerController.SILK_CAPACITY
    m.capture(loom)
    assert weaver.silk_capacity == PlayerController.SILK_CAPACITY
    assert m.player.silk_capacity == PlayerController.LOOM_SILK_CAPACITY


def test_an_enemy_base_serves_nobody_while_the_player_is_on_it(state_dir):
    m = make_mission()
    weaver = actor(m, "weaver")
    only(m, weaver)
    c = weaver.creature
    food = next(s for s in m.sites if s.kind == "food")
    c.hp = c.max_hp * 0.4
    place(c, food.x + 30, food.y, 0.0)
    m.hero.x, m.hero.y = food.x - 40, food.y
    m.hero.hp = m.hero.max_hp * 0.4
    hero_hp, hurt = m.hero.hp, c.hp
    m.update(1 / 60)
    assert c.hp == hurt, "the player standing on it denies it to the enemy"
    assert m.hero.hp <= hero_hp, "and an enemy base never heals the player"


def test_a_webbed_enemy_struggles_free_as_fast_as_the_player_can(state_dir):
    m = make_mission()
    guard = actor(m, "guard")
    guard.creature.web_pinned("trap", m.hero)
    m.hero.web_pinned("trap", guard.creature)
    m.player.set_held("move_up", True)          # the player struggles
    start = guard.creature.webbed_timer
    for _ in range(30):
        guard.creature._update_web_struggle(1 / 60)
        m.hero._update_web_struggle(1 / 60)
    assert guard.creature.webbed_timer == m.hero.webbed_timer < start


def test_level_and_armour_speed_count_for_enemies_too(state_dir):
    m = make_mission()
    guard = actor(m, "guard")
    guard.target = None
    guard.think = 1.0
    guard.update(1 / 60)
    base = guard.creature.speed
    guard.creature._progression_speed_multiplier *= 1.5
    guard.update(1 / 60)
    assert guard.creature.speed > base
