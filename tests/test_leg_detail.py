"""Leg detail too small to be seen is not drawn (DC-73).

The owner, on being shown that a GPU framebuffer was worth only 24%, chose to
cut primitives instead. The reason that works is the awkward finding from
DC-71: about 60% of render is Qt turning each stroked path into an outline
polygon on one CPU core, and **that cost is per primitive regardless of how
big the primitive is**. Drawing something invisible costs full price.

A tarantula's ``base_size`` is 27px and each leg still draws 19 primitives.
Measured on twenty of them at their real size, paired inside one process,
100 pairs: **29.33 ms to 23.97 ms, +18.3%, faster in 87 of 100 pairs.** At
6x magnification the two pictures are indistinguishable.

Two separate findings, and they needed different treatment:

**The joint nodes were never visible at any size.** They are filled circles
on the leg's own centreline whose radius is smaller than the half-width of
the stroke they sit on -- 0.75 to 0.97 of it across the model's whole size
range. They are interior tint, not joints. Skipping them is 2.6% of pixels.

**The hair could not simply go.** It is a wider stroke drawn *under* the
segment, so removing it takes weight out of the silhouette and the legs read
as visibly thinner (8.8% of pixels). Below the size where its fringe reads as
a fringe it is merged instead -- one stroke at the hair's width, in the
segment's colour warmed toward the hair's.

The tests here are pixel comparisons and geometry, not timings. Two launches
of `tools/benchmark.py` on this machine disagree by more than the effect.
"""

from __future__ import annotations

import random
import tempfile

import pytest
from PyQt5.QtGui import QImage

from desktop_bug.content.discovery import discover_models, discover_personalities
from desktop_bug.creature import Creature
from desktop_bug.creature import render_detail as D
from desktop_bug.creature.render_detail import (
    hair_detail,
    hair_pen_width,
    joint_node_shows,
)
from support import ROOT

import test_creature_render_golden as G

