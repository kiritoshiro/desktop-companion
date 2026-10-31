//! egui windows hosted inside someone else's winit event loop: the overlay
//! process opens its menus and dialogs as extra top-level windows next to
//! its own transparent overlay, all driven by the overlay's `ActiveEventLoop`.
//!
//! [`run_window`](crate::run_window) owns its event loop; [`HostedWindow`] is
//! the same winit + egui-winit + `egui_wgpu::winit::Painter` stack without
//! one. The host creates a window with [`HostedWindow::open`], forwards its
//! window events to [`HostedWindow::on_event`], and calls
//! [`HostedWindow::redraw`] on `RedrawRequested` with a closure that draws
//! the window's egui. Every hosted window keeps its own `egui::Context`;
//! they share one wgpu device through [`GpuShare`], so a pop-up menu opens
//! without creating a device each time.
//!
//! [`headless_pass`] runs a window's egui once without a window, so the host
//! can measure the content (a menu's size) before the window exists; the
//! textures that pass created are handed to `open` and uploaded with the
//! first frame.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{TexturesDelta, Vec2, ViewportId, ViewportInfo};
use egui_wgpu::wgpu;
use egui_wgpu::winit::Painter;
use egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupExisting};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::runner::{RunError, block_on};

/// One wgpu device for every hosted window. The first window creates it;
/// later ones reuse it.
#[derive(Default)]
pub struct GpuShare {
    existing: Option<WgpuSetupExisting>,
    /// DX12 could not start: use wgpu's default backends instead, so menus
    /// still open where the overlay fell back to software.
    dx12_failed: bool,
}

impl GpuShare {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once a device exists.
    pub fn ready(&self) -> bool {
        self.existing.is_some()
    }

    /// Create the device now, without a window, so the first pop-up does
    /// not stall the overlay while wgpu starts up.
    pub fn warm(&mut self) {
        if self.ready() {
            return;
        }
        let config = self.configuration();
        let instance = block_on(config.wgpu_setup.new_instance());
        match block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::default()))
        {
            Ok(rs) => {
                self.existing = Some(WgpuSetupExisting {
                    instance: rs.instance,
                    adapter: rs.adapter,
                    device: rs.device,
                    queue: rs.queue,
                })
            }
            Err(e) if !self.dx12_failed && cfg!(windows) => {
                log::warn!("Could not prepare DX12 for menus ({e}); trying the other backends");
                self.dx12_failed = true;
                self.warm();
            }
            Err(e) => log::warn!("Could not prepare the GPU for menus: {e}"),
        }
    }

    fn configuration(&self) -> WgpuConfiguration {
        let mut config = WgpuConfiguration {
            // These windows paint on the overlay's thread, which is already
            // paced by the overlay's own vsync; a second vsync wait per menu
            // frame made every spider stutter while a menu was hovered.
            surface: egui_wgpu::SurfaceConfig {
                present_mode: wgpu::PresentMode::AutoNoVsync,
                desired_maximum_frame_latency: Some(1),
            },
            ..Default::default()
        };
        match &self.existing {
            Some(existing) => config.wgpu_setup = WgpuSetup::Existing(existing.clone()),
            // DX12 only, like the overlay: every new window's painter
            // enumerates the instance's adapters, and Vulkan and GL make
            // that slow.
            None if cfg!(windows) => {
                if let WgpuSetup::CreateNew(create) = &mut config.wgpu_setup {
                    create.instance_descriptor.backends = if self.dx12_failed {
                        // The default set ranks DX12 first; it would be
                        // picked, and fail, again.
                        create.instance_descriptor.backends.difference(wgpu::Backends::DX12)
                    } else {
                        wgpu::Backends::DX12
                    };
                }
            }
            None => {}
        }
        config
    }

    fn remember(&mut self, painter: &Painter) {
        if self.existing.is_none()
            && let Some(rs) = painter.render_state()
        {
            self.existing = Some(WgpuSetupExisting {
                instance: rs.instance,
                adapter: rs.adapter,
                device: rs.device,
                queue: rs.queue,
            });
        }
    }
}

/// What one frame of a hosted window asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameReport {
    /// egui wants another frame after this long (`Duration::MAX`: only on input).
    pub repaint_delay: Duration,
}

/// A top-level window that draws egui.
pub struct HostedWindow {
    window: Arc<Window>,
    ctx: egui::Context,
    state: egui_winit::State,
    painter: Painter,
    pending: TexturesDelta,
    next_repaint: Option<Instant>,
}

