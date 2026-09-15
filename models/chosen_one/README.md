# Chosen One

Chosen One is a procedural eight-legged spider built independently from the
Snowpuff models. It uses a low indigo/gold body, eight narrow support lanes, and
five visible leg segments per side: coxa, femur/trochanter, patella/knuckle,
tibia/metatarsus, and distal tarsus/toe.

## Joint design

The `leg_chain` appearance block is shared by the procedural and sprite
renderers. Its five segment scales are `[0.86, 1.0, 0.72, 0.66, 0.48]`.
Each of the four interior joints has its own phase offset, smoothed flex value,
and bend direction. The middle patella folds most during swing; the
tibia/metatarsus reverses slightly toward the body before the toe extends.
The bend profile `[0.72, 1.0, 0.68, 0.42]` keeps the first joint from making a
crab-like sideways elbow while retaining a visible folded knee and fine distal
toe.
The renderer derives separate per-link length limits from the chain's
`segment_lengths` weights, so segment width cannot accidentally elongate a
front limb.

## Movement design

Chosen One uses `spider_gait.profile = "chosen_one"`. Each leg has an explicit
phase offset, producing a front-to-rear metachronal wave instead of releasing a
whole four-leg block at once. At most two legs swing together; the other six
remain available to support the body. The grounded controller advances in fixed
120 Hz mechanics ticks, keeps planted feet in world space, and solves body pose
from the supporting contacts.

The companion preset is `presets/chosen-one.json`. The deterministic acceptance
checks are in `tools/chosen_one_smoke.py`.
