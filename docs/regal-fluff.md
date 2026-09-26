# Regal Fluff — Phidippus regius

Select **Jumper → Regal Fluff · Phidippus regius** in a Companion slot, then
save and launch the overlay. The model is also in the all-types skin list.
Choose the Curious personality for approach/pause behavior. No existing saved
spiders or default presets are replaced. The model uses the normal discovery
and packaging path, with no external image assets or new runtime dependencies.

## Visual and motion design

The owner's four photo references informed the broad raised cephalothorax,
large anterior median eyes, compact abdomen, pale fuzzy pedipalps, striped
forelegs, and turquoise chelicerae. Eight legs and eight eyes are retained.
The head is part of the cephalothorax: its small sideways lean is a stylised
pose, not a human neck or swivelling eyeballs. The face is deliberately
presented in an elevated, illustrative view so it reads at desktop size.

The impression of cuteness is an artistic interpretation: large glossy eyes
read as attention, short fluffy palps resemble small hands, a broad head and
compact body soften the silhouette, and pauses make tiny gestures readable.
This is not evidence of human emotions in spiders.

The model has its own shorter stride and six bounded gestures: question,
reach, sideways peek, greeting, alternating foreleg dance, and palp grooming.
Each eases in and out, with 3.5–7.5 seconds of rest between displays. Nearby
mouse positions steer the cephalothorax; visible non-hostile neighbours can
trigger social displays. Social interactions respect the existing toggle.
The six remaining legs retain the shared foot solver during foreleg displays.
Expressions yield to fast walking, combat, hunting, jumping, carrying, web
restraint, hidden windows, and player control. Rendering never advances time.
Existing camouflage, labels, web nets, rolling, and equipment accents remain.

## Biological basis and deliberate liberties

- [UF/IFAS: Regal Jumping Spider](https://edis.ifas.ufl.edu/publication/IN309)
  describes the eye arrangement, iridescent chelicerae, white palp hairs,
  fringed male forelegs, visual hunting, and species-specific courtship dance.
- [Echeverri et al., 2017](https://academic.oup.com/beheco/article/28/6/1445/4091426)
  studies signalling alignment in another jumping-spider genus, Habronattus.
  Its sidling and forelimb/palp movement evidence is family-level inspiration,
  not a claim that this exact choreography occurs in P. regius.

Friendly greeting, mouse curiosity and occasional solo dancing are companion
characterisation. Real display may be courtship or threat; it does not imply
friendship. This model mixes a male-inspired black/white colour scheme with
exaggerated fluffy palps to match the requested character.

## Review

`python tools/preview_regal.py <output-folder>` renders six poses. Optional
`--frames <scratch-folder>` writes a mouse-attention animation as PNG frames.
`python -m pytest tests/test_regal.py` checks discovery, actual selector entries,
target filtering, bounded animation, interruption, and repaint determinism.
`python tools/run_all_checks.py` runs the complete repository checks.

Offscreen inspection does not replace watching the GL desktop overlay. Check
the face at your chosen size, mouse approach, another friendly spider, carry/
drop, jump, and armour with your normal display scaling.
