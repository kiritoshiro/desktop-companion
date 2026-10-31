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
| `app/engine.py` `OverlayWindow` (Adventure) | `adventure/mod.rs`, `adventure/input.rs`, `adventure/draw.rs`, `adventure/screens.rs` | **ported**: mission from the profile (`create_mission`), `mission.update` per tick, victory / defeat / Reclaim crash and blue screen, close after the end title, progress saved (never `creatures.json`); P1/P2 keys (turn-and-walk), Esc always pauses, K character, `` ` ``/`~`/VK_OEM_3 map view, F8 release as a Windows hotkey (raid paused, banner), mouse shot at the pointer, companion command buttons, HUD drag, `_adventure_captures_mouse`, NOTOPMOST + keyboard focus, controls reload. **Not ported, unreachable in Python:** free-roam (possess a spider outside a raid). Since `5e57876` Python's Adventure always builds a mission (`create_mission` never returns None, `mission.player` is never None), so every `self.mission is None` / `self.player is None` branch is dead there: `_possess_next_spider` on a click, the "Click a spider to take control" hint (only its rect is still added to the repaint region), the free pause menu with "Release spider", bite held in `tick`, the release on death and the inspector for K. The UI half is ported (`AdventureContext.in_mission = false`, tested); the engine never sets it, and "Release spider" just resumes. |
| `app/engine.py` `_show_context_menu`, `create_tray`, `_add_label_switches`, `_prompt_name`, `_announce` | `overlay_ui.rs`, `overlay_ui/*` (`OVERLAY_UI.md`) | **ported**: spider and overlay right-click menus, base carrying, cages, label switches, rename prompt, inspector, pause / character / controls / Adventure settings windows, Python's tray menu with live check marks and status line, balloons. |
| `app/engine.py` live channel (`_channel_server`, `_on_channel_message`, `_apply_pushed_preset`, `_broadcast_session_state`) | `live.rs` | **ported**: one pipe server per overlay (dc-platform), serviced every loop pass; pushed presets for the running preset applied at once (and the file poll told), progress reset, stop request (save and quit), `session_state` on connect, after a pushed preset and on every announcement. Adventure honours only stop, as Python. The preset-file poll and stop-request file stay as fallbacks. |
| `app/engine.py` `main`, `support/logging_setup.py` use | `main.rs` | **ported**: arguments, legacy state migration, logging. `--software`, `--seconds`, `--snapshot` are development additions. |
| `app/config_ui.py` `main` and process control | `settings.rs`, `settings/tests.rs` | **ported**: dc-ui `ConfigWindow` + mode menu, launch/stop of the overlay (stop request, then kill), live channel client, preset save/load and live apply, reset progress, Adventure profile edits (map, two players, screens), the window hidden while an Adventure overlay runs. |
| `app/character_ui.py`, `armour_ui.py`, `controls_ui.py`, `map_editor.py`, `admin_ui.py`, `inspector_ui.py`, `mode_menu.py`, `adventure_ui.py` (dialogs), `wood_theme.py` | dc-ui windows, opened by `settings.rs` and `overlay_ui` | **ported** (see dc-ui's PORT_STATUS); dc-app supplies thumbnails, portraits and armour icons from dc-paint. |
| `app/adventure_ui.py` HUD, `mission_ui.py`, `mission_backdrop.py`, `mission_art.py`, `desktop_surface.py` | dc-art, painted from `adventure/draw.rs` | **ported** (dc-art); dc-app fills the view structs. |
| `app/mission_factory.py`, `mission.py`, `swarm.py`, `reclaim.py`, `adventure.py`, `controls.py`, `adventure_profile.py`, `campaign.py`, `encounters.py`, `armoury.py`, `custom_maps.py`, `map_layouts.py`, `stat_text.py` | dc-adventure, run on dc-manager | **ported** (dc-adventure). |
| `app/desktop_capture.py` | dc-platform capture, converted in `adventure/screens.rs` | **ported**: the wallpaper is decoded with the `image` crate (JPEG, PNG, BMP; format sniffed, so `TranscodedWallpaper` works) and filled per screen with smooth scaling, like Qt's `SmoothTransformation`. A 3840x2400 JPEG takes about 90 ms at mission start. |
| `app/screen_activity.py`, `window_placement.py` | dc-platform, used by `adventure/screens.rs` | **ported**: playable monitors from the profile's disabled screens and monitor power states; main monitor work area. |
| `app/overlay_win32.py`, `session_control.py`, `live_channel.py`, `support/frame_policy.py`, `support/dpi.py` | dc-platform | **ported** (dc-platform). |
| `support/profiling.py` | — | **missing** (the HUD and `DESKTOP_BUG_PROFILE`). |

## Tests

`cargo test -p dc-app` (91, all headless): the Adventure engine
(`adventure/tests.rs`: raid from the profile's map, keys move the player,
bindings and Esc, pause and the UI hand-off, K and `ProfileChanged`, map
view and tilde, release and hotkey, two players, mouse drag/shot/commands,
victory/defeat recorded, Reclaim blue screen, restart, controls reload,
menu commands, CPU-rendered frames), key mapping and capture conversion,
the overlay UI layer (`overlay_ui/tests.rs`) and the settings process
(`settings/tests.rs`, byte-for-byte preset saves against Python), and
the live channel (`live.rs`: real pipe, real manager; one test drives the
real settings window against the overlay's end). Tests
that touch dc-content's process-wide state-dir override hold
`test_support::state_lock()`; the others pass their folder explicitly.

## Live run 2026-10-21 (release build, 2560x1440 main + 3840x2160, both 100%)

Seen working: the settings window (mode menu; fixed: third card cut off,
window not centred), Play Adventure from it (a 2 min territory raid at
59.5 fps, map key, defeat recorded in the profile only), the Companion
overlay on the GPU (5 spiders on both monitors, labels and bars), the live
channel against the real overlay (state on connect, pushed preset applied
at once, stop in 179 ms with the colony saved), `--software` (draws, but 99%
of a core at 51.6 fps: whole-desktop repaints), Reclaim on both monitors
(item 4: the frozen picture lines up, no overlay in it), the wallpaper
decoded, and the packaged build from outside the repository. With 40
spiders the overlay used 22% of a core at 59.8 fps; Python 84% (fps not
logged). Two Reclaim starts saw one screen while the 4K monitor was waking
(Kernel-Power 566); later runs saw both.

Still needs someone at the mouse: items 1-3, 5-7 and 9-21 below.

## Owner's live check 2026-10-25 (same setup)

Found by the owner:
- Dragging a spider also clicked the desktop under it, and right-click on
  a spider did nothing. Cause: the DC-79 click-through check never ran.
  Its 50 ms `WM_TIMER` (and the style and geometry timers) starve, because
  winit always has a paint queued while the overlay draws every vsync, and
  Windows delivers timers only when no paint is pending. The window stayed
  `WS_EX_TRANSPARENT` all the time; dragging still worked only because the
  left button is polled globally. Measured: the hit test was called 0 times
  in 20 s; with `OverlayWindow::service_timers` called on every loop pass,
  about 17 times a second.
- Settings window: tooltip text nearly invisible; the Team colour bubble and
  its arrow squashed.
- Adventure plays a spider other than the tarantula the owner had in the
  colony (no spider choice yet; a later feature, not a port bug).

Fixed the same day (`834185d` the overlay check; `1f6f670` the tooltip
ink and a 14 px bubble in a 52 px combo, as Qt) and rechecked live by the
owner: dragging no longer clicks the desktop, right-click on a spider
opens its menu, tooltips are readable, the team bubble is round.

Observation: the first Adventure raid that day ran at 31.4 fps (later
ones 57-59).

Not yet confirmed: whether menus close on an outside click and Esc
(item 11), and that text on the walnut panels is still cream after the
tooltip change (the global default text colour is now ink).

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
21. Live channel: Save while the overlay runs changes the spiders at once
    (no restart), a tray toggle shows up in the window, and Stop closes the
    overlay quickly instead of waiting for the file poll.
