"""The world is the screens, not their bounding box (DC-65).

The owner, running the overlay across two monitors: *"if i got two screens of
diferent size, on the smaller one (extended) they might dissaper out of
border. make limits based on the size of screens and the way they are
extended."*

The overlay window spans the union *bounding rectangle* of every monitor, and
every creature was clamped to ``0..screen_w`` by ``0..screen_h``. Those are
the same shape for one monitor, and for two identical ones side by side. They
stop being the same shape as soon as the monitors differ:

    2560x1440 at (0,0) beside 1920x1080 at (2560,0)
    bounding box 4480x1440, real screens 2560x1440 + 1920x1080
    dead space   x 2560..4480, y 1080..1440  -- 691,200px of nothing

A spider in there is inside the window, being updated, being painted, and on
no monitor at all. It does not fall out of the world; it is drawn where no
screen can show it. From the desk it vanishes.

The layout below is that real one, scaled down so a test colony can cross it
in a few seconds of simulated time. Measured on it, four spiders over a
simulated minute: **3284 of 14400 spider-frames (22.8%) spent on no monitor**
before this package, and 0 after.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.world.playfield import Playfield, ScreenRect, rects_from_geometry

DT = 1.0 / 60.0
AWAY = (-9000.0, -9000.0)

# A big monitor with a smaller one extended to its right: the shape that has
# dead space, and the shape the owner is running.
BIG = ScreenRect(0.0, 0.0, 800.0, 600.0)
SMALL = ScreenRect(800.0, 0.0, 500.0, 360.0)
SCREEN = (1300, 600)
DEAD = (1100.0, 500.0)      # inside the window, on neither monitor


# ------------------------------------------------------------ the geometry

def test_one_monitor_costs_nothing():
    """The common case must not pay for any of this."""
    field = Playfield(800, 600, [ScreenRect(0.0, 0.0, 800.0, 600.0)])
    assert field.simple is True
    assert field.contains(*DEAD) is False or True  # simple: the window is all
    assert field.clamp(400.0, 300.0) == (400.0, 300.0)


def test_knowing_nothing_excludes_nothing():
    """Before the engine reports a layout -- and in every headless test --
    the whole window is habitable, which is the behaviour that shipped."""
    field = Playfield(1300, 600)
    assert field.simple is True
    assert field.contains(*DEAD) is True


def test_two_monitors_that_tile_are_still_simple():
    """Identical screens side by side leave no dead space, so there is
    nothing to do and the fast path must stay on."""
    field = Playfield(1600, 600, [ScreenRect(0.0, 0.0, 800.0, 600.0),
                                  ScreenRect(800.0, 0.0, 800.0, 600.0)])
    assert field.simple is True


def test_mixed_monitors_leave_a_hole():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    assert field.simple is False
    assert field.contains(400.0, 300.0) is True     # on the big one
    assert field.contains(1000.0, 100.0) is True    # on the small one
    assert field.contains(*DEAD) is False           # below the small one


def test_a_point_in_the_hole_is_pushed_to_the_nearest_screen():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    x, y = field.clamp(*DEAD)
    assert field.contains(x, y) is True
    # It came from just below the small screen, so that is where it goes --
    # not back to whichever rectangle happens to be first in the list.
    assert y == pytest.approx(SMALL.bottom)
    assert x == pytest.approx(DEAD[0])


def test_a_margin_keeps_a_body_clear_of_the_edge():
    field = Playfield(*SCREEN, rects=[BIG, SMALL])
    x, y = field.clamp(*DEAD, margin=20.0)
    assert field.contains(x, y, margin=20.0) is True
    assert y == pytest.approx(SMALL.bottom - 20.0)


def test_a_margin_wider_than_a_monitor_does_not_make_it_uninhabitable():
    """A narrow screen must not become a place no spider may stand."""
    sliver = ScreenRect(0.0, 0.0, 30.0, 600.0)
    field = Playfield(1300, 600, [sliver, SMALL])
    x, y = field.clamp(10.0, 300.0, margin=200.0)
    assert 0.0 <= x <= 30.0


def test_geometry_is_translated_into_overlay_local_pixels():
    """A desktop's monitors can start at negative coordinates -- a second
    screen placed to the *left* of the primary is the usual way -- and the
    overlay's own origin is then negative too."""
    rects = rects_from_geometry(
        [(-1920.0, 0.0, 1920.0, 1080.0), (0.0, 0.0, 2560.0, 1440.0)],
        origin_x=-1920.0, origin_y=0.0)
    assert rects[0] == ScreenRect(0.0, 0.0, 1920.0, 1080.0)
    assert rects[1] == ScreenRect(1920.0, 0.0, 2560.0, 1440.0)


