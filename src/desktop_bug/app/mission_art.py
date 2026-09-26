"""The territory mission's buildings, painted once and cached.

The owner: *"need nicer designs for these hatchery, enemy base and other. not
just circles."* Each site is drawn into a 240x190 image with Qt only (no
image files), once per building and ownership pair, then blitted every frame:

- Home burrow: an earth mound with a silk-lined tunnel, roots and glowing
  mushroom lanterns;
- Food cache: a veined leaf canopy on twig poles over silk-wrapped prey,
  berries and a beetle;
- Silk loom: a lashed twig frame with taut warp threads, a half-woven panel
  and spools;
- Hatchery: a hanging cradle of glossy egg sacs under a twig arch, one
  hatched; sealed, it is bound in grey silk with a wax seal;
- Thorn nest: a bramble fortress of thorny branches round a maw with red
  eyes; claimed, the eyes go out and silk covers the mouth;
- Outpost (a raid's other screens): a silk den on stakes with a lantern;
  taken, the lantern turns green;
- Infestation (Reclaim the desktop): swollen acid sacs dripping green over
  a cracked pane; destroyed, the sacs are burst and grey.

The anchor is the same as the old art: the site's point sits at (120, 139).
"""
from __future__ import annotations

from functools import lru_cache
import math
import random

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import (QBrush, QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen,
                         QRadialGradient)

ART_W, ART_H = 240, 190
GROUND_Y = 139            # where the site's point is


def _pen(color, width=1.0, alpha=255):
    c = QColor(color)
    c.setAlpha(alpha)
    return QPen(c, width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)


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


def _ground(p, rng, tone):
    """An earthy patch with moss, pebbles and grass instead of flat ellipses."""
    shadow = _blob(120, GROUND_Y + 14, 104, 22, rng, 0.06)
    p.fillPath(shadow, QColor(12, 8, 4, 150))
    patch = _blob(120, GROUND_Y + 4, 100, 26, rng, 0.10)
    grad = QLinearGradient(0, GROUND_Y - 22, 0, GROUND_Y + 30)
    grad.setColorAt(0, QColor(tone).lighter(125))
    grad.setColorAt(1, QColor(tone).darker(160))
    p.fillPath(patch, QBrush(grad))
    p.setPen(_pen("#2a1d10", 1.4, 180))
    p.drawPath(patch)
    for _ in range(40):
        x, y = rng.uniform(32, 208), rng.uniform(GROUND_Y - 14, GROUND_Y + 22)
        if ((x - 120) / 96) ** 2 + ((y - GROUND_Y - 4) / 24) ** 2 > 1:
            continue
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(rng.choice(["#8a7552", "#6f5c3e", "#a08a62", "#4f4230"])))
        w = rng.uniform(2, 5)
        p.drawEllipse(QRectF(x, y, w, w * 0.7))
    for _ in range(9):
        a = rng.uniform(0, math.tau)
        x, y = 120 + math.cos(a) * 88, GROUND_Y + 4 + math.sin(a) * 20
        moss = _blob(x, y, rng.uniform(7, 13), rng.uniform(3, 6), rng, 0.3, 10)
        p.fillPath(moss, QColor(rng.choice(["#5d7442", "#6f8a4d", "#48603a"])))
    for _ in range(16):
        a = rng.uniform(0, math.tau)
        x, y = 120 + math.cos(a) * rng.uniform(80, 98), GROUND_Y + 6 + math.sin(a) * 22
        h = rng.uniform(6, 12)
        p.setPen(_pen(rng.choice(["#7c9a55", "#93ad63", "#5f7c40"]), 1.3))
        p.drawLine(QPointF(x, y), QPointF(x + rng.uniform(-3, 3), y - h))


def _glow(p, x, y, r, color, alpha=120):
    g = QRadialGradient(x, y, r)
    c = QColor(color)
    c.setAlpha(alpha)
    g.setColorAt(0, c)
    c2 = QColor(color)
    c2.setAlpha(0)
    g.setColorAt(1, c2)
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), r, r)


