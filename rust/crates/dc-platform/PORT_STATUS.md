# dc-platform port status

Windows-only Win32 shell. All coordinates are **physical pixels** (winit makes
the process per-monitor-v2 DPI aware), so the Qt logical/physical crossings
of the Python mostly disappear; `dpi.rs` keeps the conversions for callers
that still want logical units.

| Python module | Rust | Status | Notes / missing |
|---|---|---|---|
| `app/overlay_win32.py` | `overlay_win32.rs` | ported | Takes `hwnd`, `input_transparent`, `adventure` explicitly instead of reading them off the widget. Adds `WS_EX_NOACTIVATE` in companion mode and clears `WS_EX_APPWINDOW`. |
| `app/engine.py` (window parts) | `overlay.rs`, `monitors.rs`, `input.rs` | ported | One winit window over the virtual desktop (as the Python). `nativeEvent` → window subclass (`WM_NCHITTEST` → closure, `WM_HOTKEY` → `OverlayEvent::Hotkey`, `WM_MOUSEACTIVATE` → `MA_NOACTIVATE`). `_update_input_transparency` → 50 ms Win32 timer in the window procedure. Style refresh 1000 ms, geometry check 1500 ms (+ `WM_DISPLAYCHANGE`). Styles are also enforced in `WM_STYLECHANGING` so winit cannot strip them. Release hotkey (`RELEASE_HOTKEY_ID` 0xD0C1, `MOD_NOREPEAT`), `virtual_key_for`, `left_mouse_button_down`, cursor position. The rest of engine.py (tick, tray actions, missions, painting) belongs to dc-app. |
| (`QOpenGLWidget` surface, `configure_gl_surface`) | `gpu.rs` | new | DX12 + `Dx12SwapchainKind::DxgiFromVisual` (DirectComposition), `PreMultiplied` alpha. See findings below. |
| `support/frame_policy.py` | `frame_policy.rs` | ported | Probes are injectable closures. |
| `support/dpi.py` | `dpi.rs` | ported | `enable_high_dpi_scaling` = `SetProcessDpiAwarenessContext(PMv2)`; ratio lookup via `GetDpiForMonitor`. |
| `support/logging_setup.py` | `logging_setup.rs` | ported | `log` backend; same path, format, 512 KiB × 3 rotation (Python 3.13 rules, incl. "never roll an empty file"). Targets map to `desktop_bug.<name>`; foreign crates filtered to warnings. `install_excepthook` → `install_panic_hook` (panics still unwind). |
| `world/desktop_environment.py` | `desktop_environment.rs` | ported | EnumWindows snapshot with cloaked/DWM frame/layered-alpha filters. Desktop icons (SysListView32 + `VirtualAllocEx`/`ReadProcessMemory`) ported but **off by default** (`SnapshotOptions::default()`). Surfaces carry their `hwnd` in addition. |
| `app/desktop_capture.py` | `desktop_capture.rs` | ported (partial art) | GDI `BitBlt` per monitor (`CAPTUREBLT`), window list, `PrintWindow(PW_RENDERFULLCONTENT)` pictures (≤12, hung/blank skipped), wallpaper **path**. Images are `RgbaImage` (RGBA8). Missing: wallpaper decoding (no JPEG decoder dependency; `apply_wallpaper` takes a decoded image, nearest-neighbour "Fill"); `synthetic_snapshot` fills by pixel centre without antialiasing. |
| `app/screen_activity.py` | `screen_activity.rs` | ported | `playable_screens` works on monitor names/indices instead of `QScreen`s. DDC/CI is not exercised by tests (slow, hardware dependent). |
| `app/live_channel.py` | `live_channel.rs` | ported | Named pipe `\\.\pipe\DesktopBugCompanion-<16 hex>` with overlapped I/O, reader + writer thread per peer, NDJSON, `v` = 1, five message types. Name uses **SHA-256** (sha1 is not a dependency), so a Rust peer never pairs with a Python peer (both fall back to files). First instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE` (a second overlay on the same state dir cannot bind). Keys sorted recursively, compact separators. Events are polled (`poll()`) or signalled through a waker instead of Qt signals. |
| `app/session_control.py` | `session_control.rs` | ported | Plus `MtimeWatcher` for the preset (`PRESET_WATCH_MS` 700) and controls mtime polling. `stop_process` over a `StoppableProcess` trait (implemented for `std::process::Child`). |
| tray (`create_tray`, `showMessage`) | `tray.rs` | ported (wrapper) | Generic model: items, checks, exclusive groups, notes, separators, submenus; menu/icon event channel + optional waker. Balloon via `Shell_NotifyIconW(NIM_MODIFY, NIF_INFO)` on tray-icon's own icon (GUID, or probing its small ids). Missing: item tooltips/icons (native menu has none). Balloon not exercised by tests (it would pop a real notification). |
| `wood_theme.py` app id | `input.rs` | ported | `set_app_user_model_id("DesktopBugCompanion.App")`. |

## Transparency findings (Windows 11 26300, AMD RX 9070 XT, wgpu 30)

- wgpu's DX12 HWND swapchain (`DxgiFromHwnd`, the default) reports only
  `CompositeAlphaMode::Opaque` (wgpu-hal `dx12/adapter.rs`): no per-pixel alpha.
- `DxgiFromVisual` makes wgpu create a DirectComposition target/visual for the
  HWND; the surface then offers `[Auto, Inherit, Opaque, PostMultiplied,
  PreMultiplied]`. With the window created with `WS_EX_NOREDIRECTIONBITMAP`
  (winit `with_no_redirection_bitmap(true)`) and premultiplied output this
  composites correctly. Measured by `overlay_demo --verify`: screen pixel
  under the 60 % orange circle went from (117,117,117) to (192,101,62),
  expected ≈ (192,100,62); a pixel 370 px away stayed (51,51,51).
- `WS_EX_LAYERED` (needed so `WS_EX_TRANSPARENT` makes the window input
  transparent) does not hide the DirectComposition content; measured with and
  without `SetLayeredWindowAttributes(255, LWA_ALPHA)` — both show. The call is
  kept as a safeguard.
- Click-through: `WM_NCHITTEST` returns `HTCLIENT` only over the circle
  (checked with `SendMessage`); the 50 ms timer keeps `WS_EX_TRANSPARENT` set
  while the cursor is elsewhere, and `WindowFromPoint` away from the circle no
  longer returns the overlay.
- Software fallback `OverlayBackend::Layered` (`UpdateLayeredWindow` with a
  premultiplied DIB) passes the same `--verify` checks. It now presents
  through `UpdateLayeredWindowIndirect`; `present_premultiplied_rects`
  copies only the dirty rects into the DIB and passes their bounding rect
  as `prcDirty` (dc-app's partial repaints). Not yet run on screen.

## Tests

`cargo test -p dc-platform`: 51 unit tests + 1 integration test
(`tests/overlay_smoke.rs`: real winit window off-screen, styles, hit test,
`WM_HOTKEY` delivery, software present, tray menu groups). Includes a real
pipe server/client round trip (hello, 40 000-element preset, broadcast, two
clients, version mismatch disconnect, name exclusivity), frame-policy,
key mapping, DPI math, rotation, stop-request and mtime tests, and smoke
enumeration of monitors, windows and display devices.

## Not done

- No check on a mixed-DPI multi-monitor layout (only this machine's setup).
- DXGI desktop duplication (GDI `BitBlt` used; fine for a one-shot capture).
- Balloon notification and DDC/CI only compile-checked.
