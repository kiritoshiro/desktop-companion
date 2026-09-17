from __future__ import annotations

"""Flies: tiny autonomous prey that the spiders hunt.

A :class:`Fly` is a very small, self-contained creature compared with a
:class:`~desktop_bug.creature.Creature`.  It buzzes around the screen, flees
from nearby spiders, and can blunder into a finished web and get stuck.  A
stuck fly tugs the silk (which the engine repaints and which draws spiders in),
and any spider that reaches a fly devours it.

The flock lives in :class:`FlyWorld`, which the :class:`CreatureManager` owns
next to the web worlds.  The manager drives the predator side of the
interaction (which spider hunts which fly, the killing bite); this module only
owns the flies themselves: their motion, their fear of spiders, their capture
on silk, and their drawing.
"""

import math
import random
from typing import List, Optional, Sequence, Tuple

from PyQt5.QtCore import QPointF, Qt
from PyQt5.QtGui import QBrush, QColor, QPainterPath, QPen

from .math_utils import angle_lerp, clamp, distance, normalize_angle

Point = Tuple[float, float]


# A fly is small.  These are body lengths in screen pixels before the per-fly
# size jitter and the shared scale are applied.
FLY_BASE_LENGTH = 9.0
FLY_MIN_LENGTH = 5.5
FLY_MAX_LENGTH = 13.0

# Normal surface-walking / panic speeds in pixels per second.  The ordinary
# pace is deliberately modest; flies spend much of their time stopped, turning,
# or grooming-looking rather than continuously sprinting across the desktop.
FLY_CRUISE_SPEED = (38.0, 72.0)
FLY_PANIC_SPEED = (210.0, 300.0)

# Edge keep-in margin.  Flies turn back toward the middle before the very edge.
FLY_EDGE_MARGIN = 16.0

# How long the death animation plays before the fly is removed.
FLY_EAT_DURATION = 0.42


def _seg_project(ax: float, ay: float, bx: float, by: float, x: float, y: float) -> Tuple[Point, float]:
    """Closest point on segment AB to (x, y), plus the distance to it."""
    dx, dy = bx - ax, by - ay
    d2 = dx * dx + dy * dy
    if d2 <= 1e-9:
        return (ax, ay), distance(ax, ay, x, y)
    t = clamp(((x - ax) * dx + (y - ay) * dy) / d2, 0.0, 1.0)
    px, py = ax + dx * t, ay + dy * t
    return (px, py), distance(px, py, x, y)


