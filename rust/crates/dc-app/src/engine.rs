//! The overlay process (`OverlayWindow` in `app/engine.py`): one
//! transparent window over the virtual desktop, ticked at the target frame
//! rate, the frame painted on the GPU (or through `UpdateLayeredWindow`),
//! the menus and dialogs of [`OverlayUi`], and a tray icon.
//!
//! - **Companion** (`--mode companion`): the manager's colony,
//!   click-through everywhere but over what the manager wants the mouse for;
//!   right-click asks `OverlayUi` for the spider or overlay menu.
//! - **Adventure** (`--mode adventure`): the mission chosen in the adventure
//!   profile, run by [`AdventureGame`] (see `adventure/`). The window is not
//!   topmost, takes the keyboard, captures the mouse while playing, and
//!   registers the release key (F8) as a system-wide hotkey.
//!
//! Menus, dialogs and the tray menu come from `overlay_ui`; the engine
//! wires them as `OVERLAY_UI.md` lists: it syncs the UI with the scene's
//! manager every tick before `frame`, applies every [`EngineCommand`] in
//! [`Engine::apply_command`], and keeps the tray's check marks current.
//!
//! The live channel to the settings window is [`LiveChannel`]; the preset
//! file and the stop-request file stay polled as its fallbacks.
//!
//! Not ported yet: camouflage sampling, partial repaints, DPI scaling
//! (everything is in physical pixels).

