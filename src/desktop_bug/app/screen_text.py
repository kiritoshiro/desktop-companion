"""Find the words on a frozen screen, without reading them.

The owner: *"if there is any text on the screen visible spiders could use it
to create something maybe like a silk or just eat the text."*

The spiders need *where* text is, not *what* it says, so nothing is read:
no OCR, no text leaves the picture. Text is found by its shape -- small,
dense, high-contrast marks in rows a line tall -- which is cheap enough to
run once when the raid starts, needs no extra package, and keeps whatever was
on screen private.

How: the picture's horizontal edges (a copy shifted one pixel, differenced)
are averaged into 4-pixel cells; cells busy with edges are marked; marked
cells that touch form blobs, and a blob a line tall and wider than high is a
word. Photos and icons are too tall or too solid to pass.
"""
from __future__ import annotations

from collections import deque

from PyQt5.QtCore import QRectF, Qt
from PyQt5.QtGui import QImage, QPainter

CELL = 4                 # logical pixels per cell
EDGE_THRESHOLD = 34      # mean edge strength (0..255) that marks a cell
MIN_H, MAX_H = 2, 7      # a word is 8 to 28 px tall
MIN_W = 3                # and at least 12 px wide
MAX_W = 28               # longer runs are split into word-sized pieces
MIN_FILL = 0.30
MAX_WORDS = 700


def find_text_boxes(image: QImage, offset=(0.0, 0.0), limit: int = MAX_WORDS) -> list[QRectF]:
    """Boxes around word-like marks in ``image``, overlay-local (``offset``
    is the image's top-left there). ``image`` may carry a device pixel ratio;
    boxes are in logical pixels."""
    if image.isNull():
        return []
    ratio = image.devicePixelRatio() or 1.0
    width = int(image.width() / ratio)
    height = int(image.height() / ratio)
    cols, rows = width // CELL, height // CELL
    if cols < MIN_W or rows < MIN_H:
        return []
    grey = image.convertToFormat(QImage.Format_Grayscale8).scaled(
        width, height, Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
    edges = QImage(grey.size(), QImage.Format_ARGB32_Premultiplied)
    edges.fill(Qt.black)
    p = QPainter(edges)
    p.drawImage(0, 0, grey)
    p.setCompositionMode(QPainter.CompositionMode_Difference)
    p.drawImage(1, 0, grey)
    p.end()
    # Smooth downscaling hands back a 32-bit image whatever went in, so the
    # conversion to one byte a cell comes after it.
    cells = edges.scaled(cols, rows, Qt.IgnoreAspectRatio, Qt.SmoothTransformation).convertToFormat(
        QImage.Format_Grayscale8)
    stride = cells.bytesPerLine()
    bits = cells.constBits()
    bits.setsize(cells.byteCount())
    data = bytes(bits)
    marked = set()
    for y in range(rows):
        row = data[y * stride:y * stride + cols]
        for x, value in enumerate(row):
            if value >= EDGE_THRESHOLD:
                marked.add((x, y))
    boxes = []
    seen = set()
    for start in marked:
        if start in seen:
            continue
        seen.add(start)
        queue = deque([start])
        x0 = x1 = start[0]
        y0 = y1 = start[1]
        count = 0
        while queue:
            x, y = queue.popleft()
            count += 1
            x0, x1, y0, y1 = min(x0, x), max(x1, x), min(y0, y), max(y1, y)
            if y1 - y0 >= MAX_H + 3:
                continue      # already far too tall to be text; stop growing it
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    n = (x + dx, y + dy)
                    if n in marked and n not in seen:
                        seen.add(n)
                        queue.append(n)
        w, h = x1 - x0 + 1, y1 - y0 + 1
        if not (MIN_H <= h <= MAX_H and w >= MIN_W and w >= h * 1.2):
            continue
        if count / float(w * h) < MIN_FILL:
            continue
        boxes.append((x0, y0, w, h))
    boxes = _merge_rows(boxes)
    out = []
    ox, oy = offset
    for x0, y0, w, h in boxes:
        pieces = max(1, round(w / MAX_W)) if w > MAX_W else 1
        step = w / pieces
        for i in range(pieces):
            out.append(QRectF(ox + (x0 + i * step) * CELL, oy + y0 * CELL, step * CELL, h * CELL))
    out.sort(key=lambda r: (r.y(), r.x()))
    return out[:limit]


def _merge_rows(boxes):
    """Join blobs on one line with at most one empty cell between: letters of
    a word that did not quite touch."""
    boxes = sorted(boxes, key=lambda b: (b[1], b[0]))
    merged = []
    for box in boxes:
        x0, y0, w, h = box
        for i, (mx, my, mw, mh) in enumerate(merged):
            overlap = min(y0 + h, my + mh) - max(y0, my)
            gap = x0 - (mx + mw)
            if overlap >= min(h, mh) * 0.6 and -mw <= gap <= 1 and max(y0 + h, my + mh) - min(y0, my) <= MAX_H:
                nx0, ny0 = min(mx, x0), min(my, y0)
                merged[i] = (nx0, ny0, max(mx + mw, x0 + w) - nx0, max(my + mh, y0 + h) - ny0)
                break
        else:
            merged.append(box)
    return merged
