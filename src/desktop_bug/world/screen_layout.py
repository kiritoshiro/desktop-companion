"""The monitors as a map a mission can travel: which screens exist, how they join.

The owner: *"make multi screen missions too. to recognise automatically where
are the screens and allow to move to them ... this function must be
implemented smartly so that it could be utilised in various later
scenarios."*

``Playfield`` (DC-65) answers *where may a spider stand*. This module answers
the questions a mission asks on top of that:

- which screen is a point on, and which is the main one;
- how do two screens connect -- along a shared edge (a **door**: walk
  across), or, when monitors do not touch at all, through a **tunnel** the
  layout adds so every screen can still be reached;
- which way should a spider walk to reach a point on another screen
  (``route``): the next waypoint, so it never grinds into the dead corner of
  an L-shaped layout;
- where on a screen to put things (``anchor``), by fractions of that screen.

Pure data, no Qt: coordinates are overlay-local like everything else, and a
test builds a layout from plain rectangles.
"""
from __future__ import annotations

import math
from collections import deque
from dataclasses import dataclass
from typing import Iterable, Sequence

from .playfield import Playfield, ScreenRect

# A shared edge shorter than this is a corner touch, not a door.
MIN_DOOR = 80.0
# How near a tunnel mouth a spider must be to go through.
TUNNEL_REACH = 34.0
# How near a door's near side a spider must be to be sent through it.
DOOR_REACH = 40.0
# Distance from the screen edge at which a tunnel mouth sits.
TUNNEL_INSET = 70.0


@dataclass(frozen=True)
class Screen:
    index: int
    rect: ScreenRect
    primary: bool = False

    @property
    def name(self) -> str:
        return "Main screen" if self.primary else f"Screen {self.index + 1}"

    @property
    def centre(self) -> tuple[float, float]:
        return (self.rect.x + self.rect.w / 2.0, self.rect.y + self.rect.h / 2.0)

    def anchor(self, fx: float, fy: float, margin: float = 0.0) -> tuple[float, float]:
        """The point at fractions ``fx``, ``fy`` of this screen, kept ``margin`` inside."""
        r = self.rect
        return r.clamp(r.x + r.w * fx, r.y + r.h * fy, margin)


@dataclass(frozen=True)
class ScreenLink:
    """A way between two screens. ``a_point`` is on screen ``a``, ``b_point`` on ``b``.

    A door's two points are the middle of the shared edge, just either side
    of it; a tunnel's are its two mouths, well inside each screen.
    """
    a: int
    b: int
    kind: str                       # "door" or "tunnel"
    a_point: tuple[float, float]
    b_point: tuple[float, float]

    def point_on(self, screen: int) -> tuple[float, float]:
        return self.a_point if screen == self.a else self.b_point

    def other(self, screen: int) -> int:
        return self.b if screen == self.a else self.a


class ScreenLayout:
    """Every monitor, how they connect, and a way between any two of them."""

    def __init__(self, rects: Sequence[ScreenRect], primary: int = 0) -> None:
        cleaned = [r for r in rects if r.w > 1.0 and r.h > 1.0]
        if not cleaned:
            raise ValueError("a screen layout needs at least one screen")
        primary = primary if 0 <= primary < len(cleaned) else 0
        self.screens = tuple(Screen(i, r, i == primary) for i, r in enumerate(cleaned))
        self.primary = self.screens[primary]
        right = max(r.right for r in cleaned)
        bottom = max(r.bottom for r in cleaned)
        self.playfield = Playfield(right, bottom, cleaned)
        self.links = tuple(self._find_links())

    @classmethod
    def single(cls, rect: ScreenRect) -> "ScreenLayout":
        return cls([rect], 0)

    # -- queries ------------------------------------------------------------
    @property
    def count(self) -> int:
        return len(self.screens)

    @property
    def multi(self) -> bool:
        return len(self.screens) > 1

    @property
    def others(self) -> tuple[Screen, ...]:
        """Every screen but the main one."""
        return tuple(s for s in self.screens if not s.primary)

    def screen_at(self, x: float, y: float) -> Screen:
        """The screen a point is on; off every screen, the nearest one."""
        for s in self.screens:
            if s.rect.contains(x, y):
                return s
        return min(self.screens, key=lambda s: _distance_to_rect(s.rect, x, y))

    def clamp(self, x: float, y: float, margin: float = 0.0) -> tuple[float, float]:
        return self.playfield.clamp(x, y, margin)

    def links_of(self, screen: int) -> list[ScreenLink]:
        return [link for link in self.links if screen in (link.a, link.b)]

    @property
    def tunnels(self) -> tuple[ScreenLink, ...]:
        return tuple(link for link in self.links if link.kind == "tunnel")

    def path(self, start: int, goal: int) -> list[ScreenLink]:
        """The links to cross, in order, from screen ``start`` to ``goal``."""
        if start == goal:
            return []
        came = {start: None}
        queue = deque([start])
        while queue:
            here = queue.popleft()
            if here == goal:
                break
            for link in self.links_of(here):
                there = link.other(here)
                if there not in came:
                    came[there] = (here, link)
                    queue.append(there)
        if goal not in came:
            return []
        steps, node = [], goal
        while came[node] is not None:
            here, link = came[node]
            steps.append(link)
            node = here
        return list(reversed(steps))

    def route(self, x: float, y: float, tx: float, ty: float) -> tuple[float, float]:
        """Where to walk next to reach ``(tx, ty)`` from ``(x, y)``.

        On the same screen that is the target itself; otherwise the near side
        of the first door or tunnel on the way.
        """
        here, there = self.screen_at(x, y).index, self.screen_at(tx, ty).index
        steps = self.path(here, there)
        if not steps:
            return tx, ty
        link = steps[0]
        near = link.point_on(here)
        if link.kind == "door" and math.hypot(x - near[0], y - near[1]) < DOOR_REACH:
            # At the door: step through it.
            return link.point_on(link.other(here))
        return near

    def tunnel_at(self, x: float, y: float, reach: float = TUNNEL_REACH):
        """(tunnel, far mouth) when ``(x, y)`` stands in a tunnel mouth, else None."""
        for link in self.tunnels:
            for near, far in ((link.a_point, link.b_point), (link.b_point, link.a_point)):
                if math.hypot(x - near[0], y - near[1]) <= reach:
                    return link, far
        return None

    # -- building -----------------------------------------------------------
    def _find_links(self) -> Iterable[ScreenLink]:
        doors = []
        for a in self.screens:
            for b in self.screens:
                if b.index <= a.index:
                    continue
                door = _door(a, b)
                if door is not None:
                    doors.append(door)
        yield from doors
        # Screens that share no edge with the rest (a gap, a corner touch)
        # are joined by tunnels, nearest pair first, until all connect.
        groups = _components(len(self.screens), doors)
        while len(groups) > 1:
            best = None
            for ia, ga in enumerate(groups):
                for gb in groups[ia + 1:]:
                    for i in ga:
                        for j in gb:
                            d = _rect_gap(self.screens[i].rect, self.screens[j].rect)
                            if best is None or d < best[0]:
                                best = (d, i, j)
            _, i, j = best
            yield _tunnel(self.screens[i], self.screens[j])
            merged = next(g for g in groups if i in g) | next(g for g in groups if j in g)
            groups = [g for g in groups if i not in g and j not in g] + [merged]


