//! The Companion overlay (`OverlayWindow` in `app/engine.py`): one
//! transparent window over the virtual desktop, the manager ticked at the
//! target frame rate, the frame painted on the GPU (or through
//! `UpdateLayeredWindow`), click-through everywhere but over what the
//! manager wants the mouse for, and a tray icon.
//!
//! Not ported yet: Adventure missions and keys, the right-click menus and
//! dialogs, the live channel to the settings window, camouflage sampling,
//! partial repaints, and DPI scaling (everything is in physical pixels).

use crate::Args;
use crate::metrics::FontLabelMetrics;
use dc_core::PyRandom;
use dc_manager::paint::ScenePainter;
use dc_manager::{CreatureManager, DesktopSurface, MouseInput};
use dc_paint::PaintSwitches;
use dc_platform::desktop_environment::{SnapshotOptions, snapshot_desktop_surfaces};
use dc_platform::frame_policy::FramePolicy;
use dc_platform::gpu::OverlayGpu;
use dc_platform::input::left_mouse_button_down;
use dc_platform::monitors::{monitors, screen_rects_local, virtual_screen_geometry};
use dc_platform::overlay::{OverlayBackend, OverlayConfig, OverlayMode, OverlayWindow};
use dc_platform::tray::{Tray, TrayEvent, TrayMenu};
use dc_render::{Color, CpuRenderer, GpuRenderer, RenderHint, Scene};
use dc_world::playfield::ScreenRect;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const SCREEN_GEOMETRY_CHECK_MS: f64 = 1500.0;
const DESKTOP_SURFACE_CHECK_MS: f64 = 1250.0;
const PRESET_WATCH_MS: f64 = 700.0;

/// `_target_fps()`: `DESKTOP_BUG_FPS`, 10..60, default 60.
fn target_fps() -> f64 {
    std::env::var("DESKTOP_BUG_FPS").ok().and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(60.0).clamp(10.0, 60.0)
}

enum Drawer {
    Gpu { gpu: Box<OverlayGpu>, renderer: Box<GpuRenderer> },
    Soft { cpu: CpuRenderer },
}

struct Engine {
    args: Args,
    preset: PathBuf,
    state_dir: PathBuf,
    overlay: Option<OverlayWindow>,
    drawer: Option<Drawer>,
    manager: Option<Rc<RefCell<CreatureManager>>>,
    painter: ScenePainter,
    rng: PyRandom,
    tray: Option<Tray>,
    policy: FramePolicy,
    fps: f64,
    start: Instant,
    last_tick: Instant,
    next_tick: Instant,
    last_screen_check: f64,
    last_surface_check: f64,
    last_preset_check: f64,
    preset_mtime: Option<SystemTime>,
    geometry: dc_core::IRect,
    last_left_down: bool,
    frames: u64,
    error: Option<String>,
}

impl Engine {
    fn now_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn screen_rects(&self) -> Vec<ScreenRect> {
        let g = self.geometry;
        screen_rects_local((g.x, g.y), &monitors()).into_iter().map(|r| ScreenRect::new(r.x, r.y, r.w, r.h)).collect()
    }

    fn create_overlay(&mut self, el: &ActiveEventLoop, backend: OverlayBackend) -> Result<(), String> {
        let overlay = OverlayWindow::create(el, &OverlayConfig::new(OverlayMode::Companion, backend))
            .map_err(|e| e.to_string())?;
        let g = overlay.geometry();
        let drawer = match backend {
            OverlayBackend::Composition => {
                let gpu = OverlayGpu::new(overlay.window().clone(), (g.w as u32, g.h as u32))
                    .map_err(|e| format!("gpu: {e}"))?;
                let renderer = GpuRenderer::new(&gpu.device, &gpu.queue, gpu.format(), 4);
                log::info!("Painting on the GPU ({:?}, {:?})", gpu.adapter.get_info().name, gpu.config.format);
                Drawer::Gpu { gpu: Box::new(gpu), renderer: Box::new(renderer) }
            }
            OverlayBackend::Layered => {
                log::info!("Painting in software (UpdateLayeredWindow)");
                Drawer::Soft { cpu: CpuRenderer::new() }
            }
        };
        self.geometry = g;
        self.overlay = Some(overlay);
        self.drawer = Some(drawer);
        Ok(())
    }

    fn create_manager(&mut self) -> Result<(), String> {
        let g = self.geometry;
        let root = dc_content::discovery::app_root();
        let mut m = CreatureManager::new(
            &root,
            &self.preset,
            (g.w as f64, g.h as f64),
            self.args.seed,
            &self.state_dir,
            true,
            &mut self.rng,
        )
        .map_err(|e| format!("could not load {}: {e:?}", self.preset.display()))?;
        m.set_label_metrics(Box::new(FontLabelMetrics));
        m.set_screen_rects(&self.screen_rects());
        for warning in &m.warnings {
            log::warn!("{warning}");
        }
        let m = Rc::new(RefCell::new(m));
        // Click-through everywhere except where the manager wants the
        // mouse; a re-entrant call during a tick simply lets it through.
        let hit = m.clone();
        if let Some(o) = self.overlay.as_ref() {
            o.set_hit_test(move |x, y| hit.try_borrow().map(|m| m.wants_mouse(x, y)).unwrap_or(false));
        }
        self.manager = Some(m);
        self.preset_mtime = dc_platform::session_control::file_mtime(&self.preset);
        dc_platform::session_control::clear_stop_request(&self.state_dir);
        Ok(())
    }

