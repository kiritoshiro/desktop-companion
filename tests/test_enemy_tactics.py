"""Enemy kinds with their own abilities and temperaments, and squad tactics.

The owner: *"based on the way it looks make them unique in the abilities they
have ... some defenders defend the outpost most and don't chase much. some
aggressive and chase forever ... create multispider strategies ... keep it
flexible, later we will use it in strategy mode ... the first levels don't put
many enemies."*
"""
from __future__ import annotations

import math
import random
from pathlib import Path

import pytest

from desktop_bug.app.adventure_profile import fresh_profile, record_result, save_profile
from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.mission_factory import create_mission
from desktop_bug.content.enemy_kinds import ENEMY_KINDS
from desktop_bug.content.enemy_traits import TEMPERAMENTS, TRAITS, traits_for
from desktop_bug.manager import CreatureManager
from desktop_bug.world import tactics
from desktop_bug.world.playfield import ScreenRect
from desktop_bug.world.tactics import Foe, Member


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures need the one Qt application."""


# -- traits ------------------------------------------------------------------------

def test_every_enemy_kind_has_its_own_traits():
    assert set(TRAITS) == set(ENEMY_KINDS)
    for kind_id, traits in TRAITS.items():
        assert traits.temperament in TEMPERAMENTS, kind_id
        assert traits.look, kind_id
    # Recognisably different: not all one temperament, speed or health.
    assert len({t.temperament for t in TRAITS.values()}) >= 5
    assert len({t.speed for t in TRAITS.values()}) >= 5 and len({t.health for t in TRAITS.values()}) >= 5


def test_looks_match_what_they_do():
    assert TRAITS["redback_raider"].temper.chase_forever, "the red raider chases for ever"
    assert TRAITS["crab_reaver"].temperament == "sentinel", "the armoured crab holds its post"
    assert TRAITS["trapdoor_brute"].health > TRAITS["jumping_skirmisher"].health * 2
    assert TRAITS["redback_raider"].speed > TRAITS["trapdoor_brute"].speed
    assert TRAITS["harvest_stalker"].has("web") and TRAITS["pale_cave"].has("spit")
    assert traits_for(None, "weaver").has("web"), "a spider with no kind fights by its role"


# -- in a mission -------------------------------------------------------------------

def mission(map_id="territory", won=()):
    profile = fresh_profile()
    for earlier in won:
        record_result(profile, earlier, True, 100.0)
    profile["selected_map"] = map_id
    save_profile(profile)
    m = create_mission(CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4),
                       ControlSettings(), [ScreenRect(0, 0, 1600, 1000)], 0)
    m.rng = random.Random(5)
    return m


def spawn(m, kind, role="guard", pos=(900, 500)):
    spec = {"role": role, "kind": kind, "level_bonus": 0, "hp": 1.0, "armor": []}
    c = m._spawn(role, pos, spec=spec)
    return c, next(a for a in m.actors if a.creature is c)


def test_a_brute_is_tougher_and_slower_than_a_skirmisher_and_stays_so_after_levelling(state_dir):
    m = mission()
    m.CAP = 30
    brute, brute_actor = spawn(m, "trapdoor_brute")
    darter, darter_actor = spawn(m, "jumping_skirmisher", pos=(950, 600))
    assert brute.max_hp > darter.max_hp * 2
    assert brute.armor > darter.armor
    before = brute.max_hp
    brute.gain_experience(10_000, "test")
    assert brute.max_hp > before, "levelling adds to the kind's health rather than wiping it"
    assert brute.max_hp > darter.max_hp * 2
    for actor in (brute_actor, darter_actor):
        actor.think = 0
        actor.update(0.016)
    assert darter.speed > brute.speed


def test_a_sentinel_stays_at_its_post_while_an_aggressive_one_chases_for_ever(state_dir):
    m = mission()
    m.CAP = 30
    crab, crab_actor = spawn(m, "crab_reaver", pos=(900, 500))
    red, red_actor = spawn(m, "redback_raider", role="hunter", pos=(900, 560))
    # You come close, then run far away.
    m.hero.x, m.hero.y = 1000, 520
    for actor in (crab_actor, red_actor):
        actor.think = 0
        actor.update(0.016)
    assert crab_actor.target is m.hero and red_actor.target is m.hero
    m.hero.x, m.hero.y = 1500, 900
    for _ in range(40):
        for actor in (crab_actor, red_actor):
            actor.think = 0
            actor.update(0.05)
    assert crab_actor.target is None, "the sentinel gave up and went home"
    assert red_actor.target is m.hero and red_actor.hunting, "the redback is still after you"


def test_regen_heals_and_hit_and_run_springs_back(state_dir):
    m = mission()
    m.CAP = 30
    pale, pale_actor = spawn(m, "pale_cave", role="spitter")
    pale.hp = pale.max_hp * 0.5
    hp = pale.hp
    pale_actor.update(1.0)
    assert pale.hp > hp
    darter, darter_actor = spawn(m, "jumping_skirmisher", role="hunter", pos=(700, 400))
    darter_actor.strike_kind = "bite"
    darter_actor.strike_point = (m.hero.x, m.hero.y)
    darter_actor._release_strike()
    assert darter_actor.retreat_time > 0


# -- squad tactics (no mission needed) ------------------------------------------------

def members(*specs):
    return [Member(name, x, y, 1.0, role, 180.0 if role in ("ranged", "support") else 34.0, anchor=(0.0, 0.0))
            for name, x, y, role in specs]


HERO = Foe("hero", 200.0, 0.0, 1.0, 1.6)


def test_no_teamwork_on_the_first_maps():
    squad = members(("a", 0, 0, "melee"), ("b", 10, 10, "melee"))
    assert tactics.plan(squad, [HERO], 0).orders == {}
    assert tactics.tactics_level(1) == 0 and tactics.tactics_level(4) == 3


def test_focus_fire_sends_everyone_at_the_same_foe():
    squad = members(("a", 0, 0, "melee"), ("b", 10, 10, "melee"))
    weak = Foe("scout", 150.0, 40.0, 0.2, 1.0)
    result = tactics.plan(squad, [HERO, weak], 1)
    assert result.doctrine == "focus fire"
    assert {o.target for o in result.orders.values()} == {result.focus}


def test_hammer_and_anvil_tank_in_front_fast_ones_on_the_flanks():
    squad = members(("tank", 0, 0, "tank"), ("f1", 0, 30, "fast"), ("f2", 0, -30, "fast"))
    result = tactics.plan(squad, [HERO], 2)
    assert result.doctrine == "hammer and anvil"
    assert result.orders["tank"].mode == "engage"
    p1, p2 = result.orders["f1"].point, result.orders["f2"].point
    assert result.orders["f1"].mode == "flank" and p1[1] * p2[1] < 0, "one on each side"
    assert p1[0] > HERO.x - 10 and p2[0] > HERO.x - 10, "round beside and behind it"


def test_screen_keeps_the_shooters_behind_the_close_fighters():
    squad = members(("m", 60, 0, "melee"), ("r", 0, 0, "ranged"))
    result = tactics.plan(squad, [HERO], 2)
    assert result.doctrine == "screen"
    front, back = result.orders["m"].point, result.orders["r"].point
    assert math.dist(front, (HERO.x, HERO.y)) < math.dist(back, (HERO.x, HERO.y))
    assert result.orders["r"].mode == "kite"


def test_encircle_puts_three_close_fighters_round_a_foe():
    squad = members(("a", 0, 0, "melee"), ("b", 0, 40, "melee"), ("c", 0, -40, "tank"))
    result = tactics.plan(squad, [HERO], 3)
    assert result.doctrine == "encircle"
    angles = sorted(math.atan2(o.point[1] - HERO.y, o.point[0] - HERO.x) for o in result.orders.values())
    gaps = [(b - a) % math.tau for a, b in zip(angles, angles[1:] + angles[:1])]
    assert min(gaps) > 1.5, "spread round the foe"


def test_defenders_hold_a_perimeter_and_the_advanced_ones_bait_and_ambush():
    far_foe = Foe("hero", 600.0, 0.0)
    squad = members(("tank", 0, 0, "tank"), ("fast", 20, 0, "fast"), ("r", -20, 0, "ranged"))
    held = tactics.plan(squad, [far_foe], 2, "defend", anchor=(0.0, 0.0))
    assert held.doctrine == "perimeter" and all(o.mode == "hold" for o in held.orders.values())
    lure = tactics.plan(squad, [far_foe], 3, "defend", anchor=(0.0, 0.0))
    assert lure.doctrine == "bait and ambush"
    assert lure.orders["fast"].mode == "bait" and lure.orders["tank"].mode == "ambush"


def test_the_badly_hurt_fall_back_and_a_scattered_attack_regroups():
    squad = members(("a", 0, 0, "melee"), ("b", 10, 0, "melee"), ("c", 20, 0, "melee"))
    squad[0].health = 0.1
    result = tactics.plan(squad, [HERO], 1)
    assert result.orders["a"].mode == "retreat"
    spread = members(("a", 0, 0, "melee"), ("b", 600, 0, "melee"))
    regroup = tactics.plan(spread, [], 2, "attack")
    assert regroup.doctrine == "regroup"


def test_harder_maps_plan_as_squads(state_dir):
    easy = mission()
    assert easy.tactics_level == 0
    hard = mission("queen", won=("territory", "ember", "obsidian"))
    assert hard.tactics_level == 3
    guarded = next(s for s in hard.sites if len([a for a in hard.actors if a.role != "ally"
                                                   and math.dist(a.home, (s.x, s.y)) < 150]) >= 2)
    hard.hero.x, hard.hero.y = guarded.x + 120, guarded.y
    hard.squad_clock = 0
    hard._plan_squads(0.1)
    orders = [a.order for a in hard.actors if a.role != "ally" and a.order is not None]
    assert orders, "the squad at the guarded building has a plan"


def test_the_first_map_is_lightly_held(state_dir):
    m = mission()
    enemies = [a for a in m.actors if a.role != "ally"]
    assert len(enemies) <= 2
    assert m.CAP == m.EARLY_CAP


def test_a_building_guard_is_never_a_chase_for_ever_kind_when_another_will_do(state_dir):
    m = mission()
    for seed in range(30):
        m.rng = random.Random(seed)
        kind = m._enemy_kind("guard")
        assert not TRAITS[kind.id].temper.chase_forever, kind.id


def test_a_hunter_slot_gets_a_hunting_kind(state_dir):
    m = mission("queen", won=("territory", "ember", "obsidian"))
    for seed in range(30):
        m.rng = random.Random(seed)
        assert set(m._enemy_kind("hunter").tags) & {"hunter", "fast"}


def test_an_editor_map_can_choose_its_enemy_teamwork(state_dir):
    from desktop_bug.app import custom_maps

    data = custom_maps.clean_map({"title": "Mine", "tactics": 3})
    assert data["tactics"] == 3
    assert custom_maps.clean_map({"title": "Mine"})["tactics"] == -1, "by the tier unless chosen"
