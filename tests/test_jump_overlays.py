"""Nothing is left on the ground when a spider jumps (DC-82).

The owner: *"when spider jumps there is some weird things underneath it for
a moment. fix that it should not stay on the 'ground'"*.

The body is drawn raised by `jump_z`, and the leg pass raises every leg by
the same amount (`leg_y_off`). Two overlays drawn after it did not:

* `_draw_leg_connections` -- the tarantula's leg sockets, coxae and
  trochanters, painted over the shell -- re-solved each leg from ground-level
  points. At the top of a hop eight root pieces sat on the floor under the
  spider. Counted on a hand-placed tarantula at its apex: 90 solid pixels
  below the ground point before, 0 after. Models without `leg_connections`
  never had it.
* `_draw_equipment` draws in world space outside the body transform, so
  every armour piece stayed on the floor during a jump.

The shadow is meant to stay on the ground, and does.
"""

from __future__ import annotations

import random
from types import SimpleNamespace

import pytest
from PyQt5.QtGui import QImage, QPainter

from desktop_bug.content.discovery import discover_models, discover_personalities
from desktop_bug.creature import Creature
from desktop_bug.creature import render_procedural as RP
from support import ROOT


@pytest.fixture(scope="module")
def content(qapp):
    return discover_models(ROOT)[0], discover_personalities(ROOT)[0]


def _airborne(content, model_id="tarantula"):
    """A spider hand-placed at the top of a hop."""
    models, personalities = content
    random.seed(3)
    spider = Creature(models[model_id], personalities["balanced"], 800, 600)
    spider.x, spider.y = 400.0, 300.0
    spider.has_skill = lambda skill: True
    spider._launch_jump(405.0, 300.0, kind="hop", peak=spider.size * 1.2, duration=0.5)
    while spider.jump_t < 0.5:
        spider._update_jump(1.0 / 60.0)
    assert spider.jump_z > spider.size * 0.9
    return spider


def _render(spider):
    image = QImage(800, 600, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    spider.render(painter)
    painter.end()
    return image


def test_the_leg_roots_rise_with_the_legs(content, monkeypatch):
    """Every leg is solved twice a frame: once for the leg, once for the
    socket overlay. Airborne, both must start from the same raised socket."""
    spider = _airborne(content)
    ground = {id(leg): spider._leg_attach(leg)[1] for leg in spider.legs}
    roots: dict[int, list[float]] = {}
    original = type(spider)._sprite_leg_chain_points

    def record(self, leg, ax, ay, fx, fy, chain_config):
        roots.setdefault(id(leg), []).append(ay)
        return original(self, leg, ax, ay, fx, fy, chain_config)

    monkeypatch.setattr(type(spider), "_sprite_leg_chain_points", record)
    _render(spider)
    assert len(roots) == 8
    for leg_id, seen in roots.items():
        assert len(seen) >= 2, "the socket overlay did not solve this leg"
        for ay in seen:
            assert ay == pytest.approx(ground[leg_id] - spider.jump_z, abs=1e-6), (
                f"a leg was solved from {ay:.1f}, the ground-level socket is "
                f"{ground[leg_id]:.1f} and the jump is {spider.jump_z:.1f} high")


def test_nothing_solid_is_left_under_a_jumping_tarantula(content):
    spider = _airborne(content)
    spider.heading = spider.target_heading = 0.0   # body lies along the ground line
    spider._basis_cache = None
    image = _render(spider)
    below = sum(
        1
        for y in range(int(spider.y) + 2, int(spider.y + spider.size * 1.2))
        for x in range(int(spider.x - spider.size * 2), int(spider.x + spider.size * 2))
        if (image.pixel(x, y) >> 24) > 120
    )
    assert below == 0, f"{below} solid pixels under the airborne spider"


def test_armour_rises_with_the_body(content, monkeypatch):
    """Drawn mid-jump, the armour is the ground picture moved up by exactly
    the jump height."""
    items = [SimpleNamespace(slot=slot) for slot in ("abdomen", "carapace", "head", "legs")]
    monkeypatch.setattr(RP, "equipped_items", lambda progression: items)
    spider = _airborne(content)
    # Airborne legs also tuck in by an amount that depends on jump_z, which
    # would move the leg armour by more than the height. Hold the tuck still
    # so the comparison sees only the height.
    spider.airborne = False

    def armour_top(jump_z):
        spider.jump_z = jump_z
        image = QImage(800, 600, QImage.Format_ARGB32_Premultiplied)
        image.fill(0)
        painter = QPainter(image)
        spider._draw_equipment(painter)
        painter.end()
        rows = [y for y in range(image.height())
                if any((image.pixel(x, y) >> 24) > 0 for x in range(300, 500))]
        assert rows, "no armour was drawn"
        return rows[0], rows[-1]

    height = 30.0
    top_ground, bottom_ground = armour_top(0.0)
    top_air, bottom_air = armour_top(height)
    assert top_ground - top_air == pytest.approx(height, abs=1.0)
    assert bottom_ground - bottom_air == pytest.approx(height, abs=1.0)
