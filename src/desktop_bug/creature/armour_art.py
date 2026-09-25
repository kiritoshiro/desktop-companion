"""Armour pieces, painted with Qt.

The owner: *"create one armor set that would actually be logical anatomically
for tarantula. and when equipped would be visible in game. so create assets."*

One set of painters serves both places a piece is seen: the spider renderer
draws it on the moving spider, and the character window draws it on the
anatomy doll and on inventory tiles, so a piece looks the same everywhere.

Body pieces are painted in the body's own frame (x forward along the spider,
y across it), exactly where the renderer draws the carapace, abdomen and head.
Leg and palp pieces take the two ends of the segment they sit on.
"""
from __future__ import annotations

import math
from dataclasses import dataclass
from functools import lru_cache

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import (QBrush, QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen,
                         QRadialGradient)


@dataclass(frozen=True)
class Look:
    """How a piece is made: polished chitin ``plate``, woven ``silk`` or ``fluff``."""

    style: str
    base: str
    light: str
    dark: str


# Warden plates are burnished bronze chitin: warm enough to read as moulted
# exoskeleton, light enough to stand out on a black tarantula without hiding
# its orange carapace rim and red knees.
_WARDEN = Look("plate", "#a8742f", "#f4d58e", "#3a220c")
LOOKS = {
    "warden_crest": _WARDEN,
    "warden_carapace": _WARDEN,
    "warden_tergites": _WARDEN,
    "warden_greaves": _WARDEN,
    "warden_bracers": _WARDEN,
    "silk_carapace": Look("silk", "#e7dfcb", "#fffbf1", "#8d846f"),
    "fluffy_mantle": Look("fluff", "#d8cdbd", "#fff7ec", "#877c6c"),
    "leg_guard_set": Look("silk", "#e2d9c3", "#fffaf0", "#877e69"),
    "pedipalp_cuffs": Look("silk", "#d9c8a4", "#fff4dc", "#85745a"),
    "chelicerae_cap": Look("plate", "#6d7378", "#d9dee2", "#24272a"),
}
DEFAULT_LOOK = Look("silk", "#e7dfcb", "#fffbf1", "#8d846f")


def look_for(item_id: str) -> Look:
    return LOOKS.get(item_id, DEFAULT_LOOK)


def _c(value: str, alpha: int = 255) -> QColor:
    color = QColor(value)
    color.setAlpha(alpha)
    return color


def _pen(color: QColor, width: float, cap=Qt.RoundCap) -> QPen:
    return QPen(color, max(0.4, width), Qt.SolidLine, cap, Qt.RoundJoin)


def _silk_fill(p: QPainter, path: QPainterPath, look: Look, scale: float) -> None:
    """Pale woven silk: a translucent fill crossed by a fine weave."""
    bounds = path.boundingRect()
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(_c(look.base, 185)))
    p.drawPath(path)
    p.save()
    p.setClipPath(path)
    step = max(1.6, scale * 0.11)
    p.setPen(_pen(_c(look.light, 190), scale * 0.018))
    span = bounds.width() + bounds.height()
    x = bounds.left() - bounds.height()
    while x < bounds.right():
        p.drawLine(QPointF(x, bounds.top()), QPointF(x + bounds.height(), bounds.bottom()))
        p.drawLine(QPointF(x + bounds.height(), bounds.top()), QPointF(x, bounds.bottom()))
        x += step
        if x > bounds.left() + span * 2:
            break
    p.restore()
    p.setBrush(Qt.NoBrush)
    p.setPen(_pen(_c(look.dark, 200), scale * 0.028))
    p.drawPath(path)


def _fluff_fill(p: QPainter, rect: QRectF, look: Look, scale: float) -> None:
    """A soft setae mantle: a pale core ringed with tufts."""
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(_c(look.base, 200)))
    p.drawEllipse(rect)
    cx, cy = rect.center().x(), rect.center().y()
    rx, ry = rect.width() / 2, rect.height() / 2
    p.setBrush(QBrush(_c(look.light, 215)))
    for i in range(14):
        a = i * math.tau / 14
        p.drawEllipse(QPointF(cx + math.cos(a) * rx * 0.86, cy + math.sin(a) * ry * 0.86),
                      rx * 0.2, ry * 0.2)
    p.setBrush(QBrush(_c(look.dark, 90)))
    for i in range(7):
        a = i * math.tau / 7 + 0.3
        p.drawEllipse(QPointF(cx + math.cos(a) * rx * 0.42, cy + math.sin(a) * ry * 0.42),
                      rx * 0.08, ry * 0.08)


