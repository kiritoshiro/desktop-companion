"""Small painted scenes for the map cards on the level selection (the owner:
"some photos on the level selection should also be placed").

Each map gets a banner: its ground and light, a few of its wooden buildings
from :mod:`mission_art`, and what makes it itself -- flies, embers, acid on a
frozen desktop, black crystal, thorns. Painted once at twice the size.
"""
from __future__ import annotations

import math
import random
from functools import lru_cache

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QPixmap, QRadialGradient

from .mission_art import ART_H, ART_W, building_art

PICTURE_W, PICTURE_H = 216, 92
SCALE = 2

# sky top, sky bottom / horizon, ground near, ground far
THEMES = {
    "territory": ("#9cc3d8", "#e8d9a8", "#6f7a3c", "#8d8a4c"),
    "swarm": ("#b7d3c4", "#efe2a6", "#6c7e3a", "#94904e"),
    "ember": ("#3a1a14", "#d8702c", "#4e2e1c", "#7a4424"),
    "reclaim": ("#1d4f86", "#3f7fbf", "#1a3c64", "#2a5a8c"),
    "storm": ("#2c3440", "#6a7482", "#3e4636", "#565c48"),
    "obsidian": ("#140f22", "#4a3a6a", "#221c2a", "#3a3044"),
    "queen": ("#2a0c14", "#9a2c34", "#3e1e1a", "#5e2e24"),
    "burrow": ("#6a8aa0", "#d8c49a", "#5e4a30", "#7e6644"),
}
# Buildings on the banner: kind, owned, x centre (0..1), scale.
SCENES = {
    "territory": [("home", True, 0.22, 0.42), ("nest", False, 0.76, 0.40)],
    "swarm": [("flynest", False, 0.30, 0.40), ("food", True, 0.78, 0.36)],
    "ember": [("venom", False, 0.26, 0.40), ("nest", False, 0.74, 0.44)],
    "reclaim": [("nest", False, 0.70, 0.40)],
    "storm": [("flynest", False, 0.24, 0.40), ("flynest", False, 0.74, 0.36)],
    "obsidian": [("infestation", False, 0.28, 0.40), ("nest", False, 0.74, 0.42)],
    "queen": [("nest", False, 0.22, 0.36), ("hatchery", False, 0.62, 0.48)],
    "burrow": [("outpost", True, 0.18, 0.34), ("home", True, 0.50, 0.46), ("lookout", True, 0.82, 0.38)],
}
# A map made in the editor borrows the look of its kind.
KIND_LOOK = {"raid": "territory", "reclaim": "reclaim", "swarm": "swarm"}


def _sky(p, theme, w, h, horizon):
    top, low, near, far = (QColor(c) for c in THEMES[theme])
    sky = QLinearGradient(0, 0, 0, horizon)
    sky.setColorAt(0, top)
    sky.setColorAt(1, low)
    p.fillRect(QRectF(0, 0, w, horizon + 1), sky)
    ground = QLinearGradient(0, horizon, 0, h)
    ground.setColorAt(0, far)
    ground.setColorAt(1, near)
    p.fillRect(QRectF(0, horizon, w, h - horizon), ground)


def _desktop(p, w, h, rng):
    """A frozen desktop: windows, a melted hole and cracked glass."""
    back = QLinearGradient(0, 0, w, h)
    back.setColorAt(0, QColor("#1d4f86"))
    back.setColorAt(1, QColor("#3a7cc0"))
    p.fillRect(QRectF(0, 0, w, h), back)
    for x, y, ww, hh in ((10, 8, 92, 56), (70, 22, 88, 50)):
        p.fillRect(QRectF(x, y, ww, hh), QColor("#eef1f5"))
        p.fillRect(QRectF(x, y, ww, 8), QColor("#3b3f46"))
        p.setPen(QPen(QColor(90, 96, 110, 160), 1.2))
        for row in range(3):
            length = ww - 18 - rng.uniform(0, 30)
            p.drawLine(QPointF(x + 6, y + 16 + row * 9), QPointF(x + 6 + length, y + 16 + row * 9))
    # Acid hole through the window into the void.
    hole = QPainterPath()
    cx, cy = 118, 52
    for i in range(18):
        a = i / 18 * math.tau
        r = 13 + rng.uniform(-3, 3)
        pt = QPointF(cx + math.cos(a) * r * 1.3, cy + math.sin(a) * r)
        hole.moveTo(pt) if i == 0 else hole.lineTo(pt)
    hole.closeSubpath()
    glow = QRadialGradient(cx, cy, 22)
    glow.setColorAt(0, QColor(20, 6, 30))
    glow.setColorAt(0.75, QColor(40, 16, 50))
    glow.setColorAt(1, QColor(150, 230, 60))
    p.setPen(QPen(QColor(160, 240, 70), 1.6))
    p.setBrush(glow)
    p.drawPath(hole)
    # Cracks from a heavy landing.
    p.setPen(QPen(QColor(235, 245, 255, 190), 0.9))
    ox, oy = 40, 70
    for i in range(7):
        a = i / 7 * math.tau + rng.uniform(-0.2, 0.2)
        x, y = ox, oy
        for _ in range(3):
            nx = x + math.cos(a) * rng.uniform(8, 16)
            ny = y + math.sin(a) * rng.uniform(6, 12)
            p.drawLine(QPointF(x, y), QPointF(nx, ny))
            x, y, a = nx, ny, a + rng.uniform(-0.5, 0.5)


