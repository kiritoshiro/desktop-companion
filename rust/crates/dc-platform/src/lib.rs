//! Win32 shell of Desktop Bug Companion: the transparent click-through
//! overlay window, its wgpu surface, hotkeys, tray icon, frame policy, DPI,
//! logging, desktop snapshots and capture, screen activity, and the
//! single-instance live channel. Windows only.
//!
//! How the engine uses it:
//! 1. `logging_setup::configure_logging(state_dir, level)`,
//!    `input::set_app_user_model_id(input::APP_USER_MODEL_ID)`.
//! 2. In `resumed`: `overlay::OverlayWindow::create(event_loop, &config)`,
//!    then `gpu::OverlayGpu::new(overlay.window().clone(), size)`; on
//!    `GpuError::NoTransparency` recreate with `OverlayBackend::Layered` and
//!    present software frames with `present_premultiplied`.
//! 3. `overlay.set_hit_test(|x, y| manager.wants_mouse(x, y))`; the window
//!    answers `WM_NCHITTEST` and toggles `WS_EX_TRANSPARENT` on its own.
//! 4. Each frame: `overlay.drain_events()` (hotkeys, geometry),
//!    `server.poll()` (live channel), `tray.poll_events()`,
//!    `frame_policy.poll(now, Some(overlay.hwnd_raw()))`.

// Pixel loops read better as `chunks_exact(4)` than as `as_chunks::<4>()`.
#![allow(clippy::chunks_exact_to_as_chunks)]

pub mod desktop_capture;
pub mod desktop_environment;
pub mod dpi;
pub mod frame_policy;
pub mod gpu;
pub mod input;
pub mod live_channel;
pub mod logging_setup;
pub mod monitors;
pub mod overlay;
pub mod overlay_win32;
pub mod screen_activity;
pub mod session_control;
pub mod tray;
pub mod win;

pub use desktop_capture::{DesktopSnapshot, RgbaImage, ScreenShot};
pub use desktop_environment::{DesktopSurface, SnapshotOptions, SurfaceKind};
pub use frame_policy::FramePolicy;
pub use gpu::{GpuError, OverlayGpu};
pub use live_channel::{ChannelMessage, ClientEvent, OverlayChannelServer, ServerEvent, SettingsChannelClient};
pub use monitors::MonitorInfo;
pub use overlay::{OverlayBackend, OverlayConfig, OverlayError, OverlayEvent, OverlayMode, OverlayWindow};
pub use tray::{MenuEntry, MessageIcon, Tray, TrayEvent, TrayMenu};
