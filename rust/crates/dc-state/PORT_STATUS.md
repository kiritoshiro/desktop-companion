# dc-state port status

Port of `src/desktop_bug/state/*`, plus the file I/O half of `manager/persistence.py`.

| Python module | Rust | Status | Notes |
|---|---|---|---|
| `state/progression.py` | `progression.rs` | ported | `MAX_LEVEL`, `RELATIONS`, `TEAM_OPTIONS`, `ABILITY_TREE`, `ARMOR_CATALOG` (30 pieces), `ARMOR_TIERS`, `ARMOR_SETS`, `level_multiplier`, `item_at_level`, `set_pieces_worn`, `set_bonus_effects`, `xp_to_next_level`, `growth_multipliers`, `ProgressionState` (`from_value` = `from_dict`, `to_value` = `to_dict`, serde via those; `add_item`, `equip`, `unequip`, `can_unlock`), `equipped_items`, `normalize_team_id`, `normalize_team_stances`, `team_stance`, `relation_between`. |
| `state/teams.py` | `teams.rs` | ported | `TeamProfile` (colour is `dc_core::Color`), `default_color`, `distinct_color`, `fnv1a`, `default_name`, `parse_color`, `color_to_hex`, `normalize_team_name`, `normalize_teams`, `teams_payload`, `team_profile`/`team_label`/`team_color`, `stance_label`, `describe_stance`, `stance_pairs`, `minimal_stances`, `is_default_hostile`, `HOSTILITY_NOTE`. |
| `state/runtime_state.py` | `runtime_state.rs` | ported | Schema 3 constants, `normalize_namespace`, `normalize_state_key`, `new_creature_id` (seeded: `getrandbits(48)`), `slot_id_from_key`, `member_index_from_key`, `next_birth_ordinal`, `entry_rank`, `next_launch`, `migrate_creatures`, `saved_slot_members`, `slot_member_ids`, `load_payload`, `load_scene`, `stamp_seen`, `evict`, `build_payload`, `reset_saved_progress`. Rules run on raw JSON (old schemas are arbitrary dicts). |
| `manager/persistence.py` (file I/O only) | `runtime_state.rs` | partial (by design) | `RuntimeState::load` (= `_load_progression_states`), `RuntimeState::record` (one spider in the save loop, birth ordinal kept), `RuntimeState::save` (evict + atomic `.tmp` + rename), `write_payload_atomic`. Collecting live creatures, cages, webs and nests stays in `dc-manager`. |

## Typed views

- `SavedCreature`: one `creatures.json` entry (`name, model, personality, progression, slot_id, born, last_seen` + `extra`).
- `Scene { cages, webs, nests }`: `Vec<serde_json::Value>` each, because `dc-world` owns those shapes. `bases` is `Vec<Value>` for the same reason.

## Deliberate differences

- `new_creature_id` without an RNG uses `PyRandom::from_entropy().getrandbits(48)` in place of `uuid4().hex[:12]` (same format: 12 hex digits).
- `reset_saved_progress` treats a non-integer `launch` as 1, where Python would raise.

## Tests

- `tests/python_oracle.rs` against `tests/fixtures/state_oracle.json` (from `tools/dump_fixtures.py`): XP curve, growth, level multipliers, every item at several levels, talents, sets and set bonuses, `from_dict` on ten awkward inputs, team default/distinct colours and names, `normalize_teams` cases, `teams_payload`, `parse_color`, stances (normalise/minimal/pairs/queries), seeded ids, key parsing, migration (including order), slot members, eviction, payload, and a byte-for-byte check of the JSON writer.
- `tests/ported.rs`: test_progression (state parts), test_team_relations, test_team_identity (state parts), test_state_migration (including a file round trip across two launches), test_scene_persistence (state parts), test_reset_progress (file parts).

## Missing

- The tests that need a `Creature` or `CreatureManager` (XP gain and level-up, talent auto-spend, live team renames, restoring cages/webs) belong to `dc-creature`/`dc-manager`.