use crate::adventure::draw::AdventureDrawer;
use crate::adventure::screens::{self, PlayableScreens};
use crate::adventure::{AdventureGame, Effect, StartOptions, input};
use crate::live::LiveChannel;
use crate::metrics::FontLabelMetrics;
use crate::overlay_ui::{
    AdventureContext, EngineCommand, OverlayUi, UiRequest, apply_overlay_action, apply_spider_action, initial_status,
    tray_checks, tray_event_checked, tray_menu, tray_menu_for,
};
use crate::{Args, Mode};
use dc_core::PyRandom;
use dc_manager::paint::ScenePainter;
use dc_manager::{CreatureManager, DesktopSurface, MouseInput};
use dc_paint::PaintSwitches;
use dc_platform::desktop_environment::{SnapshotOptions, snapshot_desktop_surfaces};
use dc_platform::frame_policy::FramePolicy;
use dc_platform::gpu::OverlayGpu;
use dc_platform::input::left_mouse_button_down;
use dc_platform::monitors::{monitors, screen_rects_local, virtual_screen_geometry};
use dc_platform::overlay::{
    OverlayBackend, OverlayConfig, OverlayEvent, OverlayMode, OverlayWindow, RELEASE_HOTKEY_ID,
};
use dc_platform::tray::{MessageIcon, Tray, TrayEvent};
use dc_render::{Color, CpuRenderer, GpuRenderer, RenderHint, Scene};
use dc_world::playfield::ScreenRect;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const SCREEN_GEOMETRY_CHECK_MS: f64 = 1500.0;
const DESKTOP_SURFACE_CHECK_MS: f64 = 1250.0;
const PRESET_WATCH_MS: f64 = 700.0;
/// `_announce`'s tray balloon.
const ANNOUNCE_MS: u32 = 1400;
/// How often the UI layer runs while a menu or dialog is open, whatever the
/// overlay's frame rate (menus poll for outside clicks in `frame`).
const UI_OPEN_FRAME: Duration = Duration::from_millis(33);

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
    /// Companion: the colony (shared with the hit test).
    manager: Option<Rc<RefCell<CreatureManager>>>,
    /// Adventure: the raid (shared with the hit test).
    game: Option<Rc<RefCell<AdventureGame>>>,
    painter: ScenePainter,
    adventure_drawer: AdventureDrawer,
    ui: OverlayUi,
    rng: PyRandom,
    tray: Option<Tray>,
    /// The settings window's live channel (`_channel_server`).
    channel: Option<LiveChannel>,
    policy: FramePolicy,
    /// The frame rate running now (the policy may run below the target).
    fps: f64,
    /// The frame rate the user chose (tray Performance, Adventure settings).
    target_fps: f64,
    /// A base is carried ("Move the … base…"): the overlay takes the mouse
    /// so the right-click that puts it down reaches the menu.
    carrying: Rc<Cell<bool>>,
    next_ui_frame: Instant,
    exiting: bool,
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

    fn adventure(&self) -> bool {
        self.args.mode == Mode::Adventure
    }

    fn ready(&self) -> bool {
        self.manager.is_some() || self.game.is_some()
    }

    fn screen_rects(&self) -> Vec<ScreenRect> {
        let g = self.geometry;
        screen_rects_local((g.x, g.y), &monitors()).into_iter().map(|r| ScreenRect::new(r.x, r.y, r.w, r.h)).collect()
    }

    fn create_overlay(&mut self, el: &ActiveEventLoop, backend: OverlayBackend) -> Result<(), String> {
        let mode = if self.adventure() { OverlayMode::Adventure } else { OverlayMode::Companion };
        let overlay = OverlayWindow::create(el, &OverlayConfig::new(mode, backend)).map_err(|e| e.to_string())?;
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
        self.ui.set_overlay_origin((g.x, g.y));
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
        // mouse, or anywhere while a base is carried; a re-entrant call
        // during a tick simply lets it through.
        let hit = m.clone();
        let carrying = self.carrying.clone();
        if let Some(o) = self.overlay.as_ref() {
            o.set_hit_test(move |x, y| {
                carrying.get() || hit.try_borrow().map(|m| m.wants_mouse(x, y)).unwrap_or(false)
            });
        }
        self.manager = Some(m);
        self.preset_mtime = dc_platform::session_control::file_mtime(&self.preset);
        dc_platform::session_control::clear_stop_request(&self.state_dir);
        Ok(())
    }

    /// Adventure's `__init__`: the playable screens, the Reclaim picture
    /// (taken now, before the overlay shows), the manager and the mission.
    fn create_game(&mut self) -> Result<(), String> {
        let g = self.geometry;
        let origin = (g.x, g.y);
        let PlayableScreens { rects, primary, primary_rect, monitors } = screens::playable(origin, &self.state_dir);
        let root = dc_content::discovery::app_root();
        let mut capture = || screens::capture(origin, &monitors);
        let game = AdventureGame::start(
            StartOptions {
                root: &root,
                preset: &self.preset,
                state_dir: &self.state_dir,
                size: (g.w as f64, g.h as f64),
                rects,
                primary,
                primary_rect,
                seed: self.args.seed,
                label_metrics: Some(Box::new(FontLabelMetrics)),
            },
            Some(&mut capture),
        )?;
        self.game = Some(Rc::new(RefCell::new(game)));
        dc_platform::session_control::clear_stop_request(&self.state_dir);
        Ok(())
    }

    /// `_adventure_captures_mouse` as the window's hit test.
    fn install_adventure_hit_test(&self) {
        let (Some(game), Some(o)) = (self.game.clone(), self.overlay.as_ref()) else { return };
        let last = Cell::new(true);
        o.set_hit_test(move |x, y| match game.try_borrow() {
            Ok(g) => {
                let answer = g.captures_mouse(x, y);
                last.set(answer);
                answer
            }
            Err(_) => last.get(),
        });
    }

    /// The scene's manager: the colony in Companion, the raid's in
    /// Adventure, with the stream its random draws come from.
    fn with_manager<R>(&mut self, f: impl FnOnce(&mut CreatureManager, &mut PyRandom) -> R) -> Option<R> {
        if let Some(game) = &self.game {
            let mut g = game.borrow_mut();
            let g = &mut *g;
            return Some(f(&mut g.manager, &mut g.rng));
        }
        let manager = self.manager.as_ref()?;
        Some(f(&mut manager.borrow_mut(), &mut self.rng))
    }

    /// `create_tray`: Python's whole tray menu (`overlay_ui::tray_menu_for`)
    /// with every check mark as the scene is now, and the status line.
    fn create_tray(&mut self) {
        let Some(icon) = dc_ui::assets::UiAssets::app_icon_rgba() else {
            log::warn!("No app icon found; running without a tray icon");
            return;
        };
        let fps = self.target_fps;
        let built = self.with_manager(|m, _| (tray_menu_for(m, fps), initial_status(m)));
        let (menu, status) = built.unwrap_or_else(|| (tray_menu(), ""));
        match Tray::new("Desktop Bug Companion", icon.rgba, (icon.width, icon.height), &menu, None) {
            Ok(t) => {
                if !status.is_empty() {
                    t.set_text("status", status);
                }
                self.tray = Some(t);
            }
            Err(e) => log::warn!("Tray icon unavailable: {e}"),
        }
    }

    /// Push the scene's switches into the tray's check marks (the right-click
    /// menu changes the same settings).
    fn refresh_tray_checks(&mut self) {
        if self.tray.is_none() {
            return;
        }
        let fps = self.target_fps;
        let checks = self.with_manager(|m, _| tray_checks(m, fps)).unwrap_or_default();
        if let Some(tray) = self.tray.as_ref() {
            for (id, checked) in checks {
                tray.set_checked(&id, checked);
            }
        }
    }

    /// Tray menu clicks, as `overlay_ui::tray_event_checked` reads them.
    fn poll_tray(&mut self, el: &ActiveEventLoop) {
        let events = self.tray.as_ref().map(|t| t.poll_events()).unwrap_or_default();
        for event in events {
            if let TrayEvent::Menu { id, checked } = event {
                for command in tray_event_checked(&id, checked) {
                    self.apply_command(el, command);
                }
                self.refresh_tray_checks();
            }
        }
        self.apply_effects(el);
    }

    /// `_announce`: a short tray balloon, and the tray menu's status line.
    fn announce(&mut self, message: &str) {
        log::info!("{message}");
        if let Some(tray) = self.tray.as_mut() {
            tray.set_text("status", message);
            tray.show_message("Desktop Bug Companion", message, MessageIcon::Information, ANNOUNCE_MS);
        }
        self.broadcast_session_state();
    }

    /// `_broadcast_session_state`.
    fn broadcast_session_state(&mut self) {
        let Some(channel) = self.channel.take() else { return };
        self.with_manager(|m, _| channel.broadcast(m));
        self.channel = Some(channel);
    }

    /// `_on_channel_message`: what the settings window pushed since the last
    /// pass. False when it asked the overlay to stop.
    fn service_channel(&mut self, el: &ActiveEventLoop) -> bool {
        let Some(mut channel) = self.channel.take() else { return true };
        let done = self.with_manager(|m, rng| channel.service(m, rng)).unwrap_or_default();
        self.channel = Some(channel);
        if done.preset_applied {
            // Keep the file poll from applying the same content again.
            self.preset_mtime = dc_platform::session_control::file_mtime(&self.preset);
        }
        if done.stop {
            log::info!("Stop requested over the live channel");
            self.quit(el);
            return false;
        }
        true
    }

    /// `set_target_fps`: the ceiling the frame policy works from (10..60).
    fn set_target_fps(&mut self, fps: f64) {
        let target = fps.clamp(10.0, 60.0);
        self.target_fps = target;
        self.fps = self.policy.set_target_fps(target);
        log::info!("target frame rate {target:.0} FPS (running at {:.0})", self.fps);
        self.update_adventure_context();
        self.refresh_tray_checks();
    }

    /// What the pause menu and the party windows need to know.
    fn update_adventure_context(&mut self) {
        let Some(game) = &self.game else { return };
        let players = game.borrow().player_count();
        self.ui.set_adventure(AdventureContext {
            in_mission: true,
            players,
            target_fps: self.target_fps,
            // Only outside a raid, which the Rust engine never runs.
            player_creatures: Vec::new(),
        });
    }

    /// Save and leave: the raid's progress in Adventure (never the Companion
    /// colony file), the colony in Companion.
    fn quit(&mut self, el: &ActiveEventLoop) {
        if self.exiting {
            return;
        }
        self.exiting = true;
        if let Some(game) = &self.game {
            game.borrow_mut().save();
        } else if let Some(m) = &self.manager
            && let Err(e) = m.borrow_mut().save_runtime_state()
        {
            log::warn!("Could not save on quit: {e}");
        }
        if let Some(channel) = self.channel.as_mut() {
            channel.close();
        }
        el.exit();
    }

    /// Carry out what the Adventure game asked for.
    fn apply_effects(&mut self, el: &ActiveEventLoop) {
        let Some(game) = self.game.clone() else { return };
        let effects = game.borrow_mut().take_effects();
        for effect in effects {
            match effect {
                Effect::Ui(request) => self.request_ui(el, request),
                Effect::Announce(text) => self.announce(&text),
                Effect::SetTargetFps(fps) => self.set_target_fps(fps),
                Effect::Activate => {
                    if let Some(o) = self.overlay.as_ref() {
                        o.apply_click_through();
                        o.update_input_transparency();
                        o.activate();
                    }
                }
                Effect::Deactivate => {
                    if let Some(o) = self.overlay.as_ref() {
                        o.set_input_transparent(true);
                        o.apply_click_through();
                    }
                }
                Effect::RegisterReleaseHotkey(binding) => {
                    let ok = self.overlay.as_ref().is_some_and(|o| o.register_release_hotkey(&binding));
                    game.borrow_mut().release_hotkey = ok;
                }
                Effect::Stop => {
                    log::info!("Adventure over; closing the overlay");
                    self.exiting = true;
                    el.exit();
                }
            }
        }
    }

    /// One command from the menus, dialogs or tray (`OVERLAY_UI.md`). What
    /// the spider and overlay actions return (announcements, a new frame
    /// rate, Quit) is applied the same way.
    fn apply_command(&mut self, el: &ActiveEventLoop, command: EngineCommand) {
        if self.exiting {
            return;
        }
        match command {
            EngineCommand::Quit => self.quit(el),
            EngineCommand::Announce(text) => self.announce(&text),
            EngineCommand::SetTargetFps(fps) => self.set_target_fps(fps),
            EngineCommand::SpiderAction { creature, action, arg } => {
                let more = self
                    .with_manager(|m, _| apply_spider_action(m, &creature, &action, arg.as_deref()))
                    .unwrap_or_default();
                for c in more {
                    self.apply_command(el, c);
                }
                self.refresh_tray_checks();
            }
            EngineCommand::OverlayAction { action } => {
                let more = self.with_manager(|m, rng| apply_overlay_action(m, rng, &action)).unwrap_or_default();
                for c in more {
                    self.apply_command(el, c);
                }
                self.refresh_tray_checks();
            }
            // Outside a raid: the colony file.
            EngineCommand::SaveProgress if self.game.is_none() => {
                if let Some(m) = &self.manager
                    && let Err(e) = m.borrow_mut().save_runtime_state()
                {
                    log::warn!("Could not save: {e}");
                }
            }
            other => match &self.game {
                Some(game) => {
                    game.borrow_mut().apply(&other);
                }
                None => log::debug!("{other:?} means nothing in Companion mode"),
            },
        }
    }

    /// Feed the UI the scene (`sync`) before it runs.
    fn sync_ui(&mut self) {
        if let Some(game) = &self.game {
            self.ui.sync(&game.borrow().manager);
        } else if let Some(manager) = &self.manager {
            self.ui.sync(&manager.borrow());
        }
    }

    /// Ask the UI for something, synced at once so a menu shows this frame.
    fn request_ui(&mut self, el: &ActiveEventLoop, request: UiRequest) {
        self.ui.request(el, request);
        self.sync_ui();
        self.ui.frame(el);
    }

    /// Run the UI layer for this frame and apply what it asked for, before
    /// the next `frame` (a character window waits for its `SaveProgress`).
    fn run_ui(&mut self, el: &ActiveEventLoop) {
        self.sync_ui();
        self.ui.frame(el);
        for command in self.ui.take_commands() {
            self.apply_command(el, command);
        }
        self.apply_effects(el);
        self.carrying.set(self.ui.carried_base().is_some());
        self.next_ui_frame = Instant::now() + UI_OPEN_FRAME;
    }

    /// Right-click in Companion: the spider's menu over a spider, else the
    /// overlay menu. `at` is a desktop position.
    fn companion_right_click(&mut self, el: &ActiveEventLoop) {
        let (Some(manager), Some(o)) = (self.manager.as_ref(), self.overlay.as_ref()) else { return };
        let Some((x, y)) = o.cursor_local() else { return };
        let g = self.geometry;
        let at = (x + g.x as f64, y + g.y as f64);
        let request = {
            let m = manager.borrow();
            match m.creature_at(x, y).and_then(|id| m.creature(id)) {
                Some(c) => UiRequest::SpiderMenu { creature: c.progression_id.clone(), at },
                None => UiRequest::OverlayMenu { at },
            }
        };
        self.request_ui(el, request);
    }

    /// Monitors added, removed or resized.
    fn check_screens(&mut self, now_ms: f64) {
        if now_ms - self.last_screen_check < SCREEN_GEOMETRY_CHECK_MS {
            return;
        }
        self.last_screen_check = now_ms;
        let rect = virtual_screen_geometry();
        if rect == self.geometry {
            return;
        }
        if let Some(o) = self.overlay.as_ref() {
            o.sync_geometry();
        }
        self.geometry = rect;
        self.ui.set_overlay_origin((rect.x, rect.y));
        let rects = self.screen_rects();
        if let Some(m) = &self.manager {
            let mut m = m.borrow_mut();
            m.resize(rect.w as f64, rect.h as f64);
            m.set_screen_rects(&rects);
        }
        if let Some(game) = &self.game {
            let primary = screens::primary_rect_local((rect.x, rect.y), &monitors());
            game.borrow_mut().resize((rect.w as f64, rect.h as f64), &rects, primary);
        }
        if let Some(Drawer::Gpu { gpu, .. }) = self.drawer.as_mut() {
            gpu.resize(rect.w as u32, rect.h as u32);
        }
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
        self.check_screens(now_ms);
        if self.game.is_some() {
            if !self.tick_adventure(el, dt, now_ms) {
                return;
            }
        } else if !self.tick_companion(el, dt, now_ms, own) {
            return;
        }
        self.run_ui(el);
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

    /// The Adventure frame. False when the overlay is closing.
    fn tick_adventure(&mut self, el: &ActiveEventLoop, dt: f64, now_ms: f64) -> bool {
        let Some(game) = self.game.clone() else { return false };
        if now_ms - self.last_preset_check >= PRESET_WATCH_MS {
            self.last_preset_check = now_ms;
            if dc_platform::session_control::consume_stop_request(&self.state_dir) {
                log::info!("Stop requested by the settings window");
                self.quit(el);
                return false;
            }
            // Adventure ignores preset edits; the controls file is watched.
            game.borrow_mut().reload_controls_if_changed(false);
        }
        let cursor = self.overlay.as_ref().and_then(|o| o.cursor_local());
        {
            let mut g = game.borrow_mut();
            if let Some(c) = cursor {
                g.cursor = c;
            }
            g.set_ui_open(self.ui.is_open(), dt);
            g.tick(dt);
        }
        self.update_adventure_context();
        self.apply_effects(el);
        !game.borrow().is_stopping()
    }

    /// The Companion frame. False when the overlay is closing.
    fn tick_companion(&mut self, el: &ActiveEventLoop, dt: f64, now_ms: f64, own: Option<isize>) -> bool {
        let Some(manager) = self.manager.clone() else { return false };
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
                return false;
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
        // A menu or dialog has the mouse while it is open.
        let down = left_mouse_button_down() && !self.ui.is_open();
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
        true
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
        if !self.ready() || self.drawer.is_none() {
            return;
        }
        let mut scene = Scene::new();
        {
            let mut canvas = scene.canvas();
            canvas.set_render_hint(RenderHint::Antialiasing, true);
            if let Some(game) = &self.game {
                self.adventure_drawer.paint(&mut game.borrow_mut(), &mut canvas);
            } else if let Some(manager) = &self.manager {
                self.painter.paint(&mut manager.borrow_mut(), &mut canvas, None);
            }
        }
        let (w, h) = (self.geometry.w as u32, self.geometry.h as u32);
        match self.drawer.as_mut() {
            Some(Drawer::Gpu { gpu, renderer }) => {
                let Some(frame) = gpu.acquire() else { return };
                let view = frame.texture.create_view(&Default::default());
                renderer.render(&scene, &view, (w, h), Some(Color::TRANSPARENT));
                gpu.queue.present(frame);
            }
            Some(Drawer::Soft { cpu }) => {
                let image = cpu.render(&scene, w, h, None);
                if let Some(o) = self.overlay.as_ref() {
                    o.present_premultiplied(image.data(), w, h);
                }
            }
            None => return,
        }
        self.frames += 1;
    }

    /// Keys, mouse and focus for the Adventure game.
    fn adventure_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) {
        let Some(game) = self.game.clone() else { return };
        {
            let mut g = game.borrow_mut();
            g.set_ui_open(self.ui.is_open(), 0.0);
            match event {
                WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => {
                    if let Some(k) = input::key_input(event) {
                        g.key(k);
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if let Some(b) = input::mouse_button(*button) {
                        match state {
                            ElementState::Pressed => {
                                let (x, y) = g.cursor;
                                g.mouse_press(b, x, y);
                            }
                            ElementState::Released => g.mouse_release(b),
                        }
                    }
                }
                WindowEvent::CursorMoved { position, .. } => g.cursor_moved(position.x, position.y),
                WindowEvent::Focused(false) => g.focus_lost(),
                _ => {}
            }
        }
        self.apply_effects(el);
    }
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }
        if self.adventure() {
            // Before the window shows, so the Reclaim picture is of the
            // desktop alone.
            if let Err(e) = self.create_game() {
                self.error = Some(e);
                el.exit();
                return;
            }
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
        if self.adventure() {
            self.install_adventure_hit_test();
        } else if let Err(e) = self.create_manager() {
            self.error = Some(e);
            el.exit();
            return;
        }
        self.create_tray();
        self.channel = Some(LiveChannel::open(&self.state_dir, &self.preset, self.adventure()));
        self.start = Instant::now();
        self.last_tick = self.start;
        self.next_tick = self.start;
        self.apply_effects(el);
        log::info!(
            "Overlay ready: mode={:?} preset={} seed={:?}",
            self.args.mode,
            self.preset.display(),
            self.args.seed
        );
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.ui.owns_window(id) {
            self.ui.window_event(el, id, &event);
            return;
        }
        match event {
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::Resized(size) => {
                if let Some(Drawer::Gpu { gpu, .. }) = self.drawer.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. }
                if self.game.is_none() =>
            {
                self.companion_right_click(el)
            }
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::Focused(_)
                if self.game.is_some() =>
            {
                self.adventure_event(el, &event)
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if !self.ready() {
            return;
        }
        if let Some(limit) = self.args.seconds
            && self.start.elapsed().as_secs_f64() >= limit
        {
            self.quit(el);
            return;
        }
        self.poll_tray(el);
        if self.exiting || !self.service_channel(el) {
            return;
        }
        // The DC-79 click-through decision, the style refresh and the monitor
        // check: their WM_TIMERs starve while a paint is always queued.
        let overlay_due = self.overlay.as_ref().map(|o| o.service_timers());
        let events = self.overlay.as_ref().map(|o| o.drain_events()).unwrap_or_default();
        for e in events {
            match e {
                OverlayEvent::Hotkey(RELEASE_HOTKEY_ID) => {
                    // The release key, pressed anywhere: the only way back
                    // in once the desktop has the keyboard.
                    if let Some(game) = &self.game {
                        game.borrow_mut().release_hotkey_pressed();
                    }
                    self.apply_effects(el);
                }
                other => log::debug!("overlay event: {other:?}"),
            }
        }
        if self.exiting {
            return;
        }
        let now = Instant::now();
        if now >= self.next_tick {
            let interval = Duration::from_secs_f64(1.0 / self.fps.max(1.0));
            self.next_tick = (self.next_tick + interval).max(now);
            self.tick(el);
        } else if self.ui.is_open() && now >= self.next_ui_frame {
            // A menu or dialog keeps running even when the overlay's frame
            // rate has dropped (a fullscreen app in front, battery).
            self.run_ui(el);
        }
        let wake = if self.ui.is_open() { self.next_tick.min(self.next_ui_frame) } else { self.next_tick };
        let wake = overlay_due.map_or(wake, |due| wake.min(due));
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

pub fn run(args: Args, preset: PathBuf, state_dir: PathBuf) -> Result<(), String> {
    let fps = target_fps();
    let rng = match args.seed {
        Some(seed) => PyRandom::from_int(seed as i128),
        None => PyRandom::from_entropy(),
    };
    let ui = OverlayUi::with_state_dir(state_dir.clone());
    let mut engine = Engine {
        args,
        preset,
        state_dir,
        overlay: None,
        drawer: None,
        manager: None,
        game: None,
        painter: ScenePainter::new(PaintSwitches::from_env()),
        adventure_drawer: AdventureDrawer::new(PaintSwitches::from_env()),
        ui,
        rng,
        tray: None,
        channel: None,
        policy: FramePolicy::new(fps),
        fps,
        target_fps: fps,
        carrying: Rc::new(Cell::new(false)),
        next_ui_frame: Instant::now(),
        exiting: false,
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
    // The UI's windows, GPU objects, then the overlay window.
    engine.ui = OverlayUi::with_state_dir(engine.state_dir.clone());
    engine.channel = None;
    engine.drawer = None;
    engine.overlay = None;
    match engine.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
