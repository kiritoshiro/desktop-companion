"""Sticky silk the spiders shoot *at the real cursor*.

This is a separate kind of silk from :mod:`desktop_bug.webs`.  The webs module
builds decorative orb/funnel/cobweb structures in screen corners; this module is
the offensive, cursor-grabbing silk:

* A spider can *shoot a web glob* at the pointer.  The glob flies from the
  spider toward the cursor, lightly homing so it lands on the pointer.
* On a hit it becomes one of two captures:
    - ``trap``  -- the pointer is pinned roughly where it was hit.  The user
      breaks free by *moving the mouse around a bit*: wiggling fills a struggle
      meter, holding still lets it drain, and once it is full the silk snaps.
    - ``wall``  -- the pointer is shoved to the nearest wall along the spider's
      firing line, then pinned against that wall until the user wiggles loose.

The only thing that can actually move the OS pointer is the overlay/engine
layer (it owns the Win32 calls and the global screen origin).  This module is
therefore pure geometry: :meth:`MouseWebWorld.update` returns the *desired*
overlay-local cursor position each frame (or ``None`` when the pointer should be
free) and the engine applies it.  Everything here is failure-safe:

* If pointer control is unavailable (non-Windows, or ``SetCursorPos`` silently
  fails) the captures simply cannot hold the pointer.  A failed hold registers
  as the user "escaping" almost immediately, so the worst case is a trap that
  barely grabs rather than a pointer that gets stuck.
* Every capture has a hard maximum hold time, so the pointer can never be held
  indefinitely even if struggle input never arrives.
* A small dead zone plus a steady struggle-decay means tiny ``SetCursorPos``
  rounding jitter can never slowly fill the meter on its own.

Qt is imported lazily inside the ``draw`` methods so the module stays importable
in headless contexts (tests, validators) with no display.
"""

from __future__ import annotations

import math
import random
from typing import List, Optional, Tuple

from ..support.math_utils import clamp, distance, lerp

Point = Tuple[float, float]


# ----------------------------------------------------------------------
# Tuning.  Distances are in screen pixels; times in seconds.
# ----------------------------------------------------------------------
# Projectile (the flying glob of silk).
SHOT_SPEED = 1350.0           # px/s the glob travels
SHOT_HOMING = 7.0             # how strongly the glob curves toward the pointer
SHOT_HIT_RADIUS = 26.0        # within this of the pointer -> it sticks
SHOT_MAX_TRAVEL_MULT = 2.4    # give up after this * initial distance (a miss)
SHOT_MIN_TRAVEL = 60.0

# Sticky hold: how hard the pointer is held and how the user escapes.
TRAP_LEASH = 17.0             # px the pointer may stray from the anchor
TRAP_RETRACT = 0.55           # fraction of the stray the pointer is allowed to keep
TRAP_DEADZONE = 3.0           # per-frame motion under this counts as no struggle
TRAP_STRUGGLE_CAP = 27.0      # most struggle one frame can contribute
TRAP_STRUGGLE_DECAY = 170.0   # struggle bled off per second while holding still
TRAP_ESCAPE = 460.0           # accumulated struggle needed to snap the silk
TRAP_MAX_HOLD = 6.0           # absolute safety cap on an in-place trap

# Wall shove + pin.
SHOVE_DURATION = 0.45         # seconds to slam the pointer to the wall
WALL_MARGIN = 8.0             # how close to the true edge the pointer is parked
WALL_ESCAPE = 320.0           # a wall pin peels off a little easier than a trap
WALL_MAX_HOLD = 4.0           # absolute safety cap on a wall pin

# When pointer control is impossible, a capture just plays a brief visual.
NO_CONTROL_VISUAL = 0.55

# Spent silk left behind on release, fading out.
SPENT_FADE = 0.45

# Silk palette, kept in step with the pale silver of webs.py.
_SILK = (224, 233, 245)
_SILK_BRIGHT = (240, 247, 255)
_SILK_STICKY = (238, 245, 255)


def _unit(dx: float, dy: float) -> Point:
    m = math.hypot(dx, dy)
    if m <= 1e-9:
        return (1.0, 0.0)
    return (dx / m, dy / m)


