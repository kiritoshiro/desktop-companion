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

The front feelers are compact pedipalps rather than extra walking legs: five
visible tapered links (a short basal connector followed by the femur, patella,
tibia, and tarsus), painted joint collars, a raised proximal link, and a
downward fold through the knee toward a small pointed sensory claw. Their width
profile follows the walking legs but tapers more sharply toward the distal
tarsus, so they read as short legs rather than antlers or rounded antennae. They are
slightly narrower than the walking legs so the hands remain visually
subordinate to the main eight-leg silhouette. They are still called
`antennae` in the shared appearance schema for compatibility with the other
models, but the configured `sensory_hand` controller treats them as
pedipalpal-style hands: each side has its own bounded working target, grip
value, and joint trajectory rather than following the eight-leg gait. In a
passive idle or wander state they settle near the compact rest pose; the cursor
is only used as a target during an explicit inspect, aim, or catch interaction.

The hands are not frozen in that rest pose. A slow shared rub cycle periodically
draws both palps inward and outward, with a delayed flex wave through the
knuckles. Observe, inspect, aim, and catch states increase the stroke into a
small deliberate touch/probe; releasing the intent returns them to rest. The
stroke is bounded by the same short lateral and forward envelope as the hand
controller, so it cannot become cursor-chasing or turn into a long reaching
leg.

The target command is applied at the knuckles, not to the finished chain. The
proximal joint leads, the middle joint has the largest flexion authority, and
the distal joints have smaller limits so the final segment remains a curled
sensory claw. The free-link proportions follow the published tarantula
pedipalp measurements (approximately femur 0.34, patella 0.21, tibia 0.24,
tarsus 0.21 of the free palp length), while the angle limits deliberately
decrease toward the claw. This also prevents the draw pass from replacing the
articulated pose with a single mouse-facing line. The behavior is based on the
anatomical role of spider pedipalps as shortened, leg-like sensory and
prey-manipulation appendages; they are not insect antennae and are not used to
propel the body.

Each walking leg also has a painted coxa/trochanter bridge at the lateral
cephalothorax. The bridge and socket are drawn over the shell after the
under-body leg pass, so the roots remain visible without placing the entire
leg on top of the body or making a moving leg glow. The five visible links are
thickest through the coxa and femur, taper toward the tarsus, and alternate
dark brown and warm orange-brown segment colors; the final link and foot stay
dark like the reference's claw tips.

When carried, the eight legs use screen-down gravity as a broad lower-side
field: each leg preserves its own lateral lane, its knuckles sag slightly, and
fast hand motion adds a bounded counter-lag and a soft settling bounce. The
endpoints compact toward a relaxed carry pose instead of keeping the full
walking spread, while the knuckle chain remains visibly folded. They do not
aim at a shared point under the abdomen, so the relaxed legs remain visible
around the body.

The grounded gait uses a longer stance stroke and a wider turn envelope. A
supporting leg is allowed to pull or push the body through most of its usable
stroke before it swings, while a support near its rotational limit is released
early. Three legs may swing while five keep support, which keeps the tarantula
stable without forcing a sequence of tiny corrective taps.

The coxa roots sit close to the carapace perimeter, partly embedded in the
fuller flat prosoma rather than suspended outside it. The socket overlay follows
the first articulated link outward, so the leg visibly leaves the carapace
instead of looking like a tube pointed inward under the body. The larger
prosoma is deliberately broad enough to read as a complete leg-bearing shield,
while the pedicel remains the narrow waist to the abdomen.

The body is drawn as three readable masses: a large flexible abdomen, a narrow
pedicel connector, and a smaller flattened prosoma/carapace with a small front
head lobe. Anatomically, the last two are parts of the fused cephalothorax,
not a separate insect-like thorax; the visual split makes it clear that all
eight walking legs originate from the carapace rather than the abdomen.

The `stance_deadband` is intentional: the nominal spread is a recovery target,
not a pose that every foot must revisit after a small move or turn. A planted
foot can remain in its current comfortable lane until its stroke, reach, or
turn limit calls for a real step.

The default stance is arranged in four fore-aft lanes rather than eight equal
side rays: leg pair I points forward, pair II angles forward-outward, pair III
angles rearward-outward, and pair IV points backward. The side offsets are
kept smaller on the front and hind pairs so the silhouette follows the
tarantula reference pose while the middle pairs still preserve clearance.

Use `presets/tarantula.json` to preview it. The deterministic checks are in
`tools/tarantula_smoke.py`.
