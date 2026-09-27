# Mission building sprites

These transparent RGBA PNGs replace the former procedural Qt building paintings. The twelve base sprites are `home`, `food`, `silk`, `hatchery`, `nest`, `outpost`, `infestation`, `venom`, `lookout`, `amber`, `nursery`, and `flynest`. Captured-state variants are `hatchery_claimed`, `nest_claimed`, and `infestation_destroyed`.

The loader composes each source image into the existing 240×190 logical frame at 2× resolution. The soil contact point stays at the original `(120, 139)` anchor so mission overlays and map cards continue to line up.
