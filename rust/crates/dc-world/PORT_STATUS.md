# dc-world port status

Every module of `src/desktop_bug/world/` except `desktop_environment.py` is
ported (simulation only; drawing is dc-paint's). `cargo test -p dc-world`
runs 116 tests, all passing; `cargo clippy -p dc-world --all-targets` is clean.

| Python module | Rust | Status |
|---|---|---|
| `world/playfield.py` | `src/playfield.rs` | **ported**: `ScreenRect`, `Playfield` (fast path, margin, seam clamp), `rects_from_geometry` |
| `world/screen_layout.py` | `src/screen_layout.rs` | **ported**: `Screen`, `ScreenLink` (`LinkKind::Door`/`Tunnel`), `ScreenLayout` (`new`, `single`, `screen_at`, `clamp`, `links_of`, `tunnels`, `path`, `route`, `tunnel_at`) |
| `world/tactics.py` | `src/tactics.rs` | **ported**: `Member<K>`, `Foe<K>`, `Order<K>`, `Plan<K>`, `plan`, `choose_focus`, `tactics_level`; all doctrines |
| `world/webs.py` | `src/webs.rs` | **ported**: `Strand`, the four planners, `spiral_on_radii`, `radii_through`, `Web` (build, wobble, tear, mend, walkable point), `web_from_dict`, `WebWorld` (sites, adopt/repair claims, abandon, cursor tearing, dirty rects, `to_dict`/`restore`) |
| `world/flies.py` | `src/flies.rs` | **ported**: `Fly` (wander, fear, edges, monitors, webs, capture, drag, eaten), `WebShotProjectile`, `FlyRemains`, `FlySpawner`, `FlyWorld` (spawning, nests + `spawners_to_dict`/`restore_spawners`, shots at flies and spiders, remains, dirty rects) |
| `world/mouse_webs.py` | `src/mouse_webs.rs` | **ported**: `StickyHold`, `Capture::{Trap, Wall}` (`ShoveCapture`), `MouseShot`, `SpentSplat`, `MouseWebWorld` (`update` returns the desired pointer) |
| `world/jobs.py` | `src/jobs.rs` | **ported**: job table re-exported from `dc-content::jobs`, `plan_mounds`, `patch_recipe` (+ `PatchCache`), `blend`, `BaseSite` (`mounds`, `to_dict`/`from_dict`), `BaseWorld` (builder, guard line, scout blackboard, web tender, hunter, regen, food, duty cycles, monitors, move/remove) |
| `world/cage.py` | `src/cage.rs` | **ported**: `Cage`, `Corner` |
| `world/carcass.py` | `src/carcass.rs` | **ported**: `Carcass`, `carcass_for`, `eaters_near`, `is_eating`, plus the plain-data half of `creature/damage_numbers.py` a carcass needs (`DamageNumber`, `note_damage`, `update_damage_numbers`, `damage_text`, colours) |
| `world/aimed_silk.py` | `src/aimed_silk.rs` | **ported**: `AimedSilk`, living in `FlyWorld::projectiles` as `Projectile::Aimed` (`FlyWorld::launch_aimed_silk`) |
| `world/desktop_environment.py` | — | out of scope (ctypes, `dc-platform`); no world consumes a desktop-surface snapshot, so no struct is defined here |

Support modules: `creature_view.rs` (the creature interface below),
`pymath.rs` (CPython-exact `hypot`, `sum`, `round`, `**`, and Python-style
`float()`/`int()`/`str()`/truthiness over JSON for the state-file readers),
`spatial.rs` (a bucket grid; see Performance).

## Creature-facing interface (for dc-creature) — `src/creature_view.rs`

Unchanged since it was first published. Worlds take creatures as `&[C]` /
`&mut [C]` with `C: WorldCreature` and store only ids across frames (Python
held object references).

- `CreatureId(u64)`: identity (`is`); unique among live creatures, not reused
  while a world may hold it (web builder/repairer, fly hunters/trapper, shot
  shooter/target, guard alert target).
- `FlyId(u64)` / `WebId(u64)`: assigned by `FlyWorld` / `WebWorld`.
  `Creature._prey is fly` becomes `prey_fly() == Some(fly.id)`.
- `Relation { Friend, Foe, Neutral }` (`relation_to` string).
- `JobMode` (13 modes, `as_str`/`parse` give the Python strings) and
  `JobIntent<'a> { mode, target, alert_target: Option<CreatureId>, base_id: Option<&str>, facing }`.

```rust
pub trait WorldCreature {
    fn id(&self) -> CreatureId;
    fn index(&self) -> i64;
    fn progression_id(&self) -> &str;        // str(getattr(c, "progression_id", c.index))
    fn team_id(&self) -> &str;               // never empty; "neutral" for none
    fn job_id(&self) -> &str;
    fn x(&self) -> f64; fn y(&self) -> f64; fn heading(&self) -> f64; fn size(&self) -> f64;
    fn level(&self) -> f64 { 1.0 }
    fn dead(&self) -> bool { false }  fn dragging(&self) -> bool { false }
    fn airborne(&self) -> bool { false }  fn job_busy(&self) -> bool { false }
    fn player_controlled(&self) -> bool { false }  fn webbed(&self) -> bool { false }
    fn is_feeding(&self) -> bool { false }   // state == "Feed"
    fn hunting_prey(&self) -> bool { false } // _hunting_prey
    fn acts_as_hunter(&self) -> bool { false }
    fn prey_fly(&self) -> Option<FlyId> { None }
    fn desktop_fully_hidden(&self) -> bool { false }
    fn current_speed(&self) -> f64 { 0.0 }  fn aim_intent(&self) -> f64 { 0.0 }
    fn web_homing(&self) -> f64 { 0.0 }     // _progression_effect("web_homing")
    fn relation_to(&self, other: &Self) -> Relation;
    fn set_job_intent(&mut self, intent: &JobIntent<'_>); // job_mode/target/alert_target/base_id/facing
    fn set_job_base_id(&mut self, base_id: Option<&str>);
    fn heal(&mut self, amount: f64) -> f64 { 0.0 }
    fn web_pinned(&mut self, kind: &str, by: CreatureId) {}
    fn rng(&mut self) -> &mut PyRandom;     // creature's own stream
}
```

A creature has no velocity in this interface: a shot at a spider leads it by
`(0, 0)`, as in Python, where `Creature` has no `vx`/`vy`.

Call shapes the manager uses:

- `WebWorld::update(dt, cursor)`; `claim_site(id, size, prefer_corner, weights, Some(creature.rng()))`.
- `FlyWorld::update(dt, &mut spiders, Some(&mut web_world))`; `update_projectiles(dt, &mut spiders)` for the mission loop.
- `BaseWorld::update(dt, &mut creatures, Some(&mut web_world), &mut manager_rng)`.
- `MouseWebWorld::update(dt, mx, my) -> Option<(f64, f64)>` (desired pointer).
- `carcass_for(&creature, body_colour, &damage_numbers)`, `eaters_near(&carcass, &creatures)`.

Randomness: `WebWorld`, `FlyWorld` and `MouseWebWorld` own their streams
(`PyRandom::from_str_seed(&format!("{seed}:webs"))`, `:flies`,
`:mouse-webs`); `BaseWorld::update` takes the manager's `&mut PyRandom`
(Python shares `manager._rng`). Mounds and dirt patches seed from strings, as
in Python. Every draw is the same kind and order as the Python (checked by
comparing the next draw of each stream against the oracle).

## Persistence (matches `dc-state::Scene` and the state file)

`Scene { cages, webs, nests }` holds `Vec<Value>`; the payload's `bases` is a
`Vec<Value>` too. Each world reads and writes those directly:

| State-file key | Write | Read |
|---|---|---|
| `scene.webs` | `WebWorld::to_dict() -> Vec<Value>` | `WebWorld::restore(&Value)` (first 7; drops entries that no longer fit the screen; malformed entries cost the web, never the load) |
| `scene.nests` | `FlyWorld::spawners_to_dict()` | `FlyWorld::restore_spawners(&Value)` |
| `scene.cages` | `Cage::to_dict()` | `Cage::from_dict(&Value) -> Option<Cage>` |
| `bases` | `BaseWorld::to_dict()` (sorted by id, Python's rounding) | `BaseWorld::new(w, h, &saved)` / `BaseSite::from_dict` |

Python's `float()`/`int()`/`str()` coercions of hand-edited values (strings
for numbers, `null` kinds, short lists, strings indexed as sequences) are
reproduced and covered by the fixtures.

## Painter-facing state (for dc-paint)

All plain public fields or getters; nothing needs Qt.

- **Webs** — `WebWorld::webs`; per `Web`: `strands` (`points`, `kind`,
  `draw`, `temporary`; `Strand::polyline_upto_into` for the partly laid one),
  `strand_portion(i)` (how much of strand `i` shows), `is_cut(si, segi)`
  (draw the gaps), `temp_alpha()` (aux spiral fade), `wobble_offset(p)` /
  `is_wobbling()`, `hub`, `state`, `pattern`, `bbox()`; dirty rects via
  `dirty_rects()`.
- **Flies** — `FlyWorld::{flies, spawners, remains, projectiles}`.
  `Fly::{x, y, heading, length, state, wing_phase, buzz_phase,
  struggle_phase, draw_jitter, web_pegs, tether_from, web_splat, eat_t,
  dragging}`; `FlySpawner::{x, y, radius, pulse, specks}`;
  `FlyRemains::{x, y, scale, t, parts, fade()}`; `Projectile::{trail(), pos()}`.
  Python's renderer rolls a trapped fly's splat pegs on its first drawn frame
  (an rng draw inside `render`): call `FlyWorld::ensure_capture_pegs(id)` in
  draw order before painting to keep that draw. Silk colour `SHOT_SILK`.
- **Cursor silk** — `MouseWebWorld::{projectile, capture, spent}`:
  `MouseShot::{trail, pos}`, `Capture::hold()` → `StickyHold::{anchor,
  strain, wobble, wob_phase, pegs, progress()}`, `ShoveCapture::{shoving,
  trail, cur, wall}`, `SpentSplat::{at, t, spokes}`; `SILK*` colours;
  `dirty_rects()` drains, as in Python.
- **Bases** — `BaseWorld::bases`; `BaseSite::{x, y, radius(), completion(),
  alert, team_id, mounds()}`, `patch_recipe` / `PatchCache` (keys
  `"{id}:field"`, `"{id}:grit"`, `"{id}:patch:{dx:.2}:{dy:.2}"`), `blend`,
  `TEAM_TINT_*`, `PATCH_*`. Team colours come from `dc-state::teams`
  (`BaseWorld.team_profiles` is the manager's).
- **Cages** — `Cage::{x, y, w, h, corner_radius(), border_rects(),
  footprint_rect(m), outer_bbox()}`, `HANDLE`, `BORDER`.
- **Carcasses** — `Carcass::{x, y, size, body_color, legs, spent(),
  damage_numbers, damage_top(), damage_color()}`, `damage_text`.

## Tests

- `tools/dump_world_fixtures.py` writes `tests/fixtures/{webs,flies,
  mouse_webs,jobs,cage,carcass}.json` from the Python modules (run with the
  venv's `python.exe -I`; see its docstring). Creature inputs are either in
  the fixture or produced by a small script both sides run (a triangle wave
  using Python `%`, or "walk 150 px/s toward the job target").
  - `webs_parity.rs`: 40 planner outputs; site claims with weights over 3
    seeds; a 2,200-frame build / cursor tear / mend trace of 3 webs;
    `restore` of malformed entries and `set_screen`.
  - `flies_parity.rs`: a 4-spider, 900-frame flock and a 40-spider, 600-frame
    flock on two monitors (grid path) with webs, shots at flies and spiders,
    aimed silk, eating, dragging, `configure`, `clear_hunters`; nests,
    remains, fresh flies.
  - `mouse_webs_parity.rs`: 3 × 1,500 frames (trap, wall shove, miss,
    tracking; with and without pointer control; disable mid-capture).
  - `jobs_parity.rs`: an 11-creature, 2,000-frame colony (every job, two
    teams and lone spiders, a torn web to mend, food, damage, a moved base),
    every intent every frame, bases and shifts every second; mounds, patches,
    tints, `BaseSite.from_dict` on bad input.
  - `cage_carcass_parity.rs`: 150 cage cases, cage `from_dict`, 4 carcasses.
  - Tolerance 1e-9 relative. Checked at zero tolerance too: every value
    matches bit for bit except a torn web's `damaged_centroid`, a sum taken
    in set order (1 ulp; see deviation 1).
- Ported Python tests: `ported_jobs.rs` (`test_jobs.py`, the `BaseWorld` half
  of `test_jobs_economy.py`, `test_bases_on_screen.py`,
  `test_base_mounds.py`, `test_base_ground.py` patch/move parts,
  `test_trapping_is_reachable.py` job parts), `ported_worlds.rs` (world parts
  of `test_world_determinism.py`, the fly glob of `test_straight_shot.py`,
  the remains part of `test_damage_numbers.py`, cage/web persistence rules of
  `test_scene_persistence.py`), unit tests in each module
  (`test_web_geometry.py`, cursor-glob parts of `test_straight_shot.py`,
  `test_flies_on_screen.py`, carcass parts of `test_conflict.py`), and the
  earlier `python_parity.rs` (tactics, screen layout, playfield).
- Not ported: tests that need a `CreatureManager`, Qt rendering or the
  Python source text (e.g. "the renderer no longer reads `patrol_angle`").

## Deviations (forced by ownership / ids; none change seeded behaviour)

1. `Web.cut` is a `BTreeSet`. When two torn segments are exactly as close to
   the mender, Python mends them in set-hash order and Rust in index order;
   the number mended per frame (and so `is_damaged`) is identical. The
   sums in `damaged_centroid` / `intact_fraction` also run in that order
   (last-bit differences). The fixtures compare cut counts while mending.
2. `WebWorld::claim_site(.., caller_rng: None)` uses the world's stream;
   Python used the module `random`. Every Python caller passes a stream.
3. A shot whose target is no longer in the slice fizzles, and `AimedSilk`
   whose shooter is gone hits no spider (Python kept the removed object).
4. A fly stuck on a web, updated without a web world, cannot pluck it.
5. `FlyWorld::playfield` / `BaseWorld::playfield` are owned copies (Python
   shared one object); the manager must assign a new one when monitors change.

## Performance

Fear (every fly against every spider) and guard intruder scans use
`SpatialGrid` from 24 creatures up; candidates are visited in list order, so
results are bit-identical to the full scan (the 40-spider fixture runs that
path). Buffers (grid, dirty rects, guard posts, projectile lists) are reused
across frames. Remaining small per-frame allocations: `BaseWorld` clones the
base id / worker key strings per worker per frame.
