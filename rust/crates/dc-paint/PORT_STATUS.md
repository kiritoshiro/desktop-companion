# dc-paint port status

Creature and world painters on `dc_render::Canvas`. Painting is pure: no
simulation state changes and no random draws (seeded layouts such as base
mounds use their own fixed string seeds, as in Python).

Status as of 2026-10-18: **every Python drawing module below is ported**;
nothing is partial or not started. Verified against Qt renders of the same
inputs (see "Verification").

## Python modules covered

| Python | Rust | Status |
|---|---|---|
| `creature/core.py` `Creature.render` (camouflage fade, roll spin, body dispatch) | `creature.rs` | ported |
| `creature/core.py` label drawing (`_label_font`, `label_visible`, `name_visible`, `_draw_name_label`, `_draw_quiet_label`, `_draw_label_bars`, health/XP/stamina bars) | `label.rs` | ported |
| `creature/render_procedural.py` drawing half (`_render_procedural`, `_draw_leg_armour`, `_draw_equipment`, `_draw_abdomen_marking`, `_draw_leg_connections`, `_draw_antennae`, `_draw_eyes`, `_enemy_shell_path`, `_qcolor*`, `_appearance_color`, `_mix`) | `procedural.rs`, `color.rs`, `look.rs` | ported |
| `render_procedural.py` leg geometry (`_leg_draw_points`, `_sprite_leg_chain_points`, `_solve_knee`, `_safe_sprite_leg_foot`, poses) | `dc-kinematics::chain`, strung together by `resolve.rs` | ported |
| `creature/render_sprite.py` + `_load_sprite_assets`, `_draw_sprite_segment` | `sprite.rs` | ported |
| `creature/render_batch.py` (`LegBatch`) | `batch.rs` | ported (unit half of `test_leg_batching.py` ported) |
| `creature/render_detail.py` | `dc-kinematics::detail`, switch in `PaintSwitches` | ported |
| `creature/sprite_tint.py` | `sprite_tint.rs` | ported |
| `creature/regal.py` `_render_regal` | `regal.rs` | ported |
| `creature/armour_art.py` (spider pieces, anatomy doll, inventory icons) | `armour_art.rs` | ported |
| `creature/web_net.py` drawing half | `web_net.rs` | ported |
| `creature/damage_numbers.py` drawing half | `damage.rs` | ported |
| `creature/mood.py` antenna helpers | `antenna.rs` | ported |
| `world/webs.py` `Web.draw`, `WebWorld.render` | `world/webs.rs` | ported |
| `world/flies.py` render methods | `world/flies.rs` | ported |
| `world/mouse_webs.py` draw methods | `world/mouse_webs.rs` | ported |
| `world/jobs.py` `BaseWorld.render` | `world/bases.rs` | ported |
| `world/cage.py` `Cage.draw` | `world/cage.rs` | ported |
| `world/carcass.py` `Carcass.draw`, `manager/combat.py` `render_carcasses` | `world/carcass.rs`, `scene.rs` | ported |
| `manager/core.py` `render()`, `manager/surfaces.py` `_surface_visible_qregion` | `scene.rs` | ported (driven by `dc-manager::paint`) |

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

## Verification

* `tools/dump_reference.py` dumps the exact painter inputs and the Qt render
  (PyQt5, offscreen platform) of every shipped model at rest, 13 stress
  variants (armour on every slot, net, startle, hop, camouflage), two tints,
  three label styles and five world scenes: 85 cases.
  `cargo run --release -p dc-paint --example compare` renders them with
  `CpuRenderer` and writes `target-review/summary.md` and side-by-side
  sheets (Qt | Rust | diff) to the gitignored `target-review/`.
* Last run (2026-10-18):

  | family | cases | mean diff | worst | mean IoU | min IoU | min IoU (1 px) |
  |---|---|---|---|---|---|---|
  | enemy | 13 | 2.92 | 4.81 | 0.9752 | 0.9583 | 1.0000 |
  | label | 3 | 11.72 | 17.17 | 0.9514 | 0.9320 | 0.9828 |
  | procedural | 22 | 2.68 | 5.20 | 0.9715 | 0.9500 | 1.0000 |
  | regal | 4 | 4.61 | 5.50 | 0.9691 | 0.9580 | 1.0000 |
  | sprite | 29 | 1.23 | 4.78 | 0.9883 | 0.9390 | 0.9995 |
  | tarantula/chained | 9 | 3.07 | 5.37 | 0.9689 | 0.9456 | 1.0000 |
  | world | 5 | 1.51 | 2.64 | 0.9555 | 0.9211 | 0.9993 |

  `rest__tarantula`: mean diff 1.32, IoU 0.993. Every silhouette matches
  within one pixel; the sheets show no visible difference in colour, shape
  or layer order.
* Committed tests: `tests/reference.rs` (five cases against their Qt PNGs),
  `tests/switches.rs` (batching / detail levels / determinism),
  `tests/scene.rs` (layer order, clip skip, occluder clip),
  unit tests in `batch.rs`, `color.rs` and others, and
  `dc-manager`'s `tests/parity.rs`, which paints whole manager frames
  through `scene.rs` against the Python manager's render.

## Known differences (not dc-paint bugs)

* **Strokes 1 px wide or thinner** (net strands, thin legs, hairs): Qt's
  raster engine draws non-flat-capped pens of width ≤ 1 with its own line
  rasteriser, which shifts diagonal lines by up to ~0.35 px and gives them
  ~15% less coverage. `dc-render` strokes them exactly, so these lines sit up
  to a third of a pixel apart. This is the main remaining diff in the stress
  cases; matching it belongs in `dc-render`, not here.
* **Labels**: the references come from Qt's *offscreen* platform, whose
  FreeType font engine gives Segoe UI 9 pt a height of 19 px; Qt on the
  Windows platform (the real app, and what `dc-render`'s text emulates) gives
  16. The label plate is therefore ~3 px shorter and ~2 px narrower than in
  the offscreen reference; this matches the real app.
* **Faithful quirk kept**: plate abdomen armour (`armour_art.paint_abdomen`)
  calls `setClipPath` (Qt's default `ReplaceClip`) and then
  `setClipping(False)`, so it drops the occluder clip that
  `CreatureManager.render` set and draws over a covering window. Checked with
  PyQt5: Qt leaks the same pixels. `tests/scene.rs` pins this.

## Benchmark

`cargo run --release -p dc-paint --example bench 300 120`: 300 spiders (the
62 shipped models in their dumped poses, 23 of them sprite rigs) on a
1920×1080 frame, 25,141 commands.

| stage | ms / frame |
|---|---|
| record (dc-paint → `Scene`) | 4.9–5.2 |
| GPU (`Headless`, 4× MSAA, waits for the GPU; 1,115 draw calls) | 5.5 |
| record + GPU | 10.8–11.0 |
| CPU (`tiny-skia`, for reference) | ~110 |
