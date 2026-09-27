"""Generate the carved-wood interface art in ``assets/ui/``.

    python tools/generate_ui_art.py

Everything is drawn with Qt, which the project already depends on, so there
is no image library to install and the art can be regenerated after a change
to the palette or to the spider itself: the three mode pictures burn the real
procedural tarantula into the wood rather than a drawing of one.

Outputs (all PNG):

- ``wood_tile.png``      light oak, tiles seamlessly; window backgrounds
- ``wood_dark_tile.png`` walnut, tiles seamlessly; cards, HUD, dialogs
- ``card_frame.png``     nine-slice carved frame for the mode cards
- ``mode_companion.png`` / ``mode_adventure.png`` / ``mode_strategy.png``
- ``menu_*.png``         small carved emblems for menus and settings sections
- ``app_icon.png`` / ``app_icon.ico``  the app's own icon (window, tray, exe)
"""

from __future__ import annotations

import json
import math
import os
import random
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="ui-art-"))
os.environ.setdefault("DESKTOP_BUG_GL", "0")

from PyQt5.QtCore import QPointF, QRectF, Qt  # noqa: E402
from PyQt5.QtGui import (QBrush, QColor, QImage, QLinearGradient, QPainter,  # noqa: E402
                         QPainterPath, QPen, QRadialGradient)
from PyQt5.QtWidgets import QApplication  # noqa: E402

OUT = ROOT / "assets" / "ui"

# The palette the runtime theme uses too (desktop_bug/app/wood_theme.py).
OAK_LIGHT = (196, 150, 98)
OAK_DARK = (138, 92, 52)
WALNUT_LIGHT = (104, 66, 38)
WALNUT_DARK = (52, 30, 16)
BURN = QColor(38, 20, 9)
BRASS = QColor(214, 164, 72)
TEAM_RED = QColor(150, 58, 44)
TEAM_BLUE = QColor(60, 104, 126)


# ---------------------------------------------------------------- wood grain

class Noise:
    """Periodic value noise, so a texture can tile without a seam."""

    def __init__(self, seed: int) -> None:
        rng = random.Random(seed)
        self.table = [rng.random() for _ in range(256 * 256)]

    def value(self, x: float, y: float, px: int, py: int) -> float:
        ix, iy = math.floor(x), math.floor(y)
        fx, fy = x - ix, y - iy
        sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
        x0, x1 = ix % px, (ix + 1) % px
        y0, y1 = iy % py, (iy + 1) % py
        t = self.table
        a, b = t[y0 * 256 + x0], t[y0 * 256 + x1]
        c, d = t[y1 * 256 + x0], t[y1 * 256 + x1]
        top = a + (b - a) * sx
        bottom = c + (d - c) * sx
        return top + (bottom - top) * sy


def wood(width: int, height: int, light, dark, seed: int, rings: int = 7,
         warp_amount: float = 2.4) -> QImage:
    """Plain-sawn grain running left to right; tiles in both directions."""
    noise = Noise(seed)
    data = bytearray(width * height * 4)
    two_pi = math.tau
    for y in range(height):
        v = y / height
        for x in range(width):
            u = x / width
            warp = (noise.value(u * 3, v * 5, 3, 5) * 0.65
                    + noise.value(u * 7, v * 11, 7, 11) * 0.35)
            ring = 0.5 + 0.5 * math.sin((v * rings + warp * warp_amount) * two_pi)
            ring = ring ** 6
            # Fine fibres: long streaks along the grain, sharpened into lines.
            fibre = noise.value(u * 4, v * 220, 4, 220) ** 2.2
            fleck = noise.value(u * 9, v * 90, 9, 90)
            t = 0.46 * ring + 0.30 * fibre + 0.10 * warp + 0.14 * fleck
            t = min(1.0, max(0.0, t))
            i = (y * width + x) * 4
            data[i] = int(light[2] + (dark[2] - light[2]) * t)
            data[i + 1] = int(light[1] + (dark[1] - light[1]) * t)
            data[i + 2] = int(light[0] + (dark[0] - light[0]) * t)
            data[i + 3] = 255
    return QImage(bytes(data), width, height, width * 4, QImage.Format_ARGB32).copy()


# ----------------------------------------------------------------- carving

def _outside(path: QPainterPath, bounds: QRectF) -> QPainterPath:
    frame = QPainterPath()
    frame.addRect(bounds.adjusted(-200, -200, 200, 200))
    return frame.subtracted(path)


