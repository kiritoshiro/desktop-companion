"""The mission buildings, painted once like game assets and cached.

The owner: *"make nicer graphics of those buildings. like actual game asset,
not like now mostly drawn circles."*

Every building sits on the same kind of base -- a raised slab of earth with a
soil face, grass tufts, moss and pebbles, and a soft cast shadow -- and every
part of it is painted the same way (``_lit``): lit from the upper left,
textured (bark grain, stone, weave, speckle), darkened where it meets the
ground, outlined, with a rim of light on its top edge. Pictures are painted
at twice their size and carry a device pixel ratio of 2, so they stay crisp
when the mission draws them.

- Home burrow: an earth mound with a stone-arched tunnel, silk curtain,
  roots and two glowing mushroom lanterns;
- Food cache: a veined leaf canopy on lashed poles over silk-wrapped prey,
  berries and a beetle;
- Silk loom: a lashed frame with warp threads, a woven panel and spools;
- Hatchery: a cradle of veined egg sacs under a twig arch; sealed, bound in
  silk under a wax seal;
- Thorn nest: a rock-and-bramble fortress with bone spikes round a toothed
  maw with red eyes; claimed, the eyes go out and silk covers the mouth;
- Outpost: a stitched hide tent on stakes with guy ropes and a lantern;
- Infestation: acid sacs dripping over a cracked, bevelled screen pane;
  destroyed, the sacs are burst and grey;
- Venom den: a ringed stump holding a veined venom gland over a puddle;
- Lookout: a two-storey scaffold with a plank deck, ladder and lantern;
- Amber mine: a stone-rimmed pit of faceted amber crystals and a pick;
- Nursery: a leaf canopy over a silk cradle of eggs, spiderlings when yours;
- Fly nest: a rotting, bitten fruit with a leaf and a cloud of flies.

A pennant on every building shows who holds it. The anchor is unchanged:
the site's point is at (120, 139) of a 240x190 picture.
"""
from __future__ import annotations

from functools import lru_cache
import math
import random

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import QBrush, QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QRadialGradient

ART_W, ART_H = 240, 190
GROUND_Y = 139            # where the site's point is
SCALE = 2                 # painted at twice the size


def _c(color, alpha=None) -> QColor:
    c = QColor(color)
    if alpha is not None:
        c.setAlpha(alpha)
    return c


def _pen(color, width=1.0, alpha=255):
    return QPen(_c(color, alpha), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)


def _blob(cx, cy, rx, ry, rng, wobble=0.12, points=22) -> QPainterPath:
    """An irregular closed shape round (cx, cy)."""
    pts = []
    for i in range(points):
        a = i * math.tau / points
        r = 1.0 + rng.uniform(-wobble, wobble)
        pts.append(QPointF(cx + math.cos(a) * rx * r, cy + math.sin(a) * ry * r))
    path = QPainterPath(pts[0])
    for i in range(points):
        a, b = pts[i], pts[(i + 1) % points]
        path.quadTo(a, QPointF((a.x() + b.x()) / 2, (a.y() + b.y()) / 2))
    path.closeSubpath()
    return path


def _poly(*points) -> QPainterPath:
    path = QPainterPath(QPointF(*points[0]))
    for pt in points[1:]:
        path.lineTo(QPointF(*pt))
    path.closeSubpath()
    return path


# -- the shared way of painting a part -------------------------------------------

def _lit(p, path, base, rng=None, texture=None, outline=True, ao=True, rim=True, light=135, dark=165):
    """Fill a part lit from the upper left, texture it, shade its foot,
    outline it and catch the light on its upper edge."""
    box = path.boundingRect()
    base = _c(base)
    g = QLinearGradient(box.topLeft(), box.bottomRight())
    g.setColorAt(0.0, base.lighter(light))
    g.setColorAt(0.55, base)
    g.setColorAt(1.0, base.darker(dark))
    p.save()
    p.setPen(Qt.NoPen)
    p.fillPath(path, QBrush(g))
    p.setClipPath(path, Qt.IntersectClip)
    if texture is not None:
        texture(p, box, rng or random.Random(1), base)
    if ao:
        shade = QLinearGradient(0, box.top() + box.height() * 0.55, 0, box.bottom())
        shade.setColorAt(0, _c("#000000", 0))
        shade.setColorAt(1, _c("#000000", 110))
        p.fillRect(box, QBrush(shade))
    p.restore()
    if rim:
        p.save()
        p.setClipRect(QRectF(box.left() - 2, box.top() - 2, box.width() * 0.62, box.height() * 0.5))
        p.setPen(_pen(base.lighter(185), 1.3, 150))
        p.setBrush(Qt.NoBrush)
        p.drawPath(path.translated(0.8, 0.8))
        p.restore()
    if outline:
        p.setPen(_pen(base.darker(300), 1.5, 235))
        p.setBrush(Qt.NoBrush)
        p.drawPath(path)


def tex_bark(p, box, rng, base):
    p.setPen(_pen(base.darker(190), 1.0, 170))
    x = box.left() + 2
    while x < box.right():
        path = QPainterPath(QPointF(x, box.top()))
        y, cx = box.top(), x
        while y < box.bottom():
            y += 6
            cx += rng.uniform(-1.4, 1.4)
            path.lineTo(QPointF(cx, y))
        p.drawPath(path)
        x += rng.uniform(3.5, 6.5)
    p.setPen(_pen(base.lighter(150), 0.8, 90))
    for _ in range(int(box.width() * box.height() / 250) + 2):
        x, y = rng.uniform(box.left(), box.right()), rng.uniform(box.top(), box.bottom())
        p.drawLine(QPointF(x, y), QPointF(x, y + rng.uniform(2, 5)))


