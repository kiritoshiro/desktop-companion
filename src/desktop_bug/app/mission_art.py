"""Raster artwork for the Adventure mission's woodland buildings.

Transparent PNG sprites keep the detailed game-art look while Qt composes the
right state for each captured site and caches it at the old 2x render size.
"""
from __future__ import annotations

from functools import lru_cache

from PyQt5.QtCore import QRectF, Qt
from PyQt5.QtGui import QImage, QPainter

from ..content.discovery import find_data_file

ART_W, ART_H = 240, 190
GROUND_Y = 139
SCALE = 2
SPRITE_H = 137.0
SPRITE_MAX_W = 218.0

BUILDING_KINDS = (
    "home", "food", "silk", "hatchery", "nest", "outpost", "infestation",
    "venom", "lookout", "amber", "nursery", "flynest",
)
_CAPTURED_ASSETS = {
    ("hatchery", True): "hatchery_claimed",
    ("nest", True): "nest_claimed",
    ("infestation", True): "infestation_destroyed",
}


@lru_cache(maxsize=15)
def _load_asset(name: str) -> QImage:
    path = find_data_file("assets", "mission_buildings", f"{name}.png")
    image = QImage(str(path))
    if image.isNull():
        raise FileNotFoundError(f"Mission building art could not be loaded: {path}")
    return image


@lru_cache(maxsize=32)
def building_art(kind: str, owned: bool) -> QImage:
    """Return the 2x transparent sprite for a building and its captured state."""
    kind = kind if kind in BUILDING_KINDS else "hatchery"
    asset_name = _CAPTURED_ASSETS.get((kind, bool(owned)), kind)
    source = _load_asset(asset_name)

    width = SPRITE_H * source.width() / source.height()
    height = SPRITE_H
    if width > SPRITE_MAX_W:
        width = SPRITE_MAX_W
        height = width * source.height() / source.width()

    image = QImage(ART_W * SCALE, ART_H * SCALE, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing)
    painter.setRenderHint(QPainter.SmoothPixmapTransform)
    painter.scale(SCALE, SCALE)
    rect = QRectF((ART_W - width) / 2.0, GROUND_Y - height, width, height)
    painter.drawImage(rect, source)
    painter.end()
    image.setDevicePixelRatio(SCALE)
    return image
