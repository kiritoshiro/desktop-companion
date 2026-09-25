"""Silk on a webbed spider, and the spider fighting it.

The owner: *"when shooting the web at spider it should show on the spider the
net while it is active to indicate it is hit and immobilised, and the spider
should try to move out of it if he wants to be free."* Then, having seen the
first version: *"the net does not stop him from turning and the net gets
distorted. he should stay stationary without any movement when hit. and make
nicer net design not just lines."*

So a webbed spider is held completely still for as long as the net lasts: no
walking and no turning (`_speed_mult` and both locomotion paths). The net is an
orb web laid over it, generated once where the silk hit: irregular spokes,
rings that sag between them, guy lines to the ground, silk bands across the
body and dew at the crossings. It tears strand by strand as the spider works
loose.

Struggling no longer moves the body. The spider strains in bursts -- the web
pulls taut and flickers -- and each burst wears the pin down faster. A computer
spider always wants out; the player's spider struggles while a movement key is
held.
"""
from __future__ import annotations

import math

from ..support.math_utils import clamp
from .constants import (
    STRUGGLE_DRAIN,
    STRUGGLE_DUTY,
    STRUGGLE_PERIOD,
    WEBBED_SECONDS,
)

NET_SPOKES = 9
# Ring radii as fractions of the net's outer radius.
NET_RINGS = (0.28, 0.52, 0.76, 1.0)
NET_OUTER = 1.35        # outer ring, in body sizes
NET_SAG = 0.16          # how far a ring segment sags towards the hub


class WebNetMixin:
    """The net's look and the struggle against it."""

    def _pin_net(self) -> None:
        """Lay out a fresh web: spokes, rings, guy lines and where it tears."""
        rng = self.rng
        start = rng.uniform(0.0, math.tau)
        spokes = []
        for i in range(NET_SPOKES):
            angle = start + i * math.tau / NET_SPOKES + rng.uniform(-0.16, 0.16)
            rings = [fraction * rng.uniform(0.90, 1.08) for fraction in NET_RINGS]
            anchor = self.size * rng.uniform(1.75, 2.15)
            spokes.append({
                "angle": angle,
                "rings": rings,
                # World-space guy-line end: where the silk stuck to the ground.
                "anchor": (self.x + math.cos(angle) * anchor, self.y + math.sin(angle) * anchor),
                # Spokes and guy lines outlast the rings.
                "tear": rng.uniform(0.0, 0.55),
                "ring_tears": [rng.uniform(0.0, 1.0) for _ in NET_RINGS],
            })
        self.web_net = spokes
        self.web_anchors = [spoke["anchor"] for spoke in spokes]
        self.struggle_clock = 0.0

    def _wants_to_struggle(self) -> bool:
        control = self.player_control
        if control is not None:
            return bool(getattr(control, "struggling", False))
        return True

    def _update_web_struggle(self, dt: float) -> None:
        """Strain in bursts while webbed; a burst wears the silk down faster."""
        if not self.webbed or self.dead:
            self.struggle = max(0.0, self.struggle - dt * 4.0)
            if not self.webbed:
                self.web_anchors = []
                self.web_net = []
            return
        self.struggle_clock += dt
        bursting = (self.struggle_clock % STRUGGLE_PERIOD) < STRUGGLE_PERIOD * STRUGGLE_DUTY
        active = bursting and self._wants_to_struggle()
        target = 1.0 if active else 0.0
        self.struggle += (target - self.struggle) * (1.0 - math.exp(-dt * 14.0))
        if active:
            self.webbed_timer = max(0.0, self.webbed_timer - dt * STRUGGLE_DRAIN)

    def web_strength(self) -> float:
        """1 for a fresh net, falling to 0 as the spider gets loose."""
        return clamp(self.webbed_timer / WEBBED_SECONDS, 0.0, 1.0)

    def _net_points(self):
        """(spoke, [ring point...]) in world space, with the strain applied."""
        strain = 1.0 + 0.035 * self.struggle * math.sin(self.struggle_clock * 27.0)
        outer = self.size * NET_OUTER * strain
        cx, cy = self.x, self.y - self.jump_z
        for spoke in self.web_net:
            c, s = math.cos(spoke["angle"]), math.sin(spoke["angle"])
            yield spoke, [(cx + c * outer * r, cy + s * outer * r) for r in spoke["rings"]]

    def _draw_web_net(self, painter) -> None:
        if not self.webbed or self.dead or not self.web_net:
            return
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPainterPath, QPen

        strength = self.web_strength()
        shimmer = 0.5 + 0.5 * math.sin(self.struggle_clock * 27.0)
        alpha = int(clamp(110 + 130 * strength + 25 * self.struggle * shimmer, 0, 255))
        width = max(0.9, self.size * 0.030)
        cx, cy = self.x, self.y - self.jump_z
        spokes = list(self._net_points())
        count = len(spokes)

        threads = QPainterPath()
        # Spokes run from near the hub out to the ground.
        for spoke, points in spokes:
            if spoke["tear"] > strength:
                continue
            hub = points[0]
            threads.moveTo(QPointF(cx + (hub[0] - cx) * 0.25, cy + (hub[1] - cy) * 0.25))
            threads.lineTo(QPointF(*spoke["anchor"]))
        # Rings sag between neighbouring spokes, like a real orb web's spiral.
        dew = []
        for ring in range(len(NET_RINGS)):
            for i in range(count):
                spoke, points = spokes[i]
                _, next_points = spokes[(i + 1) % count]
                if spoke["ring_tears"][ring] > strength * 1.05:
                    continue
                a, b = points[ring], next_points[ring]
                mid_x, mid_y = (a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0
                control = (mid_x + (cx - mid_x) * NET_SAG, mid_y + (cy - mid_y) * NET_SAG)
                threads.moveTo(QPointF(*a))
                threads.quadTo(QPointF(*control), QPointF(*b))
                dew.append(a)

        # The wrap: silk bands across the body, in body space.
        wrap = QPainterPath()
        heading_c, heading_s = math.cos(self.heading), math.sin(self.heading)

        def body_point(forward, side):
            return QPointF(cx + heading_c * forward - heading_s * side,
                           cy + heading_s * forward + heading_c * side)

        bands = 2 + round(2 * strength)
        for i in range(bands):
            f = self.size * (-0.55 + 1.0 * (i + 0.5) / bands)
            lean = self.size * 0.18 * (1 if i % 2 else -1)
            wrap.moveTo(body_point(f - lean, -self.size * 0.55))
            wrap.quadTo(body_point(f, 0.0), body_point(f + lean, self.size * 0.55))

        painter.save()
        painter.setBrush(Qt.NoBrush)
        # A faint dark under-stroke so pale silk reads on a pale desktop.
        painter.setPen(QPen(QColor(20, 20, 26, int(alpha * 0.35)), width + 1.6,
                            Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.drawPath(threads)
        painter.drawPath(wrap)
        painter.setPen(QPen(QColor(240, 242, 246, alpha), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.drawPath(threads)
        painter.setPen(QPen(QColor(250, 250, 252, min(255, alpha + 20)), width * 1.6,
                            Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.drawPath(wrap)
        # Dew where rings cross spokes.
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(255, 255, 255, min(255, alpha + 15)))
        radius = max(0.8, width * 0.9)
        for x, y in dew:
            painter.drawEllipse(QPointF(x, y), radius, radius)
        painter.restore()
