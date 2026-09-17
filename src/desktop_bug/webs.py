"""Spider silk: webs the creatures weave, walk on, bounce-test, and finish.

A :class:`Web` is a shared world object.  One spider builds it strand by strand,
following the real construction order of a working spider, while *any* spider may
later walk onto a finished web to pluck and test its bounciness, and any weaver
may adopt and finish a web that another spider started and abandoned.

The animation/AI for an individual spider still lives in :mod:`desktop_bug.creature`.
This module owns only the silk: its geometry, its progressive build state, its
wobble physics, and how it draws.  The manager owns a single :class:`WebWorld`
that holds every web, hands free corners out to weavers, finds webs to finish or
walk on, advances wobble decay, and draws the whole set beneath the spiders.

Real orb-weavers build in four stereotyped phases -- proto-web/bridge, radii,
auxiliary spiral, capture spiral -- laying a temporary non-sticky guide spiral
from the hub outward and then replacing it with the sticky capture spiral spun
from the rim inward.  The patterns below reproduce that staged sequence so the
build reads, bit by bit, like an actual spider at work.  Funnel-weavers and
cobweb (tangle) spiders build very differently, so those two patterns follow
their own logic instead.

Qt is imported lazily inside :meth:`Web.draw` so the module can be imported in
headless contexts (tests, validators) without a display.
"""

from __future__ import annotations

import math
import random
from typing import List, Optional, Sequence, Tuple

from .math_utils import clamp, distance, lerp

Point = Tuple[float, float]

# Total webs allowed on screen at once.  Webs are cheap when static, but this
# keeps a swarm of weavers from carpeting the desktop and growing the repaint
# region without bound.
MAX_WEBS = 7

# Tearing/repair: how close the cursor must pass to a strand to snap it, the
# minimum cursor speed that counts as a deliberate swipe (so a resting pointer
# does not slowly dissolve a web), and how fast a webber re-knits torn segments.
TEAR_RADIUS = 9.0
TEAR_MIN_CURSOR_SPEED = 90.0
REPAIR_RATE = 9.0  # torn segments mended per second while a webber works

# How close (in px) the spider's body must trail the silk tip before the build
# is allowed to advance.  This is what keeps the drawn silk glued to the moving
# spider instead of racing ahead of it.
DEFAULT_LEAD_MAX = 26.0


# ======================================================================
# Strand: one continuous run of the spider, optionally laying silk
# ======================================================================
class Strand:
    """One traversal segment of the build.

    A strand is a polyline the spider walks along.  ``draw`` strands leave silk;
    ``move`` strands (``draw=False``) are repositioning runs -- for example the
    spider returning to the hub between two radii -- and leave nothing behind.

    Strands are guaranteed (by :func:`_connect`) to join end-to-end, so the silk
    tip never teleports: the start of every strand is the end of the one before.
    """

    __slots__ = ("points", "draw", "kind", "temporary", "_cum", "total")

    def __init__(self, points: Sequence[Point], draw: bool = True,
                 kind: str = "frame", temporary: bool = False) -> None:
        self.points: List[Point] = [(float(x), float(y)) for x, y in points]
        if not self.points:
            self.points = [(0.0, 0.0)]
        self.draw = bool(draw)
        self.kind = str(kind)
        self.temporary = bool(temporary)
        self._cum: List[float] = [0.0]
        total = 0.0
        for i in range(1, len(self.points)):
            total += distance(*self.points[i - 1], *self.points[i])
            self._cum.append(total)
        self.total = total

    def start(self) -> Point:
        return self.points[0]

    def end(self) -> Point:
        return self.points[-1]

    def point_at(self, t: float) -> Point:
        """Position a fraction ``t`` (0..1) along the strand by arc length."""
        if len(self.points) == 1 or self.total <= 1e-6:
            return self.points[-1]
        target = clamp(t, 0.0, 1.0) * self.total
        for i in range(1, len(self.points)):
            if self._cum[i] >= target:
                seg = self._cum[i] - self._cum[i - 1]
                local = 0.0 if seg <= 1e-6 else (target - self._cum[i - 1]) / seg
                ax, ay = self.points[i - 1]
                bx, by = self.points[i]
                return (lerp(ax, bx, local), lerp(ay, by, local))
        return self.points[-1]

    def polyline_upto(self, t: float) -> List[Point]:
        """All vertices up to fraction ``t`` plus the interpolated tip."""
        if t >= 1.0 or self.total <= 1e-6:
            return list(self.points)
        if t <= 0.0:
            return [self.points[0]]
        target = t * self.total
        out: List[Point] = [self.points[0]]
        for i in range(1, len(self.points)):
            if self._cum[i] <= target:
                out.append(self.points[i])
            else:
                seg = self._cum[i] - self._cum[i - 1]
                local = 0.0 if seg <= 1e-6 else (target - self._cum[i - 1]) / seg
                ax, ay = self.points[i - 1]
                bx, by = self.points[i]
                out.append((lerp(ax, bx, local), lerp(ay, by, local)))
                break
        return out

    def segment_count(self) -> int:
        return max(0, len(self.points) - 1)

    def segment_length(self, i: int) -> float:
        if i < 0 or i + 1 >= len(self.points):
            return 0.0
        return distance(*self.points[i], *self.points[i + 1])

    def segment_dist(self, i: int, x: float, y: float) -> float:
        """Distance from point (x, y) to segment ``i`` of this strand."""
        if i < 0 or i + 1 >= len(self.points):
            return float("inf")
        ax, ay = self.points[i]
        bx, by = self.points[i + 1]
        dx, dy = bx - ax, by - ay
        d2 = dx * dx + dy * dy
        if d2 <= 1e-9:
            return distance(ax, ay, x, y)
        t = clamp(((x - ax) * dx + (y - ay) * dy) / d2, 0.0, 1.0)
        return distance(ax + dx * t, ay + dy * t, x, y)


