"""Headless checks for XP, growth, equipment, resources, and relations."""

from __future__ import annotations

import json
import random
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.creature import Creature  # noqa: E402
from desktop_bug.progression import MAX_LEVEL, ProgressionState, xp_to_next_level  # noqa: E402


def load_data() -> tuple[dict, dict]:
    with (ROOT / "models" / "tarantula" / "model.json").open(encoding="utf-8") as handle:
        model = json.load(handle)
    with (ROOT / "personalities" / "mellow.json").open(encoding="utf-8") as handle:
        personality = json.load(handle)
    return model, personality


def main() -> None:
    random.seed(7)
    model, personality = load_data()
    spider = Creature(model, personality, 1280, 720, index=0, progression_id="smoke:0")
    other = Creature(model, personality, 1280, 720, index=1, progression_id="smoke:1")
    assert spider.level == 1 and spider.xp == 0
    old_size = spider.size
    old_speed = spider._speed_mult()

    assert spider.gain_experience(xp_to_next_level(1) - 1, "test") == []
    assert spider.level == 1
    assert spider.gain_experience(1, "test")
    assert spider.level == 2

    # A large award may cross multiple thresholds but never exceed level 30.
    events = spider.gain_experience(sum(xp_to_next_level(level) for level in range(2, 5)) + 1, "fly")
    assert spider.level >= 4 and events
    assert spider.level <= MAX_LEVEL
    assert spider.size > old_size and spider._speed_mult() > old_speed

    # Progression talents consume points and affect derived stats.
    before_hp = spider.max_hp
    spider.progression.skill_points = max(spider.progression.skill_points, 1)
    ok, _ = spider.unlock_progression_ability("vitality")
    assert ok and spider.max_hp > before_hp

    assert spider.add_inventory_item("fluffy_mantle")
    ok, _ = spider.equip_item("fluffy_mantle")
    assert ok and spider.armor > 0
    hp_before = spider.hp
    assert spider.take_damage(20) < 20 and spider.hp < hp_before
    assert spider.spend_energy(10)
    assert 0 <= spider.energy <= spider.max_energy

    spider.set_team("pack")
    other.set_team("pack")
    assert spider.relation_to(other) == "friend"
    other.set_team("rivals")
    assert spider.relation_to(other) == "neutral"
    spider.progression.relation_overrides[other.progression_id] = "foe"
    assert spider.relation_to(other) == "foe"

    state = ProgressionState.from_dict(spider.progression.to_dict())
    assert state.to_dict() == spider.progression.to_dict()
    spider.progression.level = MAX_LEVEL
    spider.gain_experience(999999, "test")
    assert spider.level == MAX_LEVEL and spider.xp == xp_to_next_level(MAX_LEVEL)
    print("OK: progression, bounded growth, abilities, armor, resources, and relations")


if __name__ == "__main__":
    main()