def tex_speckle(p, box, rng, base):
    p.setPen(Qt.NoPen)
    for _ in range(int(box.width() * box.height() / 18) + 4):
        x, y = rng.uniform(box.left(), box.right()), rng.uniform(box.top(), box.bottom())
        tone = base.lighter(rng.randint(115, 150)) if rng.random() < 0.5 else base.darker(rng.randint(125, 175))
        tone.setAlpha(rng.randint(90, 190))
        p.setBrush(tone)
        s = rng.uniform(0.8, 2.2)
        p.drawEllipse(QRectF(x, y, s, s * 0.8))


def tex_stone(p, box, rng, base):
    tex_speckle(p, box, rng, base)
    p.setPen(_pen(base.darker(230), 0.9, 170))
    for _ in range(int(box.width() / 12) + 1):
        x, y = rng.uniform(box.left(), box.right()), rng.uniform(box.top(), box.bottom())
        a = rng.uniform(0, math.tau)
        for _ in range(3):
            nx, ny = x + math.cos(a) * rng.uniform(3, 7), y + math.sin(a) * rng.uniform(3, 7)
            p.drawLine(QPointF(x, y), QPointF(nx, ny))
            x, y, a = nx, ny, a + rng.uniform(-0.8, 0.8)


def tex_weave(p, box, rng, base):
    p.setPen(_pen(base.darker(140), 0.8, 150))
    step = 3.2
    x = box.left()
    while x < box.right():
        p.drawLine(QPointF(x, box.top()), QPointF(x, box.bottom()))
        x += step
    p.setPen(_pen(base.lighter(130), 0.8, 130))
    y = box.top()
    while y < box.bottom():
        p.drawLine(QPointF(box.left(), y), QPointF(box.right(), y))
        y += step


def tex_veins(p, box, rng, base):
    p.setPen(_pen(base.darker(170), 0.9, 150))
    for _ in range(4):
        x, y = rng.uniform(box.left(), box.right()), box.top()
        path = QPainterPath(QPointF(x, y))
        while y < box.bottom():
            y += rng.uniform(3, 6)
            x += rng.uniform(-3, 3)
            path.lineTo(QPointF(x, y))
        p.drawPath(path)


# -- small parts ----------------------------------------------------------------

def _shadow(p, x, y, rx, ry, alpha=120):
    g = QRadialGradient(QPointF(x, y), rx)
    g.setColorAt(0, _c("#0a0604", alpha))
    g.setColorAt(1, _c("#0a0604", 0))
    p.save()
    p.translate(x, y)
    p.scale(1.0, ry / rx)
    p.translate(-x, -y)
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), rx, rx)
    p.restore()


def _glow(p, x, y, r, color, alpha=120):
    g = QRadialGradient(x, y, r)
    g.setColorAt(0, _c(color, alpha))
    g.setColorAt(1, _c(color, 0))
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), r, r)


def _spec(p, x, y, rx, ry, alpha=200):
    """A specular highlight: what makes a round thing look glossy."""
    g = QRadialGradient(QPointF(x, y), rx)
    g.setColorAt(0, _c("#ffffff", alpha))
    g.setColorAt(1, _c("#ffffff", 0))
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), rx, ry)


def _egg(p, x, y, rx, ry, base, rng, glossy=True):
    path = QPainterPath()
    path.addEllipse(QPointF(x, y), rx, ry)
    _lit(p, path, base, rng, tex_veins, rim=False)
    if glossy:
        _spec(p, x - rx * 0.35, y - ry * 0.4, rx * 0.45, ry * 0.3)


def _pebble(p, x, y, w, rng, tone="#7d7466"):
    path = _blob(x, y, w, w * 0.62, rng, 0.18, 9)
    _shadow(p, x + 1, y + w * 0.5, w * 1.1, w * 0.35, 100)
    _lit(p, path, tone, rng, tex_speckle, rim=False)
    _spec(p, x - w * 0.3, y - w * 0.25, w * 0.35, w * 0.2, 120)


def _tuft(p, x, y, rng, h=9):
    for _ in range(rng.randint(4, 7)):
        dx = rng.uniform(-4, 4)
        top = QPointF(x + dx + rng.uniform(-3, 3), y - h * rng.uniform(0.6, 1.2))
        blade = _poly((x + dx - 1.2, y), (top.x(), top.y()), (x + dx + 1.2, y))
        p.fillPath(blade, _c(rng.choice(["#6f8f45", "#88a653", "#5b7a3a", "#9ab562"])))


def _twig(p, a, b, width, color="#6a4526", rng=None):
    """A branch: bark-textured, outlined, lit on one side."""
    angle = math.atan2(b.y() - a.y(), b.x() - a.x())
    nx, ny = -math.sin(angle) * width / 2, math.cos(angle) * width / 2
    path = _poly((a.x() + nx, a.y() + ny), (b.x() + nx * 0.8, b.y() + ny * 0.8),
                 (b.x() - nx * 0.8, b.y() - ny * 0.8), (a.x() - nx, a.y() - ny))
    _lit(p, path, color, rng or random.Random(3), tex_bark, ao=False)


def _lashing(p, x, y, w=6):
    p.setPen(_pen("#d8c9a4", 1.4))
    for i in range(3):
        p.drawLine(QPointF(x - w / 2, y - 2 + i * 1.8), QPointF(x + w / 2, y - 1 + i * 1.8))


def _thread_ball(p, x, y, w, h, rng):
    path = QPainterPath()
    path.addEllipse(QRectF(x - w / 2, y - h / 2, w, h))
    _lit(p, path, "#e9e2cf", rng, None, light=110)
    p.save()
    p.setClipPath(path)
    p.setPen(_pen("#a89f88", 0.9, 200))
    for i in range(7):
        a = rng.uniform(-0.9, 0.9)
        p.drawLine(QPointF(x - w, y - h + i * h / 3.5 + a * 4), QPointF(x + w, y - h / 2 + i * h / 3.5 - a * 4))
    p.restore()
    _spec(p, x - w * 0.2, y - h * 0.25, w * 0.25, h * 0.18, 150)