def _connect(strands: List[Strand]) -> List[Strand]:
    """Insert short ``move`` strands so the route is one unbroken path.

    Without this, finishing a radius at the rim and starting the next radius at
    the hub would teleport the silk tip across the web.  The inserted moves are
    the spider walking back to its next start.
    """
    if not strands:
        return strands
    out: List[Strand] = [strands[0]]
    for strand in strands[1:]:
        px, py = out[-1].end()
        sx, sy = strand.start()
        if distance(px, py, sx, sy) > 1.5:
            out.append(Strand([(px, py), (sx, sy)], draw=False, kind="move"))
        out.append(strand)
    return out


# ======================================================================
# Geometry helpers shared by the planners
# ======================================================================
def _unit(dx: float, dy: float) -> Point:
    m = math.hypot(dx, dy)
    if m <= 1e-9:
        return (1.0, 0.0)
    return (dx / m, dy / m)


def _add(p: Point, d: Point, s: float) -> Point:
    return (p[0] + d[0] * s, p[1] + d[1] * s)


def _sag_points(points: Sequence[Point], anchors: Sequence[Point], droop: float) -> List[Point]:
    """Pull each point gently toward screen-down, more the farther from anchors.

    This bakes a static gravity sag into the geometry so the silk hangs like a
    real web instead of reading as a flat technical drawing.  It is computed once
    at plan time and costs nothing per frame.
    """
    if droop <= 0.0 or not anchors:
        return [(float(x), float(y)) for x, y in points]
    out: List[Point] = []
    for x, y in points:
        nearest = min(distance(x, y, ax, ay) for ax, ay in anchors)
        out.append((float(x), float(y) + droop * (nearest ** 0.5)))
    return out


# ======================================================================
# Pattern planners -- each returns (strands, hub, wobble_dir)
# ======================================================================
def _plan_corner_orb(vertex: Point, din: Point, span: float,
                     reach_inset: float, screen_w: float, screen_h: float) -> Tuple[List[Strand], Point, Point]:
    """Sector orb tucked into a 90 degree screen corner -- the signature web.

    The hub sits a little way out from the corner along the diagonal.  Radii fan
    across the room-facing half; the two outermost radii lie along the walls, so
    the web hugs the corner.  Build order follows a real orb-weaver: bridge,
    frame, radii (with a return to the hub between each), hub reinforcement, the
    temporary auxiliary spiral spun outward, then the sticky capture spiral spun
    inward while the auxiliary spiral is removed.
    """
    vx, vy = vertex
    hub_off = clamp(span * 0.20, 38.0, 80.0)
    hub = (vx + din[0] * hub_off, vy + din[1] * hub_off)
    r = span
    base = math.atan2(din[1], din[0])
    sector = 1.55  # ~89 deg either side of the diagonal -> radii reach the walls
    nr = random.randint(9, 13)

    tips: List[Point] = []
    for i in range(nr):
        a = base - sector + (2.0 * sector) * (i / (nr - 1))
        tip = (hub[0] + math.cos(a) * r, hub[1] + math.sin(a) * r)
        tip = (clamp(tip[0], 2.0, screen_w - 2.0), clamp(tip[1], 2.0, screen_h - 2.0))
        tips.append(tip)

    strands: List[Strand] = []
    # 1) Bridge / main support: hub anchored back to the corner.
    strands.append(Strand([hub, vertex], kind="bridge"))
    # 2) Frame: out along one wall, around the outer tips, back along the other.
    frame_pts = [vertex, tips[0]] + tips + [tips[-1], vertex]
    strands.append(Strand(frame_pts, kind="frame"))
    # 3) Radii: hub -> tip, one at a time (moves back to hub are auto-inserted).
    for tip in tips:
        strands.append(Strand([hub, tip], kind="radius"))
    # 4) Hub reinforcement: a tight little loop around the centre.
    hub_r = r * 0.10
    hub_loop = []
    for i in range(nr + 1):
        a = base - sector + (2.0 * sector) * (i / nr)
        hub_loop.append((hub[0] + math.cos(a) * hub_r, hub[1] + math.sin(a) * hub_r))
    strands.append(Strand(hub_loop, kind="hub"))

    # 5) Auxiliary (temporary) spiral: hub -> rim, widely spaced. Non-sticky guide.
    aux_loops = 3
    aux_inner = hub_r * 1.4
    aux_pts = _spiral_points(hub, base - sector, base + sector, aux_inner, r * 0.96, aux_loops, outward=True)
    strands.append(Strand(aux_pts, kind="aux", temporary=True))
    # 6) Capture (sticky) spiral: rim -> hub, closely spaced, replaces the aux spiral.
    cap_loops = max(5, nr - 4)
    cap_pts = _spiral_points(hub, base + sector, base - sector, r * 0.92, aux_inner * 1.2, cap_loops, outward=False)
    strands.append(Strand(cap_pts, kind="capture"))

    strands = _connect(strands)
    strands = _apply_sag(strands, [vertex, tips[0], tips[-1]], span * 0.012)
    return strands, hub, din


