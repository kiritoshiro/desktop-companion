"""The mission buildings: timber, thatch and stakes, painted once and cached.

The owner asked for game-asset buildings "of wood looking. more realistic",
showing a picture of isometric wooden watchtowers, thatched roofs and
sharpened-stake palisades casting long shadows. This draws in that style,
from scratch, with Qt only:

- **logs** (``_log``): round timber shaded across its width, bark grain along
  its length, a pale cut end, optionally sharpened;
- **stakes and palisades** (``_stake``, ``_palisade``): rows of pointed posts
  on a cross rail;
- **thatch** (``_thatch``): a straw roof built from hundreds of short straw
  strokes in rows, with a ragged fringe;
- **planks** (``_planks``): boards with grain, gaps and nail heads;
- **rope** lashings where timbers meet;
- every upright casts a **long shadow** to the lower right, as the sun in
  the picture does, on a patch of trampled earth.

Buildings:

- Home burrow: a thatched porch on log posts over a burrow door with a silk
  curtain, a woodpile and a lantern;
- Food cache: an open thatched shed with wicker baskets of berries and
  silk-wrapped prey hung from the beam;
- Silk loom: a log loom frame under a little thatch cap, warp threads, a
  woven panel, spools on a plank bench;
- Hatchery: a round stake pen full of veined egg sacs; sealed, boarded over,
  bound in silk under a wax seal;
- Thorn nest: a dark palisade fort with a thorned log gate, horned posts and
  red eyes in the dark; claimed, the eyes go out and silk covers the gate;
- Outpost: a palisade camp gate with a thatched lintel and a lantern;
- Infestation: acid sacs dripping over a cracked pane in a log frame;
  destroyed, burst and grey;
- Venom den: a hooped wooden vat on a log stand, brimming with venom;
- Lookout: a tall watchtower on braced legs, railed deck, ladder, conical
  thatch roof;
- Amber mine: a timbered mine mouth in a bank of earth, a plank cart of
  faceted amber, a pick;
- Nursery: a thatched hut on stilts with a silk cradle of eggs beneath;
- Fly nest: a broken crate of rotting fruit under a cloud of flies.

A pennant on every building shows who holds it (green yours, rust theirs).
The anchor is unchanged: the site's point is at (120, 139) of a 240x190
picture, painted at twice that size for crisp edges.
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
# The sun is up and to the left: shadows fall this far right and down per
# pixel of height.
SHADOW_X, SHADOW_Y = 0.62, 0.20

WOOD = "#9a7448"
WOOD_DARK = "#5e4428"
WOOD_OLD = "#6e5a44"
STRAW = "#c7a462"


def _c(color, alpha=None) -> QColor:
    c = QColor(color)
    if alpha is not None:
        c.setAlpha(alpha)
    return c


def _pen(color, width=1.0, alpha=255):
    return QPen(_c(color, alpha), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)


def _poly(*points) -> QPainterPath:
    path = QPainterPath(QPointF(*points[0]))
    for pt in points[1:]:
        path.lineTo(QPointF(*pt))
    path.closeSubpath()
    return path


def _blob(cx, cy, rx, ry, rng, wobble=0.12, points=22) -> QPainterPath:
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


# -- shadows ------------------------------------------------------------------

def _shadow_post(p, x, y, h, w):
    """The long shadow of an upright of height ``h`` standing at (x, y)."""
    dx, dy = h * SHADOW_X, h * SHADOW_Y
    p.fillPath(_poly((x - w / 2, y), (x + w / 2, y), (x + w / 2 + dx, y + dy), (x - w / 2 + dx, y + dy)),
               _c("#1a1208", 70))


def _shadow_mass(p, x, y, w, h, depth=18):
    """The shadow of a solid block ``w`` wide and ``h`` tall on the ground at (x, y)."""
    dx, dy = h * SHADOW_X, h * SHADOW_Y
    path = _poly((x - w / 2, y), (x + w / 2, y), (x + w / 2 + dx, y + dy), (x - w / 2 + dx, y + dy + depth * 0.2))
    p.fillPath(path, _c("#1a1208", 85))


# -- materials ----------------------------------------------------------------

def _log(p, a, b, r, color=WOOD, rng=None, sharp=False, cut=True):
    """A round log from ``a`` to ``b`` of radius ``r``: lit on its upper-left
    side, bark grain along it, a pale cut end at ``b`` (or a point)."""
    rng = rng or random.Random(int(a.x() * 7 + b.y() * 13))
    ax, ay, bx, by = a.x(), a.y(), b.x(), b.y()
    length = max(0.1, math.hypot(bx - ax, by - ay))
    ux, uy = (bx - ax) / length, (by - ay) / length
    nx, ny = -uy, ux
    tip = 0.0
    if sharp:
        tip = min(r * 2.4, length * 0.4)
    ex, ey = bx - ux * tip, by - uy * tip
    body = _poly((ax + nx * r, ay + ny * r), (ex + nx * r, ey + ny * r), (bx, by) if sharp else (ex + nx * r, ey + ny * r),
                 (ex - nx * r, ey - ny * r), (ax - nx * r, ay - ny * r))
    g = QLinearGradient(QPointF(ax + nx * r, ay + ny * r), QPointF(ax - nx * r, ay - ny * r))
    base = _c(color)
    # Which side faces the light (up-left) decides where the highlight goes.
    lit_first = (nx * -0.7 + ny * -0.7) > 0
    light, dark = base.lighter(140), base.darker(190)
    g.setColorAt(0.0, light if lit_first else dark)
    g.setColorAt(0.35, base.lighter(115) if lit_first else base)
    g.setColorAt(0.7, base.darker(120) if lit_first else base.lighter(115))
    g.setColorAt(1.0, dark if lit_first else light)
    p.save()
    p.setPen(Qt.NoPen)
    p.fillPath(body, QBrush(g))
    p.setClipPath(body)
    grain = base.darker(175)
    for k in range(int(r * 1.4) + 2):                  # bark grain along the log
        off = rng.uniform(-r * 0.85, r * 0.85)
        start = rng.uniform(0, length * 0.3)
        seg = rng.uniform(length * 0.3, length)
        wav = QPainterPath(QPointF(ax + ux * start + nx * off, ay + uy * start + ny * off))
        t = start
        while t < min(length, start + seg):
            t += 5
            jitter = rng.uniform(-0.5, 0.5)
            wav.lineTo(QPointF(ax + ux * t + nx * (off + jitter), ay + uy * t + ny * (off + jitter)))
        p.setPen(_pen(grain, rng.uniform(0.5, 1.0), rng.randint(90, 170)))
        p.drawPath(wav)
    for _ in range(int(length / 16) + 1):              # knots
        t, off = rng.uniform(0.1, 0.9) * length, rng.uniform(-r * 0.5, r * 0.5)
        p.setPen(_pen(base.darker(220), 0.8, 180))
        p.setBrush(_c(base.darker(150), 200))
        p.drawEllipse(QPointF(ax + ux * t + nx * off, ay + uy * t + ny * off), r * 0.28, r * 0.2)
    p.restore()
    p.setPen(_pen(base.darker(320), 1.1, 230))
    p.setBrush(Qt.NoBrush)
    p.drawPath(body)
    if cut and not sharp:
        end = QPainterPath()
        end.addEllipse(QPointF(ex, ey), r * 0.62 if abs(uy) > 0.7 else r * 0.9, r * 0.62 if abs(uy) <= 0.7 else r * 0.4)
        p.fillPath(end, _c("#d8b888"))
        p.setPen(_pen("#8a6a44", 0.7, 200))
        p.drawPath(end)


def _stake(p, x, y, h, w=3.4, color=WOOD, rng=None, shadow=True):
    """An upright pointed stake."""
    if shadow:
        _shadow_post(p, x, y, h, w)
    _log(p, QPointF(x, y), QPointF(x, y - h), w / 2, color, rng, sharp=True)


def _palisade(p, x0, y0, x1, y1, h, rng, color=WOOD, gap=5.0, rails=True):
    """A row of stakes of varying height from (x0, y0) to (x1, y1)."""
    count = max(2, int(math.hypot(x1 - x0, y1 - y0) / gap))
    stakes = []
    for i in range(count + 1):
        t = i / count
        stakes.append((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, h * rng.uniform(0.85, 1.12)))
    for x, y, sh in stakes:
        _shadow_post(p, x, y, sh, 3.6)
    for x, y, sh in stakes:
        _stake(p, x, y, sh, 3.8, color, rng, shadow=False)
    if rails:
        for frac in (0.35, 0.72):
            _log(p, QPointF(x0 - 2, y0 - h * frac), QPointF(x1 + 2, y1 - h * frac), 1.3, WOOD_DARK, rng)


def _thatch(p, path, rng, color=STRAW, fringe=True):
    """A straw roof: base colour, then rows of short straw strokes, then a
    ragged fringe along the bottom edge."""
    box = path.boundingRect()
    base = _c(color)
    g = QLinearGradient(box.topLeft(), box.bottomRight())
    g.setColorAt(0, base.lighter(128))
    g.setColorAt(0.5, base)
    g.setColorAt(1, base.darker(170))
    p.save()
    p.setPen(Qt.NoPen)
    p.fillPath(path, QBrush(g))
    p.setClipPath(path)
    rows = max(3, int(box.height() / 5))
    for row in range(rows):
        y = box.top() + row * box.height() / rows
        shade = base.darker(115 + (row % 2) * 30)
        shade.setAlpha(120)
        p.fillRect(QRectF(box.left(), y + box.height() / rows - 1.6, box.width(), 1.6), shade)
    for _ in range(int(box.width() * box.height() / 5)):
        x = rng.uniform(box.left(), box.right())
        y = rng.uniform(box.top(), box.bottom())
        length = rng.uniform(4, 8)
        lean = (x - box.center().x()) / max(1.0, box.width()) * 3.0 + rng.uniform(-0.8, 0.8)
        tone = base.lighter(rng.randint(105, 150)) if rng.random() < 0.55 else base.darker(rng.randint(120, 190))
        p.setPen(_pen(tone, rng.uniform(0.5, 0.95), rng.randint(150, 235)))
        p.drawLine(QPointF(x, y), QPointF(x + lean, y + length))
    p.restore()
    p.setPen(_pen(base.darker(260), 1.0, 200))
    p.setBrush(Qt.NoBrush)
    p.drawPath(path)
    if fringe:
        # Straw hanging below the eave, sampled along the lower edge of the shape.
        bottom = box.bottom()
        x = box.left() + 2
        while x < box.right() - 2:
            y_edge = None
            for step in range(int(box.height())):
                yy = bottom - step
                if path.contains(QPointF(x, yy)):
                    y_edge = yy
                    break
            if y_edge is not None and y_edge > box.top() + box.height() * 0.5:
                tone = base.darker(rng.randint(100, 160))
                p.setPen(_pen(tone, 0.8, 230))
                p.drawLine(QPointF(x, y_edge - 1), QPointF(x + rng.uniform(-1, 1), y_edge + rng.uniform(2, 5)))
            x += rng.uniform(1.2, 2.4)


def _planks(p, path, rng, color=WOOD, horizontal=True, nails=True):
    """Boards with grain, dark gaps between them, and nail heads."""
    box = path.boundingRect()
    base = _c(color)
    p.save()
    p.setClipPath(path)
    width = 6.5
    pos = box.top() if horizontal else box.left()
    end = box.bottom() if horizontal else box.right()
    while pos < end:
        tone = base.lighter(rng.randint(95, 125)) if rng.random() < 0.5 else base.darker(rng.randint(100, 125))
        board = QRectF(box.left(), pos, box.width(), width) if horizontal else QRectF(pos, box.top(), width, box.height())
        g = QLinearGradient(board.topLeft(), board.bottomLeft() if horizontal else board.topRight())
        g.setColorAt(0, tone.lighter(118))
        g.setColorAt(1, tone.darker(125))
        p.fillRect(board, QBrush(g))
        p.setPen(_pen(tone.darker(160), 0.6, 150))
        for _ in range(3):
            if horizontal:
                y = rng.uniform(board.top() + 1, board.bottom() - 1)
                p.drawLine(QPointF(board.left(), y), QPointF(board.right(), y + rng.uniform(-0.8, 0.8)))
            else:
                x = rng.uniform(board.left() + 1, board.right() - 1)
                p.drawLine(QPointF(x, board.top()), QPointF(x + rng.uniform(-0.8, 0.8), board.bottom()))
        p.setPen(_pen("#241808", 1.0, 220))
        if horizontal:
            p.drawLine(QPointF(board.left(), board.bottom()), QPointF(board.right(), board.bottom()))
        else:
            p.drawLine(QPointF(board.right(), board.top()), QPointF(board.right(), board.bottom()))
        if nails:
            p.setPen(Qt.NoPen)
            p.setBrush(_c("#3a3632"))
            if horizontal:
                for x in (board.left() + 4, board.right() - 4):
                    p.drawEllipse(QPointF(x, board.center().y()), 0.9, 0.9)
            else:
                for y in (board.top() + 4, board.bottom() - 4):
                    p.drawEllipse(QPointF(board.center().x(), y), 0.9, 0.9)
        pos += width
    p.restore()
    p.setPen(_pen(base.darker(300), 1.1, 220))
    p.setBrush(Qt.NoBrush)
    p.drawPath(path)


def _rope(p, x, y, w=7, turns=3):
    for i in range(turns):
        p.setPen(_pen("#4a3a22", 2.2))
        p.drawLine(QPointF(x - w / 2, y - 2 + i * 2.0), QPointF(x + w / 2, y - 0.6 + i * 2.0))
        p.setPen(_pen("#d8c49a", 1.2))
        p.drawLine(QPointF(x - w / 2, y - 2.3 + i * 2.0), QPointF(x + w / 2, y - 0.9 + i * 2.0))


def _glow(p, x, y, r, color, alpha=120):
    g = QRadialGradient(x, y, r)
    g.setColorAt(0, _c(color, alpha))
    g.setColorAt(1, _c(color, 0))
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), r, r)


def _spec(p, x, y, rx, ry, alpha=200):
    g = QRadialGradient(QPointF(x, y), rx)
    g.setColorAt(0, _c("#ffffff", alpha))
    g.setColorAt(1, _c("#ffffff", 0))
    p.setPen(Qt.NoPen)
    p.setBrush(QBrush(g))
    p.drawEllipse(QPointF(x, y), rx, ry)


def _fill(p, path, base, light=130, dark=170, outline=True):
    box = path.boundingRect()
    b = _c(base)
    g = QLinearGradient(box.topLeft(), box.bottomRight())
    g.setColorAt(0, b.lighter(light))
    g.setColorAt(0.55, b)
    g.setColorAt(1, b.darker(dark))
    p.setPen(Qt.NoPen)
    p.fillPath(path, QBrush(g))
    if outline:
        p.setPen(_pen(b.darker(300), 1.1, 220))
        p.setBrush(Qt.NoBrush)
        p.drawPath(path)


def _egg(p, x, y, rx, ry, base, rng, glossy=True):
    path = QPainterPath()
    path.addEllipse(QPointF(x, y), rx, ry)
    _fill(p, path, base)
    p.save()
    p.setClipPath(path)
    p.setPen(_pen(_c(base).darker(170), 0.8, 150))
    for _ in range(3):
        vx = rng.uniform(x - rx, x + rx)
        vein = QPainterPath(QPointF(vx, y - ry))
        vein.cubicTo(QPointF(vx + rng.uniform(-4, 4), y - ry * 0.3), QPointF(vx + rng.uniform(-4, 4), y + ry * 0.3),
                     QPointF(vx + rng.uniform(-3, 3), y + ry))
        p.drawPath(vein)
    p.restore()
    if glossy:
        _spec(p, x - rx * 0.35, y - ry * 0.4, rx * 0.4, ry * 0.28)


def _lantern(p, x, y, colour):
    _glow(p, x, y, 20, colour, 150)
    body = QPainterPath()
    body.addRoundedRect(QRectF(x - 4.5, y - 5.5, 9, 11), 2.5, 2.5)
    _fill(p, body, colour, light=160)
    p.setPen(_pen("#2a1e10", 1.4))
    for dy in (-5.5, 5.5):
        p.drawLine(QPointF(x - 5, y + dy), QPointF(x + 5, y + dy))
    p.drawLine(QPointF(x, y - 9), QPointF(x, y - 5.5))
    _spec(p, x - 1.4, y - 2, 2.2, 3.2, 230)


def _crystal(p, x, y, h, colour, lean=0.0):
    w = h * 0.42
    tip = (x + lean * h, y - h)
    left = _poly((x - w, y), (x - w * 0.9, y - h * 0.6), tip, (x, y - h * 0.15), (x, y))
    right = _poly((x, y), (x, y - h * 0.15), tip, (x + w * 0.9, y - h * 0.62), (x + w, y))
    _fill(p, left, _c(colour).lighter(125))
    _fill(p, right, _c(colour).darker(135))
    p.setPen(_pen("#fff6d8", 1.0, 190))
    p.drawLine(QPointF(x - w * 0.55, y - h * 0.35), QPointF(tip[0] - w * 0.1, tip[1] + h * 0.2))


def _wicker(p, x, y, w, h, rng):
    basket = _poly((x - w / 2, y - h), (x + w / 2, y - h), (x + w * 0.4, y), (x - w * 0.4, y))
    _fill(p, basket, "#b08a52")
    p.save()
    p.setClipPath(basket)
    for i in range(int(h / 2.6) + 1):
        yy = y - h + i * 2.6
        p.setPen(_pen("#6a4a26" if i % 2 else "#d8b87a", 1.2, 220))
        p.drawLine(QPointF(x - w, yy), QPointF(x + w, yy))
    for i in range(int(w / 4) + 1):
        p.setPen(_pen("#5a3e20", 0.9, 180))
        xx = x - w / 2 + i * 4
        p.drawLine(QPointF(xx, y - h), QPointF(xx + (x - xx) * 0.2, y))
    p.restore()


def _pennant(p, owned, rng):
    """Who holds it: green with a spider when yours, rust with thorns when not."""
    _shadow_post(p, 212, GROUND_Y + 4, 58, 2.4)
    _log(p, QPointF(212, GROUND_Y + 4), QPointF(212, 82), 1.5, WOOD_DARK, rng)
    colour = "#5fb38e" if owned else "#c8643a"
    flag = QPainterPath(QPointF(213.4, 86))
    flag.cubicTo(QPointF(222, 83), QPointF(228, 92), QPointF(238, 90))
    flag.cubicTo(QPointF(233, 96), QPointF(231, 101), QPointF(236, 108))
    flag.cubicTo(QPointF(226, 106), QPointF(220, 104), QPointF(213.4, 106))
    flag.closeSubpath()
    _fill(p, flag, colour)
    p.save()
    p.setClipPath(flag)
    p.setPen(_pen(_c(colour).darker(150), 1.0, 120))
    for yy in range(86, 108, 3):
        p.drawLine(QPointF(213, yy), QPointF(240, yy + 2))
    p.restore()
    mark = "#1d2a24" if owned else "#3a1810"
    p.setPen(Qt.NoPen)
    p.setBrush(_c(mark, 220))
    p.drawEllipse(QPointF(223, 96), 2.6, 2.2)
    p.setPen(_pen(mark, 0.9, 220))
    for k in (-1, 1):
        for dy in (-2, 0, 2):
            p.drawLine(QPointF(223, 96), QPointF(223 + k * 5, 96 + dy + k * 0.5))


def _ground(p, rng, tone):
    """Trampled earth under the building, with stones and grass at its edge."""
    patch = _blob(120, GROUND_Y + 3, 104, 24, rng, 0.08, 26)
    g = QRadialGradient(QPointF(116, GROUND_Y), 110)
    g.setColorAt(0, _c(tone).lighter(112))
    g.setColorAt(0.8, _c(tone))
    g.setColorAt(1, _c(tone, 0))
    p.setPen(Qt.NoPen)
    p.fillPath(patch, QBrush(g))
    p.save()
    p.setClipPath(patch)
    for _ in range(260):                                # soil grains
        x, y = rng.uniform(16, 224), rng.uniform(GROUND_Y - 22, GROUND_Y + 28)
        tone_ = _c(tone).darker(rng.randint(115, 160)) if rng.random() < 0.6 else _c(tone).lighter(rng.randint(110, 140))
        tone_.setAlpha(rng.randint(80, 170))
        p.setBrush(tone_)
        s = rng.uniform(0.6, 1.8)
        p.drawEllipse(QRectF(x, y, s, s * 0.7))
    p.restore()
    for _ in range(9):                                  # stones
        a = rng.uniform(0, math.tau)
        x, y = 120 + math.cos(a) * rng.uniform(40, 92), GROUND_Y + 3 + math.sin(a) * rng.uniform(8, 20)
        w = rng.uniform(1.8, 4.2)
        stone = _blob(x, y, w, w * 0.65, rng, 0.2, 8)
        _fill(p, stone, rng.choice(["#8a8274", "#6e685c", "#a09888"]), outline=False)
        _spec(p, x - w * 0.3, y - w * 0.25, w * 0.4, w * 0.25, 110)
    for _ in range(26):                                 # grass round the edge
        a = rng.uniform(0, math.tau)
        x, y = 120 + math.cos(a) * rng.uniform(84, 104), GROUND_Y + 3 + math.sin(a) * rng.uniform(18, 25)
        for _ in range(rng.randint(3, 6)):
            dx = rng.uniform(-3, 3)
            h = rng.uniform(4, 9)
            blade = _poly((x + dx - 1, y), (x + dx + rng.uniform(-2, 2), y - h), (x + dx + 1, y))
            p.fillPath(blade, _c(rng.choice(["#6f8a45", "#86a153", "#56733a", "#9ab060"]), 230))


# -- the buildings ---------------------------------------------------------------

def _home(p, rng, owned):
    # The burrow mound, its door, and a thatched porch on four log posts.
    mound = QPainterPath(QPointF(46, GROUND_Y + 4))
    mound.cubicTo(QPointF(50, 70), QPointF(190, 64), QPointF(196, GROUND_Y + 4))
    mound.closeSubpath()
    _shadow_mass(p, 124, GROUND_Y + 4, 140, 60)
    _fill(p, mound, "#7a6040")
    p.save()
    p.setClipPath(mound)
    for _ in range(300):
        x, y = rng.uniform(46, 196), rng.uniform(64, GROUND_Y + 4)
        p.setPen(Qt.NoPen)
        p.setBrush(_c(rng.choice(["#5a4428", "#8e7450", "#6a5234"]), rng.randint(90, 180)))
        p.drawEllipse(QPointF(x, y), rng.uniform(0.6, 2), rng.uniform(0.5, 1.4))
    p.restore()
    door = QPainterPath(QPointF(104, GROUND_Y + 4))
    door.cubicTo(QPointF(104, 98), QPointF(140, 98), QPointF(140, GROUND_Y + 4))
    door.closeSubpath()
    p.fillPath(door, _c("#0e0804"))
    p.save()
    p.setClipPath(door)
    p.setPen(_pen("#efe6cf", 0.8, 170))
    for i in range(9):
        x = 106 + i * 4
        p.drawLine(QPointF(x, 104), QPointF(x + math.sin(i) * 2.5, GROUND_Y + 4))
    p.restore()
    posts = ((92, GROUND_Y + 8), (152, GROUND_Y + 8), (100, GROUND_Y - 4), (144, GROUND_Y - 4))
    for x, y in posts[2:]:
        _shadow_post(p, x, y, 50, 4)
        _log(p, QPointF(x, y), QPointF(x, y - 46), 2.4, WOOD, rng)
    for x, y in posts[:2]:
        _shadow_post(p, x, y, 56, 4)
        _log(p, QPointF(x, y), QPointF(x, y - 52), 2.6, WOOD, rng)
    _log(p, QPointF(86, 90), QPointF(158, 90), 2.4, WOOD_DARK, rng)
    for x in (92, 152):
        _rope(p, x, 90)
    roof = _poly((74, 92), (98, 56), (146, 56), (170, 92))
    _thatch(p, roof, rng)
    ridge = QPainterPath(QPointF(96, 57))
    ridge.lineTo(QPointF(148, 57))
    _log(p, QPointF(94, 57), QPointF(150, 57), 2.2, WOOD_DARK, rng)
    for i in range(4):                                  # woodpile
        for j in range(3 - i // 2):
            _log(p, QPointF(52 + j * 2, GROUND_Y + 2 - i * 5), QPointF(76 + j * 2, GROUND_Y + 4 - i * 5), 2.4,
                 WOOD, rng)
    _log(p, QPointF(180, GROUND_Y + 6), QPointF(180, 96), 1.6, WOOD_DARK, rng)
    _lantern(p, 180, 104, "#ffb45a")


def _shed_roof(p, rng, left, right, top, eave, depth=16):
    """A gable roof seen from the front-left: the long thatched side and the
    timber gable end."""
    front = _poly((left, eave), (left + depth, top), (right + depth, top), (right, eave))
    gable = _poly((right, eave), (right + depth, top), (right + depth + 10, eave - 4))
    _fill(p, gable, WOOD_OLD)
    _planks(p, gable, rng, WOOD_OLD, horizontal=False, nails=False)
    _thatch(p, front, rng)


def _food(p, rng, owned):
    back = ((76, GROUND_Y - 10), (168, GROUND_Y - 10))
    front = ((66, GROUND_Y + 6), (158, GROUND_Y + 6))
    for x, y in back + front:
        _shadow_post(p, x, y, 56, 4)
    for x, y in back:
        _log(p, QPointF(x, y), QPointF(x, 72), 2.4, WOOD, rng)
    for x, y, w, h in ((96, GROUND_Y + 2, 26, 18), (124, GROUND_Y + 4, 24, 16), (148, GROUND_Y - 2, 20, 14)):
        _wicker(p, x, y, w, h, rng)
        for _ in range(7):                              # berries in the baskets
            bx, by = x + rng.uniform(-w * 0.3, w * 0.3), y - h - rng.uniform(0, 3)
            berry = QPainterPath()
            berry.addEllipse(QPointF(bx, by), 2.8, 2.8)
            _fill(p, berry, rng.choice(["#b02a2a", "#7a1f3a", "#c8462a"]), outline=False)
            _spec(p, bx - 0.8, by - 0.8, 1.1, 0.9, 230)
    for x, y in front:
        _log(p, QPointF(x, y), QPointF(x, 80), 2.8, WOOD, rng)
    _log(p, QPointF(60, 80), QPointF(164, 80), 2.6, WOOD_DARK, rng)
    for x, _y in front:
        _rope(p, x, 80)
    for x in (92, 124):                                 # prey wrapped in silk, hung from the beam
        p.setPen(_pen("#e8e0cc", 1.0))
        p.drawLine(QPointF(x, 82), QPointF(x, 92))
        bundle = QPainterPath()
        bundle.addEllipse(QRectF(x - 6, 92, 12, 18))
        _fill(p, bundle, "#e6dfcc", light=110)
        p.save()
        p.setClipPath(bundle)
        p.setPen(_pen("#a89f88", 0.8, 200))
        for i in range(6):
            p.drawLine(QPointF(x - 8, 92 + i * 3.4), QPointF(x + 8, 94 + i * 3.4))
        p.restore()
    _shed_roof(p, rng, 50, 164, 44, 82)


def _silk(p, rng, owned):
    bench = _poly((70, GROUND_Y - 2), (170, GROUND_Y - 2), (174, GROUND_Y + 4), (66, GROUND_Y + 4))
    _shadow_mass(p, 120, GROUND_Y + 4, 110, 10)
    for x in (72, 164):
        _shadow_post(p, x, GROUND_Y + 4, 84, 4)
        _log(p, QPointF(x, GROUND_Y + 4), QPointF(x, 46), 3.0, WOOD, rng, sharp=True)
    _log(p, QPointF(64, 58), QPointF(172, 58), 2.6, WOOD_DARK, rng)
    _log(p, QPointF(66, 112), QPointF(170, 112), 2.4, WOOD_DARK, rng)
    for x in (72, 164):
        _rope(p, x, 58)
        _rope(p, x, 112)
    p.setPen(_pen("#f2ecdc", 0.9, 220))                 # warp threads
    for i in range(18):
        x = 80 + i * 4.6
        p.drawLine(QPointF(x, 61), QPointF(x, 109))
    panel = QPainterPath()
    panel.addRect(QRectF(80, 80, 80, 28))
    _fill(p, panel, "#e7e0cb", light=108)
    p.save()
    p.setClipPath(panel)
    for i in range(0, 90, 3):
        p.setPen(_pen("#b8ae94", 0.7, 180))
        p.drawLine(QPointF(80, 80 + i * 0.4), QPointF(160, 80 + i * 0.4))
    p.restore()
    _fill(p, bench, WOOD)
    _planks(p, bench, rng, WOOD, nails=True)
    for x in (92, 120, 148):                            # spools
        spool = QPainterPath()
        spool.addRoundedRect(QRectF(x - 6, GROUND_Y - 16, 12, 14), 2, 2)
        _fill(p, spool, "#efe8d6", light=110)
        for y in (GROUND_Y - 18, GROUND_Y - 3):
            end = QPainterPath()
            end.addRoundedRect(QRectF(x - 8, y, 16, 3), 1.5, 1.5)
            _fill(p, end, WOOD_DARK)
    cap = _poly((58, 50), (74, 34), (164, 34), (180, 50))
    _thatch(p, cap, rng)


def _ring(cx, cy, rx, ry, count, back):
    pts = []
    for i in range(count):
        a = math.pi + i * math.pi / (count - 1) if back else i * math.pi / (count - 1)
        pts.append((cx + math.cos(a) * rx, cy + math.sin(a) * ry))
    return pts


def _hatchery(p, rng, owned):
    cx, cy, rx, ry = 120, GROUND_Y - 4, 62, 18
    for x, y in _ring(cx, cy, rx, ry, 15, back=True):
        _stake(p, x, y, 40 * rng.uniform(0.9, 1.1), 3.8, WOOD, rng)
    if owned:
        lid = _blob(cx, cy - 16, 58, 22, rng, 0.04)
        _fill(p, lid, WOOD)
        _planks(p, lid, rng, WOOD, horizontal=False)
        p.setPen(_pen("#e8e0cc", 2.2, 230))
        for i in range(4):
            p.drawLine(QPointF(cx - 58, cy - 26 + i * 7), QPointF(cx + 58, cy - 20 + i * 7))
        seal = QPainterPath()
        seal.addEllipse(QPointF(cx, cy - 14), 9, 9)
        _fill(p, seal, "#a62024")
        p.setPen(_pen("#f0c0a0", 1.4))
        p.drawLine(QPointF(cx - 5, cy - 14), QPointF(cx + 5, cy - 14))
        p.drawLine(QPointF(cx, cy - 19), QPointF(cx, cy - 9))
    else:
        for x, y, erx, ery in ((100, cy - 22, 14, 17), (132, cy - 24, 15, 18), (116, cy - 40, 13, 15),
                               (84, cy - 12, 11, 13), (150, cy - 12, 11, 13), (118, cy - 10, 13, 15)):
            _egg(p, x, y, erx, ery, "#a8c47a", rng)
        _glow(p, cx, cy - 22, 44, "#d8ff9a", 40)
    for x, y in _ring(cx, cy, rx, ry, 15, back=False):
        _stake(p, x, y, 34 * rng.uniform(0.9, 1.1), 4.0, WOOD, rng)
    _log(p, QPointF(cx - rx - 2, cy + 2), QPointF(cx + rx + 2, cy + 2), 1.5, WOOD_DARK, rng)


def _nest(p, rng, owned):
    dark = "#4e3a2e"
    _palisade(p, 34, GROUND_Y - 6, 92, GROUND_Y - 22, 58, rng, dark, gap=5.5)
    _palisade(p, 148, GROUND_Y - 22, 206, GROUND_Y - 6, 58, rng, dark, gap=5.5)
    # The gate: two thick horned posts and a log lintel over a dark mouth.
    mouth = _poly((96, GROUND_Y + 2), (96, 84), (144, 84), (144, GROUND_Y + 2))
    p.fillPath(mouth, _c("#060304"))
    for x in (92, 148):
        _shadow_post(p, x, GROUND_Y + 4, 80, 7)
        _log(p, QPointF(x, GROUND_Y + 4), QPointF(x, 64), 5.0, dark, rng, sharp=True)
        horn = QPainterPath(QPointF(x, 80))
        side = -1 if x < 120 else 1
        horn.cubicTo(QPointF(x + side * 18, 74), QPointF(x + side * 22, 60), QPointF(x + side * 16, 50))
        horn.cubicTo(QPointF(x + side * 16, 62), QPointF(x + side * 10, 72), QPointF(x, 86))
        _fill(p, horn, "#e6d6bc")
    _log(p, QPointF(86, 84), QPointF(154, 84), 3.4, dark, rng)
    for i in range(6):                                  # thorns on the lintel
        x = 96 + i * 9
        _fill(p, _poly((x - 2, 82), (x + 1, 72 - (i % 2) * 3), (x + 2, 82)), "#d8c0a8")
    if owned:
        p.setPen(_pen("#f2ecdc", 1.2, 235))
        for i in range(8):
            p.drawLine(QPointF(98 + i * 6, 86), QPointF(142 - i * 6, GROUND_Y))
        p.drawLine(QPointF(97, 104), QPointF(143, 100))
        p.drawLine(QPointF(97, 120), QPointF(143, 122))
    else:
        for x in (111, 129):
            _glow(p, x, 108, 12, "#ff3b2f", 180)
            eye = QPainterPath()
            eye.addEllipse(QPointF(x, 108), 3.6, 2.4)
            _fill(p, eye, "#ff5a3a", outline=False)
            _spec(p, x - 1, 107, 1.2, 0.8, 255)
    _palisade(p, 150, GROUND_Y + 6, 188, GROUND_Y + 12, 30, rng, dark, gap=5.0, rails=False)


def _outpost(p, rng, owned):
    _palisade(p, 30, GROUND_Y + 2, 82, GROUND_Y - 12, 38, rng, WOOD, gap=5.5)
    _palisade(p, 158, GROUND_Y - 12, 206, GROUND_Y + 2, 38, rng, WOOD, gap=5.5)
    for x in (88, 152):
        _shadow_post(p, x, GROUND_Y - 8, 78, 5)
        _log(p, QPointF(x, GROUND_Y - 8), QPointF(x, 60), 3.4, WOOD, rng)
        _rope(p, x, 68)
    _log(p, QPointF(80, 66), QPointF(160, 66), 2.8, WOOD_DARK, rng)
    roof = _poly((70, 68), (86, 46), (156, 46), (172, 68))
    _thatch(p, roof, rng)
    _log(p, QPointF(84, 46), QPointF(158, 46), 2.0, WOOD_DARK, rng)
    _lantern(p, 120, 80, "#7de0a8" if owned else "#ff9a3c")


def _infestation(p, rng, owned):
    _shadow_mass(p, 120, GROUND_Y, 170, 84)
    pane = QPainterPath()
    pane.addRect(QRectF(44, 58, 152, GROUND_Y - 64))
    g = QLinearGradient(44, 58, 196, GROUND_Y)
    g.setColorAt(0, _c("#3a6a94"))
    g.setColorAt(1, _c("#0e2238"))
    p.fillPath(pane, QBrush(g))
    p.save()
    p.setClipPath(pane)
    p.setPen(_pen("#d9f0ff", 1.1, 190))
    for _ in range(8):
        x, y = rng.uniform(60, 180), rng.uniform(66, 125)
        a = rng.uniform(0, math.tau)
        for _step in range(5):
            nx, ny = x + math.cos(a) * 11, y + math.sin(a) * 11
            p.drawLine(QPointF(x, y), QPointF(nx, ny))
            x, y, a = nx, ny, a + rng.uniform(-0.6, 0.6)
    p.fillRect(QRectF(44, 58, 152, 12), _c("#ffffff", 30))
    p.restore()
    _log(p, QPointF(40, GROUND_Y + 2), QPointF(40, 54), 3.4, WOOD, rng, sharp=True)
    _log(p, QPointF(200, GROUND_Y + 2), QPointF(200, 54), 3.4, WOOD, rng, sharp=True)
    _log(p, QPointF(34, 60), QPointF(206, 60), 2.6, WOOD_DARK, rng)
    _log(p, QPointF(34, GROUND_Y - 6), QPointF(206, GROUND_Y - 6), 2.6, WOOD_DARK, rng)
    for x in (40, 200):
        _rope(p, x, 60)
    for x, y, rx, ry in ((96, 104, 20, 18), (142, 100, 24, 20), (120, 78, 17, 15), (72, 118, 14, 12),
                         (168, 118, 14, 12)):
        if owned:
            _egg(p, x, y, rx * 0.85, ry * 0.8, "#6b6f66", rng, glossy=False)
            p.setPen(_pen("#262822", 1.8))
            p.drawLine(QPointF(x - rx * 0.4, y - ry * 0.3), QPointF(x + rx * 0.2, y + ry * 0.3))
        else:
            _glow(p, x, y, rx * 1.6, "#9dff3a", 70)
            _egg(p, x, y, rx, ry, "#7ccf2a", rng)
    if not owned:
        puddle = _blob(120, GROUND_Y + 4, 40, 6, rng, 0.2)
        _fill(p, puddle, "#8ae63a", outline=False)
        p.setPen(_pen("#b8ff5a", 2.6, 220))
        p.setBrush(_c("#b8ff5a"))
        for x in (92, 124, 150):
            p.drawLine(QPointF(x, 114), QPointF(x, GROUND_Y))
            p.drawEllipse(QPointF(x, GROUND_Y + 1), 2.2, 2.6)


def _venom(p, rng, owned):
    colour = "#7fe06a" if owned else "#a848d0"
    # A log stand...
    for x0, x1 in ((82, 96), (158, 144)):
        _shadow_post(p, x0, GROUND_Y + 4, 40, 4)
        _log(p, QPointF(x0, GROUND_Y + 4), QPointF(x1, 104), 2.6, WOOD, rng)
    _log(p, QPointF(86, 106), QPointF(154, 106), 2.6, WOOD_DARK, rng)
    # ...holding a hooped vat of staves.
    _shadow_mass(p, 120, 104, 70, 50)
    vat = _poly((88, 58), (152, 58), (146, 106), (94, 106))
    _fill(p, vat, WOOD)
    _planks(p, vat, rng, WOOD, horizontal=False, nails=False)
    p.save()
    p.setClipPath(vat)
    shade = QLinearGradient(88, 0, 152, 0)
    shade.setColorAt(0, _c("#ffffff", 40))
    shade.setColorAt(0.4, _c("#000000", 0))
    shade.setColorAt(1, _c("#000000", 110))
    p.fillRect(QRectF(86, 56, 70, 52), QBrush(shade))
    p.restore()
    for y, w in ((64, 62), (84, 58), (100, 54)):        # iron hoops
        hoop = QPainterPath(QPointF(120 - w / 2, y))
        hoop.quadTo(QPointF(120, y + 6), QPointF(120 + w / 2, y))
        p.setPen(_pen("#2a2622", 3.2))
        p.drawPath(hoop)
        p.setPen(_pen("#8a8680", 1.0, 200))
        p.drawPath(hoop.translated(0, -1))
    top = QPainterPath()
    top.addEllipse(QRectF(88, 52, 64, 12))
    _fill(p, top, WOOD_DARK)
    surface = QPainterPath()
    surface.addEllipse(QRectF(92, 54, 56, 8))
    _glow(p, 120, 58, 34, colour, 110)
    _fill(p, surface, colour, light=150, outline=False)
    _spec(p, 110, 56, 8, 1.6, 180)
    p.setPen(_pen(colour, 3, 230))
    for x, length in ((96, 22), (142, 30)):
        p.drawLine(QPointF(x, 60), QPointF(x - (2 if x < 120 else -2), 60 + length))
    puddle = _blob(122, GROUND_Y + 4, 30, 6, rng, 0.2)
    _fill(p, puddle, colour, outline=False)
    _spec(p, 114, GROUND_Y + 2, 8, 1.8, 150)


def _tower(p, rng, owned, top_y, deck_w, legs_spread, roof="cone", height_scale=1.0):
    """A watchtower: splayed, cross-braced legs, a railed plank deck, a
    ladder and a thatched roof."""
    base_l, base_r = 120 - legs_spread, 120 + legs_spread
    deck_y = top_y + 34
    back_legs = ((base_l + 10, GROUND_Y - 8), (base_r - 10, GROUND_Y - 8))
    front_legs = ((base_l, GROUND_Y + 6), (base_r, GROUND_Y + 6))
    for x, y in back_legs + front_legs:
        _shadow_post(p, x, y, (y - deck_y) + 30, 4.5)
    for (x, y), tx in zip(back_legs, (120 - deck_w / 2 + 6, 120 + deck_w / 2 - 6)):
        _log(p, QPointF(x, y), QPointF(tx, deck_y), 2.4, WOOD, rng)
    for yb in (GROUND_Y - 30, deck_y + 30):             # cross bracing
        _log(p, QPointF(base_l + 4, yb + 10), QPointF(base_r - 4, yb - 12), 1.6, WOOD_DARK, rng)
        _log(p, QPointF(base_l + 4, yb - 12), QPointF(base_r - 4, yb + 10), 1.6, WOOD_DARK, rng)
    for (x, y), tx in zip(front_legs, (120 - deck_w / 2, 120 + deck_w / 2)):
        _log(p, QPointF(x, y), QPointF(tx, deck_y - 2), 3.0, WOOD, rng)
    deck = _poly((120 - deck_w / 2 - 6, deck_y), (120 + deck_w / 2 + 6, deck_y),
                 (120 + deck_w / 2 + 10, deck_y - 8), (120 - deck_w / 2 - 2, deck_y - 8))
    _fill(p, deck, WOOD)
    _planks(p, deck, rng, WOOD, horizontal=False)
    rail = _poly((120 - deck_w / 2 - 6, deck_y - 2), (120 + deck_w / 2 + 6, deck_y - 2),
                 (120 + deck_w / 2 + 6, deck_y - 16), (120 - deck_w / 2 - 6, deck_y - 16))
    for i in range(int(deck_w / 5) + 3):                # stake railing
        x = 120 - deck_w / 2 - 5 + i * 5
        _log(p, QPointF(x, deck_y - 1), QPointF(x, deck_y - 18), 1.5, WOOD, rng, sharp=True)
    _log(p, QPointF(120 - deck_w / 2 - 8, deck_y - 12), QPointF(120 + deck_w / 2 + 8, deck_y - 12), 1.4, WOOD_DARK, rng)
    del rail
    for x in (120 - deck_w / 2 + 2, 120 + deck_w / 2 - 2):   # roof posts
        _log(p, QPointF(x, deck_y - 8), QPointF(x, top_y + 4), 1.6, WOOD, rng)
    for i in range(7):                                  # ladder
        y = GROUND_Y + 2 - i * ((GROUND_Y - deck_y) / 7)
        p.setPen(_pen("#3a2614", 2.4))
        p.drawLine(QPointF(base_r + 6 - i * 1.4, y), QPointF(base_r + 18 - i * 1.4, y))
    _log(p, QPointF(base_r + 6, GROUND_Y + 4), QPointF(base_r - 4, deck_y), 1.3, WOOD_DARK, rng)
    _log(p, QPointF(base_r + 18, GROUND_Y + 4), QPointF(base_r + 8, deck_y), 1.3, WOOD_DARK, rng)
    if roof == "cone":
        cone = QPainterPath(QPointF(120 - deck_w / 2 - 12, top_y + 8))
        cone.quadTo(QPointF(120 - deck_w * 0.2, top_y - 18), QPointF(120, top_y - 30))
        cone.quadTo(QPointF(120 + deck_w * 0.2, top_y - 18), QPointF(120 + deck_w / 2 + 12, top_y + 8))
        cone.closeSubpath()
        _thatch(p, cone, rng)
        _log(p, QPointF(120, top_y - 26), QPointF(120, top_y - 40), 1.4, WOOD_DARK, rng, sharp=True)
    else:
        _thatch(p, _poly((120 - deck_w / 2 - 12, top_y + 8), (120 - deck_w / 2 + 2, top_y - 12),
                         (120 + deck_w / 2 - 2, top_y - 12), (120 + deck_w / 2 + 12, top_y + 8)), rng)
    _lantern(p, 120, top_y + 16, "#7de0a8" if owned else "#ff9a3c")


def _lookout(p, rng, owned):
    _tower(p, rng, owned, top_y=38, deck_w=44, legs_spread=34)


def _amber(p, rng, owned):
    bank = QPainterPath(QPointF(30, GROUND_Y + 2))
    bank.cubicTo(QPointF(40, 64), QPointF(150, 52), QPointF(180, GROUND_Y + 2))
    bank.closeSubpath()
    _shadow_mass(p, 104, GROUND_Y + 2, 150, 60)
    _fill(p, bank, "#7a6242")
    p.save()
    p.setClipPath(bank)
    for _ in range(260):
        x, y = rng.uniform(30, 180), rng.uniform(54, GROUND_Y)
        p.setPen(Qt.NoPen)
        p.setBrush(_c(rng.choice(["#5a4428", "#8e7450", "#6a5234", "#9a8a70"]), rng.randint(90, 180)))
        p.drawEllipse(QPointF(x, y), rng.uniform(0.6, 2.2), rng.uniform(0.5, 1.5))
    p.restore()
    mouth = _poly((78, GROUND_Y + 2), (80, 90), (124, 90), (126, GROUND_Y + 2))
    g = QRadialGradient(102, GROUND_Y - 6, 36)
    g.setColorAt(0, _c("#000000"))
    g.setColorAt(1, _c("#2a1a0c"))
    p.fillPath(mouth, QBrush(g))
    _glow(p, 102, GROUND_Y - 10, 22, "#ffb020", 90)
    for x in (78, 126):                                 # timbered frame
        _log(p, QPointF(x, GROUND_Y + 4), QPointF(x, 84), 3.2, WOOD, rng)
    _log(p, QPointF(72, 88), QPointF(132, 88), 3.2, WOOD_DARK, rng)
    # A plank cart of amber on two wheels.
    cart = _poly((136, GROUND_Y - 2), (190, GROUND_Y - 2), (186, GROUND_Y - 22), (140, GROUND_Y - 22))
    _shadow_mass(p, 163, GROUND_Y + 4, 56, 26)
    for x, y, h, lean in ((148, GROUND_Y - 20, 16, -0.2), (160, GROUND_Y - 21, 22, 0.05), (172, GROUND_Y - 20, 17, 0.2),
                          (180, GROUND_Y - 21, 12, 0.3)):
        _crystal(p, x, y, h, "#e8961c", lean)
    _fill(p, cart, WOOD)
    _planks(p, cart, rng, WOOD)
    for x in (146, 180):
        wheel = QPainterPath()
        wheel.addEllipse(QPointF(x, GROUND_Y + 1), 7, 7)
        _fill(p, wheel, WOOD_DARK)
        p.setPen(_pen("#2a1a0c", 1.2))
        for a in range(0, 180, 45):
            r = math.radians(a)
            p.drawLine(QPointF(x - math.cos(r) * 6, GROUND_Y + 1 - math.sin(r) * 6),
                       QPointF(x + math.cos(r) * 6, GROUND_Y + 1 + math.sin(r) * 6))
    _log(p, QPointF(196, GROUND_Y + 4), QPointF(186, 108), 1.6, WOOD, rng)
    head = _poly((176, 104), (200, 108), (198, 113), (184, 112), (174, 116))
    _fill(p, head, "#8e949c")
    if owned:
        for x, y, h in ((52, GROUND_Y + 2, 12), (60, GROUND_Y + 4, 9)):
            _crystal(p, x, y, h, "#f2b33a")


def _nursery(p, rng, owned):
    legs = ((82, GROUND_Y + 4), (158, GROUND_Y + 4), (92, GROUND_Y - 8), (148, GROUND_Y - 8))
    for x, y in legs:
        _shadow_post(p, x, y, 70, 4)
    for x, y in legs[2:]:
        _log(p, QPointF(x, y), QPointF(x, 84), 2.2, WOOD, rng)
    # The silk cradle of eggs under the floor.
    p.setPen(_pen("#f4efe0", 1.0, 210))
    for i in range(7):
        p.drawLine(QPointF(92 + i * 9, 88), QPointF(100 + i * 7, 116))
    cradle = QPainterPath(QPointF(92, 104))
    cradle.cubicTo(QPointF(96, 132), QPointF(146, 132), QPointF(150, 104))
    cradle.closeSubpath()
    _fill(p, cradle, "#e6dfcc", light=108)
    for i in range(9):
        _egg(p, 102 + (i % 5) * 9 + (i // 5) * 4, 112 + (i // 5) * 8, 4.2, 3.8, "#efe4c0", rng)
    for x, y in legs[:2]:
        _log(p, QPointF(x, y), QPointF(x, 84), 2.6, WOOD, rng)
    floor = _poly((74, 88), (166, 88), (170, 80), (78, 80))
    _fill(p, floor, WOOD)
    _planks(p, floor, rng, WOOD, horizontal=False)
    walls = _poly((82, 80), (158, 80), (158, 56), (82, 56))
    _fill(p, walls, WOOD_OLD)
    _planks(p, walls, rng, WOOD_OLD, horizontal=False, nails=False)
    window = QPainterPath()
    window.addRect(QRectF(112, 62, 16, 12))
    p.fillPath(window, _c("#1a0f06"))
    _glow(p, 120, 68, 14, "#ffcc7a", 90)
    _thatch(p, _poly((70, 60), (92, 30), (148, 30), (170, 60)), rng)
    if owned:
        for x in (60, 182):                             # spiderlings
            p.setPen(_pen("#2b1a10", 1.2))
            for k in (-1, 1):
                for dy in (-3, 0, 3):
                    p.drawLine(QPointF(x, GROUND_Y - 2), QPointF(x + k * 8, GROUND_Y - 2 + dy + k))
            body = QPainterPath()
            body.addEllipse(QPointF(x, GROUND_Y - 2), 5, 4)
            _fill(p, body, "#7a4028")
            _spec(p, x - 1.5, GROUND_Y - 4, 1.8, 1.2, 220)


def _flynest(p, rng, owned):
    # A broken crate spilling rotten fruit.
    _shadow_mass(p, 118, GROUND_Y + 4, 100, 40)
    back = _poly((70, GROUND_Y - 4), (160, GROUND_Y - 4), (166, 100), (76, 100))
    _fill(p, back, WOOD_OLD)
    _planks(p, back, rng, WOOD_OLD)
    for x, y, r, tone in ((100, 100, 16, "#9a5a32"), (126, 96, 18, "#8a4a26"), (148, 104, 14, "#a4683a"),
                          (112, 88, 13, "#7a4a24")):
        fruit = _blob(x, y, r, r * 0.9, rng, 0.06)
        _fill(p, fruit, tone)
        p.save()
        p.setClipPath(fruit)
        for _ in range(3):
            spot = _blob(x + rng.uniform(-r * 0.5, r * 0.5), y + rng.uniform(-r * 0.5, r * 0.5), r * 0.3, r * 0.22,
                         rng, 0.3, 8)
            p.fillPath(spot, _c("#3d2a12", 200))
        p.restore()
        _spec(p, x - r * 0.35, y - r * 0.4, r * 0.3, r * 0.2, 120)
    front = _poly((64, GROUND_Y + 6), (154, GROUND_Y + 6), (158, 112), (68, 116))
    _fill(p, front, WOOD)
    _planks(p, front, rng, WOOD)
    broken = _poly((150, 112), (170, 108), (176, GROUND_Y + 4), (156, GROUND_Y + 6))
    _fill(p, broken, WOOD)
    _planks(p, broken, rng, WOOD, horizontal=False)
    for x0, y0 in ((66, 114), (150, 112)):
        _log(p, QPointF(x0, GROUND_Y + 6), QPointF(x0 + 2, y0), 1.8, WOOD_DARK, rng)
    for _ in range(16):                                 # flies
        a = rng.uniform(0, math.tau)
        r = rng.uniform(40, 76)
        x, y = 118 + math.cos(a) * r, 78 + math.sin(a) * r * 0.5
        p.setPen(Qt.NoPen)
        p.setBrush(_c("#dfe8f0", 150))
        p.drawEllipse(QPointF(x - 2.4, y - 2), 2.6, 1.6)
        p.drawEllipse(QPointF(x + 2.4, y - 2), 2.6, 1.6)
        p.setBrush(_c("#141414"))
        p.drawEllipse(QPointF(x, y), 2.0, 1.4)


PAINTERS = {"home": _home, "food": _food, "silk": _silk, "hatchery": _hatchery, "nest": _nest,
            "outpost": _outpost, "infestation": _infestation, "venom": _venom, "lookout": _lookout,
            "amber": _amber, "nursery": _nursery, "flynest": _flynest}
GROUND_TONES = {"home": "#8a7050", "food": "#86744c", "silk": "#84744e", "hatchery": "#7a6c48",
                "nest": "#5e4c3c", "outpost": "#86744c", "infestation": "#5a5a48", "venom": "#6e5e4c",
                "lookout": "#86744c", "amber": "#8a7048", "nursery": "#7e7448", "flynest": "#7a6444"}


@lru_cache(maxsize=32)
def building_art(kind, owned):
    """Painted once per building and owner, at twice the size, then reused."""
    image = QImage(ART_W * SCALE, ART_H * SCALE, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    p.scale(SCALE, SCALE)
    rng = random.Random(f"{kind}-37")
    _ground(p, rng, GROUND_TONES.get(kind, "#86744c"))
    PAINTERS.get(kind, _hatchery)(p, rng, owned)
    _pennant(p, owned, rng)
    p.end()
    image.setDevicePixelRatio(SCALE)
    return image
