"""Armour pieces, painted with Qt.

The owner: *"create one armor set that would actually be logical anatomically
for tarantula. and when equipped would be visible in game. so create assets."*
and then *"on the whole leg an armor would be nice. also make some more models
of armors on various tier and quality material, looking epic some."*

One set of painters serves both places a piece is seen: the spider renderer
draws it on the moving spider, and the character window draws it on the
anatomy doll and on inventory tiles, so a piece looks the same everywhere.

Body pieces are painted in the body's own frame (x forward along the spider,
y across it), exactly where the renderer draws the carapace, abdomen and head.
Leg and palp pieces take the joints of the limb they sit on.

A material is a ``Look``: its colours plus what makes it special -- a trim on
the edges, glowing veins or runes, a set gem, an aura, or crystal that is
see-through and faceted.
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
    """How a piece is made: polished ``plate``, woven ``silk`` or ``fluff``."""

    style: str
    base: str
    light: str
    dark: str
    trim: str | None = None      # edge colour; the dark tone when unset
    veins: str | None = None     # glowing lines (runes, embers)
    gem: str | None = None       # a set stone on the crown and the shield
    glow: str | None = None      # an aura round the piece
    alpha: int = 255             # below 255 the plate is see-through (crystal)
    facets: bool = False         # crystal facet lines


_WARDEN = Look("plate", "#a8742f", "#f4d58e", "#3a220c")
_FORAGER = Look("plate", "#3d4a24", "#a9c070", "#161c0b", trim="#d8c48c")
_FROST = Look("plate", "#86cdee", "#f4fcff", "#2b6688", trim="#e8f8ff", glow="#b8ecff",
              alpha=185, facets=True)
_BROOD = Look("plate", "#231c2e", "#8f70c0", "#060508", trim="#5b3f86", veins="#c08aff",
              gem="#d7a6ff", glow="#9a5cf0")
_SUN = Look("plate", "#dba632", "#fff3b4", "#6a3d0a", trim="#fff0a0", veins="#ff7418",
            gem="#ff3a1c", glow="#ffc64d")
_SET_LOOKS = {"warden": _WARDEN, "forager": _FORAGER, "frost": _FROST, "brood": _BROOD,
              "sun": _SUN}
LOOKS = {
    "silk_carapace": Look("silk", "#e7dfcb", "#fffbf1", "#8d846f"),
    "fluffy_mantle": Look("fluff", "#d8cdbd", "#fff7ec", "#877c6c"),
    "leg_guard_set": Look("silk", "#e2d9c3", "#fffaf0", "#877e69"),
    "pedipalp_cuffs": Look("silk", "#d9c8a4", "#fff4dc", "#85745a"),
    "chelicerae_cap": Look("plate", "#6d7378", "#d9dee2", "#24272a"),
}
DEFAULT_LOOK = Look("silk", "#e7dfcb", "#fffbf1", "#8d846f")


def look_for(item_id: str) -> Look:
    if item_id in LOOKS:
        return LOOKS[item_id]
    # A set's pieces share its material: the id prefix names the set.
    prefix = item_id.split("_", 1)[0]
    return _SET_LOOKS.get(prefix, DEFAULT_LOOK)


def _c(value: str, alpha: int = 255) -> QColor:
    color = QColor(value)
    color.setAlpha(alpha)
    return color


def _pen(color: QColor, width: float, cap=Qt.RoundCap) -> QPen:
    return QPen(color, max(0.4, width), Qt.SolidLine, cap, Qt.RoundJoin)


def _fill(look: Look, value: str) -> QColor:
    return _c(value, look.alpha)


def _edge(look: Look) -> QColor:
    return _c(look.trim or look.dark)


def _aura(p: QPainter, cx: float, cy: float, rx: float, ry: float, look: Look) -> None:
    if not look.glow:
        return
    halo = QRadialGradient(QPointF(0.0, 0.0), rx * 1.25)
    halo.setColorAt(0.55, _c(look.glow, 120))
    halo.setColorAt(1.0, _c(look.glow, 0))
    p.save()
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(halo))
    p.translate(cx, cy)
    p.scale(1.0, ry / max(1e-3, rx))
    p.drawEllipse(QPointF(0.0, 0.0), rx * 1.25, rx * 1.25)
    p.restore()


def _vein(p: QPainter, a: QPointF, b: QPointF, look: Look, width: float) -> None:
    """A glowing line: a soft wide stroke under a bright thin one."""
    if not look.veins:
        return
    p.setPen(_pen(_c(look.veins, 90), width * 2.6))
    p.drawLine(a, b)
    p.setPen(_pen(_c(look.veins, 245), width))
    p.drawLine(a, b)


def _gem(p: QPainter, x: float, y: float, r: float, look: Look) -> None:
    if not look.gem:
        return
    stone = QPainterPath(QPointF(x + r, y))
    stone.lineTo(QPointF(x, y - r * 0.8))
    stone.lineTo(QPointF(x - r, y))
    stone.lineTo(QPointF(x, y + r * 0.8))
    stone.closeSubpath()
    shine = QRadialGradient(QPointF(x + r * 0.3, y - r * 0.25), r * 1.2)
    shine.setColorAt(0.0, QColor(255, 255, 255))
    shine.setColorAt(0.35, _c(look.gem))
    shine.setColorAt(1.0, _c(look.gem).darker(170))
    p.setPen(_pen(_edge(look), r * 0.25))
    p.setBrush(QBrush(shine))
    p.drawPath(stone)


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
    x = bounds.left() - bounds.height()
    while x < bounds.right():
        p.drawLine(QPointF(x, bounds.top()), QPointF(x + bounds.height(), bounds.bottom()))
        p.drawLine(QPointF(x + bounds.height(), bounds.top()), QPointF(x, bounds.bottom()))
        x += step
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
        _aura(p, cx, 0.0, pw / 2, ph / 2, look)
        shade = QRadialGradient(QPointF(cx + pw * 0.14, -ph * 0.18), pw * 0.66)
        shade.setColorAt(0.0, _fill(look, look.light))
        shade.setColorAt(0.45, _fill(look, look.base))
        shade.setColorAt(1.0, _fill(look, look.dark))
        p.setPen(_pen(_edge(look), w * 0.04))
        p.setBrush(QBrush(shade))
        p.drawEllipse(rect)
        fx = cx - pw * 0.06
        if look.facets:
            # Crystal: a hexagonal facet round the centre, spokes to the rim.
            p.setPen(_pen(_c(look.light, 200), w * 0.016))
            ring = [QPointF(fx + math.cos(i * math.pi / 3) * pw * 0.2,
                            math.sin(i * math.pi / 3) * ph * 0.2) for i in range(7)]
            for a, b in zip(ring, ring[1:]):
                p.drawLine(a, b)
            for i in range(6):
                a = i * math.pi / 3
                p.drawLine(ring[i], QPointF(cx + math.cos(a) * pw * 0.48, math.sin(a) * ph * 0.47))
        else:
            # Striations radiate from the fovea, as they do on the shell beneath.
            for i in range(8):
                a = (i + 0.5) * math.pi / 4
                start = QPointF(fx + math.cos(a) * pw * 0.11, math.sin(a) * ph * 0.11)
                end = QPointF(fx + math.cos(a) * pw * 0.39, math.sin(a) * ph * 0.39)
                if look.veins:
                    _vein(p, start, end, look, w * 0.016)
                else:
                    p.setPen(_pen(_c(look.dark, 150), w * 0.018))
                    p.drawLine(start, end)
        # The fovea is left open, a dark pit with a bright lip.
        p.setPen(_pen(_c(look.light, 170), w * 0.014))
        p.setBrush(QBrush(_c(look.dark)))
        p.drawEllipse(QPointF(fx, 0.0), pw * 0.065, ph * 0.05)
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_c(look.trim or look.light, 230)))
        for a in (0.62, 1.57, 2.52, -0.62, -1.57, -2.52):
            p.drawEllipse(QPointF(cx + math.cos(a) * pw * 0.43, math.sin(a) * ph * 0.42),
                          w * 0.024, w * 0.024)
        _gem(p, cx + pw * 0.3, 0.0, w * 0.075, look)
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
        _aura(p, ax + w * 0.08, wag, w * 0.42, h * 0.45, look)
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
            shade.setColorAt(0.0, _fill(look, look.dark))
            shade.setColorAt(0.35, _fill(look, look.base))
            shade.setColorAt(0.78, _fill(look, look.light))
            shade.setColorAt(1.0, _fill(look, look.base))
            p.setPen(_pen(_edge(look), w * 0.024))
            p.setBrush(QBrush(shade))
            p.drawRoundedRect(band, band_w * 0.3, band_w * 0.3)
            if look.facets:
                p.setPen(_pen(_c(look.light, 190), w * 0.012))
                p.drawLine(QPointF(x0 + band_w * 0.2, -h * 0.5 + wag), QPointF(x0 + band_w * 0.7, wag))
                p.drawLine(QPointF(x0 + band_w * 0.7, wag), QPointF(x0 + band_w * 0.2, h * 0.5 + wag))
            if look.veins:
                _vein(p, QPointF(x0 + band_w * 0.55, -h * 0.4 + wag),
                      QPointF(x0 + band_w * 0.55, h * 0.4 + wag), look, w * 0.012)
        p.setClipping(False)
        if look.veins:
            _vein(p, QPointF(back + step * 0.4, wag), QPointF(front - step * 0.2, wag), look, w * 0.016)
        else:
            p.setPen(_pen(_c(look.light, 150), w * 0.02))
            p.drawLine(QPointF(back + step * 0.4, wag), QPointF(front - step * 0.2, wag))
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_c(look.trim or look.light, 220)))
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
        _aura(p, hx - w * 0.2, 0.0, w * 0.75, h * 0.5, look)
        if look.gem or look.veins:
            # A crown: spikes standing out to either side of the crest.
            p.setPen(_pen(_edge(look), w * 0.06))
            p.setBrush(QBrush(_fill(look, look.base)))
            for side in (-1.0, 1.0):
                for k, x in enumerate((hx - w * 0.6, hx - w * 0.15, hx + w * 0.25)):
                    reach = h * (0.72 if k == 1 else 0.6)
                    spike = QPainterPath(QPointF(x - w * 0.14, side * h * 0.3))
                    spike.lineTo(QPointF(x, side * reach))
                    spike.lineTo(QPointF(x + w * 0.14, side * h * 0.3))
                    spike.closeSubpath()
                    p.drawPath(spike)
        shade = QLinearGradient(hx, -h * 0.5, hx, h * 0.5)
        shade.setColorAt(0.0, _fill(look, look.light))
        shade.setColorAt(0.5, _fill(look, look.base))
        shade.setColorAt(1.0, _fill(look, look.dark))
        p.setPen(_pen(_edge(look), w * 0.08))
        p.setBrush(QBrush(shade))
        p.drawPath(crest)
        if look.veins:
            _vein(p, QPointF(hx - w * 0.8, 0.0), QPointF(hx + w * 0.4, 0.0), look, w * 0.05)
        else:
            p.setPen(_pen(_c(look.light, 200), w * 0.06))
            p.drawLine(QPointF(hx - w * 0.8, 0.0), QPointF(hx + w * 0.4, 0.0))
        _gem(p, hx - w * 0.45, 0.0, w * 0.2, look)
        cap_color = _fill(look, look.base)
        cap_edge = _edge(look)
    else:
        _silk_fill(p, crest, look, w * 2.2)
        cap_color = _c(look.light, 230)
        cap_edge = _c(look.dark)
    p.setPen(_pen(cap_edge, w * 0.06))
    p.setBrush(QBrush(cap_color))
    for side in (-1.0, 1.0):
        p.drawEllipse(QPointF(hx + w * 0.56, side * h * 0.2), w * 0.14, h * 0.11)
    p.restore()


# -- limb pieces (joints) ------------------------------------------------------

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
        if look.glow:
            p.setPen(_pen(_c(look.glow, 70), hw * 3.2))
            p.drawLine(QPointF(a, 0.0), QPointF(b, 0.0))
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
        shade.setColorAt(0.0, _fill(look, look.light))
        shade.setColorAt(0.45, _fill(look, look.base))
        shade.setColorAt(1.0, _fill(look, look.dark))
        p.setPen(_pen(_edge(look), width * 0.14))
        p.setBrush(QBrush(shade))
        p.drawPath(plate)
        if look.facets:
            p.setPen(_pen(_c(look.light, 210), width * 0.1))
            zig = QPainterPath(QPointF(a + length * 0.08, -hw * 0.5))
            for k in range(1, 5):
                zig.lineTo(QPointF(a + (b - a) * k / 4, hw * (0.5 if k % 2 else -0.5)))
            p.setBrush(Qt.NoBrush)
            p.drawPath(zig)
        elif look.veins:
            _vein(p, QPointF(a + length * 0.1, 0.0), QPointF(b - length * 0.08, 0.0), look, width * 0.14)
        else:
            p.setPen(_pen(_c(look.light, 190), width * 0.12))
            p.drawLine(QPointF(a + length * 0.12, -hw * 0.25), QPointF(b - length * 0.1, -hw * 0.2))
        p.setPen(_pen(_c(look.trim or look.dark, 210), width * 0.12, Qt.FlatCap))
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


def _knee_cop(p: QPainter, x: float, y: float, width: float, look: Look) -> None:
    """A rounded cop over the knee joint, set with the gem on crowned sets."""
    r = max(0.9, width * 0.72)
    if look.style != "plate":
        p.setPen(_pen(_c(look.light, 215), width * 0.3))
        p.setBrush(Qt.NoBrush)
        p.drawEllipse(QPointF(x, y), r * 0.8, r * 0.8)
        return
    if look.glow:
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_c(look.glow, 80)))
        p.drawEllipse(QPointF(x, y), r * 1.7, r * 1.7)
    shine = QRadialGradient(QPointF(x - r * 0.3, y - r * 0.3), r * 1.3)
    shine.setColorAt(0.0, _fill(look, look.light))
    shine.setColorAt(0.5, _fill(look, look.base))
    shine.setColorAt(1.0, _fill(look, look.dark))
    p.setPen(_pen(_edge(look), width * 0.14))
    p.setBrush(QBrush(shine))
    p.drawEllipse(QPointF(x, y), r, r)
    if look.gem:
        _gem(p, x, y, r * 0.5, look)


def paint_leg_armour(p: QPainter, points, widths, look: Look) -> None:
    """Plates down the whole leg with a cop over the knee.

    ``points`` are the leg's joints from hip to claw and ``widths`` the width
    of each segment between them. Every segment is plated except the last,
    the clawed tarsus, which stays bare to grip. The cop sits on the joint
    after the femur -- the knee.
    """
    segments = len(points) - 1
    if segments < 2:
        return
    for i in range(segments - 1):
        (x1, y1), (x2, y2) = points[i], points[i + 1]
        width = widths[min(i, len(widths) - 1)]
        start, end = (0.14, 0.9) if i == 0 else (0.04, 0.96)
        _limb_plate(p, x1, y1, x2, y2, width, look, start, end, 2 if i == 0 else 1)
    kx, ky = points[1]
    _knee_cop(p, kx, ky, widths[min(1, len(widths) - 1)], look)


def paint_bracer(p: QPainter, x1: float, y1: float, x2: float, y2: float, width: float,
                 look: Look) -> None:
    """A short bracer round a pedipalp segment."""
    _limb_plate(p, x1, y1, x2, y2, width, look, 0.18, 0.82, 1)
    if look.gem:
        _gem(p, (x1 + x2) / 2, (y1 + y2) / 2, max(0.8, width * 0.4), look)


# -- icons ---------------------------------------------------------------------

_BODY = QColor(34, 28, 26)
_BAND = QColor(226, 110, 40)
_RIM = QColor(196, 120, 64)


def _limb(p: QPainter, points, widths, band_index: int = 1) -> None:
    for i in range(len(points) - 1):
        color = _BAND if i == band_index else _BODY
        p.setPen(_pen(color, widths[i]))
        p.drawLine(QPointF(*points[i]), QPointF(*points[i + 1]))


@lru_cache(maxsize=128)
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
        w, h = s * 0.72, s * 0.62
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_RIM))
        p.drawEllipse(QRectF(-w / 2, -h / 2, w, h))
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-w * 0.4, -h * 0.4, w * 0.8, h * 0.8))
        paint_carapace(p, 0.0, w, h, look)
    elif slot == "abdomen":
        w, h = s * 0.78, s * 0.62
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-w / 2, -h / 2, w, h))
        paint_abdomen(p, 0.0, w, h, 0.0, look)
    elif slot == "head":
        w, h = s * 0.3, s * 0.4
        p.setPen(Qt.NoPen)
        p.setBrush(QBrush(_BODY))
        p.drawEllipse(QRectF(-s * 0.36, -s * 0.28, s * 0.56, s * 0.56))
        p.drawEllipse(QRectF(-w / 2 + s * 0.08, -h / 2, w, h))
        paint_head(p, s * 0.08, w, h, look)
        p.setBrush(QBrush(QColor(90, 80, 70)))
        for ex, ey in ((0.0, -0.05), (0.0, 0.05), (0.04, -0.1), (0.04, 0.1)):
            p.drawEllipse(QPointF(s * (0.02 + ex), s * ey), s * 0.022, s * 0.022)
    elif slot == "legs":
        # One whole leg: hip bottom left, claw top right (the icon is turned).
        pts = [(-s * 0.4, s * 0.3), (-s * 0.06, s * 0.22), (s * 0.06, s * 0.1),
               (s * 0.2, -s * 0.1), (s * 0.33, -s * 0.26), (s * 0.43, -s * 0.36)]
        widths = [s * 0.11, s * 0.105, s * 0.09, s * 0.075, s * 0.06]
        _limb(p, pts, widths, band_index=1)
        paint_leg_armour(p, pts, widths, look)
    else:  # pedipalps
        width = s * 0.1
        for side in (-1.0, 1.0):
            pts = [(-s * 0.34, side * s * 0.1), (-s * 0.1, side * s * 0.2),
                   (s * 0.14, side * s * 0.18), (s * 0.36, side * s * 0.08)]
            _limb(p, pts, [width] * 3, band_index=0)
            paint_bracer(p, pts[1][0], pts[1][1], pts[2][0], pts[2][1], width, look)
    p.end()
    return image
