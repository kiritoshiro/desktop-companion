//! A minimal native host for one dc-ui window: winit + egui-winit +
//! egui-wgpu (no eframe). The settings process uses it for the settings
//! window; the examples use it to show a window on its own.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::ViewportId;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::UiAction;

/// What a hosted window does each frame.
pub trait WindowApp {
    /// Called once, after the theme is applied and before the first frame.
    fn setup(&mut self, _ctx: &egui::Context) {}
    /// Draw one frame and say what should happen.
    fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction>;
    /// Carry out the actions of a frame. Return false to close the window.
    /// The default closes on [`UiAction::CloseWindow`] and logs the rest.
    fn handle(&mut self, actions: Vec<UiAction>) -> bool {
        let mut keep = true;
        for action in actions {
            if action == UiAction::CloseWindow {
                keep = false;
            } else {
                log::info!("UI action: {action:?}");
            }
        }
        keep
    }
    /// Called about every [`POLL_INTERVAL`], also while the window is hidden
    /// (a hidden window gets no frames): watch a child process, a channel.
    fn poll(&mut self) -> PollOutcome {
        PollOutcome::default()
    }
    /// The window's close button, before the loop ends.
    fn on_close(&mut self) {}
}

/// How often [`WindowApp::poll`] runs.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What a [`WindowApp::poll`] asks of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PollOutcome {
    /// Draw a frame.
    pub repaint: bool,
    /// Show (`Some(true)`, also raised and focused) or hide the window.
    pub visible: Option<bool>,
}

/// How to open the window.
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub title: String,
    /// Inner size in logical points.
    pub size: [f32; 2],
    pub min_size: Option<[f32; 2]>,
    /// Close by itself after this long (examples, smoke runs).
    pub exit_after: Option<Duration>,
    /// Use the app icon from `assets/ui` when it is found.
    pub icon: bool,
}

impl RunOptions {
    pub fn new(title: impl Into<String>, size: [f32; 2]) -> Self {
        Self { title: title.into(), size, min_size: None, exit_after: None, icon: true }
    }
}

/// Why the window could not run.
#[derive(Debug)]
pub enum RunError {
    EventLoop(String),
    Window(String),
    Gpu(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::EventLoop(e) => write!(f, "event loop: {e}"),
            RunError::Window(e) => write!(f, "window: {e}"),
            RunError::Gpu(e) => write!(f, "gpu: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Open `app` in a native window titled `title`, `size` logical points big,
/// and run until it is closed.
pub fn run_window(title: &str, size: [f32; 2], app: impl WindowApp) -> Result<(), RunError> {
    run_window_with(RunOptions::new(title, size), app)
}

/// [`run_window`] with every option.
pub fn run_window_with(options: RunOptions, app: impl WindowApp) -> Result<(), RunError> {
    let event_loop = EventLoop::new().map_err(|e| RunError::EventLoop(e.to_string()))?;
    let ctx = egui::Context::default();
    crate::theme::apply_app_theme(&ctx);
    let mut host = Host {
        app,
        options,
        ctx,
        window: None,
        state: None,
        painter: None,
        started: Instant::now(),
        error: None,
        set_up: false,
        next_repaint: None,
        next_poll: Instant::now(),
    };
    event_loop.run_app(&mut host).map_err(|e| RunError::EventLoop(e.to_string()))?;
    match host.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct Host<A: WindowApp> {
    app: A,
    options: RunOptions,
    ctx: egui::Context,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    painter: Option<egui_wgpu::winit::Painter>,
    started: Instant,
    error: Option<RunError>,
    set_up: bool,
    next_repaint: Option<Instant>,
    next_poll: Instant,
}

/// The window commands an app may send with `ctx.send_viewport_cmd`.
fn apply_viewport_commands(window: &Window, commands: &[egui::ViewportCommand]) {
    for command in commands {
        match command {
            egui::ViewportCommand::Visible(v) => window.set_visible(*v),
            egui::ViewportCommand::Minimized(m) => window.set_minimized(*m),
            egui::ViewportCommand::Focus => window.focus_window(),
            _ => {}
        }
    }
}

/// Drive a future to completion on this thread (wgpu's native futures are
/// ready almost at once).
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct ThreadWaker(std::thread::Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park_timeout(Duration::from_millis(5)),
        }
    }
}

impl<A: WindowApp> Host<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: RunError) {
        log::error!("{error}");
        self.error = Some(error);
        event_loop.exit();
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(window), Some(state), Some(painter)) =
            (self.window.clone(), self.state.as_mut(), self.painter.as_mut())
        else {
            return;
        };
        if !self.set_up {
            self.app.setup(&self.ctx);
            self.set_up = true;
        }
        let input = state.take_egui_input(&window);
        let mut actions = Vec::new();
        let app = &mut self.app;
        let mut full = self.ctx.run_pass(input, |ctx| actions = app.ui(ctx));
        state.handle_platform_output(&window, std::mem::take(&mut full.platform_output));
        let clipped = self.ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            full.pixels_per_point,
            [0.0, 0.0, 0.0, 1.0],
            &clipped,
            &mut full.textures_delta,
            Vec::new(),
            &window,
        );
        if let Some(out) = full.viewport_output.get(&ViewportId::ROOT) {
            apply_viewport_commands(&window, &out.commands);
        }
        let delay = full.viewport_output.get(&ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        self.next_repaint = if delay == Duration::ZERO {
            window.request_redraw();
            None
        } else if delay < Duration::from_secs(3600) {
            Some(Instant::now() + delay)
        } else {
            None
        };
        if !self.app.handle(actions) {
            event_loop.exit();
        }
    }
}