def _clamp_vec(dx: float, dy: float, max_len: float) -> Point:
    m = math.hypot(dx, dy)
    if m <= max_len or m <= 1e-9:
        return (dx, dy)
    s = max_len / m
    return (dx * s, dy * s)


def _ease_out(t: float) -> float:
    t = clamp(t, 0.0, 1.0)
    return 1.0 - (1.0 - t) * (1.0 - t)


# ======================================================================
# A sticky hold at a fixed anchor (used by both trap and wall-pin).
# ======================================================================
class _StickyHold:
    """Pins the pointer near ``anchor`` until the user wiggles free.

    The hold tracks the position it last *forced* the pointer to.  Any deviation
    from that on the following frame is genuine user input, so the struggle
    meter measures the user's hand and not the hold's own correction.
    """

    def __init__(self, anchor: Point, leash: float, escape: float,
                 max_hold: float, pegs: int = 9) -> None:
        self.anchor = (float(anchor[0]), float(anchor[1]))
        self.leash = float(leash)
        self.escape = float(escape)
        self.max_hold = float(max_hold)
        self.struggle = 0.0
        self.hold_t = 0.0
        self.forced_prev: Optional[Point] = None
        # Visual: where the pointer is straining to (a little past the leash so
        # the silk looks stretched), plus a wobble that grows with struggle.
        self.strain = self.anchor
        self.wobble = 0.0
        self.wob_phase = random.uniform(0.0, math.tau)
        # Fixed anchor pegs so the splat looks like real radiating silk.
        self.pegs: List[Tuple[Point, float]] = []
        for i in range(pegs):
            a = (i / pegs) * math.tau + random.uniform(-0.18, 0.18)
            r = leash * random.uniform(2.4, 4.2)
            self.pegs.append(((math.cos(a) * r, math.sin(a) * r),
                              random.uniform(0.8, 1.0)))

    def progress(self) -> float:
        return clamp(self.struggle / max(1.0, self.escape), 0.0, 1.0)

    def update(self, dt: float, mx: float, my: float,
               can_control: bool) -> Tuple[Optional[Point], bool]:
        self.hold_t += dt
        self.wob_phase += dt * (14.0 + 22.0 * self.progress())

        if not can_control:
            # Cannot actually hold the pointer; just play a brief stick then let
            # go.  Never move the cursor.
            self.strain = (mx, my)
            self.wobble = 2.0 * math.sin(self.wob_phase)
            done = self.hold_t >= NO_CONTROL_VISUAL
            return None, done

        if self.forced_prev is None:
            self.forced_prev = (mx, my)

        # Genuine user motion since we last placed the pointer.
        user_move = distance(mx, my, self.forced_prev[0], self.forced_prev[1])
        self.struggle += clamp(user_move - TRAP_DEADZONE, 0.0, TRAP_STRUGGLE_CAP)
        self.struggle = max(0.0, self.struggle - TRAP_STRUGGLE_DECAY * dt)

        # Strain visual: how far the user is yanking from the anchor.
        sdx, sdy = _clamp_vec(mx - self.anchor[0], my - self.anchor[1],
                              self.leash * 1.8)
        self.strain = (self.anchor[0] + sdx, self.anchor[1] + sdy)
        self.wobble = (1.5 + 4.0 * self.progress()) * math.sin(self.wob_phase) \
            + min(6.0, user_move * 0.25)

        if self.struggle >= self.escape or self.hold_t >= self.max_hold:
            return None, True  # snapped free / timed out -- leave pointer be

        # Hold the pointer: allow a little stray within the leash, then pull most
        # of it back toward the anchor so it reads as genuinely stuck.
        hdx, hdy = _clamp_vec(mx - self.anchor[0], my - self.anchor[1], self.leash)
        desired = (self.anchor[0] + hdx * TRAP_RETRACT,
                   self.anchor[1] + hdy * TRAP_RETRACT)
        self.forced_prev = desired
        return desired, False

    # -- geometry / drawing -------------------------------------------
    def bbox(self) -> Tuple[float, float, float, float]:
        ax, ay = self.anchor
        reach = self.leash * 4.6 + 14.0
        return (ax - reach, ay - reach, ax + reach, ay + reach)

    def draw(self, painter, alpha_mult: float = 1.0) -> None:
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen

        prog = self.progress()
        ax, ay = self.anchor
        # The sticky center sits where the pointer is actually being held, which
        # is a retracted fraction of the strain, so the silk visibly stretches.
        cx = ax + (self.strain[0] - ax) * TRAP_RETRACT
        cy = ay + (self.strain[1] - ay) * TRAP_RETRACT
        wob_n = _unit(self.strain[0] - ax, self.strain[1] - ay)
        woff = (-wob_n[1] * self.wobble, wob_n[0] * self.wobble)
        cx += woff[0]
        cy += woff[1]

        # Radiating anchor strands from the held point out to the fixed pegs.
        # As the struggle meter fills, strands stretch and a couple "snap" (their
        # alpha drops) to sell the web tearing.
        for i, (off, base) in enumerate(self.pegs):
            px = ax + off[0]
            py = ay + off[1]
            snapped = prog > 0.62 and (i % 3 == 0)
            a = int(170 * base * alpha_mult * (0.25 if snapped else 1.0) * (1.0 - prog * 0.35))
            if a <= 4:
                continue
            pen = QPen(QColor(_SILK[0], _SILK[1], _SILK[2], a), 1.0)
            pen.setCapStyle(Qt.RoundCap)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            # Bow each strand slightly toward the strained center for a taut look.
            mxp = (px + cx) * 0.5 + (cx - ax) * 0.12
            myp = (py + cy) * 0.5 + (cy - ay) * 0.12
            painter.drawPolyline(QPointF(px, py), QPointF(mxp, myp), QPointF(cx, cy))

        # A couple of concentric capture-spiral arcs around the center.
        ring_a = int(150 * alpha_mult * (1.0 - prog * 0.5))
        if ring_a > 6:
            pen = QPen(QColor(_SILK_STICKY[0], _SILK_STICKY[1], _SILK_STICKY[2], ring_a), 1.0)
            pen.setCapStyle(Qt.RoundCap)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            for ring in (0.45, 0.8):
                r = self.leash * (1.2 + ring * 1.6)
                pts = []
                steps = 14
                for s in range(steps + 1):
                    aa = (s / steps) * math.tau
                    pts.append(QPointF(cx + math.cos(aa) * r,
                                       cy + math.sin(aa) * r * 0.92))
                painter.drawPolyline(*pts)

        # Bright sticky knot on the trapped pointer.
        knot_a = int(210 * alpha_mult)
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(_SILK_BRIGHT[0], _SILK_BRIGHT[1], _SILK_BRIGHT[2], knot_a))
        painter.drawEllipse(QPointF(cx, cy), 3.1, 3.1)
        painter.setBrush(QColor(_SILK_BRIGHT[0], _SILK_BRIGHT[1], _SILK_BRIGHT[2], int(90 * alpha_mult)))
        painter.drawEllipse(QPointF(cx, cy), 5.6, 5.6)


