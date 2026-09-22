"""Spiders fight with the skills they have (DC-45).

DC-22 made conflict hurt, but only by accident: two foes damaged each other
when they happened to collide while doing something unrelated. Nothing
sought a fight, and nothing used an ability in one.

A spider now engages a foe with its own kit -- silk to pin it from range, a
pounce to close, a chase or an approach otherwise. Which skills it has is
already data (DC-19), so two spiders fight visibly differently without a
single personality-id branch, and that is what these tests check: not "the
web-shooter does X" spelled out per personality, but that the behaviour
follows the skill set.

Pinning is the payoff for spending a shot: a webbed spider is slowed and
easier to hit.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.creature.constants import (
    WEBBED_HOLD_SECONDS,
    WEBBED_SECONDS,
    WEBBED_SPEED_MULT,
)
from desktop_bug.manager import CreatureManager
from desktop_bug.manager.combat import WEBBED_DAMAGE_BONUS

DT = 1.0 / 60.0
SCREEN = (1200, 800)
AWAY = (-5000.0, -5000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _duel(monkeypatch, left_personality="hunter", right_personality="mellow",
          conflict=True, apart=90.0):
    """One spider of each hostile team, a settable distance apart."""
    base = Path(tempfile.mkdtemp(prefix="dc45-duel-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(base / "state"))
    preset = base / "duel.json"
    preset.write_text(json.dumps({
        "name": "duel",
        "slots": [
            {"model": "tarantula", "personality": left_personality, "count": 1,
             "slot_id": "a", "team": "pack_a", "job": "none"},
            {"model": "tarantula", "personality": right_personality, "count": 1,
             "slot_id": "b", "team": "pack_b", "job": "none"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False},
                     "conflict": conflict,
                     "team_relations": {"pack_a": {"pack_b": "foe"}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=8)
    left, right = manager.creatures
    left.x, left.y = 600.0 - apart / 2.0, 400.0
    right.x, right.y = 600.0 + apart / 2.0, 400.0
    return manager, left, right


def test_a_spider_is_given_the_foe_it_is_fighting(monkeypatch):
    manager, left, right = _duel(monkeypatch)
    manager.update(DT, *AWAY)
    assert left._foe is right
    assert right._foe is left


def test_a_distant_foe_is_not_engaged(monkeypatch):
    """Spiders fight when they meet, rather than charging across the desk."""
    manager, left, right = _duel(monkeypatch, apart=1100.0)
    left.x, left.y = 40.0, 40.0
    right.x, right.y = 1160.0, 760.0
    manager.update(DT, *AWAY)
    assert left._foe is None
    assert right._foe is None


def test_with_conflict_off_nobody_is_given_a_foe(monkeypatch):
    manager, left, right = _duel(monkeypatch, conflict=False)
    for _ in range(30):
        manager.update(DT, *AWAY)
    assert left._foe is None and right._foe is None


def test_a_spider_closes_on_its_foe(monkeypatch):
    """Engaging means actually going after it, not waiting to collide."""
    manager, left, right = _duel(monkeypatch, apart=150.0)
    start = math.hypot(left.x - right.x, left.y - right.y)
    closest = start
    for _ in range(int(6.0 * 60)):
        manager.update(DT, *AWAY)
        if left.dead or right.dead:
            break
        closest = min(closest, math.hypot(left.x - right.x, left.y - right.y))
    assert closest < start * 0.6, (start, closest)


def test_a_web_shooter_pins_its_foe_before_closing(monkeypatch):
    """The headline skill: trapping, used in a fight rather than only on flies."""
    manager, left, right = _duel(monkeypatch, apart=170.0)
    for creature in manager.creatures:
        creature.skills.set_enabled("shoot_web", True)
    # Give the shooter every chance to take the shot rather than relying on
    # a personality's own roll landing inside the window.
    left.personality["web_shot_chance"] = 1.0
    left.personality["web_shot_range_mult"] = 1.2
    # A spider spawns with the shared 8-18 s web-shot cooldown already
    # running, and a fight is over long before that expires -- so without
    # clearing it here this would only be testing the cooldown. That the
    # cooldown makes trapping rare in a real fight is a live balance
    # question, recorded with the package rather than tuned away here.
    left.web_shot_cooldown = 0.0

    webbed = False
    for _ in range(int(12.0 * 60)):
        manager.update(DT, *AWAY)
        if right.webbed:
            webbed = True
            break
        if right.dead or left.dead:
            break
    assert webbed, "a web-shooter never pinned the foe it was fighting"


def test_a_foe_can_be_webbed_with_mouse_trapping_turned_off(monkeypatch):
    """Silk at a spider never touches the pointer, so that switch has no say.

    The first version of this gated a shot at a foe on the *cursor* rules,
    which meant a web-shooter could never pin an enemy whenever mouse
    trapping was off -- and the test above did not catch it, because that
    setting happens to be on by default.
    """
    manager, left, right = _duel(monkeypatch, apart=170.0)
    manager.set_allow_mouse_capture(False)
    assert manager.mouse_web_world.enabled is False
    for creature in manager.creatures:
        creature.skills.set_enabled("shoot_web", True)
    left.personality["web_shot_chance"] = 1.0
    left.personality["web_shot_range_mult"] = 1.2
    left.web_shot_cooldown = 0.0

    for _ in range(int(12.0 * 60)):
        manager.update(DT, *AWAY)
        if right.webbed:
            break
        if right.dead or left.dead:
            break
    assert right.webbed, "mouse trapping being off stopped a spider webbing a spider"


def test_a_spider_with_no_silk_never_fires_any(monkeypatch):
    """Behaviour follows the skill set, not the personality's name."""
    manager, left, right = _duel(monkeypatch, apart=150.0)
    for creature in manager.creatures:
        creature.skills.set_enabled("shoot_web", False)
        creature.skills.set_enabled("wall_web", False)
    for _ in range(int(8.0 * 60)):
        manager.update(DT, *AWAY)
        if left.dead or right.dead:
            break
        assert not right.webbed and not left.webbed
        assert manager.fly_world.projectiles == []


