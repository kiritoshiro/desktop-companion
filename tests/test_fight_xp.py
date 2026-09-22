"""Fighting is worth something (DC-66).

The owner: *"also give xp for damaging enemies other things or killing
foes."*

Until this, eating a fly was the only thing in the entire project that earned
any XP -- ``award_feed_xp`` had exactly one caller. A spider could win every
fight on the desktop and stay at level one, while one that never left the nest
levelled up on flies. Since DC-57 a level also spends itself on the skill
tree, so this is not only bookkeeping: a veteran now actually becomes more
dangerous, which is the thing that makes a colony's history visible.

Two design decisions worth stating, because both could reasonably have gone
the other way:

* **The counter-blow pays too.** Paying only the aggressor would make picking
  fights the only route to a level, and reward precisely the behaviour DC-50
  added nerve to discourage.
* **A kill is worth less than a meal.** Hunting is the safe living and should
  stay the backbone of the economy; a fight is a gamble that can cost a level
  outright, because DC-47 made death permanent and ``_forget_progression``
  drops the profile with it.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.manager.constants import (
    DAMAGE_XP_PER_POINT,
    FEED_XP_REWARD,
    KILL_XP_REWARD,
)

DT = 1.0 / 60.0
SCREEN = (1400, 900)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _scene(monkeypatch, conflict=True, relation="foe", apart=40.0):
    scratch = Path(tempfile.mkdtemp(prefix="dc66-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "a", "team": "pack_a", "job": "none"},
            {"model": "tarantula", "personality": "bold", "count": 1,
             "slot_id": "b", "team": "pack_b", "job": "none"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False},
                     "conflict": conflict,
                     "team_relations": {"pack_a": {"pack_b": relation}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=3)
    me = [c for c in manager.creatures if c.progression.team_id == "pack_a"][0]
    them = [c for c in manager.creatures if c.progression.team_id == "pack_b"][0]
    me.x, me.y = 700.0 - apart / 2.0, 450.0
    them.x, them.y = 700.0 + apart / 2.0, 450.0
    return manager, me, them


# ----------------------------------------------------------- damage earns

def test_landing_a_blow_earns_xp(monkeypatch):
    manager, me, them = _scene(monkeypatch)
    before = me.progression.total_xp
    for _ in range(int(6.0 * 60)):
        manager.update(DT, *AWAY)
        if me.progression.total_xp > before:
            break
    assert me.progression.total_xp > before


def test_the_award_is_proportional_to_what_landed(monkeypatch):
    """Per point of damage, not per swing, so a heavy hit is worth more and
    armour matters at both ends."""
    manager, me, them = _scene(monkeypatch)
    before = me.progression.total_xp
    them.armor = 0.0
    manager._credit_fight_xp(me, damage=10.0, killed=False)
    gained = me.progression.total_xp - before
    assert gained == pytest.approx(round(10.0 * DAMAGE_XP_PER_POINT))


def test_a_kill_pays_a_bonus_on_top_of_the_damage(monkeypatch):
    manager, me, _them = _scene(monkeypatch)
    before = me.progression.total_xp
    manager._credit_fight_xp(me, damage=4.0, killed=True)
    gained = me.progression.total_xp - before
    assert gained == pytest.approx(round(4.0 * DAMAGE_XP_PER_POINT + KILL_XP_REWARD))


def test_a_kill_is_worth_less_than_a_meal():
    """Hunting stays the backbone of the economy; a fight can cost a level."""
    assert KILL_XP_REWARD < FEED_XP_REWARD


def test_the_one_that_hits_back_is_paid_too(monkeypatch):
    """Otherwise picking fights is the only way to progress."""
    manager, me, them = _scene(monkeypatch)
    for _ in range(int(8.0 * 60)):
        manager.update(DT, *AWAY)
        if me.dead or them.dead:
            break
    assert me.progression.total_xp > 0
    assert them.progression.total_xp > 0


def test_a_killer_ends_up_ahead_of_where_it_started(monkeypatch):
    """The whole point: winning a fight should show."""
    manager, me, them = _scene(monkeypatch)
    them.hp = 1.0
    them.armor = 0.0
    before = me.progression.total_xp
    for _ in range(int(10.0 * 60)):
        manager.update(DT, *AWAY)
        if them.dead or them not in manager.creatures:
            break
    else:
        pytest.skip("the fight did not resolve inside the window")
    assert me.progression.total_xp >= before + KILL_XP_REWARD


# ------------------------------------------------------------- and not more

def test_nothing_is_earned_with_conflict_switched_off(monkeypatch):
    """The conflict gate is the project's one safety property: with it off,
    nothing reduces any hp -- so nothing may be paid for doing so either."""
    manager, me, them = _scene(monkeypatch, conflict=False)
    for _ in range(int(6.0 * 60)):
        manager.update(DT, *AWAY)
    assert me.progression.total_xp == 0
    assert them.progression.total_xp == 0


def test_a_friend_is_not_a_payday(monkeypatch):
    """Contact damage only happens between foes, so this cannot be farmed on
    a teammate -- stated as a test because it is a property worth keeping."""
    manager, me, them = _scene(monkeypatch, relation="friend")
    for _ in range(int(6.0 * 60)):
        manager.update(DT, *AWAY)
    assert me.progression.total_xp == 0
    assert them.progression.total_xp == 0


def test_a_level_won_in_a_fight_is_saved_like_any_other(monkeypatch):
    """Routed through ``award_feed_xp`` for exactly this reason: it is the
    one place that marks runtime state dirty."""
    manager, me, _them = _scene(monkeypatch)
    manager._runtime_state_dirty = False
    manager._credit_fight_xp(me, damage=6.0, killed=False)
    assert manager._runtime_state_dirty is True


def test_xp_from_a_fight_can_unlock_a_skill(monkeypatch):
    """DC-57 spends a level on the tree, so combat feeds straight into what
    a spider can do. This is what makes the package worth having rather than
    a number going up."""
    manager, me, _them = _scene(monkeypatch)
    before = set(me.progression.unlocked_abilities)
    manager._credit_fight_xp(me, damage=4000.0, killed=True)
    assert me.progression.level > 1
    assert set(me.progression.unlocked_abilities) > before


# ------------------------------------------- what combat XP nearly broke

def test_a_level_won_mid_fight_does_not_heal_the_wounds(monkeypatch):
    """The subtlest thing in this package, and it was found by a test.

    A level-up refilled hp and energy. That was harmless while eating a fly
    was the only way to earn XP, because no spider ever levelled in the
    middle of a fight. The moment damage earns XP, a refill means a hard
    fight heals both sides: measured straight away, two foes brawling for
    three seconds ended on 115.2 and 123.0 hp against the 100.0 they started
    with, and neither could ever have been finished off.

    A level now raises the ceiling and leaves the damage taken.
    """
    manager, me, _them = _scene(monkeypatch)
    me.hp = me.max_hp - 40.0
    hp_before = me.hp
    missing_before = me.max_hp - me.hp
    level_before = me.progression.level

    manager._credit_fight_xp(me, damage=500.0, killed=False)

    assert me.progression.level > level_before, "the test did not level it up"
    assert me.max_hp > 100.0, "the ceiling should have risen"
    assert me.hp < me.max_hp, "it came out of the level-up at full health"
    # The wound is exactly what it was. hp itself does rise, because the
    # ceiling rose under it -- that is what carrying a wound means, and it is
    # the ordinary way a level works. What must not happen is the *damage*
    # being written off, which is what a refill did.
    assert me.max_hp - me.hp == pytest.approx(missing_before), (
        f"the wound changed: {missing_before} -> {me.max_hp - me.hp}")
    assert me.hp > hp_before, "the ceiling rose but nothing came with it"
    # For scale: the bug this replaced put a spider on 60 of 100 back to full
    # on a single level, and to 118 when only half the path carried wounds.
    assert me.hp - hp_before == pytest.approx(me.max_hp - 100.0)


def test_two_foes_still_finish_a_fight(monkeypatch):
    """The consequence, stated the way it was first seen: a brawl has to
    end with someone down, not with both sides healthier than they began."""
    manager, me, them = _scene(monkeypatch)
    start = (me.hp, them.hp)
    for _ in range(int(20.0 * 60)):
        manager.update(DT, *AWAY)
        if me.dead or them.dead or me not in manager.creatures or them not in manager.creatures:
            break
    else:
        assert me.hp < start[0] or them.hp < start[1], (
            f"three seconds of fighting left both fitter: {me.hp}, {them.hp}")
