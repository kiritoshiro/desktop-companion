"""Painted emblems for the skill tree (the owner: "skill tree ... looks
unimpressive everywhere. something is lacking. ah maybe some image?").

Every skill gets a round brass medallion with its own picture -- a heart for
Vitality, a shell for Hardened carapace, fangs for Power strike, a web for Web
crafter -- and every branch of the tree a crest over its column. Locked skills
show the same picture dark and drained; learned ones glow. Painted once at
twice the size and cached.
"""
from __future__ import annotations

import math
from functools import lru_cache

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import (QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QPixmap,
                         QRadialGradient)

SCALE = 2
# Which branch each column of the tree is (character_ui.TREE_LAYOUT).
BRANCHES = (("body", "Body"), ("fangs", "Fangs"), ("legs", "Legs"), ("silk", "Silk"))
SKILL_BRANCH = {"vitality": "body", "carapace_harden": "body", "power_strike": "fangs",
                "apex_predator": "fangs", "quick_step": "legs", "long_stride": "legs",
                "silk_sense": "silk", "web_crafter": "silk", "silk_tracking": "silk"}
BRANCH_COLOURS = {"body": "#c0503c", "fangs": "#d89a2c", "legs": "#5e9c4c", "silk": "#6aa6c8"}

INK = QColor(40, 24, 12)
CREAM = QColor(250, 238, 210)


def _pen(color, width):
    return QPen(QColor(color), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)


# -- the pictures, drawn in a 64 x 64 box ---------------------------------------

def _heart(p):
    path = QPainterPath(QPointF(32, 48))
    path.cubicTo(12, 36, 14, 16, 26, 18)
    path.cubicTo(30, 18, 32, 22, 32, 24)
    path.cubicTo(32, 22, 34, 18, 38, 18)
    path.cubicTo(50, 16, 52, 36, 32, 48)
    p.setBrush(QColor("#d8453a"))
    p.setPen(_pen(INK, 2.2))
    p.drawPath(path)
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(255, 220, 210, 170))
    p.drawEllipse(QPointF(25, 25), 3.5, 2.5)


def _shell(p):
    path = QPainterPath(QPointF(32, 14))
    path.cubicTo(46, 16, 50, 30, 46, 40)
    path.cubicTo(42, 48, 36, 51, 32, 52)
    path.cubicTo(28, 51, 22, 48, 18, 40)
    path.cubicTo(14, 30, 18, 16, 32, 14)
    grad = QLinearGradient(20, 14, 44, 52)
    grad.setColorAt(0, QColor("#a07a4c"))
    grad.setColorAt(1, QColor("#4e3218"))
    p.setBrush(grad)
    p.setPen(_pen(INK, 2.2))
    p.drawPath(path)
    p.setPen(_pen("#e8c890", 1.6))
    p.drawLine(QPointF(32, 17), QPointF(32, 49))
    for y, w in ((26, 10), (34, 12), (42, 9)):
        p.drawLine(QPointF(32 - w, y - 3), QPointF(32, y))
        p.drawLine(QPointF(32 + w, y - 3), QPointF(32, y))


def _fangs(p):
    for side in (-1, 1):
        path = QPainterPath(QPointF(32 + side * 4, 16))
        path.cubicTo(32 + side * 16, 18, 32 + side * 16, 36, 32 + side * 6, 50)
        path.cubicTo(32 + side * 8, 38, 32 + side * 6, 28, 32 + side * 2, 24)
        path.closeSubpath()
        grad = QLinearGradient(32, 16, 32, 50)
        grad.setColorAt(0, QColor("#f4ead0"))
        grad.setColorAt(1, QColor("#b89868"))
        p.setBrush(grad)
        p.setPen(_pen(INK, 2.0))
        p.drawPath(path)
    p.setPen(Qt.NoPen)
    p.setBrush(QColor("#7ad04a"))
    p.drawEllipse(QPointF(26, 52), 2.2, 3.0)
    p.drawEllipse(QPointF(38, 52), 2.2, 3.0)


