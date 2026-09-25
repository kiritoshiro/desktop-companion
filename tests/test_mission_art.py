"""The mission buildings are drawn, not circles (the owner: "need nicer designs
for these hatchery, enemy base and other. not just circles")."""

from __future__ import annotations

import pytest
from PyQt5.QtGui import QImage

from desktop_bug.app.mission_art import PAINTERS, building_art


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Painting needs the one Qt application."""


def _opaque(image: QImage) -> int:
    return sum(image.pixelColor(x, y).alpha() > 40
               for y in range(0, image.height(), 3) for x in range(0, image.width(), 3))


def _differs(a: QImage, b: QImage) -> int:
    return sum(a.pixel(x, y) != b.pixel(x, y)
               for y in range(0, a.height(), 3) for x in range(0, a.width(), 3))


@pytest.mark.parametrize("kind", sorted(PAINTERS))
def test_each_building_has_its_own_picture_and_shows_who_holds_it(kind):
    art = building_art(kind, False)
    assert _opaque(art) > 1000, "a building should fill a good part of its frame"
    assert building_art(kind, False) is art, "painted once, then cached"
    changed = _differs(art, building_art(kind, True))
    if kind in ("hatchery", "nest"):
        # Enemy works are redrawn when taken: sealed in silk, eyes out.
        assert changed > 100, (kind, changed)
    else:
        # A friendly site only changes its pennant, as the old art did.
        assert changed > 5, (kind, changed)


def test_the_buildings_differ_from_one_another():
    kinds = sorted(PAINTERS)
    for i, a in enumerate(kinds):
        for b in kinds[i + 1:]:
            assert _differs(building_art(a, False), building_art(b, False)) > 800, (a, b)