def _plan_orb(center: Point, span: float, screen_w: float, screen_h: float) -> Tuple[List[Strand], Point, Point]:
    """A full circular orb spun in the open, hub lifted for gravity."""
    cx, cy = center
    hub = (cx, cy - span * 0.12)  # hub sits above centre, as on a vertical orb
    r = span
    nr = random.randint(11, 15)
    tips: List[Point] = []
    for i in range(nr):
        a = -math.pi + (2.0 * math.pi) * (i / nr)
        tip = (hub[0] + math.cos(a) * r, hub[1] + math.sin(a) * r)
        tip = (clamp(tip[0], 2.0, screen_w - 2.0), clamp(tip[1], 2.0, screen_h - 2.0))
        tips.append(tip)

    strands: List[Strand] = []
    # Bridge across the top, then the frame polygon through every tip.
    strands.append(Strand([tips[0], hub], kind="bridge"))
    strands.append(Strand(tips + [tips[0]], kind="frame"))
    for tip in tips:
        strands.append(Strand([hub, tip], kind="radius"))
    hub_r = r * 0.09
    hub_loop = [(hub[0] + math.cos(-math.pi + 2.0 * math.pi * i / nr) * hub_r,
                 hub[1] + math.sin(-math.pi + 2.0 * math.pi * i / nr) * hub_r) for i in range(nr + 1)]
    strands.append(Strand(hub_loop, kind="hub"))
    aux_pts = _spiral_points(hub, -math.pi, math.pi, hub_r * 1.5, r * 0.95, 4, outward=True, full=True)
    strands.append(Strand(aux_pts, kind="aux", temporary=True))
    cap_pts = _spiral_points(hub, math.pi, -math.pi, r * 0.92, hub_r * 1.8, max(7, nr - 3), outward=False, full=True)
    strands.append(Strand(cap_pts, kind="capture"))

    strands = _connect(strands)
    strands = _apply_sag(strands, tips, span * 0.01)
    return strands, hub, (0.0, 1.0)


def _plan_funnel(vertex: Point, din: Point, span: float,
                 screen_w: float, screen_h: float) -> Tuple[List[Strand], Point, Point]:
    """Funnel-weaver style: a dense sheet over the corner with a retreat tunnel.

    Funnel-weavers lay a flat sheet leading back to a tubular retreat.  Here the
    sheet fills the corner triangle in overlapping passes and the funnel throat
    converges at the corner where the spider would wait.
    """
    vx, vy = vertex
    base = math.atan2(din[1], din[0])
    r = span
    # Mouth of the funnel a little way out; the throat is the corner itself.
    mouth = (vx + din[0] * (r * 0.30), vy + din[1] * (r * 0.30))
    a0, a1 = base - 1.5, base + 1.5
    edge0 = (vx + math.cos(a0) * r, vy + math.sin(a0) * r)
    edge1 = (vx + math.cos(a1) * r, vy + math.sin(a1) * r)
    apex = (vx + din[0] * r, vy + din[1] * r)
    edge0 = (clamp(edge0[0], 2.0, screen_w - 2.0), clamp(edge0[1], 2.0, screen_h - 2.0))
    edge1 = (clamp(edge1[0], 2.0, screen_w - 2.0), clamp(edge1[1], 2.0, screen_h - 2.0))
    apex = (clamp(apex[0], 2.0, screen_w - 2.0), clamp(apex[1], 2.0, screen_h - 2.0))

    strands: List[Strand] = []
    # Boundary of the sheet, anchored to both walls and the outer apex.
    strands.append(Strand([vertex, edge0], kind="frame"))
    strands.append(Strand([edge0, apex, edge1], kind="frame"))
    strands.append(Strand([edge1, vertex], kind="frame"))
    # Sheet fill: many threads fanning from the mouth across the outer boundary,
    # laid as back-and-forth passes so it reads as a woven sheet.
    fan = 13
    for i in range(fan):
        f = i / (fan - 1)
        a = lerp(a0, a1, f)
        far = (vx + math.cos(a) * r * random.uniform(0.9, 1.0),
               vy + math.sin(a) * r * random.uniform(0.9, 1.0))
        far = (clamp(far[0], 2.0, screen_w - 2.0), clamp(far[1], 2.0, screen_h - 2.0))
        strands.append(Strand([mouth, far], kind="sheet"))
    # A few cross threads tie the fan together into a sheet.
    for ring in (0.55, 0.78):
        cross = []
        steps = 9
        for i in range(steps):
            a = lerp(a0, a1, i / (steps - 1))
            cross.append((vx + math.cos(a) * r * ring, vy + math.sin(a) * r * ring))
        strands.append(Strand(cross, kind="sheet"))
    # Funnel throat: short converging lines into the corner retreat.
    throat = 6
    for i in range(throat):
        a = lerp(base - 0.7, base + 0.7, i / (throat - 1))
        m = (vx + math.cos(a) * r * 0.30, vy + math.sin(a) * r * 0.30)
        strands.append(Strand([m, vertex], kind="funnel"))

    strands = _connect(strands)
    strands = _apply_sag(strands, [vertex, edge0, edge1, apex], span * 0.006)
    return strands, mouth, din


def _plan_tangle(vertex: Point, din: Point, span: float,
                 screen_w: float, screen_h: float) -> Tuple[List[Strand], Point, Point]:
    """Cobweb / tangle (Theridiidae): an irregular 3D mesh with gumfoot lines.

    Cobweb spiders build a loose three-dimensional tangle anchored by a few taut
    scaffold threads, with vertical sticky 'gumfoot' lines running down to the
    floor.  Flattened to the overlay it is a scruffy corner mesh with a couple of
    weighted drop-lines.
    """
    vx, vy = vertex
    base = math.atan2(din[1], din[0])
    r = span
    down = 1.0 if din[1] >= 0 else -1.0  # which way 'down' is for the gumfoot drop

    # Scaffold: a handful of long taut anchor lines bridging the corner gap.
    scaffold_ends: List[Point] = []
    strands: List[Strand] = []
    n_scaffold = random.randint(4, 5)
    for i in range(n_scaffold):
        a = lerp(base - 1.45, base + 1.45, i / (n_scaffold - 1))
        end = (vx + math.cos(a) * r * random.uniform(0.82, 1.0),
               vy + math.sin(a) * r * random.uniform(0.82, 1.0))
        end = (clamp(end[0], 2.0, screen_w - 2.0), clamp(end[1], 2.0, screen_h - 2.0))
        scaffold_ends.append(end)
        strands.append(Strand([vertex, end], kind="scaffold"))

    # Tangle infill: short irregular threads tying random scaffold points together.
    def on_scaffold() -> Point:
        end = random.choice(scaffold_ends)
        t = random.uniform(0.25, 0.95)
        return (lerp(vx, end[0], t), lerp(vy, end[1], t))

    for _ in range(random.randint(14, 20)):
        strands.append(Strand([on_scaffold(), on_scaffold()], kind="tangle"))

    # Gumfoot lines: a couple of near-vertical drop-lines to a little anchor blob.
    for _ in range(random.randint(2, 3)):
        top = on_scaffold()
        drop = r * random.uniform(0.35, 0.6)
        foot = (top[0] + random.uniform(-6.0, 6.0), top[1] + down * drop)
        foot = (clamp(foot[0], 2.0, screen_w - 2.0), clamp(foot[1], 2.0, screen_h - 2.0))
        strands.append(Strand([top, foot], kind="gumfoot"))

    strands = _connect(strands)
    # Tangles barely sag; they are taut and irregular already.
    strands = _apply_sag(strands, [vertex] + scaffold_ends, span * 0.004)
    hub = (vx + din[0] * r * 0.4, vy + din[1] * r * 0.4)
    return strands, hub, din