def _crown(p):
    path = QPainterPath(QPointF(14, 44))
    for x, y in ((14, 22), (22, 32), (32, 16), (42, 32), (50, 22), (50, 44)):
        path.lineTo(x, y)
    path.closeSubpath()
    grad = QLinearGradient(14, 16, 50, 44)
    grad.setColorAt(0, QColor("#ffe08a"))
    grad.setColorAt(1, QColor("#b07818"))
    p.setBrush(grad)
    p.setPen(_pen(INK, 2.2))
    p.drawPath(path)
    p.drawRect(QRectF(14, 44, 36, 6))
    p.setPen(Qt.NoPen)
    for x, colour in ((22, "#c83040"), (32, "#3a8ad8"), (42, "#c83040")):
        p.setBrush(QColor(colour))
        p.drawEllipse(QPointF(x, 39), 3, 3)


def _legs(p, stride=False):
    p.setPen(_pen(INK, 3.2))
    base = (30, 34)
    for i, (dx, dy) in enumerate(((-16, -10), (-18, 2), (-15, 13), (-8, 20))):
        knee = QPointF(base[0] + dx * 0.6, base[1] + dy - 8)
        p.drawLine(QPointF(*base), knee)
        p.drawLine(knee, QPointF(base[0] + dx, base[1] + dy))
    p.setBrush(QColor("#5a3a20"))
    p.drawEllipse(QPointF(34, 32), 8, 6)
    p.setPen(_pen("#f0d8a0", 2.0))
    lines = ((42, 22, 56, 22), (44, 30, 58, 30), (42, 38, 54, 38))
    for x0, y0, x1, y1 in lines:
        p.drawLine(QPointF(x0, y0), QPointF(x1, y1))
    if stride:
        p.setPen(QPen(QColor("#f0d8a0"), 1.8, Qt.DashLine, Qt.RoundCap))
        arc = QPainterPath(QPointF(12, 52))
        arc.quadTo(32, 4, 54, 50)
        p.drawPath(arc)


def _eye(p):
    path = QPainterPath(QPointF(10, 32))
    path.quadTo(32, 12, 54, 32)
    path.quadTo(32, 52, 10, 32)
    p.setBrush(CREAM)
    p.setPen(_pen(INK, 2.2))
    p.drawPath(path)
    glow = QRadialGradient(32, 32, 10)
    glow.setColorAt(0, QColor("#bff0ff"))
    glow.setColorAt(1, QColor("#2a78a8"))
    p.setBrush(glow)
    p.drawEllipse(QPointF(32, 32), 9, 9)
    p.setBrush(INK)
    p.drawEllipse(QPointF(32, 32), 3.5, 3.5)
    p.setPen(_pen("#e8f4ff", 1.2))
    for a in range(0, 360, 60):
        r = math.radians(a)
        p.drawLine(QPointF(32 + math.cos(r) * 13, 32 + math.sin(r) * 13 * 0.7),
                   QPointF(32 + math.cos(r) * 20, 32 + math.sin(r) * 20 * 0.7))


def _web(p, thread=False):
    c = QPointF(32, 32)
    p.setPen(_pen("#f4f0e4", 1.6))
    spokes = 8
    for i in range(spokes):
        a = i / spokes * math.tau
        p.drawLine(c, QPointF(32 + math.cos(a) * 22, 32 + math.sin(a) * 22))
    p.setPen(_pen("#e8e0cc", 1.2))
    for r in (7, 13, 19):
        poly = QPainterPath()
        for i in range(spokes + 1):
            a = i / spokes * math.tau
            pt = QPointF(32 + math.cos(a) * r, 32 + math.sin(a) * r)
            poly.moveTo(pt) if i == 0 else poly.lineTo(pt)
        p.drawPath(poly)
    if thread:
        p.setPen(QPen(QColor("#ffe890"), 2.2, Qt.SolidLine, Qt.RoundCap))
        path = QPainterPath(QPointF(32, 32))
        path.cubicTo(44, 36, 40, 50, 54, 52)
        p.drawPath(path)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor("#ffe890"))
        p.drawEllipse(QPointF(54, 52), 3.2, 3.2)
    p.setPen(Qt.NoPen)
    p.setBrush(INK)
    p.drawEllipse(c, 4, 4)