class Fly:
    """A single buzzing fly."""

    def __init__(self, x: float, y: float, screen_w: float, screen_h: float,
                 heading: Optional[float] = None, scale: float = 1.0) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.scale = clamp(float(scale), 0.5, 2.2)

        self.x = float(x)
        self.y = float(y)
        jitter = random.uniform(0.82, 1.22)
        self.length = clamp(FLY_BASE_LENGTH * jitter * self.scale, FLY_MIN_LENGTH, FLY_MAX_LENGTH)
        # A convenient "radius-ish" size other code (capture/catch maths) reads.
        self.size = self.length

        self.heading = heading if heading is not None else random.uniform(-math.pi, math.pi)
        self.target_heading = self.heading
        self.cruise = random.uniform(*FLY_CRUISE_SPEED)
        self.panic = random.uniform(*FLY_PANIC_SPEED)
        self.speed = self.cruise
        self.vx = math.cos(self.heading) * self.speed
        self.vy = math.sin(self.heading) * self.speed
        self.turn_rate = random.uniform(7.0, 12.0)

        # Natural stop-walk-turn bookkeeping.  A calm fly takes short walks,
        # freezes for irregular pauses, and often turns in place before moving
        # again.  Panic temporarily overrides this little state machine.
        self.motion_mode = random.choice(("walk", "pause", "pause", "turn"))
        if self.motion_mode == "walk":
            self.motion_timer = random.uniform(0.22, 0.85)
        elif self.motion_mode == "turn":
            self.motion_timer = random.uniform(0.08, 0.28)
        else:
            self.motion_timer = random.uniform(0.18, 1.15)
        if self.motion_mode != "walk":
            self.speed = 0.0
            self.vx = self.vy = 0.0
        self.wander_timer = random.uniform(0.12, 0.45)
        self.hover_timer = 0.0
        self.panic_level = 0.0

        # Buzzing wing / body wobble clocks.
        self.wing_phase = random.random() * math.tau
        self.wing_rate = random.uniform(46.0, 64.0)
        self.buzz_phase = random.random() * math.tau
        self.buzz_rate = random.uniform(18.0, 26.0)

        # State: "flying" -> "trapped" -> "eaten" -> (removed)
        self.state = "flying"

        # Capture: a fly can be stuck on a real Web, pinned in place by a
        # trapper's thrown silk, or wrapped by a web-shot glob.  These flags
        # record which, and ``tether_from`` / ``web_splat`` drive the drawing.
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from: Optional[Point] = None
        self.web_splat = 0.0
        self._trapper = None
        self.struggle_phase = random.random() * math.tau
        self.pluck_timer = random.uniform(0.12, 0.28)
        self.trapped_for = 0.0
        self.free_progress = 0.0
        self.web_check_timer = random.uniform(0.0, 0.08)
        self._draw_jitter = 0.0
        self._web_pegs: list = []

        # Dragging: while a fly is held by the cursor it just hangs there
        # buzzing, immune to spiders, until it is let go.
        self.dragging = False
        self._drag_dx = 0.0
        self._drag_dy = 0.0
        self._drag_hist: list = []

        # Eaten animation.
        self.eat_t = 0.0

        # Predator bookkeeping (set/read by the manager).  ``hunters`` holds the
        # spiders currently locked onto this fly so the manager can spread the
        # pack out a little instead of dogpiling one fly.
        self.hunters = set()

        # Cached footprint for partial repaints.
        self._bbox_prev: Optional[Tuple[float, float, float, float]] = None

    # -- status ---------------------------------------------------------
    @property
    def alive(self) -> bool:
        """A fly a spider can still chase and eat."""
        return self.state in ("flying", "trapped")

    @property
    def trapped(self) -> bool:
        return self.state == "trapped"

    @property
    def eaten(self) -> bool:
        return self.state == "eaten"

    @property
    def removable(self) -> bool:
        return self.state == "gone"

    @property
    def is_moving(self) -> bool:
        """Whether a stalking hunter should advance this frame.

        Panic and thrown motion count as movement.  During a calm pause or an
        in-place turn the fly is considered still, even while its wings/body
        continue their tiny idle animation.
        """
        if self.state != "flying" or self.dragging:
            return False
        if self.panic_level > 0.08:
            return True
        return self.motion_mode == "walk" and math.hypot(self.vx, self.vy) > 5.0

    # -- capture state changes -----------------------------------------
    def stick_to_web(self, web, point: Point) -> None:
        self.state = "trapped"
        self.stuck_web = web
        self.captured_by_web = True
        self.tether_from = None
        self.web_splat = 0.0
        self.x, self.y = float(point[0]), float(point[1])
        self.vx = self.vy = 0.0
        self.speed = 0.0
        self.trapped_for = 0.0
        self.free_progress = 0.0
        self.pluck_timer = random.uniform(0.05, 0.14)
        try:
            web.pluck((self.x, self.y), strength=0.9)
        except Exception:
            pass

    def pin_with_tether(self, origin: Point) -> None:
        """Pinned in place by a trapper's thrown silk (no real web involved)."""
        self.state = "trapped"
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from = (float(origin[0]), float(origin[1]))
        self.web_splat = 0.0
        self.vx = self.vy = 0.0
        self.speed = 0.0
        self.trapped_for = 0.0
        self.free_progress = 0.0
        self.pluck_timer = random.uniform(0.1, 0.2)

    def _make_web_pegs(self) -> None:
        """Fixed anchor points for a static web splat (generated once per catch)."""
        self._web_pegs = []
        L = self.length
        n = random.randint(6, 8)
        for i in range(n):
            a = (i / n) * math.tau + random.uniform(-0.28, 0.28)
            r = L * random.uniform(1.4, 2.6)
            self._web_pegs.append((math.cos(a) * r, math.sin(a) * r))

    def pin_with_web_shot(self, origin: Point, kind: str = "trap") -> None:
        """Wrapped where it flew by a glob of silk shot from a spider.

        Looks like the silk a spider flings at the cursor: the fly is webbed in
        place with a clinging splat, with no line trailing back to the shooter.
        """
        self.state = "trapped"
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from = None
        self.web_splat = 1.0
        self._make_web_pegs()
        self.vx = self.vy = 0.0
        self.speed = 0.0
        self.trapped_for = 0.0
        # Web shot wraps tighter than a single thrown thread: harder to escape.
        self.free_progress = -0.6
        self.pluck_timer = random.uniform(0.1, 0.2)

    def break_free(self) -> None:
        self.state = "flying"
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from = None
        self.web_splat = 0.0
        self._trapper = None
        self.panic_level = 1.0
        self.motion_mode = "walk"
        self.motion_timer = random.uniform(0.35, 0.8)
        self.speed = self.panic
        # Pop away in a random direction.
        self.heading = random.uniform(-math.pi, math.pi)
        self.target_heading = self.heading
        self.vx = math.cos(self.heading) * self.speed
        self.vy = math.sin(self.heading) * self.speed

    def begin_eaten(self) -> None:
        if self.state == "eaten":
            return
        self.state = "eaten"
        self.eat_t = 0.0
        self.dragging = False
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from = None
        self.web_splat = 0.0

    # -- dragging -------------------------------------------------------
    def start_drag(self, mx: float, my: float) -> None:
        # Picked up: it stops being trapped/targeted and just dangles in hand.
        self.dragging = True
        self.state = "flying"
        self.stuck_web = None
        self.captured_by_web = False
        self.tether_from = None
        self.web_splat = 0.0
        self._trapper = None
        self.panic_level = 0.0
        self._drag_dx = self.x - mx
        self._drag_dy = self.y - my
        self._drag_hist = [(mx, my)]

    def drag_to(self, mx: float, my: float) -> None:
        self.x = clamp(mx + self._drag_dx, FLY_EDGE_MARGIN, self.screen_w - FLY_EDGE_MARGIN)
        self.y = clamp(my + self._drag_dy, FLY_EDGE_MARGIN, self.screen_h - FLY_EDGE_MARGIN)
        self._drag_hist.append((mx, my))
        if len(self._drag_hist) > 5:
            self._drag_hist = self._drag_hist[-5:]

    def release_drag(self, mx: float, my: float) -> None:
        self.dragging = False
        self.state = "flying"
        # Toss it along the recent drag motion so a flung fly zips off.
        vx = vy = 0.0
        if len(self._drag_hist) >= 2:
            (ax, ay), (bx, by) = self._drag_hist[0], self._drag_hist[-1]
            vx, vy = (bx - ax) * 6.0, (by - ay) * 6.0
        sp = math.hypot(vx, vy)
        if sp > 12.0:
            self.heading = math.atan2(vy, vx)
            self.target_heading = self.heading
            self.speed = clamp(sp, self.cruise, self.panic)
            self.panic_level = clamp(sp / self.panic, 0.0, 1.0)
        else:
            self.motion_mode = "walk"
            self.motion_timer = random.uniform(0.3, 0.8)
            self.speed = self.cruise
        self.vx = math.cos(self.heading) * self.speed
        self.vy = math.sin(self.heading) * self.speed
        self._drag_hist = []

    # -- per-frame ------------------------------------------------------
    def update(self, dt: float, spiders: Sequence, web_world) -> None:
        self.wing_phase += dt * self.wing_rate
        self.buzz_phase += dt * self.buzz_rate
        self.struggle_phase += dt * random.uniform(11.0, 16.0)

        if self.dragging:
            # Held by the cursor: hang there buzzing hard, nothing else.
            self.panic_level = max(self.panic_level, 0.4)
            self._draw_jitter = math.sin(self.buzz_phase) * self.length * 0.12
            return

        if self.state == "eaten":
            self.eat_t += dt
            if self.eat_t >= FLY_EAT_DURATION:
                self.state = "gone"
            return

        if self.state == "trapped":
            self._update_trapped(dt, web_world)
            return

        self._update_flying(dt, spiders, web_world)

    # -- trapped --------------------------------------------------------
    def _update_trapped(self, dt: float, web_world) -> None:
        self.trapped_for += dt
        # A pin with no real web, no tether, and no splat has nothing holding it;
        # the silk loses its grip and the fly wrenches loose.
        if (self.stuck_web is None and self.tether_from is None
                and self.web_splat <= 0.0):
            self.break_free()
            return
        if self.stuck_web is not None:
            webs = getattr(web_world, "webs", None)
            if webs is not None and self.stuck_web not in webs:
                # The web it was caught on disappeared (cleared/resized away).
                self.break_free()
                return
            # Tug the silk on a timer.  Each tug plucks the web so it visibly
            # quivers; the engine repaints animating webs and the manager reads
            # this as a distress signal that pulls nearby spiders in.
            self.pluck_timer -= dt
            if self.pluck_timer <= 0.0:
                self.pluck_timer = random.uniform(0.16, 0.34)
                try:
                    self.stuck_web.pluck((self.x, self.y),
                                         strength=random.uniform(0.55, 1.0))
                except Exception:
                    pass

        # Struggle: small wing-beating jitter while caught.
        wob = math.sin(self.struggle_phase) * self.length * 0.16
        self._draw_jitter = wob

        # A fly can eventually wrench free if nothing eats it.  A real web holds
        # tightest, a shot-on splat is next, a single thrown thread is weakest.
        if self.captured_by_web:
            escape_rate = 0.05
        elif self.web_splat > 0.0:
            escape_rate = 0.08
        else:
            escape_rate = 0.14
        self.free_progress += dt * escape_rate * random.uniform(0.4, 1.2)
        if self.free_progress >= 1.0:
            self.break_free()

    # -- flying ---------------------------------------------------------
    def _update_flying(self, dt: float, spiders: Sequence, web_world) -> None:
        # 1) Fear: steer away from any spider that has come too close.
        flee_x = 0.0
        flee_y = 0.0
        for sp in spiders:
            if sp is None:
                continue
            if getattr(sp, "dragging", False) or getattr(sp, "_desktop_fully_hidden", False):
                continue

            # A Hunter locked onto this exact fly stalks by moving only while the
            # fly itself walks.  Treat that deliberate stalking approach as
            # visually silent, otherwise the prey notices the spider every time it
            # advances and the stop-and-go hunt can never close the distance.
            is_hunter = False
            hunter_check = getattr(sp, "_is_hunter_personality", None)
            if callable(hunter_check):
                try:
                    is_hunter = bool(hunter_check())
                except Exception:
                    is_hunter = False
            if (is_hunter and getattr(sp, "_hunting_prey", False)
                    and getattr(sp, "_prey", None) is self):
                continue

            d = distance(self.x, self.y, sp.x, sp.y)
            sp_size = float(getattr(sp, "size", 24.0))
            # A bigger, faster, or actively-aiming spider is scarier from
            # farther away.
            moving = float(getattr(sp, "current_speed", 0.0))
            aim = float(getattr(sp, "aim_intent", 0.0))
            panic_radius = sp_size * 5.5 + 110.0 + moving * 0.22 + aim * 70.0
            if getattr(sp, "airborne", False):
                panic_radius += sp_size * 3.0
            if d < panic_radius and d > 1e-3:
                w = (1.0 - d / panic_radius)
                w = w * w
                flee_x += (self.x - sp.x) / d * w
                flee_y += (self.y - sp.y) / d * w

        threat = math.hypot(flee_x, flee_y)
        if threat > 0.01:
            self.panic_level = clamp(self.panic_level + dt * 4.0, 0.0, 1.0)
            flee_heading = math.atan2(flee_y, flee_x)
            self.target_heading = flee_heading
            self.hover_timer = 0.0
            self.wander_timer = random.uniform(0.05, 0.16)
        else:
            self.panic_level = clamp(self.panic_level - dt * 1.6, 0.0, 1.0)
            self._update_wander(dt)

        # 2) Edge avoidance: bias the target heading back toward open space.
        edge = FLY_EDGE_MARGIN + self.length
        steer = self._edge_steer(edge)
        if steer is not None:
            # Blend edge avoidance in firmly; the closer to the wall the stronger.
            self.target_heading = self._blend_heading(self.target_heading, steer, 0.6)

        # 3) Turn toward the desired heading and pick a speed.  Calm flies
        # alternate between walking, stopping, and turning in place.  A real
        # threat overrides that rhythm with a fast continuous escape.
        agility = self.turn_rate * (1.0 + self.panic_level * 0.8)
        self.heading = angle_lerp(self.heading, self.target_heading, agility * dt)
        if self.panic_level > 0.03:
            calm_speed = self.cruise
        elif self.motion_mode == "walk":
            calm_speed = self.cruise
        elif self.motion_mode == "turn":
            calm_speed = self.cruise * 0.06
        else:
            calm_speed = 0.0
        target_speed = calm_speed + (self.panic - calm_speed) * self.panic_level
        response = 10.0 if target_speed <= 1.0 else 7.0
        self.speed += (target_speed - self.speed) * clamp(dt * response, 0.0, 1.0)
        if self.panic_level <= 0.03 and self.motion_mode == "pause" and self.speed < 2.0:
            self.speed = 0.0

        # 4) Integrate.  The sideways buzz is strong while moving but tiny while
        # stopped so a resting fly does not visibly slide across the screen.
        fwd_x, fwd_y = math.cos(self.heading), math.sin(self.heading)
        moving01 = clamp(self.speed / max(1.0, self.cruise), 0.0, 1.0)
        buzz = (math.sin(self.buzz_phase) * self.length
                * (0.08 + moving01 * 0.82 + self.panic_level * 0.6))
        perp_x, perp_y = -fwd_y, fwd_x
        self.vx = fwd_x * self.speed + perp_x * buzz
        self.vy = fwd_y * self.speed + perp_y * buzz
        if self.speed <= 0.0 and self.panic_level <= 0.03:
            self.vx = self.vy = 0.0
        self.x += self.vx * dt
        self.y += self.vy * dt

        self._clamp_to_screen()

        # 5) Blunder into a web?  Only finished webs catch flies; throttle the
        #    scan so it is cheap with several flies on screen.
        self.web_check_timer -= dt
        if self.web_check_timer <= 0.0:
            self.web_check_timer = random.uniform(0.05, 0.1)
            self._maybe_get_stuck(dt, web_world)

    def _update_wander(self, dt: float) -> None:
        self.motion_timer -= dt
        self.wander_timer -= dt

        if self.motion_mode == "walk":
            # Small course corrections during a short walking bout.
            if self.wander_timer <= 0.0:
                self.wander_timer = random.uniform(0.18, 0.55)
                self.target_heading = normalize_angle(
                    self.target_heading + random.uniform(-0.34, 0.34))
            if self.motion_timer <= 0.0:
                roll = random.random()
                if roll < 0.58:
                    self.motion_mode = "pause"
                    self.motion_timer = random.uniform(0.22, 1.35)
                else:
                    self.motion_mode = "turn"
                    self.motion_timer = random.uniform(0.09, 0.34)
                    self.target_heading = normalize_angle(
                        self.heading + random.choice((-1.0, 1.0)) * random.uniform(0.45, 2.35))

        elif self.motion_mode == "turn":
            # Rotate mostly in place, then either inspect for a beat or set off.
            if self.motion_timer <= 0.0:
                if random.random() < 0.45:
                    self.motion_mode = "pause"
                    self.motion_timer = random.uniform(0.12, 0.7)
                else:
                    self.motion_mode = "walk"
                    self.motion_timer = random.uniform(0.25, 1.05)
                    self.wander_timer = random.uniform(0.16, 0.5)

        else:  # pause
            # Stay planted.  Some pauses end with a visible in-place turn, while
            # others resume in nearly the same direction.
            if self.motion_timer <= 0.0:
                if random.random() < 0.62:
                    self.motion_mode = "turn"
                    self.motion_timer = random.uniform(0.08, 0.32)
                    self.target_heading = normalize_angle(
                        self.heading + random.choice((-1.0, 1.0)) * random.uniform(0.35, 2.6))
                else:
                    self.motion_mode = "walk"
                    self.motion_timer = random.uniform(0.24, 1.15)
                    self.wander_timer = random.uniform(0.14, 0.45)
                    self.target_heading = normalize_angle(
                        self.heading + random.uniform(-0.42, 0.42))

    def _edge_steer(self, edge: float) -> Optional[float]:
        push_x = 0.0
        push_y = 0.0
        if self.x < edge:
            push_x += 1.0
        elif self.x > self.screen_w - edge:
            push_x -= 1.0
        if self.y < edge:
            push_y += 1.0
        elif self.y > self.screen_h - edge:
            push_y -= 1.0
        if push_x == 0.0 and push_y == 0.0:
            return None
        return math.atan2(push_y, push_x)

    @staticmethod
    def _blend_heading(a: float, b: float, t: float) -> float:
        # Spherical-ish blend on the circle.
        delta = normalize_angle(b - a)
        return normalize_angle(a + delta * clamp(t, 0.0, 1.0))

    def _clamp_to_screen(self) -> None:
        m = FLY_EDGE_MARGIN
        if self.x < m:
            self.x = m
            self.heading = self._reflect_x()
        elif self.x > self.screen_w - m:
            self.x = self.screen_w - m
            self.heading = self._reflect_x()
        if self.y < m:
            self.y = m
            self.heading = self._reflect_y()
        elif self.y > self.screen_h - m:
            self.y = self.screen_h - m
            self.heading = self._reflect_y()

    def _reflect_x(self) -> float:
        h = normalize_angle(math.pi - self.heading)
        self.target_heading = h
        return h

    def _reflect_y(self) -> float:
        h = normalize_angle(-self.heading)
        self.target_heading = h
        return h

    def _maybe_get_stuck(self, dt: float, web_world) -> None:
        webs = getattr(web_world, "webs", None)
        if not webs:
            return
        # A fast, panicking fly is harder to catch than one drifting lazily.
        base_chance = 0.55 if self.panic_level < 0.3 else 0.22
        reach = self.length * 1.3 + 5.0
        for web in webs:
            try:
                if not web.is_complete():
                    continue
            except Exception:
                continue
            snap = self._web_capture_point(web, reach)
            if snap is None:
                continue
            # Probability scaled to the frame so the catch does not depend on FPS.
            if random.random() < clamp(base_chance * (self.web_check_timer + dt) * 12.0, 0.02, 0.85):
                self.stick_to_web(web, snap)
                return

    def _web_capture_point(self, web, radius: float) -> Optional[Point]:
        """Nearest point on the web's silk within ``radius`` of the fly, or None."""
        try:
            x0, y0, x1, y1 = web.bbox()
        except Exception:
            return None
        if (self.x < x0 - radius or self.x > x1 + radius
                or self.y < y0 - radius or self.y > y1 + radius):
            return None
        tearable = getattr(web, "_tearable", None)
        strands = getattr(web, "strands", None)
        if not tearable or not strands:
            return None
        best_pt: Optional[Point] = None
        best_d = radius
        for (si, segi, _length) in tearable:
            if si < 0 or si >= len(strands):
                continue
            s = strands[si]
            pts = s.points
            if segi < 0 or segi + 1 >= len(pts):
                continue
            ax, ay = pts[segi]
            bx, by = pts[segi + 1]
            pt, d = _seg_project(ax, ay, bx, by, self.x, self.y)
            if d < best_d:
                best_d = d
                best_pt = pt
        return best_pt

    # -- geometry / drawing --------------------------------------------
    def footprint(self) -> Tuple[float, float, float, float]:
        # Generous: wings + splat + struggle swing can extend past the body.
        pad = self.length * 2.9 + 12.0
        if self.tether_from is not None:
            tx, ty = self.tether_from
            x0 = min(self.x, tx) - pad
            y0 = min(self.y, ty) - pad
            x1 = max(self.x, tx) + pad
            y1 = max(self.y, ty) + pad
            return (x0, y0, x1 - x0, y1 - y0)
        return (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)

    def render(self, painter) -> None:
        if self.state == "gone":
            return
        if self.state == "eaten":
            self._render_eaten(painter)
            return

        struggle = 0.0
        if self.state == "trapped":
            struggle = math.sin(self.struggle_phase) * 0.5

        painter.save()
        painter.translate(self.x, self.y)

        # Draw the trap silk first so the body sits over it.
        if self.state == "trapped":
            self._render_capture_silk(painter)

        # Orient the body along the heading (plus a little struggle wag), and
        # offset by the buzzing jitter when trapped or held.
        jitter = getattr(self, "_draw_jitter", 0.0)
        body_angle = self.heading + struggle * 0.4
        painter.save()
        if jitter:
            painter.translate(math.cos(self.struggle_phase * 1.3) * jitter,
                              math.sin(self.struggle_phase) * jitter)
        painter.rotate(math.degrees(body_angle))
        self._render_body(painter)
        painter.restore()

        painter.restore()

    def _render_capture_silk(self, painter) -> None:
        # A clinging splat of silk where the fly is webbed, drawn like the silk
        # that catches the cursor: bowed anchor strands, a faint capture spiral,
        # and a sticky knot.  No line trails back to the spider, and it does not
        # rotate; only a tiny struggle wobble of the centre shows it is alive.
        if not self._web_pegs:
            self._make_web_pegs()
        wob = math.sin(self.struggle_phase) * self.length * 0.05
        cx, cy = wob * 0.4, wob

        pen = QPen(QColor(240, 242, 248, 150), 1.0)
        pen.setCapStyle(Qt.RoundCap)
        painter.setPen(pen)
        painter.setBrush(Qt.NoBrush)
        for (px, py) in self._web_pegs:
            bx = (px + cx) * 0.5 + (cx - px) * 0.12
            by = (py + cy) * 0.5 + (cy - py) * 0.12
            path = QPainterPath(QPointF(px, py))
            path.quadTo(QPointF(bx, by), QPointF(cx, cy))
            painter.drawPath(path)

        # A faint concentric capture-spiral arc around the centre.
        painter.setPen(QPen(QColor(235, 240, 250, 90), 1.0, Qt.SolidLine, Qt.RoundCap))
        r = self.length * 1.5
        steps = 14
        pts = [QPointF(cx + math.cos((s / steps) * math.tau) * r,
                       cy + math.sin((s / steps) * math.tau) * r * 0.92)
               for s in range(steps + 1)]
        painter.drawPolyline(*pts)

        # Bright sticky knot over the wrapped fly.
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(245, 247, 252, 210))
        painter.drawEllipse(QPointF(cx, cy), 2.6, 2.6)
        painter.setBrush(QColor(245, 247, 252, 80))
        painter.drawEllipse(QPointF(cx, cy), 4.8, 4.8)

    def _render_body(self, painter) -> None:
        L = self.length
        # Wing flicker: alpha pulses with the beat so they read as a blur.
        beat = (math.sin(self.wing_phase) * 0.5 + 0.5)
        wing_alpha = int(70 + beat * 70)
        wing_spread = 0.55 + beat * 0.5

        painter.setPen(Qt.NoPen)

        # Wings (two translucent ovals swept back from the thorax).
        painter.setBrush(QBrush(QColor(205, 215, 230, wing_alpha)))
        for side in (-1.0, 1.0):
            painter.save()
            painter.translate(-L * 0.05, side * L * 0.18)
            painter.rotate(side * (18.0 + wing_spread * 22.0))
            painter.drawEllipse(QPointF(-L * 0.45, 0.0), L * 0.62, L * 0.30)
            painter.restore()

        # Abdomen (rear, larger, dark with a faint segmented sheen).
        painter.setBrush(QBrush(QColor(34, 36, 42, 255)))
        painter.drawEllipse(QPointF(-L * 0.42, 0.0), L * 0.55, L * 0.40)
        painter.setBrush(QBrush(QColor(60, 64, 74, 180)))
        painter.drawEllipse(QPointF(-L * 0.30, -L * 0.06), L * 0.30, L * 0.16)

        # Thorax (front body, slightly green-bronze sheen).
        painter.setBrush(QBrush(QColor(46, 52, 50, 255)))
        painter.drawEllipse(QPointF(L * 0.12, 0.0), L * 0.42, L * 0.34)
        painter.setBrush(QBrush(QColor(78, 96, 88, 150)))
        painter.drawEllipse(QPointF(L * 0.18, -L * 0.05), L * 0.20, L * 0.12)

        # Head + the two big red compound eyes.
        painter.setBrush(QBrush(QColor(28, 28, 32, 255)))
        painter.drawEllipse(QPointF(L * 0.52, 0.0), L * 0.24, L * 0.24)
        painter.setBrush(QBrush(QColor(150, 40, 34, 235)))
        for side in (-1.0, 1.0):
            painter.drawEllipse(QPointF(L * 0.56, side * L * 0.12), L * 0.13, L * 0.15)

        # Tiny legs trailing under the body.
        pen = QPen(QColor(20, 20, 24, 220))
        pen.setWidthF(max(0.8, L * 0.07))
        pen.setCapStyle(Qt.RoundCap)
        painter.setPen(pen)
        painter.setBrush(Qt.NoBrush)
        for i in range(3):
            ox = L * (0.18 - i * 0.20)
            for side in (-1.0, 1.0):
                painter.drawLine(
                    QPointF(ox, side * L * 0.18),
                    QPointF(ox - L * 0.18, side * (L * 0.42 + i * 1.0)),
                )

    def _render_eaten(self, painter) -> None:
        t = clamp(self.eat_t / FLY_EAT_DURATION, 0.0, 1.0)
        painter.save()
        painter.translate(self.x, self.y)
        # A quick scatter of little crumbs/legs flying apart, fading out.
        alpha = int(200 * (1.0 - t))
        if alpha <= 2:
            painter.restore()
            return
        pen = QPen(QColor(40, 40, 46, alpha))
        pen.setWidthF(max(0.8, self.length * 0.10))
        pen.setCapStyle(Qt.RoundCap)
        painter.setPen(pen)
        spread = self.length * (1.0 + t * 2.6)
        for i in range(6):
            ang = (i / 6.0) * math.tau + self.struggle_phase * 0.2
            x = math.cos(ang) * spread
            y = math.sin(ang) * spread
            painter.drawLine(QPointF(x * 0.4, y * 0.4), QPointF(x, y))
        painter.restore()