def _lantern(p, x, y, colour, rng):
    _glow(p, x, y, 22, colour, 150)
    body = QPainterPath()
    body.addRoundedRect(QRectF(x - 5, y - 6, 10, 12), 3, 3)
    _lit(p, body, colour, rng, None, light=150, rim=False)
    p.setPen(_pen("#3a2a18", 1.3))
    p.drawLine(QPointF(x - 5.5, y - 6), QPointF(x + 5.5, y - 6))
    p.drawLine(QPointF(x - 5.5, y + 6), QPointF(x + 5.5, y + 6))
    p.drawLine(QPointF(x, y - 10), QPointF(x, y - 6))
    _spec(p, x - 1.5, y - 2, 2.5, 3.5, 230)


def _crystal(p, x, y, h, colour, rng, lean=0.0):
    """A faceted crystal: a light face and a dark face, not a circle."""
    w = h * 0.42
    tip = (x + lean * h, y - h)
    left = _poly((x - w, y), (x - w * 0.9, y - h * 0.6), tip, (x, y - h * 0.15), (x, y))
    right = _poly((x, y), (x, y - h * 0.15), tip, (x + w * 0.9, y - h * 0.62), (x + w, y))
    _lit(p, left, _c(colour).lighter(125), rng, None, ao=False, rim=False)
    _lit(p, right, _c(colour).darker(135), rng, None, ao=False, rim=False)
    p.setPen(_pen("#fff6d8", 1.0, 190))
    p.drawLine(QPointF(x - w * 0.55, y - h * 0.35), QPointF(tip[0] - w * 0.1, tip[1] + h * 0.2))


def _pennant(p, owned):
    """Who holds it: green with a spider when yours, rust with thorns when not."""
    _twig(p, QPointF(210, GROUND_Y + 4), QPointF(210, 84), 3.2, "#8a6a44")
    colour = "#6fc4a2" if owned else "#d9784e"
    flag = QPainterPath(QPointF(211.5, 88))
    flag.cubicTo(QPointF(222, 86), QPointF(228, 94), QPointF(238, 92))
    flag.cubicTo(QPointF(232, 98), QPointF(230, 104), QPointF(236, 110))
    flag.cubicTo(QPointF(226, 108), QPointF(220, 106), QPointF(211.5, 108))
    flag.closeSubpath()
    _lit(p, flag, colour, None, None, ao=False)
    mark = "#1d2a24" if owned else "#3a1810"
    p.setPen(Qt.NoPen)
    p.setBrush(_c(mark, 220))
    p.drawEllipse(QPointF(222, 98), 2.6, 2.2)
    p.setPen(_pen(mark, 0.9, 220))
    for k in (-1, 1):
        for dy in (-2, 0, 2):
            p.drawLine(QPointF(222, 98), QPointF(222 + k * 5, 98 + dy + k * 0.5))
    p.setPen(Qt.NoPen)
    p.setBrush(_c("#f4d27a"))
    p.drawEllipse(QPointF(210, 83), 2.4, 2.4)


# -- the base every building stands on ---------------------------------------------

def _plinth(p, rng, tone):
    """A raised slab of earth: soil face, grassy top, moss, pebbles, shadow."""
    _shadow(p, 122, GROUND_Y + 22, 112, 20, 150)
    top = _blob(120, GROUND_Y + 2, 101, 25, rng, 0.07, 26)
    face = QPainterPath(top)
    face.translate(0, 12)
    side = face.united(QPainterPath(top))
    soil = QLinearGradient(0, GROUND_Y, 0, GROUND_Y + 40)
    soil.setColorAt(0, _c("#5a3f26"))
    soil.setColorAt(1, _c("#2a1c10"))
    p.fillPath(side, QBrush(soil))
    p.save()
    p.setClipPath(side)
    p.setPen(_pen("#7a5a3a", 1.0, 150))
    for k in range(3):                                  # strata
        y = GROUND_Y + 18 + k * 5
        path = QPainterPath(QPointF(16, y))
        for x in range(16, 230, 12):
            path.lineTo(QPointF(x, y + math.sin(x * 0.11 + k) * 1.4))
        p.drawPath(path)
    for _ in range(10):                                 # stones in the soil
        x, y = rng.uniform(26, 214), GROUND_Y + rng.uniform(18, 34)
        p.setPen(Qt.NoPen)
        p.setBrush(_c(rng.choice(["#8a7d6a", "#6e6254", "#9c8f7a"]), 220))
        p.drawEllipse(QPointF(x, y), rng.uniform(1.5, 3.2), rng.uniform(1.2, 2.2))
    p.setPen(_pen("#3b2716", 1.2, 200))
    for _ in range(5):                                  # roots
        x = rng.uniform(30, 210)
        path = QPainterPath(QPointF(x, GROUND_Y + 16))
        path.cubicTo(QPointF(x + 4, GROUND_Y + 24), QPointF(x - 3, GROUND_Y + 30), QPointF(x + 2, GROUND_Y + 38))
        p.drawPath(path)
    p.restore()
    p.setPen(_pen("#1e140a", 1.5, 220))
    p.setBrush(Qt.NoBrush)
    p.drawPath(side)
    _lit(p, top, tone, rng, tex_speckle, ao=False, light=128)
    for _ in range(8):                                  # moss
        a = rng.uniform(0, math.tau)
        x, y = 120 + math.cos(a) * rng.uniform(50, 88), GROUND_Y + 2 + math.sin(a) * rng.uniform(10, 19)
        moss = _blob(x, y, rng.uniform(7, 14), rng.uniform(3, 6), rng, 0.3, 10)
        _lit(p, moss, rng.choice(["#5d7a3e", "#6f8e48", "#4e6a36"]), rng, tex_speckle, outline=False, rim=False,
             ao=False)
    for _ in range(4):
        a = rng.uniform(0.2, math.pi - 0.2)
        _pebble(p, 120 + math.cos(a) * rng.uniform(60, 90), GROUND_Y + 4 + math.sin(a) * 16,
                rng.uniform(3, 5.5), rng)
    for _ in range(12):                                 # grass along the front edge
        a = rng.uniform(0.1, math.pi - 0.1)
        _tuft(p, 120 + math.cos(a) * 96, GROUND_Y + 3 + math.sin(a) * 23, rng, rng.uniform(6, 11))
    for _ in range(5):
        a = rng.uniform(math.pi + 0.3, math.tau - 0.3)
        _tuft(p, 120 + math.cos(a) * 92, GROUND_Y + 3 + math.sin(a) * 21, rng, rng.uniform(5, 8))