impl HostedWindow {
    /// Create the window and its painter. `pending` are textures from
    /// [`headless_pass`]es of `ctx` that have not been uploaded yet.
    pub fn open(
        el: &ActiveEventLoop,
        attributes: WindowAttributes,
        ctx: egui::Context,
        gpu: &mut GpuShare,
        mut pending: TexturesDelta,
    ) -> Result<Self, RunError> {
        let window = match el.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                pending.clear();
                return Err(RunError::Window(e.to_string()));
            }
        };
        let mut painter =
            block_on(Painter::new(ctx.clone(), gpu.configuration(), false, egui_wgpu::RendererOptions::default()));
        if let Err(e) = block_on(painter.set_window(ViewportId::ROOT, Some(window.clone()))) {
            pending.clear();
            painter.destroy();
            return Err(RunError::Gpu(e.to_string()));
        }
        gpu.remember(&painter);
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            painter.max_texture_side(),
        );
        window.request_redraw();
        Ok(Self { window, ctx, state, painter, pending, next_repaint: None })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Feed one window event to egui. Returns true when the window should
    /// be redrawn. Resizes the surface on `Resized`.
    pub fn on_event(&mut self, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(&self.window, event);
        if let WindowEvent::Resized(size) = event
            && let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        {
            self.painter.on_window_resized(ViewportId::ROOT, w, h);
            self.window.request_redraw();
        }
        if response.repaint {
            self.window.request_redraw();
        }
        response.repaint
    }

    /// Run one egui pass with `run` and paint it.
    pub fn redraw(&mut self, run: &mut dyn FnMut(&egui::Context)) -> FrameReport {
        let input = self.state.take_egui_input(&self.window);
        let mut full = self.ctx.run_pass(input, |ctx| run(ctx));
        self.state.handle_platform_output(&self.window, std::mem::take(&mut full.platform_output));
        let clipped = self.ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
        let mut textures = std::mem::take(&mut self.pending);
        textures.append(std::mem::take(&mut full.textures_delta));
        self.painter.paint_and_update_textures(
            ViewportId::ROOT,
            full.pixels_per_point,
            [0.0, 0.0, 0.0, 1.0],
            &clipped,
            &mut textures,
            Vec::new(),
            &self.window,
        );
        textures.clear();
        let delay = full.viewport_output.get(&ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        self.next_repaint = if delay == Duration::ZERO {
            self.window.request_redraw();
            None
        } else if delay < Duration::from_secs(3600) {
            Some(Instant::now() + delay)
        } else {
            None
        };
        FrameReport { repaint_delay: delay }
    }

    /// Ask for a frame when egui's repaint timer is due (or `force`).
    pub fn poll(&mut self, force: bool) {
        let due = self.next_repaint.is_some_and(|at| Instant::now() >= at);
        if due {
            self.next_repaint = None;
        }
        if due || force {
            self.window.request_redraw();
        }
    }

    /// When egui next wants a frame on its own.
    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }
}

impl Drop for HostedWindow {
    fn drop(&mut self) {
        self.pending.clear();
        // GPU objects before the window.
        self.painter.destroy();
    }
}

/// Run one egui pass of `ctx` without a window, on a viewport `size` points
/// big at `pixels_per_point`, and return the textures it created (to be
/// uploaded by the window's first frame). The shapes are thrown away.
pub fn headless_pass(
    ctx: &egui::Context,
    size: Vec2,
    pixels_per_point: f32,
    run: &mut dyn FnMut(&egui::Context),
) -> TexturesDelta {
    let mut input =
        egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
    input.viewports.insert(
        ViewportId::ROOT,
        ViewportInfo { native_pixels_per_point: Some(pixels_per_point), ..Default::default() },
    );
    let mut full = ctx.run_pass(input, |ctx| run(ctx));
    std::mem::take(&mut full.textures_delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backends(share: &GpuShare) -> Option<wgpu::Backends> {
        match share.configuration().wgpu_setup {
            WgpuSetup::CreateNew(create) => Some(create.instance_descriptor.backends),
            WgpuSetup::Existing(_) => None,
        }
    }

    #[test]
    fn menus_try_dx12_first_then_every_backend() {
        let mut share = GpuShare::new();
        let config = share.configuration();
        assert_eq!(config.surface.present_mode, wgpu::PresentMode::AutoNoVsync, "no second vsync wait");
        if cfg!(windows) {
            assert_eq!(backends(&share), Some(wgpu::Backends::DX12));
            share.dx12_failed = true;
            let fallback = backends(&share).expect("a new device");
            assert!(!fallback.contains(wgpu::Backends::DX12), "a failed DX12 start is not retried");
            assert!(fallback.contains(wgpu::Backends::VULKAN) || fallback.contains(wgpu::Backends::GL));
        }
    }
}
