"""A base is dug earth you can pick up and carry (DC-51).

Two things the owner asked for after watching a colony run.

*"crate actual dirt patches instead of gemetrical mould. meake it more
realistic."* A base used to be a dashed circle with a dozen `drawChord`
domes inside it, each with a smaller dome for a crest and a team-coloured
arc on top once it was finished. Every one of those is an exact geometric
primitive, and a dozen of them together read as a diagram of a base rather
than as soil. What replaced them -- ragged, smoothed, overlapping patches on
a field of damp earth -- is checked here for the properties that make it
*not* the old thing, because "looks like dirt" is not something a test can
assert and "is not a circle" is.

*"removing the bases with right click or moving them somewhere."* Removing
one already worked (DC-49). Moving is the half that did not exist, and it is
the kinder half: a base is where a team heals and banks its food, so
deleting one to get it out of an awkward corner costs the team both.

`tests/test_base_render_golden.py` holds the picture itself; this holds the
reasons it looks the way it does.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.world.jobs import (
    MAX_BUILD_PROGRESS,
    PATCH_POINTS,
    BaseSite,
    BaseWorld,
    _PATCH_CACHE,
    _blend,
    patch_recipe,
)

SCREEN = (1400, 900)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


@pytest.fixture
def colony(monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="dc51-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "one_team.json"
    preset.write_text(json.dumps({
        "name": "one_team",
        "slots": [{"model": "tarantula", "personality": "mellow", "count": 1,
                   "slot_id": "a", "team": "hunters", "job": "builder"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=4)
    creature = manager.creatures[0]
    creature.x, creature.y = 400.0, 400.0
    manager.base_world.ensure_site(creature)
    return manager


def _site(manager) -> BaseSite:
    return next(iter(manager.base_world.bases.values()))


# --------------------------------------------------------------- dirt patches

def test_a_patch_is_not_a_circle():
    """The single property that separates a patch from the old dome.

    A `drawChord` dome has one radius; this outline has a different one at
    every point, and the spread has to be big enough to see. At a wobble of
    0.05 the shape would pass every other check here and still look like an
    ellipse on screen.
    """
    outline = patch_recipe("demo|1:patch:10.00:20.00")["body"]
    assert len(outline) == PATCH_POINTS
    radii = [math.hypot(x, y / 0.58) for x, y in outline]
    spread = (max(radii) - min(radii)) / (sum(radii) / len(radii))
    assert spread > 0.2, f"the outline is near-circular: {spread:.3f}"


def test_a_patch_is_closed_and_stays_near_its_centre():
    """Ragged, not spiky: a vertex twice as far out as its neighbours reads
    as a splash rather than as earth."""
    for part in ("body", "crown"):
        outline = patch_recipe("demo|1:patch:10.00:20.00")[part]
        for x, y in outline:
            assert 0.4 <= math.hypot(x, y / 0.58) <= 1.6, (part, x, y)


def test_the_same_base_digs_the_same_earth_in_the_next_process():
    """Placement is derived, not saved, so a restart must reproduce it.

    `random.Random(str)` is stable across processes where `hash(str)` is not
    -- Python salts string hashing per run -- and this is the check that
    stops someone swapping one for the other.
    """
    key = "colony|7:patch:100.00:200.00"
    first = patch_recipe(key)
    _PATCH_CACHE.clear()
    second = patch_recipe(key)
    assert first == second
    assert patch_recipe("colony|7:patch:100.00:201.00") != first


def test_two_mounds_of_one_base_are_not_the_same_shape():
    site = BaseSite(id="shape|1", owner_id="o", team_id="hunters",
                    x=400.0, y=300.0, build_progress=MAX_BUILD_PROGRESS)
    shapes = {
        tuple(patch_recipe(f"{site.id}:patch:{mx:.2f}:{my:.2f}")["body"])
        for mx, my, *_rest in site.mounds()
    }
    assert len(shapes) == len(site.mounds())


def test_the_team_tint_leaves_the_soil_brown():
    """The first attempt blended 16% into the body and 30% into the crown,
    and a finished base rendered as a heap of coloured pebbles. Whatever the
    team colour is, including a pure one, the earth has to stay earth."""
    for colour in ((255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)):
        soil = _blend((86, 62, 42), colour, 0.08)
        # Red stays the strongest channel, which is what "brown" means here.
        # Green over blue is *not* claimed: a pure-blue team does lift blue
        # past green, and a tint too weak to do that would be invisible.
        assert soil[0] > soil[1], (colour, soil)
        assert soil[0] > soil[2], (colour, soil)


def test_the_base_renderer_draws_no_chords_or_ellipse_outline():
    """The old shapes, pinned as gone.

    Source-level because the alternative is proving a negative about pixels.
    `drawChord` was the dome and the coloured crest arc; the dashed
    `drawEllipse` was the ring around the site. The only ellipse left is the
    alert flash, which is a transient warning rather than part of the base,
    and the loose clods.
    """
    source = (Path(__file__).resolve().parents[1] / "src" / "desktop_bug"
              / "world" / "jobs.py").read_text(encoding="utf-8")
    render = source[source.index("    def render(self, painter, clip=None)"):]
    assert "drawChord" not in render
    assert "Qt.DashLine" not in render, "the ring around the site is back"


def test_an_unbuilt_base_draws_nothing_at_all(qapp):
    """The dashed circle used to mark a site before a grain had been moved.

    It was an area marker drawn around the base rather than the base itself,
    which is exactly the geometry the owner objected to.
    """
    from PyQt5.QtGui import QColor, QImage, QPainter

    world = BaseWorld(400, 400)
    site = BaseSite(id="empty|1", owner_id="o", team_id="hunters",
                    x=200.0, y=200.0, build_progress=0.0)
    world.bases[site.id] = site
    image = QImage(400, 400, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(0, 0, 0, 0))
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    world.render(painter)
    painter.end()

    painted = sum(
        1
        for y in range(0, 400, 2)
        for x in range(0, 400, 2)
        if image.pixelColor(x, y).alpha() > 8
    )
    assert painted == 0, f"{painted} pixels drawn for a base nobody has dug"


def test_the_dug_area_grows_with_the_base(qapp):
    """A base halfway through has to look halfway through."""
    from PyQt5.QtGui import QColor, QImage, QPainter

    def painted(completion: float) -> int:
        world = BaseWorld(400, 400)
        site = BaseSite(id="grow|1", owner_id="o", team_id="hunters",
                        x=200.0, y=200.0, level=3,
                        build_progress=MAX_BUILD_PROGRESS * completion)
        world.bases[site.id] = site
        image = QImage(400, 400, QImage.Format_ARGB32_Premultiplied)
        image.fill(QColor(0, 0, 0, 0))
        painter = QPainter(image)
        painter.setRenderHint(QPainter.Antialiasing, True)
        world.render(painter)
        painter.end()
        return sum(
            1
            for y in range(400)
            for x in range(400)
            if image.pixelColor(x, y).alpha() > 20
        )

    quarter, full = painted(0.25), painted(1.0)
    assert full > quarter * 1.5, (quarter, full)


# ----------------------------------------------------------------- moving one

def test_a_base_can_be_carried_somewhere_else(colony):
    site = _site(colony)
    site.build_progress = MAX_BUILD_PROGRESS * 0.7
    site.level = 2
    site.resources = 9.0

    message = colony.move_base(site, 900.0, 650.0)

    assert (site.x, site.y) == (900.0, 650.0)
    assert "oved" in message, message
    # What was dug stays dug, and the larder comes with it.
    assert site.build_progress == pytest.approx(MAX_BUILD_PROGRESS * 0.7)
    assert site.level == 2
    assert site.resources == pytest.approx(9.0)


def test_the_earth_travels_with_the_base(colony):
    """Mound offsets are relative to the centre, so the pile keeps its shape
    rather than being re-dug somewhere else."""
    site = _site(colony)
    site.build_progress = MAX_BUILD_PROGRESS
    before = [(round(x - site.x, 6), round(y - site.y, 6))
              for x, y, *_rest in site.mounds()]
    colony.move_base(site, 880.0, 300.0)
    after = [(round(x - site.x, 6), round(y - site.y, 6))
             for x, y, *_rest in site.mounds()]
    assert after == before


def test_a_base_cannot_be_dropped_off_the_desktop(colony):
    site = _site(colony)
    colony.move_base(site, -500.0, 99999.0)
    assert 0.0 < site.x < SCREEN[0]
    assert 0.0 < site.y < SCREEN[1]


def test_moving_a_base_that_is_gone_says_so(colony):
    site = _site(colony)
    colony.remove_base(site)
    assert "no base" in colony.move_base(site, 100.0, 100.0)


def test_the_click_target_follows_the_base(colony):
    """Whatever `base_at` finds after a move has to be the moved base, or a
    second right-click would offer to act on empty ground."""
    site = _site(colony)
    assert colony.base_at(400.0, 400.0) is site
    colony.move_base(site, 1000.0, 700.0)
    assert colony.base_at(400.0, 400.0) is None
    assert colony.base_at(1000.0, 700.0) is site


def test_the_world_reports_whether_it_moved_anything():
    world = BaseWorld(*SCREEN)
    assert world.move_base("nobody", 10.0, 10.0) is False
    site = BaseSite(id="here|1", owner_id="o", team_id="hunters", x=50.0, y=50.0)
    world.bases[site.id] = site
    assert world.move_base("here|1", 300.0, 300.0) is True


# ------------------------------------------------- picking one up and putting
# it down
#
# The overlay's right-click handler is a QMenu built inside a live, frameless,
# click-through window, which is not a thing to construct in a test. The state
# the two clicks pass between them is, and that is where this can actually go
# wrong: a base removed while it is being carried, or a stale id offering to
# put down something that no longer exists.

class _Carrier:
    """Just enough of OverlayWindow for the carry helpers to run on."""

    def __init__(self, manager):
        self.manager = manager
        self._moving_base_id = None
        self.said = []
        self.repainted = 0

    def _announce(self, message):
        self.said.append(message)
        return message

    def _request_full_repaint(self):
        self.repainted += 1

    # Bound from the real class, so this tests the shipped code rather than a
    # copy of it that can drift.
    from desktop_bug.app.engine import OverlayWindow
    _carried_base = OverlayWindow._carried_base
    _pick_up_base = OverlayWindow._pick_up_base
    _cancel_base_move = OverlayWindow._cancel_base_move
    _drop_base = OverlayWindow._drop_base


def test_picking_a_base_up_and_putting_it_down(colony):
    carrier = _Carrier(colony)
    site = _site(colony)

    carrier._pick_up_base(site)
    assert carrier._carried_base() is site
    assert "Right-click" in carrier.said[-1]

    carrier._drop_base(site, 1100.0, 200.0)
    assert (site.x, site.y) == (1100.0, 200.0)
    assert carrier._carried_base() is None
    assert carrier.repainted == 1, "a moved base was not repainted"


def test_a_base_removed_mid_carry_is_simply_forgotten(colony):
    """Otherwise the next right-click offers to put down a deleted base."""
    carrier = _Carrier(colony)
    site = _site(colony)
    carrier._pick_up_base(site)
    colony.remove_base(site)
    assert carrier._carried_base() is None
    assert carrier._moving_base_id is None


def test_the_carry_can_be_called_off(colony):
    carrier = _Carrier(colony)
    carrier._pick_up_base(_site(colony))
    carrier._cancel_base_move()
    assert carrier._carried_base() is None


def test_the_overlay_menu_offers_both_moving_and_clearing():
    """Source-level, for the same reason as the renderer check above: the menu
    itself needs a live overlay window, and what matters is that the entries
    exist at all -- removing a single base shipped without either."""
    source = (Path(__file__).resolve().parents[1] / "src" / "desktop_bug"
              / "app" / "engine.py").read_text(encoding="utf-8")
    assert "Move the {team_name} base" in source
    assert "Put the {carried_name} base down here" in source
    assert "Remove every base" in source


# ------------------------------------------------------------ dragging one
#
# "make it possible to right clich the base and move it. drag or remove it."
# The right-click route shipped first; this is the direct one. A base is the
# largest thing drawn on the overlay and spiders stand on top of their own, so
# most of what can go wrong here is about priority and about how much desktop
# the overlay is allowed to claim.

def _press(manager, x, y):
    manager.update(1.0 / 60.0, x, y, mouse_down=True, mouse_pressed=True)


def _drag(manager, x, y):
    manager.update(1.0 / 60.0, x, y, mouse_down=True)


def _release(manager, x, y):
    manager.update(1.0 / 60.0, x, y, mouse_released=True)


def _ready_to_grab(manager):
    """A finished base with nobody standing on it.

    Spiders win the grab over the earth they are standing on, which is the
    point of the ordering and is checked on its own below. Every test that
    wants to grab the *base* has to get the builder out of the way first --
    the fixture puts it exactly on its own base, because that is where a
    builder spends its life.
    """
    site = _site(manager)
    site.build_progress = MAX_BUILD_PROGRESS
    for creature in manager.creatures:
        creature.x, creature.y = 50.0, 850.0
        creature._initialize_legs()
    return site


def test_a_base_can_be_dragged(colony):
    site = _ready_to_grab(colony)
    start = (site.x, site.y)

    _press(colony, site.x, site.y)
    assert colony._dragged_base is site
    _drag(colony, start[0] + 200.0, start[1] + 120.0)
    _release(colony, start[0] + 200.0, start[1] + 120.0)

    assert colony._dragged_base is None
    assert site.x == pytest.approx(start[0] + 200.0)
    assert site.y == pytest.approx(start[1] + 120.0)


def test_the_base_does_not_jump_to_the_cursor(colony):
    """Grabbed by its edge, it has to stay grabbed by its edge."""
    site = _ready_to_grab(colony)
    grab_x, grab_y = site.x + 18.0, site.y + 6.0
    offset = (site.x - grab_x, site.y - grab_y)

    _press(colony, grab_x, grab_y)
    _drag(colony, 900.0, 500.0)
    assert (site.x - 900.0, site.y - 500.0) == pytest.approx(offset)


def test_dragging_a_base_carries_its_earth_unchanged(colony):
    """The shapes are seeded per patch. Seeded from world position they were
    re-rolled on every frame of a drag, which shimmered and grew the cache
    without bound; they are seeded from the offset within the base instead."""
    site = _ready_to_grab(colony)
    before = [patch_recipe(f"{site.id}:patch:{mx - site.x:.2f}:{my - site.y:.2f}")["body"]
              for mx, my, *_rest in site.mounds()]

    _press(colony, site.x, site.y)
    _drag(colony, 1000.0, 600.0)
    _release(colony, 1000.0, 600.0)

    after = [patch_recipe(f"{site.id}:patch:{mx - site.x:.2f}:{my - site.y:.2f}")["body"]
             for mx, my, *_rest in site.mounds()]
    assert after == before


def test_a_spider_standing_on_its_base_is_still_grabbable(colony):
    """The failure this ordering exists to prevent. A builder spends most of
    its life inside its own base's footprint."""
    site = _site(colony)
    site.build_progress = MAX_BUILD_PROGRESS
    creature = colony.creatures[0]
    creature.x, creature.y = site.x, site.y
    creature._initialize_legs()

    _press(colony, site.x, site.y)
    assert colony.dragged_creature is creature
    assert colony._dragged_base is None


