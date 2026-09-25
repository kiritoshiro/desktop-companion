"""Silk on a webbed spider, and the spider fighting it.

The owner: *"when shooting the web at spider it should show on the spider the
net while it is active to indicate it is hit and immobilised, and the spider
should try to move out of it if he wants to be free."*

Being webbed (DC-45) was only a timer: stuck fast for WEBBED_HOLD_SECONDS,
then slowed until WEBBED_SECONDS, with nothing drawn. Now:

- a net is drawn over the body -- a wrap of crossing strands plus a few guy
  lines to where the silk hit the ground -- thickest while the spider is held
  fast, thinning and tearing as it works loose;
- the spider struggles in bursts, jerking against the silk, and each burst
  wears the web down faster. A computer spider always wants out; the player's
  spider struggles while a movement key is held.
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

# How many guy lines hold a fresh net to the ground.
NET_ANCHORS = 5


class WebNetMixin:
    """The net's look and the struggle against it."""

    def _pin_net(self) -> None:
        """Lay out a fresh net's guy lines where the silk hit."""
        start = self.rng.uniform(0.0, math.tau)
        self.web_anchors = []
        for i in range(NET_ANCHORS):
            angle = start + i * math.tau / NET_ANCHORS + self.rng.uniform(-0.35, 0.35)
            reach = self.size * self.rng.uniform(1.45, 1.95)
            self.web_anchors.append((self.x + math.cos(angle) * reach,
                                     self.y + math.sin(angle) * reach))
        self.struggle_clock = 0.0

    def _wants_to_struggle(self) -> bool:
        control = self.player_control
        if control is not None:
            return bool(getattr(control, "struggling", False))
        return True

    def _update_web_struggle(self, dt: float) -> None:
        """Struggle in bursts while webbed; a burst wears the silk down faster."""
        if not self.webbed or self.dead:
            self.struggle = max(0.0, self.struggle - dt * 4.0)
            if not self.webbed:
                self.web_anchors = []
            return
        self.struggle_clock += dt
        bursting = (self.struggle_clock % STRUGGLE_PERIOD) < STRUGGLE_PERIOD * STRUGGLE_DUTY
        active = bursting and self._wants_to_struggle()
        target = 1.0 if active else 0.0
        self.struggle += (target - self.struggle) * (1.0 - math.exp(-dt * 14.0))
        if active:
            self.webbed_timer = max(0.0, self.webbed_timer - dt * STRUGGLE_DRAIN)

    def web_struggle_offset(self) -> tuple:
        """The body's jerk against the silk, in pixels, sideways to its heading."""
        if self.struggle <= 1e-3:
            return 0.0, 0.0
        swing = math.sin(self.struggle_clock * 27.0) * self.struggle * self.size * 0.07
        return -math.sin(self.heading) * swing, math.cos(self.heading) * swing

    def web_strength(self) -> float:
        """1 for a fresh net, falling to 0 as the spider gets loose."""
        return clamp(self.webbed_timer / WEBBED_SECONDS, 0.0, 1.0)

    def _draw_web_net(self, painter) -> None:
        if not self.webbed or self.dead:
            return
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QPen

        strength = self.web_strength()
        ox, oy = self.combat_body_offset()
        cx, cy = self.x + ox, self.y + oy - self.jump_z
        alpha = int(70 + 170 * strength)
        width = max(1.0, self.size * (0.028 + 0.022 * strength))
        painter.save()
        painter.setBrush(Qt.NoBrush)
        # Guy lines tear one by one as the net loosens.
        kept = math.ceil(len(self.web_anchors) * strength ** 0.7)
        painter.setPen(QPen(QColor(236, 236, 230, int(alpha * 0.8)), width * 0.8,
                            Qt.SolidLine, Qt.RoundCap))
        for ax, ay in self.web_anchors[:kept]:
            dx, dy = ax - cx, ay - cy
            span = math.hypot(dx, dy) or 1.0
            edge = self.size * 0.55
            painter.drawLine(QPointF(cx + dx / span * edge, cy + dy / span * edge), QPointF(ax, ay))
        # The wrap: chords across an ellipse round the body, in body space.
        painter.translate(cx, cy)
        painter.rotate(math.degrees(self.heading))
        rx, ry = self.size * 0.95, self.size * 0.62
        strands = 4 + round(4 * strength)
        painter.setPen(QPen(QColor(245, 245, 240, alpha), width, Qt.SolidLine, Qt.RoundCap))
        for i in range(strands):
            a = i * math.tau / strands + 0.4
            b = a + 2.3
            painter.drawLine(QPointF(math.cos(a) * rx, math.sin(a) * ry),
                             QPointF(math.cos(b) * rx, math.sin(b) * ry))
        painter.setPen(QPen(QColor(245, 245, 240, int(alpha * 0.7)), width * 0.8,
                            Qt.DashLine, Qt.RoundCap))
        painter.drawEllipse(QPointF(0.0, 0.0), rx, ry)
        painter.restore()
