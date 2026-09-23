"""The capture spiral is knotted to the radii (DC-70).

The owner, looking at a web on the desktop: *"the webs being not realistic as
they are not conected stright to the spine but rotating little by little
despite the spine."*

That is a real orb-weaver's web being described. The capture spiral is not a
curve laid over the top of the radii -- it is a run of straight silk segments,
each bridging two neighbouring spokes and **attached to every spoke it
crosses**. That attachment is why the rings of a real web line up with the
radii instead of drifting against them.

``_spiral_points`` sampled an Archimedean spiral on its own parameter,
``max(12, loops * 16)`` evenly spaced steps, so its corners landed wherever
that arithmetic put them. Measured on the shipped webs:

    full orb    12 radii, 210 spiral vertices, 191 off a radius (91%)
                worst 15.0 deg adrift, with radii 30.0 deg apart
    corner orb  10 radii, 146 spiral vertices, 129 off a radius (88%)
                worst 18.5 deg adrift, with radii 36.0 deg apart

Half a sector out, and a little further on every turn -- exactly "rotating
little by little despite the spine".

Three things had to be true together, and each was found by fixing the one
before it and measuring again:

1. **The spiral walks the spokes.** One vertex per radius, the radius
   shrinking as it goes. This alone took the full orb to 0%.
2. **It follows the spoke that exists, not the one that was asked for.** A
   radius whose tip runs off the screen is clamped back, which moves it. The
   corner web still had 35% adrift until the angles were taken from the
   clamped tips.
3. **The radii bend with the sag.** The static gravity sag is a position
   field, so two points in the same place move together -- but a radius drawn
   as a bare hub-to-tip line has nothing in the middle to move. The spiral
   sagged and the straight spokes did not, and they came apart again: 82%,
   with the geometry otherwise exact. Radii now carry a vertex at each
   crossing.
"""

from __future__ import annotations

import math
import random

import pytest
from desktop_bug.world import webs as W

SCREEN = (1200, 800)
SEEDS = (4, 9, 17, 23)


def _plans():
    """Both orb patterns, over several seeds, with the sag applied as shipped."""
    for seed in SEEDS:
        random.seed(seed)
        strands, hub, _ = W._plan_orb((600.0, 400.0), 260.0, *SCREEN)
        yield f"orb/{seed}", strands, hub
        random.seed(seed)
        strands, hub, _ = W._plan_corner_orb(
            (40.0, 40.0), (0.707, 0.707), 300.0, 38.0, *SCREEN)
        yield f"corner/{seed}", strands, hub


def _knots(strands):
    return [(round(x, 2), round(y, 2))
            for s in strands if s.kind == "radius" for (x, y) in s.points]


def _spiral_points(strands):
    return [p for s in strands if s.kind in ("capture", "aux") for p in s.points]


# ------------------------------------------------------------ the report

def test_every_spiral_vertex_sits_on_a_radius():
    """The whole of it, stated as the property a real web has.

    Measured in pixels rather than in angle: once the radii bend under the
    sag, "points at a spoke" stops meaning anything and "is on a spoke" is
    the thing that matters.
    """
    for label, strands, _hub in _plans():
        knots = _knots(strands)
        assert knots, label
        loose = []
        for (px, py) in _spiral_points(strands):
            gap = min(math.hypot(px - kx, py - ky) for kx, ky in knots)
            if gap > 0.75:
                loose.append(round(gap, 2))
        assert not loose, f"{label}: {len(loose)} loose vertices, worst {max(loose)}px"


def test_the_webs_actually_have_spirals_and_spokes():
    """A planner that quietly produced neither would pass everything above."""
    for label, strands, _hub in _plans():
        radii = [s for s in strands if s.kind == "radius"]
        capture = [s for s in strands if s.kind == "capture"]
        assert len(radii) >= 9, (label, len(radii))
        assert capture, label
        assert sum(len(s.points) for s in capture) > 40, label


def test_a_radius_is_no_longer_a_bare_two_point_line():
    """It has to carry a vertex at every crossing, or it cannot bend with the
    sag and the spiral comes off it."""
    for label, strands, _hub in _plans():
        for strand in strands:
            if strand.kind == "radius":
                assert len(strand.points) > 2, label


# ------------------------------------------------- the generator in isolation

def test_the_spiral_visits_each_spoke_in_turn():
    angles = [i * math.tau / 8.0 for i in range(8)]
    pts, crossings = W._spiral_on_radii((0.0, 0.0), angles, 100.0, 10.0,
                                        loops=3, full=True)
    assert len(pts) == len(crossings)
    visited = [i for i, _r in crossings]
    assert visited[:9] == [0, 1, 2, 3, 4, 5, 6, 7, 0], visited[:9]


def test_a_sector_spiral_folds_back_across_its_fan():
    """A corner web does not wind round; it sweeps out and back."""
    angles = [i * 0.2 for i in range(6)]
    _pts, crossings = W._spiral_on_radii((0.0, 0.0), angles, 100.0, 10.0,
                                         loops=2, full=False)
    visited = [i for i, _r in crossings]
    assert visited[:6] == [0, 1, 2, 3, 4, 5]
    assert visited[6:11] == [4, 3, 2, 1, 0], visited[6:11]


def test_the_radius_shrinks_all_the_way_in():
    _pts, crossings = W._spiral_on_radii(
        (0.0, 0.0), [i * math.tau / 10.0 for i in range(10)],
        200.0, 20.0, loops=4, full=True)
    radii = [r for _i, r in crossings]
    assert radii[0] == pytest.approx(200.0)
    assert radii[-1] == pytest.approx(20.0)
    assert all(a >= b - 1e-9 for a, b in zip(radii, radii[1:])), "not monotonic"


def test_a_spoke_cut_short_by_the_screen_does_not_carry_silk_past_its_tip():
    """The last 3-4% of loose vertices in the corner web were these."""
    angles = [0.0, 0.5, 1.0]
    reaches = [300.0, 40.0, 300.0]     # the middle spoke is clipped short
    _pts, crossings = W._spiral_on_radii((0.0, 0.0), angles, 250.0, 20.0,
                                         loops=3, full=True, reaches=reaches)
    for index, radius in crossings:
        assert radius <= reaches[index] + 1e-6, (index, radius)


def test_the_old_sampler_is_gone():
    """`_spiral_points` sampled the curve instead of the spokes. It has no
    callers left, and leaving it would invite the bug straight back."""
    assert not hasattr(W, "_spiral_points")