FULL_DETAIL = G.GOLDEN.parent / "creature_render_full_detail.png"


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering needs a live QApplication; see conftest.qapp for why."""


@pytest.fixture(autouse=True)
def _levels_back_on():
    yield
    D.ENABLED = True


def _render(levels: bool, monkeypatch) -> QImage:
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="dc73-"))
    D.ENABLED = levels
    return G.render_reference_frame()


# ------------------------------------------------------- the picture, both ways

def test_turning_the_levels_off_matches_the_full_detail_reference(monkeypatch):
    """With detail levels off, match the full-detail image at the current size."""
    assert FULL_DETAIL.exists(), "the full-detail reference is missing"
    before = QImage(str(FULL_DETAIL))
    assert not before.isNull()
    assert G.max_channel_difference(_render(False, monkeypatch), before) <= G.PNG_TOLERANCE


def test_the_levels_move_only_a_little(monkeypatch):
    before = QImage(str(FULL_DETAIL))
    live = _render(True, monkeypatch)
    moved = sum(1 for y in range(live.height()) for x in range(live.width())
                if live.pixel(x, y) != before.pixel(x, y))
    total = live.width() * live.height()
    # Measured 9.46% on twenty tarantulas at 24.5px, where the whole spider is
    # legs; this five-model colony is less leg per pixel.
    assert moved > 0, "nothing changed, so this proves nothing"
    assert moved / total < 0.12, f"{100.0 * moved / total:.2f}% of pixels moved"


def test_the_levelled_render_is_deterministic(monkeypatch):
    assert G.max_channel_difference(_render(True, monkeypatch),
                                    _render(True, monkeypatch)) == 0


# --------------------------------------------------------------- joint nodes

def test_a_node_inside_its_own_leg_is_not_drawn():
    # the tarantula's patella at 22.6px: radius 1.66 against segments 3.42
    # and 2.96, so half the thinner one is 1.48 -- it stands 0.18px proud,
    # which is not a joint, it is a stain.
    assert not joint_node_shows(1.66, 3.42, 2.96)


def test_a_node_that_stands_proud_is_drawn():
    assert joint_node_shows(3.00, 3.42, 2.96)


def test_it_is_measured_against_the_thinner_neighbour():
    """A node only has to protrude somewhere to be seen."""
    assert joint_node_shows(2.2, 9.0, 3.0)
    assert not joint_node_shows(2.2, 9.0, 9.0)


def test_a_node_with_no_neighbours_is_kept():
    """Unsegmented models call this without widths; they must not go dark."""
    assert joint_node_shows(1.0)


def test_the_tarantulas_joints_never_protrude_at_any_size_it_reaches():
    """The finding this package rests on, stated so it fails if a model
    change ever makes the nodes visible again -- at which point they should
    be drawn, and this test should be the thing that says so."""
    models, personalities = discover_models(ROOT)[0], discover_personalities(ROOT)[0]
    seen = 0
    for scale in (0.9, 1.5, 2.2, 3.0, 4.5):
        random.seed(1)
        spider = Creature(models["tarantula"], personalities["balanced"],
                          1600, 900, size_scale=scale)
        chain = spider._sprite_leg_chain_config()
        assert chain is not None
        thickness = float(spider._appearance("leg_thickness", 1.0))
        node_scale = float(spider._appearance("joint_node_scale", 1.0))
        base_width = max(1.4, spider.size * 0.066 * thickness)
        node_r = max(1.0, base_width * 0.24 * node_scale)
        widths = [base_width * w for w in chain["width_scales"]]
        for index, joint_scale in enumerate(chain["joint_scales"]):
            radius = node_r * joint_scale * (1.12 if index == 1 else 0.96)
            assert not joint_node_shows(radius, *widths[index:index + 2]), (
                f"size {spider.size:.1f}, joint {index}: radius {radius:.2f} "
                f"against widths {widths[index:index + 2]}"
            )
            seen += 1
    assert seen == 5 * 4, seen


# ---------------------------------------------------------------------- hair

def test_a_thin_leg_merges_its_hair():
    detail = hair_detail(1.6, {"hairy": True, "hair_scale": 0.42})
    assert detail.merge_hair and not detail.separate_hair
    assert detail.strokes_per_segment == 1


def test_a_thick_leg_keeps_its_fringe():
    detail = hair_detail(5.8, {"hairy": True, "hair_scale": 0.42})
    assert detail.separate_hair and not detail.merge_hair
    assert detail.strokes_per_segment == 2


def test_a_smooth_model_has_no_hair_either_way():
    detail = hair_detail(5.8, {"hairy": False, "hair_scale": 0.0})
    assert not detail.separate_hair and not detail.merge_hair
    assert hair_detail(5.8, None).strokes_per_segment == 1


def test_the_merged_stroke_keeps_the_legs_weight():
    """The whole reason the hair is merged rather than dropped. Dropping it
    narrows the leg from the hair's width to the segment's, which is 8.8% of
    pixels and reads as thinner legs."""
    assert hair_pen_width(3.0, 0.42) == pytest.approx(3.0 * (1.12 + 0.42 * 0.55))
    assert hair_pen_width(3.0, 0.42) > 3.0


def test_the_level_is_decided_from_the_resting_width():
    """A level that changed with `leg.lift` would flicker segment by segment
    and leg by leg while a spider walked."""
    chain = {"hairy": True, "hair_scale": 0.42}
    # The renderer's own width is
    #   max(1.4, size * 0.066 * thickness * (1 + lift * 0.24)) * (1 + startle * 0.10)
    # so across a full step and a full startle the width a tarantula's leg is
    # actually drawn at spans this range at its default size.
    size = 24.5
    resting = max(1.4, size * 0.066)
    level = hair_detail(resting, chain)
    assert level.merge_hair
    for lift in (0.0, 0.5, 1.0):
        for startle in (0.0, 1.0):
            live = max(1.4, size * 0.066 * (1.0 + lift * 0.24)) * (1.0 + startle * 0.10)
            assert live >= resting
            assert hair_detail(live, chain) == level, (
                f"lift {lift}, startle {startle}: width {live:.2f} against "
                f"resting {resting:.2f} changed the level mid-walk"
            )


# -------------------------------------------------------------- and it is used

def test_the_renderer_consults_the_levels(monkeypatch):
    """Everything above is theatre if `_render_procedural` stopped asking."""
    calls = {"hair": 0, "joints": 0}
    import desktop_bug.creature.render_procedural as RP
    real_hair, real_joint = RP.hair_detail, RP.joint_node_shows

    def counted_hair(*a, **k):
        calls["hair"] += 1
        return real_hair(*a, **k)

    def counted_joint(*a, **k):
        calls["joints"] += 1
        return real_joint(*a, **k)

    monkeypatch.setattr(RP, "hair_detail", counted_hair)
    monkeypatch.setattr(RP, "joint_node_shows", counted_joint)
    _render(True, monkeypatch)
    assert calls["hair"] > 0
    assert calls["joints"] > 0


def test_a_hairy_tarantula_draws_half_the_leg_strokes_at_desktop_size():
    """19 primitives a leg becomes 6: five merged strokes and a toe."""
    models, personalities = discover_models(ROOT)[0], discover_personalities(ROOT)[0]
    random.seed(1)
    spider = Creature(models["tarantula"], personalities["balanced"],
                      1600, 900, size_scale=1.0)
    chain = spider._sprite_leg_chain_config()
    thickness = float(spider._appearance("leg_thickness", 1.0))
    base_width = max(1.4, spider.size * 0.066 * thickness)
    detail = hair_detail(base_width, chain)
    assert detail.merge_hair, f"at {spider.size:.1f}px the hair should merge"
    strokes = len(chain["width_scales"]) * detail.strokes_per_segment
    assert strokes == 5