# -- body pieces (body frame) --------------------------------------------------

def paint_carapace(p: QPainter, cx: float, w: float, h: float, look: Look) -> None:
    """A plate over the prosoma, smaller than the shell so the rim still shows."""
    pw, ph = w * 0.80, h * 0.78
    rect = QRectF(cx - pw / 2, -ph / 2, pw, ph)
    p.save()
    if look.style == "plate":
        glow = QRadialGradient(QPointF(cx + pw * 0.14, -ph * 0.18), pw * 0.66)
        glow.setColorAt(0.0, _c(look.light))
        glow.setColorAt(0.45, _c(look.base))
        glow.setColorAt(1.0, _c(look.dark))
        p.setPen(_pen(_c(look.dark), w * 0.035))
        p.setBrush(QBrush(glow))
        p.drawEllipse(rect)
        # Striations radiate from the fovea, as they do on the shell beneath.
        fx = cx - pw * 0.06
        p.setPen(_pen(_c(look.dark, 150), w * 0.018))
        for i in range(8):
            a = (i + 0.5) * math.pi / 4
            p.drawLine(QPointF(fx + math.cos(a) * pw * 0.11, math.sin(a) * ph * 0.11),
                       QPointF(fx + math.cos(a) * pw * 0.39, math.sin(a) * ph * 0.39))
        # The fovea is left open, a dark pit with a bright lip.
        p.setPen(_pen(_c(look.light, 170), w * 0.014))
        p.setBrush(QBrush(_c(look.dark)))
        p.drawEllipse(QPointF(fx, 0.0), pw * 0.065, ph * 0.05)
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_c(look.light, 230)))
        for a in (0.62, 1.57, 2.52, -0.62, -1.57, -2.52):
            p.drawEllipse(QPointF(cx + math.cos(a) * pw * 0.43, math.sin(a) * ph * 0.42),
                          w * 0.024, w * 0.024)
        p.setBrush(Qt.NoBrush)
        p.setPen(_pen(_c(look.light, 170), w * 0.03))
        p.drawArc(rect.adjusted(pw * 0.07, ph * 0.07, -pw * 0.07, -ph * 0.07), 20 * 16, 80 * 16)
    elif look.style == "fluff":
        _fluff_fill(p, rect, look, w)
    else:
        path = QPainterPath()
        path.addEllipse(rect)
        _silk_fill(p, path, look, w)
    p.restore()


def paint_abdomen(p: QPainter, ax: float, w: float, h: float, wag: float, look: Look) -> None:
    """Overlapping tergite bands over the soft abdomen; the spinneret end is bare."""
    shell = QRectF(ax - w * 0.47, -h * 0.47 + wag, w * 0.94, h * 0.94)
    outline = QPainterPath()
    outline.addEllipse(shell)
    p.save()
    if look.style == "plate":
        p.setClipPath(outline)
        front, back = ax + w * 0.46, ax - w * 0.27
        count = 4
        step = (front - back) / count
        band_w = step * 1.32
        # Rearmost first: each band's front edge overlaps the one behind it,
        # the way a real abdomen's plates lie.
        for i in range(count):
            x0 = back + i * step
            band = QRectF(x0, -h * 0.6 + wag, band_w, h * 1.2)
            shade = QLinearGradient(band.left(), 0.0, band.right(), 0.0)
            shade.setColorAt(0.0, _c(look.dark))
            shade.setColorAt(0.35, _c(look.base))
            shade.setColorAt(0.78, _c(look.light))
            shade.setColorAt(1.0, _c(look.base))
            p.setPen(_pen(_c(look.dark), w * 0.024))
            p.setBrush(QBrush(shade))
            p.drawRoundedRect(band, band_w * 0.3, band_w * 0.3)
        p.setClipping(False)
        p.setPen(_pen(_c(look.light, 150), w * 0.02))
        p.drawLine(QPointF(back + step * 0.4, wag), QPointF(front - step * 0.2, wag))
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_c(look.light, 220)))
        for i in range(count):
            x = back + (i + 0.62) * step
            for side in (-1.0, 1.0):
                # Keep each rivet inside the shell outline.
                t = max(0.0, 1.0 - ((x - ax) / (w * 0.47)) ** 2)
                p.drawEllipse(QPointF(x, wag + side * h * 0.47 * math.sqrt(t) * 0.72),
                              w * 0.02, w * 0.02)
    elif look.style == "fluff":
        _fluff_fill(p, shell.adjusted(w * 0.05, h * 0.05, -w * 0.05, -h * 0.05), look, w)
    else:
        _silk_fill(p, outline, look, w)
    p.restore()


