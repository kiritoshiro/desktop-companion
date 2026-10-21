# dc-creature port status

Simulation half of `src/desktop_bug/creature/`. One `Creature` per spider.
`creature.kin` is its `dc_kinematics::Kinematics` (pose, targets, speeds,
legs, gait clocks, roll, drift, throw inertia, catch reach). Every other
Python attribute is a plain `pub` field named without its leading underscore.

## Modules

| Python | Rust | Status |
|---|---|---|
| `core.py` `__init__` (RNG streams in draw order, dead draw at line 173 kept, gait-spell stream) | `creature.rs` | ported |
| `core.py` progression/stats/XP/equipment/damage/heal/die/teams/jobs | `progression.rs` | ported |
| `core.py` `update` (line 1285), `_apply_cage_bounds` | `update.rs` | ported |
| `core.py` drag/throw, `hit_test`, `set_size_scale`, `resize_screen`, `set_skill_enabled`, mood mode, social switch, gait style, name | `manipulation.rs` | ported |
| `core.py` labels (`label_visible`, `name_visible`, `label_style`, text, bar heights, border colour), `bounding_rect` | `labels.rs` | ported (font metrics via the `LabelMetrics` trait) |
| `core.py` `render*`, `_draw_*` | — | out of scope (dc-paint), see `RenderSnapshot` |
| `mood.py` | `mood.rs` | ported (incl. `antenna_drive_from_mood`, `build_antenna_points`) |
| `perception.py` | `perception.rs` | ported |
| `phase_scheduler.py` | `phase_scheduler.rs` | ported (the stream is passed in; seeded creatures share `rng`) |
| `arbiter.py` | `arbiter.rs` | ported (closures become `Action`; same candidate order and draws; ties keep the first) |
| `expression.py` | `expression.rs` | ported (non-drawing) |
| `behaviour/core.py` | `behaviour/core.rs` | ported |
| `behaviour/actions.py`, `jobs.py`, `phases.py`, `temperament.py`, `updates.py`, `webs.py` | `behaviour/*.rs` | ported (drift helpers reused from dc-kinematics) |
| `damage_numbers.py`, `web_net.py`, `regal.py` | same names | logic ported; drawing is dc-paint's |
| `constants.py` | `constants.rs` | ported (gait half re-exported from dc-kinematics) |
| `render_*`, `sprite_tint.py`, `armour_art.py` | — | dc-paint |

## Per-frame contract (for dc-manager)

```rust
// before: keep these fields current on the creature, as the Python manager
// wrote them: neighbors (ids, including itself), prey (PreyView, refreshed
// every frame) + hunting_prey, foe, flee_timer/flee_from/recovering/
// escaped_timer/engagement_timer, job_mode/job_target/job_alert_target/
// job_base_id/job_facing (or via dc_world::WorldCreature::set_job_intent),
// cage (CageRect), playfield (Arc<Playfield>), force_show_*, hovered,
// team_stances/team_profiles, allow_social, desktop_* fields.
let mut cmds = Vec::new();
let mut env = Env { population: &pop, webs: Some(&mut web_world), flies: Some(&mut fly_world),
                    mouse_webs: Some(&mut mouse_webs), bases: Some(&base_world), commands: &mut cmds };
// drag first, as the Python manager did: start_drag / drag_to(dt, mx, my) / release_drag
match creature.update(dt, fx, fy, screen_w, screen_h, &mut env) {   // (fx, fy) = _creature_focus
    UpdateOutcome::Ran | UpdateOutcome::Dead => {}
    UpdateOutcome::PlayerControlled => controller.update(dt, &mut creature), // ControlledSpider
}
pop.set(creature.view());              // later creatures see this one's new state
for c in cmds.drain(..) { /* every creature */ target.apply_command(&c, &pop) }
```

* `Population` holds a `CreatureView` per creature. Refresh a creature's
  entry right after its update; Python updated in sequence.
* `Command` covers the two places Python wrote to another creature from
  inside `_update_play`: `BumpMood` and `EnterPlay`. Apply them before the
  next creature updates.
* The manager reads back: `view()`, `dead`, `state`, `job_busy`, `kin.x/y`,
  `hp`, `bbox` (from `bounding_rect`), `webbed()`, `fleeing()`,
  `render_snapshot()`, and the `WorldCreature` impl.