def _door(a: Screen, b: Screen) -> ScreenLink | None:
    ra, rb = a.rect, b.rect
    step = 24.0
    # Side by side.
    lo, hi = max(ra.y, rb.y), min(ra.bottom, rb.bottom)
    if hi - lo >= MIN_DOOR:
        mid = (lo + hi) / 2.0
        if abs(ra.right - rb.x) <= 1.5:
            return ScreenLink(a.index, b.index, "door", (ra.right - step, mid), (rb.x + step, mid))
        if abs(rb.right - ra.x) <= 1.5:
            return ScreenLink(a.index, b.index, "door", (ra.x + step, mid), (rb.right - step, mid))
    # Stacked.
    lo, hi = max(ra.x, rb.x), min(ra.right, rb.right)
    if hi - lo >= MIN_DOOR:
        mid = (lo + hi) / 2.0
        if abs(ra.bottom - rb.y) <= 1.5:
            return ScreenLink(a.index, b.index, "door", (mid, ra.bottom - step), (mid, rb.y + step))
        if abs(rb.bottom - ra.y) <= 1.5:
            return ScreenLink(a.index, b.index, "door", (mid, ra.y + step), (mid, rb.bottom - step))
    return None


def _tunnel(a: Screen, b: Screen) -> ScreenLink:
    """Mouths on the sides of each screen that face the other one."""
    (ax, ay), (bx, by) = a.centre, b.centre
    angle = math.atan2(by - ay, bx - ax)

    def mouth(screen, towards):
        r = screen.rect
        cx, cy = screen.centre
        # Walk from the centre towards the other screen until the inset edge.
        reach_x = (r.w / 2.0 - TUNNEL_INSET) / max(1e-6, abs(math.cos(towards)))
        reach_y = (r.h / 2.0 - TUNNEL_INSET) / max(1e-6, abs(math.sin(towards)))
        reach = max(0.0, min(reach_x, reach_y))
        return r.clamp(cx + math.cos(towards) * reach, cy + math.sin(towards) * reach, TUNNEL_INSET)

    return ScreenLink(a.index, b.index, "tunnel", mouth(a, angle), mouth(b, angle + math.pi))


def _components(count: int, links: Iterable[ScreenLink]) -> list[set]:
    groups = [{i} for i in range(count)]
    for link in links:
        ga = next(g for g in groups if link.a in g)
        gb = next(g for g in groups if link.b in g)
        if ga is not gb:
            groups = [g for g in groups if g is not ga and g is not gb] + [ga | gb]
    return groups


def _distance_to_rect(r: ScreenRect, x: float, y: float) -> float:
    dx = max(r.x - x, 0.0, x - r.right)
    dy = max(r.y - y, 0.0, y - r.bottom)
    return math.hypot(dx, dy)


def _rect_gap(a: ScreenRect, b: ScreenRect) -> float:
    dx = max(b.x - a.right, 0.0, a.x - b.right)
    dy = max(b.y - a.bottom, 0.0, a.y - b.bottom)
    return math.hypot(dx, dy)
