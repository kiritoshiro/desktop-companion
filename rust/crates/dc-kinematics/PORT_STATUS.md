# dc-kinematics port status

Port of `src/desktop_bug/creature/kinematics/` plus the kinematic pieces the
package calls into elsewhere. Parity is checked against frame-by-frame Python
traces (`tests/parity.rs`, fixtures from `tools/dump_fixtures.py`): every
snapshot field, every frame, within 1e-9 relative. The observed deviation is
only the fixtures' 13-significant-digit rounding (max 5e-10 on pixel values);
the chain solve and `math.hypot` / `sum` are bit-exact.

## Modules

| Python | Rust | Status |
|---|---|---|
| `kinematics/legstate.py` | `legstate.rs` (`LegState`, `ContactState`, `JointBends`) | ported |
| `kinematics/core.py` | `core.rs` (`Kinematics`, `FrameCtx`, lively gait, feelers) | ported |
| `kinematics/geometry.py` | `geometry.rs` | ported |
| `kinematics/legs.py` | `legs.rs` | ported |
| `kinematics/gait.py` | `gait.rs` | ported |
| `kinematics/movement.py` | `movement.rs` | ported |
| `kinematics/roll.py` | `roll.rs` | ported (see side effects below) |
| `kinematics/stepping.py` | `stepping.rs` | ported |
| `kinematics/spider_gait.py` | `spider_gait.rs` | ported |
| `kinematics/__init__.py` | `lib.rs` | ported |
| `creature/constants.py` (gait part) | `constants.rs` | ported: `smootherstep`, `GAIT_BY_STATE`/`gait_by_state`, `GAIT_SPELL_GAP`, `GAIT_SPELL_LENGTH`, `VALID_GAIT_STYLES`, `GAIT_LABELS`, `normalize_gait_style` |
| `behaviour/temperament.py` drift helpers | `drift.rs` | ported: `_drift_state_multiplier`, `_is_drift_sliding`, `_drift_amount`, `_prime_drift`, `_guard_drifter_travel_direction` |
| `render_procedural.py` leg geometry | `chain.rs`, `config.rs` | ported: `_sprite_leg_chain_config`, `_update_spider_joint_articulation`, `_sprite_leg_chain_points`, `_solve_knee`, `_safe_sprite_leg_foot`, `_leg_attach_drawn`, `_leg_front_factor`, `_leg_lift_px`, `_leg_draw_points`, `_picked_up_leg_pose`, `_front_leg_feeler_pose`, `_combat_leg_pose` (changed, see below) |
| `render_detail.py` | `detail.rs` | ported (`ENABLED` is an argument) |
| `support/math_utils` CPython semantics | `pymath.rs` | `hypot` (CPython `vector_norm`), `py_sum` (Neumaier), `py_pow`, `wrap_pi`, JSON `float()`/`int()`/`bool()` |

## For `dc-creature`

* Re-export `dc_kinematics::constants::*` instead of redefining the gait constants.
* Own one `Kinematics` per creature. It owns the pose (`x`, `y`, `heading`,
  `target_*`, `speed`, `current_speed`, `vel_*`, `turn_rate`, `size`,
  `motion_paused`), body animation (`bob_phase`, `breath_phase`, `body_bob`,
  `body_sway`, `abdomen_pulse`, `ceph_pulse`), throw inertia, roll fields,
  `catch_point`/`catch_blend`/`inspect_intent`, drift state (`kin.drift`,
  including `wiggle_burst`), every gait clock and the legs. Behaviour code
  writes those fields directly, as the Python did on `self`.
* Construction order (`creature/core.py`): `Kinematics::new(rig, gait_style,
  PyRandom::from_str_seed(&format!("{progression_id}:gait-spell")), x, y,
  heading, size)`; then `seed_legs(rng)` at line 321, the creature's own
  `active_gait_index = rng.randrange(..)`, `seed_gait_clocks(rng)` (lines
  344-369); set `turn_rate`, `bob_phase`, `breath_phase`, `drift.*` where the
  creature draws them; finally `initialize_legs(&ctx, rng)`.
* `Rig::from_model_json(model, personality, CURVED_LEGS)` takes the resolved
  model (`legs`, `appearance`) and personality JSON; no `dc-content` types.
* Each frame build a `FrameCtx` (state via `MotionState::from_name`, fleeing,
  observer/drifter flags, reverse_walk, job facing, airborne, dragging,
  webbed_held, startled_timer, drift_run phase, held_release_timer, focus,
  held-drag inputs, screen size, margin, optional split playfield as any
  `Fn(x, y, margin) -> (x, y)`), then call `locomotion_frame(dt, &ctx, rng)`
  (aim, grounded solve or `move_body`, roll reconciliation, `update_legs`).
  Dragging calls `update_held_leg_springs` + `update_legs`; airborne frames
  call only `update_legs`; `update_gait_spell(dt)` is the expression tick.
* Side effects reported, not performed: `finish_roll`/`reconcile_roll`
  (true) mean `squash = 1.0`; `update_roll` returning true also means
  `enter_idle()`.
* `apply_json_overrides` / `Serialize` give a full kinematic snapshot (field
  names = Python attribute names without the leading underscore).

## Python quirks kept on purpose

* `carry_bounce`, `carry_spring`, `carry_joint_wave` are read from the
  sanitised chain config, which never contains them, so their defaults always
  apply. Preserved; noted on `LegChainConfig`.
* `LegDef` keeps key *presence*: `_solve_knee` defaults `upper_len` to 0.8
  where everything else uses 0.85; sectors default `rest_angle` to 90 where
  the ideal foot uses 0.

## Not ported / differences

* `_sprite_leg_chain_points`' per-frame memo cache: the solve is pure, the
  painter may cache. Fixed arrays replace lists (max 16 legs, 5 segments).
* `SpiderGaitConfig.profile` is an enum (`GaitProfile`) so the config is
  `Copy` on the hot path; unknown names map to `Other` (behaviour unchanged).
* `_combat_leg_pose` (squared up at a foe, not the bite): Python also lifted
  the front feet `0.52 * size` straight up the screen. Seen from above, that
  pulled them back behind the head of a spider walking down the screen at its
  foe, and stretched them far ahead of one walking up. Here the raise is a
  reach towards the foe (`0.46` instead of `0.30`, same `0.34` spread),
  and it eases by up to 65% while the spider walks (`current_speed / 120`).
* A non-numeric value in a leg definition or personality falls back to the
  default instead of raising.
* Tests not ported because they test something else: antenna/palp posture in
  `test_tarantula.py` (expression), model-shape asserts on JSON
  (`dc-content`), golden-image renders (`dc-paint`), the controls editor and
  settings round trip in `test_turn_movement.py` (`dc-ui`/`dc-adventure`).
  The turn-and-walk movement is ported as a kinematic driver of
  `PlayerController._update_turn_movement`.

## Tests

`cargo test -p dc-kinematics`: 51 tests (11 trace replays, 25 locomotion
ports, 10 geometry ports, 5 unit). Benchmark: `cargo test --release -p
dc-kinematics --test bench -- --ignored --nocapture` (about 6 us per spider
per frame for the grounded tarantula, about 19 us with eight chain solves;
the Python reference takes about 256 us for the same frame).

Regenerate fixtures: `.venv/Scripts/python.exe -I rust/crates/dc-kinematics/tools/dump_fixtures.py`.