# -- the buildings ---------------------------------------------------------------

def _home(p, rng, owned):
    mound = QPainterPath(QPointF(40, GROUND_Y + 6))
    mound.cubicTo(QPointF(40, 50), QPointF(200, 44), QPointF(202, GROUND_Y + 6))
    mound.closeSubpath()
    _lit(p, mound, "#8a6a44", rng, tex_stone)
    for _ in range(4):                                  # roots over the mound
        x = rng.uniform(60, 180)
        root = QPainterPath(QPointF(x, 62 + abs(x - 120) * 0.25))
        root.cubicTo(QPointF(x - 12, 90), QPointF(x + 10, 110), QPointF(x - 6, GROUND_Y + 2))
        p.setPen(_pen("#3a2614", 3.2, 230))
        p.drawPath(root)
        p.setPen(_pen("#6d4a2a", 1.6))
        p.drawPath(root)
    for i in range(9):                                  # the stone arch
        a = math.pi + i * math.pi / 8
        x, y = 121 + math.cos(a) * 32, GROUND_Y + 4 + math.sin(a) * 38
        _lit(p, _blob(x, y, 7.5, 6, rng, 0.15, 8), "#9a9080", rng, tex_stone)
    door = QPainterPath(QPointF(95, GROUND_Y + 5))
    door.cubicTo(QPointF(95, 88), QPointF(147, 88), QPointF(147, GROUND_Y + 5))
    door.closeSubpath()
    g = QRadialGradient(121, GROUND_Y - 4, 36)
    g.setColorAt(0, _c("#000000"))
    g.setColorAt(1, _c("#2a1a0e"))
    p.fillPath(door, QBrush(g))
    p.save()
    p.setClipPath(door)
    p.setPen(_pen("#f1ead6", 0.9, 170))                 # silk curtain
    for i in range(11):
        x = 98 + i * 4.8
        p.drawLine(QPointF(x, 96), QPointF(x + math.sin(i) * 3, GROUND_Y + 5))
    p.restore()
    for x, h in ((66, 26), (176, 22)):                  # mushroom lanterns
        stem = QPainterPath()
        stem.addRoundedRect(QRectF(x - 3, GROUND_Y - h + 8, 6, h - 6), 2, 2)
        _lit(p, stem, "#e8dcc2", rng, None, rim=False)
        _glow(p, x, GROUND_Y - h + 6, 20, "#ffb95a", 130)
        cap = QPainterPath(QPointF(x - 13, GROUND_Y - h + 9))
        cap.cubicTo(QPointF(x - 12, GROUND_Y - h - 8), QPointF(x + 12, GROUND_Y - h - 8),
                    QPointF(x + 13, GROUND_Y - h + 9))
        cap.closeSubpath()
        _lit(p, cap, "#d8702e", rng, None)
        p.setPen(Qt.NoPen)
        p.setBrush(_c("#fbe3b8"))
        for dx, dy in ((-6, -2), (2, -5), (6, 1)):
            p.drawEllipse(QPointF(x + dx, GROUND_Y - h + 3 + dy), 1.8, 1.4)
        p.setPen(_pen("#ffd08a", 1.0, 200))
        p.drawLine(QPointF(x - 11, GROUND_Y - h + 9.5), QPointF(x + 11, GROUND_Y - h + 9.5))


def _food(p, rng, owned):
    for a, b in ((QPointF(62, GROUND_Y + 4), QPointF(70, 66)), (QPointF(180, GROUND_Y + 4), QPointF(172, 66))):
        _twig(p, a, b, 5, rng=rng)
        _lashing(p, b.x(), b.y() + 6)
    leaf = QPainterPath(QPointF(40, 72))
    leaf.cubicTo(QPointF(76, 30), QPointF(170, 26), QPointF(204, 70))
    leaf.cubicTo(QPointF(170, 60), QPointF(80, 62), QPointF(40, 72))
    _lit(p, leaf, "#7a9a4a", rng, None)
    p.setPen(_pen("#4a6a2c", 1.6))
    p.drawLine(QPointF(44, 70), QPointF(200, 68))
    p.setPen(_pen("#4a6a2c", 1.0, 200))
    for i in range(9):                                  # veins
        x = 58 + i * 16
        p.drawLine(QPointF(x, 69), QPointF(x + 8, 44 + abs(x - 122) * 0.18))
    for x, y, w, h in ((96, GROUND_Y - 14, 26, 18), (124, GROUND_Y - 10, 30, 20), (110, GROUND_Y - 30, 22, 16)):
        _shadow(p, x + 2, y + h / 2, w * 0.7, 4, 100)
        _thread_ball(p, x, y, w, h, rng)
    for x, y in ((76, GROUND_Y - 6), (82, GROUND_Y - 11), (86, GROUND_Y - 4)):   # berries
        berry = QPainterPath()
        berry.addEllipse(QPointF(x, y), 4.5, 4.5)
        _lit(p, berry, "#b8262a", rng, None, rim=False)
        _spec(p, x - 1.5, y - 1.5, 1.8, 1.4, 240)
    p.setPen(_pen("#141414", 1.2))                      # a beetle
    for k in (-1, 1):
        for dx in (-5, 0, 5):
            p.drawLine(QPointF(160 + dx, GROUND_Y - 4), QPointF(160 + dx + k * 3, GROUND_Y - 4 + k * 10))
    shell = QPainterPath()
    shell.addEllipse(QPointF(160, GROUND_Y - 4), 11, 7.5)
    _lit(p, shell, "#2c4a5a", rng, None)
    p.setPen(_pen("#0c1820", 1.0))
    p.drawLine(QPointF(160, GROUND_Y - 11), QPointF(160, GROUND_Y + 3))
    _spec(p, 156, GROUND_Y - 7, 4, 2.2, 200)


