"""Where a spider is allowed to be: the screens, not their bounding box (DC-65).

The owner, running the overlay across two monitors: *"if i got two screens of
diferent size, on the smaller one (extended) they might dissaper out of
border. make limits based on the size of screens and the way they are
extended."*

The overlay window spans ``virtual_screen_geometry()`` -- the union *bounding
rectangle* of every monitor -- and every creature is clamped to ``0..screen_w``
by ``0..screen_h``. On one monitor, or on two identical ones side by side,
those are the same shape and nothing is wrong. They stop being the same shape
the moment the monitors differ:

    2560x1440 at (0,0) and 1920x1080 at (2560,0)
    bounding box: 4480x1440
    real screens:  2560x1440 + 1920x1080
    dead space:   x 2560..4480, y 1080..1440  -- 691,200 px of nothing

A spider that wanders into dead space is inside the overlay window, is being
updated, is being painted, and is on no monitor at all. It has not fallen out
of the world; it is drawn where no screen can show it. From the desk it simply
vanishes, which is exactly what was reported.

So the bound has to be the union of the actual screen rectangles. That union
is not a rectangle and cannot be expressed as one, which is the whole reason
this is a module rather than two numbers.

**The fast path matters.** One monitor is the overwhelmingly common case and
must cost nothing: ``simple`` is true whenever the rectangles exactly tile
their own bounding box, and every method then falls through to plain
arithmetic. A colony of five spiders checked per frame is not a place to do
polygon work for no reason.

Coordinates here are always overlay-local: the same space creatures, cages,
webs and the cursor already share. Translating monitor geometry into it is the
caller's job (see ``engine.py``), and is the one place a screen origin exists.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable, List, Sequence, Tuple


@dataclass(frozen=True)
class ScreenRect:
    """One monitor, in overlay-local pixels."""

    x: float
    y: float
    w: float
    h: float

    @property
    def right(self) -> float:
        return self.x + self.w

    @property
    def bottom(self) -> float:
        return self.y + self.h

    def contains(self, px: float, py: float, margin: float = 0.0) -> bool:
        # A rectangle thinner than two margins has no interior to speak of;
        # treating it as empty would make a narrow monitor uninhabitable, so
        # the margin is capped at what actually fits.
        mx = min(margin, self.w * 0.5)
        my = min(margin, self.h * 0.5)
        return (self.x + mx <= px <= self.right - mx
                and self.y + my <= py <= self.bottom - my)

    def clamp(self, px: float, py: float, margin: float = 0.0) -> Tuple[float, float]:
        mx = min(margin, self.w * 0.5)
        my = min(margin, self.h * 0.5)
        return (
            min(max(px, self.x + mx), self.right - mx),
            min(max(py, self.y + my), self.bottom - my),
        )


class Playfield:
    """The habitable area of the overlay: a union of screen rectangles.

    Constructed with the overlay's own size and no rectangles, which means
    "the whole window is habitable" -- the single-monitor behaviour the
    project had before this module existed, and the behaviour every headless
    test gets without asking for it.
    """

    def __init__(self, width: float, height: float,
                 rects: Sequence[ScreenRect] | None = None) -> None:
        self.width = float(width)
        self.height = float(height)
        self._rects: List[ScreenRect] = []
        self.set_rects(rects)

    # ------------------------------------------------------------------
    # Definition
    # ------------------------------------------------------------------
    def set_rects(self, rects: Iterable[ScreenRect] | None) -> None:
        cleaned = [r for r in (rects or ()) if r.w > 1.0 and r.h > 1.0]
        self._rects = cleaned
        self._simple = self._is_whole_window(cleaned)

    def set_size(self, width: float, height: float) -> None:
        self.width = float(width)
        self.height = float(height)
        self._simple = self._is_whole_window(self._rects)

    @property
    def rects(self) -> Tuple[ScreenRect, ...]:
        return tuple(self._rects)

    @property
    def simple(self) -> bool:
        """Whether the screens tile the whole overlay, so no work is needed.

        True with no rectangles at all (nothing is known, so nothing is
        excluded), and true for one monitor, which is the case that must not
        pay for this module.
        """
        return self._simple

    def _is_whole_window(self, rects: Sequence[ScreenRect]) -> bool:
        if not rects:
            return True
        # Compared by area rather than by shape: any arrangement whose
        # rectangles are disjoint and sum to the bounding box leaves no dead
        # space, whether that is one monitor or four tiling a wall.
        covered = sum(r.w * r.h for r in rects)
        return abs(covered - self.width * self.height) < 1.0

    # ------------------------------------------------------------------
    # Queries
    # ------------------------------------------------------------------
    def _covered(self, px: float, py: float) -> bool:
        return any(r.contains(px, py) for r in self._rects)

    def contains(self, px: float, py: float, margin: float = 0.0) -> bool:
        """Whether a body of half-size ``margin`` at the point is on the screens.

        Judged against the monitors *together*: the square around the point
        must lie on screen, not all on one monitor. Each monitor used to apply
        the margin to every edge, including the edge it shares with its
        neighbour, so a band ``2 * margin`` wide along the join counted as
        off-screen from both sides; a spider walking across it was pushed
        back every frame and stuck there (the owner: "spiders get stuck ...
        at the edge").
        """
        if self._simple:
            return (margin <= px <= self.width - margin
                    and margin <= py <= self.height - margin)
        if margin <= 0.0:
            return self._covered(px, py)
        m = float(margin)
        return all(self._covered(px + dx, py + dy)
                   for dx in (-m, 0.0, m) for dy in (-m, 0.0, m))

    def _open_sides(self, rect: ScreenRect) -> Tuple[bool, bool, bool, bool]:
        """(left, right, top, bottom): which edges another monitor continues."""
        def overlaps(a0, a1, b0, b1):
            return min(a1, b1) - max(a0, b0) > 1.0

        left = right = top = bottom = False
        for other in self._rects:
            if other is rect:
                continue
            if overlaps(rect.y, rect.bottom, other.y, other.bottom):
                left = left or abs(other.right - rect.x) <= 1.5
                right = right or abs(other.x - rect.right) <= 1.5
            if overlaps(rect.x, rect.right, other.x, other.right):
                top = top or abs(other.bottom - rect.y) <= 1.5
                bottom = bottom or abs(other.y - rect.bottom) <= 1.5
        return left, right, top, bottom

    def clamp(self, px: float, py: float, margin: float = 0.0) -> Tuple[float, float]:
        """Nearest habitable point, which is ``(px, py)`` when already on one.

        With several monitors the nearest point is found per rectangle and the
        closest wins, so a spider that strays into dead space is pushed back
        onto whichever screen it left rather than to a fixed one. An edge a
        monitor shares with its neighbour gets no margin, so the answer can
        sit at the join instead of a margin into one screen.
        """
        if self._simple:
            return (
                min(max(px, margin), max(margin, self.width - margin)),
                min(max(py, margin), max(margin, self.height - margin)),
            )
        if not self._rects:
            return px, py
        if self.contains(px, py, margin):
            return px, py
        best = None
        best_d = float("inf")
        for rect in self._rects:
            candidates = [rect.clamp(px, py, margin)]
            if margin > 0.0:
                left, right, top, bottom = self._open_sides(rect)
                mx = min(margin, rect.w * 0.5)
                my = min(margin, rect.h * 0.5)
                x0 = rect.x + (0.0 if left else mx)
                x1 = rect.right - (0.0 if right else mx)
                y0 = rect.y + (0.0 if top else my)
                y1 = rect.bottom - (0.0 if bottom else my)
                seam = (min(max(px, x0), x1), min(max(py, y0), y1))
                if self.contains(seam[0], seam[1], margin):
                    candidates.append(seam)
            for cx, cy in candidates:
                d = (cx - px) ** 2 + (cy - py) ** 2
                if d < best_d:
                    best_d = d
                    best = (cx, cy)
        return best if best is not None else (px, py)


def rects_from_geometry(screens: Iterable[Tuple[float, float, float, float]],
                        origin_x: float, origin_y: float) -> List[ScreenRect]:
    """Translate absolute monitor rectangles into overlay-local ones.

    ``screens`` is ``(x, y, w, h)`` per monitor in the desktop's own
    coordinates; ``origin`` is the overlay window's top-left in that same
    space. Kept separate from Qt so it can be tested without a screen.
    """
    return [
        ScreenRect(float(x) - float(origin_x), float(y) - float(origin_y),
                   float(w), float(h))
        for (x, y, w, h) in screens
    ]
