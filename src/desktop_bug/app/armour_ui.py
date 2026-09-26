"""The armour screen: a tarantula anatomy doll and a bag of pieces to drag onto it.

The owner: *"i want it to look like in those games where a whole person
anatomy is showed and on each part of it some armor type to be equipped. so
create an inventory with a list of items at bottom that could be dragged and
equipped on the anatomical drawing of spider."*

The doll is a top-down red-knee drawn to the tarantula model's proportions,
and every worn piece is painted on it with the same armour_art painters the
game uses, so what the doll shows is what the spider wears on the desktop.
Drop a piece anywhere on the doll and it goes to its part, which lights up
while it is dragged; drag a worn piece back to the bag, or double-click it,
to take it off.
"""
from __future__ import annotations

import math
import random

from PyQt5.QtCore import QMimeData, QPoint, QPointF, QRectF, QSize, Qt, pyqtSignal
from PyQt5.QtGui import (QBrush, QColor, QDrag, QFont, QPainter, QPainterPath,
                         QPainterPathStroker, QPen, QPixmap, QRadialGradient, QTransform)
from PyQt5.QtWidgets import (QApplication, QFrame, QGridLayout, QLabel, QScrollArea, QToolTip,
                             QVBoxLayout, QWidget)

from . import wood_theme
from .stat_text import SLOT_NAMES, TIER_COLORS, TIER_NAMES, item_tooltip
from ..creature import armour_art
from ..state.progression import ARMOR_BY_ID, ARMOR_TIERS

MIME = "application/x-desktop-companion-armour"
SLOT_ORDER = ("head", "pedipalps", "carapace", "abdomen", "legs")

# The tarantula model's own proportions (models/tarantula), in body sizes.
ABDOMEN = (-0.60, 1.06, 0.86)      # offset, length, width
CEPH = (0.22, 0.76, 0.66)
HEAD = (0.64, 0.26, 0.34)
# One side's legs: where each attaches round the carapace (degrees from the
# front), where it points, and a length factor. Mirrored for the other side.
LEGS = ((38.0, 32.0, 1.0), (72.0, 70.0, 0.92), (104.0, 112.0, 0.9), (134.0, 150.0, 1.04))
LEG_SEGMENTS = (0.62, 0.24, 0.48, 0.44, 0.28)       # femur, patella, tibia, metatarsus, tarsus
LEG_WIDTHS = (0.13, 0.12, 0.1, 0.08, 0.062)
PALP_SEGMENTS = (0.12, 0.22, 0.12, 0.2, 0.15)
PALP_WIDTHS = (0.09, 0.09, 0.085, 0.075, 0.06)

BODY = QColor(28, 22, 21)
BODY_LIGHT = QColor(70, 60, 54)
SETAE = QColor(120, 108, 96, 150)
BAND = QColor(226, 110, 40)
RIM = QColor(200, 124, 66)


def short_name(item) -> str:
    """"Sunforged palp gauntlets" -> "Palp gauntlets".

    A set piece's first word is its set, which its colours and the tooltip
    already say; without it the name fits on a plaque and a tile.
    """
    name = item.name.split(" ", 1)[1] if item.set_id and " " in item.name else item.name
    return name[:1].upper() + name[1:]


def _payload(event) -> str:
    return bytes(event.mimeData().data(MIME)).decode("utf-8", "replace")