def _sphere(p, x, y, r, base, light="#ffffff", edge=None):
    g = QRadialGradient(x - r * 0.35, y - r * 0.4, r * 1.3)
    g.setColorAt(0, QColor(light))
    g.setColorAt(0.25, QColor(base).lighter(130))
    g.setColorAt(1, QColor(base).darker(170))
    p.setBrush(QBrush(g))
    p.setPen(_pen(edge, 1.0) if edge else Qt.NoPen)
    p.drawEllipse(QPointF(x, y), r, r * 0.92)


def _twig(p, a, b, width, color="#5b3d22"):
    p.setPen(_pen("#24170c", width + 2.4))
    p.drawLine(a, b)
    p.setPen(_pen(color, width))
    p.drawLine(a, b)
    p.setPen(_pen(QColor(color).lighter(150), max(1.0, width * 0.25), 160))
    p.drawLine(QPointF(a.x() - width * 0.2, a.y()), QPointF(b.x() - width * 0.2, b.y()))


def _silk_bundle(p, x, y, w, h, rng):
    g = QRadialGradient(x - w * 0.2, y - h * 0.3, w)
    g.setColorAt(0, QColor("#fbf7ec"))
    g.setColorAt(1, QColor("#b9b2a0"))
    p.setBrush(QBrush(g))
    p.setPen(_pen("#8d8674", 1.0))
    p.drawEllipse(QRectF(x - w / 2, y - h / 2, w, h))
    p.setPen(_pen("#e8e2d2", 0.9, 220))
    for i in range(5):
        t = -0.4 + i * 0.2
        p.drawLine(QPointF(x - w / 2 + 2, y + h * t), QPointF(x + w / 2 - 2, y + h * (t + rng.uniform(0.1, 0.3))))


# ------------------------------------------------------------ the buildings

def _home(p, rng, owned):
    mound = QPainterPath(QPointF(38, GROUND_Y + 6))
    mound.cubicTo(QPointF(46, 70), QPointF(98, 44), QPointF(128, 46))
    mound.cubicTo(QPointF(170, 48), QPointF(204, 84), QPointF(204, GROUND_Y + 6))
    mound.closeSubpath()
    g = QLinearGradient(0, 44, 0, GROUND_Y + 6)
    g.setColorAt(0, QColor("#a07446"))
    g.setColorAt(1, QColor("#5a3b1f"))
    p.fillPath(mound, QBrush(g))
    p.setPen(_pen("#2d1d0e", 2))
    p.drawPath(mound)
    for _ in range(26):                       # soil clods
        x, y = rng.uniform(60, 185), rng.uniform(60, GROUND_Y)
        if not mound.contains(QPointF(x, y)):
            continue
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(rng.choice(["#7d5733", "#b08257", "#654427"])))
        p.drawEllipse(QRectF(x, y, rng.uniform(3, 7), rng.uniform(2, 4)))
    for side in (-1, 1):                      # roots
        path = QPainterPath(QPointF(120 + side * 30, 52))
        path.cubicTo(QPointF(120 + side * 60, 60), QPointF(120 + side * 55, 95), QPointF(120 + side * 82, 108))
        p.setPen(_pen("#3b2714", 3.2))
        p.drawPath(path)
        p.setPen(_pen("#8b6640", 1.2))
        p.drawPath(path)
    # The tunnel: dark mouth, silk lining.
    mouth = QRectF(92, 86, 58, 52)
    tg = QRadialGradient(121, 118, 34)
    tg.setColorAt(0, QColor("#07050a"))
    tg.setColorAt(1, QColor("#2b1f18"))
    p.setBrush(QBrush(tg))
    p.setPen(_pen("#d9ccb0", 2.2))
    p.drawChord(mouth, 0, 180 * 16)
    p.drawLine(QPointF(92, 112), QPointF(150, 112))
    p.fillRect(QRectF(93, 112, 56, GROUND_Y - 112), QColor("#0d0a0c"))
    p.setPen(_pen("#eee4cc", 0.8, 200))
    for i in range(9):
        a = math.pi * (0.08 + 0.84 * i / 8)
        x, y = 121 + math.cos(a) * 29, 112 - math.sin(a) * 26
        p.drawLine(QPointF(x, y), QPointF(121 + math.cos(a) * 17, 112 - math.sin(a) * 15))
    # Mushroom lanterns.
    for x, y, s in ((64, 118, 1.0), (178, 122, 0.85)):
        _glow(p, x, y - 6, 22 * s, "#ffb45a", 110)
        p.setPen(_pen("#e7dcc3", 3.5 * s))
        p.drawLine(QPointF(x, y), QPointF(x, y + 16 * s))
        cap = QPainterPath(QPointF(x - 13 * s, y + 1))
        cap.quadTo(QPointF(x, y - 18 * s), QPointF(x + 13 * s, y + 1))
        cap.closeSubpath()
        p.fillPath(cap, QColor("#d9773c"))
        p.setPen(_pen("#ffd79a", 1.0))
        p.drawPath(cap)
        for dx in (-5, 2, 7):
            p.setBrush(QColor("#ffe7b8"))
            p.setPen(Qt.NoPen)
            p.drawEllipse(QPointF(x + dx * s, y - 5 * s), 1.6 * s, 1.3 * s)


