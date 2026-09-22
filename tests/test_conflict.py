"""Conflict: a fight can hurt, and losing it is fatal (DC-22, DC-47).

`hp`, `armor` and `damage` have been level-scaled stats on a spider since the
progression work, and `take_damage`/`heal` have existed beside them -- but
until DC-22 nothing in the running application ever called them. Only a test
did. Conflict is what finally uses them.

DC-47 then reversed what losing costs. A beaten spider used to be knocked out
and recover; it now dies, leaves a carcass, and the carcass is eaten and gone
shortly after. These tests were rewritten for that rather than replaced,
because what most of them guard -- that the gate is the only damage path,
that friends never fight, that a fight reads as exchanges -- did not change.

The safety property still matters more than the feature: with conflict off,
nothing anywhere may reduce a creature's hp. That is asserted two ways, by
simulation and against the source, because a future package adding a second
damage path would otherwise silently escape the switch.
"""

from __future__ import annotations

import json
import math
import re
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.world.carcass import CARCASS_LIFETIME, Carcass
from support import ROOT

DT = 1.0 / 60.0
SCREEN = (1000, 700)
AWAY = (-5000.0, -5000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _brawl(monkeypatch, conflict=None):
    """Two spiders on mutually hostile teams, standing on each other."""
    base = Path(tempfile.mkdtemp(prefix="dc22-conflict-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(base / "state"))
    # `team_relations` is the preset key; stances are stored in both
    # directions, so declaring it once is enough.
    settings = {"flies": {"enabled": False, "spawner": False},
                "team_relations": {"pack_a": {"pack_b": "foe"}}}
    if conflict is not None:
        settings["conflict"] = conflict
    preset = base / "brawl.json"
    preset.write_text(json.dumps({
        "name": "brawl",
        "slots": [
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "a", "team": "pack_a", "job": "none"},
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "b", "team": "pack_b", "job": "none"},
        ],
        "settings": settings,
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=6)
    assert len(manager.creatures) == 2
    return manager


def _press_together(manager):
    """Hold the two spiders in contact, whatever their behaviour wants."""
    if len(manager.creatures) < 2:
        return
    first, second = manager.creatures[0], manager.creatures[1]
    first.x, first.y = 500.0, 350.0
    second.x, second.y = 500.0 + first.size * 0.4, 350.0


def _run(manager, seconds, keep_together=True):
    for _ in range(int(seconds * 60)):
        if keep_together:
            _press_together(manager)
        manager.update(DT, *AWAY)


def test_conflict_is_on_by_default():
    """The audience for conflict is the user who watches it; off would hide it."""
    manager = CreatureManager(ROOT / "presets" / "colony.json", *SCREEN, seed=1)
    assert manager.conflict_enabled is True


def test_touching_foes_hurt_each_other(monkeypatch):
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    assert first.relation_to(second) == "foe"
    before = (first.hp, second.hp)
    _run(manager, 3.0)
    assert first.hp < before[0] or second.hp < before[1], (first.hp, second.hp)


def test_friends_standing_together_never_fight(monkeypatch):
    """Only declared foes trade blows; crowding a teammate is not an attack."""
    base = Path(tempfile.mkdtemp(prefix="dc22-friends-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(base / "state"))
    preset = base / "friends.json"
    preset.write_text(json.dumps({
        "name": "friends",
        "slots": [{"model": "tarantula", "personality": "mellow", "count": 2,
                   "slot_id": "a", "team": "pack_a", "job": "none"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=6)
    _run(manager, 4.0)
    assert all(c.hp == c.max_hp for c in manager.creatures)
    assert not any(c.dead for c in manager.creatures)
    assert manager.carcasses == []


def test_a_loser_dies_and_leaves_a_carcass(monkeypatch):
    """DC-47: being beaten is fatal, and leaves something behind."""
    manager = _brawl(monkeypatch)
    second = manager.creatures[1]
    where = (second.x, second.y)
    second.hp = 1.0
    _run(manager, 2.0)

    assert second.dead, second.hp
    assert second not in manager.creatures, "the beaten spider is still in the colony"
    assert len(manager.creatures) == 1
    assert len(manager.carcasses) == 1
    remains = manager.carcasses[0]
    assert math.hypot(remains.x - second.x, remains.y - second.y) < 1.0, (
        f"remains at {(remains.x, remains.y)}, spider fell at {(second.x, second.y)} "
        f"(started {where})"
    )


def test_a_carcass_is_eaten_and_gone(monkeypatch):
    """"Soon disappears, as it is being eaten" -- so it must actually go."""
    manager = _brawl(monkeypatch)
    manager.creatures[1].hp = 1.0
    _run(manager, 2.0)
    assert manager.carcasses

    _run(manager, CARCASS_LIFETIME + 1.0, keep_together=False)
    assert manager.carcasses == [], "the remains are still lying there"


def test_a_carcass_nobody_touches_still_goes(monkeypatch):
    """Nothing may litter the desktop permanently, eaten or not."""
    manager = _brawl(monkeypatch)
    manager.creatures[1].hp = 1.0
    _run(manager, 2.0)
    assert manager.carcasses
    survivor = manager.creatures[0]
    survivor.x, survivor.y = 60.0, 60.0
    manager.carcasses[0].x, manager.carcasses[0].y = 900.0, 620.0
    _run(manager, CARCASS_LIFETIME + 1.0, keep_together=False)
    assert manager.carcasses == []


def test_being_eaten_is_faster_than_being_left():
    """Otherwise "as it is being eaten" is just a word for a fade-out.

    Tested on the carcass itself rather than through a brawl: in a real
    fight the winner is standing on the body it just made, so it starts
    being eaten immediately and both cases finish at the same time.
    """
    eaten = Carcass(100.0, 100.0, 18.0, "pack_a")
    ignored = Carcass(300.0, 300.0, 18.0, "pack_a")
    for _ in range(60):
        eaten.update(DT, eaters=1)
        ignored.update(DT, eaters=0)
    assert eaten.spent > ignored.spent * 2.0, (eaten.spent, ignored.spent)
    assert ignored.spent > 0.0, "a carcass nobody touches must still be going"


def test_a_crowd_does_not_make_a_carcass_vanish_instantly():
    """Feeding does not stack; the whole colony arriving is still a moment."""
    one = Carcass(0.0, 0.0, 18.0, "pack_a")
    many = Carcass(0.0, 0.0, 18.0, "pack_a")
    for _ in range(30):
        one.update(DT, eaters=1)
        many.update(DT, eaters=6)
    assert many.spent == pytest.approx(one.spent)


def test_death_takes_the_saved_profile_with_it(monkeypatch):
    """Otherwise dying costs nothing that survives a restart."""
    manager = _brawl(monkeypatch)
    second = manager.creatures[1]
    key = second.progression_id
    manager._progression_states[key] = {"name": "Doomed", "progression": {"level": 9}}
    second.hp = 1.0
    _run(manager, 2.0)
    assert second.dead
    assert key not in manager._progression_states, (
        "a dead spider's level would be inherited by whoever fills its slot"
    )


def test_a_dying_spider_is_not_a_target_in_its_last_frame(monkeypatch):
    """It is swept out at the end of the tick, not the instant hp hits zero."""
    manager = _brawl(monkeypatch)
    second = manager.creatures[1]
    second._die()
    assert second.dead
    assert not manager._creature_can_hunt(second)


def test_with_conflict_off_nothing_loses_hp(monkeypatch):
    manager = _brawl(monkeypatch, conflict=False)
    assert manager.conflict_enabled is False
    _run(manager, 6.0)
    assert all(c.hp == c.max_hp for c in manager.creatures)
    assert not any(c.dead for c in manager.creatures)
    assert manager.carcasses == []


def test_the_switch_is_the_only_thing_standing_between_off_and_damage():
    """Asserted against the source, not just by simulation.

    A second damage path added elsewhere would pass the test above -- two
    peaceful spiders would still be unharmed -- while quietly escaping the
    setting. So the rule is structural: `take_damage` has exactly one caller
    outside the creature itself, and that caller lives behind the gate.
    """
    src = ROOT / "src" / "desktop_bug"
    callers = []
    for path in src.rglob("*.py"):
        for line in path.read_text(encoding="utf-8").splitlines():
            if re.search(r"\.take_damage\(", line) and "def take_damage" not in line:
                callers.append(path.relative_to(src).as_posix())
    assert set(callers) == {"manager/combat.py"}, (
        f"take_damage is called from {sorted(set(callers))}; every damage path must "
        "sit behind the conflict gate in manager/combat.py"
    )
    gate = (src / "manager" / "combat.py").read_text(encoding="utf-8")
    assert "if not self.conflict_enabled:" in gate


def test_turning_conflict_off_still_clears_away_remains(monkeypatch):
    """Switching it off must not strand bodies on the desktop."""
    manager = _brawl(monkeypatch)
    manager.creatures[1].hp = 1.0
    _run(manager, 2.0)
    assert manager.carcasses

    manager.set_conflict_enabled(False)
    _run(manager, CARCASS_LIFETIME + 1.0, keep_together=False)
    assert manager.carcasses == [], "remains stranded when conflict was switched off"


def test_a_fight_is_exchanges_rather_than_a_drain(monkeypatch):
    """Hits land on a cooldown, so a brawl is legible rather than instant."""
    manager = _brawl(monkeypatch)
    second = manager.creatures[1]
    start = second.hp
    _run(manager, 1.0)
    lost_in_one_second = start - second.hp
    assert lost_in_one_second > 0.0
    # At one exchange per ~0.85 s a second cannot cost more than a couple of
    # hits; draining a full-health spider in under a second would not read.
    assert lost_in_one_second < second.max_hp * 0.5, lost_in_one_second


def test_eating_a_carcass_feeds_the_eaters_colony(monkeypatch):
    """A dead spider is food, in the same banked resource DC-21 spends."""
    manager = _brawl(monkeypatch)
    survivor = manager.creatures[0]
    site = manager.base_world.ensure_site(survivor)
    site.resources = 0.0
    manager.creatures[1].hp = 1.0
    _run(manager, 2.0)
    assert manager.carcasses

    remains = manager.carcasses[0]
    for _ in range(int((CARCASS_LIFETIME + 1.0) * 60)):
        survivor.x, survivor.y = remains.x, remains.y
        manager.update(DT, *AWAY)
        if not manager.carcasses:
            break
    assert manager.carcasses == []
    assert site.resources > 0.0, "eating a whole spider fed its eater's team nothing"
