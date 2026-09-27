# Regal Scout — corrected face orientation

Select **Jumper → Regal Scout · corrected face**. Regal Fluff remains a
separate selectable model with its original geometry and appearance.

The original face was an upright portrait painted on a body whose forward
direction was the portrait's top. That put the palps toward the abdomen and
the small eyes ahead of the large front lenses. An upright preview concealed
the reversed anatomy; it became obvious when the whole spider turned.

Scout reverses the complete face along the body's forward/back axis. The
crown and small eyes are toward the abdomen, the large eyes form the front
eye line, and the chelicerae and fluffy pedipalps project forward, away from
the abdomen. Facing down the page, small eyes are above the large eyes and
palps are below. This ordering rotates with the body, rather than remaining
fixed to the screen. Palp sockets and grooming animation use the same face
transform, so moving just the eyes cannot leave the appendages behind.

The fluffy ivory ovals beside the turquoise mouthparts are pedipalps. The
flat white spots on the rear oval are abdomen markings, not eyes or palps.

Scout shares Fluff's compact eight-leg skeleton, curiosity controller and
six gestures. Its skin thumbnail faces down to show the corrected anatomy
upright. No original model data, saved spider, or default preset is changed.

Reproduce the corrected pose sheet with:

```text
python tools/preview_regal.py <output-folder> --model regal_scout
```

`tests/test_regal_scout.py` inspects actual painted eye/palp positions at four
headings, in both resting and expressive poses, and checks that both models
remain selectable. The original renderer was also compared against its
pre-change version at four headings: the old model was pixel-identical.

The preview remains a stylised view of the spider. Owner review on the live
desktop is still necessary to judge the pose and face at the chosen scale.
