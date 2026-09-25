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
    assert len(spider.web_anchors) == 9
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


@pytest.mark.parametrize("keys", [{"move_up"}, {"move_left"}, {"move_down", "move_right"}])
def test_a_webbed_spider_does_not_move_or_turn(keys):
    """The owner: "the net does not stop him from turning ... he should stay
    stationary without any movement when hit." Both movement modes."""
    from desktop_bug.app.controls import ControlSettings

    for movement in ("screen", "turn"):
        spider = _tarantula()
        player = PlayerController(spider, ControlSettings(movement=movement))
        spider.web_pinned("trap")
        player.held = set(keys)
        x0, y0, h0 = spider.x, spider.y, spider.heading
        offsets = []
        while spider.webbed:
            spider.update(DT, -9000.0, -9000.0, 2400, 1400)
            offsets.append(math.hypot(*spider.combat_body_offset()))
            if spider.webbed:
                assert math.hypot(spider.x - x0, spider.y - y0) < 1e-6, (movement, keys, "walked")
                assert abs(spider.heading - h0) < 1e-6, (movement, keys, "turned")
        assert max(offsets) < 1e-9, "struggling strains the net, not the body"
        assert player.jump() is False or not spider.webbed


def test_a_computer_spider_is_held_still_too():
    spider = _tarantula()
    spider.web_pinned("trap")
    spider.target_x, spider.target_y = spider.x + 300.0, spider.y + 200.0
    spider.target_heading = spider.heading + 2.0
    spider.speed = 120.0
    x0, y0, h0 = spider.x, spider.y, spider.heading
    config = spider._spider_gait_config()
    for _ in range(60):
        advance_controller(spider, DT, config)
    assert math.hypot(spider.x - x0, spider.y - y0) < 1e-6
    assert abs(spider.heading - h0) < 1e-6


def test_the_net_is_a_web_that_tears_as_it_loosens():
    from PyQt5.QtGui import QColor, QImage, QPainter

    spider = _tarantula()
    spider.web_pinned("trap")
    assert len(spider.web_net) == 9 and all(len(s["rings"]) == 4 for s in spider.web_net)
    box = spider.bounding_rect()
    for ax, ay in spider.web_anchors:
        assert box[0] <= ax <= box[2] and box[1] <= ay <= box[3], "repaint must cover the guy lines"

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
    fresh = white_pixels(spider)
    assert fresh > white_pixels(free) + 30, "the net should show"
    spider.webbed_timer = WEBBED_SECONDS * 0.2
    assert white_pixels(spider) < fresh * 0.7, "a loosening net has torn strands"
