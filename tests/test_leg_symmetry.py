"""Legs bow away from the body, not up the screen (DC-67).

The owner, looking at a tarantula on the desktop: *"something feels off with
some legs. it seems some of them are under the body."*

``_sprite_leg_chain_points`` lifts each joint by ``rise`` to read as height
off the floor, and it did that by subtracting from the point's **screen** y.
Screen-up is not away from the body. For a spider facing right it is outward
for the four legs on one flank and straight across the carapace for the four
on the other, so:

    Δs = -rise · cos(heading)

one side's joints move outboard, the other's move inboard by exactly as much,
and at heading 90 or 270 the fault disappears entirely. Legs are drawn
underneath the shell, so an inboard proximal joint is not merely misplaced --
it is invisible, and the spider looks like it has legs coming out from under
itself. Measured on a neutral stance with the heading set by hand:

    heading   0 deg : left legs +0.43..+0.46 outboard of their sockets,
                      right legs -0.09..-0.12 (inboard, under the shell)
    heading  90 deg : both flanks +0.14..+0.19 -- symmetric, correct
    heading 180 deg : the same fault, mirrored onto the left flank

Only a spider walking straight up or down the screen was ever drawn right.

**Scope.** Eight of the 49 models use the solved leg chain, and the tarantula
is the only one that sets ``elevated_arc`` above zero -- for the other seven
``rise`` is 0.0 and this change is arithmetically a no-op. That is checked
below, because "it only affects the model we were asked to focus on" is the
kind of claim that should fail loudly if it stops being true.

Everything here sets the heading and the foot positions by hand instead of
walking a spider. That is deliberate: a first attempt measured this off a
240-frame walk and the numbers were not reproducible between runs -- three
runs disagreed about *which leg* was worst. A creature's walk draws from
streams this harness does not control, so the walk was removed from the
measurement rather than trusted.
"""

from __future__ import annotations

import math
import random

import pytest
from desktop_bug.content.discovery import discover_models, discover_personalities
from desktop_bug.creature import Creature
from support import ROOT

HEADINGS = (0.0, 30.0, 45.0, 90.0, 135.0, 180.0, 270.0)


@pytest.fixture(scope="module")
def content(qapp):
    models, _ = discover_models(ROOT)
    personalities, _ = discover_personalities(ROOT)
    return models, personalities


def _standing(content, model_id="tarantula", heading_deg=0.0, scale=3.0):
    """A spider at a known heading with every foot at its own rest pose.

    No stepping, no randomness in the pose: the stance is symmetric by
    construction, so any left/right difference in the result comes from the
    chain solver and nothing else.
    """
    models, personalities = content
    random.seed(1)
    spider = Creature(models[model_id], personalities["balanced"], 1600, 900,
                      size_scale=scale)
    spider.x, spider.y = 800.0, 450.0
    spider.heading = spider.target_heading = math.radians(heading_deg)
    spider._basis_cache = None
    fx, fy, rx, ry = spider._basis()
    for leg in spider.legs:
        d = leg.definition
        sign = spider._side_sign(d.get("side", "right"))
        rf = float(d["rest_forward"]) * spider.size
        rs = float(d["rest_side"]) * spider.size * sign
        leg.foot_x = spider.x + fx * rf + rx * rs
        leg.foot_y = spider.y + fy * rf + ry * rs
        leg.lift = 0.0
    return spider


def _outboard(spider, leg):
    """How far the first joint sits outside its own socket, in body widths.

    Negative means the joint has folded inboard -- under the shell, where it
    cannot be seen.
    """
    chain = spider._sprite_leg_chain_config()
    ax, ay = spider._leg_attach(leg)
    points = spider._sprite_leg_chain_points(leg, ax, ay, leg.foot_x, leg.foot_y, chain)
    _, root_s = spider._world_to_body_local(*points[0])
    _, joint_s = spider._world_to_body_local(*points[1])
    side = spider._side_sign(leg.definition.get("side", "right"))
    return (joint_s - root_s) * side / spider.size


# --------------------------------------------------------------- the fault

@pytest.mark.parametrize("heading", HEADINGS)
def test_no_leg_folds_its_first_joint_under_the_shell(content, heading):
    spider = _standing(content, heading_deg=heading)
    for leg in spider.legs:
        assert _outboard(spider, leg) >= 0.0, (
            f"{leg.definition['name']} at heading {heading} folds inboard")


