"""Recolour sprite-rig art without redrawing it.

Twenty-three of the forty-nine models are `sprite_rig`: their bodies and legs
are PNGs rather than procedural shapes. Until DC-48 the ``colors`` block did
not reach them at all, so a sprite model's art was fixed and only its
procedural trim -- joints, feet, pedipalps, antennae, eyes, blush -- took a
colour. The two halves of one spider could be set to disagree, and a palette
override silently did almost nothing.

The art turns out to be well suited to recolouring. Measured across all
twenty-three models, every one is **single-hue art whose form lives in
luminance**: hue interquartile range runs 0-26 degrees, mean saturation 0.09
to 0.66, and luminance spans nearly the full range. Replacing hue while
keeping saturation and value therefore leaves the shading, the fluff fringe,
the specular highlight and the fur texture exactly as drawn.

Two approaches were measured on `plush_curly_hybrid` (435,600 pixels across
six parts):

* Qt composition (screen a grey lift, multiply the target colour) takes
  12 ms for the whole model and is forty times faster -- but it was rejected
  after rendering both and looking at them. It muddies the art badly: the
  radial curl texture and the highlight are lost and every colour turns
  towards dark slate.
* The hue swap below takes about 85-110 ms for a whole model and keeps all
  of it.

Quantising the lookup key was tried and abandoned: it cuts the table from
44,767 entries to 1,403 and saves only a quarter of the time, because the
cost is the Python pixel loop rather than the colour conversion. The exact
version is kept for being both simpler and better.

That cost is paid once per model per palette, lazily, and only for a model
that actually carries an override -- an untinted model never enters this
module and stays pixel-identical to before.
"""

from __future__ import annotations

import colorsys

from PyQt5.QtGui import QImage, QPixmap

# Which palette entry each sprite part takes its colour from. A part with no
# entry here is never tinted: the shadow is a soft black blob that must stay
# neutral, and an unknown part is left as the artist drew it.
PART_COLOR_KEYS = {
    "abdomen": "body",
    "cephalothorax": "body",
    "leg_upper": "legs",
    "leg_lower": "legs",
    "leg_knuckle": "legs",
    "leg_tip": "leg_tip",
}

# Below this saturation a pixel carries no hue worth replacing -- white fluff,
# grey shading, the anti-aliased rim. Recolouring it would tint the highlights
# and make the fur look dyed rather than the body.
NEUTRAL_SATURATION = 0.08


def _target_hue(rgb) -> float | None:
    """The hue to swap in, or None when the target itself is neutral."""
    try:
        r, g, b = (max(0, min(255, int(c))) / 255.0 for c in rgb)
    except (TypeError, ValueError):
        return None
    hue, sat, _value = colorsys.rgb_to_hsv(r, g, b)
    if sat < NEUTRAL_SATURATION:
        # Asking for grey would flatten the art to grey. Leave it alone.
        return None
    return hue


def tint_pixmap(pixmap: QPixmap, rgb) -> QPixmap:
    """Return ``pixmap`` with its hue replaced by ``rgb``'s, shading intact.

    Saturation, value and alpha are taken from the source pixel, so the drawn
    form survives and only the colour changes.
    """
    hue = _target_hue(rgb)
    if hue is None or pixmap.isNull():
        return pixmap

    image = pixmap.toImage().convertToFormat(QImage.Format_ARGB32)
    width, height = image.width(), image.height()
    buffer = image.bits()
    buffer.setsize(image.byteCount())
    view = memoryview(buffer)
    stride = image.bytesPerLine()

    # Pixels repeat heavily in shaded art, so converting each distinct colour
    # once and reusing it is worth the dictionary.
    lookup: dict[tuple[int, int, int], tuple[int, int, int]] = {}
    for y in range(height):
        row = y * stride
        for x in range(width):
            i = row + x * 4
            if view[i + 3] == 0:
                continue
            # Qt's ARGB32 is little-endian in memory: B, G, R, A.
            key = (view[i + 2], view[i + 1], view[i])
            swapped = lookup.get(key)
            if swapped is None:
                _hue, sat, value = colorsys.rgb_to_hsv(
                    key[0] / 255.0, key[1] / 255.0, key[2] / 255.0)
                if sat < NEUTRAL_SATURATION:
                    swapped = (view[i], view[i + 1], view[i + 2])
                else:
                    red, green, blue = colorsys.hsv_to_rgb(hue, sat, value)
                    swapped = (int(blue * 255), int(green * 255), int(red * 255))
                lookup[key] = swapped
            view[i], view[i + 1], view[i + 2] = swapped

    return QPixmap.fromImage(image)


def palette_signature(overrides) -> tuple:
    """A hashable key for the parts of a palette that change sprite art.

    Only overridden keys that a part actually reads matter, so two spiders
    differing in eye colour alone share one set of tinted pixmaps.
    """
    if not isinstance(overrides, dict) or not overrides:
        return ()
    wanted = set(PART_COLOR_KEYS.values())
    return tuple(sorted(
        (str(key), tuple(int(c) for c in value))
        for key, value in overrides.items()
        if str(key) in wanted
        and isinstance(value, (list, tuple)) and len(value) == 3
    ))


def tint_assets(assets: dict, overrides) -> dict:
    """Tinted copies of the parts whose colour was overridden.

    Parts without an override, and the shadow, are passed through unchanged
    so an untouched model keeps exactly the pixels it shipped with.
    """
    signature = palette_signature(overrides)
    if not signature:
        return assets
    palette = dict(signature)
    tinted = {}
    for name, pixmap in assets.items():
        key = PART_COLOR_KEYS.get(name)
        rgb = palette.get(key) if key else None
        tinted[name] = tint_pixmap(pixmap, rgb) if rgb is not None else pixmap
    return tinted