def _flies(p, w, h, rng, count, dark=False):
    for _ in range(count):
        x, y = rng.uniform(6, w - 6), rng.uniform(6, h * 0.62)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(255, 255, 255, 150))
        p.drawEllipse(QPointF(x - 2, y - 1.5), 2.2, 1.3)
        p.drawEllipse(QPointF(x + 2, y - 1.5), 2.2, 1.3)
        p.setBrush(QColor(30, 30, 24) if not dark else QColor(12, 12, 10))
        p.drawEllipse(QPointF(x, y), 1.8, 1.4)


def _embers(p, w, h, rng):
    for _ in range(26):
        x, y = rng.uniform(0, w), rng.uniform(0, h * 0.8)
        r = rng.uniform(0.6, 1.6)
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(255, rng.randint(120, 200), 60, rng.randint(140, 230)))
        p.drawEllipse(QPointF(x, y), r, r)


def _rain(p, w, h, rng):
    p.setPen(QPen(QColor(200, 214, 230, 90), 0.8))
    for _ in range(46):
        x, y = rng.uniform(-10, w), rng.uniform(-10, h)
        p.drawLine(QPointF(x, y), QPointF(x - 4, y + 11))


def _thorns(p, w, h, rng):
    p.setPen(QPen(QColor(40, 14, 12), 1.6))
    for x in range(0, int(w) + 8, 9):
        base = h - rng.uniform(0, 3)
        top = base - rng.uniform(8, 16)
        lean = rng.uniform(-3, 3)
        p.drawLine(QPointF(x, base), QPointF(x + lean, top))
        p.drawLine(QPointF(x + lean * 0.5, (base + top) / 2), QPointF(x + lean * 0.5 + 3, (base + top) / 2 - 3))


def _shards(p, w, h, rng):
    for _ in range(6):
        x, y = rng.uniform(0, w), h - rng.uniform(4, 18)
        s = rng.uniform(5, 11)
        path = QPainterPath(QPointF(x - s * 0.4, y))
        path.lineTo(x, y - s * 1.6)
        path.lineTo(x + s * 0.4, y)
        path.closeSubpath()
        p.setPen(QPen(QColor(150, 120, 220, 170), 0.7))
        p.setBrush(QColor(40, 26, 64, 230))
        p.drawPath(path)


def _paint_scene(look, w, h, rng):
    img = QImage(int(w * SCALE), int(h * SCALE), QImage.Format_ARGB32_Premultiplied)
    img.fill(Qt.transparent)
    p = QPainter(img)
    p.setRenderHint(QPainter.Antialiasing)
    p.setRenderHint(QPainter.SmoothPixmapTransform)
    p.scale(SCALE, SCALE)
    horizon = h * 0.42
    if look == "reclaim":
        _desktop(p, w, h, rng)
    else:
        _sky(p, look, w, h, horizon)
    if look in ("ember",):
        _embers(p, w, h, rng)
    if look == "storm":
        _rain(p, w, h, rng)
    for kind, owned, fx, scale in SCENES.get(look, SCENES["territory"]):
        art = building_art(kind, owned)
        bw, bh = ART_W * scale, ART_H * scale
        # The art's ground point (120, 139) sits a little above the banner's foot.
        x = fx * w - 120 * scale
        y = h - 8 - 139 * scale
        p.drawImage(QRectF(x, y, bw, bh), art)
    if look in ("swarm", "storm"):
        _flies(p, w, h, rng, 14 if look == "swarm" else 24, dark=look == "storm")
    if look == "queen":
        _thorns(p, w, h, rng)
    if look == "obsidian":
        _shards(p, w, h, rng)
    # A soft vignette so the picture sits in the card.
    edge = QRadialGradient(w / 2, h / 2, w * 0.62)
    edge.setColorAt(0.6, QColor(0, 0, 0, 0))
    edge.setColorAt(1, QColor(0, 0, 0, 120))
    p.fillRect(QRectF(0, 0, w, h), edge)
    p.end()
    return img


@lru_cache(maxsize=32)
def map_picture(map_id: str, kind: str = "raid", locked: bool = False,
                width: int = PICTURE_W, height: int = PICTURE_H) -> QPixmap:
    """The card banner for a map; a map from the editor looks like its kind.
    A locked map's picture is dimmed and drained of colour."""
    look = map_id if map_id in THEMES else KIND_LOOK.get(kind, "territory")
    img = _paint_scene(look, width, height, random.Random(f"card-{map_id}"))
    if locked:
        grey = img.convertToFormat(QImage.Format_Grayscale8).convertToFormat(QImage.Format_ARGB32_Premultiplied)
        p = QPainter(img)
        p.setOpacity(0.7)
        p.drawImage(0, 0, grey)
        p.setOpacity(1)
        p.fillRect(img.rect(), QColor(20, 10, 4, 90))
        p.end()
    pixmap = QPixmap.fromImage(img)
    pixmap.setDevicePixelRatio(SCALE)
    return pixmap
