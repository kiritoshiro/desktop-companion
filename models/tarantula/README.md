# Tarantula

Tarantula is a separate procedural spider model with a smaller body and long,
slender, robust legs. Each leg has five visible segments and four independently
phased interior joints. The middle patella folds during a step, the distal
hinge turns inward, and the final tarsus descends to the ground.

The `elevated_arc` and `proximal_lift` settings raise the first limb from the
body, then lower every following joint in sequence before the tarsus drops to
its planted foot. The legs are rendered as separated, widened rods with
individual joint collars and a small hair halo per segment; body fuzz is kept
separate from the leg geometry so it cannot become a continuous wing-like fill.
Elevation is visual only; the support-driven controller still solves body
movement from fixed ground contacts. The model uses a metachronal
`tarantula` gait profile with at most two airborne legs.

`segment_lengths` is separate from width scales. It gives each link an
individual maximum, with the femur/patella carrying the longest span and the
distal tarsus staying short. The renderer projects every joint back inside
that limit when a front foot reaches or turns, preventing a stretched
proximal limb.

The feelers use the same tarantula visual language: five broad tapered links,
visible joint collars, a lifted proximal link, and progressively descending
distal links. They are still called `antennae` in the shared appearance schema
for compatibility with the other models.

Use `presets/tarantula.json` to preview it. The deterministic checks are in
`tools/tarantula_smoke.py`.
