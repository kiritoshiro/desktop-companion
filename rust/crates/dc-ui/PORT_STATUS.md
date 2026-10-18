# dc-ui port status

egui 0.37 windows for the PyQt5 windows in `src/desktop_bug/app/`. Each window
is a struct with `fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction>` (and
`show(&mut self, ui)` to draw inside a parent). dc-ui never spawns processes or
writes files; the app layer carries out the `UiAction`s and answers through
setters (`load_preset`, `set_status`, `set_party`, `saved`, ...).

| Python module | Rust | Status | Notes / missing |
|---|---|---|---|
| `app/wood_theme.py` | `theme.rs`, `widgets.rs`, `paint.rs`, `assets.rs` | ported | Palette constants, `_lift`, Qt `lighter`/`darker`; Segoe UI / Consolas loaded from the Windows font folder; `dialog_visuals` (dialog_qss/app_qss), `companion_visuals` (companion_qss), `shell_visuals` (shell_qss); frames for every styled `QFrame`; carved wood/brass/stop buttons with hover/pressed/disabled, check boxes, pill switches, segment control, key caps, player badges, `VitalBar`, `StatTile`, section headers, level badge, team chip, XP bar, wood tab bar, tiled wood boards and the nine-sliced `card_frame.png`. All 29 `menu_*.png` emblems, mode art and textures load as textures; missing art falls back to plain colours. `app_icon` is used as the window icon. Not ported: `apply_app_theme`'s `SetCurrentProcessExplicitAppUserModelID` (dc-platform's job). |
| `app/skill_art.py` | `art.rs` | ported | Medallions, branch crests, stat icons drawn as egui shapes (gradients approximated by meshes; locked = drained to grey). |
| `app/stat_text.py` | `stat_text.rs` | ported | Tooltips are `(text, colour, strong)` lines instead of Qt HTML. |
| `app/config_ui.py` | `windows/config.rs` | ported | All six sections with their emblems, titles and purpose lines; slot table (category, skin with thumbnails, colour swatch, temperament, count, "n of 5" skills, colour-only team, job, remove); abilities, colours (palette presets + Surprise me), new-team, clear and reset dialogs; teams panel (rename, recolour, stances); size, Draggable, Always show switches; flies panel shown only when on with min/max coupling; summary as the launch tooltip; status bar; version in `ConfigWindow::title()`; `apply_live_session_state`; count_random / retired-override / unknown-setting round trips. Process control, the live channel and file I/O are `UiAction`s. The Qt colour dialog is egui's colour picker. Thumbnails come from a `ThumbnailFn` (dc-paint); a fallback spider is drawn without one. |
| `app/mode_menu.py` | `windows/mode_menu.rs` | ported | Home with three pictured mode cards; page headers with "← Modes" (leaving Adventure yields `LeaveAdventure`); Adventure page: hero strip, mission cards in a 3-column grid with record/lock lines and pictures, "Your maps", the raid setup card (One/Two player segments, key-cap rows, per-screen pill switches), Controls/Map editor/Admin buttons; Strategy plaque. Map pictures come from a `MapPictureFn` (map_art in dc-paint); a stand-in is painted without one. |
| `app/controls_ui.py` | `windows/controls.rs` | ported | Movement, aim cone, face-mouse (disabled in turn mode), rebinding by key, modifier or mouse button with swap, Esc cancels, reset, `lock_bindings` for two players; `ControlsDialog`. |
| `app/character_ui.py` | `windows/character.rs` | ported | `skill_tree` (Body/Fangs/Legs/Silk columns, crests, linked medallion nodes), `CharacterSheet` (badge, name, XP, Armour/Skills tabs, who buttons, doll above bag, item panel with upgrade/sell, bonuses and set lines), `CharacterDialog`, `PartyDialog` (two sheets sharing one bag), `CharacterWindow::open` (`show_party_windows`). Armoury rules are applied to the view at once and reported as actions. |
| `app/armour_ui.py` | `windows/armour.rs` | ported | `SpiderDoll` geometry, hit zones, plaques and callouts, drag-and-drop both ways, double-click to take off, click to pick; `InventoryBag` with tier-sorted tiles, level/spare badges and tooltips. Worn pieces are tier-coloured overlays until dc-paint's `armour_art` painters are wired (`ArmourIconFn` already accepted for icons). |
| `app/inspector_ui.py` | `windows/inspector.rs` | ported | Portrait header (`ThumbnailFn`), team chip, level badge, XP, Overview (vitals, four stat tiles, pins, team picker + typed name committed on Enter, relationships), Skill tree, Inventory & armour (doll, bag, item label, Add to bag). `InspectorView` is filled by the overlay (TODO: from dc-creature/dc-manager). |
| `app/adventure_ui.py` | `windows/adventure.rs` | partial | `PauseDialog`, `AdventureSettingsDialog`, `short_binding`, `hud_rect` ported. `draw_hud`, `_board`, `_well` paint the overlay and belong to dc-paint. |
| `app/admin_ui.py` | `windows/admin.rs` | ported | The window. Its profile edits (`reset_progress`, `give_amber`, ...) are dc-adventure's (`admin.rs`) and arrive as actions. |
| `app/map_editor.py` | `windows/map_editor.rs` | ported | Two-screen canvas with drag to move (and to the second screen), Map/Buildings/Enemies/Boss tabs, `SpiderEditor`, guards (max 8), problems line, New/Save/Delete (confirmed)/Save and play/Close. Rules (`clean_map`, `problems`, `new_map`) are dc-adventure's. Building pictures are placeholders until dc-paint's `building_art`. |