def _food(p, rng, owned):
    _twig(p, QPointF(62, GROUND_Y + 4), QPointF(66, 74), 5)
    _twig(p, QPointF(178, GROUND_Y + 4), QPointF(174, 72), 5)
    leaf = QPainterPath(QPointF(34, 82))
    leaf.cubicTo(QPointF(70, 22), QPointF(170, 18), QPointF(208, 80))
    leaf.cubicTo(QPointF(160, 62), QPointF(84, 62), QPointF(34, 82))
    g = QLinearGradient(0, 30, 0, 82)
    g.setColorAt(0, QColor("#8fae5a"))
    g.setColorAt(1, QColor("#4d6d33"))
    p.fillPath(leaf, QBrush(g))
    p.setPen(_pen("#2f4520", 1.8))
    p.drawPath(leaf)
    p.setPen(_pen("#c9d99a", 1.4, 200))
    spine = QPainterPath(QPointF(40, 80))
    spine.quadTo(QPointF(120, 36), QPointF(202, 78))
    p.drawPath(spine)
    for i in range(1, 8):                     # veins
        t = i / 8
        x = 40 + 162 * t
        y = 80 - 44 * math.sin(math.pi * t)
        p.drawLine(QPointF(x, y), QPointF(x - 10, y + 12))
        p.drawLine(QPointF(x, y), QPointF(x + 10, y - 10))
    # The cache beneath.
    for x, y, w, h in ((96, 118, 30, 20), (130, 122, 34, 22), (112, 104, 26, 18)):
        _silk_bundle(p, x, y, w, h, rng)
    for x, y in ((78, 128), (152, 128), (162, 114)):
        _sphere(p, x, y, 8, "#b8322c", "#ffd6c9")
        p.setPen(_pen("#4f6b30", 1.4))
        p.drawLine(QPointF(x, y - 7), QPointF(x + 3, y - 12))
    beetle = QRectF(146, 126, 22, 14)        # a beetle, for the larder
    p.setPen(_pen("#15110c", 1.2))
    for i in range(3):
        p.drawLine(QPointF(150 + i * 6, 130), QPointF(146 + i * 7, 124))
        p.drawLine(QPointF(150 + i * 6, 138), QPointF(146 + i * 7, 144))
    bg = QLinearGradient(146, 126, 168, 140)
    bg.setColorAt(0, QColor("#3a5b6b"))
    bg.setColorAt(1, QColor("#172630"))
    p.setBrush(QBrush(bg))
    p.drawEllipse(beetle)
    p.drawLine(QPointF(157, 126), QPointF(157, 140))


