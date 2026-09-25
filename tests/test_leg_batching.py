"""One spider's legs are issued as a few paths, not a few hundred draws (DC-71).

The owner, looking at twenty spiders crawling at 21 FPS: *"maybe we should
consider utilising more cpu threads and /or gpu too? ... what is limtiing us
compared to them?"*

Profiling the shipped renderer named the suspects, and the answer was not the
one the call count suggested:

    drawEllipse   397 calls/frame   10.1% of render
    drawLine      246 calls/frame    9.0%
    setPen        391 calls/frame    1.7%

`setPen` is nearly free, so "too many pen changes" was wrong. Batching takes
the leg pass from 128 lines and 152 ellipses down to 28 stroke draws and 7
fills -- **8.5x fewer calls -- and buys 5%**. Measured paired, alternating
batched and unbatched frames inside one process over 120 pairs, three times:
+1.28, +1.33 and +1.38 ms saved per frame at twenty tarantulas, batched
faster in 115, 118 and 118 pairs of 120.

Five percent for eight and a half times fewer calls is the finding. The
overhead of talking to Qt was never the ceiling; filling the pixels is. That
is the number to keep, because it is the one that says a renderer rewrite in
this direction has nothing much left to give and the GPU is the only real
lever.

The tests below are call counts and pixel comparisons, not timings. A
stopwatch here would be flaky -- two launches of `tools/benchmark.py` on this
machine disagreed by more than the effect, with the `creatures` section, which
no renderer change can touch, swinging 7.7 to 10.0 ms between runs.
"""

from __future__ import annotations

import tempfile

import pytest
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QImage, QPainterPath

from desktop_bug.creature import render_batch as RB
from desktop_bug.creature.render_batch import (
    LAYER_CORE,
    LAYER_FOOT,
    LAYER_JOINT,
    LAYER_SEGMENT,
    LegBatch,
)

import test_creature_render_golden as G

