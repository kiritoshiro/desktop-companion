"""Floating damage numbers over a spider that has been hit.

The owner: *"indicate how much health was taken from the spiders."* The health
bar shrinks, but a bar says nothing about one blow. Every hit that lands now
puts a red "-N" beside the spider that rises and fades. Blows landing within
DAMAGE_MERGE_SECONDS of each other add up into one number rather than piling
up unreadably. The blow that kills a spider carries its number over to the
remains, because the spider itself is swept away at the end of that tick.
"""
from __future__ import annotations

from ..support.math_utils import clamp

DAMAGE_NUMBER_SECONDS = 1.1
DAMAGE_MERGE_SECONDS = 0.15
DAMAGE_RISE_PX = 30.0
# Room the numbers need beside and above the spider (bounding_rect).
DAMAGE_NUMBER_MARGIN = 56.0


class DamageNumbersMixin:
    """Record, age and draw the numbers."""

    def _note_damage(self, dealt: float) -> None:
        if dealt <= 0.0:
            return
        newest = self.damage_numbers[-1] if self.damage_numbers else None
        if newest is not None and newest[1] < DAMAGE_MERGE_SECONDS:
            newest[0] += dealt
            return
        # Alternate a little sideways so consecutive numbers do not overlap.
        nudge = 8.0 * (len(self.damage_numbers) % 3)
        self.damage_numbers.append([dealt, 0.0, nudge])

    def _update_damage_numbers(self, dt: float) -> None:
        if not self.damage_numbers:
            return
        for number in self.damage_numbers:
            number[1] += dt
        self.damage_numbers = [n for n in self.damage_numbers if n[1] < DAMAGE_NUMBER_SECONDS]

    def _damage_top(self) -> float:
        """Where the numbers start: the top of the drawn spider."""
        return min([self.y] + [leg.foot_y for leg in self.legs]) - self.jump_z

    @staticmethod
    def damage_text(amount: float) -> str:
        return f"-{amount:.0f}" if amount >= 1.0 else f"-{amount:.1f}"

    def _draw_damage_numbers(self, painter) -> None:
        if not self.damage_numbers:
            return
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QColor, QFont, QPainterPath, QPen

        font = QFont(painter.font())
        font.setBold(True)
        font.setPixelSize(int(clamp(self.size * 0.55, 12.0, 20.0)))
        top = self._damage_top()
        painter.save()
        for amount, age, nudge in self.damage_numbers:
            t = clamp(age / DAMAGE_NUMBER_SECONDS, 0.0, 1.0)
            alpha = int(255 * (1.0 - t * t))
            x = self.x + self.size * 0.8 + nudge
            y = top - DAMAGE_RISE_PX * t
            path = QPainterPath()
            path.addText(QPointF(x, y), font, self.damage_text(amount))
            # A dark outline so it reads on any desktop.
            painter.setPen(QPen(QColor(20, 10, 10, alpha), 3.0, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
            painter.setBrush(Qt.NoBrush)
            painter.drawPath(path)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(255, 86, 64, alpha))
            painter.drawPath(path)
        painter.restore()

    def _damage_number_bounds(self, bbox):
        """``bbox`` grown to cover the numbers, while any are showing."""
        if not self.damage_numbers:
            return bbox
        min_x, min_y, max_x, max_y = bbox
        top = self._damage_top()
        return (min_x, min(min_y, top - DAMAGE_RISE_PX - DAMAGE_NUMBER_MARGIN),
                max(max_x, self.x + self.size * 0.8 + DAMAGE_NUMBER_MARGIN * 2.0), max_y)