impl<A: WindowApp> ApplicationHandler for Host<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(self.options.size[0] as f64, self.options.size[1] as f64));
        if let Some([w, h]) = self.options.min_size {
            attributes = attributes.with_min_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64));
        }
        if self.options.icon
            && let Some(icon) = crate::assets::UiAssets::app_window_icon()
        {
            attributes = attributes.with_window_icon(Some(icon));
        }
        // Hidden until it is placed, so it never shows where Windows put it.
        attributes = attributes.with_visible(false);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(e) => return self.fail(event_loop, RunError::Window(e.to_string())),
        };
        center_on_primary(&window);
        window.set_visible(true);
        let mut painter = block_on(egui_wgpu::winit::Painter::new(
            self.ctx.clone(),
            egui_wgpu::WgpuConfiguration::default(),
            false,
            egui_wgpu::RendererOptions::default(),
        ));
        if let Err(e) = block_on(painter.set_window(ViewportId::ROOT, Some(window.clone()))) {
            return self.fail(event_loop, RunError::Gpu(e.to_string()));
        }
        let state = egui_winit::State::new(
            self.ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            painter.max_texture_side(),
        );
        self.state = Some(state);
        self.painter = Some(painter);
        window.request_redraw();
        self.window = Some(window);
        self.started = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else { return };
        if let Some(state) = self.state.as_mut() {
            let response = state.on_window_event(&window, &event);
            if response.repaint {
                window.request_redraw();
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                self.app.on_close();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(painter), Some(w), Some(h)) =
                    (self.painter.as_mut(), NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    painter.on_window_resized(ViewportId::ROOT, w, h);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(limit) = self.options.exit_after
            && self.started.elapsed() >= limit
        {
            event_loop.exit();
            return;
        }
        if Instant::now() >= self.next_poll {
            self.next_poll = Instant::now() + POLL_INTERVAL;
            let outcome = self.app.poll();
            if let Some(window) = &self.window {
                if let Some(v) = outcome.visible {
                    window.set_visible(v);
                    if v {
                        window.set_minimized(false);
                        window.focus_window();
                    }
                }
                if outcome.repaint || outcome.visible == Some(true) {
                    window.request_redraw();
                }
            }
        }
        let mut wake = Some(self.next_repaint.map_or(self.next_poll, |w| w.min(self.next_poll)));
        if let Some(limit) = self.options.exit_after {
            let end = self.started + limit;
            wake = Some(wake.map_or(end, |w| w.min(end)));
        }
        if let Some(at) = self.next_repaint
            && Instant::now() >= at
        {
            self.next_repaint = None;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(match wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(painter) = self.painter.as_mut() {
            painter.destroy();
        }
    }
}

/// `window_placement.center_on_primary`: the whole window, frame included,
/// in the middle of the main monitor's work area (without the taskbar).
fn center_on_primary(window: &Window) {
    let list = dc_platform::monitors::monitors();
    let Some(primary) = list.get(dc_platform::monitors::primary_index(&list)) else { return };
    let area = primary.work;
    let frame = window.outer_size();
    let w = (frame.width as i32).min(area.w);
    let h = (frame.height as i32).min(area.h);
    let x = area.x + (area.w - w) / 2;
    let y = area.y + (area.h - h) / 2;
    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
}