def recess(p: QPainter, path: QPainterPath, depth: float = 7.0, shade: int = 60) -> None:
    """Carve ``path`` into the surface: a floor in shadow, lit on the far lip."""
    bounds = path.boundingRect()
    p.save()
    p.setClipPath(path)
    p.fillPath(path, QColor(30, 15, 5, shade))
    outside = _outside(path, bounds)
    p.setPen(Qt.NoPen)
    steps = 8
    for i in range(steps, 0, -1):
        d = depth * i / steps
        p.setBrush(QColor(20, 9, 2, int(34 * (1 - (i - 1) / steps))))
        p.drawPath(outside.translated(d * 0.8, d))
    for i in range(1, 4):
        p.setBrush(QColor(255, 226, 170, 26))
        p.drawPath(outside.translated(-i * 0.7, -i * 0.8))
    p.restore()
    # The lit top edge of the cut, outside the recess.
    p.save()
    p.setClipPath(_outside(path, bounds))
    p.setPen(QPen(QColor(255, 230, 180, 70), 1.4))
    p.setBrush(Qt.NoBrush)
    p.drawPath(path.translated(0.9, 1.1))
    p.restore()


def relief(p: QPainter, path: QPainterPath, height: float = 4.0, fill=None) -> None:
    """Raise ``path`` out of the surface: shadow below right, light above left."""
    p.save()
    p.setPen(Qt.NoPen)
    for i in range(6, 0, -1):
        d = height * i / 6
        p.setBrush(QColor(20, 9, 2, 22))
        p.drawPath(path.translated(d * 0.7, d))
    if fill is not None:
        p.setBrush(fill)
        p.drawPath(path)
    p.setClipPath(path)
    outside = _outside(path, path.boundingRect())
    for i in range(1, 4):
        p.setBrush(QColor(255, 232, 186, 34))
        p.drawPath(outside.translated(i * 0.8, i * 0.9))
    for i in range(1, 4):
        p.setBrush(QColor(20, 9, 2, 40))
        p.drawPath(outside.translated(-i * 0.8, -i * 0.9))
    p.restore()