    fn create_tray(&mut self) {
        let Some(icon) = dc_ui::assets::UiAssets::app_icon_rgba() else {
            log::warn!("No app icon found; running without a tray icon");
            return;
        };
        let menu = TrayMenu::new().item("quit", "Quit");
        match Tray::new("Desktop Bug Companion", icon.rgba, (icon.width, icon.height), &menu, None) {
            Ok(t) => self.tray = Some(t),
            Err(e) => log::warn!("Tray icon unavailable: {e}"),
        }
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        if let Some(m) = &self.manager
            && let Err(e) = m.borrow_mut().save_runtime_state()
        {
            log::warn!("Could not save on quit: {e}");
        }
        el.exit();
    }

    /// `OverlayWindow.tick`.
    fn tick(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64().clamp(0.001, 0.05);
        self.last_tick = now;
        let now_ms = self.now_ms();
        let own = self.overlay.as_ref().map(|o| o.hwnd_raw());
        if let Some(fps) = self.policy.poll(now_ms, own) {
            log::info!("frame rate now {fps:.0} FPS");
            self.fps = fps;
        }
        let Some(manager) = self.manager.clone() else { return };
        if now_ms - self.last_screen_check >= SCREEN_GEOMETRY_CHECK_MS {
            self.last_screen_check = now_ms;
            let rect = virtual_screen_geometry();
            if rect != self.geometry {
                if let Some(o) = self.overlay.as_ref() {
                    o.sync_geometry();
                }
                self.geometry = rect;
                let rects = self.screen_rects();
                let mut m = manager.borrow_mut();
                m.resize(rect.w as f64, rect.h as f64);
                m.set_screen_rects(&rects);
                if let Some(Drawer::Gpu { gpu, .. }) = self.drawer.as_mut() {
                    gpu.resize(rect.w as u32, rect.h as u32);
                }
            }
        }
        if now_ms - self.last_surface_check >= DESKTOP_SURFACE_CHECK_MS {
            self.last_surface_check = now_ms;
            let mut m = manager.borrow_mut();
            let opts = SnapshotOptions {
                exclude_hwnd: own,
                origin_x: self.geometry.x,
                origin_y: self.geometry.y,
                screen_w: Some(self.geometry.w),
                screen_h: Some(self.geometry.h),
                include_desktop_icons: m.desktop_icons_enabled,
            };
            let surfaces = snapshot_desktop_surfaces(&opts)
                .into_iter()
                .map(|s| DesktopSurface {
                    kind: s.kind.as_str().to_string(),
                    class_name: s.class_name,
                    x: s.x,
                    y: s.y,
                    w: s.w,
                    h: s.h,
                })
                .collect();
            m.set_desktop_surfaces(surfaces);
        }
        if now_ms - self.last_preset_check >= PRESET_WATCH_MS {
            self.last_preset_check = now_ms;
            if dc_platform::session_control::consume_stop_request(&self.state_dir) {
                log::info!("Stop requested by the settings window");
                self.quit(el);
                return;
            }
            let mtime = dc_platform::session_control::file_mtime(&self.preset);
            if mtime.is_some() && mtime != self.preset_mtime {
                self.preset_mtime = mtime;
                match std::fs::read_to_string(&self.preset).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                    Some(data) => {
                        let message = manager.borrow_mut().reload_from_preset_data(&data, &mut self.rng);
                        log::info!("{message}");
                    }
                    None => log::warn!("Preset changed but could not be read: {}", self.preset.display()),
                }
            }
        }
        let (mx, my) = self.overlay.as_ref().and_then(|o| o.cursor_local()).unwrap_or((-100000.0, -100000.0));
        let down = left_mouse_button_down();
        let input = MouseInput { down, pressed: down && !self.last_left_down, released: self.last_left_down && !down };
        self.last_left_down = down;
        let desired = {
            let mut m = manager.borrow_mut();
            let desired = m.update(dt, mx, my, input, &mut self.rng);
            if m.naming_enabled || m.always_show_names {
                m.update_hover(mx, my);
            } else {
                m.clear_hover();
            }
            desired
        };
        // A timed development run never takes the real pointer.
        if let (Some((x, y)), Some(o), None) = (desired, self.overlay.as_ref(), self.args.seconds) {
            o.set_cursor_local(x, y);
        }
        if let Some(path) = self.args.snapshot.clone()
            && now_ms >= 3000.0
        {
            self.args.snapshot = None;
            self.save_snapshot(&path);
        }
        if let Some(o) = self.overlay.as_ref() {
            o.window().request_redraw();
        }
    }

    /// The primary monitor as the compositor shows it, overlay included.
    fn save_snapshot(&self, path: &std::path::Path) {
        let list = monitors();
        let p = &list[dc_platform::monitors::primary_index(&list)];
        let Some(shot) = dc_platform::desktop_capture::grab_screen(p.rect.x, p.rect.y, p.rect.w, p.rect.h) else {
            log::warn!("Screen capture failed");
            return;
        };
        match dc_render::Image::from_straight_rgba(shot.width, shot.height, shot.pixels) {
            Some(image) => match image.save_png(path) {
                Ok(()) => log::info!("Snapshot saved to {}", path.display()),
                Err(e) => log::warn!("Snapshot not saved: {e:?}"),
            },
            None => log::warn!("Snapshot had the wrong size"),
        }
    }

    fn paint(&mut self) {
        let (Some(manager), Some(drawer)) = (self.manager.clone(), self.drawer.as_mut()) else { return };
        let mut scene = Scene::new();
        {
            let mut canvas = scene.canvas();
            canvas.set_render_hint(RenderHint::Antialiasing, true);
            self.painter.paint(&mut manager.borrow_mut(), &mut canvas, None);
        }
        let (w, h) = (self.geometry.w as u32, self.geometry.h as u32);
        match drawer {
            Drawer::Gpu { gpu, renderer } => {
                let Some(frame) = gpu.acquire() else { return };
                let view = frame.texture.create_view(&Default::default());
                renderer.render(&scene, &view, (w, h), Some(Color::TRANSPARENT));
                gpu.queue.present(frame);
            }
            Drawer::Soft { cpu } => {
                let image = cpu.render(&scene, w, h, None);
                if let Some(o) = self.overlay.as_ref() {
                    o.present_premultiplied(image.data(), w, h);
                }
            }
        }
        self.frames += 1;
    }
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }
        let first = if self.args.software { OverlayBackend::Layered } else { OverlayBackend::Composition };
        if let Err(e) = self.create_overlay(el, first) {
            log::warn!("{e}; falling back to the software overlay");
            self.overlay = None;
            self.drawer = None;
            if let Err(e) = self.create_overlay(el, OverlayBackend::Layered) {
                self.error = Some(e);
                el.exit();
                return;
            }
        }
        if let Err(e) = self.create_manager() {
            self.error = Some(e);
            el.exit();
            return;
        }
        self.create_tray();
        self.start = Instant::now();
        self.last_tick = self.start;
        self.next_tick = self.start;
        log::info!("Overlay ready: preset={} seed={:?}", self.preset.display(), self.args.seed);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::Resized(size) => {
                if let Some(Drawer::Gpu { gpu, .. }) = self.drawer.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::CloseRequested => self.quit(el),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.manager.is_none() {
            return;
        }
        if let Some(limit) = self.args.seconds
            && self.start.elapsed().as_secs_f64() >= limit
        {
            self.quit(el);
            return;
        }
        let quit = self
            .tray
            .as_ref()
            .map(|t| t.poll_events().into_iter().any(|e| matches!(e, TrayEvent::Menu { ref id, .. } if id == "quit")))
            .unwrap_or(false);
        if quit {
            self.quit(el);
            return;
        }
        if let Some(o) = self.overlay.as_ref() {
            for e in o.drain_events() {
                log::debug!("overlay event: {e:?}");
            }
        }
        let now = Instant::now();
        if now >= self.next_tick {
            let interval = Duration::from_secs_f64(1.0 / self.fps.max(1.0));
            self.next_tick = (self.next_tick + interval).max(now);
            self.tick(el);
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }
}