def _spiral_points(hub: Point, a_start: float, a_end: float, r_start: float, r_end: float,
                   loops: int, outward: bool, full: bool = False) -> List[Point]:
    """Sample an Archimedean spiral around ``hub``.

    For sector webs the angle sweeps once across the fan per loop and folds back,
    giving the characteristic to-and-fro spiral; for a full orb (``full``) the
    angle simply winds continuously.
    """
    pts: List[Point] = []
    steps = max(12, loops * 16)
    for i in range(steps + 1):
        f = i / steps
        rr = lerp(r_start, r_end, f)
        if full:
            a = a_start + (a_end - a_start) * f + (2.0 * math.pi * loops) * f * (1.0 if outward else -1.0)
        else:
            # Fold the sweep back and forth across the sector as radius changes.
            phase = (f * loops) % 1.0
            tri = 1.0 - abs(2.0 * phase - 1.0)
            a = lerp(a_start, a_end, tri)
        pts.append((hub[0] + math.cos(a) * rr, hub[1] + math.sin(a) * rr))
    return pts


def _apply_sag(strands: List[Strand], anchors: Sequence[Point], droop: float) -> List[Strand]:
    out: List[Strand] = []
    for s in strands:
        sagged = _sag_points(s.points, anchors, droop)
        out.append(Strand(sagged, draw=s.draw, kind=s.kind, temporary=s.temporary))
    return out


_PATTERN_PLANNERS = {
    "corner_orb": _plan_corner_orb,
    "orb": _plan_orb,
    "funnel": _plan_funnel,
    "tangle": _plan_tangle,
}