UNBATCHED = G.GOLDEN.parent / "creature_render_unbatched.png"


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Rendering needs a live QApplication; see conftest.qapp for why."""


@pytest.fixture(autouse=True)
def _batching_back_on():
    yield
    RB.BATCH_LEGS = True


def _render(batched: bool, monkeypatch) -> QImage:
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="dc71-"))
    RB.BATCH_LEGS = batched
    return G.render_reference_frame()


# --------------------------------------------------- the picture, both ways

def test_turning_batching_off_matches_the_unbatched_reference(monkeypatch):
    """With batching off, match the unbatched image at the current spider size."""
    assert UNBATCHED.exists(), "the pre-batch reference is missing"
    before = QImage(str(UNBATCHED))
    assert not before.isNull()
    assert G.max_channel_difference(_render(False, monkeypatch), before) <= G.PNG_TOLERANCE


def test_batching_moves_only_a_little_and_only_where_expected(monkeypatch):
    """Two same-colour translucent circles drawn one after the other
    composite twice; the same two inside one filled path composite once. So
    the joint-node rims shift slightly, and the leftover-brush sliver on the
    spline legs goes. Nothing else may."""
    before = QImage(str(UNBATCHED))
    live = _render(True, monkeypatch)
    differing = 0
    for y in range(live.height()):
        for x in range(live.width()):
            if live.pixel(x, y) != before.pixel(x, y):
                differing += 1
    total = live.width() * live.height()
    # Measured: 1.11% on this five-model colony, 0.59% on twenty tarantulas.
    # The colony moves more because four of its models take the spline branch
    # and so lose the leftover-brush sliver as well as the joint rims.
    assert differing / total < 0.02, (
        f"{differing} of {total} pixels moved ({100.0 * differing / total:.2f}%); "
        "batching should be a regrouping, not a redesign"
    )
    assert differing > 0, "nothing moved at all, so this test proves nothing"


def test_the_batched_render_is_deterministic(monkeypatch):
    """Grouping walks dictionaries. If their order ever varied, the golden
    reference would be pinning luck."""
    a = _render(True, monkeypatch)
    b = _render(True, monkeypatch)
    assert G.max_channel_difference(a, b) == 0


# ------------------------------------------------------ the call count itself

def _count_draws(batched: bool, monkeypatch) -> dict:
    tally = {"lines": 0, "dots": 0, "points": 0, "issued": 0}
    real_line, real_dot = LegBatch.line, LegBatch.dot
    real_point, real_flush = LegBatch.point, LegBatch.flush

    def line(self, *a, **k):
        tally["lines"] += 1
        if self._direct is not None:
            tally["issued"] += 1
        return real_line(self, *a, **k)

    def dot(self, *a, **k):
        tally["dots"] += 1
        if self._direct is not None:
            tally["issued"] += 1
        return real_dot(self, *a, **k)

    def point(self, *a, **k):
        tally["points"] += 1
        if self._direct is not None:
            tally["issued"] += 1
        return real_point(self, *a, **k)

    def flush(self, painter):
        n = real_flush(self, painter)
        tally["issued"] += n
        return n

    # Restored by hand rather than through monkeypatch, because this helper
    # is called twice in one test: the second call would otherwise capture the
    # first call's wrapper as "the real one" and keep the first tally counting
    # through the second render, which is exactly how direct mode appeared to
    # draw twice as many primitives as the batched one.
    LegBatch.line, LegBatch.dot = line, dot
    LegBatch.point, LegBatch.flush = point, flush
    try:
        _render(batched, monkeypatch)
    finally:
        LegBatch.line, LegBatch.dot = real_line, real_dot
        LegBatch.point, LegBatch.flush = real_point, real_flush
    return tally


def test_batching_collapses_the_leg_pass(monkeypatch):
    direct = _count_draws(False, monkeypatch)
    batched = _count_draws(True, monkeypatch)
    primitives = batched["lines"] + batched["dots"] + batched["points"]
    assert primitives == direct["lines"] + direct["dots"] + direct["points"], \
        "the two modes must draw the same things, only grouped differently"
    assert direct["issued"] == primitives, "passthrough must issue one draw each"
    # This was a fourfold floor, and 280 primitives collapsing to 33 draws,
    # until DC-73 landed. DC-73 removes the joint nodes, and the joint nodes
    # were precisely what batched best -- 152 ellipses into 7 fills, because a
    # radius lives inside the path while a stroke width does not. What is left
    # is mostly strokes, which group only by width, so 122 primitives become
    # 44 draws. The two packages overlap rather than add: DC-73 takes away the
    # work DC-71 was best at avoiding.
    assert batched["issued"] * 2 < primitives, (
        f"{primitives} primitives became {batched['issued']} draws; "
        "expected at least a twofold collapse"
    )


# ---------------------------------------------------------- the batch itself

class _Recorder:
    """Enough of a painter to record the order things were issued in."""

    def __init__(self):
        self.calls = []

    def setPen(self, pen):
        self.calls.append(("pen", pen if pen == Qt.NoPen else pen.color().rgba()))

    def setBrush(self, brush):
        self.calls.append(("brush", brush if brush == Qt.NoBrush else brush.color().rgba()))

    def drawPath(self, path):
        self.calls.append(("path", path.elementCount()))

    def drawPoints(self, *pts):
        self.calls.append(("points", len(pts)))

    def drawPoint(self, *a):
        self.calls.append(("point", None))

    def drawLine(self, *a):
        self.calls.append(("line", None))

    def drawEllipse(self, *a):
        self.calls.append(("ellipse", None))


def test_layers_are_issued_in_the_order_the_renderer_produced_them():
    """Hair under segments, joints over both, toes last. Batching gives up
    the leg-by-leg interleaving; it must not give up this."""
    batch = LegBatch()
    red, green, blue = QColor(255, 0, 0), QColor(0, 255, 0), QColor(0, 0, 255)
    # deliberately added out of order
    batch.point(LAYER_FOOT, blue, 1.0, 9.0, 9.0)
    batch.dot(LAYER_CORE, green, 5.0, 5.0, 1.0)
    batch.line(LAYER_SEGMENT, red, 2.0, 0.0, 0.0, 1.0, 1.0)
    batch.dot(LAYER_JOINT, green, 4.0, 4.0, 2.0)
    rec = _Recorder()
    assert batch.flush(rec) == 4
    kinds = [c[0] for c in rec.calls if c[0] in ("path", "points")]
    assert kinds == ["path", "path", "path", "points"], rec.calls


def test_same_colour_and_width_share_one_draw():
    batch = LegBatch()
    color = QColor(10, 20, 30, 200)
    for i in range(12):
        batch.line(LAYER_SEGMENT, color, 3.0, float(i), 0.0, float(i), 5.0)
    rec = _Recorder()
    assert batch.flush(rec) == 1


def test_a_different_width_cannot_share_a_pen():
    """Width is pen state, so it splits the group. This is why batching the
    strokes is worth so much less than batching the joint nodes: leg widths
    grow with `leg.lift`, so eight legs give eight widths per segment."""
    batch = LegBatch()
    color = QColor(10, 20, 30, 200)
    batch.line(LAYER_SEGMENT, color, 3.0, 0.0, 0.0, 1.0, 1.0)
    batch.line(LAYER_SEGMENT, color, 3.5, 0.0, 0.0, 1.0, 1.0)
    rec = _Recorder()
    assert batch.flush(rec) == 2


def test_ellipses_of_any_size_still_share_one_draw():
    """The opposite case, and the reason the joint nodes collapse to seven
    draws for a whole frame: a radius lives inside the path, not in the
    brush."""
    batch = LegBatch()
    color = QColor(90, 70, 60, 225)
    for i in range(1, 20):
        batch.dot(LAYER_JOINT, color, float(i), 0.0, i * 0.3)
    rec = _Recorder()
    assert batch.flush(rec) == 1


def test_a_toe_stays_a_point():
    """It was a `drawPoint` with a round-cap pen. Rewriting it as a filled
    circle of half the pen width looks like the same thing and is not: at a
    one-pixel width the pen paints one solid pixel and the circle
    antialiases to a faint blob. That moved 544 pixels across twenty spiders
    before it was caught, so `drawPoints` -- which batches *and* rasterises
    each one exactly as `drawPoint` did -- is load-bearing."""
    batch = LegBatch()
    color = QColor(90, 70, 60, 220)
    for i in range(8):
        batch.point(LAYER_FOOT, color, 1.0, float(i), 0.0)
    rec = _Recorder()
    assert batch.flush(rec) == 1
    assert ("points", 8) in rec.calls, rec.calls


def test_the_width_step_is_off():
    """Snapping widths would let more strokes share a pen. Measured, it buys
    nothing -- 20 stroke draws instead of 28 at a 0.25px step, with no change
    in frame time, and at 0.5px the frame got *slower*, because a fatter
    stroke covers more pixels and pixels are what actually cost. It stays
    available and stays off."""
    assert RB.LEG_WIDTH_STEP == 0.0
    stepped = LegBatch(width_step=0.5)
    color = QColor(1, 2, 3)
    stepped.line(LAYER_SEGMENT, color, 3.1, 0.0, 0.0, 1.0, 0.0)
    stepped.line(LAYER_SEGMENT, color, 3.2, 0.0, 0.0, 1.0, 0.0)
    rec = _Recorder()
    assert stepped.flush(rec) == 1, "both snap to 3.0 and should share a pen"
    apart = LegBatch(width_step=0.5)
    apart.line(LAYER_SEGMENT, color, 3.1, 0.0, 0.0, 1.0, 0.0)
    apart.line(LAYER_SEGMENT, color, 3.4, 0.0, 0.0, 1.0, 0.0)
    assert apart.flush(_Recorder()) == 2, "3.0 and 3.5 are different pens"


def test_an_empty_batch_issues_nothing():
    rec = _Recorder()
    batch = LegBatch()
    assert batch.is_empty()
    assert batch.flush(rec) == 0
    assert rec.calls == []


def test_passthrough_draws_immediately_and_flushes_nothing():
    rec = _Recorder()
    batch = LegBatch(direct=rec)
    batch.line(LAYER_SEGMENT, QColor(1, 2, 3), 2.0, 0.0, 0.0, 1.0, 1.0)
    batch.dot(LAYER_JOINT, QColor(1, 2, 3), 0.0, 0.0, 1.0)
    batch.point(LAYER_FOOT, QColor(1, 2, 3), 1.0, 0.0, 0.0)
    assert [c[0] for c in rec.calls if c[0] in ("line", "ellipse", "point", "points")] == \
        ["line", "ellipse", "point"], rec.calls
    assert batch.flush(rec) == 0


def test_passthrough_leaves_the_brush_alone_on_a_path():
    """Not a nicety -- it is the whole of why `add_path` has a passthrough
    branch of its own. The shipped renderer stroked the spline legs with
    whatever brush was current, filling them; reproducing that is what makes
    `test_turning_batching_off_restores_the_previous_picture` able to reach
    0/255."""
    rec = _Recorder()
    batch = LegBatch(direct=rec)
    path = QPainterPath()
    path.moveTo(0.0, 0.0)
    path.lineTo(5.0, 5.0)
    batch.add_path(LAYER_SEGMENT, QColor(1, 2, 3), 2.0, path)
    assert not any(c[0] == "brush" for c in rec.calls), rec.calls


def test_the_renderer_really_goes_through_the_batch(monkeypatch):
    """Everything above is theatre if `_render_procedural` stopped calling it."""
    seen = {"n": 0}
    real = LegBatch.flush

    def counting(self, painter):
        seen["n"] += 1
        return real(self, painter)

    monkeypatch.setattr(LegBatch, "flush", counting)
    _render(True, monkeypatch)
    assert seen["n"] > 0
