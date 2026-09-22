"""Sprite-rig art takes the palette too (DC-48).

Twenty-three of the forty-nine models draw their body and legs from PNGs.
Until this package the ``colors`` block never reached them: a colour
override repainted the procedural trim -- joints, feet, antennae, eyes --
and left the art alone, so half the spider changed and half did not.

The art is single-hue with its form carried in luminance, which is why
swapping hue while keeping saturation and value reads as native rather than
as a dyed sprite. These tests pin both halves of that: the recolour really
happens, and the drawn form survives it.

The first test is the one that matters most. A model with no override must
come out byte-identical to what it shipped with, because otherwise this
package silently restyles forty-nine creatures.
"""

from __future__ import annotations

import colorsys
import json
from pathlib import Path

import pytest
from desktop_bug.creature.core import Creature
from desktop_bug.creature.sprite_tint import (
    NEUTRAL_SATURATION,
    PART_COLOR_KEYS,
    palette_signature,
    tint_pixmap,
)
from PyQt5.QtGui import QImage, QPixmap, qAlpha, qBlue, qGreen, qRed

ROOT = Path(__file__).resolve().parents[1]
SPRITE_MODEL = "plush_curly_hybrid"


@pytest.fixture(scope="module")
def traits():
    return json.loads((ROOT / "personalities" / "mellow.json").read_text(encoding="utf-8"))


def _model(model_id: str) -> dict:
    folder = ROOT / "models" / model_id
    data = json.loads((folder / "model.json").read_text(encoding="utf-8"))
    data["_folder"] = str(folder.resolve())
    return data


def _creature(qapp, traits, model_id=SPRITE_MODEL, overrides=None) -> Creature:
    return Creature(_model(model_id), traits, 300, 300, index=0, color_overrides=overrides)


def _pixels(pixmap: QPixmap):
    image = pixmap.toImage().convertToFormat(QImage.Format_ARGB32)
    for y in range(0, image.height(), 3):
        for x in range(0, image.width(), 3):
            px = image.pixel(x, y)
            if qAlpha(px) > 0:
                yield qRed(px), qGreen(px), qBlue(px), qAlpha(px)


def test_a_model_without_an_override_is_untouched(qapp, traits):
    """The regression this package must not cause."""
    plain = _creature(qapp, traits)
    assets = plain._load_sprite_assets()
    assert assets, "the fixture model has no sprite assets"

    for name, pixmap in assets.items():
        source = QPixmap(str(ROOT / "models" / SPRITE_MODEL / "assets" / f"{name}.png"))
        assert pixmap.toImage() == source.toImage(), (
            f"{name}.png was altered for a spider with no colour override"
        )


def test_an_override_actually_recolours_the_art(qapp, traits):
    plain = _creature(qapp, traits)._load_sprite_assets()
    blue = _creature(qapp, traits, overrides={"body": [60, 105, 205]})._load_sprite_assets()
    assert plain["abdomen"].toImage() != blue["abdomen"].toImage(), (
        "the body override did not change the abdomen art"
    )


def test_only_the_overridden_parts_change(qapp, traits):
    """A body override must not repaint the legs."""
    plain = _creature(qapp, traits)._load_sprite_assets()
    blue = _creature(qapp, traits, overrides={"body": [60, 105, 205]})._load_sprite_assets()
    assert plain["leg_upper"].toImage() == blue["leg_upper"].toImage()
    assert plain["abdomen"].toImage() != blue["abdomen"].toImage()


def test_the_shadow_is_never_tinted(qapp, traits):
    """It is a soft neutral blob; colouring it would ring the spider."""
    assert "shadow" not in PART_COLOR_KEYS
    plain = _creature(qapp, traits)._load_sprite_assets()
    green = _creature(qapp, traits,
                      overrides={"body": [40, 160, 60], "legs": [40, 160, 60],
                                 "leg_tip": [40, 160, 60]})._load_sprite_assets()
    if "shadow" in plain:
        assert plain["shadow"].toImage() == green["shadow"].toImage()


