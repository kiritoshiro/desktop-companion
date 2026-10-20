# Rust port of Desktop Bug Companion

A staged rewrite of the Python/PyQt5 app (`../src/desktop_bug`) in Rust, with
wgpu for the overlay and egui for the windows. The Python app stays the
reference and keeps working until the Rust build reaches parity.

## Crates

| Crate | Ports | Depends on |
|---|---|---|
| `dc-core` | CPython-exact RNG (`PyRandom`), `support/math_utils`, `Color`, `Rect` | — |
| `dc-content` | `content/*`: body plans, model JSON, personalities, presets, skills, palettes, enemy kinds/traits, discovery, preset I/O | core |
| `dc-state` | `state/*`: progression, teams, runtime state (schema v3, migration) | core, content |
| `dc-kinematics` | `creature/kinematics/*`: legs, IK, gait, stepping, movement, roll | core |
| `dc-render` | Replaces QPainter: a retained display list (`Scene`) + tiny-skia CPU backend (tests, offscreen) + wgpu GPU backend (overlay), text | core |
| `dc-platform` | Win32: layered click-through overlay window, hit-test/input transparency, hotkeys, tray, frame policy, DPI, desktop window snapshot, capture, screen activity, single-instance IPC, logging | core |
| `dc-creature` | `creature/*` simulation: mood, perception, phase scheduler, arbiter, expression, behaviour, `Creature` | core, content, state, kinematics, world |
| `dc-world` | `world/*`: playfield, screen layout, tactics, webs, flies, mouse webs, bases, cages, carcasses, aimed silk | core, content |
| `dc-manager` | `manager/*`: `CreatureManager` | creature, world, … |
| `dc-paint` | Creature and world painters (`render_procedural`, sprites, batching, tint, regal, armour art, web net, damage numbers, labels; webs, flies, mouse webs, bases, cages, carcasses; the manager's layer order) onto `dc-render`, from plain snapshot structs | render, kinematics, content |
| `dc-art` | Mission and menu art: `mission_art`, `mission_backdrop`, `map_art`, `skill_art`, `mission_ui` HUD, `desktop_surface`, `screen_text`, `tools/generate_ui_art.py` | render, content |
| `dc-adventure` | `app/` Qt-free game logic: campaign, map layouts, custom maps, controls, armoury, adventure profile, encounters, mission, swarm, reclaim | world, creature, manager |
| `dc-ui` | egui windows: settings (`config_ui`), mode menu, character/inspector/armour, controls editor, admin, map editor, wood theme | content, state, render |
| `dc-app` | The binary: settings process and `--engine` overlay process (`app/engine.py`) | everything |

## Running and packaging

- Settings window: `cargo run --release -p dc-app`; the overlay alone:
  `cargo run --release -p dc-app -- --engine [--mode adventure] [--seconds S]`.
  Set `DESKTOP_BUG_STATE_DIR` to a scratch folder for test runs; otherwise
  the app shares `%LOCALAPPDATA%\DesktopBugCompanion\state` with the Python
  app.
- `pwsh rust/package.ps1` builds a release and packs `rust/dist/DesktopBugCompanion/`
  (the exe beside `models`, `personalities`, `presets`, `assets`) and a zip.
  Release builds have no console window. The exe has no embedded icon yet
  (the windows and the tray set theirs at run time).

## Rules for everyone working in parallel

1. **Stay in your crate.** Edit only files inside the crate(s) you own. Read
   anything. Never edit the Python tree, the workspace `Cargo.toml`, or
   `Cargo.lock` by hand.
2. **Dependencies:** use `workspace = true` entries only. You may add a
   workspace crate to your own `Cargo.toml` and enable extra features on an
   existing dependency. Do not add new external crates; ask instead.
3. **Your own target dir:** build with
   `CARGO_TARGET_DIR=target-<your-crate> cargo test -p <your-crate>` so
   parallel builds don't block each other on the file lock.
4. **Faithful port first.** Keep names recognisable (`snake_case` from the
   Python, `CamelCase` types), keep every constant's value, and put the source
   path in each module's doc comment (`//! Port of creature/kinematics/gait.py`).
   Behaviour changes are not part of the port; note any you are forced into.
5. **Numbers:** simulation is `f64`. Use `dc_core::math` helpers
   (`py_round`, `py_mod`, `normalize_angle`) where Python semantics differ
   from Rust's.
6. **Randomness:** pass `&mut PyRandom` explicitly. Keep the draw order and
   the kind of draw identical to the Python (a `random.choice` is
   `rng.choice`, a `uniform` is `rng.uniform`, never substitute one for the
   other). Seeds that are strings in Python stay strings
   (`PyRandom::from_str_seed(&format!("{seed}:flies"))`).
7. **JSON:** `serde` with the exact key names used on disk. Unknown keys must
   survive a load/save round trip where Python preserves them
   (`#[serde(flatten)] extra: serde_json::Map<…>`).
8. **Tests:** port the Python tests that cover your modules (`../tests/`) as
   Rust unit or integration tests. Where a Python trace is the oracle, add a
   small generator under `tools/` that dumps JSON fixtures from the Python
   code (run it with `../.venv/Scripts/python.exe -I` or the main checkout's
   venv and `PYTHONPATH=../src`), commit the fixture, and compare.
9. **No commits.** The coordinator commits after each wave.
10. **Status:** keep `crates/<crate>/PORT_STATUS.md` listing every Python
    module you cover as ported / partial / not started, with what is missing.

`rustfmt.toml` sets the width; run `cargo fmt -p <your-crate>` only.