PICTURES = {
    "vitality": _heart, "carapace_harden": _shell, "power_strike": _fangs,
    "apex_predator": _crown, "quick_step": _legs, "long_stride": lambda p: _legs(p, True),
    "silk_sense": _eye, "web_crafter": _web, "silk_tracking": lambda p: _web(p, True),
}
BRANCH_PICTURES = {"body": _heart, "fangs": _fangs, "legs": _legs, "silk": _web}


def _medallion(p, rim, look):
    """The round plate a picture sits on."""
    face = QRadialGradient(28, 24, 34)
    if look == "locked":
        face.setColorAt(0, QColor(70, 56, 44))
        face.setColorAt(1, QColor(30, 22, 16))
    else:
        base = QColor(rim)
        face.setColorAt(0, base.lighter(115))
        face.setColorAt(1, base.darker(260))
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(0, 0, 0, 90))
    p.drawEllipse(QPointF(33, 34), 29, 29)
    p.setBrush(face)
    p.drawEllipse(QPointF(32, 32), 29, 29)
    ring = QLinearGradient(0, 0, 64, 64)
    if look == "locked":
        ring.setColorAt(0, QColor(110, 96, 80))
        ring.setColorAt(1, QColor(60, 50, 40))
    else:
        ring.setColorAt(0, QColor("#fbe3a0"))
        ring.setColorAt(1, QColor("#8a5a1c"))
    p.setBrush(Qt.NoBrush)
    p.setPen(QPen(ring, 3.4))
    p.drawEllipse(QPointF(32, 32), 28, 28)


def _render(picture, rim, look, size):
    image = QImage(size * SCALE, size * SCALE, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    p.scale(size * SCALE / 64.0, size * SCALE / 64.0)
    if look == "learned":
        glow = QRadialGradient(32, 32, 32)
        glow.setColorAt(0.7, QColor(255, 220, 120, 150))
        glow.setColorAt(1, QColor(255, 220, 120, 0))
        p.setPen(Qt.NoPen)
        p.setBrush(glow)
        p.drawEllipse(QPointF(32, 32), 32, 32)
    _medallion(p, rim, look)
    p.save()
    p.translate(32, 32)
    p.scale(0.78, 0.78)
    p.translate(-32, -32)
    picture(p)
    p.restore()
    p.end()
    if look == "locked":
        grey = image.convertToFormat(QImage.Format_Grayscale8).convertToFormat(QImage.Format_ARGB32_Premultiplied)
        alpha = image.convertToFormat(QImage.Format_Alpha8)
        grey.setAlphaChannel(alpha)
        p = QPainter(grey)
        p.setCompositionMode(QPainter.CompositionMode_SourceAtop)
        p.fillRect(grey.rect(), QColor(30, 20, 10, 120))
        p.end()
        image = grey
    pixmap = QPixmap.fromImage(image)
    pixmap.setDevicePixelRatio(SCALE)
    return pixmap


@lru_cache(maxsize=64)
def skill_icon(ability_id: str, look: str = "available", size: int = 46) -> QPixmap:
    """A skill's medallion. ``look``: "learned", "available" or "locked"."""
    branch = SKILL_BRANCH.get(ability_id, "body")
    picture = PICTURES.get(ability_id, BRANCH_PICTURES[branch])
    return _render(picture, BRANCH_COLOURS[branch], look, size)


@lru_cache(maxsize=16)
def branch_crest(branch: str, size: int = 34) -> QPixmap:
    """The crest over a branch's column."""
    return _render(BRANCH_PICTURES.get(branch, _heart), BRANCH_COLOURS.get(branch, "#c0503c"), "available", size)


# Small pictures for the inspector's stat tiles.
STAT_PICTURES = {"armor": _shell, "damage": _fangs, "points": _crown, "worn": _shell,
                 "speed": _legs, "silk": _web}


@lru_cache(maxsize=16)
def stat_icon(kind: str, size: int = 30) -> QPixmap:
    colours = {"armor": "#8a6a44", "damage": "#d89a2c", "points": "#c9a044", "worn": "#6a7a8c",
               "speed": "#5e9c4c", "silk": "#6aa6c8"}
    return _render(STAT_PICTURES.get(kind, _shell), colours.get(kind, "#8a6a44"), "available", size)