def _silk(p, rng, owned):
    for x in (62, 178):
        _twig(p, QPointF(x, GROUND_Y + 4), QPointF(x + (8 if x < 120 else -8), 44), 7)
    _twig(p, QPointF(58, 52), QPointF(182, 52), 6, "#6b4a2a")
    _twig(p, QPointF(66, 118), QPointF(174, 118), 5, "#6b4a2a")
    for x in (64, 176):                       # lashings
        for dy in (0, 5):
            p.setPen(_pen("#e2d7bd", 1.2))
            p.drawLine(QPointF(x - 6, 49 + dy), QPointF(x + 6, 55 + dy))
    p.setPen(_pen("#eef4ee", 0.9, 230))
    for i in range(14):                       # warp
        x = 74 + i * 7
        p.drawLine(QPointF(x, 56), QPointF(x, 115))
    cloth = QRectF(74, 82, 91, 33)            # the woven part
    cg = QLinearGradient(0, 82, 0, 115)
    cg.setColorAt(0, QColor(236, 240, 232, 150))
    cg.setColorAt(1, QColor(210, 220, 214, 230))
    p.fillRect(cloth, QBrush(cg))
    p.setPen(_pen("#ffffff", 0.7, 170))
    for j in range(8):
        y = 84 + j * 4
        path = QPainterPath(QPointF(74, y))
        for i in range(13):
            path.lineTo(QPointF(80 + i * 7, y + (1.2 if i % 2 else -1.2)))
        p.drawPath(path)
    for i, x in enumerate((92, 120, 148)):   # spools
        body = QRectF(x - 11, 122, 22, 16)
        p.setPen(_pen("#5a3b20", 1.2))
        p.setBrush(QColor("#8c6238"))
        p.drawRect(QRectF(x - 13, 120, 26, 3))
        p.drawRect(QRectF(x - 13, 137, 26, 3))
        sg = QLinearGradient(x - 11, 0, x + 11, 0)
        sg.setColorAt(0, QColor("#c9d6cf"))
        sg.setColorAt(0.5, QColor("#fbfdf8"))
        sg.setColorAt(1, QColor("#aebdb6"))
        p.setBrush(QBrush(sg))
        p.setPen(Qt.NoPen)
        p.drawRect(body)
    for x in (80, 118, 160):                  # dew
        _sphere(p, x, 60 + rng.uniform(0, 30), 2.2, "#bfe7ff")


def _hatchery(p, rng, owned):
    arch = QPainterPath(QPointF(56, GROUND_Y + 4))
    arch.cubicTo(QPointF(48, 40), QPointF(192, 40), QPointF(184, GROUND_Y + 4))
    p.setBrush(Qt.NoBrush)
    p.setPen(_pen("#24170c", 9))
    p.drawPath(arch)
    p.setPen(_pen("#5d4027", 6))
    p.drawPath(arch)
    p.setPen(_pen("#8f6a44", 1.6, 190))
    p.drawPath(arch)
    p.setPen(_pen("#e6e0d0", 1.0, 200))
    for x in (96, 120, 144):                  # hanging silk
        p.drawLine(QPointF(x, 67 + abs(x - 120) * 0.12), QPointF(x + rng.uniform(-4, 4), 90))
    cradle = QPainterPath(QPointF(78, 92))
    cradle.cubicTo(QPointF(84, 140), QPointF(156, 140), QPointF(162, 92))
    p.setPen(_pen("#d8d1bf", 1.2, 210))
    p.setBrush(QColor(230, 224, 205, 70))
    p.drawPath(cradle)
    eggs = [(98, 104, 13), (122, 100, 15), (145, 106, 12), (110, 122, 12), (136, 124, 13), (122, 116, 11)]
    for x, y, r in eggs:
        if owned:
            _sphere(p, x, y, r, "#8e8a80", "#e9e6dd")
        else:
            _glow(p, x, y, r * 1.9, "#9dde6b", 70)
            _sphere(p, x, y, r, "#8fa36a", "#f2ffd8", "#3d4a2c")
            p.setPen(_pen("#4f6236", 0.8, 180))
            for _ in range(3):                # veins
                a = rng.uniform(0, math.tau)
                p.drawLine(QPointF(x, y), QPointF(x + math.cos(a) * r * 0.8, y + math.sin(a) * r * 0.7))
    shell = QPainterPath(QPointF(160, 132))   # one hatched, broken open
    for i in range(7):
        a = math.pi * (1 + i / 6)
        shell.lineTo(QPointF(170 + math.cos(a) * 10, 132 + math.sin(a) * 9 + (3 if i % 2 else 0)))
    p.setPen(_pen("#6d7556", 1.0))
    p.setBrush(QColor("#c8cfae"))
    p.drawPath(shell)
    if owned:
        p.setPen(_pen("#cfcac0", 3.5, 235))
        for i in range(5):                    # bound in grey silk
            y = 96 + i * 8
            p.drawLine(QPointF(82, y), QPointF(160, y + 6))
        _sphere(p, 122, 112, 9, "#a8322c", "#ffb3a0", "#5c1712")   # wax seal
        p.setPen(_pen("#ffd9c9", 1.4))
        p.drawLine(QPointF(117, 112), QPointF(127, 112))
        p.drawLine(QPointF(122, 107), QPointF(122, 117))


