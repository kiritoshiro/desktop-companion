# dc-app port status

The binary: the settings process (`app/config_ui.py` `main`, no
`--engine`) and the overlay process (`app/engine.py` `main`, `--engine`).
The game logic, art and windows it drives live in the other crates; this
table covers what dc-app itself carries out of each Python module.

```
CARGO_TARGET_DIR=target-<you> cargo test -p dc-app      # headless: no window opens
DC_ADVENTURE_FRAMES=<dir> cargo test -p dc-app frames_to_png_on_request   # Adventure frames as PNG
```

## Modules

| Python | Rust | Status |
|---|---|---|
| `app/engine.py` `OverlayWindow` (Companion) | `engine.rs` | **ported**: one window over the virtual desktop (GPU on DirectComposition, `UpdateLayeredWindow` fallback), manager ticked at the frame policy's rate, global left button polled, desktop windows refreshed every 1.25 s, monitor changes, preset live-reload and stop requests (700 ms), label metrics from Segoe UI, hover, cursor capture by mouse webs (never in `--seconds` runs). **Missing:** camouflage sampling, partial repaints, the profile HUD, DPI scaling (all physical pixels). |
| `app/engine.py` `OverlayWindow` (Adventure) | `adventure/mod.rs`, `adventure/input.rs`, `adventure/draw.rs`, `adventure/screens.rs` | **ported**: mission from the profile (`create_mission`), `mission.update` per tick, victory / defeat / Reclaim crash and blue screen, close after the end title, progress saved (never `creatures.json`); P1/P2 keys (turn-and-walk), Esc always pauses, K character, `` ` ``/`~`/VK_OEM_3 map view, F8 release as a Windows hotkey (raid paused, banner), mouse shot at the pointer, companion command buttons, HUD drag, `_adventure_captures_mouse`, NOTOPMOST + keyboard focus, controls reload. **Partial:** no free-roam (possess a spider outside a raid), so "Release spider" just resumes. |
| `app/engine.py` `_show_context_menu`, `create_tray`, `_add_label_switches`, `_prompt_name`, `_announce` | `overlay_ui.rs`, `overlay_ui/*` (`OVERLAY_UI.md`) | **ported**: spider and overlay right-click menus, base carrying, cages, label switches, rename prompt, inspector, pause / character / controls / Adventure settings windows, Python's tray menu with live check marks and status line, balloons. |
| `app/engine.py` live channel (`OverlayChannelServer`) | — | **missing** on the overlay side: the settings window falls back to the preset file and the stop-request file, as Python did when the channel failed to bind. |
| `app/engine.py` `main`, `support/logging_setup.py` use | `main.rs` | **ported**: arguments, legacy state migration, logging. `--software`, `--seconds`, `--snapshot` are development additions. |
| `app/config_ui.py` `main` and process control | `settings.rs`, `settings/tests.rs` | **ported**: dc-ui `ConfigWindow` + mode menu, launch/stop of the overlay (stop request, then kill), live channel client, preset save/load and live apply, reset progress, Adventure profile edits (map, two players, screens), the window hidden while an Adventure overlay runs. |
| `app/character_ui.py`, `armour_ui.py`, `controls_ui.py`, `map_editor.py`, `admin_ui.py`, `inspector_ui.py`, `mode_menu.py`, `adventure_ui.py` (dialogs), `wood_theme.py` | dc-ui windows, opened by `settings.rs` and `overlay_ui` | **ported** (see dc-ui's PORT_STATUS); dc-app supplies thumbnails, portraits and armour icons from dc-paint. |
| `app/adventure_ui.py` HUD, `mission_ui.py`, `mission_backdrop.py`, `mission_art.py`, `desktop_surface.py` | dc-art, painted from `adventure/draw.rs` | **ported** (dc-art); dc-app fills the view structs. |
| `app/mission_factory.py`, `mission.py`, `swarm.py`, `reclaim.py`, `adventure.py`, `controls.py`, `adventure_profile.py`, `campaign.py`, `encounters.py`, `armoury.py`, `custom_maps.py`, `map_layouts.py`, `stat_text.py` | dc-adventure, run on dc-manager | **ported** (dc-adventure). |
| `app/desktop_capture.py` | dc-platform capture, converted in `adventure/screens.rs` | **partial**: the wallpaper is not decoded (no JPEG decoder), so a broken Reclaim window shows plain blue behind it. |
| `app/screen_activity.py`, `window_placement.py` | dc-platform, used by `adventure/screens.rs` | **ported**: playable monitors from the profile's disabled screens and monitor power states; main monitor work area. |
| `app/overlay_win32.py`, `session_control.py`, `live_channel.py`, `support/frame_policy.py`, `support/dpi.py` | dc-platform | **ported** (dc-platform). |
| `support/profiling.py` | — | **missing** (the HUD and `DESKTOP_BUG_PROFILE`). |

## Tests

`cargo test -p dc-app` (87, all headless): the Adventure engine
(`adventure/tests.rs`: raid from the profile's map, keys move the player,
bindings and Esc, pause and the UI hand-off, K and `ProfileChanged`, map
view and tilde, release and hotkey, two players, mouse drag/shot/commands,
victory/defeat recorded, Reclaim blue screen, restart, controls reload,
menu commands, CPU-rendered frames), key mapping and capture conversion,
the overlay UI layer (`overlay_ui/tests.rs`) and the settings process
(`settings/tests.rs`, byte-for-byte preset saves against Python). Tests
that touch dc-content's process-wide state-dir override hold
`test_support::state_lock()`; the others pass their folder explicitly.

## Owner should check live (not verifiable headlessly)

Adventure:
1. The overlay gets the keyboard at start (W/A/S/D, arrows, Esc, K, `` ` ``).
2. F8 works with another window focused; pressing it again takes the keyboard back.
3. Clicks never reach the desktop while playing (GPU path); on `--software`, fully transparent pixels may still let clicks through.
4. Reclaim capture: right monitors and scaling, the overlay not in the picture.
5. The settings window comes back after the end title.
6. Tray balloons for the map and release keys.
7. The pause menu flow with the real windows (Resume, Restart, Exit, Character, Controls, Settings).
8. Frame rate with the backdrop and Reclaim on the GPU at 2560×1440 + 4K.

Menus and dialogs:
9. Menu placement and DPI on each monitor (mixed scaling).
10. Menus and dialogs above fullscreen apps and above the overlay.
11. A menu closes on an outside click and on Esc.
12. Focus moves into a menu or dialog and back to the overlay after it.
13. Latency when a menu or dialog first opens (wgpu device and fonts).
14. The inspector's portrait looks right.
15. Carrying a base captures the whole screen until the right-click that puts it down.

Settings window:
16. It hides before the Adventure overlay launches (not in the Reclaim picture).
17. Dialog sizes inside a 960×860 window.
18. Non-modal dialogs: the window behind stays usable and nothing doubles.
19. Stop can freeze the window for up to 5 s (stop request, then kill).
20. Missing slot thumbnails (a fallback spider is drawn).