def _start_drag(source: QWidget, text: str, icon) -> None:
    mime = QMimeData()
    mime.setData(MIME, text.encode("utf-8"))
    drag = QDrag(source)
    drag.setMimeData(mime)
    pixmap = QPixmap.fromImage(icon)
    drag.setPixmap(pixmap)
    drag.setHotSpot(QPoint(pixmap.width() // 2, pixmap.height() // 2))
    drag.exec_(Qt.MoveAction)


def _polar(x: float, y: float, angle_deg: float, length: float, side: float):
    a = math.radians(angle_deg)
    return x + math.cos(a) * length, y + side * math.sin(a) * length


class SpiderDoll(QWidget):
    """The anatomy doll. Emits ``equip(item_id)`` and ``unequip(slot)``."""

    equip = pyqtSignal(str)
    unequip = pyqtSignal(str)
    picked = pyqtSignal(str)

    PLAQUE_W, PLAQUE_H = 156, 54

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedSize(620, 440)
        self.setAcceptDrops(True)
        self.setMouseTracking(True)
        self.state = None
        self.drag_slot = None      # the part a dragged piece would go to
        self.hover_slot = None
        self._press = None
        self.unit = 80.0
        self.transform = QTransform()
        self.transform.translate(self.width() / 2, self.height() / 2 + 8)
        self.transform.rotate(-90.0)  # the spider's front points up
        self._build_geometry()

    # -- geometry ------------------------------------------------------------
    def _build_geometry(self) -> None:
        s = self.unit
        self.legs = []
        for side in (-1.0, 1.0):
            for attach, aim, scale in LEGS:
                a = math.radians(attach)
                root = (CEPH[0] * s + math.cos(a) * CEPH[1] * s * 0.36,
                        side * math.sin(a) * CEPH[2] * s * 0.40)
                femur_angle = aim * 0.55 + 90.0 * 0.45
                angles = (femur_angle, (femur_angle + aim) / 2, aim, aim + 6.0 * (aim - 90) / 60, aim)
                points = [root]
                for length, angle in zip(LEG_SEGMENTS, angles):
                    points.append(_polar(*points[-1], angle, length * s * scale, side))
                self.legs.append(points)
        self.palps = []
        for side in (-1.0, 1.0):
            points = [(0.54 * s, side * 0.13 * s)]
            for length, angle in zip(PALP_SEGMENTS, (60.0, 34.0, 18.0, 8.0, -4.0)):
                points.append(_polar(*points[-1], angle, length * s, side))
            self.palps.append(points)
        rng = random.Random(7)
        self.hairs = [(rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-0.6, 0.6))
                      for _ in range(150)]
        # Where each part is, for hovering and dropping (widget coordinates).
        zones = {}
        zones["abdomen"] = self._ellipse(ABDOMEN[0] * s, 0.0, ABDOMEN[1] * s, ABDOMEN[2] * s)
        zones["carapace"] = self._ellipse(CEPH[0] * s, 0.0, CEPH[1] * s, CEPH[2] * s)
        zones["head"] = self._ellipse(0.7 * s, 0.0, 0.4 * s, 0.42 * s)
        zones["legs"] = self._strokes(self.legs, 0.2 * s)
        zones["pedipalps"] = self._strokes(self.palps, 0.18 * s)
        self.zones = {slot: self.transform.map(path) for slot, path in zones.items()}
        anchors = {
            "head": (0.72 * s, 0.0), "pedipalps": self.palps[1][3],
            "carapace": (CEPH[0] * s, 0.18 * s), "abdomen": (ABDOMEN[0] * s - 0.1 * s, 0.22 * s),
            "legs": self.legs[1][2],
        }
        self.anchors = {slot: self.transform.map(QPointF(*xy)) for slot, xy in anchors.items()}
        w, h, pw, ph = self.width(), self.height(), self.PLAQUE_W, self.PLAQUE_H
        self.plaques = {
            "head": QRectF(8, 10, pw, ph),
            "pedipalps": QRectF(w - pw - 8, 10, pw, ph),
            "carapace": QRectF(w - pw - 8, h * 0.44, pw, ph),
            "abdomen": QRectF(w - pw - 8, h - ph - 10, pw, ph),
            "legs": QRectF(8, h * 0.5, pw, ph),
        }

    @staticmethod
    def _ellipse(cx, cy, length, width) -> QPainterPath:
        path = QPainterPath()
        path.addEllipse(QPointF(cx, cy), length / 2, width / 2)
        return path

    @staticmethod
    def _strokes(chains, width) -> QPainterPath:
        path = QPainterPath()
        for chain in chains:
            path.moveTo(QPointF(*chain[0]))
            for point in chain[1:]:
                path.lineTo(QPointF(*point))
        stroker = QPainterPathStroker()
        stroker.setWidth(width)
        stroker.setCapStyle(Qt.RoundCap)
        return stroker.createStroke(path)

    def slot_at(self, pos) -> str | None:
        point = QPointF(pos)
        for slot in SLOT_ORDER:
            if self.plaques[slot].contains(point) or self.zones[slot].contains(point):
                return slot
        return None

    def show_state(self, state) -> None:
        self.state = state
        self.update()

    def _worn(self, slot):
        if self.state is None:
            return None
        return ARMOR_BY_ID.get(self.state.equipped.get(slot, ""))

    # -- painting ------------------------------------------------------------
    def paintEvent(self, event):  # noqa: N802 - Qt API name
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing, True)
        self._paint_stage(p)
        p.save()
        p.setTransform(self.transform, True)
        self._paint_spider(p)
        p.restore()
        self._paint_highlight(p)
        self._paint_callouts(p)

    def _paint_stage(self, p: QPainter) -> None:
        glow = QRadialGradient(QPointF(self.width() / 2, self.height() / 2), self.width() * 0.46)
        glow.setColorAt(0.0, QColor(96, 70, 40, 150))
        glow.setColorAt(1.0, QColor(20, 12, 6, 0))
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(glow))
        p.drawRect(self.rect())
        # A faint orb web behind the doll, like a specimen pinned in silk.
        cx, cy = self.width() / 2, self.height() / 2 + 8
        p.setPen(QPen(QColor(246, 226, 184, 30), 1.0))
        for i in range(12):
            a = i * math.tau / 12
            p.drawLine(QPointF(cx, cy), QPointF(cx + math.cos(a) * 205, cy + math.sin(a) * 205))
        for r in range(40, 210, 28):
            ring = QPainterPath()
            for i in range(13):
                a = i * math.tau / 12
                pt = QPointF(cx + math.cos(a) * r, cy + math.sin(a) * r)
                ring.moveTo(pt) if i == 0 else ring.lineTo(pt)
            p.drawPath(ring)

    def _limb(self, p: QPainter, points, widths, band_index: int) -> None:
        s = self.unit
        for i in range(len(points) - 1):
            (x1, y1), (x2, y2) = points[i], points[i + 1]
            width = widths[i] * s
            p.setPen(QPen(SETAE, width * 1.35, Qt.SolidLine, Qt.RoundCap))
            p.drawLine(QPointF(x1, y1), QPointF(x2, y2))
            p.setPen(QPen(BAND if i == band_index else BODY, width, Qt.SolidLine, Qt.RoundCap))
            p.drawLine(QPointF(x1, y1), QPointF(x2, y2))
            if i in (2, 3):  # the pale rings at the far joints of a red-knee
                p.setPen(QPen(QColor(226, 150, 90, 170), width * 0.9, Qt.SolidLine, Qt.FlatCap))
                p.drawLine(QPointF(x1 + (x2 - x1) * 0.02, y1 + (y2 - y1) * 0.02),
                           QPointF(x1 + (x2 - x1) * 0.1, y1 + (y2 - y1) * 0.1))
            # Setae standing off the segment.
            dx, dy = x2 - x1, y2 - y1
            length = math.hypot(dx, dy) or 1.0
            nx, ny = -dy / length, dx / length
            p.setPen(QPen(SETAE, 1.0))
            for k in range(1, 5):
                t = k / 5
                bx, by = x1 + dx * t, y1 + dy * t
                for sign in (-1.0, 1.0):
                    p.drawLine(QPointF(bx + nx * sign * width * 0.45, by + ny * sign * width * 0.45),
                               QPointF(bx + nx * sign * width * 0.9 + dx / length * width * 0.3,
                                       by + ny * sign * width * 0.9 + dy / length * width * 0.3))
        tx, ty = points[-1]
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(BODY))
        p.drawEllipse(QPointF(tx, ty), widths[-1] * s * 0.6, widths[-1] * s * 0.6)

    def _paint_spider(self, p: QPainter) -> None:
        s = self.unit
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(QColor(0, 0, 0, 60)))
        p.drawEllipse(QPointF(-0.2 * s + 6, 6), 1.1 * s, 0.7 * s)
        for points in self.legs:
            self._limb(p, points, LEG_WIDTHS, band_index=1)
        legs = self._worn("legs")
        if legs is not None:
            look = armour_art.look_for(legs.id)
            widths = [width * s for width in LEG_WIDTHS]
            for points in self.legs:
                armour_art.paint_leg_armour(p, points, widths, look)
        for points in self.palps:
            self._limb(p, points, PALP_WIDTHS, band_index=1)
        palps = self._worn("pedipalps")
        if palps is not None:
            look = armour_art.look_for(palps.id)
            for points in self.palps:
                (x0, y0), (x1, y1) = points[2], points[3]
                armour_art.paint_bracer(p, x0, y0, x1, y1, PALP_WIDTHS[2] * s, look)
        # Abdomen: black, densely haired.
        ax, aw, ah = ABDOMEN[0] * s, ABDOMEN[1] * s, ABDOMEN[2] * s
        shade = QRadialGradient(QPointF(ax + aw * 0.12, -ah * 0.15), aw * 0.6)
        shade.setColorAt(0.0, BODY_LIGHT)
        shade.setColorAt(1.0, BODY)
        p.setPen(QPen(QColor(12, 10, 10), 1.5))
        p.setBrush(QBrush(shade))
        p.drawEllipse(QPointF(ax, 0.0), aw / 2, ah / 2)
        p.setPen(QPen(SETAE, 1.0))
        for hx, hy, tilt in self.hairs:
            if hx * hx + hy * hy > 0.9:
                continue
            x, y = ax + hx * aw * 0.5, hy * ah * 0.5
            p.drawLine(QPointF(x, y), QPointF(x - aw * 0.05, y + tilt * ah * 0.04))
        # Pedicel and carapace, with the red-knee's orange rim round the black patch.
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(BODY))
        p.drawEllipse(QPointF(-0.06 * s, 0.0), 0.09 * s, 0.06 * s)
        cx, cw, ch = CEPH[0] * s, CEPH[1] * s, CEPH[2] * s
        p.setBrush(QBrush(RIM))
        p.drawEllipse(QPointF(cx, 0.0), cw / 2, ch / 2)
        patch = QRadialGradient(QPointF(cx + cw * 0.1, -ch * 0.1), cw * 0.45)
        patch.setColorAt(0.0, BODY_LIGHT)
        patch.setColorAt(1.0, BODY)
        p.setBrush(QBrush(patch))
        p.drawEllipse(QPointF(cx, 0.0), cw / 2 - 0.1 * s, ch / 2 - 0.1 * s)
        p.setPen(QPen(QColor(90, 76, 66, 160), 1.2))
        for i in range(8):
            a = (i + 0.5) * math.pi / 4
            p.drawLine(QPointF(cx - cw * 0.05 + math.cos(a) * cw * 0.08, math.sin(a) * ch * 0.08),
                       QPointF(cx - cw * 0.05 + math.cos(a) * cw * 0.26, math.sin(a) * ch * 0.26))
        # Head lobe and the chelicerae in front of it.
        hx, hw, hh = HEAD[0] * s, HEAD[1] * s, HEAD[2] * s
        p.setPen(QPen(QColor(12, 10, 10), 1.2))
        p.setBrush(QBrush(BODY))
        for side in (-1.0, 1.0):
            p.drawEllipse(QPointF(hx + hw * 0.5, side * hh * 0.2), hw * 0.3, hh * 0.22)
        p.drawEllipse(QPointF(hx, 0.0), hw / 2, hh / 2)
        for slot in ("abdomen", "carapace", "head"):
            item = self._worn(slot)
            if item is None:
                continue
            look = armour_art.look_for(item.id)
            if slot == "abdomen":
                armour_art.paint_abdomen(p, ax, aw, ah, 0.0, look)
            elif slot == "carapace":
                armour_art.paint_carapace(p, cx, cw, ch, look)
            else:
                armour_art.paint_head(p, hx, hw, hh, look)
        # Eight eyes on the ocular mound, drawn last so every visor leaves them clear.
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(QColor(10, 8, 8)))
        p.drawEllipse(QPointF(hx - hw * 0.1, 0.0), hw * 0.34, hh * 0.3)
        p.setBrush(QBrush(QColor(150, 140, 128)))
        for ex, ey, r in ((0.02, -0.09, 0.05), (0.02, 0.09, 0.05), (-0.05, -0.04, 0.035),
                          (-0.05, 0.04, 0.035), (0.08, -0.15, 0.03), (0.08, 0.15, 0.03),
                          (-0.1, -0.14, 0.028), (-0.1, 0.14, 0.028)):
            p.drawEllipse(QPointF(hx - hw * 0.1 + ex * s, ey * s), r * s * 0.6, r * s * 0.6)

    def _paint_highlight(self, p: QPainter) -> None:
        for slot, colour in ((self.hover_slot, QColor(246, 226, 184, 60)),
                             (self.drag_slot, QColor(214, 164, 72, 110))):
            if slot is None:
                continue
            p.setPen(QPen(QColor(wood_theme.BRASS), 2.0, Qt.DashLine))
            p.setBrush(QBrush(colour))
            p.drawPath(self.zones[slot])

    def _paint_callouts(self, p: QPainter) -> None:
        title = QFont(wood_theme.UI_FONT)
        title.setPixelSize(11)
        title.setBold(True)
        body = QFont(wood_theme.UI_FONT)
        body.setPixelSize(12)
        body.setBold(True)
        for slot in SLOT_ORDER:
            rect = self.plaques[slot]
            item = self._worn(slot)
            lit = slot in (self.drag_slot, self.hover_slot)
            anchor = self.anchors[slot]
            edge = QPointF(rect.right() if rect.center().x() < self.width() / 2 else rect.left(),
                           rect.center().y())
            p.setPen(QPen(QColor(wood_theme.BRASS if lit or item else wood_theme.INK_SOFT), 1.4))
            p.drawLine(edge, anchor)
            p.setBrush(QBrush(QColor(wood_theme.BRASS)))
            p.drawEllipse(anchor, 3.0, 3.0)
            p.setBrush(QBrush(QColor(42, 24, 12, 235)))
            border = QColor(TIER_COLORS.get(item.tier) if item else
                            (wood_theme.BRASS if lit else wood_theme.BRASS_DEEP))
            p.setPen(QPen(border, 2.0 if lit else 1.2))
            p.drawRoundedRect(rect, 8, 8)
            socket = QRectF(rect.left() + 6, rect.top() + 6, rect.height() - 12, rect.height() - 12)
            p.setBrush(QBrush(QColor(20, 12, 6, 220)))
            p.setPen(QPen(QColor(wood_theme.INK_SOFT), 1.0, Qt.SolidLine if item else Qt.DashLine))
            p.drawRoundedRect(socket, 6, 6)
            if item is not None:
                icon = armour_art.armour_icon(item.id, item.slot, 64)
                p.drawImage(socket, icon)
            p.setFont(title)
            p.setPen(QColor(wood_theme.BRASS))
            text_x = socket.right() + 8
            p.drawText(QRectF(text_x, rect.top() + 6, rect.right() - text_x - 4, 14),
                       Qt.AlignLeft | Qt.AlignVCenter, SLOT_NAMES[slot])
            p.setFont(body)
            p.setPen(QColor(wood_theme.CREAM if item else wood_theme.CREAM_SOFT))
            p.drawText(QRectF(text_x, rect.top() + 20, rect.right() - text_x - 4, rect.height() - 24),
                       Qt.AlignLeft | Qt.AlignTop | Qt.TextWordWrap,
                       short_name(item) if item else "Drag armour here")

    # -- mouse and drops ------------------------------------------------------
    def event(self, event):  # noqa: D401 - Qt override
        if event.type() == event.ToolTip:
            slot = self.slot_at(event.pos())
            item = self._worn(slot) if slot else None
            if item is not None:
                QToolTip.showText(event.globalPos(),
                                  item_tooltip(item, self.state, self.state.item_levels.get(item.id, 1)) +
                                  "<p><i>Drag to the bag or double-click to take off</i></p>", self)
            elif slot is not None:
                QToolTip.showText(event.globalPos(),
                                  f"<b>{SLOT_NAMES[slot]}</b><br>Nothing worn. Drag a piece here.", self)
            else:
                QToolTip.hideText()
            return True
        return super().event(event)

    def mouseMoveEvent(self, event):  # noqa: N802
        slot = self.slot_at(event.pos())
        if slot != self.hover_slot:
            self.hover_slot = slot
            self.setCursor(Qt.OpenHandCursor if self._worn(slot) else Qt.ArrowCursor)
            self.update()
        if (self._press is not None and event.buttons() & Qt.LeftButton
                and (event.pos() - self._press[0]).manhattanLength() >= QApplication.startDragDistance()):
            slot = self._press[1]
            self._press = None
            item = self._worn(slot)
            if item is not None:
                _start_drag(self, "worn:" + slot, armour_art.armour_icon(item.id, item.slot, 64))

    def mousePressEvent(self, event):  # noqa: N802
        if event.button() == Qt.LeftButton:
            slot = self.slot_at(event.pos())
            self._press = (event.pos(), slot) if self._worn(slot) else None

    def mouseReleaseEvent(self, event):  # noqa: N802
        if self._press is not None:
            # A click without a drag picks the worn piece for the item panel.
            item = self._worn(self._press[1])
            if item is not None:
                self.picked.emit(item.id)
        self._press = None

    def mouseDoubleClickEvent(self, event):  # noqa: N802
        slot = self.slot_at(event.pos())
        if self._worn(slot):
            self.unequip.emit(slot)

    def leaveEvent(self, event):  # noqa: N802
        self.hover_slot = None
        self.update()

    def dragEnterEvent(self, event):  # noqa: N802
        item = ARMOR_BY_ID.get(_payload(event)) if event.mimeData().hasFormat(MIME) else None
        if item is None:
            event.ignore()
            return
        self.drag_slot = item.slot
        self.update()
        event.acceptProposedAction()

    def dragMoveEvent(self, event):  # noqa: N802
        if self.drag_slot is not None:
            event.acceptProposedAction()

    def dragLeaveEvent(self, event):  # noqa: N802
        self.drag_slot = None
        self.update()

    def dropEvent(self, event):  # noqa: N802
        item = ARMOR_BY_ID.get(_payload(event))
        self.drag_slot = None
        self.update()
        if item is not None:
            event.acceptProposedAction()
            self.equip.emit(item.id)