def burn(p: QPainter, path: QPainterPath, width: float = 2.2, alpha: int = 235) -> None:
    """Pyrography: a scorched halo, then the dark line itself."""
    p.save()
    p.setBrush(Qt.NoBrush)
    for w, a in ((width * 4.0, 18), (width * 2.4, 36), (width * 1.5, 70)):
        p.setPen(QPen(QColor(60, 28, 8, a), w, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        p.drawPath(path)
    p.setPen(QPen(QColor(BURN.red(), BURN.green(), BURN.blue(), alpha), width,
                  Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
    p.drawPath(path)
    p.restore()


def groove(p: QPainter, path: QPainterPath, width: float = 3.0) -> None:
    """A V-cut line: a dark channel with a lit lower lip."""
    p.save()
    p.setBrush(Qt.NoBrush)
    p.setPen(QPen(QColor(255, 228, 176, 90), width * 0.55, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
    p.drawPath(path.translated(width * 0.35, width * 0.45))
    p.setPen(QPen(QColor(34, 16, 5, 215), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
    p.drawPath(path)
    p.restore()


def inlay(p: QPainter, path: QPainterPath, color: QColor) -> None:
    """A brass or stained inlay set flush into the wood."""
    p.save()
    grad = QLinearGradient(path.boundingRect().topLeft(), path.boundingRect().bottomRight())
    grad.setColorAt(0.0, color.lighter(135))
    grad.setColorAt(0.55, color)
    grad.setColorAt(1.0, color.darker(140))
    p.setPen(QPen(QColor(30, 14, 4, 220), 1.6))
    p.setBrush(QBrush(grad))
    p.drawPath(path)
    p.restore()


def rounded(rect: QRectF, radius: float) -> QPainterPath:
    path = QPainterPath()
    path.addRoundedRect(rect, radius, radius)
    return path


def ellipse(cx: float, cy: float, rx: float, ry: float) -> QPainterPath:
    path = QPainterPath()
    path.addEllipse(QPointF(cx, cy), rx, ry)
    return path


def polyline(points, closed: bool = False) -> QPainterPath:
    path = QPainterPath(QPointF(*points[0]))
    for pt in points[1:]:
        path.lineTo(QPointF(*pt))
    if closed:
        path.closeSubpath()
    return path


def vignette(p: QPainter, w: int, h: int) -> None:
    grad = QRadialGradient(QPointF(w * 0.5, h * 0.45), max(w, h) * 0.72)
    grad.setColorAt(0.55, QColor(0, 0, 0, 0))
    grad.setColorAt(1.0, QColor(20, 8, 0, 120))
    p.fillRect(QRectF(0, 0, w, h), QBrush(grad))


# ---------------------------------------------------------------- the spider

_MODEL = None
_TRAITS = None


def spider_image(size: int, heading: float, walk_frames: int = 0, seed: int = 3,
                 burned: bool = True) -> QImage:
    """The real tarantula, rendered and burned into a wood tone.

    Returned centred in a square ``size`` x ``size`` transparent image.
    ``walk_frames`` lets it take a few strides first, so the legs are in a
    stepping pose rather than the symmetric rest pose.
    """
    global _MODEL, _TRAITS
    from desktop_bug.content.body_plans import resolve_body_plan
    from desktop_bug.creature import Creature

    if _MODEL is None:
        _MODEL = resolve_body_plan(json.loads(
            (ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8")))
        _TRAITS = json.loads((ROOT / "personalities" / "mellow.json").read_text(encoding="utf-8"))
    random.seed(seed)
    creature = Creature(_MODEL, _TRAITS, 4000, 4000, index=0, progression_id=f"art:{seed}")
    creature.x, creature.y = 2000.0, 2000.0
    creature.heading = creature.target_heading = heading
    creature._initialize_legs()
    for skill in ("roll", "jump", "weave_web", "shoot_web", "drift"):
        try:
            creature.set_skill_enabled(skill, False)
        except Exception:
            pass
    dt = 1.0 / 60.0
    for i in range(max(walk_frames, 20)):
        if walk_frames and i < walk_frames:
            creature.target_x = creature.x + math.cos(heading) * 400.0
            creature.target_y = creature.y + math.sin(heading) * 400.0
        else:
            creature.target_x, creature.target_y = creature.x, creature.y
        creature.target_heading = heading
        creature.update(dt, -100000.0, -100000.0, 4000, 4000)
    raw = QImage(size, size, QImage.Format_ARGB32_Premultiplied)
    raw.fill(0)
    p = QPainter(raw)
    p.setRenderHint(QPainter.Antialiasing, True)
    scale = size / (creature.size * 5.2)
    p.translate(size / 2, size / 2)
    p.scale(scale, scale)
    p.translate(-creature.x, -creature.y)
    creature.render(p, always_show_names=False)
    p.end()
    return _burn_tone(raw) if burned else raw.convertToFormat(QImage.Format_ARGB32)


def _burn_tone(image: QImage) -> QImage:
    """Map a colour render to scorched wood: dark body, amber where it was lit."""
    image = image.convertToFormat(QImage.Format_ARGB32)
    w, h = image.width(), image.height()
    ptr = image.bits()
    ptr.setsize(w * h * 4)
    data = bytearray(ptr.asstring())
    for i in range(0, len(data), 4):
        a = data[i + 3]
        if not a:
            continue
        b, g, r = data[i], data[i + 1], data[i + 2]
        lum = (0.3 * r + 0.55 * g + 0.15 * b) / 255.0
        warm = max(0.0, (r - b) / 255.0)       # the red-knee bands
        t = min(1.0, lum * 1.6 + warm * 0.55)
        data[i + 2] = int(34 + (176 - 34) * t)
        data[i + 1] = int(18 + (112 - 18) * t)
        data[i] = int(8 + (52 - 8) * t)
    return QImage(bytes(data), w, h, w * 4, QImage.Format_ARGB32).copy()


def place_spider(p: QPainter, image: QImage, cx: float, cy: float, scorch: bool = True) -> None:
    """Burn a spider image in at a point, with a scorched halo under it."""
    w = image.width()
    x, y = cx - w / 2, cy - w / 2
    if scorch:
        p.save()
        p.setOpacity(0.10)
        for dx, dy in ((-3, -2), (3, -2), (-3, 3), (3, 3), (0, 4), (4, 0), (0, -3), (-4, 0)):
            p.drawImage(QPointF(x + dx + 2, y + dy + 3), image)
        p.restore()
    p.drawImage(QPointF(x, y), image)


# ------------------------------------------------------------------ panels

W, H = 720, 440


def base_panel(seed: int):
    image = wood(W, H, OAK_LIGHT, OAK_DARK, seed=seed, rings=8, warp_amount=1.6)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    inner = rounded(QRectF(22, 22, W - 44, H - 44), 18)
    recess(p, inner, depth=9, shade=38)
    groove(p, rounded(QRectF(11, 11, W - 22, H - 22), 24), width=2.2)
    return image, p


def finish(p: QPainter, image: QImage, name: str) -> None:
    vignette(p, W, H)
    p.end()
    image.save(str(OUT / name))
    print("wrote", name)


def web(p: QPainter, cx: float, cy: float, radius: float, a0: float, a1: float,
        spokes: int = 7, turns: int = 5) -> None:
    ends = []
    for i in range(spokes):
        a = a0 + (a1 - a0) * i / (spokes - 1)
        ends.append(a)
        burn(p, polyline([(cx, cy), (cx + math.cos(a) * radius, cy + math.sin(a) * radius)]), 1.3)
    for k in range(1, turns + 1):
        r = radius * k / (turns + 0.6)
        pts = []
        for a in ends:
            sag = 0.93 if k % 2 else 0.97
            pts.append((cx + math.cos(a) * r * sag, cy + math.sin(a) * r * sag))
        burn(p, polyline(pts), 1.0, alpha=200)


def fly(p: QPainter, x: float, y: float, s: float = 1.0) -> None:
    for side in (-1, 1):
        wing = ellipse(x + side * 6 * s, y - 5 * s, 7 * s, 4 * s)
        burn(p, wing, 1.0, alpha=160)
    body = ellipse(x, y, 4.5 * s, 7 * s)
    p.save()
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(40, 20, 8, 235))
    p.drawPath(body)
    p.restore()


def star(cx, cy, r_out, r_in, points=5, rot=-math.pi / 2):
    pts = []
    for i in range(points * 2):
        r = r_out if i % 2 == 0 else r_in
        a = rot + i * math.pi / points
        pts.append((cx + math.cos(a) * r, cy + math.sin(a) * r))
    return polyline(pts, closed=True)


def companion() -> None:
    image, p = base_panel(seed=11)
    # A monitor burned into the board, the desktop the spiders live on.
    screen = QRectF(196, 78, 330, 206)
    relief(p, rounded(screen.adjusted(-12, -12, 12, 12), 16), height=5,
           fill=QColor(118, 76, 42))
    recess(p, rounded(screen, 8), depth=6, shade=70)
    stand = polyline([(330, 296), (392, 296), (410, 338), (312, 338)], closed=True)
    relief(p, stand, height=4, fill=QColor(118, 76, 42))
    for r in (QRectF(222, 102, 118, 70), QRectF(360, 150, 132, 86)):
        burn(p, rounded(r, 5), 1.4, alpha=150)
        burn(p, polyline([(r.left(), r.top() + 13), (r.right(), r.top() + 13)]), 1.1, alpha=150)
    # A web in the top-left corner and its catch.
    web(p, 34, 34, 150, 0.05, math.pi / 2 - 0.05)
    fly(p, 104, 92, 1.1)
    # The moon over the desk: it lives there all day and all night.
    moon = ellipse(612, 92, 30, 30).subtracted(ellipse(626, 82, 27, 27))
    inlay(p, moon, BRASS)
    for sx, sy, r in ((560, 70, 6), (660, 150, 5), (584, 140, 4)):
        inlay(p, star(sx, sy, r, r * 0.45), BRASS)
    # The spider, resting in front of the screen.
    place_spider(p, spider_image(250, -0.55, walk_frames=0, seed=4), 520, 322)
    finish(p, image, "mode_companion.png")


def adventure() -> None:
    image, p = base_panel(seed=23)
    # The leap: a dashed arc from where it pushed off.
    arc = QPainterPath(QPointF(90, 360))
    arc.cubicTo(QPointF(150, 150), QPointF(270, 130), QPointF(330, 214))
    p.save()
    pen = QPen(QColor(38, 20, 9, 200), 3.0, Qt.DashLine, Qt.RoundCap)
    pen.setDashPattern([2.5, 3.0])
    p.setPen(pen)
    p.drawPath(arc)
    p.restore()
    for i, (x, y) in enumerate(((78, 372), (98, 380), (60, 380))):
        groove(p, polyline([(x - 10, y), (x + 10, y)]), 2.0)
    # Speed lines behind it.
    for i in range(4):
        y = 236 + i * 18
        burn(p, polyline([(236 - i * 14, y), (318 - i * 6, y)]), 1.8, alpha=200 - i * 30)
    # The silk shot: from the spider to a fly, ending in a splat.
    silk = QPainterPath(QPointF(470, 214))
    silk.quadTo(QPointF(540, 150), QPointF(606, 150))
    burn(p, silk, 1.6)
    for i in range(8):
        a = i * math.tau / 8
        burn(p, polyline([(606, 150), (606 + math.cos(a) * 16, 150 + math.sin(a) * 16)]), 1.1, alpha=190)
    fly(p, 606, 150, 1.25)
    # Level up: three brass chevrons, and a small skill tree.
    for i in range(3):
        y = 310 - i * 22
        chevron = polyline([(590, y + 14), (620, y), (650, y + 14), (650, y + 24),
                            (620, y + 10), (590, y + 24)], closed=True)
        inlay(p, chevron, BRASS if i < 2 else BRASS.darker(150))
    nodes = [(84, 84), (130, 60), (130, 108), (176, 84)]
    for a, b in ((0, 1), (0, 2), (1, 3), (2, 3)):
        groove(p, polyline([nodes[a], nodes[b]]), 2.4)
    for i, (x, y) in enumerate(nodes):
        path = ellipse(x, y, 12, 12)
        if i < 3:
            inlay(p, path, BRASS)
        else:
            recess(p, path, depth=3, shade=80)
            groove(p, path, 1.6)
    # The spider mid-stride, heading for its prey.
    place_spider(p, spider_image(270, -0.42, walk_frames=34, seed=9), 406, 250)
    finish(p, image, "mode_adventure.png")


def strategy() -> None:
    image, p = base_panel(seed=37)
    # A map carved into the board: contour lines, then two bases.
    rng = random.Random(5)
    for k in range(6):
        pts = []
        for i in range(41):
            a = i / 40 * math.tau
            r = 60 + k * 30 + math.sin(a * 3 + k) * 10 + rng.uniform(-3, 3)
            pts.append((360 + math.cos(a) * r * 1.6, 220 + math.sin(a) * r * 0.8))
        burn(p, polyline(pts, closed=True), 0.7, alpha=55)

    def base(cx, cy, flag_color, flip):
        for dx, dy, r in ((0, 0, 34), (-40, 14, 24), (38, 16, 22), (-8, 30, 20)):
            mound = ellipse(cx + dx, cy + dy, r * 1.25, r * 0.75)
            relief(p, mound, height=4, fill=QColor(112, 70, 36))
            for j in range(3):
                burn(p, polyline([(cx + dx - r * 0.6, cy + dy - r * 0.1 + j * 5),
                                  (cx + dx + r * 0.5, cy + dy - r * 0.2 + j * 5)]), 0.8, alpha=90)
        pole = polyline([(cx, cy - 10), (cx, cy - 86)])
        groove(p, pole, 3.2)
        sgn = -1 if flip else 1
        flag = polyline([(cx, cy - 86), (cx + sgn * 50, cy - 74), (cx, cy - 58)], closed=True)
        inlay(p, flag, flag_color)

    base(170, 318, TEAM_BLUE, False)
    base(560, 158, TEAM_RED, True)
    # Orders: a dashed route and an arrowhead into the rival camp.
    route = QPainterPath(QPointF(232, 290))
    route.cubicTo(QPointF(330, 250), QPointF(400, 250), QPointF(488, 196))
    p.save()
    pen = QPen(QColor(38, 20, 9, 210), 4.0, Qt.DashLine, Qt.RoundCap)
    pen.setDashPattern([3.0, 2.5])
    p.setPen(pen)
    p.drawPath(route)
    p.restore()
    head = polyline([(488, 196), (462, 196), (480, 176)], closed=True)
    inlay(p, head, QColor(60, 30, 12))
    # Squads beside each base.
    for (x, y, h, s) in ((300, 372, -0.5, 1), (352, 330, -0.6, 2), (250, 396, -0.3, 3)):
        place_spider(p, spider_image(120, h, walk_frames=18, seed=20 + s), x, y, scorch=True)
    for (x, y, h, s) in ((460, 110, 2.6, 4), (470, 250, 2.9, 5)):
        place_spider(p, spider_image(110, h, walk_frames=18, seed=30 + s), x, y, scorch=True)
    finish(p, image, "mode_strategy.png")


def card_frame() -> None:
    """A nine-slice frame: a carved border 28px wide around dark walnut."""
    size = 160
    image = wood(size, size, WALNUT_LIGHT, WALNUT_DARK, seed=71, rings=3)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    relief(p, rounded(QRectF(4, 4, size - 8, size - 8), 16), height=3)
    groove(p, rounded(QRectF(12, 12, size - 24, size - 24), 11), width=2.0)
    recess(p, rounded(QRectF(22, 22, size - 44, size - 44), 8), depth=4, shade=30)
    p.end()
    # Round the outer corners off to transparent.
    out = QImage(size, size, QImage.Format_ARGB32_Premultiplied)
    out.fill(0)
    q = QPainter(out)
    q.setRenderHint(QPainter.Antialiasing, True)
    q.setClipPath(rounded(QRectF(0, 0, size, size), 18))
    q.drawImage(0, 0, image)
    q.end()
    out.save(str(OUT / "card_frame.png"))
    print("wrote card_frame.png")


def menu_emblem(name: str) -> None:
    """A compact, transparent walnut medallion with a carved action symbol.

    These icons sit beside menu labels and Companion section headings. The
    outer ring stays warm and legible at 20-32px; a few simple inlaid shapes
    give every action a recognizable meaning without relying on emoji fonts.
    """
    size = 72
    image = QImage(size, size, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    disc = ellipse(36, 36, 32, 32)
    relief(p, disc, height=3, fill=QColor(91, 55, 29))
    inner = ellipse(36, 36, 27, 27)
    recess(p, inner, depth=3, shade=35)
    groove(p, ellipse(36, 36, 29, 29), 1.6)
    gold = QColor(222, 177, 92)
    pale = QColor(255, 232, 181)
    chestnut = QColor(156, 93, 43)

    def stroke(points, width=2.8, color=gold):
        path = polyline(points)
        p.save()
        p.setPen(QPen(QColor(31, 15, 5, 220), width + 1.4, Qt.SolidLine,
                      Qt.RoundCap, Qt.RoundJoin))
        p.drawPath(path)
        p.setPen(QPen(color, width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        p.drawPath(path)
        p.restore()

    def dot(x, y, radius=3.2, color=pale):
        relief(p, ellipse(x, y, radius, radius), height=1.2, fill=color)

    def spider(cx=36, cy=39, scale=1.0, eye_color=pale):
        for side in (-1, 1):
            for index, dy in enumerate((-8, -3, 3, 8)):
                x0, y0 = cx + side * 6 * scale, cy + dy * scale
                reach = (13 if index in (0, 3) else 16) * scale
                bend = (9 if index < 2 else -9) * scale
                stroke(((x0, y0), (cx + side * 11 * scale, y0 + bend),
                        (cx + side * reach * scale, y0 + (bend + dy * .4) * scale)),
                       2.0 * scale)
        p.save()
        p.setPen(QPen(QColor(29, 13, 4, 230), 1.4))
        p.setBrush(QColor(159, 103, 53))
        p.drawEllipse(QRectF(cx - 7 * scale, cy - 5 * scale, 14 * scale, 15 * scale))
        p.setBrush(QColor(111, 69, 38))
        p.drawEllipse(QRectF(cx - 7 * scale, cy - 12 * scale, 14 * scale, 11 * scale))
        p.setPen(Qt.NoPen)
        p.setBrush(eye_color)
        p.drawEllipse(QRectF(cx - 4.8 * scale, cy - 10 * scale, 3.7 * scale, 3.7 * scale))
        p.drawEllipse(QRectF(cx + 1.1 * scale, cy - 10 * scale, 3.7 * scale, 3.7 * scale))
        p.restore()

    def eye(cx=36, cy=37):
        path = QPainterPath(QPointF(cx - 16, cy))
        path.quadTo(QPointF(cx, cy - 14), QPointF(cx + 16, cy))
        path.quadTo(QPointF(cx, cy + 14), QPointF(cx - 16, cy))
        stroke(((cx - 16, cy), (cx - 8, cy - 8), (cx, cy - 11),
                (cx + 8, cy - 8), (cx + 16, cy), (cx + 8, cy + 8),
                (cx, cy + 11), (cx - 8, cy + 8), (cx - 16, cy)), 2.3)
        p.save()
        p.setPen(QPen(QColor(35, 17, 6), 1.4))
        p.setBrush(QColor(72, 139, 154))
        p.drawEllipse(QRectF(cx - 5.5, cy - 5.5, 11, 11))
        p.setBrush(pale)
        p.drawEllipse(QRectF(cx - 2, cy - 3.8, 3, 3))
        p.restore()

    if name in {"companion", "creatures", "appearance"}:
        if name == "creatures":
            spider(26, 41, .66)
            spider(45, 42, .62)
            dot(36, 26, 2.4)
        elif name == "appearance":
            spider(36, 42, 1.0)
        else:
            spider(36, 42, .83)
            web(p, 15, 15, 12, 0, math.pi / 2, spokes=4, turns=2)
    elif name == "inspect":
        p.save()
        p.setPen(QPen(QColor(38, 18, 6), 5.0, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(43, 43, 56, 56)
        p.setPen(QPen(gold, 3.1))
        p.drawEllipse(QRectF(17, 16, 31, 31))
        p.restore()
        eye(32, 31)
    elif name in {"pin", "name", "rename"}:
        tag = polyline(((20, 27), (43, 23), (54, 35), (36, 53), (20, 38)), closed=True)
        relief(p, tag, height=2.5, fill=chestnut if name == "pin" else gold)
        dot(28, 32, 2.4, pale)
        if name == "pin":
            stroke(((43, 32), (42, 41), (36, 48)), 2.4, pale)
        elif name == "rename":
            stroke(((31, 43), (45, 29), (49, 33), (35, 47), (30, 48), (31, 43)), 2.2, pale)
        else:
            stroke(((29, 39), (37, 33), (44, 40)), 2.1, pale)
    elif name == "skills":
        stroke(((24, 48), (24, 33), (36, 25), (48, 33), (48, 48)), 2.4)
        stroke(((24, 33), (36, 40), (48, 33)), 2.1)
        for x, y in ((24, 48), (24, 33), (36, 25), (48, 33), (48, 48), (36, 40)):
            dot(x, y, 3.4, pale if (x, y) == (36, 25) else gold)
    elif name == "base":
        mound = QPainterPath(QPointF(18, 47))
        mound.quadTo(QPointF(36, 27), QPointF(54, 47))
        stroke(((18, 47), (23, 41), (29, 37), (36, 35), (43, 37), (49, 41), (54, 47)), 2.7)
        stroke(((30, 50), (30, 43), (36, 39), (42, 43), (42, 50)), 2.3, pale)
        stroke(((36, 22), (36, 34)), 1.8)
        stroke(((36, 23), (47, 27), (36, 31)), 2.0, gold)
    elif name in {"move", "interaction"}:
        if name == "move":
            stroke(((36, 19), (36, 53)), 2.2)
            stroke(((19, 36), (53, 36)), 2.2)
            for pts in (((36, 19), (31, 26)), ((36, 19), (41, 26)),
                        ((36, 53), (31, 46)), ((36, 53), (41, 46)),
                        ((19, 36), (26, 31)), ((19, 36), (26, 41)),
                        ((53, 36), (46, 31)), ((53, 36), (46, 41))):
                stroke(pts, 2.0, pale)
        else:
            stroke(((24, 18), (24, 48), (32, 41), (38, 53), (44, 50),
                    (38, 38), (49, 38), (24, 18)), 2.5, pale)
            dot(50, 23, 2.5, gold)
    elif name == "remove":
        stroke(((23, 23), (49, 49)), 4.0, pale)
        stroke(((49, 23), (23, 49)), 4.0, pale)
    elif name in {"cage", "teams"}:
        if name == "cage":
            stroke(((21, 26), (51, 26), (51, 49), (21, 49), (21, 26)), 2.1)
            for x in (28, 36, 44):
                stroke(((x, 27), (x, 48)), 1.7, pale)
            stroke(((22, 34), (50, 34)), 1.4)
        else:
            dot(27, 36, 8, QColor(119, 151, 179))
            dot(45, 36, 8, QColor(190, 89, 70))
            stroke(((28, 37), (35, 29), (43, 37)), 2.2, pale)
            dot(36, 22, 2.2)
    elif name == "armor":
        shield = polyline(((36, 18), (50, 24), (49, 39), (43, 48), (36, 54),
                           (29, 48), (23, 39), (22, 24), (36, 18)), closed=True)
        relief(p, shield, height=2.5, fill=QColor(125, 92, 52))
        groove(p, polyline(((36, 23), (36, 48))), 1.7)
        stroke(((27, 28), (36, 33), (45, 28)), 1.7, pale)
        dot(36, 40, 2.4, gold)
    elif name == "labels":
        eye(36, 36)
    elif name == "health":
        heart = QPainterPath(QPointF(36, 51))
        heart.cubicTo(QPointF(31, 46), QPointF(20, 40), QPointF(20, 31))
        heart.cubicTo(QPointF(20, 21), QPointF(31, 19), QPointF(36, 28))
        heart.cubicTo(QPointF(41, 19), QPointF(52, 21), QPointF(52, 31))
        heart.cubicTo(QPointF(52, 40), QPointF(41, 46), QPointF(36, 51))
        p.save()
        p.setPen(QPen(QColor(50, 19, 8), 2.8, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        p.setBrush(QColor(173, 70, 49))
        p.drawPath(heart)
        p.setPen(QPen(pale, 1.5, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(36, 27, 36, 44)
        p.drawLine(29, 35, 43, 35)
        p.restore()
    elif name == "stamina":
        bolt = polyline(((40, 18), (25, 38), (34, 38), (30, 54), (48, 31),
                         (39, 31), (44, 18)), closed=True)
        relief(p, bolt, height=2.0, fill=QColor(223, 181, 67))
    elif name == "xp":
        stroke(((21, 50), (21, 24)), 2.0, pale)
        stroke(((21, 50), (53, 50)), 2.0, pale)
        stroke(((24, 45), (32, 39), (38, 42), (48, 28)), 3.0)
        stroke(((42, 28), (48, 28), (48, 34)), 2.0, pale)
    elif name == "preset":
        page = rounded(QRectF(22, 17, 28, 38), 3)
        relief(p, page, height=1.8, fill=chestnut)
        stroke(((28, 28), (44, 28)), 1.7, pale)
        stroke(((28, 35), (44, 35)), 1.7, pale)
        stroke(((28, 42), (40, 42)), 1.7, pale)
        stroke(((44, 18), (44, 27), (51, 24)), 1.7)
    elif name in {"behavior", "performance"}:
        if name == "behavior":
            for y, knob_x in ((27, 42), (37, 29), (47, 46)):
                stroke(((21, y), (51, y)), 2.0)
                dot(knob_x, y, 3.5, pale)
        else:
            arc = QPainterPath(QPointF(20, 47))
            arc.cubicTo(QPointF(21, 20), QPointF(51, 20), QPointF(52, 47))
            p.save()
            p.setPen(QPen(gold, 3.0, Qt.SolidLine, Qt.RoundCap))
            p.drawPath(arc)
            p.restore()
            stroke(((36, 40), (47, 29)), 2.6, pale)
            dot(36, 40, 3)
    elif name == "flies":
        fly(p, 36, 39, 1.5)
        stroke(((23, 21), (19, 17)), 1.5, pale)
        stroke(((49, 21), (53, 17)), 1.5, pale)
    elif name in {"launch", "quit", "adventure"}:
        if name == "adventure":
            spider(34, 40, .77)
            stroke(((17, 51), (25, 47), (21, 54)), 2.3, pale)
            stroke(((48, 22), (55, 17), (52, 27)), 2.1, gold)
        else:
            stroke(((24, 20), (40, 20), (40, 52), (24, 52), (24, 20)), 2.3)
            if name == "launch":
                stroke(((28, 36), (54, 36), (46, 28)), 3.0, pale)
                stroke(((54, 36), (46, 44)), 3.0, pale)
            else:
                stroke(((52, 36), (42, 36), (48, 30)), 2.6, pale)
                stroke(((42, 36), (48, 42)), 2.6, pale)
    elif name == "strategy":
        stroke(((21, 48), (27, 33), (41, 38), (48, 23), (54, 48), (21, 48)), 2.2)
        stroke(((29, 27), (29, 18), (46, 22), (29, 26)), 2.3, pale)
        dot(28, 48, 2.5)
    elif name == "randomize":
        stroke(((20, 24), (28, 24), (44, 48), (52, 48)), 2.4)
        stroke(((20, 48), (28, 48), (44, 24), (52, 24)), 2.4, pale)
        stroke(((47, 20), (53, 24), (47, 28)), 2.0)
        stroke(((47, 44), (53, 48), (47, 52)), 2.0)
    elif name == "help":
        p.save()
        p.setPen(QPen(pale, 3.0))
        font = p.font()
        font.setFamily("Georgia")
        font.setBold(True)
        font.setPointSize(31)
        p.setFont(font)
        p.drawText(QRectF(19, 13, 34, 43), Qt.AlignCenter, "?")
        p.restore()
    p.end()
    image.save(str(OUT / f"menu_{name}.png"))
    print("wrote", f"menu_{name}.png")


ICON_SIZES = (16, 24, 32, 48, 64, 128, 256)


def app_icon_image(size: int = 256) -> QImage:
    """A walnut medallion with a carved rim and the tarantula burned in.

    Small sizes (the title bar and taskbar use 16-32px) get a lighter disc
    and the spider in its own black and orange, larger: burned brown on brown
    turned to mud below 48px.
    """
    small = size <= 48
    image = QImage(size, size, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    disc = ellipse(size / 2, size / 2, size * 0.47, size * 0.47)
    board = (wood(size, size, (214, 172, 118), (184, 138, 88), seed=41, rings=3) if small
             else wood(size, size, (150, 100, 58), (96, 60, 32), seed=41, rings=4))
    p.save()
    p.setClipPath(disc)
    p.drawImage(0, 0, board)
    p.restore()
    rim = size * (0.07 if small else 0.035)
    p.setBrush(Qt.NoBrush)
    p.setPen(QPen(QColor(30, 14, 4, 235), rim))
    p.drawPath(disc)
    p.setPen(QPen(QColor(255, 226, 170, 90), rim * 0.35))
    p.drawEllipse(QPointF(size / 2 - rim * 0.25, size / 2 - rim * 0.25), size * 0.43, size * 0.43)
    # Facing up and slightly right, legs spread: reads as a spider at 16px.
    if small:
        # Rendered large and scaled down, so the legs keep their shape.
        spider = spider_image(256, -1.9, walk_frames=0, seed=4, burned=False)
        spider = spider.scaled(int(size * 1.22), int(size * 1.22), Qt.KeepAspectRatio,
                               Qt.SmoothTransformation)
        place_spider(p, spider, size / 2, size / 2 + size * 0.03, scorch=False)
    else:
        spider = spider_image(int(size * 1.02), -1.9, walk_frames=0, seed=4)
        place_spider(p, spider, size / 2, size / 2 + size * 0.02, scorch=size >= 64)
    p.end()
    return image


def write_ico(images, path: Path) -> None:
    """A Windows .ico holding one PNG per size (Vista and later read these)."""
    import struct
    from PyQt5.QtCore import QBuffer, QByteArray, QIODevice

    blobs = []
    for image in images:
        data = QByteArray()
        buffer = QBuffer(data)
        buffer.open(QIODevice.WriteOnly)
        image.save(buffer, "PNG")
        buffer.close()
        blobs.append((image.width(), bytes(data)))
    header = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    entries, payload = b"", b""
    for width, blob in blobs:
        dim = 0 if width >= 256 else width
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset + len(payload))
        payload += blob
    path.write_bytes(header + entries + payload)


def app_icon() -> None:
    big = app_icon_image(256)
    big.save(str(OUT / "app_icon.png"))
    images = [app_icon_image(n) if n <= 48 else
              big.scaled(n, n, Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
              if n != 256 else big for n in ICON_SIZES]
    write_ico(images, OUT / "app_icon.ico")
    print("wrote app_icon.png, app_icon.ico")


def main() -> None:
    app = QApplication.instance() or QApplication(sys.argv[:1])  # noqa: F841
    OUT.mkdir(parents=True, exist_ok=True)
    # The window background: bigger and gentler than the panels, so the
    # repeat is not noticed across a whole window.
    wood(512, 512, (202, 158, 106), (168, 120, 72), seed=3, rings=6,
         warp_amount=1.1).save(str(OUT / "wood_tile.png"))
    print("wrote wood_tile.png")
    wood(256, 256, WALNUT_LIGHT, WALNUT_DARK, seed=5, rings=4).save(str(OUT / "wood_dark_tile.png"))
    print("wrote wood_dark_tile.png")
    card_frame()
    app_icon()
    for name in ("companion", "adventure", "strategy", "inspect", "pin", "name", "rename",
                 "skills", "preset", "creatures", "teams", "behavior", "flies", "launch",
                 "base", "move", "remove", "cage", "armor", "labels", "health", "stamina", "xp",
                 "appearance", "performance", "randomize", "interaction", "help", "quit"):
        menu_emblem(name)
    only = set(sys.argv[1:])
    for name, make in (("companion", companion), ("adventure", adventure), ("strategy", strategy)):
        if not only or name in only:
            make()


if __name__ == "__main__":
    main()