def _silk(p, rng, owned):
    for a, b in ((QPointF(66, GROUND_Y + 4), QPointF(64, 42)), (QPointF(176, GROUND_Y + 4), QPointF(178, 42))):
        _twig(p, a, b, 6, rng=rng)
    _twig(p, QPointF(56, 48), QPointF(186, 48), 6, rng=rng)
    _twig(p, QPointF(62, 108), QPointF(180, 108), 5, rng=rng)
    for x in (64, 178):
        _lashing(p, x, 48)
        _lashing(p, x, 108)
    p.setPen(_pen("#f2ecdc", 0.9, 220))                 # warp
    for i in range(20):
        x = 74 + i * 5.1
        p.drawLine(QPointF(x, 51), QPointF(x, 105))
    panel = QPainterPath()
    panel.addRect(QRectF(74, 72, 98, 32))
    _lit(p, panel, "#e7e0cb", rng, tex_weave, light=110)
    for x in (86, 118, 150):                            # spools
        spool = QPainterPath()
        spool.addRoundedRect(QRectF(x - 9, GROUND_Y - 22, 18, 20), 3, 3)
        _lit(p, spool, "#efe8d6", rng, tex_weave, light=112)
        for y in (GROUND_Y - 24, GROUND_Y - 3):
            end = QPainterPath()
            end.addRoundedRect(QRectF(x - 11, y, 22, 4), 2, 2)
            _lit(p, end, "#7a5230", rng, None, rim=False)


def _hatchery(p, rng, owned):
    arch = QPainterPath(QPointF(56, GROUND_Y + 4))
    arch.cubicTo(QPointF(52, 34), QPointF(188, 34), QPointF(184, GROUND_Y + 4))
    p.setPen(_pen("#2a1a0c", 9))
    p.drawPath(arch)
    p.setPen(_pen("#6a4526", 6.5))
    p.drawPath(arch)
    p.setPen(_pen("#9a7450", 1.6, 170))
    p.drawPath(arch.translated(-1.5, -1.5))
    p.setPen(_pen("#e8e0cc", 1.1, 220))                 # hanging threads
    for x in (92, 120, 148):
        p.drawLine(QPointF(x, 50), QPointF(x, 72))
    if owned:
        bundle = _blob(120, 100, 48, 36, rng, 0.06)
        _lit(p, bundle, "#bdb6a4", rng, tex_weave, light=120)
        p.setPen(_pen("#857d6a", 2.2))
        for i in range(5):
            p.drawLine(QPointF(76, 78 + i * 11), QPointF(164, 70 + i * 13))
        seal = QPainterPath()
        seal.addEllipse(QPointF(120, 102), 11, 11)
        _lit(p, seal, "#a62024", rng, None)
        p.setPen(_pen("#f0c0a0", 1.4))
        p.drawLine(QPointF(114, 102), QPointF(126, 102))
        p.drawLine(QPointF(120, 96), QPointF(120, 108))
        _spec(p, 116, 98, 3, 2, 200)
    else:
        for x, y, rx, ry in ((104, 96, 17, 20), (136, 94, 18, 21), (120, 78, 15, 17), (88, 116, 13, 15),
                             (152, 116, 13, 15), (120, 112, 16, 18)):
            _egg(p, x, y, rx, ry, "#a8c47a", rng)
        _glow(p, 120, 100, 44, "#d8ff9a", 40)
        broken = _poly((150, GROUND_Y - 6), (160, GROUND_Y - 16), (166, GROUND_Y - 8), (172, GROUND_Y - 18),
                       (176, GROUND_Y - 4))
        _lit(p, broken, "#dfe8c8", rng, None, rim=False)


