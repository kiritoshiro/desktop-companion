"""The frozen desktop as something spiders can damage.

The owner: *"enemies spit acid on spiders but it would also melt a hole in
... any open window, and a desktop part would be visible and later even
desktop would be broken bit by bit. also some spiders, big ones, when walking
or jumping could crack the desktop, and later even the whole screen glass
seemingly. ... spiders could use [text] to create something maybe like a silk
or just eat the text."*

Each monitor is three layers, top to bottom:

- **glass and windows** -- the screenshot, with cracks drawn over it;
- **desktop** -- the wallpaper, shown where acid melted through a window;
- **void** -- what is left when the desktop itself is eaten away.

Acid on a window melts the window and shows the desktop behind it; acid on
the desktop (or on a hole already melted) melts the desktop and shows the
void. Heavy landings crack the glass; enough cracks and a whole screen's
glass shatters and falls away, leaving its desktop bare. Words on the frozen
screen can be eaten, letter by letter.

This is only a picture. The real windows underneath are never touched, and
when the raid ends the picture goes and the real desktop is back.
"""
from __future__ import annotations

import math
import random
from dataclasses import dataclass, field

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import (QColor, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QPolygonF,
                         QRadialGradient)

from .desktop_capture import DesktopSnapshot
from .screen_text import find_text_boxes

# How much one crack of full force stresses a screen's glass; at 1.0 it shatters.
STRESS_PER_CRACK = 0.11
SHARD_SECONDS = 2.4
# Spacing of the grid on which the desktop's remaining area is sampled.
INTEGRITY_STEP = 32


@dataclass
class Word:
    rect: QRectF                # overlay-local
    colour: QColor              # the background it is erased to
    screen: int
    eaten: float = 0.0

    @property
    def done(self) -> bool:
        return self.eaten >= 1.0

    @property
    def centre(self) -> tuple[float, float]:
        c = self.rect.center()
        return c.x(), c.y()


@dataclass
class Shard:
    polygon: QPolygonF          # screen-local
    cx: float
    cy: float
    vx: float
    vy: float
    spin: float
    age: float = 0.0
    dx: float = 0.0
    dy: float = 0.0
    angle: float = 0.0


@dataclass
class ScreenLayers:
    index: int
    x: float
    y: float
    w: float
    h: float
    top: QImage
    under: QImage
    void: QImage
    glass: QImage
    frame: QImage
    stress: float = 0.0
    shattered: bool = False
    shards: list = field(default_factory=list)
    pane: QImage | None = None

    @property
    def rect(self) -> QRectF:
        return QRectF(self.x, self.y, self.w, self.h)

    def local(self, x, y):
        return x - self.x, y - self.y