class ItemTile(QFrame):
    """One piece in the bag: its picture and name. Drag it onto the doll."""

    equip = pyqtSignal(str)
    picked = pyqtSignal(str)
    HEIGHT = 124

    def __init__(self, item, state, spares: int = 0, parent=None):
        super().__init__(parent)
        self.item = item
        self.setObjectName("itemTile")
        self.setProperty("tier", item.tier)
        self.setFixedSize(100, self.HEIGHT)
        self.setCursor(Qt.OpenHandCursor)
        level = state.item_levels.get(item.id, 1)
        self.setToolTip(item_tooltip(item, state, level) +
                        "<p><i>Drag onto the spider, or double-click. Click for upgrade and sell.</i></p>")
        self._press = None
        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 6, 6, 4)
        layout.setSpacing(2)
        picture = QLabel()
        picture.setAlignment(Qt.AlignCenter)
        picture.setPixmap(QPixmap.fromImage(armour_art.armour_icon(item.id, item.slot, 64)))
        layout.addWidget(picture)
        # Level and spares ride on the picture's corners.
        self.level_badge = QLabel(f"Lv {level}", picture)
        self.level_badge.setObjectName("tileBadge")
        self.level_badge.move(0, 0)
        self.level_badge.setVisible(level > 1)
        self.spare_badge = QLabel(f"+{spares}", picture)
        self.spare_badge.setObjectName("tileBadge")
        self.spare_badge.setToolTip(f"{spares} spare{'s' if spares != 1 else ''}: stack to upgrade, or sell")
        self.spare_badge.move(60, 0)
        self.spare_badge.setVisible(spares > 0)
        name = QLabel(short_name(item))
        name.setObjectName("tileName")
        name.setAlignment(Qt.AlignCenter)
        name.setWordWrap(True)
        layout.addWidget(name)
        tier = QLabel(TIER_NAMES.get(item.tier, item.tier))
        tier.setObjectName("tileTier")
        tier.setAlignment(Qt.AlignCenter)
        tier.setStyleSheet(f"color: {TIER_COLORS.get(item.tier, '#cfc8b8')};")
        layout.addWidget(tier)

    def mousePressEvent(self, event):  # noqa: N802
        if event.button() == Qt.LeftButton:
            self._press = event.pos()

    def mouseMoveEvent(self, event):  # noqa: N802
        if (self._press is not None and event.buttons() & Qt.LeftButton
                and (event.pos() - self._press).manhattanLength() >= QApplication.startDragDistance()):
            self._press = None
            _start_drag(self, self.item.id, armour_art.armour_icon(self.item.id, self.item.slot, 64))

    def mouseReleaseEvent(self, event):  # noqa: N802
        if self._press is not None:
            self.picked.emit(self.item.id)
        self._press = None

    def mouseDoubleClickEvent(self, event):  # noqa: N802
        self.equip.emit(self.item.id)