def _nest(p, rng, owned):
    dome = QPainterPath(QPointF(34, GROUND_Y + 6))
    dome.cubicTo(QPointF(34, 36), QPointF(206, 36), QPointF(206, GROUND_Y + 6))
    dome.closeSubpath()
    _lit(p, dome, "#4a2c36", rng, tex_stone, light=140)
    for _ in range(9):                                  # brambles
        start = QPointF(rng.uniform(40, 200), GROUND_Y + rng.uniform(-2, 6))
        top = QPointF(rng.uniform(70, 170), rng.uniform(40, 76))
        end = QPointF(rng.uniform(40, 200), GROUND_Y + rng.uniform(-10, 4))
        path = QPainterPath(start)
        path.quadTo(top, end)
        p.setPen(_pen("#120a0c", 6))
        p.drawPath(path)
        p.setPen(_pen(rng.choice(["#6a3a44", "#7a4640", "#5a3446"]), 3.6))
        p.drawPath(path)
        p.setPen(_pen("#b07a70", 1.0, 150))
        p.drawPath(path.translated(-0.8, -0.8))
        for t in (0.2, 0.4, 0.6, 0.8):
            pt = path.pointAtPercent(t)
            a = math.radians(-path.angleAtPercent(t) + rng.choice((90, -90)))
            thorn = _poly((pt.x() - 2, pt.y()), (pt.x() + math.cos(a) * 7, pt.y() + math.sin(a) * 7),
                          (pt.x() + 2, pt.y()))
            _lit(p, thorn, "#d8c0a8", rng, None, outline=False, ao=False, rim=False)
    for x, y, h, lean in ((50, 70, 30, -0.25), (84, 44, 26, -0.1), (156, 42, 28, 0.1), (192, 70, 30, 0.25)):
        _lit(p, _poly((x - 7, y + h), (x + lean * h, y), (x + 7, y + h)), "#e8d6c0", rng, None)
    maw = QPainterPath(QPointF(88, GROUND_Y + 2))
    maw.cubicTo(QPointF(88, 82), QPointF(154, 82), QPointF(154, GROUND_Y + 2))
    maw.closeSubpath()
    g = QRadialGradient(121, 122, 40)
    g.setColorAt(0, _c("#000000"))
    g.setColorAt(1, _c("#2a1016"))
    p.fillPath(maw, QBrush(g))
    for i in range(7):                                  # teeth
        x = 92 + i * 9.5
        drop = abs(x - 121)
        tooth = _poly((x - 3.5, 92 + drop * 0.28), (x, 102 + drop * 0.2), (x + 3.5, 92 + drop * 0.28))
        _lit(p, tooth, "#efe2c8", rng, None, rim=False, ao=False)
    p.setPen(_pen("#8a5a62", 2))
    p.setBrush(Qt.NoBrush)
    p.drawPath(maw)
    if owned:
        p.setPen(_pen("#f2ecdc", 1.2, 235))
        for i in range(9):
            p.drawLine(QPointF(92 + i * 7, 96), QPointF(150 - i * 6.5, GROUND_Y))
        p.drawLine(QPointF(90, 112), QPointF(152, 108))
        p.drawLine(QPointF(90, 124), QPointF(152, 126))
    else:
        for x in (110, 132):
            _glow(p, x, 118, 12, "#ff3b2f", 170)
            eye = QPainterPath()
            eye.addEllipse(QPointF(x, 118), 3.6, 2.4)
            _lit(p, eye, "#ff5a3a", rng, None, rim=False, ao=False)
            _spec(p, x - 1, 117, 1.2, 0.8, 255)


def _outpost(p, rng, owned):
    for x0, x1 in ((54, 102), (186, 138)):
        _twig(p, QPointF(x0, GROUND_Y + 4), QPointF(x1, 46), 4.5, rng=rng)
    tent = QPainterPath(QPointF(46, GROUND_Y + 3))
    tent.quadTo(QPointF(84, 72), QPointF(120, 48))
    tent.quadTo(QPointF(156, 72), QPointF(194, GROUND_Y + 3))
    tent.closeSubpath()
    _lit(p, tent, "#c8b48e", rng, tex_speckle, light=125)
    p.setPen(_pen("#7a6444", 1.1, 200))                 # seams and stitches
    for x in (84, 120, 156):
        p.drawLine(QPointF(120, 50), QPointF(x + (x - 120) * 0.3, GROUND_Y + 2))
    for i in range(12):
        t = i / 12
        x, y = 84 + (120 - 84) * t, GROUND_Y + 2 - (GROUND_Y + 2 - 52) * t
        p.drawLine(QPointF(x - 2, y), QPointF(x + 2, y + 1))
    door = QPainterPath(QPointF(100, GROUND_Y + 3))
    door.quadTo(QPointF(120, 80), QPointF(140, GROUND_Y + 3))
    door.closeSubpath()
    p.fillPath(door, _c("#140c06"))
    p.setPen(_pen("#d8c9a4", 1.0, 200))                 # guy ropes
    p.drawLine(QPointF(70, 96), QPointF(30, GROUND_Y + 10))
    p.drawLine(QPointF(170, 96), QPointF(212, GROUND_Y + 8))
    _lantern(p, 120, 40, "#7de0a8" if owned else "#ff9a3c", rng)


def _infestation(p, rng, owned):
    frame = QPainterPath()
    frame.addRoundedRect(QRectF(34, 56, 172, GROUND_Y - 50), 7, 7)
    _lit(p, frame, "#4c5a66", rng, None, light=150)
    pane = QPainterPath()
    pane.addRoundedRect(QRectF(41, 63, 158, GROUND_Y - 64), 4, 4)
    g = QLinearGradient(41, 63, 199, GROUND_Y)
    g.setColorAt(0, _c("#3a6a94"))
    g.setColorAt(1, _c("#0e2238"))
    p.fillPath(pane, QBrush(g))
    p.save()
    p.setClipPath(pane)
    p.setPen(_pen("#d9f0ff", 1.1, 190))
    for _ in range(8):
        x, y = rng.uniform(60, 180), rng.uniform(70, 125)
        a = rng.uniform(0, math.tau)
        for _step in range(5):
            nx, ny = x + math.cos(a) * 11, y + math.sin(a) * 11
            p.drawLine(QPointF(x, y), QPointF(nx, ny))
            x, y, a = nx, ny, a + rng.uniform(-0.6, 0.6)
    p.fillRect(QRectF(41, 63, 158, 14), _c("#ffffff", 30))
    p.restore()
    for x, y, rx, ry in ((96, 106, 20, 18), (142, 102, 24, 20), (120, 80, 17, 15), (72, 120, 14, 12),
                         (168, 120, 14, 12)):
        if owned:
            _egg(p, x, y, rx * 0.85, ry * 0.8, "#6b6f66", rng, glossy=False)
            p.setPen(_pen("#262822", 1.8))
            p.drawLine(QPointF(x - rx * 0.4, y - ry * 0.3), QPointF(x + rx * 0.2, y + ry * 0.3))
        else:
            _glow(p, x, y, rx * 1.6, "#9dff3a", 70)
            _egg(p, x, y, rx, ry, "#7ccf2a", rng)
    if not owned:
        _lit(p, _blob(120, GROUND_Y + 2, 40, 7, rng, 0.2), "#8ae63a", rng, None, rim=False, ao=False)
        p.setPen(_pen("#b8ff5a", 2.6, 220))
        p.setBrush(_c("#b8ff5a"))
        for x in (92, 124, 150):
            p.drawLine(QPointF(x, 116), QPointF(x, GROUND_Y - 2))
            p.drawEllipse(QPointF(x, GROUND_Y - 1), 2.4, 2.8)