# ======================================================================
# Captures
# ======================================================================
class _Capture:
    kind = "capture"

    def update(self, dt: float, mx: float, my: float,
               can_control: bool) -> Tuple[Optional[Point], bool]:
        raise NotImplementedError

    def bbox(self) -> Tuple[float, float, float, float]:
        raise NotImplementedError

    def release_point(self) -> Point:
        raise NotImplementedError

    def draw(self, painter) -> None:
        raise NotImplementedError


class TrapCapture(_Capture):
    """Pin the pointer where the glob hit it; wiggle to escape."""

    kind = "trap"

    def __init__(self, anchor: Point) -> None:
        self.hold = _StickyHold(anchor, TRAP_LEASH, TRAP_ESCAPE, TRAP_MAX_HOLD)

    def update(self, dt, mx, my, can_control):
        return self.hold.update(dt, mx, my, can_control)

    def bbox(self):
        return self.hold.bbox()

    def release_point(self):
        return self.hold.strain

    def draw(self, painter):
        self.hold.draw(painter)


class ShoveCapture(_Capture):
    """Slam the pointer to the nearest wall along the firing line, then pin it."""

    kind = "wall"

    def __init__(self, hit: Point, fire_dir: Point,
                 screen_w: float, screen_h: float) -> None:
        self.start = (float(hit[0]), float(hit[1]))
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.wall = self._wall_point(self.start, fire_dir, screen_w, screen_h)
        self.phase = "shove"
        self.t = 0.0
        self.cur = self.start
        # Trailing comet positions for the shove streak.
        self.trail: List[Point] = [self.start]
        self.hold: Optional[_StickyHold] = None

    @staticmethod
    def _wall_point(start: Point, fire_dir: Point,
                    sw: float, sh: float) -> Point:
        dx, dy = _unit(*fire_dir)
        sx, sy = start
        # Distance to each wall along the firing ray (only positive hits count).
        best = None
        if dx > 1e-6:
            t = (sw - WALL_MARGIN - sx) / dx
            best = _closer(best, t)
        elif dx < -1e-6:
            t = (WALL_MARGIN - sx) / dx
            best = _closer(best, t)
        if dy > 1e-6:
            t = (sh - WALL_MARGIN - sy) / dy
            best = _closer(best, t)
        elif dy < -1e-6:
            t = (WALL_MARGIN - sy) / dy
            best = _closer(best, t)
        if best is None or best <= 1.0:
            # Firing line is degenerate; fall back to the nearest wall.
            return _nearest_wall(start, sw, sh)
        wx = clamp(sx + dx * best, WALL_MARGIN, sw - WALL_MARGIN)
        wy = clamp(sy + dy * best, WALL_MARGIN, sh - WALL_MARGIN)
        # Snap to whichever edge we actually reached so it parks flush.
        return _snap_to_edge((wx, wy), sw, sh)

    def update(self, dt, mx, my, can_control):
        if self.phase == "shove":
            self.t += dt
            f = _ease_out(self.t / SHOVE_DURATION)
            self.cur = (lerp(self.start[0], self.wall[0], f),
                        lerp(self.start[1], self.wall[1], f))
            self.trail.append(self.cur)
            if len(self.trail) > 10:
                self.trail = self.trail[-10:]
            if not can_control:
                # Cannot move the pointer; show the streak briefly then stop.
                if self.t >= min(SHOVE_DURATION, NO_CONTROL_VISUAL):
                    return None, True
                return None, False
            if self.t >= SHOVE_DURATION:
                self.phase = "pin"
                self.hold = _StickyHold(self.wall, TRAP_LEASH * 0.9,
                                        WALL_ESCAPE, WALL_MAX_HOLD)
            return self.wall if self.t >= SHOVE_DURATION else self.cur, False

        # Pinned against the wall.
        assert self.hold is not None
        return self.hold.update(dt, mx, my, can_control)

    def bbox(self):
        if self.hold is not None:
            return self.hold.bbox()
        xs = [p[0] for p in self.trail] + [self.wall[0]]
        ys = [p[1] for p in self.trail] + [self.wall[1]]
        pad = 18.0
        return (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)

    def release_point(self):
        if self.hold is not None:
            return self.hold.strain
        return self.cur

    def draw(self, painter):
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen

        if self.phase == "shove" and len(self.trail) >= 2:
            n = len(self.trail)
            for i in range(1, n):
                a = int(150 * (i / n))
                pen = QPen(QColor(_SILK[0], _SILK[1], _SILK[2], a), 1.0 + 1.6 * (i / n))
                pen.setCapStyle(Qt.RoundCap)
                painter.setPen(pen)
                painter.drawLine(QPointF(*self.trail[i - 1]), QPointF(*self.trail[i]))
            # Sticky head.
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(_SILK_BRIGHT[0], _SILK_BRIGHT[1], _SILK_BRIGHT[2], 210))
            painter.drawEllipse(QPointF(*self.cur), 3.0, 3.0)
        if self.hold is not None:
            self.hold.draw(painter)