# Silk colours shared with the cursor web-shot so the two look like the same silk.
_SHOT_SILK = (236, 240, 248)
WEB_SHOT_SPEED = 1500.0
WEB_SHOT_HIT_RADIUS = 16.0


class WebShotProjectile:
    """A glob of silk a spider flings at a fly: thrown, not guided.

    This is the same idea as the cursor web shot, but aimed at a fly instead of
    the pointer, so it never touches the real mouse. Like that one it used to
    steer onto its target for the whole flight. It now aims once, leading the
    fly by where it is actually going, and flies straight; in-flight correction
    is what the Silk tracking ability restores.
    """

    def __init__(self, shooter, fly, kind: str = "trap", homing: float = 0.0) -> None:
        self.shooter = shooter
        self.fly = fly
        self.kind = kind
        self.homing = max(0.0, float(homing))
        fx, fy = math.cos(shooter.heading), math.sin(shooter.heading)
        ox = shooter.x + fx * shooter.size * 0.6
        oy = shooter.y + fy * shooter.size * 0.6
        self.origin = (ox, oy)
        self.pos = (ox, oy)
        # Lead the fly, or a straight shot would always arrive behind it.
        # Solved iteratively for the same reason as the cursor glob: aiming
        # ahead lengthens the flight, which moves the interception point again.
        travel = (math.hypot(fly.x - ox, fly.y - oy) or 1.0) / WEB_SHOT_SPEED
        fvx = getattr(fly, "vx", 0.0)
        fvy = getattr(fly, "vy", 0.0)
        aim_x, aim_y = fly.x, fly.y
        for _ in range(4):
            aim_x = fly.x + fvx * travel
            aim_y = fly.y + fvy * travel
            travel = (math.hypot(aim_x - ox, aim_y - oy) or 1.0) / WEB_SHOT_SPEED
        ux, uy = (aim_x - ox), (aim_y - oy)
        d = math.hypot(ux, uy) or 1.0
        self.vel = (ux / d * WEB_SHOT_SPEED, uy / d * WEB_SHOT_SPEED)
        self.travelled = 0.0
        self.max_travel = max(160.0, d * 1.8)
        self.trail = [self.pos]
        self.done = False
        self.hit = False

    def update(self, dt: float) -> None:
        fly = self.fly
        # If the prey is gone or already caught, the glob just fizzles.
        if fly is None or not fly.alive or fly.eaten or fly.dragging or fly.trapped:
            self.done = True
            return
        tx, ty = fly.x, fly.y
        if self.homing > 0.0:
            dx, dy = tx - self.pos[0], ty - self.pos[1]
            dd = math.hypot(dx, dy) or 1.0
            vx = self.vel[0] + dx / dd * WEB_SHOT_SPEED * 5.0 * self.homing * dt
            vy = self.vel[1] + dy / dd * WEB_SHOT_SPEED * 5.0 * self.homing * dt
            sp = math.hypot(vx, vy) or 1.0
            vx, vy = vx / sp * WEB_SHOT_SPEED, vy / sp * WEB_SHOT_SPEED
        else:
            vx, vy = self.vel
        self.vel = (vx, vy)
        nx, ny = self.pos[0] + vx * dt, self.pos[1] + vy * dt
        self.travelled += math.hypot(nx - self.pos[0], ny - self.pos[1])
        self.pos = (nx, ny)
        self.trail.append(self.pos)
        if len(self.trail) > 9:
            self.trail = self.trail[-9:]
        if distance(nx, ny, tx, ty) <= WEB_SHOT_HIT_RADIUS:
            fly.pin_with_web_shot(self.origin, self.kind)
            fly._trapper = self.shooter
            self.hit = True
            self.done = True
        elif self.travelled >= self.max_travel:
            self.done = True

    def footprint(self) -> Tuple[float, float, float, float]:
        xs = [p[0] for p in self.trail]
        ys = [p[1] for p in self.trail]
        pad = 12.0
        return (min(xs) - pad, min(ys) - pad,
                (max(xs) - min(xs)) + pad * 2, (max(ys) - min(ys)) + pad * 2)

    def render(self, painter) -> None:
        n = len(self.trail)
        for i in range(1, n):
            a = int(130 * (i / n))
            pen = QPen(QColor(_SHOT_SILK[0], _SHOT_SILK[1], _SHOT_SILK[2], a),
                       0.8 + 1.5 * (i / n))
            pen.setCapStyle(Qt.RoundCap)
            painter.setPen(pen)
            painter.drawLine(QPointF(*self.trail[i - 1]), QPointF(*self.trail[i]))
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(_SHOT_SILK[0], _SHOT_SILK[1], _SHOT_SILK[2], 230))
        painter.drawEllipse(QPointF(*self.pos), 3.0, 3.0)
        painter.setBrush(QColor(_SHOT_SILK[0], _SHOT_SILK[1], _SHOT_SILK[2], 80))
        painter.drawEllipse(QPointF(*self.pos), 5.4, 5.4)


