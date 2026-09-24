"""Bars under a spider's label, and the palps' colour after a drop (DC-87).

The owner asked for an XP bar under the health bar, switched from the main
menu, and for the bars to be "same width for all spiders" -- they used to
stretch to the name box, so a long name drew a long bar. Also reported: "when
tarantula is draged and is droped for a second his pedipapls lose colors".
The legs held their colour through the drop; the palps flashed pale, because
they tested ``startle`` alone and skipped the gate that holds the startle
colour off for a moment after a grab.
"""

import json
import math
import random
import tempfile
from pathlib import Path

import pytest

from desktop_bug.content.body_plans import resolve_body_plan
from desktop_bug.content.preset_io import validate_preset
from desktop_bug.creature import Creature
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import xp_to_next_level
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering and the manager both need the one Qt application object."""


DT = 1.0 / 60.0
SCREEN = (1600, 900)
AWAY = (-100000.0, -100000.0)
GREEN = (104, 194, 108)


def build(seed: int = 9) -> Creature:
    random.seed(seed)
    model = resolve_body_plan(json.loads(
        (ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8")))
    traits = json.loads((ROOT / "personalities" / "mellow.json").read_text(encoding="utf-8"))
    creature = Creature(model, traits, *SCREEN, index=0, progression_id=f"bars:{seed}")
    creature.x, creature.y = 800.0, 450.0
    creature._initialize_legs()
    for _ in range(30):
        creature.update(DT, *AWAY, *SCREEN)
    return creature


def paint(creature: Creature):
    from PyQt5.QtGui import QColor, QImage, QPainter

    image = QImage(600, 600, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(0, 0, 0, 0))
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    creature.x, creature.y = 300.0, 380.0
    creature.render(painter, always_show_names=True)
    painter.end()
    return image


def widest_run(image, target, rows=range(120, 380)) -> tuple[int, int]:
    """Widest horizontal run of a colour, and the row it is on."""
    best, best_row = 0, -1
    for y in rows:
        run = 0
        for x in range(40, 560):
            pixel = image.pixelColor(x, y)
            close = (pixel.alpha() > 150
                     and math.dist((pixel.red(), pixel.green(), pixel.blue()), target) < 40)
            run = run + 1 if close else 0
            if run > best:
                best, best_row = run, y
    return best, best_row


def test_the_health_bar_is_the_same_width_whatever_the_name_or_health() -> None:
    widths = []
    for name, max_hp in (("Bo", None), ("Maximilian the Magnificent", None), ("Bo", 900.0)):
        creature = build()
        creature.set_name(name)
        if max_hp is not None:
            creature.max_hp = max_hp
            creature.hp = max_hp
        creature.set_health_label_pinned(True)
        widths.append(widest_run(paint(creature), GREEN)[0])
    assert widths[0] >= 20, f"no health bar was drawn: {widths}"
    assert max(widths) - min(widths) <= 1, (
        f"bars differ in width for a short name, a long name, more health: {widths}")


def test_the_xp_bar_sits_under_the_health_bar_and_fills_with_xp() -> None:
    creature = build()
    creature.set_name("Bo")
    creature.set_health_label_pinned(True)
    creature.force_show_xp = True
    threshold = xp_to_next_level(creature.level)

    creature.progression.xp = int(threshold * 0.9)
    image = paint(creature)
    health, health_row = widest_run(image, GREEN)
    most, xp_row = widest_run(image, Creature.XP_BAR_COLOR)
    assert most >= 15, f"no XP bar was drawn: widest run {most}px"
    assert xp_row > health_row, f"the XP bar is not under the health bar ({xp_row} vs {health_row})"
    assert most <= health, "the XP bar is wider than the health bar"

    creature.progression.xp = int(threshold * 0.2)
    little = widest_run(paint(creature), Creature.XP_BAR_COLOR)[0]
    assert little < most * 0.5, f"the XP bar did not follow the XP: {little}px vs {most}px"

    creature.force_show_xp = False
    assert widest_run(paint(creature), Creature.XP_BAR_COLOR)[0] < 6, "XP bar drawn while off"


def test_the_xp_bar_is_inside_the_repaint_footprint() -> None:
    creature = build()
    creature.set_name("Bo")
    creature.set_health_label_pinned(True)
    plain = creature.bounding_rect(True)
    creature.force_show_xp = True
    assert creature.bounding_rect(True)[1] < plain[1] - 2.0


def test_the_switch_reaches_every_spider_and_is_saved() -> None:
    scratch = Path(tempfile.mkdtemp(prefix="dc-xp-"))
    preset = scratch / "xp.json"
    preset.write_text(json.dumps({
        "name": "xp",
        "slots": [{"model": "tarantula", "personality": "mellow", "count": 2}],
        "settings": {"always_show_xp": True, "flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=4)
    assert all(c.xp_label_pinned for c in manager.creatures), "the preset setting was ignored"
    manager.set_always_show_xp(False)
    assert not any(c.xp_label_pinned for c in manager.creatures)
    manager.set_always_show_xp(True)
    born = manager._create_creature(
        manager.creatures[0].model, manager.creatures[0].personality, index=9)
    assert born.xp_label_pinned, "a spider born later is missing its XP bar"
    with pytest.raises(ValueError):
        validate_preset({"name": "x", "slots": [{"model": "tarantula", "personality": "mellow", "count": 1}],
                         "settings": {"always_show_xp": "yes"}})


def test_the_palps_keep_their_colour_when_a_spider_is_dropped() -> None:
    """The legs and the palps must agree on whether the spider is startled."""
    def highlights(creature) -> int:
        """How many times the pale startle colour is asked for in one frame.

        A calm tarantula asks for it too, for the sheen on its body, so the
        dropped spider is compared with a calm one rather than with zero.
        """
        asked = []
        original = creature._qcolor

        def record(key, alpha=255):
            asked.append(key)
            return original(key, alpha)

        creature._qcolor = record
        paint(creature)
        del creature._qcolor
        return asked.count("highlight")

    calm = highlights(build())
    creature = build()
    creature.start_drag(creature.x, creature.y)
    for _ in range(10):
        creature.drag_to(DT, creature.x + 2.0, creature.y)
    creature.release_drag(creature.x, creature.y)
    assert creature._startle_amount() > 0.35, "the drop did not leave the spider startled"
    assert not creature._startle_highlight_active(creature._startle_amount()), (
        "the legs would change colour on the drop too; this test is meant for the palps")
    dropped = highlights(creature)
    assert dropped == calm, (
        f"right after a drop the startle colour was asked for {dropped} times, "
        f"against {calm} on a calm spider: the palps changed colour")
