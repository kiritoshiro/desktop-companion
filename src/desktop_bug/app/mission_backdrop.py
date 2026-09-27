"""A painted map under a mission, instead of the desktop.

The owner: *"implement the background change on clicking the button ~ so that
instead of desktop it would show a map for the mission. so generate graphic
for each game and when clicking that button it would show that map instead of
desktop and again after clicking it it would show desktop again."*

Every map has its own ground: the meadow of *Take back the desktop*, the
flowered meadow of the fly levels, the scorched earth of *Ember hollow*, black
glass for *Obsidian deep*, thorns and roots for the Queen, and a dark digital
waste for *Reclaim the desktop*. On it, trodden paths run from your home to
every building, each building stands in a clearing, and props suit the place:
grass and pebbles, embers and lava cracks, crystals, bones, puddles.

The picture is painted once per screen and size -- at half size, then
smoothed up -- and kept, so showing it costs one image draw a frame.
"""
from __future__ import annotations

import math
import random

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import (QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QPixmap,
                         QRadialGradient)

# Painted at this share of the screen's size, then smoothed up.
DETAIL = 0.5

# ground top, ground bottom, path, props
LOOKS = {
    "territory": {"ground": ("#76853f", "#5c6b33"), "path": "#8f7248", "props": ("grass", "pebbles", "logs", "flowers")},
    "swarm": {"ground": ("#7c9444", "#5f7a36"), "path": "#94784c", "props": ("grass", "flowers", "flowers", "pebbles")},
    "ember": {"ground": ("#5c301d", "#3c1d12"), "path": "#7c4c2c", "props": ("ash", "lava", "embers", "rocks")},
    "reclaim": {"ground": ("#12264a", "#081328"), "path": "#1e4a7c", "props": ("shards", "glow")},
    "storm": {"ground": ("#3f4c37", "#2b3528"), "path": "#5c5242", "props": ("grass", "puddles", "rocks")},
    "obsidian": {"ground": ("#211b2c", "#120e1a"), "path": "#3c3250", "props": ("rocks", "crystals", "cracks")},
    "queen": {"ground": ("#43201c", "#2a1110"), "path": "#6c3c2a", "props": ("roots", "thorns", "bones", "rocks")},
}
# A map made in the editor borrows the look of its kind.
KIND_LOOK = {"raid": "territory", "reclaim": "reclaim", "swarm": "swarm"}


def look_for(map_id: str, kind: str = "raid") -> str:
    return map_id if map_id in LOOKS else KIND_LOOK.get(kind, "territory")


def _shade(color, amount):
    c = QColor(color)
    return c.lighter(amount) if amount >= 100 else c.darker(int(10000 / max(1, amount)))


# -- the ground -----------------------------------------------------------------

def _ground(p, w, h, look, rng):
    top, bottom = LOOKS[look]["ground"]
    grad = QLinearGradient(0, 0, 0, h)
    grad.setColorAt(0, QColor(top))
    grad.setColorAt(1, QColor(bottom))
    p.fillRect(QRectF(0, 0, w, h), grad)
    p.setPen(Qt.NoPen)
    # Mottling: soft patches lighter and darker than the ground.
    for _ in range(int(w * h / 900)):
        x, y = rng.uniform(0, w), rng.uniform(0, h)
        r = rng.uniform(4, 26)
        base = QColor(top if rng.random() < 0.5 else bottom)
        tone = base.lighter(rng.randint(108, 130)) if rng.random() < 0.5 else base.darker(rng.randint(110, 140))
        tone.setAlpha(rng.randint(18, 45))
        p.setBrush(tone)
        p.drawEllipse(QPointF(x, y), r, r * rng.uniform(0.5, 1.0))