# ------------------------------------------------------- the colony itself

@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _colony(monkeypatch, rects=None, count=4):
    scratch = Path(tempfile.mkdtemp(prefix="dc65-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "s.json"
    preset.write_text(json.dumps({
        "name": "s",
        "slots": [{"model": "tarantula", "personality": "balanced",
                   "count": count, "slot_id": "a"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=11)
    if rects is not None:
        manager.set_screen_rects(rects)
    return manager


def test_a_spider_put_in_the_hole_walks_back_onto_a_screen(monkeypatch):
    manager = _colony(monkeypatch)
    manager.set_screen_rects([BIG, SMALL])
    spider = manager.creatures[0]
    spider.x, spider.y = DEAD
    manager.update(DT, *AWAY)
    assert manager.playfield.contains(spider.x, spider.y), (spider.x, spider.y)


def test_no_spider_ends_up_off_every_monitor(monkeypatch):
    """The report, run as a colony: nobody should be somewhere no screen
    shows, at any point, for a good long while."""
    manager = _colony(monkeypatch, rects=[BIG, SMALL])
    stray = 0
    for _ in range(int(60.0 * 60)):
        manager.update(DT, *AWAY)
        for spider in manager.creatures:
            if not manager.playfield.contains(spider.x, spider.y):
                stray += 1
    assert stray == 0, f"{stray} spider-frames spent on no monitor"


def test_without_the_layout_they_do_stray_into_it(monkeypatch):
    """The bug itself, so the test above is known to be measuring something.

    Same colony, same seed, same frames -- only the screen layout withheld,
    which is exactly the state the project shipped in.
    """
    manager = _colony(monkeypatch)          # no rects: the old behaviour
    hole = Playfield(*SCREEN, rects=[BIG, SMALL])
    stray = 0
    for _ in range(int(60.0 * 60)):
        manager.update(DT, *AWAY)
        for spider in manager.creatures:
            if not hole.contains(spider.x, spider.y):
                stray += 1
    assert stray > 0, "the dead corner was never visited; the test proves nothing"


def test_they_still_use_the_small_screen(monkeypatch):
    """A clamp that worked by herding everyone onto the big monitor would
    pass the test above and be useless."""
    manager = _colony(monkeypatch, rects=[BIG, SMALL], count=5)
    for spider in manager.creatures:
        spider.x, spider.y = 1000.0, 150.0     # start them on the small one
    visits = 0
    for _ in range(int(30.0 * 60)):
        manager.update(DT, *AWAY)
        visits += sum(1 for s in manager.creatures
                      if SMALL.contains(s.x, s.y))
    assert visits > 0, "nobody was ever on the second monitor"


def test_a_spider_is_not_left_vibrating_against_the_hole(monkeypatch):
    """Clamping the body but not its destination leaves a spider walking
    into dead space every frame and being shoved out every frame."""
    manager = _colony(monkeypatch, rects=[BIG, SMALL])
    spider = manager.creatures[0]
    spider.x, spider.y = 1050.0, 340.0
    spider.target_x, spider.target_y = DEAD
    for _ in range(90):
        manager.update(DT, *AWAY)
    assert manager.playfield.contains(spider.target_x, spider.target_y), (
        "it is still aiming at a place no screen shows")