def _closer(best, t):
    if t <= 1.0:
        return best
    if best is None:
        return t
    return min(best, t)


def _nearest_wall(p: Point, sw: float, sh: float) -> Point:
    x, y = p
    d = [(x, "l"), (sw - x, "r"), (y, "t"), (sh - y, "b")]
    d.sort(key=lambda e: e[0])
    side = d[0][1]
    if side == "l":
        return (WALL_MARGIN, clamp(y, WALL_MARGIN, sh - WALL_MARGIN))
    if side == "r":
        return (sw - WALL_MARGIN, clamp(y, WALL_MARGIN, sh - WALL_MARGIN))
    if side == "t":
        return (clamp(x, WALL_MARGIN, sw - WALL_MARGIN), WALL_MARGIN)
    return (clamp(x, WALL_MARGIN, sw - WALL_MARGIN), sh - WALL_MARGIN)


def _snap_to_edge(p: Point, sw: float, sh: float) -> Point:
    x, y = p
    d = [(x - WALL_MARGIN, "l"), (sw - WALL_MARGIN - x, "r"),
         (y - WALL_MARGIN, "t"), (sh - WALL_MARGIN - y, "b")]
    d.sort(key=lambda e: e[0])
    side = d[0][1]
    if side == "l":
        return (WALL_MARGIN, y)
    if side == "r":
        return (sw - WALL_MARGIN, y)
    if side == "t":
        return (x, WALL_MARGIN)
    return (x, sh - WALL_MARGIN)


