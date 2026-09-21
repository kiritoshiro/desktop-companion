"""Draggable, resizable containment cages for the desktop overlay.

A :class:`Cage` is a plain geometric rectangle plus a little interaction logic.
It draws a soft fenced boundary whose open interior stays fully click-through,
so the real desktop underneath the cage remains usable.  Spiders that are
*placed inside* a cage are confined to it: their autonomous movement is clamped
to the interior, while the user can still pick one up and carry it out.

The class is intentionally framework-light.  It holds geometry and hit-testing
only; the manager owns the list of cages, decides membership, and asks the cage
to draw itself.  Qt types are imported lazily inside ``draw`` so the module can
be imported in headless contexts (tests) without a display.
"""
from __future__ import annotations

from typing import List, Optional, Tuple


def _clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


class Cage:
    # Half the side of the square corner grips, in pixels.
    HANDLE = 11.0
    # Thickness of the draggable border band used to move the whole cage.
    BORDER = 14.0
    MIN_W = 90.0
    MIN_H = 90.0

    __slots__ = ("x", "y", "w", "h")

    def __init__(self, x: float, y: float, w: float, h: float):
        self.x = float(x)
        self.y = float(y)
        self.w = max(self.MIN_W, float(w))
        self.h = max(self.MIN_H, float(h))

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------
    def to_dict(self) -> dict:
        """Serialise this cage for the runtime state file."""
        return {"x": self.x, "y": self.y, "w": self.w, "h": self.h}

    @classmethod
    def from_dict(cls, data) -> Optional["Cage"]:
        """Rebuild a saved cage, or return ``None`` if the entry is unusable."""
        if not isinstance(data, dict):
            return None
        try:
            return cls(float(data["x"]), float(data["y"]),
                       float(data["w"]), float(data["h"]))
        except (TypeError, ValueError, KeyError):
            return None

    # ------------------------------------------------------------------
    # Basic geometry
    # ------------------------------------------------------------------
    def right(self) -> float:
        return self.x + self.w

    def bottom(self) -> float:
        return self.y + self.h

    def center(self) -> Tuple[float, float]:
        return self.x + self.w * 0.5, self.y + self.h * 0.5

    def rect(self) -> Tuple[float, float, float, float]:
        """Outer rectangle as (x, y, w, h)."""
        return self.x, self.y, self.w, self.h

    def outer_bbox(self) -> Tuple[float, float, float, float]:
        """(x0, y0, x1, y1) including the corner grips, for repaint regions."""
        return (
            self.x - self.HANDLE,
            self.y - self.HANDLE,
            self.right() + self.HANDLE,
            self.bottom() + self.HANDLE,
        )

    def footprint_rect(self, margin: float = 6.0) -> Tuple[float, float, float, float]:
        """Full (x, y, w, h) covering everything the cage paints.

        Unlike :meth:`border_rects`, this includes the whole translucent interior
        wash. It is used when the cage moves or resizes so the old wash is fully
        cleared and cannot leave a faint rectangle ghosting behind at the previous
        position. The handle size plus a small margin also covers the soft glow
        stroke and the corner grips.
        """
        m = self.HANDLE + margin
        return (self.x - m, self.y - m, self.w + 2.0 * m, self.h + 2.0 * m)

    def contains_point(self, px: float, py: float) -> bool:
        return self.x <= px <= self.right() and self.y <= py <= self.bottom()

    def contains_center(self, px: float, py: float) -> bool:
        """Alias used for spider membership; identical to ``contains_point``."""
        return self.contains_point(px, py)

    def interior(self, inset: float) -> Tuple[float, float, float, float]:
        """Clamp bounds for a body centre, as (left, top, right, bottom).

        ``inset`` keeps the body (and most of its legs) inside the visible
        fence.  If the cage is smaller than twice the inset the interior
        collapses to the centre so a confined spider simply sits in the middle.
        """
        left = self.x + inset
        right = self.right() - inset
        top = self.y + inset
        bottom = self.bottom() - inset
        if right < left:
            cx = self.x + self.w * 0.5
            left = right = cx
        if bottom < top:
            cy = self.y + self.h * 0.5
            top = bottom = cy
        return left, top, right, bottom

    def clamp_center(self, px: float, py: float, inset: float) -> Tuple[float, float]:
        left, top, right, bottom = self.interior(inset)
        return _clamp(px, left, right), _clamp(py, top, bottom)

    # ------------------------------------------------------------------
    # Hit testing for direct manipulation
    # ------------------------------------------------------------------
    def corner_at(self, px: float, py: float) -> Optional[str]:
        """Return 'nw'/'ne'/'sw'/'se' if the point is on a corner grip."""
        h = self.HANDLE + 3.0
        corners = (
            ("nw", self.x, self.y),
            ("ne", self.right(), self.y),
            ("sw", self.x, self.bottom()),
            ("se", self.right(), self.bottom()),
        )
        for name, cx, cy in corners:
            if abs(px - cx) <= h and abs(py - cy) <= h:
                return name
        return None

    def on_border(self, px: float, py: float) -> bool:
        """True when the point sits on the draggable frame band.

        The open interior and the area well outside the cage both return False
        so clicks there fall through to the desktop or to a spider.
        """
        b = self.BORDER
        if not (self.x - b <= px <= self.right() + b and self.y - b <= py <= self.bottom() + b):
            return False
        inner_left = self.x + b
        inner_right = self.right() - b
        inner_top = self.y + b
        inner_bottom = self.bottom() - b
        inside_hole = inner_left < px < inner_right and inner_top < py < inner_bottom
        return not inside_hole

    # ------------------------------------------------------------------
    # Mutation
    # ------------------------------------------------------------------
    def move_to(self, x: float, y: float) -> None:
        self.x = float(x)
        self.y = float(y)

    def translate(self, dx: float, dy: float) -> None:
        self.x += dx
        self.y += dy

    def resize_corner(self, corner: str, px: float, py: float) -> None:
        """Drag ``corner`` to (px, py); the opposite corner stays fixed."""
        left = self.x
        top = self.y
        right = self.right()
        bottom = self.bottom()
        if "w" in corner:
            left = min(px, right - self.MIN_W)
        if "e" in corner:
            right = max(px, left + self.MIN_W)
        if "n" in corner:
            top = min(py, bottom - self.MIN_H)
        if "s" in corner:
            bottom = max(py, top + self.MIN_H)
        self.x = left
        self.y = top
        self.w = max(self.MIN_W, right - left)
        self.h = max(self.MIN_H, bottom - top)

    def clamp_to_screen(self, screen_w: float, screen_h: float, margin: float = 4.0) -> None:
        """Keep the cage from being dragged fully off the virtual desktop."""
        self.w = min(self.w, max(self.MIN_W, screen_w - margin * 2))
        self.h = min(self.h, max(self.MIN_H, screen_h - margin * 2))
        self.x = _clamp(self.x, margin, max(margin, screen_w - self.w - margin))
        self.y = _clamp(self.y, margin, max(margin, screen_h - self.h - margin))

    # ------------------------------------------------------------------
    # Repaint helpers
    # ------------------------------------------------------------------
    def border_rects(self) -> List[Tuple[float, float, float, float]]:
        """Thin (x, y, w, h) bands tracing the fence plus the corner grips.

        Returning the frame as separate bands (instead of one filled rect) keeps
        the dirty repaint region to the visible outline and leaves the big open
        interior untouched, which is what makes partial repainting cheap.
        """
        b = self.BORDER + 2.0
        hg = self.HANDLE + 4.0
        x0, y0, x1, y1 = self.outer_bbox()
        ow = x1 - x0
        rects: List[Tuple[float, float, float, float]] = [
            (x0, y0, ow, b + self.HANDLE),                       # top band
            (x0, y1 - b - self.HANDLE, ow, b + self.HANDLE),     # bottom band
            (x0, y0, b + self.HANDLE, y1 - y0),                  # left band
            (x1 - b - self.HANDLE, y0, b + self.HANDLE, y1 - y0),  # right band
        ]
        # Corner grips are already covered by the bands above; included margins
        # (hg) keep antialiased edges clean during a fast drag.
        for cx, cy in (
            (self.x, self.y),
            (self.right(), self.y),
            (self.x, self.bottom()),
            (self.right(), self.bottom()),
        ):
            rects.append((cx - hg, cy - hg, hg * 2.0, hg * 2.0))
        return rects

    # ------------------------------------------------------------------
    # Drawing
    # ------------------------------------------------------------------
    def draw(self, painter, member_count: int = 0, active: bool = False) -> None:
        """Render a soft terrarium-style fence with corner grips."""
        from PyQt5.QtCore import QRectF, Qt
        from PyQt5.QtGui import QColor, QPen, QBrush

        rect = QRectF(self.x, self.y, self.w, self.h)
        radius = min(22.0, self.w * 0.12, self.h * 0.12)

        # Faint interior wash so the enclosure reads as a space without hiding
        # whatever is on the desktop underneath it.
        wash = QColor(150, 205, 235, 26 if not active else 40)
        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(wash))
        painter.drawRoundedRect(rect, radius, radius)

        # Outer soft glow for the boundary, then a crisp inner stroke.
        glow = QColor(40, 120, 165, 70)
        painter.setBrush(Qt.NoBrush)
        painter.setPen(QPen(glow, 6.0, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.drawRoundedRect(rect, radius, radius)

        line = QColor(225, 245, 255, 235) if active else QColor(205, 232, 245, 205)
        painter.setPen(QPen(line, 2.2, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.drawRoundedRect(rect, radius, radius)

        # Corner grips signal that the cage is resizable.
        grip_fill = QColor(245, 252, 255, 240)
        grip_edge = QColor(40, 120, 165, 230)
        painter.setBrush(QBrush(grip_fill))
        painter.setPen(QPen(grip_edge, 1.6))
        g = self.HANDLE
        for cx, cy in (
            (self.x, self.y),
            (self.right(), self.y),
            (self.x, self.bottom()),
            (self.right(), self.bottom()),
        ):
            painter.drawRoundedRect(QRectF(cx - g * 0.5, cy - g * 0.5, g, g), 3.0, 3.0)

        if member_count > 0:
            tag = QColor(40, 120, 165, 220)
            painter.setPen(QPen(tag, 1.0))
            painter.setBrush(QBrush(QColor(245, 252, 255, 220)))
            label = f"\U0001F577 {member_count}" if member_count > 1 else "\U0001F577"
            from PyQt5.QtGui import QFont, QFontMetrics

            font = QFont()
            font.setPointSizeF(9.0)
            painter.setFont(font)
            fm = QFontMetrics(font)
            tw = fm.horizontalAdvance(label) + 12
            th = fm.height() + 4
            bx = self.x + 6.0
            by = self.y + 6.0
            painter.drawRoundedRect(QRectF(bx, by, tw, th), 5.0, 5.0)
            painter.setPen(QPen(QColor(20, 70, 100, 255)))
            painter.drawText(QRectF(bx, by, tw, th), Qt.AlignCenter, label)
