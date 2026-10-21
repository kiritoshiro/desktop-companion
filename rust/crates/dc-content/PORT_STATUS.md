# dc-content port status

Port of `src/desktop_bug/content/*`.

| Python module | Rust | Status | Notes |
|---|---|---|---|
| `content/body_plans.py` | `body_plans.rs` (+ `data/body_plans.json`, exported unchanged) | ported | Enemy rigs computed in Rust; matches Python to the bit (oracle). `resolve_body_plan` on raw JSON and `resolve_model_body_plan` on `ModelDef`. |
| `content/personality_profiles.py` | `personality_profiles.rs`, `personality.rs` | ported | All tables, `temperament_for`, `canonical_personality_definitions`, `selectable_personality_ids`, `behaviour_modules_for`, `movement_profile_for`, `personality_gait_style`, `phase_scores_for`, `phase_duration_multiplier_for`, `ability_bundle_ids_for`, `ability_ids_for`, `annotate_personality`. Dict-or-id arguments are `PersonalityRef`. |
| `content/skills.py` | `skills.rs` | ported | Registry, normalisation, composition, summaries, `SkillSet`. |
| `content/palettes.py` | `palettes.rs` | ported | `colorsys.hsv_to_rgb`/`rgb_to_hsv` ported bit-exact; `random_palette` keeps the draw order. |
| `content/enemy_kinds.py` | `enemy_kinds.rs` | ported | `pick_skin` uses `PyRandom::from_int(seed).choice`. |
| `content/enemy_traits.py` | `enemy_traits.rs` | ported | `Temperament` is `EnemyTemperament` (name clash with personality temperament). |
| `content/discovery.py` | `discovery.rs` | ported | Roots, `state_dir` (`DESKTOP_BUG_STATE_DIR`, `portable.txt`, `%LOCALAPPDATA%`), legacy-state migration, writability probe, model/personality/preset discovery, `validate_model`, `validate_personality`. Rules also on an explicit `StateLocation`. `run_data_checks` = the data half of `tools/run_all_checks.py`. |
| `content/preset_io.py` | `preset_io.rs` | ported | `validate_preset` (same messages), `load_preset`, `save_preset` (user folder, shipped presets refused), `safe_preset_filename`. Also hosts `parse_color` from `state/teams.py` (needed by validation; `dc_state::teams::parse_color` wraps it). |
| `world/jobs.py` (job table only) | `jobs.rs` | partial (by design) | Only `JOB_DEFINITIONS`/`JOB_IDS`/`normalize_job_id`, because preset validation needs them and `dc-world` depends on this crate. Duty cycles and job logic stay in `dc-world`, which should re-export these. |

## Typed files

- `model.rs`: `ModelDef` (required keys plain; `appearance`, `fur`, `legs`, `render_mode`, `body_plan` optional), `LegDef`, `Appearance` and its sub-objects (`LegChain`, `Antennae`, `BodyPart` for `head`/`pedicel`, `LegConnections`, `LegShadow`, `CarapaceRim`, `SpiderGait`, `Fur`). All 62 shipped models load and round-trip.
- `personality.rs`: `Personality` (required keys plain, composition keys typed, every flat tunable in `tunables` with `f64_or`/`range`/`flag` accessors), `Temperament`. All 21 files load and round-trip.
- `preset.rs`: `Preset`, `PresetSlot`, `PresetSettings`, `FliesSettings`, `TeamEntry`. All 5 presets load and round-trip.
- Every level has a flattened `extra` map, so unknown keys round-trip.

## Deliberate differences (forced)

- **Defaults:** optional fields are `Option`. Python applies different defaults to the same key at different call sites (e.g. `abdomen_scale`), so the consumer applies its own. Accessors carry a default only where every Python reader agrees (`LegDef::attach_side() = 0.30`, `LegChain::segment_count() = 4`, `ModelDef::render_mode() = "procedural"`, ...).
- **Numbers:** known numeric fields are `f64`, so an integer such as `"base_size": 27` writes back as `27.0` (equal under Python `==`; the tests compare with `util::json_eq_numeric`). Unknown keys keep their exact form.
- **`app_root()`:** a Rust binary has no source file to count up from. Order: `set_app_root` override, `DESKTOP_BUG_APP_ROOT`, the nearest folder at or above the executable that has `models/` and `personalities/`, this crate's checkout, the executable's folder. There is no PyInstaller `_MEIPASS`; the `_internal` fallback stays.
- **`set_state_dir_override`:** a process-wide override ahead of `DESKTOP_BUG_STATE_DIR`, because tests cannot safely change the environment of a threaded process.
- **JSON writing:** `util::python_json_dumps` reproduces `json.dumps(indent=2)` byte for byte (ensure_ascii, separators, float repr). The dc-state oracle checks this.

## Tests

- `tests/python_oracle.rs` against `tests/fixtures/content_oracle.json` (from `tools/dump_fixtures.py`): body plans, palettes, enemy kinds/skins/picks, enemy traits, every discovered personality (annotated) and model (resolved), derived skills/modules/gaits, temperament/phase tables by id, skill summaries, and the validation messages for 70+ bad inputs.
- `tests/ported.rs`: test_body_plans, test_personality_catalog, test_model_variations, test_behaviour_modules (data parts), test_trapping_is_reachable, test_enemy_kinds (data parts), test_preset_path (source build), test_preset_save_location, test_state_location; plus loading and round-tripping every model, personality and preset file, and `run_data_checks`.

## Missing

- `test_preset_path.py::test_frozen_build` (PyInstaller layout) has no Rust equivalent.
- `migrate_legacy_state_dir` copies with `std::fs::copy` (no timestamp preservation like `shutil.copy2`).