class DesktopSurface:
    def __init__(self, snapshot: DesktopSnapshot, seed: int | None = None, find_text: bool = True):
        self.rng = random.Random(seed)
        self.snapshot = snapshot          # kept for Retry: a fresh surface from the same picture
        self.windows = list(snapshot.windows)
        self.primary = snapshot.primary
        self.layers: list[ScreenLayers] = []
        self.words: list[Word] = []
        self.version = 0
        self._integrity = None
        for index, shot in enumerate(snapshot.screens):
            ratio = shot.image.devicePixelRatio() or 1.0
            top = shot.image.convertToFormat(QImage.Format_ARGB32_Premultiplied)
            top.setDevicePixelRatio(ratio)
            under = self._desktop_layer(shot, top)
            void = _void_layer(top.width(), top.height(), ratio, self.rng)
            glass = QImage(top.size(), QImage.Format_ARGB32_Premultiplied)
            glass.fill(Qt.transparent)
            glass.setDevicePixelRatio(ratio)
            frame = QImage(top.size(), QImage.Format_RGB32)
            frame.setDevicePixelRatio(ratio)
            r = shot.rect
            layer = ScreenLayers(index, r.x, r.y, r.w, r.h, top, under, void, glass, frame)
            self.layers.append(layer)
            self._compose(layer, QRectF(0, 0, r.w, r.h))
            if find_text:
                for box in find_text_boxes(top, (r.x, r.y)):
                    self.words.append(Word(box, self._background_of(layer, box), index))

    @staticmethod
    def _desktop_layer(shot, top):
        if shot.wallpaper is not None and not shot.wallpaper.isNull():
            under = shot.wallpaper.convertToFormat(QImage.Format_ARGB32_Premultiplied)
        else:
            # No wallpaper file: a plain desktop in the screenshot's own tint.
            under = QImage(top.size(), QImage.Format_ARGB32_Premultiplied)
            gradient = QLinearGradient(0, 0, 0, top.height())
            gradient.setColorAt(0, QColor(28, 72, 104))
            gradient.setColorAt(1, QColor(12, 34, 58))
            p = QPainter(under)
            p.fillRect(under.rect(), gradient)
            p.end()
        under.setDevicePixelRatio(top.devicePixelRatio())
        return under

    # -- queries -----------------------------------------------------------
    def layer_at(self, x: float, y: float) -> ScreenLayers | None:
        for layer in self.layers:
            if layer.x <= x < layer.x + layer.w and layer.y <= y < layer.y + layer.h:
                return layer
        return None

    @staticmethod
    def _alpha(image: QImage, lx: float, ly: float) -> int:
        ratio = image.devicePixelRatio() or 1.0
        px, py = int(lx * ratio), int(ly * ratio)
        if 0 <= px < image.width() and 0 <= py < image.height():
            return QColor.fromRgba(image.pixel(px, py)).alpha()
        return 0

    def in_window(self, x: float, y: float) -> bool:
        return any(w.contains(QPointF(x, y)) for w in self.windows)

    def what_is_at(self, x: float, y: float) -> str:
        """"window", "desktop" or "void": the layer seen at a point."""
        layer = self.layer_at(x, y)
        if layer is None:
            return "void"
        lx, ly = layer.local(x, y)
        if self._alpha(layer.top, lx, ly) > 0:
            return "window" if self.in_window(x, y) else "desktop"
        return "desktop" if self._alpha(layer.under, lx, ly) > 0 else "void"

    @property
    def integrity(self) -> float:
        """How much of the desktop is left, 0..1: the share of the screens
        where the void does not show."""
        if self._integrity is None:
            total = left = 0
            for layer in self.layers:
                y = INTEGRITY_STEP / 2
                while y < layer.h:
                    x = INTEGRITY_STEP / 2
                    while x < layer.w:
                        total += 1
                        if self._alpha(layer.under, x, y) > 0 or self._alpha(layer.top, x, y) > 0:
                            left += 1
                        x += INTEGRITY_STEP
                    y += INTEGRITY_STEP
            self._integrity = left / total if total else 1.0
        return self._integrity

    def living_words(self) -> list[Word]:
        return [w for w in self.words if not w.done and self._word_visible(w)]

    def _word_visible(self, word: Word) -> bool:
        layer = self.layers[word.screen]
        lx, ly = layer.local(*word.centre)
        return self._alpha(layer.top, lx, ly) > 0

    def word_near(self, x: float, y: float, reach: float) -> Word | None:
        best, best_d = None, reach
        for word in self.words:
            if word.done:
                continue
            r = word.rect
            dx = max(r.left() - x, 0.0, x - r.right())
            dy = max(r.top() - y, 0.0, y - r.bottom())
            d = math.hypot(dx, dy)
            if d <= best_d and self._word_visible(word):
                best, best_d = word, d
        return best

    # -- damage ------------------------------------------------------------
    def melt(self, x: float, y: float, radius: float) -> str | None:
        """Acid at a point: melts what is on top there. Returns what was melted
        ("window" or "desktop"), or None when only the void was left."""
        layer = self.layer_at(x, y)
        if layer is None:
            return None
        lx, ly = layer.local(x, y)
        blob = _blob(lx, ly, radius, self.rng)
        if self._alpha(layer.top, lx, ly) > 0:
            result = "window" if self.in_window(x, y) else "desktop"
            _burn_away(layer.top, blob, radius, self.rng)
            if result == "desktop":
                _burn_away(layer.under, blob, radius, self.rng)
        elif self._alpha(layer.under, lx, ly) > 0:
            result = "desktop"
            _burn_away(layer.under, blob, radius, self.rng)
        else:
            return None
        pad = radius * 1.7
        self._compose(layer, QRectF(lx - pad, ly - pad, pad * 2, pad * 2 + radius * 1.5))
        self._integrity = None
        return result

    def crack(self, x: float, y: float, force: float) -> bool:
        """A heavy landing: cracks radiate from the point. True if the screen's
        glass shattered from it."""
        layer = self.layer_at(x, y)
        if layer is None or layer.shattered:
            return False
        force = max(0.05, min(1.0, force))
        lx, ly = layer.local(x, y)
        area = _draw_cracks(layer.glass, lx, ly, force, self.rng)
        self._compose(layer, area)
        layer.stress += force * STRESS_PER_CRACK
        if layer.stress >= 1.0:
            self.shatter(layer.index)
            return True
        return False

    def shatter(self, index: int) -> None:
        """The whole glass of one screen breaks and falls away; its desktop
        (with any holes already melted in it) is left bare."""
        layer = self.layers[index]
        if layer.shattered:
            return
        layer.shattered = True
        pane = QImage(layer.top.size(), QImage.Format_ARGB32_Premultiplied)
        pane.setDevicePixelRatio(layer.top.devicePixelRatio())
        pane.fill(Qt.transparent)
        p = QPainter(pane)
        p.drawImage(0, 0, layer.top)
        p.drawImage(0, 0, layer.glass)
        p.end()
        layer.pane = pane
        layer.shards = _shards(layer.w, layer.h, self.rng)
        layer.top.fill(Qt.transparent)
        layer.glass.fill(Qt.transparent)
        self._compose(layer, QRectF(0, 0, layer.w, layer.h))
        self._integrity = None

    def eat(self, word: Word, amount: float) -> bool:
        """Eat ``amount`` (0..1) more of a word, left to right. True when gone."""
        if word.done:
            return True
        layer = self.layers[word.screen]
        before = word.eaten
        word.eaten = min(1.0, word.eaten + amount)
        r = word.rect.translated(-layer.x, -layer.y)
        bite = QRectF(r.x() + r.width() * before - 1, r.y() - 1, r.width() * (word.eaten - before) + 2, r.height() + 2)
        p = QPainter(layer.top)
        p.setCompositionMode(QPainter.CompositionMode_SourceAtop)
        p.fillRect(bite, word.colour)
        p.end()
        self._compose(layer, bite.adjusted(-2, -2, 2, 2))
        return word.done

    def update(self, dt: float) -> None:
        for layer in self.layers:
            if not layer.shards:
                continue
            for shard in layer.shards:
                shard.age += dt
                shard.vy += 900.0 * dt
                shard.dx += shard.vx * dt
                shard.dy += shard.vy * dt
                shard.angle += shard.spin * dt
            if all(s.age >= SHARD_SECONDS for s in layer.shards):
                layer.shards = []
                layer.pane = None

    # -- painting ----------------------------------------------------------
    def _compose(self, layer: ScreenLayers, area: QRectF) -> None:
        area = area.intersected(QRectF(0, 0, layer.w, layer.h))
        if area.isEmpty():
            return
        p = QPainter(layer.frame)
        p.setClipRect(area.toAlignedRect())
        p.drawImage(0, 0, layer.void)
        p.drawImage(0, 0, layer.under)
        p.drawImage(0, 0, layer.top)
        p.drawImage(0, 0, layer.glass)
        p.end()
        self.version += 1

    def paint(self, painter: QPainter) -> None:
        for layer in self.layers:
            painter.drawImage(QPointF(layer.x, layer.y), layer.frame)
            if layer.shards and layer.pane is not None:
                self._paint_shards(painter, layer)

    @staticmethod
    def _paint_shards(painter, layer):
        for shard in layer.shards:
            if shard.age >= SHARD_SECONDS:
                continue
            painter.save()
            painter.translate(layer.x + shard.cx + shard.dx, layer.y + shard.cy + shard.dy)
            painter.rotate(math.degrees(shard.angle))
            painter.translate(-shard.cx, -shard.cy)
            painter.setOpacity(max(0.0, 1.0 - shard.age / SHARD_SECONDS))
            path = QPainterPath()
            path.addPolygon(shard.polygon)
            path.closeSubpath()
            painter.setClipPath(path)
            painter.drawImage(0, 0, layer.pane)
            painter.setClipping(False)
            painter.setPen(QPen(QColor(230, 245, 255, 150), 1.2))
            painter.setBrush(Qt.NoBrush)
            painter.drawPath(path)
            painter.restore()

    def _background_of(self, layer: ScreenLayers, box: QRectF) -> QColor:
        """The colour around a word: the average of the pixels just outside it."""
        r = box.translated(-layer.x, -layer.y)
        ratio = layer.top.devicePixelRatio() or 1.0
        samples = []
        for fx in (0.0, 0.25, 0.5, 0.75, 1.0):
            for y in (r.top() - 2, r.bottom() + 2):
                px, py = int((r.left() + r.width() * fx) * ratio), int(y * ratio)
                if 0 <= px < layer.top.width() and 0 <= py < layer.top.height():
                    samples.append(QColor(layer.top.pixel(px, py)))
        if not samples:
            return QColor(240, 240, 240)
        n = len(samples)
        return QColor(sum(c.red() for c in samples) // n, sum(c.green() for c in samples) // n,
                      sum(c.blue() for c in samples) // n)


# -- drawing helpers ---------------------------------------------------------

def _blob(x, y, radius, rng) -> QPainterPath:
    """An irregular round hole."""
    points = 16
    path = QPainterPath()
    for i in range(points + 1):
        a = i / points * math.tau
        r = radius * (0.78 + 0.3 * rng.random()) if i < points else None
        if i == 0:
            first = r
            path.moveTo(x + math.cos(a) * r, y + math.sin(a) * r)
        elif i == points:
            path.lineTo(x + first, y)
        else:
            path.lineTo(x + math.cos(a) * r, y + math.sin(a) * r)
    return path


def _burn_away(image: QImage, blob: QPainterPath, radius: float, rng) -> None:
    """Scorch a ring round the hole, run a few drips down, then melt it through."""
    centre = blob.boundingRect().center()
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    p.setCompositionMode(QPainter.CompositionMode_SourceAtop)
    scorch = QRadialGradient(centre, radius * 1.45)
    scorch.setColorAt(0.0, QColor(40, 60, 10, 230))
    scorch.setColorAt(0.62, QColor(58, 72, 18, 210))
    scorch.setColorAt(0.8, QColor(90, 70, 20, 120))
    scorch.setColorAt(1.0, QColor(90, 70, 20, 0))
    p.setPen(Qt.NoPen)
    p.setBrush(scorch)
    p.drawEllipse(centre, radius * 1.45, radius * 1.45)
    for _ in range(rng.randint(2, 4)):
        dx = (rng.random() - 0.5) * radius * 1.3
        length = radius * (0.6 + rng.random() * 1.4)
        top = QPointF(centre.x() + dx, centre.y() + radius * 0.5)
        p.setPen(QPen(QColor(120, 170, 30, 190), 2.0 + rng.random() * 2.5, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(top, QPointF(top.x(), top.y() + length))
    p.setCompositionMode(QPainter.CompositionMode_Clear)
    p.setPen(Qt.NoPen)
    p.setBrush(Qt.black)
    p.drawPath(blob)
    # A glowing rim, where anything is left to glow.
    p.setCompositionMode(QPainter.CompositionMode_SourceAtop)
    p.setBrush(Qt.NoBrush)
    p.setPen(QPen(QColor(190, 255, 70, 210), 3.0))
    p.drawPath(blob)
    p.end()


def _draw_cracks(glass: QImage, x: float, y: float, force: float, rng) -> QRectF:
    """Jagged cracks radiating from a point; returns the area drawn over."""
    rays = 3 + int(force * 6)
    lines = []
    for i in range(rays):
        angle = (i + rng.random() * 0.8) / rays * math.tau
        length = (60 + rng.random() * 90) * (0.6 + force * 1.6)
        _crack_ray(lines, x, y, angle, length, rng, depth=0)
    p = QPainter(glass)
    p.setRenderHint(QPainter.Antialiasing)
    for (ax, ay), (bx, by) in lines:
        p.setPen(QPen(QColor(0, 0, 0, 120), 2.6, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(QPointF(ax + 0.8, ay + 0.8), QPointF(bx + 0.8, by + 0.8))
    for (ax, ay), (bx, by) in lines:
        p.setPen(QPen(QColor(236, 246, 255, 215), 1.1, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(QPointF(ax, ay), QPointF(bx, by))
    # The impact: a small crushed star.
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(255, 255, 255, 70))
    p.drawEllipse(QPointF(x, y), 4 + force * 8, 4 + force * 8)
    p.end()
    xs = [pt[0] for seg in lines for pt in seg] + [x]
    ys = [pt[1] for seg in lines for pt in seg] + [y]
    return QRectF(min(xs) - 4, min(ys) - 4, max(xs) - min(xs) + 8, max(ys) - min(ys) + 8)


def _crack_ray(lines, x, y, angle, length, rng, depth):
    travelled = 0.0
    while travelled < length:
        step = 10 + rng.random() * 16
        angle += (rng.random() - 0.5) * 0.7
        nx, ny = x + math.cos(angle) * step, y + math.sin(angle) * step
        lines.append(((x, y), (nx, ny)))
        x, y = nx, ny
        travelled += step
        if depth < 2 and rng.random() < 0.14:
            _crack_ray(lines, x, y, angle + (rng.random() - 0.5) * 1.6, (length - travelled) * 0.6, rng, depth + 1)


def _shards(w, h, rng) -> list[Shard]:
    """The glass broken into triangles on a jittered grid."""
    cols, rows = 8, 5
    grid = []
    for j in range(rows + 1):
        row = []
        for i in range(cols + 1):
            jx = 0 if i in (0, cols) else (rng.random() - 0.5) * w / cols * 0.7
            jy = 0 if j in (0, rows) else (rng.random() - 0.5) * h / rows * 0.7
            row.append(QPointF(w * i / cols + jx, h * j / rows + jy))
        grid.append(row)
    shards = []
    for j in range(rows):
        for i in range(cols):
            a, b, c, d = grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]
            for tri in ((a, b, c), (a, c, d)) if rng.random() < 0.5 else ((a, b, d), (b, c, d)):
                cx = sum(pt.x() for pt in tri) / 3
                cy = sum(pt.y() for pt in tri) / 3
                shards.append(Shard(QPolygonF(list(tri)), cx, cy,
                                    vx=(cx - w / 2) / w * 220 + (rng.random() - 0.5) * 80,
                                    vy=-60 - rng.random() * 120,
                                    spin=(rng.random() - 0.5) * 3.0,
                                    age=-(rng.random() * 0.35)))
    return shards


def _void_layer(width: int, height: int, ratio: float, rng) -> QImage:
    """What lies under the desktop: black depth, a faint grid, stray pixels."""
    image = QImage(width, height, QImage.Format_ARGB32_Premultiplied)
    image.setDevicePixelRatio(ratio)
    w, h = width / ratio, height / ratio
    p = QPainter(image)
    gradient = QRadialGradient(QPointF(w / 2, h / 2), max(w, h) * 0.7)
    gradient.setColorAt(0.0, QColor(18, 10, 30))
    gradient.setColorAt(1.0, QColor(3, 2, 8))
    p.fillRect(QRectF(0, 0, w, h), gradient)
    p.setPen(QPen(QColor(80, 40, 140, 38), 1))
    step = 48
    x = 0
    while x < w:
        p.drawLine(QPointF(x, 0), QPointF(x, h))
        x += step
    y = 0
    while y < h:
        p.drawLine(QPointF(0, y), QPointF(w, y))
        y += step
    p.setPen(Qt.NoPen)
    for _ in range(int(w * h / 9000)):
        p.setBrush(QColor(rng.choice((150, 90, 60)), rng.randint(40, 200), rng.randint(120, 255), rng.randint(40, 140)))
        size = rng.choice((2, 2, 3, 4))
        p.drawRect(QRectF(rng.random() * w, rng.random() * h, size, size))
    p.end()
    return image
