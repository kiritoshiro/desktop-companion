"""Collect one spider's leg drawing so it can be issued as a few paths.

Measured on the shipped renderer, a tarantula's legs cost 521 painter calls
per frame and the legs are 78% of render. Profiling which of those calls
actually cost anything was a surprise:

    drawEllipse   397 calls/frame   10.1% of render
    drawLine      246 calls/frame    9.0%
    setPen        391 calls/frame    1.7%

`setPen` is nearly free, so "too many pen changes" was the wrong diagnosis.
The cost is in the draws, and the largest single item is the joint nodes.

That split decides how each kind batches:

* **Fills batch cleanly.** An ellipse's radius lives inside the path, so
  every joint node on every leg that shares a colour can go into one
  ``QPainterPath`` and be filled once, whatever size each one is.
* **Strokes do not.** A line's width is pen state, and the shipped widths
  vary per leg -- they grow with ``leg.lift`` -- so eight legs of five
  segments produce forty distinct widths and forty groups of one. Grouping
  them at all means snapping widths to a step, which changes the picture.
  ``width_step`` is therefore explicit, and 0 disables it.

What batching gives up is the leg-by-leg interleaving: the shipped code draws
leg one entirely, then leg two, so leg two's segments cover leg one's joints.
Here every leg's segments are drawn, then every leg's joints. The layer order
within that -- hair under segments, joints over both, feet last -- is
preserved exactly, because each entry carries the layer it came from.
"""

from __future__ import annotations

from PyQt5.QtCore import QPointF, Qt
from PyQt5.QtGui import QBrush, QColor, QPainterPath, QPen

# Draw order. The shipped renderer produces these in this sequence within a
# single leg; flushing in the same sequence across all legs is what keeps the
# picture's structure when the per-leg interleaving goes away.
LAYER_SHADOW = -1   # DC-83: the legs' shadow on the ground, under everything
LAYER_FUZZ = 0      # the legacy unsegmented fluff spline, under everything
LAYER_HAIR = 1      # the hairy over-stroke on a segmented chain
LAYER_SEGMENT = 2   # the segments themselves
LAYER_JOINT = 3     # joint nodes
LAYER_CORE = 4      # the smaller core inside each joint
LAYER_FOOT = 5      # the toe


