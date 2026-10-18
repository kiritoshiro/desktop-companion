# dc-manager port status

`CreatureManager` from `src/desktop_bug/manager/`. Ported by a Codex worker
(2026-10-18), compile-fixed and verified by the coordinator.

| Python module | Rust | Status |
|---|---|---|
| `manager/core.py` | `src/core.rs` | **ported**: construction (`new`, `empty`), `create_creature`, neighbour links, render order, screen rects/resize, `creature_at`, `wants_mouse`, `update` / `update_with_controller`, camouflage touch |
| `manager/core.py` (presets, randomise) | `src/roster.rs` | **ported**: `load_preset`, `load_preset_data`, `reload_from_preset_data`, `replace_with_traits`, `randomize_*`, `session_snapshot` |
| `manager/core.py` (settings) | `src/naming.rs` (labels, teams, behaviour), `src/prey.rs` (flies) | **ported**: the `set_*` setters the settings window uses |
| `manager/core.py::render` | `src/render.rs` | **ported as data**: `draw_output()` returns `FrameDraw` (Python's layer order, per-creature `RenderSnapshot`, occlusion regions); `dc-paint` paints it |
| `manager/colony.py` | `src/colony.rs` | **ported** |
| `manager/prey.py`, `hunting.py` | `src/prey.rs`, `src/hunting.rs` | **ported** |
| `manager/combat.py` | `src/combat.rs` | **ported** (`render_carcasses` is `dc-paint`'s) |
| `manager/cages.py` | `src/cages.rs` | **ported** |
| `manager/naming.py` | `src/naming.rs` | **ported** |
| `manager/persistence.py` | `src/persistence.rs` | **ported**; writes `creatures.json` in Python's layout |
| `manager/surfaces.py` | `src/surfaces.rs` | **ported** (`_surface_visible_qregion` → `dc_paint::scene::surface_visible_region`) |
| `manager/constants.py` | `src/constants.rs` | **ported** |

## API for dc-app

- `CreatureManager::new(root, preset_path, (w, h), seed, state_dir, can_control_cursor, &mut rng)`:
  discovers models/personalities under `root`, restores `state_dir/creatures.json`,
  loads the preset. `rng` is the manager's stream (Python's `self._rng`): pass
  `PyRandom::from_int(seed)` when seeded, entropy otherwise, and keep passing
  the same one to `update` and every roster call.
- Per frame: `update(dt, mx, my, MouseInput { down, pressed, released }, &mut rng)`
  returns the cursor position the mouse webs want (`desired_cursor`).
  Player-controlled spiders: `update_with_controller(.., |dt, creature| ..)` runs
  the controller at that creature's turn, as Python did.
- Hover/naming: `update_hover(x, y)`, `clear_hover()`, `name_creature`, pins.
- OS inputs are explicit: `set_screen_rects`, `resize`, `set_desktop_surfaces`
  (window/folder snapshot), `set_label_metrics` (real font metrics; the default
  `ApproxLabelMetrics` estimate changes label width, which changes the bounding
  box that decides hiding behind windows).
- Drawing: `draw_output()` → `FrameDraw`; dirty rects via `carcass_dirty_rects`,
  `cage_dirty_rects`.
- Saving: `mark_runtime_state_dirty` + `flush_runtime_state(dt)` (debounced, as in
  Python) or `save_runtime_state()`.

## Changes forced by the port

- The state folder and content root are explicit arguments.
- The manager RNG is passed in (`&mut PyRandom`), not owned.
- `DesktopSurface` keys are a JSON string of Python's rounded tuple; lookups keep
  the first (frontmost) match, as `_surface_by_key.setdefault` did.
- Label text width comes from `LabelMetrics` instead of `QFontMetrics`.
- Mission hand-off (`save_runtime_state` delegating to `self.mission`) belongs to
  dc-app, which owns both the manager and the mission.

## Tests

`tests/parity.rs` replays 21 runs of the unmodified Python manager
(`tools/dump_fixtures.py`, seeds 3/7/11): colony, fight, hunt, cages
(drag, naming, pins, hover), stalk (fly pursuit), duel (to the death, with a
carcass; 60 s), and surfaces (explorers hiding in windows and folders; 60 s).
Every recorded frame matches to 1e-6: positions, headings, states, HP, XP,
foes, prey, cages, bases, flies, carcasses, hiding flags, label text; the
manager RNG stream; and the saved `creatures.json`. The fixtures carry the
oracle's Qt label metrics (`OracleMetrics`), because offscreen Qt measures
labels much wider than the estimate.

Regenerate fixtures with the main checkout's venv:

    $env:PYTHONPATH="<worktree>\src"; $env:QT_QPA_PLATFORM="offscreen"
    ..\projects\desktop-companion\.venv\Scripts\python.exe -I tools\dump_fixtures.py

## Not yet done

- The Python unit tests that use `CreatureManager` (about 48 files under
  `tests/`, e.g. `test_conflict.py`, `test_combat_nerve.py`,
  `test_colony_growth.py`, `test_scene_persistence.py`, `test_reset_progress.py`,
  `test_team_relations.py`, `test_two_screens.py`) are not ported one by one; the
  parity replays cover the same loop but not every edge case those tests pin.
- Not covered by the replays: dragging spiders/flies/spawners/bases with the
  mouse, multi-monitor playfields, mouse-web cursor capture, desktop icons.