def _nest(p, rng, owned):
    dome = QPainterPath(QPointF(36, GROUND_Y + 6))
    dome.cubicTo(QPointF(36, 40), QPointF(204, 40), QPointF(204, GROUND_Y + 6))
    dome.closeSubpath()
    g = QRadialGradient(120, 80, 110)
    g.setColorAt(0, QColor("#4a2a36"))
    g.setColorAt(1, QColor("#1b1017"))
    p.fillPath(dome, QBrush(g))
    # Brambles: thorny branches arching over the dome.
    for i in range(11):
        start = QPointF(rng.uniform(38, 202), GROUND_Y + rng.uniform(-2, 6))
        top = QPointF(rng.uniform(70, 170), rng.uniform(46, 80))
        end = QPointF(rng.uniform(38, 202), GROUND_Y + rng.uniform(-10, 4))
        path = QPainterPath(start)
        path.quadTo(top, end)
        p.setPen(_pen("#140b0f", 6))
        p.drawPath(path)
        p.setPen(_pen(rng.choice(["#5b3440", "#6d3d3a", "#4a2c3a"]), 3.6))
        p.drawPath(path)
        for t in (0.2, 0.35, 0.5, 0.65, 0.8):
            pt = path.pointAtPercent(t)
            angle = path.angleAtPercent(t)
            a = math.radians(-angle + rng.choice((90, -90)))
            tip = QPointF(pt.x() + math.cos(a) * 7, pt.y() + math.sin(a) * 7)
            thorn = QPainterPath(QPointF(pt.x() - 2, pt.y()))
            thorn.lineTo(tip)
            thorn.lineTo(QPointF(pt.x() + 2, pt.y()))
            p.fillPath(thorn, QColor("#c9a890"))
    for x, y, h in ((52, 70, 26), (84, 46, 22), (156, 44, 24), (190, 70, 26)):   # spikes
        spike = QPainterPath(QPointF(x - 6, y + h))
        spike.lineTo(QPointF(x, y))
        spike.lineTo(QPointF(x + 6, y + h))
        sg = QLinearGradient(x, y, x, y + h)
        sg.setColorAt(0, QColor("#efd9c4"))
        sg.setColorAt(1, QColor("#8a6552"))
        p.fillPath(spike, QBrush(sg))
    # The maw.
    maw = QRectF(90, 90, 62, 52)
    mg = QRadialGradient(121, 124, 36)
    mg.setColorAt(0, QColor("#000000"))
    mg.setColorAt(1, QColor("#2a1219"))
    p.setBrush(QBrush(mg))
    p.setPen(_pen("#a5747c", 2))
    p.drawChord(maw, 0, 180 * 16)
    p.fillRect(QRectF(91, 116, 60, GROUND_Y - 116), QColor("#050304"))
    if owned:
        p.setPen(_pen("#e9e3d6", 1.1, 230))
        for i in range(8):                    # silk over the mouth
            p.drawLine(QPointF(92 + i * 8, 100), QPointF(150 - i * 7, GROUND_Y - 2))
    else:
        for x in (110, 132):                  # eyes in the dark
            _glow(p, x, 118, 11, "#ff3b2f", 150)
            p.setBrush(QColor("#ff6b4f"))
            p.setPen(Qt.NoPen)
            p.drawEllipse(QPointF(x, 118), 3.2, 2.2)