* `bounding_rect(always_show_names, &dyn LabelMetrics)` needs the label
  font's metrics (`ApproxLabelMetrics` is a font-less estimate). `wants_mouse`
  does not depend on it: `creature_at` uses `hit_test`.

## Traits implemented

* `dc_world::WorldCreature` for `Creature` (`world_impls.rs`).
* This crate's narrow world traits for dc-world's worlds: `WebWorldApi` for
  `WebWorld`, `FlyWorldApi` for `FlyWorld` (silk shots go through a small
  `WorldCreature` stand-in), `MouseWebApi` for `MouseWebWorld`, `BaseLookup`
  for `BaseWorld`. Ids are the raw `u64` inside dc-world's id newtypes.
* `dc_adventure::adventure::ControlledSpider` and
  `dc_adventure::mission::MissionSpider` for `Creature` (dc-creature depends
  on dc-adventure, so dc-adventure must never depend on dc-creature; note
  this also pulls dc-art and dc-render into dc-creature's build).

## Painter-facing state (for dc-paint)

`Creature::render_snapshot() -> RenderSnapshot` (`snapshot.rs`) copies what
`Creature.render` reads, with the field names (and serde names) of
`dc_paint::CreatureDraw`: pose (`x … combat_body_offset`, `state`,
`spider_gait`), expression (`startled_timer … antenna_hand_targets`, `mood`),
`regal` (the live gesture, or a resting one while `_regal_blocked()`),
`palette` (colours in model order, override keys, camouflage),
`camouflage_opacity_drop`, `worn` (`progression.equipped`), web net
(`webbed`, `web_net`, `web_anchors`, `struggle`, `struggle_clock`,
`webbed_timer`), `label` (`LabelDraw` fields) and `damage_numbers`
(`[amount, age, nudge]`) with `damage_team`. Leg geometry is not copied: it
comes from `creature.kin` through the `dc_kinematics::chain` helpers.
`label_spec(always_show)` gives the resolved label (text, point size, top,
border colour, bars).

## Parity

`tools/dump_fixtures.py` builds creatures exactly as
`CreatureManager._create_creature` does (seed 11, recorded constructor
arguments) and drives the real `Creature.update` with a recorded cursor path,
drag events and manager-side writes (foe, damage, XP, silk pin, flee, cage);
`tests/parity.rs` replays them. 35 scenarios, 33,840 creature-frames: the
colony's five spiders solo, every shipped personality on the plain spider,
the colony together, a sociable cluster (play/cuddle/inspect and the
cross-creature commands), drag and throw, a fight between rivals, a cage,
and two web-world scenarios running dc-world's real `WebWorld`. Every frame's
state and phase match exactly; numbers agree to 5e-13 relative (the
fixtures' 13-digit rounding). Construction is checked field by field
(position, timers, cooldowns, mood, phase deck, legs).

The fixture script patches `WebWorld.claim_site` to draw from the web world's
own stream when the creature passes no `rng`: Python falls back to the
unseeded module-level `random` there, which no replay can match, and
dc-world made the same choice.

The manager-level oracle (seeded `CreatureManager` runs with flies, bases,
jobs, hunting and combat) is replayed by dc-manager's `tests/parity.rs`,
which drives this crate's `Creature` through the real manager loop.

Regenerate: `.venv/Scripts/python.exe -I rust/crates/dc-creature/tools/dump_fixtures.py`.

## Ported tests

`tests/parity.rs` (35 replays), `tests/bench.rs` (ignored; about 9 to 12 us
per creature per frame for 50 to 200 creatures in a crowd, release build),
and the Python test ports listed below.

TESTS_PLACEHOLDER

## Differences and things left out

* `leg.contact_state = "air"` on pickup becomes `ContactState::Swing`
  (dc-kinematics has no `Air`; the gait only tests `== "stance"`, so the
  behaviour is identical).
* An unseeded creature draws from an entropy-seeded `PyRandom` instead of
  the module-level `random`; its scheduler gets its own entropy stream, as
  Python's did.
* `advance_pose` (player control) passes no foe to `_update_posture`; a
  controlled spider has its foe cleared by the controller in Python too.
* `set_skill_enabled` takes the web world as an argument (Python reached it
  through `self.web_world`).
* `job_alert_target` is a `CreatureId` (Python held the creature).
* Not here, by design: `render*`/`_draw_*` (dc-paint), the manager's prey
  choice, combat blows, desktop-surface hiding and camouflage touch scanning
  (dc-manager).
