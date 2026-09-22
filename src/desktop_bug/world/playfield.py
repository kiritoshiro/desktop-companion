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
    def contains(self, px: float, py: float, margin: float = 0.0) -> bool:
        if self._simple:
            return (margin <= px <= self.width - margin
                    and margin <= py <= self.height - margin)
        return any(r.contains(px, py, margin) for r in self._rects)

    def clamp(self, px: float, py: float, margin: float = 0.0) -> Tuple[float, float]:
        """Nearest habitable point, which is ``(px, py)`` when already on one.

        With several monitors the nearest point is found per rectangle and the
        closest wins, so a spider that strays into dead space is pushed back
        onto whichever screen it left rather than to a fixed one.
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
            cx, cy = rect.clamp(px, py, margin)
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
