"""Headless checks for XP, growth, equipment, resources, and relations."""

from __future__ import annotations

import random

import pytest
from desktop_bug.creature import Creature
from desktop_bug.state.progression import MAX_LEVEL, ProgressionState, xp_to_next_level
from support import load_pair


@pytest.fixture
def pair():
    """Two spiders from the same model, with distinct progression identities."""
    random.seed(7)
    model, personality = load_pair()
    return (
        Creature(model, personality, 1280, 720, index=0, progression_id="smoke:0"),
        Creature(model, personality, 1280, 720, index=1, progression_id="smoke:1"),
    )


@pytest.fixture
def spider(pair):
    return pair[0]


def test_a_new_spider_starts_at_the_bottom(spider):
    assert spider.level == 1 and spider.xp == 0


def test_a_level_arrives_exactly_at_the_threshold(spider):
    assert spider.gain_experience(xp_to_next_level(1) - 1, "test") == []
    assert spider.level == 1
    assert spider.gain_experience(1, "test")
    assert spider.level == 2


def test_growth_follows_levels(spider):
    old_size = spider.size
    old_speed = spider._speed_mult()
    spider.gain_experience(sum(xp_to_next_level(level) for level in range(1, 5)) + 1, "fly")
    assert spider.size > old_size and spider._speed_mult() > old_speed


def test_one_award_may_cross_several_thresholds_but_not_the_ceiling(spider):
    events = spider.gain_experience(
        sum(xp_to_next_level(level) for level in range(1, 5)) + 1, "fly"
    )
    assert spider.level >= 4 and events
    assert spider.level <= MAX_LEVEL


def test_a_talent_costs_a_point_and_raises_a_stat(spider):
    """DC-57 made this happen on its own, so this checks it happened.

    The spider used to bank a point per level and wait for someone to open
    the inspector and spend it, which no spider in a colony was ever going to
    get. Levelling now spends them, so the test that used to call
    `unlock_progression_ability` by hand watches the level-up do it -- and
    then checks that the manual API, which the inspector still calls, refuses
    an already-unlocked node rather than charging for it twice.
    """
    before_hp = spider.max_hp
    before_points = spider.progression.skill_points
    spider.gain_experience(sum(xp_to_next_level(level) for level in range(1, 5)) + 1, "fly")

    assert "vitality" in spider.progression.unlocked_abilities
    assert spider.max_hp > before_hp
    # Points were spent, not merely banked.
    assert spider.progression.skill_points < before_points + 4

    ok, message = spider.unlock_progression_ability("vitality")
    assert ok is False
    assert "already" in message.lower(), message


def test_worn_equipment_absorbs_damage(spider):
    assert spider.add_inventory_item("fluffy_mantle")
    ok, _ = spider.equip_item("fluffy_mantle")
    assert ok and spider.armor > 0
    hp_before = spider.hp
    assert spider.take_damage(20) < 20 and spider.hp < hp_before


def test_energy_stays_within_its_bounds(spider):
    assert spider.spend_energy(10)
    assert 0 <= spider.energy <= spider.max_energy


@pytest.mark.parametrize(
    "their_team,expected",
    [
        pytest.param("pack", "friend", id="same-team"),
        # Two ordinary teams stay unrelated until a preset declares a stance,
        # which keeps an ordinary scene peaceful.
        pytest.param("pack_b", "neutral", id="two-ordinary-teams"),
        # "rivals" is hostile by default, so picking it in the settings window
        # means something without editing relations pair by pair.
        pytest.param("rivals", "foe", id="rivals-are-hostile-by-default"),
    ],
)
def test_a_team_decides_the_default_relation(pair, their_team, expected):
    spider, other = pair
    spider.set_team("pack")
    other.set_team(their_team)
    assert spider.relation_to(other) == expected


@pytest.mark.parametrize("override", ["friend", "foe"])
def test_a_pair_choice_outranks_the_team(pair, override):
    spider, other = pair
    spider.set_team("pack")
    other.set_team("rivals")
    spider.progression.relation_overrides[other.progression_id] = override
    assert spider.relation_to(other) == override


def test_progression_survives_a_round_trip(spider):
    state = ProgressionState.from_dict(spider.progression.to_dict())
    assert state.to_dict() == spider.progression.to_dict()


def test_experience_stops_at_the_last_level(spider):
    spider.progression.level = MAX_LEVEL
    spider.gain_experience(999999, "test")
    assert spider.level == MAX_LEVEL and spider.xp == xp_to_next_level(MAX_LEVEL)