def _venom(p, rng, owned):
    stump = QPainterPath(QPointF(60, GROUND_Y + 5))
    stump.lineTo(QPointF(70, 66))
    stump.quadTo(QPointF(120, 58), QPointF(170, 66))
    stump.lineTo(QPointF(180, GROUND_Y + 5))
    stump.quadTo(QPointF(120, GROUND_Y + 12), QPointF(60, GROUND_Y + 5))
    _lit(p, stump, "#6a4a30", rng, tex_bark)
    for x, k in ((58, 1), (182, -1)):                   # root flares
        flare = _poly((x, GROUND_Y + 6), (x + k * 10, GROUND_Y - 16), (x + k * 22, GROUND_Y + 6))
        _lit(p, flare, "#5a3e28", rng, tex_bark, rim=False)
    top = QPainterPath()
    top.addEllipse(QRectF(70, 57, 100, 20))
    _lit(p, top, "#b89468", rng, None, rim=False, ao=False)
    p.setPen(_pen("#7a5a3a", 1.0, 200))
    p.setBrush(Qt.NoBrush)
    for r in (40, 30, 20, 10):                          # growth rings
        p.drawEllipse(QRectF(120 - r, 67 - r * 0.2, r * 2, r * 0.4))
    hollow = QPainterPath()
    hollow.addEllipse(QRectF(98, 62, 44, 10))
    p.fillPath(hollow, _c("#1a0e08"))
    colour = "#7fe06a" if owned else "#b04ad8"
    _glow(p, 120, 98, 32, colour, 140)
    _egg(p, 120, 98, 16, 18, colour, rng)
    p.setPen(_pen(colour, 3, 230))
    for x, length in ((112, 24), (127, 32)):
        p.drawLine(QPointF(x, 110), QPointF(x, 110 + length))
    _lit(p, _blob(120, GROUND_Y + 4, 28, 6, rng, 0.2), colour, rng, None, rim=False, ao=False)
    _spec(p, 112, GROUND_Y + 2, 8, 2, 150)


def _lookout(p, rng, owned):
    for x0, x1 in ((74, 100), (166, 140)):
        _twig(p, QPointF(x0, GROUND_Y + 4), QPointF(x1, 52), 5.5, rng=rng)
    for y, x0, x1 in ((114, 80, 160), (86, 90, 150)):
        _twig(p, QPointF(x0, y), QPointF(x1, y), 3.5, rng=rng)
        _lashing(p, x0 + 3, y)
        _lashing(p, x1 - 3, y)
    _twig(p, QPointF(84, 114), QPointF(146, 86), 2.5, rng=rng)
    _twig(p, QPointF(158, GROUND_Y + 4), QPointF(166, 88), 2.2, rng=rng)
    _twig(p, QPointF(176, GROUND_Y + 4), QPointF(180, 88), 2.2, rng=rng)
    p.setPen(_pen("#3a2614", 2.6))
    for i in range(6):                                  # ladder rungs
        y = GROUND_Y - 4 - i * 9
        p.drawLine(QPointF(160 + i * 1.2, y), QPointF(176 + i * 0.6, y))
    deck = QPainterPath()
    deck.addRoundedRect(QRectF(86, 46, 68, 10), 2, 2)
    _lit(p, deck, "#9a7450", rng, tex_bark)
    p.setPen(_pen("#3a2614", 1.0, 200))
    for x in range(92, 152, 9):
        p.drawLine(QPointF(x, 47), QPointF(x, 55))
    for x in (90, 150):
        _twig(p, QPointF(x, 46), QPointF(x, 30), 2.4, rng=rng)
    p.setPen(_pen("#d8c9a4", 1.2))
    p.drawLine(QPointF(90, 34), QPointF(150, 34))
    _lantern(p, 120, 26, "#7de0a8" if owned else "#ff9a3c", rng)


def _amber(p, rng, owned):
    pit = QPainterPath()
    pit.addEllipse(QRectF(56, 98, 128, 44))
    g = QRadialGradient(120, 124, 70)
    g.setColorAt(0, _c("#1a0f06"))
    g.setColorAt(1, _c("#4a3218"))
    p.fillPath(pit, QBrush(g))
    rim = [(120 + math.cos(i * math.tau / 14) * 64, 120 + math.sin(i * math.tau / 14) * 22, i) for i in range(14)]
    for x, y, i in rim:                                 # the back of the rim
        if math.sin(i * math.tau / 14) < 0:
            _lit(p, _blob(x, y, 9, 6, rng, 0.2, 9), "#8a8070", rng, tex_stone)
    _glow(p, 120, 116, 50, "#ffb020", 90)
    for x, y, h, lean in ((96, 128, 26, -0.2), (112, 124, 36, 0.05), (130, 130, 24, 0.25), (146, 126, 30, 0.12),
                          (82, 132, 16, -0.3), (160, 134, 16, 0.35)):
        _crystal(p, x, y, h, "#e8961c", rng, lean)
    for x, y, i in rim:                                 # the front of the rim, over the crystals' feet
        if math.sin(i * math.tau / 14) >= 0.2:
            _lit(p, _blob(x, y, 9, 6, rng, 0.2, 9), "#8a8070", rng, tex_stone)
    _twig(p, QPointF(176, 70), QPointF(196, 122), 4, rng=rng)
    _lit(p, _poly((164, 64), (190, 70), (186, 76), (170, 74), (158, 80)), "#9aa0a8", rng, tex_stone)
    if owned:
        for x, y, h in ((62, 92, 14), (72, 88, 10)):
            _crystal(p, x, y, h, "#f2b33a", rng)


