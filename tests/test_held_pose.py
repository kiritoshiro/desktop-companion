"""A carried spider's legs must react to how fast the hand is moving.

`_sprite_leg_chain_points` computed `held_speed01` -- how fast the spider is
being dragged, as a 0..1 fraction -- and then never used it, while the comment
beside it said faster hand motion should increase the relaxed bend. That is
finding B6: the intent was unfinished rather than dead, and ruff's F841 was the
only thing that knew. The bend is now applied, and this is what says so.
"""

from __future__ import annotations

import math
import random

import pytest
from desktop_bug.creature import Creature
from support import load_pair

FAST = 400.0  # well past the 260 px/s the bend saturates at


@pytest.fixture
def carried() -> Creature:
    random.seed(3)
    model, personality = load_pair()
    spider = Creature(model, personality, 1200, 800)
    spider.x, spider.y = 600.0, 400.0
    spider.heading = spider.target_heading = 0.0
    spider._initialize_legs()
    spider.dragging = True
    return spider


def chain_bend(spider: Creature) -> float:
    """How far the drawn chain bows away from a straight attach-to-foot line.

    The solve is memoised per drawn frame on `(leg, attach, foot)`, and drag
    speed is deliberately not part of that key because it cannot change inside
    one frame. Measuring two speeds means measuring two frames, so the cache is
    cleared first -- which is what `Creature.render` does at the top of each.
    """
    spider._chain_points_cache.clear()
    config = spider._sprite_leg_chain_config()
    total = 0.0
    for leg in spider.legs:
        ax, ay = spider._leg_attach(leg)
        fx, fy = spider._visual_foot_for_render(leg)
        points = spider._sprite_leg_chain_points(leg, ax, ay, fx, fy, config)
        span = math.hypot(fx - ax, fy - ay)
        if span < 1e-6:
            continue
        for px, py in points[1:-1]:
            # Perpendicular distance from the straight line between the ends.
            total += abs((fx - ax) * (ay - py) - (ax - px) * (fy - ay)) / span
    return total


def test_a_still_hand_still_leaves_a_soft_knee(carried):
    """The bend must not collapse to a straight spoke when nothing moves."""
    carried.current_speed = 0.0
    assert chain_bend(carried) > 0.0


def test_a_faster_hand_bends_the_carried_legs_more(carried):
    carried.current_speed = 0.0
    still = chain_bend(carried)
    carried.current_speed = FAST
    fast = chain_bend(carried)
    assert fast > still, (still, fast)


def test_the_extra_bend_is_a_nudge_and_not_a_fold(carried):
    """Half the weight of the drag response, as the helper below it uses."""
    carried.current_speed = 0.0
    still = chain_bend(carried)
    carried.current_speed = FAST
    fast = chain_bend(carried)
    assert fast <= still * 1.35, (still, fast)