class FlyRemains:
    """The leftover bits of a devoured fly, fading on the spot over a few seconds."""

    LIFETIME = 3.6

    def __init__(self, x: float, y: float, scale: float = 1.0) -> None:
        self.x = float(x)
        self.y = float(y)
        self.scale = clamp(float(scale), 0.5, 2.2)
        self.t = 0.0
        L = FLY_BASE_LENGTH * self.scale
        # A few disassembled parts: legs, a torn wing, a body crumb, each with a
        # small outward offset and final resting spot.
        self.parts = []
        for _ in range(random.randint(4, 6)):
            ang = random.uniform(0.0, math.tau)
            dist = random.uniform(L * 0.4, L * 1.6)
            kind = random.choice(("leg", "leg", "wing", "crumb"))
            self.parts.append({
                "x": math.cos(ang) * dist,
                "y": math.sin(ang) * dist,
                "rot": random.uniform(0.0, math.tau),
                "len": L * random.uniform(0.4, 0.9),
                "kind": kind,
            })

    def update(self, dt: float) -> bool:
        self.t += dt
        return self.t >= self.LIFETIME

    def footprint(self) -> Tuple[float, float, float, float]:
        pad = FLY_BASE_LENGTH * self.scale * 2.2 + 8.0
        return (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)

    def render(self, painter) -> None:
        fade = clamp(1.0 - self.t / self.LIFETIME, 0.0, 1.0)
        if fade <= 0.02:
            return
        alpha = int(190 * fade)
        painter.save()
        painter.translate(self.x, self.y)
        # A faint damp smear under the bits.
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(30, 30, 36, int(50 * fade)))
        painter.drawEllipse(QPointF(0.0, 0.0), FLY_BASE_LENGTH * self.scale * 1.2,
                            FLY_BASE_LENGTH * self.scale * 0.7)
        for part in self.parts:
            painter.save()
            painter.translate(part["x"], part["y"])
            painter.rotate(math.degrees(part["rot"]))
            if part["kind"] == "wing":
                painter.setPen(Qt.NoPen)
                painter.setBrush(QColor(205, 215, 230, int(110 * fade)))
                painter.drawEllipse(QPointF(0.0, 0.0), part["len"] * 0.5, part["len"] * 0.26)
            elif part["kind"] == "crumb":
                painter.setPen(Qt.NoPen)
                painter.setBrush(QColor(36, 38, 44, alpha))
                painter.drawEllipse(QPointF(0.0, 0.0), part["len"] * 0.3, part["len"] * 0.24)
            else:  # leg
                pen = QPen(QColor(22, 22, 26, alpha))
                pen.setWidthF(max(0.8, FLY_BASE_LENGTH * self.scale * 0.08))
                pen.setCapStyle(Qt.RoundCap)
                painter.setPen(pen)
                painter.drawLine(QPointF(-part["len"] * 0.5, 0.0),
                                 QPointF(part["len"] * 0.5, part["len"] * 0.18))
            painter.restore()
        painter.restore()