def paint_head(p: QPainter, hx: float, w: float, h: float, look: Look) -> None:
    """A crest over the eye mound with capped chelicerae; eyes are drawn after."""
    p.save()
    crest = QPainterPath(QPointF(hx - w * 0.95, 0.0))
    crest.quadTo(QPointF(hx - w * 0.35, -h * 0.5), QPointF(hx + w * 0.12, -h * 0.4))
    crest.lineTo(QPointF(hx + w * 0.46, -h * 0.14))
    crest.lineTo(QPointF(hx + w * 0.46, h * 0.14))
    crest.lineTo(QPointF(hx + w * 0.12, h * 0.4))
    crest.quadTo(QPointF(hx - w * 0.35, h * 0.5), QPointF(hx - w * 0.95, 0.0))
    if look.style == "plate":
        shade = QLinearGradient(hx, -h * 0.5, hx, h * 0.5)
        shade.setColorAt(0.0, _c(look.light))
        shade.setColorAt(0.5, _c(look.base))
        shade.setColorAt(1.0, _c(look.dark))
        p.setPen(_pen(_c(look.dark), w * 0.08))
        p.setBrush(QBrush(shade))
        p.drawPath(crest)
        p.setPen(_pen(_c(look.light, 200), w * 0.06))
        p.drawLine(QPointF(hx - w * 0.8, 0.0), QPointF(hx + w * 0.4, 0.0))
        cap_color = _c(look.base)
    else:
        _silk_fill(p, crest, look, w * 2.2)
        cap_color = _c(look.light, 230)
    p.setPen(_pen(_c(look.dark), w * 0.06))
    p.setBrush(QBrush(cap_color))
    for side in (-1.0, 1.0):
        p.drawEllipse(QPointF(hx + w * 0.56, side * h * 0.2), w * 0.14, h * 0.11)
    p.restore()


# -- limb pieces (segment ends) ------------------------------------------------

def _limb_plate(p: QPainter, x1: float, y1: float, x2: float, y2: float, width: float,
                look: Look, start: float, end: float, straps: int) -> None:
    dx, dy = x2 - x1, y2 - y1
    length = math.hypot(dx, dy)
    if length < 1e-3:
        return
    p.save()
    p.translate(x1, y1)
    p.rotate(math.degrees(math.atan2(dy, dx)))
    a, b = length * start, length * end
    hw = max(0.8, width * 0.68)
    if look.style == "plate":
        plate = QPainterPath(QPointF(a, -hw * 0.7))
        plate.lineTo(QPointF(a + length * 0.1, -hw))
        plate.lineTo(QPointF(b - length * 0.08, -hw * 0.9))
        plate.lineTo(QPointF(b, -hw * 0.55))
        plate.lineTo(QPointF(b, hw * 0.55))
        plate.lineTo(QPointF(b - length * 0.08, hw * 0.9))
        plate.lineTo(QPointF(a + length * 0.1, hw))
        plate.lineTo(QPointF(a, hw * 0.7))
        plate.closeSubpath()
        shade = QLinearGradient(0.0, -hw, 0.0, hw)
        shade.setColorAt(0.0, _c(look.light))
        shade.setColorAt(0.45, _c(look.base))
        shade.setColorAt(1.0, _c(look.dark))
        p.setPen(_pen(_c(look.dark), width * 0.14))
        p.setBrush(QBrush(shade))
        p.drawPath(plate)
        p.setPen(_pen(_c(look.light, 190), width * 0.12))
        p.drawLine(QPointF(a + length * 0.12, -hw * 0.25), QPointF(b - length * 0.1, -hw * 0.2))
        p.setPen(_pen(_c(look.dark, 210), width * 0.12, Qt.FlatCap))
        for i in range(straps):
            x = a + (b - a) * (i + 1) / (straps + 1)
            p.drawLine(QPointF(x, -hw * 0.95), QPointF(x, hw * 0.95))
    else:
        # Silk: wraps wound round the segment.
        p.setPen(_pen(_c(look.light, 215), width * 0.3, Qt.FlatCap))
        wraps = straps + 2
        for i in range(wraps):
            x = a + (b - a) * (i + 0.5) / wraps
            p.drawLine(QPointF(x - length * 0.02, -hw * 1.05), QPointF(x + length * 0.02, hw * 1.05))
    p.restore()


