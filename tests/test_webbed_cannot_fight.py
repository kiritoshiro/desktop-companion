"""A spider trapped in silk cannot fight back.

The owner: *"when trapped by web, shouldn't be able to fight back."* Held by
the net (#84) it could not move or turn, but it still traded blows in contact,
hit back when struck, fired silk and pounced.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.app.adventure import PlayerController
from desktop_bug.manager import CreatureManager

DT = 1.0 / 60.0
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


@pytest.fixture
def duel(monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="nofight-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "f.json"
    preset.write_text(json.dumps({
        "name": "f",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1, "slot_id": "a", "team": "hunters"},
            {"model": "tarantula", "personality": "bold", "count": 1, "slot_id": "b", "team": "rivals"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False}, "conflict": True,
                     "team_relations": {"hunters": {"rivals": "foe"}}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, 420, 300, seed=6)
    first, second = manager.creatures
    for creature in (first, second):
        creature.max_hp = creature.hp = 100000.0
        creature.attack_cooldown = 0.0
    first.x, first.y, second.x, second.y = 200.0, 150.0, 215.0, 150.0
    return manager, first, second


def test_a_webbed_spider_lands_no_blow(duel):
    manager, first, second = duel
    first.web_pinned("trap", second)
    manager._trade_blow(first, second)
    assert second.hp == second.max_hp
    assert first.strike_clock is None, "no bite animation either"


def test_a_webbed_spider_does_not_hit_back(duel):
    manager, first, second = duel
    second.web_pinned("trap", first)
    manager._trade_blow(first, second)
    assert second.hp < second.max_hp, "the free spider's blow lands"
    assert first.hp == first.max_hp, "the webbed one cannot answer it"


def test_in_contact_only_the_free_spider_fights(duel):
    """The webbed one is first in the list: the free one must still get its
    swing, and the webbed one must never land one."""
    manager, first, second = duel
    assert manager.creatures.index(first) < manager.creatures.index(second)
    first.web_pinned("trap", second)
    for _ in range(120):
        first.x, first.y, second.x, second.y = 200.0, 150.0, 215.0, 150.0
        first.webbed_timer = max(first.webbed_timer, 1.0)
        manager._resolve_combat(DT)
        for creature in (first, second):
            creature.attack_cooldown = max(0.0, creature.attack_cooldown - DT)
    assert first.hp < first.max_hp, "the free spider should be hitting the webbed one"
    assert second.hp == second.max_hp, "the webbed spider landed a blow"


def test_a_webbed_spider_fires_no_silk_and_cannot_pounce(duel):
    manager, first, second = duel
    first.web_pinned("trap", second)
    assert first._maybe_shoot_web_at_foe(second, 120.0) is False
    assert first._maybe_shoot_web_at_cursor(80.0, first.x + 80.0, first.y) is False
    first._launch_jump(first.x + 60.0, first.y, kind="pounce", after="idle")
    assert not first.airborne


def test_the_player_cannot_bite_or_shoot_while_webbed(duel):
    manager, first, second = duel
    player = PlayerController(first)
    player.aim = (second.x, second.y)
    first.web_pinned("trap", second)
    assert player.bite(manager) is False
    assert player.shoot(manager) is False
    assert second.hp == second.max_hp
    assert "struggle" in player.feedback.lower()
    silk = player.silk
    first.webbed_timer = 0.0
    assert player.shoot(manager) is True and player.silk == silk - 1, "free again, it can fight"
