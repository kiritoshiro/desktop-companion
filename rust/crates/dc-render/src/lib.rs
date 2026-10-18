//! `dc-render`: the QPainter replacement for the Rust port of Desktop Bug Companion.
//!
//! Painting code records into a retained [`Scene`] through a [`Canvas`]
//! whose methods mirror `QPainter` one to one (`save`, `translate`,
//! `set_brush`, `draw_ellipse_at`, `draw_path`, `set_clip_path`,
//! `draw_image`, `draw_text_in`, …). Two backends rasterise a scene:
//!
//! * [`CpuRenderer`] – tiny-skia, into an [`Image`] (tests, offscreen art,
//!   software overlay fallback). `Image::paint(|c| …)` is `QPainter(&image)`.
//! * [`GpuRenderer`] – wgpu, into any `TextureView` (the overlay window).
//!   Everything batches into one vertex/index buffer and a handful of draw
//!   calls: a single uber-shader handles solid colours, linear / radial /
//!   conical gradients, images (texture atlas) and text (glyph atlas); clip
//!   rects use scissors, clip paths the stencil buffer, overlaps are resolved
//!   with a per-command depth value, and 4× MSAA antialiases.
//!   [`gpu::Headless`] renders offscreen and reads back to an [`Image`].
//!
//! Geometry types: [`Path`] (`QPainterPath`), [`PathBuilder`], [`Transform`]
//! (`QTransform`), [`Region`] (`QRegion`). Paint types: [`Brush`], [`Pen`],
//! [`LinearGradient`], [`RadialGradient`], [`ConicalGradient`],
//! [`CompositionMode`]. Text: [`Font`], [`FontMetrics`], [`TextFlags`].
//!
//! See `README.md` ("Porting QPainter code") for the call-by-call mapping.

pub mod canvas;
pub mod cpu;
pub mod demo;
pub mod gpu;
pub mod image;
pub mod paint;
pub mod path;
pub mod region;
pub mod scene;
pub(crate) mod stroke;
pub mod text;
pub mod transform;

pub use canvas::{Canvas, ClipOp};
pub use cpu::CpuRenderer;
pub use dc_core::{Color, Rect, Vec2};
pub use gpu::{GpuRenderer, Headless};
pub use image::{Image, ImageError, TransformMode};
pub use paint::{
    Brush, Cap, CompositionMode, ConicalGradient, DashPattern, GradientStop, GradientStops, Join, LinearGradient, Pen,
    PenStyle, RadialGradient, RenderHint, Spread, parse_color, premultiply, unpremultiply,
};
pub use path::{FillRule, Path, PathBuilder, PathEl, Verb};
pub use region::Region;
pub use scene::{Scene, SceneStats};
pub use text::{ElideMode, Font, FontMetrics, TextFlags, Weight};
pub use transform::Transform;
