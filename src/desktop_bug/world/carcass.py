"""What a beaten spider leaves behind, and how long it lasts.

DC-47. A spider that loses a fight dies: it is removed from the scene and
this is left in its place, curled up, until it has been eaten. Modelled on
`FlyRemains`, which already does the same job for a devoured fly -- a small
object the world owns, fading on the spot and reporting when it is done.

A carcass that a living spider is standing over goes faster and feeds that
spider's team, so "eaten" is something that happens rather than a word for
a fade-out.
"""

from __future__ import annotations

import math
from typing import Tuple

from ..creature.damage_numbers import DamageNumbersMixin

# How long a carcass lasts with nobody eating it, and how much faster it
# goes when somebody is. Short either way: the owner asked for it to
# disappear soon, and a desktop littered with bodies is not the tone.
CARCASS_LIFETIME = 16.0
EATEN_SPEED_UP = 3.2
# DC-50: before anything starts eating, the body simply lies there. The owner
# watched a colony and asked for exactly this -- "once dead he disapears too
# quikcly let it sit there upside down for a few seconds before vanishing" --
# because a death that is gone in a moment does not register as a death. A
# scavenger arriving during the wake starts the clock early rather than
# waiting it out.
CARCASS_SETTLE_SECONDS = 4.0
# How close a living spider must be, relative to the carcass's own size, to
# be eating it.
FEEDING_REACH = 1.9
# What a fully eaten spider is worth to the team that ate it, in the same
# banked food DC-21 spends on building.
CARCASS_FOOD_AMOUNT = 9.0


class Carcass(DamageNumbersMixin):
    """A dead spider's remains, consumed over a few seconds."""

    def __init__(self, x: float, y: float, size: float, team_id: str,
                 colors: dict | None = None) -> None:
        self.x = float(x)
        self.y = float(y)
        self.size = max(4.0, float(size))
        self.team_id = str(team_id or "neutral")
        self.colors = dict(colors or {})
        self.t = 0.0
        self.settle_t = 0.0
        self.being_eaten = False
        # The killing blow's number, carried over from the spider.
        self.damage_numbers = []
        # Legs fold under a dead spider rather than staying splayed; these are
        # fixed at death so the shape does not shimmer while it fades.
        self.legs = []
        for index in range(8):
            side = -1.0 if index % 2 else 1.0
            along = (index // 2) - 1.5
            self.legs.append((
                along * self.size * 0.26,
                side * self.size * 0.30,
                side * (0.7 + 0.25 * abs(along)),
            ))

    @property
    def settling(self) -> bool:
        """Still lying intact, before anything has begun on it."""
        return self.settle_t < CARCASS_SETTLE_SECONDS

    @property
    def spent(self) -> float:
        """How far through being eaten this carcass is, 0..1."""
        return max(0.0, min(1.0, self.t / CARCASS_LIFETIME))

    def update(self, dt: float, eaters: int = 0) -> bool:
        """Advance, returning True once there is nothing left.

        More than one spider feeding does not stack: a carcass surrounded by
        the whole colony should still take a moment, not vanish on the frame
        they arrive.
        """
        self.being_eaten = eaters > 0
        step = max(0.0, float(dt))
        self._update_damage_numbers(step)
        if self.settling and not self.being_eaten:
            # Lying there. The settle clock runs, the decay clock does not.
            self.settle_t += step
            return False
        rate = EATEN_SPEED_UP if self.being_eaten else 1.0
        self.settle_t = CARCASS_SETTLE_SECONDS
        self.t += step * rate
        return self.t >= CARCASS_LIFETIME

    def footprint(self) -> Tuple[float, float, float, float]:
        reach = self.size * 1.6
        x0, y0 = self.x - reach, self.y - reach
        x1, y1 = self.x + reach, self.y + reach
        x0, y0, x1, y1 = self._damage_number_bounds((x0, y0, x1, y1))
        return (x0, y0, x1 - x0, y1 - y0)

    def _damage_top(self) -> float:
        return self.y - self.size * 0.6

    def draw(self, painter) -> None:
        """Draw the remains: a shrinking, fading curl of legs and body."""
        from PyQt5.QtCore import QPointF, QRectF, Qt
        from PyQt5.QtGui import QColor, QPen

        left = 1.0 - self.spent
        if left <= 0.0:
            return
        alpha = int(225 * left)
        body = self.colors.get("body") or (132, 120, 136)
        # Drained of colour but not of contrast: a corpse should read as dead
        # without becoming invisible against a dark desktop, which the first
        # pass at 0.55 was -- checked by rendering it, not by reasoning.
        shade = QColor(int(body[0] * 0.78) + 26, int(body[1] * 0.78) + 24,
                       int(body[2] * 0.78) + 28, alpha)
        scale = self.size * (0.55 + 0.45 * left)

        painter.save()
        painter.setPen(QPen(shade, max(1.0, scale * 0.10), Qt.SolidLine, Qt.RoundCap))
        for along, across, curl in self.legs:
            # Curled inwards: a dead spider's legs fold under it.
            knee = (self.x + along * left, self.y + across * left)
            toe = (knee[0] + along * 0.5 * left, knee[1] + across * curl * 0.45 * left)
            painter.drawLine(QPointF(*knee), QPointF(*toe))
        painter.setPen(Qt.NoPen)
        painter.setBrush(shade)
        painter.drawEllipse(QRectF(self.x - scale * 0.36, self.y - scale * 0.28,
                                   scale * 0.72, scale * 0.56))
        painter.restore()
        self._draw_damage_numbers(painter)


def curled_size(creature) -> float:
    """The size a carcass takes from the spider it came from."""
    return max(4.0, float(getattr(creature, "size", 18.0)))


def carcass_for(creature) -> Carcass:
    """Build the remains of a spider that has just died."""
    colors = {}
    model_colors = getattr(creature, "colors", None)
    if isinstance(model_colors, dict):
        body = model_colors.get("body") or model_colors.get("abdomen")
        if isinstance(body, (list, tuple)) and len(body) >= 3:
            colors["body"] = tuple(int(c) for c in body[:3])
    carcass = Carcass(
        creature.x, creature.y, curled_size(creature),
        str(getattr(creature.progression, "team_id", "neutral")),
        colors,
    )
    carcass.damage_numbers = [list(n) for n in getattr(creature, "damage_numbers", [])]
    return carcass


def eaters_near(carcass: Carcass, creatures) -> int:
    """How many living spiders are close enough to be feeding on this."""
    reach = carcass.size * FEEDING_REACH
    count = 0
    for creature in creatures:
        if getattr(creature, "dead", False) or getattr(creature, "dragging", False):
            continue
        if math.hypot(creature.x - carcass.x, creature.y - carcass.y) <= reach:
            count += 1
    return count
