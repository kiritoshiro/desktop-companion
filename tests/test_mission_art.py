"""The mission buildings use generated, transparent game-art sprites."""

from __future__ import annotations

import pytest
from PyQt5.QtGui import QImage

from desktop_bug.app.mission_art import ART_H, ART_W, BUILDING_KINDS, GROUND_Y, SCALE, building_art


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Loading and composing the sprites needs the one Qt application."""


def _opaque(image: QImage) -> int:
    return sum(image.pixelColor(x, y).alpha() > 40
               for y in range(0, image.height(), 3) for x in range(0, image.width(), 3))


def _differs(a: QImage, b: QImage) -> int:
    return sum(a.pixel(x, y) != b.pixel(x, y)
               for y in range(0, a.height(), 3) for x in range(0, a.width(), 3))


@pytest.mark.parametrize("kind", BUILDING_KINDS)
def test_every_building_loads_its_own_cached_transparent_png(kind):
    art = building_art(kind, False)
    assert not art.isNull()
    assert _opaque(art) > 1000, f"{kind} should fill a useful part of its frame"
    assert building_art(kind, False) is art, "composed art is cached"
    assert art.pixelColor(0, 0).alpha() == 0
    assert art.pixelColor(ART_W * SCALE - 1, ART_H * SCALE - 1).alpha() == 0


def test_building_sprites_share_the_ground_anchor_and_hidpi_canvas():
    for kind in BUILDING_KINDS:
        art = building_art(kind, False)
        assert art.devicePixelRatio() == SCALE
        assert art.width() == ART_W * SCALE and art.height() == ART_H * SCALE
        ys = [y for y in range(art.height()) if any(art.pixelColor(x, y).alpha() > 8
                                                     for x in range(art.width()))]
        assert ys and abs((ys[-1] + 1) - GROUND_Y * SCALE) <= 2, (kind, ys[-1] if ys else None)


def test_only_sites_with_capture_art_change_their_sprite_state():
    captured = {"hatchery", "nest", "infestation"}
    for kind in BUILDING_KINDS:
        changed = _differs(building_art(kind, False), building_art(kind, True))
        if kind in captured:
            assert changed > 100, (kind, changed)
        else:
            assert changed == 0, (kind, changed)


def test_buildings_differ_from_one_another():
    for i, a in enumerate(BUILDING_KINDS):
        for b in BUILDING_KINDS[i + 1:]:
            assert _differs(building_art(a, False), building_art(b, False)) > 800, (a, b)


def test_unknown_building_kind_keeps_the_existing_hatchery_fallback():
    assert _differs(building_art("unknown", False), building_art("hatchery", False)) == 0
