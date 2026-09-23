"""Stop drawing leg detail that cannot be seen (DC-73).

DC-71 measured where a frame goes and the answer was awkward: batching 8.5x
fewer draw calls bought 5%, covering 16x fewer pixels saved 25%, and about
60% of render is Qt turning each stroked path into an outline polygon on one
CPU core. **That cost is per primitive. It does not care how big the
primitive is** -- which is what makes drawing an invisible one so expensive.

A tarantula's ``base_size`` is 27px, and at that size one leg still draws
five hair over-strokes, five segments, four joint nodes, four joint cores and
a toe: 19 primitives, 152 for the spider. Measured on twenty of them at their
real size, paired inside one process:

    everything   39.40 ms
    no joints    35.28 ms   +10.5%
    no hair      32.75 ms   +16.9%
    neither      28.60 ms   +27.4%

Bigger than the 24% a GPU framebuffer gave for the same scene, and without
the window surgery.

**The joint nodes were never visible.** Not "too small at desktop size" --
never. They are filled circles drawn on the leg's own centreline, and their
radius is smaller than the half-width of the stroke they sit on at every
size the model reaches:

    spider size   patella node radius   thinner adjacent half-width
        22.6            1.66                   1.48   protrudes 0.18px
        37.6            2.14                   2.46   inside the leg
        55.2            3.15                   3.62   inside the leg

The other three joints are inside the leg at every size. So the nodes are not
joints, they are a faint interior tint -- 2.6% of pixels, indistinguishable
from the full render at 6x magnification. They are drawn when they actually
protrude, and otherwise skipped.

**The hair could not simply go.** It is a wider stroke drawn *under* the
segment, so removing it takes weight out of the silhouette and the legs read
as thinner -- 8.8% of pixels, and visible. Below the size where its fringe
reads as a fringe it is merged instead: one stroke at the hair's width, in
the segment's colour warmed toward the hair's. Same weight, half the strokes.

Every threshold here is in pixels of drawn result, and each model is measured
against them on its own. A size threshold would have had to be retuned for
each of the 49 models; this does not.
"""

from __future__ import annotations

from typing import NamedTuple

# How far a joint node has to stand proud of the leg it sits on before it
# reads as a joint rather than as a stain inside the stroke.
JOINT_PROTRUDE_MIN_PX = 0.50

# How far the hair over-stroke sticks out past its segment before it reads as
# a fringe. Below this it is only making the line slightly wider, which one
# stroke can do on its own.
HAIR_FRINGE_MIN_PX = 0.75

# When the hair is merged away the segment takes its full width, so the leg
# keeps its weight, and a little of its colour, so it keeps its warmth.
MERGED_HAIR_TINT = 0.30

# Off restores the previous drawing exactly: every hair stroke separate, every
# joint node drawn. Kept because the only measurement this machine gives
# reproducibly is a paired one taken inside a single process -- across two
# process launches the noise is larger than the effect -- and because it makes
# "the detail levels changed nothing else" a testable claim.
ENABLED = True


class LegDetail(NamedTuple):
    """What is worth drawing on one spider's legs."""

    separate_hair: bool   # the hair gets its own stroke under each segment
    merge_hair: bool      # or the segment stroke widens and warms instead

    @property
    def strokes_per_segment(self) -> int:
        return 2 if self.separate_hair else 1


def hair_detail(base_width: float, chain_config) -> LegDetail:
    """Decide once per spider, from the width its legs are about to be drawn at.

    ``base_width`` is the renderer's own resting leg width, passed in rather
    than recomputed, so the two cannot drift apart. It deliberately excludes
    the lift and startle terms, which vary frame to frame -- a level that
    flickered while a spider walked would be worse than no level at all.
    """
    hairy = bool(chain_config and chain_config.get("hairy")
                 and float(chain_config.get("hair_scale", 0.0)) > 0.0)
    if not hairy:
        return LegDetail(separate_hair=False, merge_hair=False)
    if not ENABLED:
        return LegDetail(separate_hair=True, merge_hair=False)
    hair_scale = float(chain_config["hair_scale"])
    # The hair pen is `width * (1.12 + hair_scale * 0.55)`; half the excess
    # over the segment is what actually shows past it.
    fringe = base_width * (0.12 + hair_scale * 0.55) * 0.5
    if fringe >= HAIR_FRINGE_MIN_PX:
        return LegDetail(separate_hair=True, merge_hair=False)
    return LegDetail(separate_hair=False, merge_hair=True)


def hair_pen_width(width: float, hair_scale: float) -> float:
    """The width the hair stroke would have had -- and, when merged, the width
    the segment takes over so the leg keeps its silhouette."""
    return width * (1.12 + hair_scale * 0.55)


def joint_node_shows(radius: float, *adjacent_widths: float) -> bool:
    """Would this node stand proud of the leg, or is it inside the stroke?

    Compared against the thinner neighbour, because a node only has to
    protrude somewhere to be seen.
    """
    if not ENABLED:
        return True
    widths = [w for w in adjacent_widths if w > 0.0]
    if not widths:
        return True
    return radius >= min(widths) * 0.5 + JOINT_PROTRUDE_MIN_PX