@pytest.mark.parametrize("heading", HEADINGS)
def test_the_two_flanks_bow_by_the_same_amount(content, heading):
    """The bug in one line: this was the thing that was not true."""
    spider = _standing(content, heading_deg=heading)
    by_name = {leg.definition["name"]: _outboard(spider, leg) for leg in spider.legs}
    for stem in ("front", "mid_front", "mid_rear", "rear"):
        left = by_name[f"{stem}_left"]
        right = by_name[f"{stem}_right"]
        assert left == pytest.approx(right, abs=0.01), (
            f"{stem} at heading {heading}: left {left:.3f} vs right {right:.3f}")


def test_the_bow_does_not_depend_on_which_way_it_walks(content):
    """The arc is a fact about the spider, not about the compass.

    Before DC-67 the same leg bowed +0.43 facing one way and -0.12 facing the
    other, which is why the fault came and went as a spider turned.
    """
    measured = {}
    for heading in HEADINGS:
        spider = _standing(content, heading_deg=heading)
        measured[heading] = [_outboard(spider, leg) for leg in spider.legs]
    first = measured[HEADINGS[0]]
    for heading, values in measured.items():
        for expected, actual in zip(first, values):
            assert actual == pytest.approx(expected, abs=0.01), heading


def test_the_joints_still_lift_off_the_body(content):
    """Symmetry is not enough on its own -- zero bow everywhere would pass
    every test above and turn a tarantula into a starfish."""
    spider = _standing(content)
    bows = [_outboard(spider, leg) for leg in spider.legs]
    assert min(bows) > 0.05, bows


# ----------------------------------------------------------------- scope

def test_only_the_tarantula_is_affected(content):
    """``rise`` is ``size * elevated_arc * ...``, so a model with no elevated
    arc is untouched by this change however the lift is applied. Eight models
    solve a leg chain; exactly one of them arcs."""
    models, personalities = content
    arced = []
    for model_id, model in sorted(models.items()):
        chain = (model.get("appearance") or {}).get("leg_chain") or {}
        if not chain.get("enabled"):
            continue
        random.seed(1)
        spider = Creature(model, personalities["balanced"], 800, 600)
        if spider._sprite_leg_chain_config()["elevated_arc"] > 0.0:
            arced.append(model_id)
    assert arced == ["tarantula"], arced


# ------------------------------------------- DC-69: and not splayed flat

def test_the_bow_is_a_knee_fold_not_a_splay(content):
    """DC-67 fixed the symmetry and then kept pushing.

    It replaced a screen-space lift with a bow along the body's own outward
    axis *of the same magnitude*, which made the first joint bow 0.45
    body-widths at every heading where the original managed 0.15 at the
    vertical ones -- three times wider. Watched on a real desktop the
    spiders read as splayed flat, and the owner's verdict was "thats not how
    they supposed to lok like".

    The knee fold comes from `bend`, which is body-relative and was always
    symmetric. The screen-space term is gone (DC-69), so this is what is
    left, and it is the same number the original produced at the headings it
    drew correctly.
    """
    for heading in HEADINGS:
        spider = _standing(content, heading_deg=heading)
        for leg in spider.legs:
            bow = _outboard(spider, leg)
            # 0.30 until DC-83 moved the tarantula's sockets 0.09 body widths
            # inward, under the carapace rim, so the first joint is measured
            # from further in. The splay this guards against was 0.45.
            assert 0.05 < bow < 0.40, (
                f"{leg.definition['name']} at heading {heading} bows {bow:.3f}")


def test_the_pose_does_not_know_which_way_it_is_walking(content):
    """The property that was wrong underneath both bugs.

    A screen-space offset gives a leg a pose that depends on the compass.
    Nothing in a spider's anatomy does, so every joint of every leg must land
    in the same body-local place whichever way it faces.
    """
    reference = None
    for heading in HEADINGS:
        spider = _standing(content, heading_deg=heading)
        chain = spider._sprite_leg_chain_config()
        pose = []
        for leg in spider.legs:
            ax, ay = spider._leg_attach(leg)
            points = spider._sprite_leg_chain_points(
                leg, ax, ay, leg.foot_x, leg.foot_y, chain)
            for px, py in points:
                f, s = spider._world_to_body_local(px, py)
                pose.append((round(f / spider.size, 4), round(s / spider.size, 4)))
        if reference is None:
            reference = pose
        else:
            assert pose == pytest.approx(reference, abs=0.01), heading
