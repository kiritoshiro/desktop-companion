//! Smoke test of the real overlay window: create it on a winit loop, check
//! its styles, hit-testing and hotkey delivery, then close it. The mouse is
//! never moved. One test function, because winit allows one event loop per
//! process.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dc_platform::overlay::{OverlayBackend, OverlayConfig, OverlayEvent, OverlayMode, OverlayWindow, current_ex_style};
use dc_platform::overlay_win32::{WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT};
use dc_platform::tray::{Tray, TrayMenu};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PostMessageW, SendMessageW, WM_HOTKEY, WM_NCHITTEST};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

#[derive(Default)]
struct App {
    overlay: Option<OverlayWindow>,
    error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.overlay.is_none() {
            let mut cfg = OverlayConfig::new(OverlayMode::Companion, OverlayBackend::Layered);
            // A small fixed rectangle, so nothing covers the desktop.
            cfg.geometry = Some(dc_core::IRect::new(-32000 + 100, -32000 + 100, 64, 48));
            cfg.auto_input_transparency = false;
            match OverlayWindow::create(el, &cfg) {
                Ok(o) => self.overlay = Some(o),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

fn pump(event_loop: &mut EventLoop<()>, app: &mut App, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        let _ = event_loop.pump_app_events(Some(Duration::from_millis(5)), app);
    }
}

fn lparam(x: i32, y: i32) -> LPARAM {
    LPARAM(((x as u32 & 0xFFFF) | ((y as u32 & 0xFFFF) << 16)) as isize)
}

#[test]
fn overlay_window_styles_hit_test_hotkeys_and_tray() {
    let mut event_loop = EventLoop::builder().with_any_thread(true).build().expect("event loop");
    let mut app = App::default();
    pump(&mut event_loop, &mut app, 100);
    if let Some(e) = &app.error {
        panic!("overlay creation failed: {e}");
    }
    let overlay = app.overlay.as_ref().expect("no overlay created");
    let hwnd = overlay.hwnd();
    let g = overlay.geometry();

    // Styles: layered tool window, no caption, not activatable, and starting
    // out transparent to input in companion mode.
    let ex = current_ex_style(hwnd);
    assert_ne!(ex & WS_EX_LAYERED, 0, "{ex:#x}");
    assert_ne!(ex & WS_EX_TOOLWINDOW, 0, "{ex:#x}");
    assert_ne!(ex & WS_EX_NOACTIVATE, 0, "{ex:#x}");
    assert_ne!(ex & WS_EX_TRANSPARENT, 0, "{ex:#x}");
    // winit keeps WS_CAPTION on undecorated windows and removes the frame in
    // WM_NCCALCSIZE, so check what matters: the client area is the window.
    let mut client = windows::Win32::Foundation::RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }.unwrap();
    assert_eq!((client.right, client.bottom), (g.w, g.h), "the overlay has a frame");

    // Toggling input transparency writes the bit, and winit rewriting the
    // styles cannot strip ours (WM_STYLECHANGING).
    assert!(overlay.set_input_transparent(false));
    assert_eq!(current_ex_style(hwnd) & WS_EX_TRANSPARENT, 0);
    dc_platform::overlay::force_ex_style(hwnd, 0);
    let ex = current_ex_style(hwnd);
    assert_ne!(ex & WS_EX_LAYERED, 0, "a style rewrite stripped WS_EX_LAYERED");
    assert_eq!(ex & WS_EX_TRANSPARENT, 0);
    assert!(overlay.set_input_transparent(true));
    assert_ne!(current_ex_style(hwnd) & WS_EX_TRANSPARENT, 0);

    // The hit-test hook gets overlay-local points.
    let seen = Rc::new(Cell::new((f64::NAN, f64::NAN)));
    let s = seen.clone();
    overlay.set_hit_test(move |x, y| {
        s.set((x, y));
        x < 10.0
    });
    let ht = unsafe { SendMessageW(hwnd, WM_NCHITTEST, Some(WPARAM(0)), Some(lparam(g.x + 5, g.y + 7))) }.0;
    assert_eq!(ht, 1, "HTCLIENT over something interactive");
    assert_eq!(seen.get(), (5.0, 7.0));
    let ht = unsafe { SendMessageW(hwnd, WM_NCHITTEST, Some(WPARAM(0)), Some(lparam(g.x + 30, g.y + 7))) }.0;
    assert_eq!(ht, -1, "HTTRANSPARENT elsewhere");

    // Hotkeys: register a combination nobody uses, then deliver WM_HOTKEY
    // the way Windows does (posted to the window).
    let registered = overlay.register_hotkey(0xD0C9, 0x0001 | 0x0002 | 0x0004, 0x87); // Alt+Ctrl+Shift+F24
    unsafe {
        PostMessageW(Some(hwnd), WM_HOTKEY, WPARAM(0xD0C9), LPARAM(0)).unwrap();
    }
    pump(&mut event_loop, &mut app, 60);
    let overlay = app.overlay.as_ref().unwrap();
    let events = overlay.drain_events();
    assert!(events.contains(&OverlayEvent::Hotkey(0xD0C9)), "{events:?} (registered: {registered})");
    overlay.unregister_hotkey(0xD0C9);
    // Companion mode never registers the release key.
    assert!(!overlay.register_release_hotkey("F8"));

    // Software present of a fully transparent frame succeeds.
    let frame = vec![0u8; g.w as usize * g.h as usize * 4];
    assert!(overlay.present_premultiplied(&frame, g.w as u32, g.h as u32));

    // The tray: a menu with an exclusive group.
    let model = TrayMenu::new()
        .note("status", "Running")
        .submenu(
            "performance",
            "Performance",
            TrayMenu::new().radio("fps60", "60 FPS", true, "fps").radio("fps30", "30 FPS", false, "fps"),
        )
        .separator()
        .item("quit", "Quit overlay");
    let icon: Vec<u8> = (0..16 * 16).flat_map(|_| [120u8, 80, 40, 255]).collect();
    let tray = Tray::new("Desktop Bug Companion test", icon, (16, 16), &model, None).expect("tray");
    tray.set_checked("fps30", true);
    assert_eq!(tray.is_checked("fps30"), Some(true));
    assert_eq!(tray.is_checked("fps60"), Some(false), "an exclusive sibling stayed checked");
    tray.set_text("status", "Running - dragging is on.");
    assert!(tray.poll_events().is_empty());
    drop(tray);

    app.overlay = None;
    pump(&mut event_loop, &mut app, 30);
}