def _outpost(p, rng, owned):
    for x0, x1 in ((58, 104), (182, 136)):               # stakes
        _twig(p, QPointF(x0, GROUND_Y + 4), QPointF(x1, 52), 5)
    tent = QPainterPath(QPointF(46, GROUND_Y + 2))
    tent.quadTo(QPointF(84, 70), QPointF(120, 48))
    tent.quadTo(QPointF(156, 70), QPointF(194, GROUND_Y + 2))
    tent.closeSubpath()
    g = QLinearGradient(0, 48, 0, GROUND_Y)
    g.setColorAt(0, QColor("#e8e0cc"))
    g.setColorAt(1, QColor("#9d927c"))
    p.fillPath(tent, QBrush(g))
    p.setPen(_pen("#fff8e6", 1.0, 150))
    for i in range(9):                                     # silk strands
        p.drawLine(QPointF(120, 50), QPointF(52 + i * 17, GROUND_Y + 1))
    door = QPainterPath(QPointF(100, GROUND_Y + 2))
    door.quadTo(QPointF(120, 84), QPointF(140, GROUND_Y + 2))
    door.closeSubpath()
    p.fillPath(door, QColor("#1a120c"))
    colour = "#7de0a8" if owned else "#ff9a3c"
    _glow(p, 120, 44, 16, colour, 170)
    _sphere(p, 120, 44, 6, colour)


def _infestation(p, rng, owned):
    pane = QRectF(34, 58, 172, GROUND_Y - 52)
    p.setPen(_pen("#7fa6c4", 2, 160))
    p.setBrush(QColor(20, 40, 60, 120))
    p.drawRoundedRect(pane, 6, 6)
    p.setPen(_pen("#d9f0ff", 1.1, 170))
    for _ in range(9):                                     # cracks in the pane
        x, y = rng.uniform(70, 170), rng.uniform(70, 120)
        a = rng.uniform(0, math.tau)
        for _step in range(4):
            nx, ny = x + math.cos(a) * 12, y + math.sin(a) * 12
            p.drawLine(QPointF(x, y), QPointF(nx, ny))
            x, y, a = nx, ny, a + rng.uniform(-0.6, 0.6)
    sacs = ((96, 104, 26), (140, 100, 30), (118, 76, 22), (72, 118, 17), (168, 120, 18))
    for x, y, r in sacs:
        if owned:
            _sphere(p, x, y, r * 0.8, "#6b6f66", "#b9bcb2")
            p.setPen(_pen("#2b2d28", 2))
            p.drawLine(QPointF(x - r * 0.4, y - r * 0.2), QPointF(x + r * 0.3, y + r * 0.3))
        else:
            _glow(p, x, y, r * 1.5, "#9dff3a", 90)
            _sphere(p, x, y, r, "#7fcf2a", "#e9ffb0", "#35610d")
    if not owned:
        p.setPen(_pen("#a8ff45", 3, 200))
        for x in (92, 124, 150):                           # drips
            p.drawLine(QPointF(x, 118), QPointF(x, GROUND_Y + rng.uniform(-6, 6)))


PAINTERS = {"home": _home, "food": _food, "silk": _silk, "hatchery": _hatchery, "nest": _nest,
            "outpost": _outpost, "infestation": _infestation}
GROUND_TONES = {"home": "#6e5536", "food": "#5e5a35", "silk": "#5e5236",
                "hatchery": "#4e5132", "nest": "#3f2f30", "outpost": "#5a4c36", "infestation": "#2f3d2a"}


@lru_cache(maxsize=20)
def building_art(kind, owned):
    """Static detail is painted once per building/ownership pair, not per frame."""
    image = QImage(ART_W, ART_H, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    rng = random.Random(f"{kind}-37")
    _ground(p, rng, GROUND_TONES.get(kind, "#5a4a30"))
    PAINTERS.get(kind, _hatchery)(p, rng, owned)
    # Ownership pennant: a silhouette as well as a distinct colour.
    p.setPen(_pen("#dbc49a", 3))
    p.drawLine(QPointF(210, 88), QPointF(210, GROUND_Y + 4))
    flag = QPainterPath(QPointF(211, 88))
    flag.cubicTo(QPointF(220, 90), QPointF(226, 96), QPointF(234, 94))
    flag.lineTo(QPointF(211, 106))
    p.fillPath(flag, QColor("#80ccb1" if owned else "#e09a75"))
    p.end()
    return image