class FlySpawner:
    """A movable nest the flies crawl out of."""

    def __init__(self, x: float, y: float, scale: float = 1.0) -> None:
        self.x = float(x)
        self.y = float(y)
        self.scale = clamp(float(scale), 0.5, 2.2)
        self.radius = 22.0 * self.scale
        self.pulse = random.random() * math.tau
        self.dragging = False
        self._drag_dx = 0.0
        self._drag_dy = 0.0
        # A few flecks/specks crawling on the nest for life.
        self.specks = []
        for _ in range(5):
            self.specks.append([random.uniform(0.0, math.tau),
                                random.uniform(0.3, 0.85),
                                random.uniform(0.4, 1.1)])

    # -- dragging
    def hit_test(self, x: float, y: float) -> bool:
        return distance(self.x, self.y, x, y) <= self.radius + 4.0

    def start_drag(self, mx: float, my: float) -> None:
        self.dragging = True
        self._drag_dx = self.x - mx
        self._drag_dy = self.y - my

    def drag_to(self, mx: float, my: float, sw: float, sh: float) -> None:
        self.x = clamp(mx + self._drag_dx, self.radius, sw - self.radius)
        self.y = clamp(my + self._drag_dy, self.radius, sh - self.radius)

    def release_drag(self) -> None:
        self.dragging = False

    def clamp_to_screen(self, sw: float, sh: float) -> None:
        self.x = clamp(self.x, self.radius, max(self.radius, sw - self.radius))
        self.y = clamp(self.y, self.radius, max(self.radius, sh - self.radius))

    def emit_point(self) -> Tuple[float, float, float]:
        """A point at the nest opening with an outward-ish heading."""
        ang = random.uniform(0.0, math.tau)
        r = self.radius * 0.5
        return (self.x + math.cos(ang) * r, self.y + math.sin(ang) * r, ang)

    def update(self, dt: float) -> None:
        self.pulse += dt * 2.4
        for s in self.specks:
            s[0] += dt * s[2] * 0.6

    def footprint(self) -> Tuple[float, float, float, float]:
        pad = self.radius + 12.0
        return (self.x - pad, self.y - pad, pad * 2.0, pad * 2.0)

    def render(self, painter) -> None:
        painter.save()
        painter.translate(self.x, self.y)
        R = self.radius
        breath = 1.0 + math.sin(self.pulse) * 0.04
        painter.setPen(Qt.NoPen)
        # Soft shadow.
        painter.setBrush(QColor(0, 0, 0, 45))
        painter.drawEllipse(QPointF(0.0, R * 0.55), R * 1.05, R * 0.5)
        # Mottled nest body: a dark rotting clump.
        painter.setBrush(QColor(58, 44, 38, 235))
        painter.drawEllipse(QPointF(0.0, 0.0), R * breath, R * 0.92 * breath)
        painter.setBrush(QColor(78, 60, 48, 220))
        painter.drawEllipse(QPointF(-R * 0.22, -R * 0.16), R * 0.55, R * 0.42)
        painter.setBrush(QColor(96, 78, 58, 150))
        painter.drawEllipse(QPointF(R * 0.18, R * 0.05), R * 0.4, R * 0.3)
        # A dark opening the flies pour out of.
        painter.setBrush(QColor(16, 12, 12, 240))
        painter.drawEllipse(QPointF(0.0, -R * 0.02), R * 0.42, R * 0.36)
        # Crawling specks.
        painter.setBrush(QColor(20, 20, 24, 220))
        for ang, rr, _sp in self.specks:
            sx = math.cos(ang) * R * rr
            sy = math.sin(ang) * R * rr * 0.85
            painter.drawEllipse(QPointF(sx, sy), R * 0.07, R * 0.07)
        painter.restore()