class LegBatch:
    """One spider's leg pass, held until it can be issued as a few paths."""

    __slots__ = ("_strokes", "_fills", "_points", "_width_step", "_direct")

    def __init__(self, width_step: float = 0.0, direct=None) -> None:
        self._strokes: dict = {}
        self._fills: dict = {}
        self._points: dict = {}
        self._width_step = float(width_step)
        # A painter here turns batching off: every call draws straight away,
        # in collection order, which is the leg-by-leg order the renderer
        # produced before this class existed. That is what makes "batching
        # changed nothing but the grouping" a testable claim rather than a
        # hope, and it lets one process time both ways against each other
        # instead of comparing two noisy process launches.
        self._direct = direct

    # ------------------------------------------------------------ collecting

    def _quantize(self, width: float) -> float:
        step = self._width_step
        if step <= 0.0:
            return float(width)
        # Snap to the nearest step, never below one step, so a hairline never
        # rounds away to nothing.
        return max(step, round(float(width) / step) * step)

    def _stroke_path(self, layer: int, color: QColor, width: float) -> QPainterPath:
        width = self._quantize(width)
        key = (layer, color.rgba(), width)
        entry = self._strokes.get(key)
        if entry is None:
            entry = self._strokes[key] = (QColor(color), width, QPainterPath())
        return entry[2]

    def _fill_path(self, layer: int, color: QColor) -> QPainterPath:
        key = (layer, color.rgba())
        entry = self._fills.get(key)
        if entry is None:
            entry = self._fills[key] = (QColor(color), QPainterPath())
        return entry[1]

    def line(self, layer: int, color: QColor, width: float,
             x1: float, y1: float, x2: float, y2: float) -> None:
        if self._direct is not None:
            self._direct.setPen(QPen(color, width, Qt.SolidLine,
                                     Qt.RoundCap, Qt.RoundJoin))
            self._direct.drawLine(QPointF(x1, y1), QPointF(x2, y2))
            return
        path = self._stroke_path(layer, color, width)
        path.moveTo(QPointF(x1, y1))
        path.lineTo(QPointF(x2, y2))

    def add_path(self, layer: int, color: QColor, width: float,
                 sub: QPainterPath) -> None:
        """Fold an already-built path (the fluff spline) into its group.

        Passthrough deliberately does *not* set a brush, because the renderer
        it stands in for did not either: it called ``drawPath`` on a leg
        spline with whatever brush happened to be current -- the body
        shadow's for the first leg, a joint node's for every later one -- so
        Qt filled the region between the leg and its chord as well as
        stroking it. That is 736 pixels of thin dark sliver on the models
        that use this branch, and it is plainly accidental.

        Batching cannot reproduce it: "whatever was current" is a property of
        draw order, and regrouping the draws is the entire point. So the
        batched flush strokes with ``NoBrush`` and the sliver goes. Keeping
        the old behaviour here is what lets a test assert that turning
        batching off restores the previous picture to the pixel.
        """
        if self._direct is not None:
            self._direct.setPen(QPen(color, width, Qt.SolidLine,
                                     Qt.RoundCap, Qt.RoundJoin))
            self._direct.drawPath(sub)
            return
        self._stroke_path(layer, color, width).addPath(sub)

    def point(self, layer: int, color: QColor, width: float,
              x: float, y: float) -> None:
        """A toe. `drawPoint` with a round cap is not the same shape as a
        filled circle of half the pen width: at a one-pixel width the pen
        paints one solid pixel and the circle antialiases to a faint blob,
        which moved 544 pixels across twenty spiders. `drawPoints` takes a
        whole group in one call and rasterises each one exactly as
        `drawPoint` did."""
        if self._direct is not None:
            self._direct.setPen(QPen(color, width, Qt.SolidLine, Qt.RoundCap))
            self._direct.drawPoint(QPointF(x, y))
            return
        width = self._quantize(width)
        key = (layer, color.rgba(), width)
        entry = self._points.get(key)
        if entry is None:
            entry = self._points[key] = (QColor(color), width, [])
        entry[2].append(QPointF(x, y))

    def dot(self, layer: int, color: QColor, x: float, y: float,
            radius: float) -> None:
        if self._direct is not None:
            self._direct.setPen(Qt.NoPen)
            self._direct.setBrush(QBrush(color))
            self._direct.drawEllipse(QPointF(x, y), radius, radius)
            return
        self._fill_path(layer, color).addEllipse(QPointF(x, y), radius, radius)

    # -------------------------------------------------------------- flushing

    def is_empty(self) -> bool:
        return not self._strokes and not self._fills and not self._points

    def flush(self, painter) -> int:
        """Issue the collected work. Returns the number of painter draws made,
        which is what the tests assert on -- a timing would be flaky, a call
        count is exact on every machine."""
        draws = 0
        if self._direct is not None:
            return draws
        # Interleave by layer so hair still sits under segments and joints
        # still sit over them.
        strokes: dict = {}
        for (layer, _rgba, _w), entry in self._strokes.items():
            strokes.setdefault(layer, []).append(entry)
        fills: dict = {}
        for (layer, _rgba), entry in self._fills.items():
            fills.setdefault(layer, []).append(entry)
        points: dict = {}
        for (layer, _rgba, _w), entry in self._points.items():
            points.setdefault(layer, []).append(entry)

        for layer in sorted(set(strokes) | set(fills) | set(points)):
            for color, width, path in strokes.get(layer, ()):
                painter.setPen(QPen(color, width, Qt.SolidLine,
                                    Qt.RoundCap, Qt.RoundJoin))
                painter.setBrush(Qt.NoBrush)
                painter.drawPath(path)
                draws += 1
            for color, path in fills.get(layer, ()):
                painter.setPen(Qt.NoPen)
                painter.setBrush(QBrush(color))
                painter.drawPath(path)
                draws += 1
            for color, width, pts in points.get(layer, ()):
                painter.setPen(QPen(color, width, Qt.SolidLine, Qt.RoundCap))
                painter.drawPoints(*pts)
                draws += 1
        self._strokes.clear()
        self._fills.clear()
        self._points.clear()
        return draws


# The step leg-stroke widths snap to before they can share a pen, in pixels.
# Widths grow with leg.lift, so without a step eight legs of five segments
# give forty groups of one and nothing batches. 0 disables the snapping and
# leaves every stroke exactly as shipped.
LEG_WIDTH_STEP = 0.0

# Off turns every leg draw back into the one-call-per-primitive form the
# renderer used before batching existed. Kept so the two can be compared
# inside a single process, which is the only way this machine gives a
# reproducible answer -- across process launches the noise is larger than
# the effect.
BATCH_LEGS = True