# ======================================================================
# Projectile: the flying glob of silk.
# ======================================================================
class _Projectile:
    """A glob of silk, thrown rather than guided.

    It used to steer onto the pointer for its whole flight, so it could not be
    dodged and read as a homing missile instead of a thrown web. It now aims
    once, leading a moving target by the time the glob will take to arrive, and
    then flies straight. Correcting in flight is what the Silk tracking ability
    restores, so a spider earns it rather than starting with it.
    """

    def __init__(self, origin: Point, target: Point, kind: str,
                 homing: float = 0.0, lead: Point = (0.0, 0.0)) -> None:
        self.kind = kind
        self.pos = (float(origin[0]), float(origin[1]))
        self.origin = self.pos
        self.homing = max(0.0, float(homing))
        # Aim where the target is going to be, not where it was. Without this a
        # straight shot at anything moving would always trail behind it.
        # Solved iteratively: aiming ahead lengthens the flight, which moves the
        # interception point again. One pass under-leads enough to miss.
        travel = distance(origin[0], origin[1], target[0], target[1]) / SHOT_SPEED
        aim = target
        for _ in range(4):
            aim = (target[0] + lead[0] * travel, target[1] + lead[1] * travel)
            travel = distance(origin[0], origin[1], aim[0], aim[1]) / SHOT_SPEED
        d = distance(origin[0], origin[1], aim[0], aim[1])
        self.max_travel = max(SHOT_MIN_TRAVEL, d * SHOT_MAX_TRAVEL_MULT)
        self.travelled = 0.0
        ux, uy = _unit(aim[0] - origin[0], aim[1] - origin[1])
        self.vel = (ux * SHOT_SPEED, uy * SHOT_SPEED)
        self.trail: List[Point] = [self.pos]

    def update(self, dt: float, mx: float, my: float) -> str:
        """Advance the glob.  Returns 'fly' | 'hit' | 'miss'."""
        if self.homing > 0.0:
            # Silk tracking: steer onto the live pointer while in flight.
            desired = _unit(mx - self.pos[0], my - self.pos[1])
            vx = self.vel[0] + desired[0] * SHOT_SPEED * SHOT_HOMING * self.homing * dt
            vy = self.vel[1] + desired[1] * SHOT_SPEED * SHOT_HOMING * self.homing * dt
            sp = math.hypot(vx, vy)
            if sp > 1e-6:
                vx, vy = vx / sp * SHOT_SPEED, vy / sp * SHOT_SPEED
        else:
            vx, vy = self.vel
        self.vel = (vx, vy)
        step = (vx * dt, vy * dt)
        self.pos = (self.pos[0] + step[0], self.pos[1] + step[1])
        self.travelled += math.hypot(*step)
        self.trail.append(self.pos)
        if len(self.trail) > 8:
            self.trail = self.trail[-8:]
        if distance(self.pos[0], self.pos[1], mx, my) <= SHOT_HIT_RADIUS:
            return "hit"
        if self.travelled >= self.max_travel:
            return "miss"
        return "fly"

    def bbox(self) -> Tuple[float, float, float, float]:
        xs = [p[0] for p in self.trail]
        ys = [p[1] for p in self.trail]
        pad = 10.0
        return (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)

    def draw(self, painter) -> None:
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen

        n = len(self.trail)
        for i in range(1, n):
            a = int(120 * (i / n))
            pen = QPen(QColor(_SILK[0], _SILK[1], _SILK[2], a), 0.8 + 1.4 * (i / n))
            pen.setCapStyle(Qt.RoundCap)
            painter.setPen(pen)
            painter.drawLine(QPointF(*self.trail[i - 1]), QPointF(*self.trail[i]))
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(_SILK_BRIGHT[0], _SILK_BRIGHT[1], _SILK_BRIGHT[2], 220))
        painter.drawEllipse(QPointF(*self.pos), 2.8, 2.8)
        painter.setBrush(QColor(_SILK_BRIGHT[0], _SILK_BRIGHT[1], _SILK_BRIGHT[2], 80))
        painter.drawEllipse(QPointF(*self.pos), 5.0, 5.0)