def _paths(p, sites, look, scale, rng):
    """Trodden paths from home to every building, and a clearing round each."""
    if not sites:
        return
    colour = QColor(LOOKS[look]["path"])
    home = next((s for s in sites if s[2] == "home"), sites[0])
    hx, hy = home[0] * scale, home[1] * scale
    for x, y, kind in sites:
        if (x, y, kind) == home:
            continue
        x, y = x * scale, y * scale
        mx, my = (hx + x) / 2 + rng.uniform(-60, 60) * scale * 2, (hy + y) / 2 + rng.uniform(-50, 50) * scale * 2
        path = QPainterPath(QPointF(hx, hy))
        path.quadTo(QPointF(mx, my), QPointF(x, y))
        for width, alpha, tone in ((58, 90, 90), (40, 150, 100), (22, 110, 118)):
            c = _shade(colour, tone)
            c.setAlpha(alpha)
            p.setPen(QPen(c, width * scale, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
            p.setBrush(Qt.NoBrush)
            p.drawPath(path)
    p.setPen(Qt.NoPen)
    for x, y, _kind in sites:
        x, y = x * scale, y * scale
        glow = QRadialGradient(x, y + 20 * scale, 150 * scale)
        inner = QColor(colour)
        inner.setAlpha(170)
        outer = QColor(colour)
        outer.setAlpha(0)
        glow.setColorAt(0.0, inner)
        glow.setColorAt(0.55, inner)
        glow.setColorAt(1.0, outer)
        p.setBrush(glow)
        p.drawEllipse(QPointF(x, y + 20 * scale), 150 * scale, 95 * scale)


# -- props ------------------------------------------------------------------------

def _grass(p, x, y, s, rng, look):
    base = QColor(LOOKS[look]["ground"][0])
    for _ in range(rng.randint(4, 8)):
        c = base.lighter(rng.randint(110, 150)) if rng.random() < 0.6 else base.darker(125)
        c.setAlpha(210)
        p.setPen(QPen(c, 1.2 * s, Qt.SolidLine, Qt.RoundCap))
        dx = rng.uniform(-5, 5) * s
        p.drawLine(QPointF(x + dx, y), QPointF(x + dx + rng.uniform(-3, 3) * s, y - rng.uniform(5, 11) * s))


def _flowers(p, x, y, s, rng, look):
    colours = ("#f4e9c8", "#f2c94c", "#e07a8a", "#b9a6ee", "#ffffff")
    p.setPen(Qt.NoPen)
    for _ in range(rng.randint(2, 5)):
        c = QColor(rng.choice(colours))
        px, py = x + rng.uniform(-8, 8) * s, y + rng.uniform(-5, 5) * s
        p.setBrush(c)
        p.drawEllipse(QPointF(px, py), 1.6 * s, 1.6 * s)


def _pebbles(p, x, y, s, rng, look, dark=False):
    for _ in range(rng.randint(1, 4)):
        r = rng.uniform(1.5, 4.5) * s
        px, py = x + rng.uniform(-9, 9) * s, y + rng.uniform(-6, 6) * s
        base = QColor(70, 64, 58) if dark else QColor(rng.randint(130, 170), rng.randint(120, 150), rng.randint(100, 130))
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(0, 0, 0, 60))
        p.drawEllipse(QPointF(px + r * 0.3, py + r * 0.35), r, r * 0.7)
        p.setBrush(base)
        p.drawEllipse(QPointF(px, py), r, r * 0.72)
        p.setBrush(base.lighter(140))
        p.drawEllipse(QPointF(px - r * 0.3, py - r * 0.25), r * 0.35, r * 0.22)


def _rocks(p, x, y, s, rng, look):
    dark = look in ("obsidian", "ember", "queen", "storm")
    r = rng.uniform(7, 16) * s
    path = QPainterPath()
    for i in range(7):
        a = i / 7 * math.tau
        pt = QPointF(x + math.cos(a) * r * rng.uniform(0.75, 1.15), y + math.sin(a) * r * 0.62 * rng.uniform(0.8, 1.1))
        path.moveTo(pt) if i == 0 else path.lineTo(pt)
    path.closeSubpath()
    base = QColor(52, 44, 58) if look == "obsidian" else (QColor(78, 66, 58) if dark else QColor(128, 116, 98))
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(0, 0, 0, 70))
    p.drawPath(path.translated(r * 0.35, r * 0.3))
    grad = QLinearGradient(x - r, y - r, x + r, y + r)
    grad.setColorAt(0, base.lighter(150))
    grad.setColorAt(1, base.darker(130))
    p.setBrush(grad)
    p.drawPath(path)


