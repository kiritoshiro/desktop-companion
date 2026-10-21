//! The wgpu side of the overlay: an instance and surface that can actually
//! show per-pixel alpha on Windows (no Python counterpart; replaces
//! `QOpenGLWidget` + `WA_TranslucentBackground` + the GL alpha buffer of
//! `configure_gl_surface`).
//!
//! Findings (wgpu 30 on Windows 11):
//! - A DX12 swapchain made from the HWND (`Dx12SwapchainKind::DxgiFromHwnd`,
//!   the default) only reports `CompositeAlphaMode::Opaque`: a flip-model
//!   HWND swapchain cannot carry per-pixel alpha.
//! - `Dx12SwapchainKind::DxgiFromVisual` makes wgpu create a
//!   DirectComposition target + visual for the HWND and a composition
//!   swapchain on it; that reports `PreMultiplied` / `PostMultiplied`
//!   (plus `Auto`, `Inherit`, `Opaque`). With the window created without a
//!   redirection bitmap ([`crate::overlay::OverlayBackend::Composition`])
//!   this is the transparent path, and we pick `PreMultiplied`.
//! - Vulkan and GL on Windows generally report `Opaque` only, so the
//!   instance is DX12-only; when DX12 is unavailable or no premultiplied
//!   mode is offered, [`OverlayGpu::new`] fails with
//!   [`GpuError::NoTransparency`] and the engine should fall back to
//!   [`crate::overlay::OverlayBackend::Layered`] (software,
//!   `UpdateLayeredWindow`).
//!
//! Colours written to the surface must be premultiplied, and the clear
//! colour is `(0, 0, 0, 0)`.

use std::sync::Arc;

use wgpu::{CompositeAlphaMode, Dx12BackendOptions, Dx12SwapchainKind};
use winit::window::Window;

/// The instance options that make DX12 present through DirectComposition.
pub fn overlay_instance_descriptor() -> wgpu::InstanceDescriptor {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::DX12;
    desc.backend_options.dx12 =
        Dx12BackendOptions { presentation_system: Dx12SwapchainKind::DxgiFromVisual, ..Default::default() };
    desc
}

pub fn overlay_instance() -> wgpu::Instance {
    wgpu::Instance::new(overlay_instance_descriptor())
}

/// The best alpha mode for a transparent overlay, `None` when the surface
/// cannot be transparent at all.
pub fn preferred_alpha_mode(modes: &[CompositeAlphaMode]) -> Option<CompositeAlphaMode> {
    [CompositeAlphaMode::PreMultiplied, CompositeAlphaMode::PostMultiplied].into_iter().find(|m| modes.contains(m))
}

/// Prefer a plain 8-bit format (non-sRGB first, as the Qt path painted in
/// gamma space), then whatever the surface offers first.
pub fn preferred_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat::*;
    [Bgra8Unorm, Rgba8Unorm, Bgra8UnormSrgb, Rgba8UnormSrgb]
        .into_iter()
        .find(|f| formats.contains(f))
        .or_else(|| formats.first().copied())
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("could not create the overlay surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no DX12 adapter for the overlay: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("could not open the GPU device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the overlay surface offers no transparent alpha mode (got {0:?})")]
    NoTransparency(Vec<CompositeAlphaMode>),
    #[error("the overlay surface reports no formats")]
    NoFormat,
}

/// Instance, adapter, device, queue and a configured transparent surface.
pub struct OverlayGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    /// Every alpha mode the surface reported (for the log).
    pub alpha_modes: Vec<CompositeAlphaMode>,
}

impl OverlayGpu {
    /// Build everything for `window` at `size` (physical pixels). Blocks on
    /// the async adapter/device requests.
    pub fn new(window: Arc<Window>, size: (u32, u32)) -> Result<Self, GpuError> {
        let instance = overlay_instance();
        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor { label: Some("dc-overlay"), ..Default::default() }),
        )?;
        let caps = surface.get_capabilities(&adapter);
        let alpha_modes = caps.alpha_modes.clone();
        let Some(alpha) = preferred_alpha_mode(&caps.alpha_modes) else {
            return Err(GpuError::NoTransparency(alpha_modes));
        };
        let format = preferred_format(&caps.formats).ok_or(GpuError::NoFormat)?;
        let mut config =
            surface.get_default_config(&adapter, size.0.max(1), size.1.max(1)).ok_or(GpuError::NoFormat)?;
        config.format = format;
        config.alpha_mode = alpha;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        log::info!(
            target: "overlay",
            "Overlay surface: {:?} via DX12/DirectComposition, {format:?}, alpha {alpha:?} (offered {alpha_modes:?})",
            adapter.get_info().name
        );
        Ok(Self { instance, adapter, device, queue, surface, config, alpha_modes })
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Reconfigure after the window changed size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width, height) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// The next frame's texture, reconfiguring once on `Outdated`/`Lost`/
    /// `Suboptimal`. `None` means skip this frame.
    pub fn acquire(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_preference() {
        use CompositeAlphaMode::*;
        assert_eq!(preferred_alpha_mode(&[Opaque]), None);
        assert_eq!(preferred_alpha_mode(&[Auto, Inherit, Opaque, PostMultiplied, PreMultiplied]), Some(PreMultiplied));
        assert_eq!(preferred_alpha_mode(&[Opaque, PostMultiplied]), Some(PostMultiplied));
    }

    #[test]
    fn format_preference() {
        use wgpu::TextureFormat::*;
        assert_eq!(preferred_format(&[Rgba8UnormSrgb, Bgra8Unorm]), Some(Bgra8Unorm));
        assert_eq!(preferred_format(&[Rgba16Float]), Some(Rgba16Float));
        assert_eq!(preferred_format(&[]), None);
    }

    #[test]
    fn instance_is_dx12_through_composition() {
        let d = overlay_instance_descriptor();
        assert_eq!(d.backends, wgpu::Backends::DX12);
        assert_eq!(d.backend_options.dx12.presentation_system, Dx12SwapchainKind::DxgiFromVisual);
    }
}