def test_a_base_is_not_grabbable_when_dragging_is_off(colony):
    site = _ready_to_grab(colony)
    colony.set_interferable(False)
    assert colony.base_grab_at(site.x, site.y) is None
    _press(colony, site.x, site.y)
    assert colony._dragged_base is None


def test_letting_go_of_the_base_when_dragging_is_switched_off(colony):
    """Mid-drag, the same way a held spider is released."""
    site = _ready_to_grab(colony)
    _press(colony, site.x, site.y)
    assert colony._dragged_base is site
    colony.set_interferable(False)
    colony.update(1.0 / 60.0, site.x, site.y, mouse_down=True)
    assert colony._dragged_base is None


def test_the_grab_area_follows_the_earth_not_the_site(colony):
    """`base_at` reaches the whole site radius so a right-click is
    forgiving. The grab follows the dug soil instead, which matters most
    while a base is being built: a level-5 site barely started is 80px of
    radius with almost no earth in it, and claiming all of that from the
    mouse would put a large invisible dead zone on the desktop.

    Once a base is finished the two are close, by design -- at that point
    there really is earth all the way out."""
    site = _site(colony)
    site.level = 5
    site.build_progress = MAX_BUILD_PROGRESS * 0.3
    edge_x = site.x + site.radius - 2.0
    assert colony.base_at(edge_x, site.y) is site
    assert colony.base_grab_at(edge_x, site.y) is None
    assert colony.base_grab_at(site.x, site.y) is site