def _logs(p, x, y, s, rng, look):
    length, r = rng.uniform(22, 40) * s, rng.uniform(3.5, 6) * s
    angle = rng.uniform(-0.5, 0.5)
    p.save()
    p.translate(x, y)
    p.rotate(math.degrees(angle))
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(0, 0, 0, 60))
    p.drawRoundedRect(QRectF(-length / 2 + 3 * s, -r + 3 * s, length, r * 2), r, r)
    grad = QLinearGradient(0, -r, 0, r)
    grad.setColorAt(0, QColor("#8a6440"))
    grad.setColorAt(1, QColor("#4a3220"))
    p.setBrush(grad)
    p.drawRoundedRect(QRectF(-length / 2, -r, length, r * 2), r, r)
    p.setBrush(QColor("#c8a476"))
    p.drawEllipse(QPointF(length / 2 - r * 0.4, 0), r * 0.55, r * 0.9)
    p.restore()


def _ash(p, x, y, s, rng, look):
    p.setPen(Qt.NoPen)
    c = QColor(120, 110, 104, rng.randint(40, 80))
    p.setBrush(c)
    p.drawEllipse(QPointF(x, y), rng.uniform(8, 22) * s, rng.uniform(4, 10) * s)


def _lava(p, x, y, s, rng, look):
    path = QPainterPath(QPointF(x, y))
    a = rng.uniform(0, math.tau)
    for _ in range(rng.randint(3, 6)):
        a += rng.uniform(-0.8, 0.8)
        x, y = x + math.cos(a) * rng.uniform(8, 18) * s, y + math.sin(a) * rng.uniform(6, 14) * s
        path.lineTo(x, y)
    for width, colour in ((6, QColor(255, 90, 20, 60)), (2.6, QColor(255, 140, 40, 200)), (1.0, QColor(255, 230, 150, 230))):
        p.setPen(QPen(colour, width * s, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        p.setBrush(Qt.NoBrush)
        p.drawPath(path)


def _embers(p, x, y, s, rng, look):
    p.setPen(Qt.NoPen)
    for _ in range(rng.randint(1, 3)):
        px, py = x + rng.uniform(-10, 10) * s, y + rng.uniform(-8, 8) * s
        p.setBrush(QColor(255, 120, 30, 70))
        p.drawEllipse(QPointF(px, py), 4 * s, 4 * s)
        p.setBrush(QColor(255, rng.randint(170, 220), 80, 230))
        p.drawEllipse(QPointF(px, py), 1.3 * s, 1.3 * s)


def _crystals(p, x, y, s, rng, look):
    for _ in range(rng.randint(2, 4)):
        h = rng.uniform(10, 24) * s
        wdt = h * rng.uniform(0.25, 0.4)
        lean = rng.uniform(-0.35, 0.35)
        bx = x + rng.uniform(-8, 8) * s
        top = QPointF(bx + lean * h, y - h)
        path = QPainterPath(QPointF(bx - wdt, y))
        path.lineTo(bx - wdt * 0.6, y - h * 0.7)
        path.lineTo(top)
        path.lineTo(bx + wdt * 0.6, y - h * 0.7)
        path.lineTo(bx + wdt, y)
        path.closeSubpath()
        grad = QLinearGradient(bx - wdt, y - h, bx + wdt, y)
        grad.setColorAt(0, QColor(196, 150, 255, 235))
        grad.setColorAt(1, QColor(70, 40, 120, 235))
        p.setPen(QPen(QColor(230, 210, 255, 140), 0.8 * s))
        p.setBrush(grad)
        p.drawPath(path)


def _cracks(p, x, y, s, rng, look):
    p.setPen(QPen(QColor(150, 110, 220, 110), 1.1 * s, Qt.SolidLine, Qt.RoundCap))
    a = rng.uniform(0, math.tau)
    for _ in range(rng.randint(2, 4)):
        nx, ny = x + math.cos(a) * rng.uniform(10, 24) * s, y + math.sin(a) * rng.uniform(8, 18) * s
        p.drawLine(QPointF(x, y), QPointF(nx, ny))
        x, y, a = nx, ny, a + rng.uniform(-0.9, 0.9)


def _roots(p, x, y, s, rng, look):
    path = QPainterPath(QPointF(x, y))
    path.cubicTo(QPointF(x + rng.uniform(-30, 30) * s, y + rng.uniform(-20, 20) * s),
                 QPointF(x + rng.uniform(-40, 40) * s, y + rng.uniform(-20, 20) * s),
                 QPointF(x + rng.uniform(-60, 60) * s, y + rng.uniform(-30, 30) * s))
    p.setPen(QPen(QColor(30, 14, 10, 200), rng.uniform(2.5, 4.5) * s, Qt.SolidLine, Qt.RoundCap))
    p.setBrush(Qt.NoBrush)
    p.drawPath(path)
    p.setPen(QPen(QColor(90, 50, 34, 150), 1.0 * s, Qt.SolidLine, Qt.RoundCap))
    p.drawPath(path.translated(-0.8 * s, -0.8 * s))


def _thorns(p, x, y, s, rng, look):
    p.setPen(Qt.NoPen)
    for _ in range(rng.randint(2, 4)):
        bx = x + rng.uniform(-10, 10) * s
        h = rng.uniform(8, 16) * s
        lean = rng.uniform(-4, 4) * s
        path = QPainterPath(QPointF(bx - 2 * s, y))
        path.quadTo(QPointF(bx + lean * 0.3, y - h * 0.5), QPointF(bx + lean, y - h))
        path.quadTo(QPointF(bx + lean * 0.4, y - h * 0.4), QPointF(bx + 2 * s, y))
        path.closeSubpath()
        p.setBrush(QColor(28, 10, 10, 230))
        p.drawPath(path)


def _bones(p, x, y, s, rng, look):
    p.save()
    p.translate(x, y)
    p.rotate(rng.uniform(0, 180))
    p.setPen(QPen(QColor(60, 50, 40, 160), 0.8 * s))
    p.setBrush(QColor(226, 214, 190))
    length = rng.uniform(8, 14) * s
    p.drawRoundedRect(QRectF(-length / 2, -1.2 * s, length, 2.4 * s), 1.2 * s, 1.2 * s)
    for end in (-1, 1):
        for side in (-1, 1):
            p.drawEllipse(QPointF(end * length / 2, side * 1.6 * s), 1.6 * s, 1.6 * s)
    p.restore()


def _puddles(p, x, y, s, rng, look):
    rx, ry = rng.uniform(10, 26) * s, rng.uniform(5, 11) * s
    grad = QRadialGradient(x - rx * 0.3, y - ry * 0.3, rx)
    grad.setColorAt(0, QColor(150, 170, 190, 170))
    grad.setColorAt(1, QColor(60, 76, 90, 170))
    p.setPen(QPen(QColor(40, 36, 30, 120), 1.0 * s))
    p.setBrush(grad)
    p.drawEllipse(QPointF(x, y), rx, ry)


def _grid(p, w, h, s):
    p.setPen(QPen(QColor(70, 140, 220, 38), 1.0))
    step = 48 * s
    x = 0.0
    while x < w:
        p.drawLine(QPointF(x, 0), QPointF(x, h))
        x += step
    y = 0.0
    while y < h:
        p.drawLine(QPointF(0, y), QPointF(w, y))
        y += step


def _shards(p, x, y, s, rng, look):
    path = QPainterPath()
    r = rng.uniform(8, 20) * s
    for i in range(3):
        a = rng.uniform(0, math.tau)
        pt = QPointF(x + math.cos(a) * r, y + math.sin(a) * r)
        path.moveTo(pt) if i == 0 else path.lineTo(pt)
    path.closeSubpath()
    p.setPen(QPen(QColor(150, 210, 255, 150), 0.8 * s))
    p.setBrush(QColor(40, 90, 160, 110))
    p.drawPath(path)


def _glow(p, x, y, s, rng, look):
    g = QRadialGradient(x, y, 30 * s)
    g.setColorAt(0, QColor(120, 230, 90, 70))
    g.setColorAt(1, QColor(120, 230, 90, 0))
    p.setPen(Qt.NoPen)
    p.setBrush(g)
    p.drawEllipse(QPointF(x, y), 30 * s, 30 * s)


PROPS = {"grass": _grass, "flowers": _flowers, "pebbles": _pebbles, "rocks": _rocks, "logs": _logs,
         "ash": _ash, "lava": _lava, "embers": _embers, "crystals": _crystals, "cracks": _cracks,
         "roots": _roots, "thorns": _thorns, "bones": _bones, "puddles": _puddles, "shards": _shards,
         "glow": _glow}
# How many of each per million painted pixels.
DENSITY = {"grass": 900, "flowers": 260, "pebbles": 240, "rocks": 22, "logs": 5, "ash": 200, "lava": 12,
           "embers": 160, "crystals": 22, "cracks": 140, "roots": 40, "thorns": 160, "bones": 9,
           "puddles": 30, "shards": 90, "glow": 6}


def _vignette(p, w, h):
    g = QRadialGradient(w / 2, h / 2, max(w, h) * 0.72)
    g.setColorAt(0.55, QColor(0, 0, 0, 0))
    g.setColorAt(1.0, QColor(0, 0, 0, 120))
    p.fillRect(QRectF(0, 0, w, h), g)


def paint_backdrop(look: str, width: int, height: int, sites=(), seed: int = 0) -> QImage:
    """The ground of one screen, ``width`` x ``height``. ``sites``: (x, y,
    kind) of the buildings on it, in this screen's own pixels."""
    w, h = max(8, int(width * DETAIL)), max(8, int(height * DETAIL))
    image = QImage(w, h, QImage.Format_RGB32)
    rng = random.Random(f"{look}-{seed}-{width}x{height}")
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    _ground(p, w, h, look, rng)
    s = DETAIL * 2.0          # prop sizes are in full-size pixels / 2
    if look == "reclaim":
        _grid(p, w, h, s)
    _paths(p, list(sites), look, DETAIL, rng)
    megapixels = w * h / 1_000_000
    near = [(x * DETAIL, y * DETAIL) for x, y, _k in sites]
    for name in LOOKS[look]["props"]:
        painter = PROPS[name]
        for _ in range(int(DENSITY[name] * megapixels * 4)):
            x, y = rng.uniform(0, w), rng.uniform(0, h)
            # Keep the clearings round the buildings clear of props.
            if any((x - sx) ** 2 + ((y - sy) * 1.5) ** 2 < (95 * DETAIL * 2) ** 2 for sx, sy in near):
                continue
            painter(p, x, y, s, rng, look)
    _vignette(p, w, h)
    p.end()
    return image.scaled(int(width), int(height), Qt.IgnoreAspectRatio, Qt.SmoothTransformation)


class MissionBackdrop:
    """Paints each screen's map under a mission, cached per screen and size."""

    def __init__(self):
        self._cache: dict = {}

    def pixmaps(self, mission):
        """(screen rect, pixmap) for every screen of the mission."""
        info = mission.map_info
        look = look_for(info.id, getattr(info, "kind", "raid"))
        result = []
        for screen in mission.layout.screens:
            r = screen.rect
            sites = tuple((round(s.x - r.x), round(s.y - r.y), s.kind) for s in mission.sites
                          if getattr(s, "screen", 0) == screen.index)
            key = (info.id, screen.index, int(r.w), int(r.h), sites)
            pixmap = self._cache.get(key)
            if pixmap is None:
                image = paint_backdrop(look, int(r.w), int(r.h), sites, seed=screen.index)
                pixmap = self._cache[key] = QPixmap.fromImage(image)
            result.append((r, pixmap))
        return result

    def paint(self, painter, mission) -> None:
        for r, pixmap in self.pixmaps(mission):
            painter.drawPixmap(QRectF(r.x, r.y, r.w, r.h), pixmap, QRectF(pixmap.rect()))