def test_the_drawn_form_survives_the_recolour(qapp, traits):
    """Shading lives in luminance, so luminance must come through unchanged.

    This is the whole reason a hue swap was chosen over compositing a colour
    over the art, which was measured at forty times faster and rejected for
    flattening the texture.
    """
    source = QPixmap(str(ROOT / "models" / SPRITE_MODEL / "assets" / "abdomen.png"))
    tinted = tint_pixmap(source, [60, 105, 205])

    before = list(_pixels(source))
    after = list(_pixels(tinted))
    assert len(before) == len(after)

    for (r0, g0, b0, a0), (r1, g1, b1, a1) in zip(before, after):
        assert a0 == a1, "alpha changed, so the silhouette moved"
        _h0, s0, v0 = colorsys.rgb_to_hsv(r0 / 255, g0 / 255, b0 / 255)
        _h1, s1, v1 = colorsys.rgb_to_hsv(r1 / 255, g1 / 255, b1 / 255)
        # Value is asserted exactly, not within a tolerance: HSV's V is
        # max(r,g,b), and the swap rebuilds the pixel at the same V, so
        # measured drift across the whole sprite is 0. If shading ever starts
        # sliding, this catches it on the first pixel.
        assert v0 == v1, ("luminance moved, so the shading changed", v0, v1)
        # Saturation is only preserved to 8-bit precision: rebuilding a colour
        # at a new hue lands on a neighbouring byte. Measured worst case over
        # this sprite is 0.0244.
        assert abs(s0 - s1) <= 0.03, ("saturation drifted too far", s0, s1)


def test_the_hue_really_becomes_the_requested_one(qapp, traits):
    source = QPixmap(str(ROOT / "models" / SPRITE_MODEL / "assets" / "abdomen.png"))
    want_rgb = [60, 105, 205]
    want_hue, _s, _v = colorsys.rgb_to_hsv(*(c / 255 for c in want_rgb))
    tinted = tint_pixmap(source, want_rgb)

    # Only pixels that carry real colour can carry an accurate hue. In 8-bit
    # RGB a dark or washed-out pixel has very few bits of hue left: measured
    # on this sprite, the error is 0.0008 at min(s,v)~0.7 and 0.05 at ~0.1.
    # Checking those would be testing the colour space, not the code.
    checked = 0
    for r, g, b, _a in _pixels(tinted):
        hue, sat, value = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
        if sat < NEUTRAL_SATURATION:
            continue  # near-white fluff is deliberately left alone
        if min(sat, value) < 0.3:
            continue
        gap = abs(hue - want_hue)
        assert min(gap, 1.0 - gap) < 0.025, (hue, want_hue, sat, value)
        checked += 1
    assert checked > 1000, "almost nothing was coloured, so this proved little"


def test_asking_for_grey_leaves_the_art_alone(qapp, traits):
    """A neutral target has no hue to swap in; flattening to grey is worse."""
    source = QPixmap(str(ROOT / "models" / SPRITE_MODEL / "assets" / "abdomen.png"))
    assert tint_pixmap(source, [128, 128, 128]).toImage() == source.toImage()


def test_two_palettes_do_not_share_one_cache_entry(qapp, traits):
    """The cache used to be keyed on the folder alone, so the first spider
    of a model would have decided the colour for every later one."""
    blue = _creature(qapp, traits, overrides={"body": [60, 105, 205]})._load_sprite_assets()
    green = _creature(qapp, traits, overrides={"body": [40, 160, 60]})._load_sprite_assets()
    assert blue["abdomen"].toImage() != green["abdomen"].toImage()


def test_the_signature_ignores_colours_no_sprite_part_reads(qapp, traits):
    """Two spiders differing only in eye colour should share tinted art."""
    assert palette_signature({"eyes": [1, 2, 3]}) == ()
    assert palette_signature({"body": [60, 105, 205]}) != ()
    assert palette_signature(None) == ()
    assert palette_signature({}) == ()


def test_a_procedural_model_is_unaffected(qapp, traits):
    """Twenty-six models have no art at all and must not go near this."""
    plain = _creature(qapp, traits, model_id="spider",
                      overrides={"body": [60, 105, 205]})
    assert plain._load_sprite_assets() == {}