pub fn run(args: Args, preset: PathBuf, state_dir: PathBuf) -> Result<(), String> {
    let fps = target_fps();
    let rng = match args.seed {
        Some(seed) => PyRandom::from_int(seed as i128),
        None => PyRandom::from_entropy(),
    };
    let mut engine = Engine {
        args,
        preset,
        state_dir,
        overlay: None,
        drawer: None,
        manager: None,
        painter: ScenePainter::new(PaintSwitches::from_env()),
        rng,
        tray: None,
        policy: FramePolicy::new(fps),
        fps,
        start: Instant::now(),
        last_tick: Instant::now(),
        next_tick: Instant::now(),
        last_screen_check: 0.0,
        last_surface_check: f64::NEG_INFINITY,
        last_preset_check: 0.0,
        preset_mtime: None,
        geometry: virtual_screen_geometry(),
        last_left_down: false,
        frames: 0,
        error: None,
    };
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop.run_app(&mut engine).map_err(|e| format!("event loop failed: {e}"))?;
    let secs = engine.start.elapsed().as_secs_f64();
    log::info!(
        "Overlay stopped after {} frames in {secs:.1} s ({:.1} fps)",
        engine.frames,
        engine.frames as f64 / secs.max(0.001)
    );
    // GPU objects before the window, then the window.
    engine.drawer = None;
    engine.overlay = None;
    match engine.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