class _SpentSplat:
    """A short-lived torn-silk splat left where a capture released."""

    def __init__(self, at: Point) -> None:
        self.at = (float(at[0]), float(at[1]))
        self.t = 0.0
        self.spokes = []
        for _ in range(7):
            a = random.uniform(0.0, math.tau)
            r = random.uniform(8.0, 22.0)
            self.spokes.append((math.cos(a) * r, math.sin(a) * r))

    def update(self, dt: float) -> bool:
        self.t += dt
        return self.t >= SPENT_FADE

    def bbox(self) -> Tuple[float, float, float, float]:
        return (self.at[0] - 26.0, self.at[1] - 26.0,
                self.at[0] + 26.0, self.at[1] + 26.0)

    def draw(self, painter) -> None:
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen

        fade = clamp(1.0 - self.t / SPENT_FADE, 0.0, 1.0)
        a = int(150 * fade)
        if a <= 4:
            return
        pen = QPen(QColor(_SILK[0], _SILK[1], _SILK[2], a), 1.0)
        pen.setCapStyle(Qt.RoundCap)
        painter.setPen(pen)
        for off in self.spokes:
            # Spokes recoil outward slightly as the silk relaxes.
            k = 1.0 + (1.0 - fade) * 0.4
            painter.drawLine(QPointF(*self.at),
                             QPointF(self.at[0] + off[0] * k, self.at[1] + off[1] * k))