def _nursery(p, rng, owned):
    _twig(p, QPointF(120, 60), QPointF(120, GROUND_Y + 3), 3.5, rng=rng)
    leaf = QPainterPath(QPointF(44, 70))
    leaf.cubicTo(QPointF(80, 26), QPointF(160, 26), QPointF(196, 70))
    leaf.cubicTo(QPointF(160, 58), QPointF(80, 58), QPointF(44, 70))
    _lit(p, leaf, "#5f8a42", rng, None)
    p.setPen(_pen("#3a5a26", 1.4))
    p.drawLine(QPointF(48, 68), QPointF(192, 68))
    p.setPen(_pen("#3a5a26", 0.9, 200))
    for i in range(8):
        x = 60 + i * 17
        p.drawLine(QPointF(x, 67), QPointF(x + 7, 44 + abs(x - 120) * 0.2))
    p.setPen(_pen("#f4efe0", 1.0, 210))                 # the cradle's threads
    for i in range(9):
        p.drawLine(QPointF(70 + i * 12, 70), QPointF(92 + i * 7, 122))
    cradle = QPainterPath(QPointF(70, 96))
    cradle.cubicTo(QPointF(76, 140), QPointF(164, 140), QPointF(170, 96))
    cradle.closeSubpath()
    _lit(p, cradle, "#e6dfcc", rng, tex_weave, light=112)
    for i in range(12):
        _egg(p, 86 + (i % 6) * 13 + (i // 6) * 6, 106 + (i // 6) * 11, 5.5, 5, "#efe4c0", rng)
    if owned:
        for x in (82, 156):                             # spiderlings
            p.setPen(_pen("#2b1a10", 1.2))
            for k in (-1, 1):
                for dy in (-3, 0, 3):
                    p.drawLine(QPointF(x, GROUND_Y - 6), QPointF(x + k * 8, GROUND_Y - 6 + dy + k))
            body = QPainterPath()
            body.addEllipse(QPointF(x, GROUND_Y - 6), 5, 4)
            _lit(p, body, "#7a4028", rng, None, rim=False)
            _spec(p, x - 1.5, GROUND_Y - 8, 1.8, 1.2, 220)


def _flynest(p, rng, owned):
    fruit = _blob(120, 102, 46, 38, rng, 0.05)
    _lit(p, fruit, "#9a5a32", rng, tex_speckle, light=140)
    bite = QPainterPath()
    bite.addEllipse(QPointF(160, 88), 16, 14)
    eaten = bite.intersected(fruit)
    p.fillPath(eaten, _c("#e6c89a"))
    p.setPen(_pen("#6a3a1c", 1.2))
    p.setBrush(Qt.NoBrush)
    p.drawPath(eaten)
    for _ in range(6):                                  # rot
        spot = _blob(rng.uniform(92, 142), rng.uniform(90, 124), rng.uniform(4, 9), rng.uniform(3, 6), rng, 0.3, 9)
        _lit(p, spot, "#3d2a12", rng, None, rim=False, ao=False, outline=False)
    _twig(p, QPointF(118, 66), QPointF(124, 50), 3, rng=rng)
    leaf = QPainterPath(QPointF(124, 56))
    leaf.cubicTo(QPointF(136, 42), QPointF(154, 44), QPointF(160, 50))
    leaf.cubicTo(QPointF(150, 58), QPointF(136, 60), QPointF(124, 56))
    _lit(p, leaf, "#6a8a3a", rng, None)
    _spec(p, 102, 84, 14, 8, 90)
    for _ in range(14):                                 # flies: body and wings, not dots
        a = rng.uniform(0, math.tau)
        r = rng.uniform(52, 80)
        x, y = 120 + math.cos(a) * r, 84 + math.sin(a) * r * 0.55
        p.setPen(Qt.NoPen)
        p.setBrush(_c("#dfe8f0", 150))
        p.drawEllipse(QPointF(x - 2.4, y - 2), 2.6, 1.6)
        p.drawEllipse(QPointF(x + 2.4, y - 2), 2.6, 1.6)
        p.setBrush(_c("#141414"))
        p.drawEllipse(QPointF(x, y), 2.0, 1.4)


PAINTERS = {"home": _home, "food": _food, "silk": _silk, "hatchery": _hatchery, "nest": _nest,
            "outpost": _outpost, "infestation": _infestation, "venom": _venom, "lookout": _lookout,
            "amber": _amber, "nursery": _nursery, "flynest": _flynest}
GROUND_TONES = {"home": "#7a6a42", "food": "#6e7440", "silk": "#6e6a44",
                "hatchery": "#5e6a3c", "nest": "#4a3a34", "outpost": "#6a6040", "infestation": "#3e4a36",
                "venom": "#4e4440", "lookout": "#6a6040", "amber": "#6e5a34", "nursery": "#5e6e3e",
                "flynest": "#5a4a30"}


@lru_cache(maxsize=32)
def building_art(kind, owned):
    """Painted once per building and owner, at twice the size, then reused."""
    image = QImage(ART_W * SCALE, ART_H * SCALE, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    p.scale(SCALE, SCALE)
    rng = random.Random(f"{kind}-37")
    _plinth(p, rng, GROUND_TONES.get(kind, "#6a5a3a"))
    PAINTERS.get(kind, _hatchery)(p, rng, owned)
    _pennant(p, owned)
    p.end()
    image.setDevicePixelRatio(SCALE)
    return image
