# dc-adventure port status

| Python module | Rust | Status |
|---|---|---|
| `app/campaign.py` | `src/campaign.rs` | **ported**: `MAPS`, `MapInfo`, `COMPANIONS`, `SELL_PRICES`, `find_map`, `chosen_map`, `map_unlocked`, `enemy_loadout`, `roll_drop`, `tier_rank` |
| `app/map_layouts.py` | `src/map_layouts.rs` | **ported**: `BuildingKind`, `Placement`, `Guard` (role or spec), `LAYOUTS`, `EFFECTS`, `layout_for` |
| `app/custom_maps.py` | `src/custom_maps.rs` | **ported**: `SpiderSpec`, `CustomMap`, `clean_spec`, `clean_map`, `new_map`, `problems`, `map_info`, `placements`, `list_maps`/`load_map`/`save_map`/`delete_map` |
| `app/controls.py` (+ `virtual_key_for` from `engine.py`) | `src/controls.rs` | **ported**: `Action`, `Key`, `MouseButton`, `Input`, `ControlSettings` (`from_dict` incl. v2 migration, `to_dict`, `rebind`, `action_text`, `half_cone`), `DEFAULT_MOVEMENT = Turn`, `SECOND_PLAYER_BINDINGS`, `second_player_controls`, `load/save_controls`, `clamp_to_cone`, `instructions`, `virtual_key_for` |
| `app/armoury.py` | `src/armoury.rs` | **ported** |
| `app/adventure_profile.py` | `src/adventure_profile.rs` | **ported**: `AdventureProfile` (v4), load + v1/v2 migration, atomic save, `HERO`, `PLAYER_TWO`, `*_progression`, `store_progression`, `unlock_companion`, `record_result` |
| `app/encounters.py` | `src/encounters.rs` | **ported**: `EncounterProfile`, `PROFILES`, `EncounterDirector` (`outposts`, `defenders_for`, `update`, `retry`), `OutpostSite` trait |
| `app/stat_text.py` | `src/stat_text.rs` | **ported** (byte-identical HTML for every skill and piece/level) |
| `app/adventure.py` | `src/adventure.rs` | **ported** over the `ControlledSpider` trait (see below) |
| `app/mission_factory.py` | `src/mission_factory.rs` | **partial**: `build_layout` and `plan_mission`; building the mission waits for wave 2 |
| `app/admin_ui.py` (data functions) | `src/admin.rs` | **ported**: `set_admin`, `reset_progress`, `give_amber`, `give_all_armour`, `unlock_all_companions`, `set_hero_level`; the dialog is dc-ui's |
| `app/mission.py`, `swarm.py`, `reclaim.py` | — | not started (wave 2) |

`src/progression.rs` re-exports `dc_state::progression` and
`dc_content::enemy_kinds` names used here; `src/pyjson.rs` holds Python
value semantics (`int()`, `float()`, truthiness, clamping, `json.dumps(indent=2)`
with `ensure_ascii`).

## Design changes forced by the port

- **State folder is explicit.** `find_map`, `chosen_map`, `map_unlocked`,
  `load_map`, `save_map`, ... take `state_dir: &Path`; profile/controls
  functions take the file path (`profile_path(state_dir)`,
  `controls_path(state_dir)`). Callers get it from
  `dc_content::discovery::state_dir()`.
- **Profile is typed.** Python's dict became `AdventureProfile` with
  `Dict<V> = Vec<(String, V)>` for ordered maps. Unknown keys are dropped, as
  Python's loader drops them.
- **Qt keys** became `controls::Key`; `Key::name()` matches
  `QKeySequence.toString()` (probed: unknown names read as `Key::Unknown`,
  written "Key 33554431"; only "" is rejected, as in Qt).
- **`PlayerController` does not own the creature.** Every method takes
  `&mut impl ControlledSpider`; `bite` returns `Bite::Hit(index)` and the caller
  runs `_trade_blow`; `shoot` returns `SilkShot { angle, reach }` and the caller
  builds `AimedSilk`. `web_range` is a field (the mission's Lookout bonus set
  `control.WEB_RANGE` per instance).
- `store_progression` for an unknown companion stores it under its id
  (Python raised `KeyError`). `EncounterDirector` drops the unused `rng`.
- Output JSON is written with Python's layout; float text can differ in rare
  cases (`1e-07` vs `1e-7`), which both readers accept.

## Interfaces wave 2 (`mission.py`, `swarm.py`, `reclaim.py`) uses

- `MissionActor(PlayerController)` → a struct holding a `PlayerController`
  plus actor state; the creature implements `adventure::ControlledSpider`
  (`advance_pose` is the creature pipeline Python's `advance_pose` ran).
- `mission_factory::plan_mission(profile, state_dir, rects, primary)` →
  `MissionPlan { info, layout }` (layout None for Reclaim: build it from the
  desktop snapshot).
- Layout: `ScreenLayout::{primary, others, clamp, route, screen_at, tunnel_at}`.
- Buildings: `custom_maps::placements(&map)` or `map_layouts::layout_for(id)`;
  `Placement.guards` is `Guard::Role` or `Guard::Spec`. `custom_maps::load_map`
  gives `cap`, `wave_every`, `tactics`, `enemies`, `boss`.
- `encounters::profile(kind)`, `EncounterDirector::new(profile, layout, tier)`,
  `.outposts(&site_points, 240.0)`, `.update(dt, &mut sites)` with sites
  implementing `OutpostSite`, `.retry(index)`.
- `campaign::enemy_loadout(&info, role, &mut rng)`, `roll_drop(..)`,
  `armoury::add_loot`, `adventure_profile::{record_result, unlock_companion,
  store_progression, spider_progression, companion_progression,
  hero_progression, save_profile, load_profile}`, `PARTY_SIZE`.
- `dc_world::tactics::{plan, tactics_level, Member, Foe}`.
- `controls::second_player_controls(Some(controls.movement))`.

## Tests

Unit tests port the data parts of `test_adventure_controls.py`,
`test_map_view_and_release.py`, `test_turn_movement.py`, `test_local_coop.py`,
`test_editor_and_admin.py`, `test_mission_variety.py`,
`test_loot_and_campaign.py`, `test_screens_and_reclaim.py`,
`test_adventure_hero.py` and `test_armour_set.py`. `tests/python_parity.rs`
checks fixtures from `tools/dump_fixtures.py`: every tooltip, `enemy_loadout`/
`roll_drop` draw order (12 seeds × all maps × 3 roles, plus the next draw),
360 encounter plans with 90 s wave traces, odd custom maps, controls and
profile files.