def test_being_webbed_holds_a_spider_then_slows_it():
    """Silk has to cost the thing it lands on something.

    DC-45 only slowed a webbed spider, to WEBBED_SPEED_MULT. Watching a
    colony, the owner reported that webbing another spider "does not
    imobalise him like the mouse" -- and they were right: a spider at a
    third speed still walks off looking unbothered, so the skill read as
    nothing at all. DC-50 holds it outright for the front of the pin and
    only slows it once it has worked partly free.
    """
    from desktop_bug.creature.core import Creature
    import json as _json
    root = Path(__file__).resolve().parents[1]
    model = _json.loads((root / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    traits = _json.loads((root / "personalities" / "mellow.json").read_text(encoding="utf-8"))
    spider = Creature(model, traits, *SCREEN, index=0)
    free = spider._speed_mult()
    spider.web_pinned("trap")
    assert spider.webbed
    assert spider.webbed_held, "fresh silk should hold, not merely slow"
    assert spider._speed_mult() == 0.0

    # Worked partly free: still hampered, but moving again.
    spider.webbed_timer = WEBBED_SECONDS - WEBBED_HOLD_SECONDS - 0.01
    assert not spider.webbed_held
    assert spider.webbed
    assert spider._speed_mult() == pytest.approx(free * WEBBED_SPEED_MULT)


def test_silk_wears_off(monkeypatch):
    """Being pinned is a setback, not a sentence."""
    manager, left, right = _duel(monkeypatch)
    right.web_pinned("trap", left)
    assert right.webbed
    for _ in range(int((WEBBED_SECONDS + 1.0) * 60)):
        manager.update(DT, *AWAY)
        if right.dead:
            pytest.skip("the fight ended before the silk did")
    assert not right.webbed
    assert right.webbed_by is None


def test_a_pinned_foe_takes_a_harder_hit(monkeypatch):
    """The payoff for spending a shot instead of walking up and biting."""
    manager, left, right = _duel(monkeypatch, apart=10.0)
    plain = right.hp
    manager._trade_blow(left, right)
    plain_loss = plain - right.hp

    right.hp = right.max_hp
    left.attack_cooldown = 0.0
    right.web_pinned("trap", left)
    manager._trade_blow(left, right)
    pinned_loss = right.max_hp - right.hp

    assert pinned_loss > plain_loss
    assert pinned_loss == pytest.approx(plain_loss * WEBBED_DAMAGE_BONUS, rel=0.05)


def test_a_dead_foe_is_dropped_as_a_target(monkeypatch):
    manager, left, right = _duel(monkeypatch, apart=40.0)
    manager.update(DT, *AWAY)
    assert left._foe is right
    right._die()
    manager.update(DT, *AWAY)
    assert left._foe is None
    assert right not in manager.creatures


def test_silk_cannot_pin_a_spider_that_is_already_dead(monkeypatch):
    manager, left, right = _duel(monkeypatch)
    right._die()
    right.web_pinned("trap", left)
    assert not right.webbed


def test_hunting_a_fly_still_wins_over_fighting(monkeypatch):
    """A fly is food and will leave; a foe will still be there in a moment.

    Tested through `_creature_focus`, which is where the precedence actually
    lives: whatever it returns is what Chase and Approach steer at, so it
    decides which of the two a spider is really going after.
    """
    manager, left, right = _duel(monkeypatch, apart=60.0)
    manager.update(DT, *AWAY)
    assert left._foe is right

    # Foe only: the fight is what it steers at.
    fx, fy, hunting = manager._creature_focus(left, *AWAY)
    assert (fx, fy) == (right.x, right.y)
    assert hunting is False

    class _Fly:
        x, y, alive, eaten = 123.0, 456.0, True, False

    left._prey = _Fly()
    fx, fy, hunting = manager._creature_focus(left, *AWAY)
    assert (fx, fy) == (123.0, 456.0), "a foe outranked a live fly"
    assert hunting is True