def paint_greave(p: QPainter, x1: float, y1: float, x2: float, y2: float, width: float,
                 look: Look) -> None:
    """A plate along the femur, from hip to just short of the (red) knee."""
    _limb_plate(p, x1, y1, x2, y2, width, look, 0.14, 0.84, 2)


def paint_bracer(p: QPainter, x1: float, y1: float, x2: float, y2: float, width: float,
                 look: Look) -> None:
    """A short bracer round a pedipalp segment."""
    _limb_plate(p, x1, y1, x2, y2, width, look, 0.18, 0.82, 1)


# -- icons ---------------------------------------------------------------------

_BODY = QColor(34, 28, 26)
_BAND = QColor(226, 110, 40)
_RIM = QColor(196, 120, 64)


def _limb(p: QPainter, points, width: float, band_index: int = 2) -> None:
    for i in range(len(points) - 1):
        color = _BAND if i == band_index else _BODY
        p.setPen(_pen(color, width * (1.0 - i * 0.12)))
        p.drawLine(QPointF(*points[i]), QPointF(*points[i + 1]))


@lru_cache(maxsize=64)
def armour_icon(item_id: str, slot: str, size: int = 64) -> QImage:
    """An inventory picture: the piece on a dark sketch of the body part it fits."""
    image = QImage(size, size, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    look = look_for(item_id)
    s = float(size)
    p.translate(s / 2, s / 2)
    p.rotate(-90.0)  # the spider's front points up
    if slot == "carapace":
        w, h = s * 0.78, s * 0.68
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_RIM))
        p.drawEllipse(QRectF(-w / 2, -h / 2, w, h))
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-w * 0.4, -h * 0.4, w * 0.8, h * 0.8))
        paint_carapace(p, 0.0, w, h, look)
    elif slot == "abdomen":
        w, h = s * 0.82, s * 0.66
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-w / 2, -h / 2, w, h))
        paint_abdomen(p, 0.0, w, h, 0.0, look)
    elif slot == "head":
        w, h = s * 0.34, s * 0.44
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-s * 0.36, -s * 0.3, s * 0.6, s * 0.6))
        p.drawEllipse(QRectF(-w / 2 + s * 0.08, -h / 2, w, h))
        paint_head(p, s * 0.08, w, h, look)
        p.setBrush(QBrush(QColor(90, 80, 70)))
        for ex, ey in ((0.0, -0.05), (0.0, 0.05), (0.04, -0.1), (0.04, 0.1)):
            p.drawEllipse(QPointF(s * (0.02 + ex), s * ey), s * 0.022, s * 0.022)
    elif slot == "legs":
        width = s * 0.11
        for offset in (-0.18, 0.18):
            pts = [(-s * 0.36, s * offset), (s * 0.02, s * (offset - 0.14)),
                   (s * 0.16, s * (offset - 0.1)), (s * 0.38, s * (offset + 0.06))]
            _limb(p, pts, width, band_index=2)
            paint_greave(p, pts[0][0], pts[0][1], pts[1][0], pts[1][1], width, look)
    else:  # pedipalps
        width = s * 0.1
        for side in (-1.0, 1.0):
            pts = [(-s * 0.34, side * s * 0.1), (-s * 0.1, side * s * 0.2),
                   (s * 0.14, side * s * 0.18), (s * 0.36, side * s * 0.08)]
            _limb(p, pts, width, band_index=0)
            paint_bracer(p, pts[1][0], pts[1][1], pts[2][0], pts[2][1], width, look)
    p.end()
    return image
