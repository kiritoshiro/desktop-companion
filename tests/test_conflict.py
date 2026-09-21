"""Conflict: a fight can hurt, end, and be recovered from (DC-22).

`hp`, `armor` and `damage` have been level-scaled stats on a spider since the
progression work, and `take_damage`/`heal` have existed alongside them -- but
until this package nothing in the running application ever called them. Only
a test did. Conflict is what finally uses them.

The safety property matters more than the feature: with conflict off, nothing
anywhere may reduce a creature's hp. That is asserted two ways here, by
simulation and against the source, because a future package adding a second
damage path would otherwise silently escape the switch.

Pet tone is a constraint, not a preference: a spider that loses is knocked
out and comes back. Nothing removes a spider permanently, and a test says so.
"""

from __future__ import annotations

import json
import math
import re
import tempfile
from pathlib import Path

import pytest
from desktop_bug.creature.constants import KNOCKOUT_RECOVERY_FRACTION, KNOCKOUT_SECONDS
from desktop_bug.manager import CreatureManager
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
    first, second = manager.creatures
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
    assert not any(c.knocked_out for c in manager.creatures)


def test_a_loser_is_knocked_out_and_comes_back(monkeypatch):
    """The pet tone: no spider is ever removed, only sidelined."""
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    second.hp = 1.0
    _run(manager, 2.0)
    assert second.knocked_out, second.hp
    assert len(manager.creatures) == 2, "a spider was removed from the scene"

    # Downed: still present, but doing nothing and decided about by nobody.
    assert second.state == "Downed"
    assert second.motion_paused
    assert first.relation_to(second) == "foe"

    _run(manager, KNOCKOUT_SECONDS + 1.5, keep_together=False)
    assert not second.knocked_out, "it never got back up"
    assert second.hp == pytest.approx(second.max_hp * KNOCKOUT_RECOVERY_FRACTION, rel=0.02)
    assert len(manager.creatures) == 2


def test_a_downed_spider_is_not_a_target(monkeypatch):
    """Nothing should keep attacking, hunting or reporting something face-down."""
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    second.hp = 1.0
    _run(manager, 2.0)
    assert second.knocked_out
    floor = second.hp
    _run(manager, 3.0)
    assert second.hp == floor, "a knocked-out spider kept taking hits"
    assert not manager._creature_can_hunt(second)


def test_with_conflict_off_nothing_loses_hp(monkeypatch):
    manager = _brawl(monkeypatch, conflict=False)
    assert manager.conflict_enabled is False
    _run(manager, 6.0)
    assert all(c.hp == c.max_hp for c in manager.creatures)
    assert not any(c.knocked_out for c in manager.creatures)


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
        text = path.read_text(encoding="utf-8")
        for line in text.splitlines():
            if re.search(r"\.take_damage\(", line) and "def take_damage" not in line:
                callers.append(path.relative_to(src).as_posix())
    assert set(callers) == {"manager/combat.py"}, (
        f"take_damage is called from {sorted(set(callers))}; every damage path must "
        "sit behind the conflict gate in manager/combat.py"
    )
    gate = (src / "manager" / "combat.py").read_text(encoding="utf-8")
    assert "if not self.conflict_enabled:" in gate


def test_turning_conflict_off_still_lets_the_downed_recover(monkeypatch):
    """Switching it off must not strand someone face-down forever."""
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    second.hp = 1.0
    _run(manager, 2.0)
    assert second.knocked_out

    manager.set_conflict_enabled(False)
    _run(manager, KNOCKOUT_SECONDS + 1.5, keep_together=False)
    assert not second.knocked_out, "it was stranded when conflict was switched off"


def test_a_fight_is_exchanges_rather_than_a_drain(monkeypatch):
    """Hits land on a cooldown, so a brawl is legible rather than instant."""
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    start = second.hp
    _run(manager, 1.0)
    lost_in_one_second = start - second.hp
    assert lost_in_one_second > 0.0
    # At one exchange per ~0.85 s a second cannot cost more than a couple of
    # hits; draining a full-health spider in under a second would not read.
    assert lost_in_one_second < second.max_hp * 0.5, lost_in_one_second


def test_a_spider_recovers_at_its_own_base_when_its_team_holds_one(monkeypatch):
    manager = _brawl(monkeypatch)
    first, second = manager.creatures
    site = manager.base_world.ensure_site(second)
    site.x, site.y = 120.0, 120.0
    second.hp = 1.0
    _run(manager, 2.0)
    assert second.knocked_out
    # Measured on the frame it comes back, not later: once up it wanders off
    # like any other spider, which says nothing about where it reappeared.
    came_back_at = None
    for _ in range(int((KNOCKOUT_SECONDS + 3.0) * 60)):
        manager.update(DT, *AWAY)
        if not second.knocked_out:
            came_back_at = (second.x, second.y)
            break
    assert came_back_at is not None, "it never got back up"
    home = math.hypot(came_back_at[0] - site.x, came_back_at[1] - site.y)
    assert home < site.radius + 60.0, f"came back {home:.0f}px from its base"
