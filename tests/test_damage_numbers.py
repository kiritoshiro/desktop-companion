"""Floating damage numbers: how much health a blow took.

The owner: *"indicate how much health was taken from the spiders."*
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from support import load_pair
from desktop_bug.creature import Creature
from desktop_bug.creature.damage_numbers import (
    DAMAGE_MERGE_SECONDS,
    DAMAGE_NUMBER_SECONDS,
)
from desktop_bug.world.carcass import carcass_for

DT = 1.0 / 60.0


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and painting need the one Qt application."""


def _spider():
    model, personality = load_pair()
    spider = Creature(model, personality, 900, 700, index=0, seed=7)
    spider.x, spider.y = 450.0, 350.0
    spider.max_hp = spider.hp = 1000.0
    spider.armor = 0.0
    return spider


def test_a_hit_shows_what_it_took_and_quick_hits_add_up():
    spider = _spider()
    dealt = spider.take_damage(12.0)
    assert spider.damage_numbers and spider.damage_numbers[0][0] == pytest.approx(dealt)
    spider._update_damage_numbers(DAMAGE_MERGE_SECONDS / 2)
    spider.take_damage(5.0)
    assert len(spider.damage_numbers) == 1 and spider.damage_numbers[0][0] == pytest.approx(17.0)
    spider._update_damage_numbers(DAMAGE_MERGE_SECONDS)
    spider.take_damage(3.0)
    assert len(spider.damage_numbers) == 2, "a later blow gets its own number"
    spider._update_damage_numbers(DAMAGE_NUMBER_SECONDS)
    assert spider.damage_numbers == [], "numbers fade out"
    assert Creature.damage_text(12.4) == "-12" and Creature.damage_text(0.3) == "-0.3"


def test_the_repaint_area_covers_the_numbers():
    from desktop_bug.creature.damage_numbers import DAMAGE_RISE_PX

    spider = _spider()
    spider.take_damage(9.0)
    box = spider.bounding_rect()
    top = spider._damage_top()
    assert box[1] <= top - DAMAGE_RISE_PX - 20.0, "the risen number is above the box"
    assert box[2] >= spider.x + spider.size * 0.8 + 60.0, "the number's width is outside the box"


def test_the_killing_blow_shows_on_the_remains():
    spider = _spider()
    spider.hp = 5.0
    spider.take_damage(40.0)
    assert spider.dead
    remains = carcass_for(spider)
    assert remains.damage_numbers and remains.damage_numbers[0][0] == pytest.approx(40.0)
    plain = carcass_for(_spider())
    assert remains.footprint()[3] > plain.footprint()[3], "its repaint area covers the number"
    remains.update(DAMAGE_NUMBER_SECONDS)
    assert remains.damage_numbers == []


def test_a_fight_puts_numbers_over_the_spiders(monkeypatch):
    from desktop_bug.manager import CreatureManager

    scratch = Path(tempfile.mkdtemp(prefix="dmg-"))
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
    left, right = manager.creatures
    left.x, left.y, right.x, right.y = 150.0, 165.0, 270.0, 165.0
    seen = False
    for _ in range(600):
        manager.update(DT, -9000.0, -9000.0)
        if any(c.damage_numbers for c in manager.creatures):
            seen = True
            break
    assert seen, "a fight should show damage numbers"


def test_the_number_is_drawn_in_red():
    from PyQt5.QtGui import QColor, QImage, QPainter

    def red(spider):
        image = QImage(300, 300, QImage.Format_ARGB32)
        image.fill(QColor(0, 0, 0, 0))
        painter = QPainter(image)
        painter.translate(150 - spider.x, 150 - spider.y)
        spider.render(painter)
        painter.end()
        count = 0
        for y in range(0, 300, 2):
            for x in range(0, 300, 2):
                c = image.pixelColor(x, y)
                count += c.alpha() > 150 and c.red() > 200 and c.green() < 120 and c.blue() < 110
        return count

    hit = _spider()
    hit.take_damage(15.0)
    hit.hurt_flash = 0.0      # the tint is a separate cue; count only the number
    assert red(hit) > red(_spider()) + 10


def test_damage_to_your_side_is_red_and_to_an_enemy_ice_blue():
    """The owner: "make the damage done to allies and to enemies of different colour"."""
    from desktop_bug.creature.damage_numbers import ALLY_DAMAGE_COLOR, ENEMY_DAMAGE_COLOR

    ally, enemy = _spider(), _spider()
    ally.progression.team_id = "adventurers"
    enemy.progression.team_id = "rivals"
    assert ally.damage_color() == ALLY_DAMAGE_COLOR
    assert enemy.damage_color() == ENEMY_DAMAGE_COLOR != ALLY_DAMAGE_COLOR
    enemy.take_damage(enemy.hp + 50.0)
    assert carcass_for(enemy).damage_color() == ENEMY_DAMAGE_COLOR, "the remains keep the enemy's colour"


def test_stamina_is_yellow_everywhere():
    from desktop_bug.app import wood_theme
    from PyQt5.QtGui import QColor

    for colour in (QColor(*Creature.STAMINA_BAR_COLOR), QColor(wood_theme.STAMINA)):
        assert 40 <= colour.hue() <= 60 and colour.saturation() > 180, colour.name()


def test_numbers_are_drawn_last_above_every_spider_and_name(monkeypatch):
    """The owner: "my name on the map is hiding damage I'm doing to enemies."
    Every spider and label is drawn first; the numbers go on top."""
    from PyQt5.QtGui import QImage, QPainter
    from desktop_bug.manager import CreatureManager

    scratch = Path(tempfile.mkdtemp(prefix="dmg-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "f.json"
    preset.write_text(json.dumps({
        "name": "f",
        "slots": [{"model": "tarantula", "personality": "bold", "count": 2, "slot_id": "a"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, 420, 300, seed=6)
    first, second = manager.creatures
    first.x, first.y, second.x, second.y = 200.0, 150.0, 210.0, 140.0
    first.take_damage(5.0)
    calls = []
    for spider in manager.creatures:
        monkeypatch.setattr(spider, "_draw_name_label",
                            lambda *a, s=spider, **k: calls.append(("name", s)))
        monkeypatch.setattr(spider, "_draw_damage_numbers",
                            lambda *a, s=spider, **k: calls.append(("numbers", s)))
    image = QImage(420, 300, QImage.Format_ARGB32_Premultiplied)
    painter = QPainter(image)
    manager.render(painter)
    painter.end()
    names = [i for i, (kind, _) in enumerate(calls) if kind == "name"]
    numbers = [i for i, (kind, _) in enumerate(calls) if kind == "numbers"]
    assert len(names) == 2 and numbers
    assert min(numbers) > max(names), calls