## Data

`model/` holds the view models the windows draw. `bridge.rs` maps the real
types onto them: `settings_catalog`/`presets` (dc-content discovery, skills,
jobs, palettes, body plans, enemy kinds), `progression_catalog` and
`From<&ProgressionState>` (dc-state), `adventure_view`, `party_view`,
`companions`, `enemy_kind_list` and `ControlSettings` both ways
(dc-adventure). `sample.rs` keeps copies of the same tables for tests; a test
checks they equal the real ones, and that `teams.rs` agrees with
`dc_state::teams`.

Remaining `TODO(content)`: `InspectorView` from a live creature
(dc-creature/dc-manager), dc-paint portraits/thumbnails/armour/map/building
art via the callback types, and `teams.rs` could delegate to `dc_state::teams`
outright (it is checked equal instead). dc-paint's `armour_icon` and dc-art's
`building_art`/`map_art` now exist and return `dc_render` images; the app
layer turns them into textures and passes them through `ArmourIconFn`,
`MapPictureFn` and `ThumbnailFn` (dc-ui does not depend on dc-paint/dc-art).

## Runner

`run_window(title, size, app)` / `run_window_with(RunOptions, app)`: winit
0.30 + egui-winit + `egui_wgpu::winit::Painter`, repaint on demand,
`exit_after` for smoke runs. `examples/settings_window.rs` opens the settings
window on the checkout's real content (`--seconds N`, `--page`, `--root`).

## Tests

Unit tests per module, plus headless integration tests (`tests/`) that drive
windows through `egui::RawInput` clicks, keys and drags on recorded widget
rects (`probe.rs`) and read labels and toggled/disabled state from the
accesskit tree. They port the intent of test_settings_table,
test_settings_consolidation, test_settings_polish, test_settings_layout,
test_reset_progress, test_team_identity, test_adventure_polish,
test_ingame_polish, test_inspector_ui, test_adventure_controls,
test_local_coop, test_editor_and_admin, test_wood_theme and
test_mission_list_and_close (window side).

Result (2026-10-18): `cargo test -p dc-ui` 99 passed (58 unit, 16 adventure
windows, 25 settings window); `cargo clippy -p dc-ui --all-targets` has no
dc-ui warnings; the example ran with `--seconds 3` and `--page skirmish
--seconds 2` and closed itself.

Layout fixes made while finishing: mission and custom-map cards stack top to
bottom (a grid cell is a horizontal layout, which had spread the cards past
the window), and the right-aligned Done/Reset rows reserve one row instead of
centring in the remaining height (which pushed "Apply and return" off the
Adventure settings dialog).
