# overlay_ui: what the engine calls

`src/overlay_ui.rs` (+ `src/overlay_ui/`) is the overlay's menus and dialogs.
The original interface (`UiRequest`, `EngineCommand`, `OverlayUi::{new,
request, is_open, owns_window, window_event, frame, take_commands}`) is
unchanged. Everything below is an **addition** the engine has to wire.

## New `EngineCommand` variants (handle them; the match in `adventure/mod.rs` must list them)

**dc-app does not build until `adventure/mod.rs` `Game::apply` lists them.**
Minimal fix (line ~577), then route them properly in `engine.rs`:

```rust
EngineCommand::SpiderAction { .. }
| EngineCommand::OverlayAction { .. }
| EngineCommand::Announce(_)
| EngineCommand::SetTargetFps(_)
| EngineCommand::SaveProgress => return false,
```

(`SaveProgress` belongs to the game: `mission.save_progress(manager)`.)

| Variant | Meaning | Engine does |
|---|---|---|
| `Announce(String)` | `_announce(message)` / the tray's `announce` | tray balloon (`Tray::show_message`, 1400 ms) **and** `tray.set_text("status", msg)` |
| `SetTargetFps(f64)` | tray "Performance", Adventure settings "Apply and return" | `set_target_fps` (frame policy) |
| `SaveProgress` | pause "Retry saving victory" / "Exit raid", and before the character window opened from the pause menu | in a raid `mission.save_progress(manager)`, else `manager.save_runtime_state()` |

`ReleaseSpider` was **not** added: "Release spider" is only offered outside a
raid (`AdventureContext::in_mission == false`), which the Rust engine never
runs; that choice just resumes.

## New `OverlayUi` methods

- `sync(&mut self, manager: &CreatureManager)` — **call every tick before
  `frame`** (Companion: the manager; Adventure: the mission's manager). A
  new menu gets its entries and the inspector its live data (every 400 ms,
  plus the dc-paint portrait) from it. A menu or inspector does not appear
  until it has been synced once. Cheap when nothing is open.
- `set_overlay_origin((x, y))` — the overlay's top-left on the desktop
  (physical px, `virtual_screen_geometry()` x/y). `at` in requests is a
  desktop position; this maps it to manager coordinates ("Add a cage here",
  "Put the base down here"). Call on start and when the geometry changes.
- `set_overlay_dpi(map)` — the overlay's local `DpiMap`, with the origin:
  the desktop position also converts to the manager's logical pixels on a
  scaled monitor (identity at 100%). Same moments as the origin.
- `set_adventure(AdventureContext { in_mission, players, target_fps,
  player_creatures })` — what the pause menu offers, one vs two-player
  character window, the settings dialog's frame rate. Default: in a raid, one
  player, 60 FPS.
- `with_state_dir(path)` — constructor; `new()` uses
  `dc_content::discovery::state_dir()` (profile and `controls.json` live there).
- `carried_base()` — id of the base picked up with "Move the … base…".

## New free functions

- `apply_spider_action(&mut manager, creature, action, arg) -> Vec<EngineCommand>`
  for `EngineCommand::SpiderAction`. Actions: `pin_level`, `pin_health`
  (`on`/`off`), `rename` (name; `""` clears), `clear_name`, `skill`
  (`<id>=on|off`), `team`, `relation` (`<other id>=friend|neutral|foe`),
  `unlock`, `equip`, `unequip`, `add_item`, `cage` (optional `x,y`).
- `apply_overlay_action(&mut manager, &mut rng, action) -> Vec<EngineCommand>`
  for `EngineCommand::OverlayAction` (`name` or `name=arg`; see the doc
  comment for the list: `fps=`, `size=`, `randomize_*`, switches,
  `fly_rate=`, `add_cage[=x,y]`, `remove_cages`, `remove_base=`,
  `move_base=<id>@x,y`, `remove_bases`, `quit`). Feed what both return
  (`Announce`, `SetTargetFps`, `Quit`) back into the command handling.
- Tray: `tray_menu_for(&manager, fps)` (or `tray_menu()` before the manager
  exists) builds Python's whole tray menu; on `TrayEvent::Menu { id, checked }`
  call `tray_event_checked(&id, checked)` (or `tray_event(&id)`), and after
  applying commands push `tray_checks(&manager, fps)` into `Tray::set_checked`
  (the right-click menu changes the same switches). `initial_status(&manager)`
  is the status line's first text.

## Flow notes

- Character (`UiRequest::Character`) and pause → "Character & armour":
  the UI emits `SaveProgress` and opens the window on the **first `frame`
  after `take_commands`**, so apply commands right after taking them. On
  close: `ProfileChanged` (from the pause menu, the pause menu then comes
  back). Saving before your own `UiRequest::Character` as well is harmless.
- Pause: `Resume` on Resume / Esc / close button; `RestartMission, Resume`;
  `SaveProgress, ExitMission`; "save" keeps the menu open. Settings →
  `SetTargetFps` (only on "Apply and return"), `ControlsChanged`, pause again.
- Controls (`UiRequest::Controls`): saved to `controls.json` on every change;
  `ControlsChanged` on close.
- `is_open()` is true from the request until the last window closes,
  including the frame a character window waits for its save.
- Windows: pop-up menus and dialogs are separate always-on-top winit windows
  in the engine's event loop (dc-ui `host::HostedWindow`, one shared wgpu
  device). Forward their events (`owns_window`) and redraws to
  `window_event`. Menus close on Esc, focus loss, or any mouse press outside
  them (polled in `frame`, so keep calling `frame` while `is_open()`).
