"""A webbed spider shows the net and fights it.

The owner: *"when shooting the web at spider it should show on the spider the
net while it is active to indicate it is hit and immobilised, and the spider
should try to move out of it if he wants to be free."*
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import advance_controller, build_creature
from support import ROOT
from desktop_bug.app.adventure import PlayerController
from desktop_bug.content.body_plans import resolve_body_plan
from desktop_bug.creature.constants import WEBBED_SECONDS

DT = 1.0 / 60.0


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Creatures and painting need the one Qt application."""


def _tarantula():
    model = resolve_body_plan(json.loads((ROOT / "models/tarantula/model.json").read_text()))
    personality = json.loads((ROOT / "personalities/bold.json").read_text())
    random.seed(8)
    spider = build_creature(model, personality)
    spider.speed = 0.0
    spider.target_x, spider.target_y = spider.x, spider.y
    config = spider._spider_gait_config()
    for _ in range(30):
        advance_controller(spider, DT, config)
    return spider


def _seconds_to_free(spider, step) -> float:
    spider.web_pinned("trap")
    elapsed = 0.0
    while spider.webbed and elapsed < 10.0:
        step()
        elapsed += DT
    return elapsed


def test_a_fresh_net_is_anchored_and_goes_when_free():
    spider = _tarantula()
    spider.web_pinned("trap")
    assert len(spider.web_anchors) == 5
    assert all(math.hypot(ax - spider.x, ay - spider.y) > spider.size for ax, ay in spider.web_anchors)
    spider.webbed_timer = DT / 2
    spider._update_web_struggle(DT)
    spider.webbed_timer = 0.0
    spider._update_web_struggle(DT)
    assert spider.web_anchors == []


def test_a_computer_spider_struggles_free_sooner():
    spider = _tarantula()

    def step():
        spider.webbed_timer = max(0.0, spider.webbed_timer - DT)
        spider._update_web_struggle(DT)

    freed = _seconds_to_free(spider, step)
    assert freed < WEBBED_SECONDS * 0.8, f"struggling should shorten the pin, took {freed:.2f}s"
    assert freed > WEBBED_SECONDS * 0.5, f"too easy: {freed:.2f}s"


def test_the_player_frees_itself_faster_by_struggling():
    def run(hold):
        spider = _tarantula()
        player = PlayerController(spider)
        player.held = set(hold)

        def step():
            spider.update(DT, -9000.0, -9000.0, 2400, 1400)

        return _seconds_to_free(spider, step)

    waiting = run(())
    fighting = run({"move_left"})
    assert waiting == pytest.approx(WEBBED_SECONDS, abs=0.05), "not struggling takes the full pin"
    assert fighting < waiting * 0.8, (fighting, waiting)


def test_struggling_jerks_the_body_and_the_net_is_drawn():
    from PyQt5.QtGui import QColor, QImage, QPainter

    spider = _tarantula()
    spider.web_pinned("trap")
    offsets = []
    for _ in range(40):
        spider._update_web_struggle(DT)
        offsets.append(math.hypot(*spider.combat_body_offset()))
    assert max(offsets) > spider.size * 0.03, "the body should jerk against the silk"

    def white_pixels(target):
        image = QImage(240, 240, QImage.Format_ARGB32)
        image.fill(QColor(0, 0, 0, 0))
        painter = QPainter(image)
        painter.translate(120 - target.x, 120 - target.y)
        target.render(painter)
        painter.end()
        count = 0
        for y in range(0, 240, 2):
            for x in range(0, 240, 2):
                c = image.pixelColor(x, y)
                count += c.alpha() > 120 and min(c.red(), c.green(), c.blue()) > 200
        return count

    free = _tarantula()
    assert white_pixels(spider) > white_pixels(free) + 30, "the net should show"
