# dc-paint port status

Creature and world painters on `dc_render::Canvas`. Painting is pure: no
simulation state changes and no random draws (seeded layouts such as base
mounds use their own fixed string seeds, as in Python).

_Draft — being completed; see the final version below once verification is in._

## Input structs (filled by dc-creature / dc-world / dc-manager)

| Struct | Python source | Notes |
|---|---|---|
| `CreatureDraw` | `Creature` attributes read by `render` | pose, expression, `mood`, `palette`, `worn`, `legs`, web net, `label`, `damage_numbers` |
| `LegDraw` | the per-leg locals of the leg loops | produced by `resolve::resolve_legs` from `dc_kinematics::Kinematics` |
| `Palette` | `colors`, `color_overrides`, `_camouflage_color`, `_camouflage_strength`, `personality.camouflage_color_blend` | |
| `CreatureLook` | `model["appearance"]`, `render_mode`, `_folder` | parsed once per model (`CreatureLook::from_model_json`) |
| `WebDraw` / `StrandDraw` | `Web`, `Strand` | |
| `FlyWorldDraw` (`FlyDraw`, `FlySpawnerDraw`, `FlyRemainsDraw`, `WebShotDraw`) | `FlyWorld` | |
| `MouseWebDraw` (`SpentSplatDraw`, `CaptureDraw`, `StickyHoldDraw`, `MouseShotDraw`) | `MouseWebWorld` | |
| `BaseDraw` | `BaseSite` + `team_color` | |
| `CageDraw` | `Cage` + `member_count`, `active` | |
| `CarcassDraw` | `Carcass` | |
| `SceneDraw` / `CreatureEntry` | `CreatureManager.render` inputs | |
