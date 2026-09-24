"""Bases are on a real screen, and spiders go to the ones you can see (DC-85).

The owner: *"spiders seem to want to go to a base that is above the screen and
they get stuck there at the border and legs fly everywhere ... they seem to be
moving towards invisible bases that were made before but the current moulds
are ignored."*

Their saved state held 27 bases. Twenty were lone spiders' bases that were
never built -- invisible -- left by spiders long gone. "neutral" was treated
as a team, so a lone spider looked after the nearest of all 25 neutral bases:
six of their seven spiders were walking to empty ground. Two built bases sat
in the dead zone of their two-monitor desktop (the bounding box includes an
area no screen shows), and two more sat so near a monitor's edge that their
guards' watch posts were off it. Replayed from a copy of that state on the
owner's real monitor layout: 27 bases -> 7, all on screen, and every spider
heading for a base with earth on it.
"""

from __future__ import annotations

from types import SimpleNamespace

from desktop_bug.world.jobs import GUARD_STANDOFF_PAD, BaseSite, BaseWorld
from desktop_bug.world.playfield import Playfield, ScreenRect

# The owner's layout, overlay-local: a 2560x1440 monitor dropped 718 px, and a
# 3840x2160 one beside it. The top-left 2560x718 is a dead zone.
RECTS = [ScreenRect(0.0, 718.0, 2560.0, 1440.0), ScreenRect(2560.0, 0.0, 3840.0, 2160.0)]


def _world(*sites: BaseSite) -> BaseWorld:
    world = BaseWorld(6400, 2160, [site.to_dict() for site in sites])
    playfield = Playfield(6400, 2160)
    playfield.set_rects(RECTS)
    world.playfield = playfield
    return world


def _site(site_id, owner, team="neutral", x=3000.0, y=1000.0, built=0.0, level=0):
    return BaseSite(id=site_id, owner_id=owner, team_id=team, x=x, y=y,
                    build_progress=built, level=level)


def _spider(key, team="neutral", x=3000.0, y=1000.0):
    return SimpleNamespace(progression=SimpleNamespace(team_id=team), progression_id=key,
                           index=0, x=x, y=y)


def _posts_on_screen(world, site):
    return world.playfield.contains(site.x, site.y, site.radius + GUARD_STANDOFF_PAD)


def test_a_base_in_the_dead_zone_is_moved_onto_a_monitor():
    site = _site("owner:a", "a", x=95.0, y=241.0, built=46.0)
    world = _world(site)
    world.keep_all_on_screen()
    moved = world.bases["owner:a"]
    assert world.playfield.contains(moved.x, moved.y), (moved.x, moved.y)
    assert _posts_on_screen(world, moved)


def test_a_base_at_a_monitor_edge_is_moved_clear_of_it():
    """team:rivals was 42 px from the top-right corner; its guards' posts were
    off the screen."""
    site = _site("team:rivals", "r", team="rivals", x=6358.0, y=42.0, level=5, built=120.0)
    world = _world(site)
    world.keep_all_on_screen()
    assert _posts_on_screen(world, world.bases["team:rivals"])


def test_a_base_already_well_on_screen_does_not_move():
    world = _world(_site("owner:a", "a", x=4000.0, y=1000.0, built=50.0))
    world.keep_all_on_screen()
    assert (world.bases["owner:a"].x, world.bases["owner:a"].y) == (4000.0, 1000.0)


def test_a_lone_spider_does_not_adopt_another_lone_spiders_empty_base():
    mine = _site("owner:me", "me", x=3000.0, y=1000.0, built=30.0)
    theirs = _site("owner:them", "them", x=3010.0, y=1000.0, built=0.0)
    world = _world(mine, theirs)
    spider = _spider("me", x=3010.0)
    assert world._site_for_team(spider) is world.bases["owner:me"]


def test_a_visible_base_beats_an_invisible_one():
    """With nothing built on its own base, a lone spider looks after the
    nearest base that has earth on it -- not the nearest empty one."""
    own = _site("owner:me", "me", x=3000.0, y=1000.0, built=0.0)
    far_built = _site("owner:gone", "gone", x=4500.0, y=1500.0, built=80.0)
    world = _world(own, far_built)
    assert world._site_for_team(_spider("me")) is world.bases["owner:gone"]


def test_a_team_prefers_its_built_base():
    empty = _site("team:x", "a", team="x", x=3000.0, y=1000.0, built=0.0)
    built = _site("team:x2", "b", team="x", x=5000.0, y=1500.0, built=60.0)
    world = _world(empty, built)
    assert world._site_for_team(_spider("c", team="x")) is world.bases["team:x2"]


def test_empty_bases_of_spiders_that_are_gone_are_dropped():
    world = _world(
        _site("owner:gone-empty", "gone-empty", built=0.0),
        _site("owner:gone-built", "gone-built", built=40.0),
        _site("owner:alive", "alive", built=0.0),
        _site("team:x", "someone", team="x", built=0.0),
    )
    world._prune_orphan_sites([_spider("alive")])
    assert set(world.bases) == {"owner:gone-built", "owner:alive", "team:x"}
