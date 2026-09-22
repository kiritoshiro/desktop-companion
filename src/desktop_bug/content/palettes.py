"""Named colour palettes for a spider, and a way to roll a fresh one.

Two things needed the same thing at the same time, so they share it.

The settings window's colour dialog asked for seven colours one at a time,
each behind its own system colour picker, with no way to say "make this one
look like that one". The owner asked for presets embedded in that picker.

Separately, a base that raises a new spider (DC-55) has to give it a
appearance, and the owner asked for those to be random. A random colour per
key would produce a spider with a green body, pink legs and orange feet; what
"random colours" means is a random *palette*.

So a palette here is derived from a single hue rather than listed as seven
free choices. A spider is mostly one colour in shade, with its bands and
highlights a little brighter and warmer, and its eyes brighter still so they
read as eyes. That relationship is what makes the shipped
models look like animals, and it is what a hand-mixed set of seven sliders
usually loses.
"""

from __future__ import annotations

import colorsys
import random
from dataclasses import dataclass

# Every colour key the renderer understands, in the order the settings window
# shows them. A palette always fills all of them: a partly-filled one would
# blend a preset with whatever the model already had, which is how the first
# version produced spiders with mismatched feet.
PALETTE_KEYS = ("body", "legs", "leg_band", "leg_dark", "leg_tip",
                "highlight", "eyes")


def _rgb(hue: float, saturation: float, value: float) -> list[int]:
    red, green, blue = colorsys.hsv_to_rgb(hue % 1.0, min(1.0, max(0.0, saturation)),
                                           min(1.0, max(0.0, value)))
    return [int(round(red * 255)), int(round(green * 255)), int(round(blue * 255))]


def palette_from_hue(hue: float, saturation: float = 0.55,
                     value: float = 0.78) -> dict:
    """Build a whole spider's palette from one hue.

    The proportions are read off the shipped tarantula, which is the model the
    project is built around: a very dark body, legs a little lighter, near-black
    leg tips, and a band and highlight bright enough to read at spider size.
    The eyes are shifted a little warmer and made much brighter, so they read
    as eyes against a dark body without becoming a second colour scheme.
    """
    return {
        "body": _rgb(hue, saturation, value * 0.30),
        "legs": _rgb(hue, saturation * 0.94, value * 0.44),
        "leg_band": _rgb(hue + 0.030, saturation * 0.86, value * 0.86),
        "leg_dark": _rgb(hue, saturation, value * 0.21),
        "leg_tip": _rgb(hue, saturation * 0.90, value * 0.17),
        "highlight": _rgb(hue + 0.018, saturation * 0.64, value * 0.92),
        # A small warm shift, not the opposite side of the circle. Rendered
        # and looked at: a complementary offset of 0.46 gave a jade spider
        # magenta eyes. The shipped tarantula puts its body at hue 0.03 and
        # its eyes at 0.12, which is the relationship copied here.
        "eyes": _rgb(hue + 0.090, 0.64, 0.97),
    }


@dataclass(frozen=True)
class Palette:
    id: str
    name: str
    colors: dict

    def as_overrides(self) -> dict:
        """A fresh copy, because the caller stores this on a widget."""
        return {key: list(value) for key, value in self.colors.items()}


def _named(palette_id: str, name: str, hue: float, saturation: float,
           value: float) -> Palette:
    return Palette(palette_id, name, palette_from_hue(hue, saturation, value))


# Spread around the hue circle on purpose, so the list reads as a range of
# choices rather than as eight browns. Named for the colour rather than for a
# species, because these apply to every body plan.
NAMED_PALETTES: tuple[Palette, ...] = (
    _named("cocoa", "Cocoa", 0.055, 0.62, 0.74),
    _named("copper", "Copper", 0.075, 0.78, 0.88),
    _named("ember", "Ember", 0.015, 0.74, 0.82),
    _named("amber", "Amber", 0.115, 0.76, 0.92),
    _named("moss", "Moss", 0.270, 0.52, 0.70),
    _named("jade", "Jade", 0.430, 0.55, 0.76),
    _named("frost", "Frost", 0.545, 0.44, 0.88),
    _named("ink", "Ink", 0.620, 0.58, 0.60),
    _named("plum", "Plum", 0.775, 0.54, 0.74),
    _named("rose", "Rose", 0.925, 0.50, 0.90),
    _named("ash", "Ash", 0.600, 0.08, 0.72),
    _named("bone", "Bone", 0.100, 0.16, 0.95),
)

PALETTE_BY_ID = {palette.id: palette for palette in NAMED_PALETTES}


def random_palette(rng: random.Random | None = None) -> dict:
    """A fresh palette, anywhere on the hue circle.

    Not a choice from `NAMED_PALETTES`: a colony that raises a dozen spiders
    should not produce two Cocoas and three Jades. The saturation and value
    ranges are narrow enough that every roll still looks like a spider --
    wide-open ranges gave washed-out ghosts and fluorescent toys.
    """
    rng = rng or random
    return palette_from_hue(rng.random(),
                            rng.uniform(0.34, 0.78),
                            rng.uniform(0.62, 0.94))