# ======================================================================
# Web: one shared piece of silk with progressive build + wobble
# ======================================================================
class Web:
    """A single web: its silk geometry, build progress, and bounce physics."""

    def __init__(self, pattern: str, strands: List[Strand], hub: Point,
                 wobble_dir: Point, spec_id: str, reach_inset: float,
                 screen_w: float = 1e9, screen_h: float = 1e9) -> None:
        self.pattern = pattern
        self.strands = strands
        self.hub = hub
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.wobble_dir = _unit(*wobble_dir)
        self.spec_id = spec_id
        self.reach_inset = reach_inset

        # Build progress.  ``built`` is the index of the strand currently being
        # laid; everything before it is complete.  ``active_t`` is 0..1 along it.
        self.built = 0
        self.active_t = 0.0
        self.state = "building"  # building -> complete (or stays building if abandoned)
        self.builder = None       # the Creature currently weaving, or None if abandoned
        self.just_completed = 0   # frames of forced repaint right after completion

        # Index of the first capture-spiral strand, used to fade the temporary
        # auxiliary spiral out as the capture spiral replaces it.
        self.capture_start = None
        self.capture_count = 0
        for i, s in enumerate(self.strands):
            if s.kind == "capture":
                if self.capture_start is None:
                    self.capture_start = i
                self.capture_count += 1

        # Wobble (pluck/bounce) state.
        self.bounce_amp = 0.0
        self.bounce_phase = 0.0
        self.bounce_omega = 17.0
        self.bounce_origin = hub

        # Tearing/repair.  ``cut`` holds (strand_index, segment_index) pairs that
        # the cursor has snapped; a webber can mend them.  ``repairer`` is the
        # Creature currently re-knitting this web, if any.  ``_dmg_frames`` forces
        # a few repaint frames whenever the torn set changes.
        self.cut: set = set()
        self.repairer = None
        self._dmg_frames = 0
        self._tearable_total = 0.0
        self._tearable = []  # list of (strand_index, segment_index, length)
        for si, s in enumerate(self.strands):
            if not s.draw or s.kind == "move":
                continue
            for segi in range(s.segment_count()):
                length = s.segment_length(segi)
                if length <= 1e-6:
                    continue
                self._tearable.append((si, segi, length))
                self._tearable_total += length

        # Cached footprint over the true geometry (+pad for line width and wobble).
        xs = [p[0] for s in self.strands for p in s.points]
        ys = [p[1] for s in self.strands for p in s.points]
        pad = 8.0
        self._bbox = (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)

    # -- progress -------------------------------------------------------
    def is_complete(self) -> bool:
        return self.state == "complete"

    def progress(self) -> float:
        n = len(self.strands)
        if n == 0:
            return 1.0
        return clamp((self.built + self.active_t) / n, 0.0, 1.0)

    def temp_alpha(self) -> float:
        """Opacity of the temporary auxiliary spiral (fades as capture is laid)."""
        if self.capture_start is None:
            return 1.0
        if self.built < self.capture_start:
            return 1.0
        done = (self.built - self.capture_start) + self.active_t
        return clamp(1.0 - done / max(1.0, float(self.capture_count)), 0.0, 1.0)

    def _reachable(self, p: Point, screen_w: float, screen_h: float) -> Point:
        """Clamp a silk point to where a spider body can actually stand.

        A spider cannot press its body flush against a screen edge, so the
        reachable target is inset by ``reach_inset``.  The silk itself still
        *draws* to the true endpoint; only walking/keeping-up uses this.
        """
        ins = self.reach_inset
        return (clamp(p[0], ins, screen_w - ins), clamp(p[1], ins, screen_h - ins))

    def working_point(self, clamp_reachable: bool = True,
                      screen_w: Optional[float] = None,
                      screen_h: Optional[float] = None) -> Tuple[Point, bool]:
        """Where the silk tip is right now, and whether the spider is drawing.

        When ``clamp_reachable`` the point is pulled to where a spider body can
        actually stand (it cannot reach the last sliver against a screen edge);
        the silk itself still draws to the true endpoint.
        """
        sw = self.screen_w if screen_w is None else screen_w
        sh = self.screen_h if screen_h is None else screen_h
        if self.built >= len(self.strands):
            return self.hub, False
        strand = self.strands[self.built]
        tip = strand.point_at(self.active_t)
        if clamp_reachable:
            tip = self._reachable(tip, sw, sh)
        return tip, strand.draw

    def advance(self, dt: float, weave_speed: float, spider_xy: Point,
                lead_max: float = DEFAULT_LEAD_MAX) -> None:
        """Lay a little more silk, but only if the spider has kept up.

        Gating advancement on the spider trailing the tip is what keeps the
        drawn strand pinned to the spider's body rather than running ahead.
        """
        if self.built >= len(self.strands):
            if self.state != "complete":
                self.state = "complete"
                self.just_completed = 3
            return
        strand = self.strands[self.built]
        tip = strand.point_at(self.active_t)
        rtip = self._reachable(tip, self.screen_w, self.screen_h)
        if distance(rtip[0], rtip[1], spider_xy[0], spider_xy[1]) > lead_max:
            return  # wait for the spider to catch up to the (reachable) tip
        # Repositioning runs (lifting silk to a new anchor) are much quicker
        # than careful silk-laying, so the spider scurries between anchors.
        eff = weave_speed * (3.0 if not strand.draw else 1.0)
        length = max(1.0, strand.total)
        self.active_t += dt * eff / length
        while self.active_t >= 1.0 and self.built < len(self.strands):
            self.built += 1
            self.active_t -= 1.0
            if self.built >= len(self.strands):
                self.active_t = 0.0
                self.state = "complete"
                self.just_completed = 3
                break
            strand = self.strands[self.built]
            length = max(1.0, strand.total)

    # -- wobble / bounce ------------------------------------------------
    def pluck(self, at_xy: Point, strength: float = 1.0) -> None:
        self.bounce_origin = (float(at_xy[0]), float(at_xy[1]))
        self.bounce_amp = max(self.bounce_amp, clamp(strength, 0.2, 1.6) * 6.5)
        self.bounce_phase = 0.0

    def update(self, dt: float) -> None:
        if self.bounce_amp > 0.01:
            self.bounce_phase += dt * self.bounce_omega
            self.bounce_amp *= math.exp(-3.0 * dt)
            if self.bounce_amp <= 0.01:
                self.bounce_amp = 0.0
        if self.just_completed > 0:
            self.just_completed -= 1
        if self._dmg_frames > 0:
            self._dmg_frames -= 1

    def is_animating(self) -> bool:
        return (self.state == "building" or self.bounce_amp > 0.01
                or self.just_completed > 0 or self._dmg_frames > 0)

    # -- tearing / repair ----------------------------------------------
    def is_damaged(self) -> bool:
        return bool(self.cut)

    def intact_fraction(self) -> float:
        """Fraction of the web's silk length still present (1.0 = pristine)."""
        if self._tearable_total <= 1e-6:
            return 1.0
        torn = 0.0
        for (si, segi) in self.cut:
            if 0 <= si < len(self.strands):
                torn += self.strands[si].segment_length(segi)
        return clamp(1.0 - torn / self._tearable_total, 0.0, 1.0)

    def damage_near(self, x: float, y: float, radius: float = TEAR_RADIUS) -> int:
        """Snap the strand segments the cursor is passing through.

        Only a finished web tears, and only the segments within ``radius`` of the
        point are cut, so dragging the pointer across a web breaks the parts it
        actually crosses rather than the whole web at once.  Returns the number
        of newly cut segments.
        """
        if not self.is_complete():
            return 0
        # Cheap reject: ignore points well outside the web footprint.
        x0, y0, x1, y1 = self._bbox
        if x < x0 - radius or x > x1 + radius or y < y0 - radius or y > y1 + radius:
            return 0
        newly = 0
        for (si, segi, _length) in self._tearable:
            key = (si, segi)
            if key in self.cut:
                continue
            if self.strands[si].segment_dist(segi, x, y) <= radius:
                self.cut.add(key)
                newly += 1
        if newly:
            self._dmg_frames = max(self._dmg_frames, 3)
            # A torn web quivers from the disturbance.
            self.pluck((x, y), strength=0.7)
        return newly

    def damaged_centroid(self) -> Optional[Point]:
        if not self.cut:
            return None
        sx = sy = 0.0
        n = 0
        for (si, segi) in self.cut:
            if 0 <= si < len(self.strands):
                s = self.strands[si]
                if segi + 1 < len(s.points):
                    ax, ay = s.points[segi]
                    bx, by = s.points[segi + 1]
                    sx += (ax + bx) * 0.5
                    sy += (ay + by) * 0.5
                    n += 1
        if n == 0:
            return None
        return (sx / n, sy / n)

    def repair_near(self, x: float, y: float, dt: float,
                    rate: float = REPAIR_RATE, radius: float = 1e9) -> int:
        """Re-knit torn segments, nearest to (x, y) first.  Returns mended count."""
        if not self.cut:
            return 0
        self._repair_accum = getattr(self, "_repair_accum", 0.0) + rate * dt
        n = int(self._repair_accum)
        if n <= 0:
            return 0
        self._repair_accum -= n
        # Order torn segments by distance to the worker so the mend reads local.
        candidates = []
        for (si, segi) in self.cut:
            if 0 <= si < len(self.strands):
                d = self.strands[si].segment_dist(segi, x, y)
                if d <= radius:
                    candidates.append((d, (si, segi)))
        candidates.sort(key=lambda e: e[0])
        mended = 0
        for _d, key in candidates[:n]:
            self.cut.discard(key)
            mended += 1
        if mended:
            self._dmg_frames = max(self._dmg_frames, 3)
        return mended

    # -- geometry -------------------------------------------------------
    def bbox(self) -> Tuple[float, float, float, float]:
        return self._bbox

    def footprint_xywh(self) -> Tuple[float, float, float, float]:
        x0, y0, x1, y1 = self._bbox
        return (x0, y0, x1 - x0, y1 - y0)

    def contains_region(self, sw: float, sh: float) -> bool:
        x0, y0, x1, y1 = self._bbox
        return x1 >= 0 and y1 >= 0 and x0 <= sw and y0 <= sh

    def walkable_point(self, screen_w: float, screen_h: float) -> Point:
        """A reachable spot on the web for another spider to stand and pluck."""
        candidates = [s for s in self.strands if s.draw and s.kind in
                      ("capture", "radius", "frame", "sheet", "scaffold", "aux")]
        ins = self.reach_inset
        if candidates:
            s = random.choice(candidates)
            p = s.point_at(random.uniform(0.3, 0.8))
        else:
            p = self.hub
        return (clamp(p[0], ins, screen_w - ins), clamp(p[1], ins, screen_h - ins))

    def _wobble_offset(self, p: Point) -> Point:
        if self.bounce_amp <= 0.01:
            return p
        d = distance(p[0], p[1], self.bounce_origin[0], self.bounce_origin[1])
        fall = math.exp(-d / 90.0)
        amt = self.bounce_amp * fall * math.cos(self.bounce_phase - d * 0.035)
        return (p[0] + self.wobble_dir[0] * amt, p[1] + self.wobble_dir[1] * amt)

    @staticmethod
    def _stroke_run(painter, run: List[Point]) -> None:
        from PyQt5.QtCore import QPointF
        from PyQt5.QtGui import QPainterPath

        path = QPainterPath()
        path.moveTo(QPointF(run[0][0], run[0][1]))
        for p in run[1:]:
            path.lineTo(QPointF(p[0], p[1]))
        painter.drawPath(path)

    # -- drawing --------------------------------------------------------
    def draw(self, painter, clip: Optional[Tuple[float, float, float, float]] = None) -> None:
        if clip is not None:
            x0, y0, x1, y1 = self._bbox
            if x1 < clip[0] or x0 > clip[2] or y1 < clip[1] or y0 > clip[3]:
                return
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen, QPainterPath

        wobbling = self.bounce_amp > 0.01
        temp_a = self.temp_alpha()

        # Pale silk palette.  Structural threads are faint silver; the sticky
        # capture spiral is a touch brighter; the temporary spiral is fainter and
        # fades away as the capture spiral replaces it.
        def pen_for(kind: str, temporary: bool) -> Optional[QPen]:
            if temporary:
                a = int(120 * temp_a)
                if a <= 3:
                    return None
                pen = QPen(QColor(210, 224, 238, a), 0.9)
                pen.setDashPattern([2.0, 3.0])
            elif kind == "capture":
                pen = QPen(QColor(238, 245, 255, 205), 1.15)
            elif kind in ("bridge", "scaffold"):
                pen = QPen(QColor(225, 234, 246, 180), 1.35)
            elif kind == "gumfoot":
                pen = QPen(QColor(232, 240, 250, 195), 1.1)
            elif kind in ("sheet", "funnel"):
                pen = QPen(QColor(222, 232, 244, 120), 0.85)
            else:  # frame, radius, hub, tangle
                pen = QPen(QColor(224, 233, 245, 165), 1.0)
            pen.setCapStyle(Qt.RoundCap)
            pen.setJoinStyle(Qt.RoundJoin)
            return pen

        for i, s in enumerate(self.strands):
            if not s.draw:
                continue
            if i < self.built:
                portion = 1.0
            elif i == self.built and self.state != "complete":
                portion = self.active_t
            elif self.state == "complete":
                portion = 1.0
            else:
                continue
            if portion <= 0.0:
                continue
            pen = pen_for(s.kind, s.temporary)
            if pen is None:
                continue
            pts = s.polyline_upto(portion)
            if len(pts) < 2:
                continue
            if wobbling:
                pts = [self._wobble_offset(p) for p in pts]
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            # When the web is finished and torn, draw only the runs of segments
            # the cursor has not snapped, so the gaps where it was broken show.
            has_cuts = self.cut and self.state == "complete" and any(
                (i, segi) in self.cut for segi in range(len(pts) - 1))
            if has_cuts:
                run: List[Point] = [pts[0]]
                for segi in range(len(pts) - 1):
                    if (i, segi) in self.cut:
                        if len(run) >= 2:
                            self._stroke_run(painter, run)
                        run = [pts[segi + 1]]
                    else:
                        run.append(pts[segi + 1])
                if len(run) >= 2:
                    self._stroke_run(painter, run)
            else:
                path = QPainterPath()
                path.moveTo(QPointF(pts[0][0], pts[0][1]))
                for p in pts[1:]:
                    path.lineTo(QPointF(p[0], p[1]))
                painter.drawPath(path)

            # Dewdrop beads along the sticky capture spiral (skip on torn webs to
            # avoid beads floating over snapped gaps).
            if s.kind == "capture" and len(pts) > 4 and not has_cuts:
                painter.setPen(Qt.NoPen)
                painter.setBrush(QColor(245, 250, 255, 150))
                step = max(2, len(pts) // 10)
                for j in range(0, len(pts), step):
                    bx, by = pts[j]
                    painter.drawEllipse(QPointF(bx, by), 0.9, 0.9)

        # A small bright hub knot once the web exists enough to have one.
        if self.built > 1:
            hub = self._wobble_offset(self.hub) if wobbling else self.hub
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(240, 247, 255, 150))
            painter.drawEllipse(QPointF(hub[0], hub[1]), 1.6, 1.6)