class InventoryBag(QScrollArea):
    """The pieces the hero owns and is not wearing, best first, in a grid.

    A worn piece dropped here comes off.
    """

    COLUMNS = 5

    equip = pyqtSignal(str)
    unequip = pyqtSignal(str)
    picked = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("inventoryBag")
        self.setAcceptDrops(True)
        self.setWidgetResizable(True)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self.setFixedWidth(self.COLUMNS * 108 + 34)
        self.inner = QWidget()
        self.inner.setObjectName("bagInner")
        self.row = QGridLayout(self.inner)
        self.row.setContentsMargins(8, 8, 8, 8)
        self.row.setSpacing(8)
        self.row.setAlignment(Qt.AlignTop | Qt.AlignLeft)
        self.setWidget(self.inner)
        self.tiles = {}

    def show_state(self, state, worn=None, spares=None) -> None:
        """Tiles for owned pieces nobody wears. ``worn``: pieces worn by any
        spider (default: this one's); ``spares``: item id -> spare count."""
        spares = spares or {}
        while self.row.count():
            entry = self.row.takeAt(0)
            if entry.widget() is not None:
                # Off the screen now, not at the next event-loop pass: a worn
                # piece must leave the bag the moment it is put on.
                entry.widget().setParent(None)
                entry.widget().deleteLater()
        self.tiles = {}
        worn = set(state.equipped.values()) if worn is None else set(worn)
        owned = [ARMOR_BY_ID[i] for i in state.inventory if i in ARMOR_BY_ID and i not in worn]
        owned.sort(key=lambda item: (-ARMOR_TIERS.index(item.tier), item.set_id,
                                     SLOT_ORDER.index(item.slot)))
        for index, item in enumerate(owned):
            tile = ItemTile(item, state, spares.get(item.id, 0))
            tile.equip.connect(self.equip)
            tile.picked.connect(self.picked)
            self.row.addWidget(tile, index // self.COLUMNS, index % self.COLUMNS)
            self.tiles[item.id] = tile
        if not self.tiles:
            empty = QLabel("Everything you own is worn. Drag a piece off the spider to put it back.")
            empty.setObjectName("statLine")
            empty.setWordWrap(True)
            self.row.addWidget(empty, 0, 0, 1, self.COLUMNS)
        # Tiles have a fixed size; the grid must be given room for every row or
        # the scroll area squeezes the rows over one another.
        rows = max(1, -(-len(self.tiles) // self.COLUMNS))
        self.inner.setMinimumHeight(rows * (ItemTile.HEIGHT + self.row.spacing()) + 16)

    def sizeHint(self):  # noqa: N802
        return QSize(self.COLUMNS * 108 + 34, 440)

    def dragEnterEvent(self, event):  # noqa: N802
        if event.mimeData().hasFormat(MIME) and _payload(event).startswith("worn:"):
            event.acceptProposedAction()
        else:
            event.ignore()

    def dragMoveEvent(self, event):  # noqa: N802
        event.acceptProposedAction()

    def dropEvent(self, event):  # noqa: N802
        text = _payload(event)
        if text.startswith("worn:"):
            event.acceptProposedAction()
            self.unequip.emit(text.split(":", 1)[1])