class FlyWorld:
    """Owns every fly: spawning on a timer, updating, and drawing."""

    def __init__(self, screen_w: float, screen_h: float, *, enabled: bool = True,
                 min_interval: float = 4.0, max_interval: float = 9.0,
                 max_flies: int = 6, scale: float = 1.0) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        self.enabled = bool(enabled)
        self.min_interval = float(min_interval)
        self.max_interval = float(max_interval)
        self.max_flies = int(max_flies)
        self.scale = clamp(float(scale), 0.5, 2.2)

        self.flies: List[Fly] = []
        self.spawners: List[FlySpawner] = []
        self.projectiles: List[WebShotProjectile] = []
        self.remains: List[FlyRemains] = []
        self.use_spawner = True
        self._spawn_timer = self._roll_interval()
        self._dirty: List[Tuple[float, float, float, float]] = []
        self._removed: List[Tuple[float, float, float, float]] = []

    # -- configuration --------------------------------------------------
    def set_screen(self, screen_w: float, screen_h: float) -> None:
        self.screen_w = float(screen_w)
        self.screen_h = float(screen_h)
        for fly in self.flies:
            fly.screen_w = self.screen_w
            fly.screen_h = self.screen_h
        for sp in self.spawners:
            sp.clamp_to_screen(self.screen_w, self.screen_h)

    def configure(self, *, enabled: Optional[bool] = None,
                  min_interval: Optional[float] = None,
                  max_interval: Optional[float] = None,
                  max_flies: Optional[int] = None,
                  scale: Optional[float] = None,
                  use_spawner: Optional[bool] = None) -> None:
        if enabled is not None:
            self.enabled = bool(enabled)
        if min_interval is not None:
            self.min_interval = max(0.3, float(min_interval))
        if max_interval is not None:
            self.max_interval = max(0.4, float(max_interval))
        if self.max_interval < self.min_interval:
            self.max_interval = self.min_interval
        if max_flies is not None:
            self.max_flies = max(0, int(max_flies))
        if scale is not None:
            self.scale = clamp(float(scale), 0.5, 2.2)
            for sp in self.spawners:
                sp.scale = self.scale
                sp.radius = 22.0 * self.scale
        if use_spawner is not None:
            self.use_spawner = bool(use_spawner)
            if self.use_spawner:
                self.ensure_spawner()
            else:
                self.clear_spawners()
        # Re-roll the next spawn so interval edits take effect promptly.
        self._spawn_timer = min(self._spawn_timer, self.max_interval)

    def _roll_interval(self) -> float:
        lo = max(0.3, min(self.min_interval, self.max_interval))
        hi = max(lo, max(self.min_interval, self.max_interval))
        return random.uniform(lo, hi)

    # -- lifecycle ------------------------------------------------------
    def clear(self) -> None:
        for fly in self.flies:
            self._removed.append(fly.footprint())
        self.flies = []
        for proj in self.projectiles:
            self._removed.append(proj.footprint())
        self.projectiles = []

    def clear_hunters(self) -> None:
        """Drop predator bookkeeping (used when the spider roster is rebuilt)."""
        for fly in self.flies:
            fly.hunters = set()
            fly._trapper = None
            if fly.state == "trapped" and fly.tether_from is not None and not fly.captured_by_web:
                # A thread from a now-gone spider should not linger forever.
                fly.tether_from = None
        # Projectiles reference spiders that may be gone; drop them.
        for proj in self.projectiles:
            self._removed.append(proj.footprint())
        self.projectiles = []

    def count_alive(self) -> int:
        return sum(1 for f in self.flies if f.alive)

    # -- spawners -------------------------------------------------------
    def _default_spawner_pos(self) -> Tuple[float, float]:
        # Place the default fly nest at the exact center of the screen.
        return (self.screen_w * 0.5, self.screen_h * 0.5)

    def ensure_spawner(self) -> None:
        if not self.spawners:
            x, y = self._default_spawner_pos()
            self.spawners.append(FlySpawner(x, y, scale=self.scale))

    def add_spawner(self, at: Optional[Point] = None) -> "FlySpawner":
        if at is None:
            at = (random.uniform(self.screen_w * 0.2, self.screen_w * 0.8),
                  random.uniform(self.screen_h * 0.2, self.screen_h * 0.8))
        sp = FlySpawner(at[0], at[1], scale=self.scale)
        sp.clamp_to_screen(self.screen_w, self.screen_h)
        self.spawners.append(sp)
        self.use_spawner = True
        return sp

    def clear_spawners(self) -> None:
        for sp in self.spawners:
            self._removed.append(sp.footprint())
        self.spawners = []

    def reset_spawners(self) -> None:
        self.clear_spawners()
        self.use_spawner = True
        self.ensure_spawner()

    def hit_spawner_at(self, x: float, y: float) -> Optional["FlySpawner"]:
        for sp in reversed(self.spawners):
            if sp.hit_test(x, y):
                return sp
        return None

    def hit_fly_at(self, x: float, y: float) -> Optional[Fly]:
        # Topmost (last drawn) first; only catch flies that are not gone/eaten.
        for fly in reversed(self.flies):
            if not fly.alive:
                continue
            grab = fly.length * 1.6 + 6.0
            if distance(fly.x, fly.y, x, y) <= grab:
                return fly
        return None

    # -- web shots / remains -------------------------------------------
    def launch_web_shot(self, shooter, fly, kind: str = "trap") -> bool:
        if fly is None or not fly.alive or fly.trapped or fly.dragging:
            return False
        homing = 0.0
        tracking = getattr(shooter, "_progression_effect", None)
        if callable(tracking):
            homing = tracking("web_homing")
        self.projectiles.append(WebShotProjectile(shooter, fly, kind, homing=homing))
        return True

    def add_remains(self, at: Point, scale: float = 1.0) -> None:
        self.remains.append(FlyRemains(at[0], at[1], scale=scale))

    def _spawn_edge_point(self) -> Tuple[float, float, float]:
        """A point just inside a random screen edge, heading inward."""
        w, h = self.screen_w, self.screen_h
        m = FLY_EDGE_MARGIN + 6.0
        side = random.choice(("top", "bottom", "left", "right"))
        if side == "top":
            return random.uniform(m, w - m), m, random.uniform(0.2, math.pi - 0.2)
        if side == "bottom":
            return random.uniform(m, w - m), h - m, random.uniform(-math.pi + 0.2, -0.2)
        if side == "left":
            return m, random.uniform(m, h - m), random.uniform(-1.2, 1.2)
        return w - m, random.uniform(m, h - m), random.uniform(math.pi - 1.2, math.pi + 1.2)

    def _spawn_origin(self) -> Tuple[float, float, float]:
        """Where a new fly appears: from a nest if one exists, else a screen edge."""
        if self.use_spawner and self.spawners:
            sp = random.choice(self.spawners)
            return sp.emit_point()
        return self._spawn_edge_point()

    def spawn(self, force: bool = False) -> Optional[Fly]:
        if self.count_alive() >= self.max_flies and not force:
            return None
        x, y, heading = self._spawn_origin()
        fly = Fly(x, y, self.screen_w, self.screen_h, heading=heading, scale=self.scale)
        # A fly leaving the nest starts with one short outward walking bout.
        fly.motion_mode = "walk"
        fly.motion_timer = random.uniform(0.28, 0.75)
        fly.speed = fly.cruise
        fly.vx = math.cos(heading) * fly.speed
        fly.vy = math.sin(heading) * fly.speed
        self.flies.append(fly)
        return fly

    def spawn_now(self) -> str:
        """User asked for a fly right now (ignores the enabled toggle)."""
        cap = self.max_flies if self.max_flies > 0 else 1
        if self.count_alive() >= cap:
            return "The screen already has plenty of flies."
        self.spawn(force=True)
        return "Released a fly."

    # -- per-frame ------------------------------------------------------
    def update(self, dt: float, spiders: Sequence, web_world) -> None:
        if self.use_spawner:
            self.ensure_spawner()

        # Spawn on a timer when enabled and below the cap.
        if self.enabled and self.max_flies > 0:
            self._spawn_timer -= dt
            if self._spawn_timer <= 0.0:
                self._spawn_timer = self._roll_interval()
                self.spawn()

        dirty: List[Tuple[float, float, float, float]] = []

        for sp in self.spawners:
            sp.update(dt)
            dirty.append(sp.footprint())

        # Web-shot globs in flight.
        live_proj: List[WebShotProjectile] = []
        for proj in self.projectiles:
            proj.update(dt)
            dirty.append(proj.footprint())
            if not proj.done:
                live_proj.append(proj)
        self.projectiles = live_proj

        # Fading remains.
        live_remains: List[FlyRemains] = []
        for rem in self.remains:
            dirty.append(rem.footprint())
            if not rem.update(dt):
                live_remains.append(rem)
        self.remains = live_remains

        survivors: List[Fly] = []
        for fly in self.flies:
            fly.update(dt, spiders, web_world)
            if fly.removable:
                self._removed.append(fly.footprint())
                continue
            # Active flies always expose their footprint: they move every frame.
            fp = fly.footprint()
            dirty.append(fp)
            if fly._bbox_prev is not None:
                dirty.append(fly._bbox_prev)
            fly._bbox_prev = fp
            survivors.append(fly)
        self.flies = survivors

        if self._removed:
            dirty.extend(self._removed)
            self._removed = []
        self._dirty = dirty

    def dirty_rects(self) -> List[Tuple[float, float, float, float]]:
        return self._dirty

    def render(self, painter, clip: Optional[Tuple[float, float, float, float]] = None) -> None:
        def visible(fp):
            if clip is None:
                return True
            fx, fy, fw, fh = fp
            return not (fx + fw < clip[0] or fx > clip[2] or fy + fh < clip[1] or fy > clip[3])

        # Nest at the bottom, then ground remains, then flies, then the silk
        # globs streaking over the top.
        for sp in self.spawners:
            if visible(sp.footprint()):
                sp.render(painter)
        for rem in self.remains:
            if visible(rem.footprint()):
                rem.render(painter)
        for fly in self.flies:
            if visible(fly.footprint()):
                fly.render(painter)
        for proj in self.projectiles:
            if visible(proj.footprint()):
                proj.render(painter)