# ======================================================================
# WebWorld: the shared store the manager owns
# ======================================================================
class WebWorld:
    """Holds every web; hands corners to weavers; finds webs to finish/walk on."""

    def __init__(self, screen_w: float, screen_h: float) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.webs: List[Web] = []
        self._dirty: List[Tuple[float, float, float, float]] = []
        self._removed: List[Tuple[float, float, float, float]] = []
        self._prev_cursor: Optional[Tuple[float, float]] = None

    # -- screen / lifecycle --------------------------------------------
    def set_screen(self, screen_w: float, screen_h: float) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        kept: List[Web] = []
        for web in self.webs:
            web.screen_w = self.screen_w
            web.screen_h = self.screen_h
            if web.contains_region(self.screen_w, self.screen_h):
                kept.append(web)
            else:
                self._removed.append(web.footprint_xywh())
        self.webs = kept

    def clear(self) -> None:
        for web in self.webs:
            self._removed.append(web.footprint_xywh())
        self.webs = []

    # -- anchor specs ---------------------------------------------------
    def _specs(self) -> List[dict]:
        """Candidate anchor sites: the four corners (primary) and two edges."""
        w, h = self.screen_w, self.screen_h
        inv = 1.0 / math.sqrt(2.0)
        specs = [
            {"id": "corner_nw", "kind": "corner", "vertex": (0.0, 0.0), "din": (inv, inv)},
            {"id": "corner_ne", "kind": "corner", "vertex": (w, 0.0), "din": (-inv, inv)},
            {"id": "corner_sw", "kind": "corner", "vertex": (0.0, h), "din": (inv, -inv)},
            {"id": "corner_se", "kind": "corner", "vertex": (w, h), "din": (-inv, -inv)},
            {"id": "edge_top", "kind": "edge", "vertex": (w * 0.5, 0.0), "din": (0.0, 1.0)},
            {"id": "edge_bottom", "kind": "edge", "vertex": (w * 0.5, h), "din": (0.0, -1.0)},
        ]
        return specs

    def _occupied(self, spec_id: str) -> bool:
        return any(web.spec_id == spec_id for web in self.webs)

    def _span_for(self, spec: dict, base_size: float) -> float:
        small = min(self.screen_w, self.screen_h)
        span = clamp(base_size * 6.0, 90.0, small * 0.34)
        return span

    def _pick_pattern(self, spec: dict, weights: Optional[dict]) -> str:
        if spec["kind"] == "edge":
            # Open span midway along an edge: a full orb fits naturally here.
            return "orb" if random.random() < 0.7 else "corner_orb"
        w = {"corner_orb": 0.62, "funnel": 0.2, "tangle": 0.18}
        if weights:
            for k, v in weights.items():
                if k in w:
                    try:
                        w[k] = max(0.0, float(v))
                    except Exception:
                        pass
        keys = list(w.keys())
        total = sum(w[k] for k in keys) or 1.0
        roll = random.random() * total
        acc = 0.0
        for k in keys:
            acc += w[k]
            if roll <= acc:
                return k
        return "corner_orb"

    def _random_open_spec(self) -> dict:
        """A web site out in the open, away from the walls."""
        w, h = self.screen_w, self.screen_h
        mx = w * 0.16
        my = h * 0.16
        vx = random.uniform(mx, w - mx)
        vy = random.uniform(my, h - my)
        # Orient roughly toward screen centre so the static sag reads naturally.
        din = _unit((w * 0.5) - vx, (h * 0.5) - vy)
        return {"id": f"open_{vx:.0f}_{vy:.0f}", "kind": "open", "vertex": (vx, vy), "din": din}

    def _too_close_to_existing(self, point: Point, min_sep: float) -> bool:
        for web in self.webs:
            if distance(point[0], point[1], web.hub[0], web.hub[1]) < min_sep:
                return True
        return False

    # -- weaver API -----------------------------------------------------
    def claim_site(self, creature, prefer_corner: bool = True,
                   pattern_weights: Optional[dict] = None) -> Optional[Web]:
        """Start a fresh web for ``creature`` at a free corner, edge, or open spot."""
        if len(self.webs) >= MAX_WEBS:
            return None
        specs = self._specs()
        # Offer a few open-space sites too, so webs can appear in random places
        # and not only hug the screen corners.
        specs.extend(self._random_open_spec() for _ in range(4))
        random.shuffle(specs)
        if prefer_corner:
            specs.sort(key=lambda s: 0 if s["kind"] == "corner" else 1)
        base_size = float(getattr(creature, "size", 24.0))
        reach_inset = base_size * 0.7 + 22.0
        for spec in specs:
            if spec["kind"] in ("corner", "edge") and self._occupied(spec["id"]):
                continue
            span = self._span_for(spec, base_size)
            if spec["kind"] == "open":
                # Keep open webs from stacking on top of each other.
                if self._too_close_to_existing(spec["vertex"], span * 1.25):
                    continue
                strands, hub, wob = _plan_orb(spec["vertex"], span * 0.8,
                                              self.screen_w, self.screen_h)
                pattern = "orb"
            else:
                pattern = self._pick_pattern(spec, pattern_weights)
                planner = _PATTERN_PLANNERS.get(pattern, _plan_corner_orb)
                if spec["kind"] == "edge" and pattern == "orb":
                    center = (spec["vertex"][0], spec["vertex"][1] + spec["din"][1] * span)
                    strands, hub, wob = planner(center, span, self.screen_w, self.screen_h)
                elif pattern == "orb":
                    # A corner site asked for an orb: centre it out along the diagonal.
                    center = (spec["vertex"][0] + spec["din"][0] * span,
                              spec["vertex"][1] + spec["din"][1] * span)
                    strands, hub, wob = _plan_orb(center, span * 0.8, self.screen_w, self.screen_h)
                elif pattern == "corner_orb":
                    strands, hub, wob = planner(spec["vertex"], spec["din"], span,
                                                reach_inset, self.screen_w, self.screen_h)
                else:
                    strands, hub, wob = planner(spec["vertex"], spec["din"], span,
                                                self.screen_w, self.screen_h)
            web = Web(pattern, strands, hub, wob, spec["id"], reach_inset,
                      self.screen_w, self.screen_h)
            web.builder = creature
            self.webs.append(web)
            return web
        return None

    def find_adoptable_web(self, creature, max_dist: float = 1e9) -> Optional[Web]:
        """Nearest unfinished web nobody is currently building -- to finish it."""
        cx, cy = getattr(creature, "x", 0.0), getattr(creature, "y", 0.0)
        best = None
        best_d = max_dist
        for web in self.webs:
            if web.state != "building" or web.builder is not None:
                continue
            d = distance(cx, cy, web.hub[0], web.hub[1])
            if d <= best_d:
                best_d = d
                best = web
        return best

    def find_walkable_web(self, creature, max_dist: float = 1e9) -> Optional[Web]:
        """Nearest finished web for a spider to walk onto and bounce-test."""
        cx, cy = getattr(creature, "x", 0.0), getattr(creature, "y", 0.0)
        best = None
        best_d = max_dist
        for web in self.webs:
            if not web.is_complete():
                continue
            d = distance(cx, cy, web.hub[0], web.hub[1])
            if d <= best_d:
                best_d = d
                best = web
        return best

    def adopt(self, web: Web, creature) -> bool:
        """Take over an abandoned unfinished web."""
        if web not in self.webs or web.state != "building" or web.builder is not None:
            return False
        web.builder = creature
        return True

    def find_repairable_web(self, creature, max_dist: float = 1e9) -> Optional[Web]:
        """Nearest finished, torn web nobody is currently mending -- to repair."""
        cx, cy = getattr(creature, "x", 0.0), getattr(creature, "y", 0.0)
        best = None
        best_d = max_dist
        for web in self.webs:
            if not web.is_complete() or not web.is_damaged() or web.repairer is not None:
                continue
            d = distance(cx, cy, web.hub[0], web.hub[1])
            if d <= best_d:
                best_d = d
                best = web
        return best

    def claim_repair(self, web: Web, creature) -> bool:
        if web not in self.webs or not web.is_complete() or web.repairer is not None:
            return False
        web.repairer = creature
        return True

    def release_repair(self, web: Optional[Web]) -> None:
        if web is not None and web in self.webs and web.repairer is not None:
            web.repairer = None

    def intact_complete_count(self) -> int:
        """Finished webs that are not torn -- these sate a webber's urge to build."""
        return sum(1 for w in self.webs if w.is_complete() and not w.is_damaged())

    def abandon(self, web: Optional[Web]) -> None:
        """Leave an in-progress web for anyone to finish; complete webs persist."""
        if web is None or web not in self.webs:
            return
        if web.builder is not None:
            web.builder = None
        # If barely started, drop it so a stray stub does not litter the corner.
        if web.state == "building" and web.built <= 1 and web.active_t < 0.2:
            self._removed.append(web.footprint_xywh())
            self.webs.remove(web)

    def remove(self, web: Web) -> None:
        if web in self.webs:
            self._removed.append(web.footprint_xywh())
            self.webs.remove(web)

    # -- per-frame ------------------------------------------------------
    def update(self, dt: float, cursor: Optional[Tuple[float, float]] = None) -> None:
        # A finished web tears where a moving pointer crosses it. Requiring the
        # pointer to actually be moving (and not held still) means you break a
        # net by dragging the mouse around it, not by parking the cursor on it.
        if cursor is not None and dt > 1e-6:
            cx, cy = float(cursor[0]), float(cursor[1])
            if self._prev_cursor is not None:
                speed = distance(cx, cy, self._prev_cursor[0], self._prev_cursor[1]) / dt
                if speed >= TEAR_MIN_CURSOR_SPEED and self.webs:
                    # Sample a few points along the cursor's path this frame so a
                    # fast swipe still snaps every strand it passed between frames.
                    px, py = self._prev_cursor
                    steps = max(1, min(6, int(distance(cx, cy, px, py) / TEAR_RADIUS)))
                    for web in self.webs:
                        if not web.is_complete():
                            continue
                        for s in range(steps + 1):
                            f = s / steps
                            web.damage_near(lerp(px, cx, f), lerp(py, cy, f))
            self._prev_cursor = (cx, cy)

        dirty: List[Tuple[float, float, float, float]] = []
        for web in self.webs:
            web.update(dt)
            if web.is_animating():
                dirty.append(web.footprint_xywh())
        if self._removed:
            dirty.extend(self._removed)
            self._removed = []
        self._dirty = dirty

    def dirty_rects(self) -> List[Tuple[float, float, float, float]]:
        return self._dirty

    def render(self, painter, clip: Optional[Tuple[float, float, float, float]] = None) -> None:
        for web in self.webs:
            web.draw(painter, clip)

    # -- introspection (handy for tests / status) ----------------------
    def count(self) -> int:
        return len(self.webs)

    def count_building(self) -> int:
        return sum(1 for w in self.webs if w.state == "building")