def test_a_barely_started_base_can_still_be_picked_up(colony):
    """Otherwise the one you most want to move is the one you cannot."""
    site = _site(colony)
    site.level = 0
    site.build_progress = 0.0
    assert colony.base_grab_at(site.x + 10.0, site.y) is site


def test_the_overlay_claims_the_mouse_over_the_earth_and_not_beside_it(colony):
    site = _ready_to_grab(colony)
    assert colony.wants_mouse(site.x, site.y) is True
    assert colony.wants_mouse(site.x + 400.0, site.y + 300.0) is False


def test_a_drag_does_not_write_the_save_file_every_frame(colony, monkeypatch):
    """Sixty saves a second, otherwise. It saves once, on release."""
    saves = []
    site = _ready_to_grab(colony)
    monkeypatch.setattr(type(colony), "save_runtime_state",
                        lambda self: saves.append(1))
    _press(colony, site.x, site.y)
    for step in range(30):
        _drag(colony, site.x + step * 4.0, site.y)
    assert saves == []
    _release(colony, site.x + 120.0, site.y)
    assert len(saves) == 1


def test_a_base_cannot_be_dragged_off_the_desktop(colony):
    site = _ready_to_grab(colony)
    _press(colony, site.x, site.y)
    _drag(colony, -800.0, -800.0)
    _release(colony, -800.0, -800.0)
    assert 0.0 < site.x < SCREEN[0]
    assert 0.0 < site.y < SCREEN[1]