# ======================================================================
# The shared world the manager owns.
# ======================================================================
class MouseWebWorld:
    """Holds the single active cursor-silk effect plus fading spent splats.

    Only one projectile/capture is live at a time (there is one pointer).  The
    manager advances this each frame with the live cursor position; the engine
    applies the returned desired position to the real pointer.
    """

    def __init__(self, screen_w: float, screen_h: float,
                 can_control: bool = True) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.enabled = True
        self.can_control = bool(can_control)
        self.projectile: Optional[_Projectile] = None
        self.capture: Optional[_Capture] = None
        self.spent: List[_SpentSplat] = []
        self._dirty: List[Tuple[float, float, float, float]] = []
        self._prev_active_bbox: Optional[Tuple[float, float, float, float]] = None

    # -- lifecycle ------------------------------------------------------
    def set_screen(self, screen_w: float, screen_h: float) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        # A live capture references absolute coordinates that may now be off
        # screen; safest to release it cleanly.
        self.clear()

    def clear(self) -> None:
        if self._prev_active_bbox is not None:
            self._dirty.append(_xywh(self._prev_active_bbox))
        if self.capture is not None:
            self._dirty.append(_xywh(self.capture.bbox()))
        if self.projectile is not None:
            self._dirty.append(_xywh(self.projectile.bbox()))
        self.projectile = None
        self.capture = None
        self._prev_active_bbox = None

    def busy(self) -> bool:
        return self.projectile is not None or self.capture is not None

    def active_kind(self) -> Optional[str]:
        if self.projectile is not None:
            return self.projectile.kind
        if self.capture is not None:
            return self.capture.kind
        return None

    # -- weaver/spider API ---------------------------------------------
    def shoot(self, origin: Point, target: Point, kind: str = "trap",
              homing: float = 0.0, lead: Point = (0.0, 0.0)) -> bool:
        """Launch a glob from ``origin`` toward ``target``.  One at a time."""
        if not self.enabled or self.busy():
            return False
        kind = "wall" if str(kind).lower().startswith("w") else "trap"
        self.projectile = _Projectile(origin, target, kind, homing=homing, lead=lead)
        return True

    # -- per-frame ------------------------------------------------------
    def update(self, dt: float, mx: float, my: float) -> Optional[Point]:
        desired: Optional[Point] = None
        dirty: List[Tuple[float, float, float, float]] = []

        # Fade spent splats.
        if self.spent:
            still: List[_SpentSplat] = []
            for s in self.spent:
                dirty.append(_xywh(s.bbox()))
                if not s.update(dt):
                    still.append(s)
            self.spent = still

        if not self.enabled:
            # Effects are off: drop anything live and free the pointer.
            if self.busy():
                self.clear()
            self._merge_active_dirty(dirty)
            return None

        if self.projectile is not None:
            result = self.projectile.update(dt, mx, my)
            if result == "hit":
                hit = self.projectile.pos
                if self.projectile.kind == "wall":
                    fire = (hit[0] - self.projectile.origin[0],
                            hit[1] - self.projectile.origin[1])
                    self.capture = ShoveCapture(hit, fire, self.screen_w, self.screen_h)
                else:
                    # Anchor the trap on the live pointer for a clean grab.
                    self.capture = TrapCapture((mx, my))
                self.projectile = None
            elif result == "miss":
                self.spent.append(_SpentSplat(self.projectile.pos))
                self.projectile = None

        if self.capture is not None:
            desired, done = self.capture.update(dt, mx, my, self.can_control)
            if done:
                self.spent.append(_SpentSplat(self.capture.release_point()))
                self.capture = None
                desired = None

        self._merge_active_dirty(dirty)
        return desired

    def _merge_active_dirty(self, dirty: List[Tuple[float, float, float, float]]) -> None:
        # The live projectile/capture move with the pointer every frame, so we
        # must repaint both their current footprint and the one they just left.
        cur = None
        if self.projectile is not None:
            cur = self.projectile.bbox()
        elif self.capture is not None:
            cur = self.capture.bbox()
        if cur is not None:
            dirty.append(_xywh(cur))
        if self._prev_active_bbox is not None:
            dirty.append(_xywh(self._prev_active_bbox))
        self._prev_active_bbox = cur
        self._dirty.extend(dirty)

    def dirty_rects(self) -> List[Tuple[float, float, float, float]]:
        out = self._dirty
        self._dirty = []
        return out

    def is_animating(self) -> bool:
        return self.busy() or bool(self.spent)

    # -- drawing --------------------------------------------------------
    def render(self, painter, clip: Optional[Tuple[float, float, float, float]] = None) -> None:
        def visible(bbox):
            if clip is None:
                return True
            x0, y0, x1, y1 = bbox
            return not (x1 < clip[0] or x0 > clip[2] or y1 < clip[1] or y0 > clip[3])

        for s in self.spent:
            if visible(s.bbox()):
                s.draw(painter)
        if self.capture is not None and visible(self.capture.bbox()):
            self.capture.draw(painter)
        if self.projectile is not None and visible(self.projectile.bbox()):
            self.projectile.draw(painter)


def _xywh(bbox: Tuple[float, float, float, float]) -> Tuple[float, float, float, float]:
    x0, y0, x1, y1 = bbox
    return (x0, y0, x1 - x0, y1 - y0)
