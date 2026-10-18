//! GPU backend (wgpu).
//!
//! A frame is one render pass. Every command of the scene becomes triangles
//! in a single vertex/index buffer (reused between frames) and one entry in a
//! storage buffer of paint records; consecutive commands that share pipeline
//! state (composition mode, scissor, image texture, stencil clip) form one
//! draw call. Solid colours, gradients and glyphs never break a batch; images
//! only do when they live in different atlas pages.
//!
//! * Antialiasing: 4× MSAA (or whatever `sample_count` is given).
//! * Overlap: each command gets its own depth value and is drawn with a
//!   strict depth test, so the overlapping triangles of one stroke never
//!   blend twice, while later commands always draw over earlier ones.
//! * Clips: clip rects become scissor rects; clip paths are written into the
//!   stencil buffer (one increment per intersected path).
//! * Thin strokes (< 1 device px) are drawn 1 px wide with proportionally
//!   less alpha, like a coverage-based rasteriser would.
//! * Output is premultiplied alpha; use a non-sRGB target format.
//!
//! Driving it from a window:
//! ```ignore
//! let mut r = GpuRenderer::new(&device, &queue, surface_format, 4);
//! // every frame
//! let frame = surface.get_current_texture()?;
//! let view = frame.texture.create_view(&Default::default());
//! r.render(&scene, &view, (width, height), Some(Color::TRANSPARENT));
//! frame.present();
//! ```

mod atlas;

use crate::image::Image;
use crate::paint::{Brush, CompositionMode};
use crate::path::{FillRule, Path, Polylines};
use crate::scene::{Command, Op, SOLID, Scene, Shape};
use crate::stroke::{self, StrokeParams, TriOut};
use crate::text::{self, Face, FaceId};
use crate::transform::{Transform, map_f32};
use atlas::Shelves;
use bytemuck::{Pod, Zeroable};
use dc_core::Color;
use lyon::tessellation as lt;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Flattening tolerance in device pixels.
const TOL: f32 = 0.2;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
const PAGE_SIZE: u32 = 2048;
const MAX_PAGES: usize = 4;
const ATLAS_MAX_IMAGE: u32 = 1024;
const GLYPH_ATLAS_START: u32 = 1024;
const GLYPH_ATLAS_MAX: u32 = 4096;
/// Above this device em size glyphs are tessellated outlines, not bitmaps.
const MAX_BITMAP_GLYPH_PX: f32 = 160.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    paint: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct GpuPaint {
    kind: u32,
    stop_start: u32,
    stop_count: u32,
    z: f32,
    color: [f32; 4],
    inv0: [f32; 4],
    inv1: [f32; 4],
    g0: [f32; 4],
    g1: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct GpuStop {
    color: [f32; 4],
    pos: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PipeKey {
    Content { blend: CompositionMode, stencil: bool },
    StencilWrite,
    StencilReset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TexKey {
    Page(u32),
    Dedicated(u64),
}

#[derive(Clone, Copy, Debug)]
struct Batch {
    key: PipeKey,
    scissor: [u32; 4],
    tex: Option<TexKey>,
    stencil_ref: u32,
    start: u32,
    end: u32,
}

/// Per-frame statistics of the last [`GpuRenderer::render`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuStats {
    pub draw_calls: u32,
    pub vertices: u32,
    pub indices: u32,
    pub commands: u32,
    /// CPU time spent building geometry and uploading, in microseconds.
    pub prepare_us: u32,
}

#[derive(Debug)]
enum Full {
    Glyphs,
    Images,
}

struct GrowBuffer {
    buf: wgpu::Buffer,
    cap: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl GrowBuffer {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages, cap: u64) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage,
            mapped_at_creation: false,
        });
        Self { buf, cap, usage, label }
    }

    /// Uploads `data`; returns true when the buffer had to be recreated.
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> bool {
        let need = (data.len() as u64).max(16).next_multiple_of(16);
        let mut grown = false;
        if need > self.cap {
            self.cap = need.next_power_of_two();
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: self.cap,
                usage: self.usage,
                mapped_at_creation: false,
            });
            grown = true;
        }
        if !data.is_empty() {
            let len = data.len() & !3;
            queue.write_buffer(&self.buf, 0, &data[..len]);
            if len < data.len() {
                let mut tail = [0u8; 4];
                tail[..data.len() - len].copy_from_slice(&data[len..]);
                queue.write_buffer(&self.buf, len as u64, &tail);
            }
        }
        grown
    }
}

#[derive(Clone, Copy)]
struct GlyphSlot {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
}

struct GlyphAtlas {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
    shelves: Shelves,
    map: HashMap<(FaceId, u16, u16, u8), Option<GlyphSlot>>,
    generation: u64,
}

impl GlyphAtlas {
    fn new(device: &wgpu::Device, size: u32, generation: u64) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dc-render glyph atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        Self { tex, view, size, shelves: Shelves::new(size, size), map: HashMap::new(), generation }
    }
}

struct Page {
    tex: wgpu::Texture,
    bind: wgpu::BindGroup,
    shelves: Shelves,
}

struct ImgEntry {
    version: u64,
    tex: TexKey,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    last_used: u64,
}

struct Dedicated {
    tex: wgpu::Texture,
    bind: wgpu::BindGroup,
    w: u32,
    h: u32,
}

struct Targets {
    size: (u32, u32),
    msaa: Option<wgpu::TextureView>,
    ds: wgpu::TextureView,
    fresh: bool,
}

#[derive(Default)]
struct Frame {
    verts: Vec<Vertex>,
    idx: Vec<u32>,
    paints: Vec<GpuPaint>,
    stops: Vec<GpuStop>,
    batches: Vec<Batch>,
}

impl Frame {
    fn clear(&mut self) {
        self.verts.clear();
        self.idx.clear();
        self.paints.clear();
        self.stops.clear();
        self.batches.clear();
        // Paint 0: the stencil / dummy paint.
        self.paints.push(GpuPaint { inv0: [1.0, 0.0, 0.0, 1.0], ..GpuPaint::zeroed() });
    }

    fn begin(&mut self, key: PipeKey, scissor: [u32; 4], tex: Option<TexKey>, stencil_ref: u32) {
        let at = self.idx.len() as u32;
        if let Some(b) = self.batches.last_mut() {
            let tex_ok = tex.is_none() || b.tex.is_none() || b.tex == tex;
            if b.key == key && b.scissor == scissor && b.stencil_ref == stencil_ref && tex_ok {
                if b.tex.is_none() {
                    b.tex = tex;
                }
                return;
            }
        }
        self.batches.push(Batch { key, scissor, tex, stencil_ref, start: at, end: at });
    }

    fn finish(&mut self) {
        if let Some(b) = self.batches.last_mut() {
            b.end = self.idx.len() as u32;
        }
    }
}

struct Geo<'a> {
    v: &'a mut Vec<Vertex>,
    i: &'a mut Vec<u32>,
    m: [f32; 6],
    paint: u32,
}

impl TriOut for Geo<'_> {
    #[inline]
    fn vertex(&mut self, p: [f32; 2]) -> u32 {
        self.v.push(Vertex { pos: map_f32(&self.m, p[0], p[1]), uv: [0.0; 2], paint: self.paint });
        self.v.len() as u32 - 1
    }
    #[inline]
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.i.extend_from_slice(&[a, b, c]);
    }
}

/// The wgpu renderer. Create one per device/target format.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    sample_count: u32,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bgl0: wgpu::BindGroupLayout,
    bgl1: wgpu::BindGroupLayout,
    pipelines: HashMap<PipeKey, wgpu::RenderPipeline>,
    globals: wgpu::Buffer,
    vbuf: GrowBuffer,
    ibuf: GrowBuffer,
    pbuf: GrowBuffer,
    sbuf: GrowBuffer,
    sampler: wgpu::Sampler,
    glyphs: GlyphAtlas,
    pages: Vec<Page>,
    entries: HashMap<u64, ImgEntry>,
    dedicated: HashMap<u64, Dedicated>,
    dummy_bind: wgpu::BindGroup,
    bind0: Option<(wgpu::BindGroup, u64)>,
    bind0_dirty: bool,
    targets: Option<Targets>,
    frame: Frame,
    lines: Polylines,
    lyon: lt::FillTessellator,
    lyon_vb: lt::VertexBuffers<[f32; 2], u32>,
    faces: HashMap<FaceId, Arc<Face>>,
    outlines: HashMap<(FaceId, u16), Option<Path>>,
    frame_no: u64,
    force_dedicated: bool,
    stats: GpuStats,
}

static WARNED_BLEND: AtomicBool = AtomicBool::new(false);

fn gpu_blend(m: CompositionMode) -> CompositionMode {
    use CompositionMode as C;
    match m {
        C::SourceOver
        | C::DestinationOver
        | C::Source
        | C::SourceIn
        | C::DestinationIn
        | C::SourceOut
        | C::DestinationOut
        | C::SourceAtop
        | C::DestinationAtop
        | C::Xor
        | C::Plus => m,
        C::Clear => C::DestinationOut,
        _ => {
            if !WARNED_BLEND.swap(true, Ordering::Relaxed) {
                log::warn!("composition mode {m:?} is CPU-only; GPU draws it as SourceOver");
            }
            C::SourceOver
        }
    }
}

fn blend_state(m: CompositionMode) -> wgpu::BlendState {
    use CompositionMode as C;
    use wgpu::BlendFactor as F;
    let (s, d) = match m {
        C::SourceOver => (F::One, F::OneMinusSrcAlpha),
        C::DestinationOver => (F::OneMinusDstAlpha, F::One),
        C::Source => (F::One, F::Zero),
        C::SourceIn => (F::DstAlpha, F::Zero),
        C::DestinationIn => (F::Zero, F::SrcAlpha),
        C::SourceOut => (F::OneMinusDstAlpha, F::Zero),
        C::DestinationOut => (F::Zero, F::OneMinusSrcAlpha),
        C::SourceAtop => (F::DstAlpha, F::OneMinusSrcAlpha),
        C::DestinationAtop => (F::OneMinusDstAlpha, F::SrcAlpha),
        C::Xor => (F::OneMinusDstAlpha, F::OneMinusSrcAlpha),
        C::Plus => (F::One, F::One),
        _ => (F::One, F::OneMinusSrcAlpha),
    };
    let c = wgpu::BlendComponent { src_factor: s, dst_factor: d, operation: wgpu::BlendOperation::Add };
    wgpu::BlendState { color: c, alpha: c }
}

impl GpuRenderer {
    /// `format` is the target's format (non-sRGB for Qt-identical blending);
    /// `sample_count` is the MSAA level (4 recommended, 1 disables MSAA).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dc-render shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let frag_vis = wgpu::ShaderStages::FRAGMENT;
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: frag_vis,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dc-render bgl0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1),
                storage(2),
                tex_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: frag_vis,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dc-render bgl1"),
            entries: &[tex_entry(0)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dc-render layout"),
            bind_group_layouts: &[Some(&bgl0), Some(&bgl1)],
            immediate_size: 0,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dc-render globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dc-render linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let dummy = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dc-render dummy"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_view = dummy.create_view(&Default::default());
        let dummy_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dc-render dummy bind"),
            layout: &bgl1,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&dummy_view) }],
        });
        let vu = wgpu::BufferUsages::VERTEX;
        let iu = wgpu::BufferUsages::INDEX;
        let su = wgpu::BufferUsages::STORAGE;
        GpuRenderer {
            vbuf: GrowBuffer::new(device, "dc-render vertices", vu, 1 << 20),
            ibuf: GrowBuffer::new(device, "dc-render indices", iu, 1 << 20),
            pbuf: GrowBuffer::new(device, "dc-render paints", su, 1 << 18),
            sbuf: GrowBuffer::new(device, "dc-render stops", su, 1 << 14),
            glyphs: GlyphAtlas::new(device, GLYPH_ATLAS_START, 1),
            device: device.clone(),
            queue: queue.clone(),
            format,
            sample_count: sample_count.max(1),
            shader,
            layout,
            bgl0,
            bgl1,
            pipelines: HashMap::new(),
            globals,
            sampler,
            pages: Vec::new(),
            entries: HashMap::new(),
            dedicated: HashMap::new(),
            dummy_bind,
            bind0: None,
            bind0_dirty: true,
            targets: None,
            frame: Frame::default(),
            lines: Polylines::default(),
            lyon: lt::FillTessellator::new(),
            lyon_vb: lt::VertexBuffers::new(),
            faces: HashMap::new(),
            outlines: HashMap::new(),
            frame_no: 0,
            force_dedicated: false,
            stats: GpuStats::default(),
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Statistics of the last rendered frame.
    pub fn stats(&self) -> GpuStats {
        self.stats
    }

    /// Renders `scene` into `target` (`size` in pixels). `clear: Some(c)`
    /// starts from `c`; `None` keeps what this renderer drew last frame (with
    /// MSAA the multisampled buffer persists) or the target's content (no MSAA).
    pub fn render(&mut self, scene: &Scene, target: &wgpu::TextureView, size: (u32, u32), clear: Option<Color>) {
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        self.render_inner(scene, target, size, clear);
    }

    fn render_inner(
        &mut self,
        scene: &Scene,
        target: &wgpu::TextureView,
        size: (u32, u32),
        clear: Option<Color>,
    ) -> wgpu::SubmissionIndex {
        self.frame_no += 1;
        self.ensure_targets(size);
        self.force_dedicated = false;
        let t0 = std::time::Instant::now();
        let mut attempt = 0;
        loop {
            match self.prepare(scene, size) {
                Ok(()) => break,
                Err(Full::Glyphs) => {
                    let next = (self.glyphs.size * 2).min(GLYPH_ATLAS_MAX);
                    let generation = self.glyphs.generation + 1;
                    self.glyphs = GlyphAtlas::new(&self.device, next, generation);
                    self.bind0_dirty = true;
                }
                Err(Full::Images) => {
                    for p in &mut self.pages {
                        p.shelves.reset();
                    }
                    self.entries.retain(|_, e| matches!(e.tex, TexKey::Dedicated(_)));
                    if attempt >= 1 {
                        self.force_dedicated = true;
                    }
                }
            }
            attempt += 1;
            if attempt > 4 {
                log::warn!("dc-render: atlas overflow, frame drawn incomplete");
                break;
            }
        }
        self.evict();
        self.upload(size);
        self.ensure_pipelines();
        self.stats.prepare_us = t0.elapsed().as_micros() as u32;
        self.encode(target, size, clear)
    }

    fn ensure_targets(&mut self, size: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let ext = wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 };
        let msaa = (self.sample_count > 1).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("dc-render msaa"),
                    size: ext,
                    mip_level_count: 1,
                    sample_count: self.sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let ds = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("dc-render depth-stencil"),
                size: ext,
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        self.targets = Some(Targets { size, msaa, ds, fresh: true });
    }

    fn evict(&mut self) {
        let now = self.frame_no;
        let stale: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.tex, TexKey::Dedicated(_)) && now - e.last_used > 300)
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            self.entries.remove(&k);
            self.dedicated.remove(&k);
        }
    }

    fn upload(&mut self, size: (u32, u32)) {
        let f = &self.frame;
        self.queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
        self.vbuf.write(&self.device, &self.queue, bytemuck::cast_slice(&f.verts));
        self.ibuf.write(&self.device, &self.queue, bytemuck::cast_slice(&f.idx));
        let mut grown = self.pbuf.write(&self.device, &self.queue, bytemuck::cast_slice(&f.paints));
        let stops: &[GpuStop] =
            if f.stops.is_empty() { &[GpuStop { color: [0.0; 4], pos: [0.0; 4] }] } else { &f.stops };
        grown |= self.sbuf.write(&self.device, &self.queue, bytemuck::cast_slice(stops));
        let gen_ok = self.bind0.as_ref().is_some_and(|b| b.1 == self.glyphs.generation);
        if grown || self.bind0_dirty || !gen_ok {
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dc-render bind0"),
                layout: &self.bgl0,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.pbuf.buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: self.sbuf.buf.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.glyphs.view),
                    },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.bind0 = Some((bg, self.glyphs.generation));
            self.bind0_dirty = false;
        }
    }

    fn ensure_pipelines(&mut self) {
        let keys: Vec<PipeKey> = {
            let mut k: Vec<PipeKey> = self.frame.batches.iter().map(|b| b.key).collect();
            k.sort_by_key(|k| format!("{k:?}"));
            k.dedup();
            k
        };
        for key in keys {
            if !self.pipelines.contains_key(&key) {
                let p = self.create_pipeline(key);
                self.pipelines.insert(key, p);
            }
        }
    }

    fn create_pipeline(&self, key: PipeKey) -> wgpu::RenderPipeline {
        let face = |compare, pass_op| wgpu::StencilFaceState {
            compare,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op,
        };
        let (blend, writes, depth_write, depth_cmp, stencil) = match key {
            PipeKey::Content { blend, stencil } => {
                let f = if stencil {
                    face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::Keep)
                } else {
                    face(wgpu::CompareFunction::Always, wgpu::StencilOperation::Keep)
                };
                (
                    Some(blend_state(blend)),
                    wgpu::ColorWrites::ALL,
                    true,
                    wgpu::CompareFunction::Greater,
                    wgpu::StencilState { front: f, back: f, read_mask: 0xff, write_mask: 0 },
                )
            }
            PipeKey::StencilWrite => {
                let f = face(wgpu::CompareFunction::Equal, wgpu::StencilOperation::IncrementClamp);
                (
                    None,
                    wgpu::ColorWrites::empty(),
                    false,
                    wgpu::CompareFunction::Always,
                    wgpu::StencilState { front: f, back: f, read_mask: 0xff, write_mask: 0xff },
                )
            }
            PipeKey::StencilReset => {
                let f = face(wgpu::CompareFunction::Always, wgpu::StencilOperation::Replace);
                (
                    None,
                    wgpu::ColorWrites::empty(),
                    false,
                    wgpu::CompareFunction::Always,
                    wgpu::StencilState { front: f, back: f, read_mask: 0xff, write_mask: 0xff },
                )
            }
        };
        let attrs = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 16, shader_location: 2 },
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dc-render pipeline"),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vbl)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(depth_cmp),
                stencil,
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: self.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: self.format, blend, write_mask: writes })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    fn encode(&mut self, target: &wgpu::TextureView, size: (u32, u32), clear: Option<Color>) -> wgpu::SubmissionIndex {
        let targets = self.targets.as_mut().unwrap();
        let fresh = std::mem::replace(&mut targets.fresh, false);
        let clear = clear.or(if fresh && targets.msaa.is_some() { Some(Color::TRANSPARENT) } else { None });
        let load = match clear {
            Some(c) => {
                let p = crate::paint::premul_f32(c);
                wgpu::LoadOp::Clear(wgpu::Color { r: p[0] as f64, g: p[1] as f64, b: p[2] as f64, a: p[3] as f64 })
            }
            None => wgpu::LoadOp::Load,
        };
        let targets = self.targets.as_ref().unwrap();
        let (view, resolve) = match &targets.msaa {
            Some(m) => (m, Some(target)),
            None => (target, None),
        };
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("dc-render") });
        let mut draws = 0u32;
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dc-render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.ds,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !self.frame.idx.is_empty() {
                pass.set_vertex_buffer(0, self.vbuf.buf.slice(..));
                pass.set_index_buffer(self.ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_bind_group(0, &self.bind0.as_ref().unwrap().0, &[]);
                let mut cur_pipe: Option<PipeKey> = None;
                let mut cur_tex: Option<Option<TexKey>> = None;
                let mut cur_scissor = [u32::MAX; 4];
                let mut cur_ref = u32::MAX;
                for b in &self.frame.batches {
                    if b.end <= b.start {
                        continue;
                    }
                    if cur_pipe != Some(b.key) {
                        pass.set_pipeline(&self.pipelines[&b.key]);
                        cur_pipe = Some(b.key);
                    }
                    if cur_tex != Some(b.tex) {
                        let bg = match b.tex {
                            None => &self.dummy_bind,
                            Some(TexKey::Page(i)) => &self.pages[i as usize].bind,
                            Some(TexKey::Dedicated(id)) => {
                                self.dedicated.get(&id).map_or(&self.dummy_bind, |d| &d.bind)
                            }
                        };
                        pass.set_bind_group(1, bg, &[]);
                        cur_tex = Some(b.tex);
                    }
                    if cur_scissor != b.scissor {
                        let s = b.scissor;
                        pass.set_scissor_rect(s[0], s[1], s[2] - s[0], s[3] - s[1]);
                        cur_scissor = s;
                    }
                    if cur_ref != b.stencil_ref {
                        pass.set_stencil_reference(b.stencil_ref);
                        cur_ref = b.stencil_ref;
                    }
                    pass.draw_indexed(b.start..b.end, 0, 0..1);
                    draws += 1;
                }
            }
        }
        let _ = size;
        self.stats.draw_calls = draws;
        self.queue.submit(Some(enc.finish()))
    }

    // ------------------------------------------------------------- prepare

    fn face(&mut self, id: FaceId) -> Option<Arc<Face>> {
        if let Some(f) = self.faces.get(&id) {
            return Some(f.clone());
        }
        let f = text::face(id)?;
        self.faces.insert(id, f.clone());
        Some(f)
    }

    fn paint_record(&mut self, scene: &Scene, cmd: &Command, z: f32, alpha: f32, extra_kind: u32) -> u32 {
        let t = Transform::from_f32(cmd.transform);
        let inv = t.inverted().unwrap_or(Transform::IDENTITY);
        let opacity = cmd.opacity * alpha;
        let mut p = GpuPaint {
            kind: extra_kind,
            z,
            inv0: [inv.m11 as f32, inv.m12 as f32, inv.m21 as f32, inv.m22 as f32],
            inv1: [inv.dx as f32, inv.dy as f32, opacity, 0.0],
            ..GpuPaint::zeroed()
        };
        let brush = if cmd.paint == SOLID { Brush::Solid(cmd.color) } else { scene.paints[cmd.paint as usize] };
        let f = &mut self.frame;
        let mut push_stops = |stops: &crate::paint::GradientStops, p: &mut GpuPaint| {
            p.stop_start = f.stops.len() as u32;
            for s in stops.as_slice() {
                f.stops.push(GpuStop { color: crate::paint::premul_f32(s.color), pos: [s.pos, 0.0, 0.0, 0.0] });
            }
            if stops.is_empty() {
                f.stops.push(GpuStop { color: [0.0; 4], pos: [0.0; 4] });
            }
            p.stop_count = stops.len().max(1) as u32;
        };
        let spread = |s: crate::paint::Spread| match s {
            crate::paint::Spread::Pad => 0.0,
            crate::paint::Spread::Reflect => 1.0,
            crate::paint::Spread::Repeat => 2.0,
        };
        match brush {
            Brush::None => {}
            Brush::Solid(c) => {
                let pc = crate::paint::premul_f32(c);
                p.color = [pc[0] * opacity, pc[1] * opacity, pc[2] * opacity, pc[3] * opacity];
            }
            Brush::Linear(g) => {
                p.kind |= 1;
                p.g0 = [g.x1, g.y1, g.x2, g.y2];
                p.inv1[3] = spread(g.spread);
                push_stops(&g.stops, &mut p);
            }
            Brush::Radial(g) => {
                p.kind |= 2;
                p.g0 = [g.cx, g.cy, g.fx, g.fy];
                p.g1 = [g.radius, g.focal_radius, 0.0, 0.0];
                p.inv1[3] = spread(g.spread);
                push_stops(&g.stops, &mut p);
            }
            Brush::Conical(g) => {
                p.kind |= 3;
                p.g0 = [g.cx, g.cy, g.angle.to_radians(), 0.0];
                push_stops(&g.stops, &mut p);
            }
        }
        f.paints.push(p);
        f.paints.len() as u32 - 1
    }

    fn image_paint(&mut self, cmd: &Command, z: f32, smooth: bool) -> u32 {
        let p = GpuPaint {
            kind: if smooth { 4 } else { 5 },
            z,
            inv0: [1.0, 0.0, 0.0, 1.0],
            inv1: [0.0, 0.0, cmd.opacity, 0.0],
            ..GpuPaint::zeroed()
        };
        self.frame.paints.push(p);
        self.frame.paints.len() as u32 - 1
    }

    fn prepare(&mut self, scene: &Scene, size: (u32, u32)) -> Result<(), Full> {
        self.frame.clear();
        let full = [0u32, 0, size.0, size.1];
        let mut stencil_clip = 0u32;
        let mut stencil_dirty = false;
        for (i, cmd) in scene.cmds.iter().enumerate() {
            let z = ((i + 1).min((1 << 23) - 1)) as f32 / 16_777_216.0;
            let clip = scene.clips[cmd.clip as usize];
            let scissor = match clip.scissor {
                None => full,
                Some(s) => {
                    let l = s[0].round().clamp(0.0, size.0 as f32) as u32;
                    let t = s[1].round().clamp(0.0, size.1 as f32) as u32;
                    let r = s[2].round().clamp(0.0, size.0 as f32) as u32;
                    let b = s[3].round().clamp(0.0, size.1 as f32) as u32;
                    [l, t, r, b]
                }
            };
            if scissor[2] <= scissor[0] || scissor[3] <= scissor[1] {
                continue;
            }
            let nshapes = clip.shapes.1 - clip.shapes.0;
            if nshapes > 0 && stencil_clip != cmd.clip {
                if stencil_dirty {
                    self.frame.begin(PipeKey::StencilReset, full, None, 0);
                    let (w, h) = (size.0 as f32, size.1 as f32);
                    let mut g = Geo {
                        v: &mut self.frame.verts,
                        i: &mut self.frame.idx,
                        m: Transform::IDENTITY.to_f32(),
                        paint: 0,
                    };
                    let a = g.vertex([0.0, 0.0]);
                    let b = g.vertex([w, 0.0]);
                    let c = g.vertex([w, h]);
                    let d = g.vertex([0.0, h]);
                    g.tri(a, b, c);
                    g.tri(a, c, d);
                    self.frame.finish();
                }
                for k in 0..nshapes {
                    let cs = scene.clip_shapes[(clip.shapes.0 + k) as usize];
                    self.frame.begin(PipeKey::StencilWrite, full, None, k);
                    self.fill_shape(scene, &cs.shape, cs.rule, cs.transform, 0);
                    self.frame.finish();
                }
                stencil_clip = cmd.clip;
                stencil_dirty = true;
            }
            let key = PipeKey::Content { blend: gpu_blend(cmd.comp), stencil: nshapes > 0 };
            match cmd.op {
                Op::Fill { shape, rule } => {
                    let paint = self.paint_record(scene, cmd, z, 1.0, 0);
                    self.frame.begin(key, scissor, None, nshapes);
                    self.fill_shape(scene, &shape, rule, cmd.transform, paint);
                }
                Op::Stroke { shape, stroke } => {
                    let st = scene.strokes[stroke as usize];
                    let t = Transform::from_f32(cmd.transform);
                    let s = t.scale_factor().max(1e-9) as f32;
                    let max_s = t.max_scale().max(1e-9) as f32;
                    let mut w_dev =
                        if st.cosmetic { if st.width > 0.0 { st.width } else { 1.0 } } else { st.width * s };
                    let mut alpha = 1.0;
                    if w_dev < 1.0 {
                        alpha = w_dev.max(0.0);
                        w_dev = 1.0;
                    }
                    let paint = self.paint_record(scene, cmd, z, alpha, 0);
                    self.frame.begin(key, scissor, None, nshapes);
                    let params = StrokeParams {
                        half_width: w_dev * 0.5 / s,
                        cap: st.cap,
                        join: st.join,
                        miter_limit: st.miter_limit,
                        tolerance: TOL / max_s,
                    };
                    let mut g = Geo { v: &mut self.frame.verts, i: &mut self.frame.idx, m: cmd.transform, paint };
                    if let Shape::Line { x1, y1, x2, y2 } = shape {
                        if x1 == x2 && y1 == y2 {
                            stroke::dot(&mut g, [x1, y1], &params);
                        } else {
                            stroke::stroke_contour(&[[x1, y1], [x2, y2]], false, &params, &mut g);
                        }
                    } else {
                        self.lines.clear();
                        scene.flatten_shape(&shape, TOL / max_s, &mut self.lines);
                        stroke::stroke_polylines(&self.lines, &params, &mut g);
                    }
                }
                Op::Image { image, src, dst, smooth } => {
                    let img = &scene.images[image as usize];
                    let (tex, uvo) = self.image_slot(img)?;
                    let paint = self.image_paint(cmd, z, smooth);
                    self.frame.begin(key, scissor, Some(tex), nshapes);
                    let m = cmd.transform;
                    let u0 = uvo[0] + src[0] * uvo[2];
                    let v0 = uvo[1] + src[1] * uvo[3];
                    let u1 = uvo[0] + (src[0] + src[2]) * uvo[2];
                    let v1 = uvo[1] + (src[1] + src[3]) * uvo[3];
                    let corners = [
                        ([dst[0], dst[1]], [u0, v0]),
                        ([dst[0] + dst[2], dst[1]], [u1, v0]),
                        ([dst[0] + dst[2], dst[1] + dst[3]], [u1, v1]),
                        ([dst[0], dst[1] + dst[3]], [u0, v1]),
                    ];
                    let base = self.frame.verts.len() as u32;
                    for (p, uv) in corners {
                        self.frame.verts.push(Vertex { pos: map_f32(&m, p[0], p[1]), uv, paint });
                    }
                    self.frame.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                Op::Glyphs { start, end, px } => {
                    self.glyph_run(scene, cmd, z, key, scissor, nshapes, start, end, px)?;
                }
            }
            self.frame.finish();
        }
        self.stats.vertices = self.frame.verts.len() as u32;
        self.stats.indices = self.frame.idx.len() as u32;
        self.stats.commands = scene.cmds.len() as u32;
        Ok(())
    }

    fn fill_shape(&mut self, scene: &Scene, shape: &Shape, rule: FillRule, m: [f32; 6], paint: u32) {
        let t = Transform::from_f32(m);
        let max_s = t.max_scale().max(1e-9) as f32;
        let tol = TOL / max_s;
        let mut g = Geo { v: &mut self.frame.verts, i: &mut self.frame.idx, m, paint };
        match *shape {
            Shape::Rect { x, y, w, h } => {
                let a = g.vertex([x, y]);
                let b = g.vertex([x + w, y]);
                let c = g.vertex([x + w, y + h]);
                let d = g.vertex([x, y + h]);
                g.tri(a, b, c);
                g.tri(a, c, d);
            }
            Shape::Ellipse { cx, cy, rx, ry } => {
                let r = rx.max(ry);
                if r <= 0.0 {
                    return;
                }
                let step = if r <= tol {
                    std::f32::consts::FRAC_PI_2
                } else {
                    (2.0 * (1.0 - tol / r).acos()).clamp(0.02, 1.0)
                };
                let n = ((std::f32::consts::TAU / step).ceil() as u32).clamp(8, 1024);
                let c = g.vertex([cx, cy]);
                let first = g.vertex([cx + rx, cy]);
                let (sn, cs) = (std::f32::consts::TAU / n as f32).sin_cos();
                let (mut vx, mut vy) = (1.0f32, 0.0f32);
                let mut prev = first;
                for k in 1..n {
                    let nx = vx * cs - vy * sn;
                    vy = vx * sn + vy * cs;
                    vx = nx;
                    let cur = g.vertex([cx + rx * vx, cy + ry * vy]);
                    g.tri(c, prev, cur);
                    prev = cur;
                    let _ = k;
                }
                g.tri(c, prev, first);
            }
            Shape::Line { .. } => {}
            Shape::Path { .. } => {
                self.lines.clear();
                scene.flatten_shape(shape, tol, &mut self.lines);
                fill_polylines(&self.lines, rule, &mut g, &mut self.lyon, &mut self.lyon_vb);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn glyph_run(
        &mut self,
        scene: &Scene,
        cmd: &Command,
        z: f32,
        key: PipeKey,
        scissor: [u32; 4],
        nshapes: u32,
        start: u32,
        end: u32,
        px: f32,
    ) -> Result<(), Full> {
        let t = Transform::from_f32(cmd.transform);
        let s = t.scale_factor() as f32;
        let dev_px = px * s;
        if dev_px < 0.5 {
            return Ok(());
        }
        if dev_px > MAX_BITMAP_GLYPH_PX {
            // Large text: tessellate the outlines.
            let paint = self.paint_record(scene, cmd, z, 1.0, 0);
            self.frame.begin(key, scissor, None, nshapes);
            let mut lines = std::mem::take(&mut self.lines);
            lines.clear();
            for gp in &scene.glyphs[start as usize..end as usize] {
                let k = (gp.face, gp.glyph);
                if !self.outlines.contains_key(&k) {
                    let o = self.face(gp.face).and_then(|f| text::glyph_outline_em(&f, gp.glyph));
                    self.outlines.insert(k, o);
                }
                if let Some(o) = &self.outlines[&k] {
                    let gt = Transform::new(px as f64, 0.0, 0.0, px as f64, gp.x as f64, gp.y as f64);
                    let tol = TOL / t.max_scale().max(1e-9) as f32;
                    lines.flatten(o.transformed(&gt).as_ref(), tol);
                }
            }
            let mut g = Geo { v: &mut self.frame.verts, i: &mut self.frame.idx, m: cmd.transform, paint };
            fill_polylines(&lines, FillRule::Winding, &mut g, &mut self.lyon, &mut self.lyon_vb);
            self.lines = lines;
            return Ok(());
        }
        let q = ((dev_px * 4.0).round() / 4.0).max(0.25);
        let qkey = (q * 4.0) as u16;
        let axis = t.m12 == 0.0 && t.m21 == 0.0 && t.m11 > 0.0 && t.m22 > 0.0 && (t.m11 - t.m22).abs() < 1e-4;
        let paint = self.paint_record(scene, cmd, z, 1.0, 16);
        self.frame.begin(key, scissor, None, nshapes);
        let gsize = self.glyphs.size as f32;
        for gp in &scene.glyphs[start as usize..end as usize] {
            let (dx, dy) = t.map(gp.x as f64, gp.y as f64);
            let (bin, ix, iy) = if axis {
                let fx = dx - dx.floor();
                let bin = ((fx * 4.0).floor() as u8).min(3);
                (bin, dx.floor() as f32, dy.round() as f32)
            } else {
                (0, 0.0, 0.0)
            };
            let Some(slot) = self.glyph_slot(gp.face, gp.glyph, q, qkey, bin)? else { continue };
            let (u0, v0) = (slot.x as f32 / gsize, slot.y as f32 / gsize);
            let (u1, v1) = ((slot.x + slot.w) as f32 / gsize, (slot.y + slot.h) as f32 / gsize);
            let base = self.frame.verts.len() as u32;
            if axis {
                let x0 = ix + slot.left as f32;
                let y0 = iy + slot.top as f32;
                let (x1, y1) = (x0 + slot.w as f32, y0 + slot.h as f32);
                for (p, uv) in [([x0, y0], [u0, v0]), ([x1, y0], [u1, v0]), ([x1, y1], [u1, v1]), ([x0, y1], [u0, v1])]
                {
                    self.frame.verts.push(Vertex { pos: p, uv, paint });
                }
            } else {
                let k = px / q;
                let lx0 = gp.x + slot.left as f32 * k;
                let ly0 = gp.y + slot.top as f32 * k;
                let (lx1, ly1) = (lx0 + slot.w as f32 * k, ly0 + slot.h as f32 * k);
                let m = cmd.transform;
                for (p, uv) in
                    [([lx0, ly0], [u0, v0]), ([lx1, ly0], [u1, v0]), ([lx1, ly1], [u1, v1]), ([lx0, ly1], [u0, v1])]
                {
                    self.frame.verts.push(Vertex { pos: map_f32(&m, p[0], p[1]), uv, paint });
                }
            }
            self.frame.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Ok(())
    }

    fn glyph_slot(&mut self, face: FaceId, glyph: u16, q: f32, qkey: u16, bin: u8) -> Result<Option<GlyphSlot>, Full> {
        let key = (face, glyph, qkey, bin);
        if let Some(s) = self.glyphs.map.get(&key) {
            return Ok(*s);
        }
        let Some(f) = self.face(face) else {
            self.glyphs.map.insert(key, None);
            return Ok(None);
        };
        let Some(bm) = text::rasterize(&f, glyph, q, bin as f32 * 0.25, 0.0) else {
            self.glyphs.map.insert(key, None);
            return Ok(None);
        };
        let Some((x, y)) = self.glyphs.shelves.alloc(bm.width + 1, bm.height + 1) else {
            if self.glyphs.size >= GLYPH_ATLAS_MAX && self.glyphs.map.len() < 8 {
                // A single enormous glyph: give up on it.
                self.glyphs.map.insert(key, None);
                return Ok(None);
            }
            return Err(Full::Glyphs);
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.glyphs.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bm.coverage,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bm.width), rows_per_image: Some(bm.height) },
            wgpu::Extent3d { width: bm.width, height: bm.height, depth_or_array_layers: 1 },
        );
        let slot = GlyphSlot { x, y, w: bm.width, h: bm.height, left: bm.left, top: bm.top };
        self.glyphs.map.insert(key, Some(slot));
        Ok(Some(slot))
    }

    /// Texture and `(u_off, v_off, u_per_px, v_per_px)` for an image.
    fn image_slot(&mut self, img: &Image) -> Result<(TexKey, [f32; 4]), Full> {
        let now = self.frame_no;
        let (w, h) = img.size();
        if let Some(e) = self.entries.get_mut(&img.id())
            && e.w == w
            && e.h == h
        {
            if e.version != img.version() {
                e.version = img.version();
                let (tex, x, y) = (e.tex, e.x, e.y);
                e.last_used = now;
                self.write_image(tex, x, y, img);
            } else {
                e.last_used = now;
            }
            return Ok(self.uv_of(img.id()));
        }
        // New (or resized) image.
        let use_page = !self.force_dedicated && w + 2 <= ATLAS_MAX_IMAGE && h + 2 <= ATLAS_MAX_IMAGE;
        if use_page {
            let mut placed = None;
            for (i, p) in self.pages.iter_mut().enumerate() {
                if let Some((x, y)) = p.shelves.alloc(w + 2, h + 2) {
                    placed = Some((i, x, y));
                    break;
                }
            }
            if placed.is_none() && self.pages.len() < MAX_PAGES {
                self.pages.push(self.new_page());
                let i = self.pages.len() - 1;
                placed = self.pages[i].shelves.alloc(w + 2, h + 2).map(|(x, y)| (i, x, y));
            }
            let Some((i, x, y)) = placed else { return Err(Full::Images) };
            let tex = TexKey::Page(i as u32);
            self.entries.insert(img.id(), ImgEntry { version: img.version(), tex, x, y, w, h, last_used: now });
            self.write_image(tex, x, y, img);
        } else {
            let tex = TexKey::Dedicated(img.id());
            let need_new = self.dedicated.get(&img.id()).is_none_or(|d| d.w != w || d.h != h);
            if need_new {
                let d = self.new_dedicated(w, h);
                self.dedicated.insert(img.id(), d);
            }
            self.entries.insert(img.id(), ImgEntry { version: img.version(), tex, x: 0, y: 0, w, h, last_used: now });
            self.write_image(tex, 0, 0, img);
        }
        Ok(self.uv_of(img.id()))
    }

    fn uv_of(&self, id: u64) -> (TexKey, [f32; 4]) {
        let e = &self.entries[&id];
        match e.tex {
            TexKey::Page(_) => {
                let s = PAGE_SIZE as f32;
                (e.tex, [(e.x + 1) as f32 / s, (e.y + 1) as f32 / s, 1.0 / s, 1.0 / s])
            }
            TexKey::Dedicated(_) => (e.tex, [0.0, 0.0, 1.0 / e.w as f32, 1.0 / e.h as f32]),
        }
    }

    fn new_page(&self) -> Page {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dc-render image atlas"),
            size: wgpu::Extent3d { width: PAGE_SIZE, height: PAGE_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dc-render page bind"),
            layout: &self.bgl1,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        Page { tex, bind, shelves: Shelves::new(PAGE_SIZE, PAGE_SIZE) }
    }

    fn new_dedicated(&self, w: u32, h: u32) -> Dedicated {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dc-render image"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dc-render image bind"),
            layout: &self.bgl1,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        Dedicated { tex, bind, w, h }
    }

    fn write_image(&self, tex: TexKey, x: u32, y: u32, img: &Image) {
        let (w, h) = img.size();
        match tex {
            TexKey::Page(i) => {
                // 1-px replicated border so bilinear sampling never bleeds.
                let (pw, ph) = (w + 2, h + 2);
                let mut buf = vec![0u8; (pw * ph * 4) as usize];
                let src = img.data();
                for py in 0..ph {
                    let sy = (py as i64 - 1).clamp(0, h as i64 - 1) as usize;
                    for px in 0..pw {
                        let sx = (px as i64 - 1).clamp(0, w as i64 - 1) as usize;
                        let s = (sy * w as usize + sx) * 4;
                        let d = ((py * pw + px) * 4) as usize;
                        buf[d..d + 4].copy_from_slice(&src[s..s + 4]);
                    }
                }
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.pages[i as usize].tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &buf,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(pw * 4), rows_per_image: Some(ph) },
                    wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
                );
            }
            TexKey::Dedicated(id) => {
                let Some(d) = self.dedicated.get(&id) else { return };
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &d.tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    img.data(),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
            }
        }
    }
}

// ---------------------------------------------------------------- fills

fn signed_area(s: &[[f32; 2]]) -> f32 {
    let n = s.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (s[i], s[(i + 1) % n]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a * 0.5
}

/// Convex and winding once (so a triangle fan covers it exactly).
fn is_convex(s: &[[f32; 2]]) -> bool {
    let n = s.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    let mut flips = 0;
    let mut last_dx = 0.0f32;
    for i in 0..n {
        let a = s[i];
        let b = s[(i + 1) % n];
        let c = s[(i + 2) % n];
        let cr = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cr.abs() > 1e-7 {
            if sign == 0.0 {
                sign = cr.signum();
            } else if cr.signum() != sign {
                return false;
            }
        }
        let dx = b[0] - a[0];
        if dx != 0.0 {
            if last_dx != 0.0 && dx.signum() != last_dx.signum() {
                flips += 1;
            }
            last_dx = dx;
        }
    }
    // Count the wrap-around change too.
    let first_dx = (0..n).map(|i| s[(i + 1) % n][0] - s[i][0]).find(|d| *d != 0.0).unwrap_or(0.0);
    if first_dx != 0.0 && last_dx != 0.0 && first_dx.signum() != last_dx.signum() {
        flips += 1;
    }
    flips <= 2
}

fn bbox(s: &[[f32; 2]]) -> [f32; 4] {
    let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for p in s {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

fn fill_polylines(
    pl: &Polylines,
    rule: FillRule,
    out: &mut Geo<'_>,
    lyon: &mut lt::FillTessellator,
    vb: &mut lt::VertexBuffers<[f32; 2], u32>,
) {
    let contours: Vec<&[[f32; 2]]> = pl.contours.iter().map(|c| pl.contour(c)).filter(|s| s.len() >= 3).collect();
    if contours.is_empty() {
        return;
    }
    let fan_ok = if contours.len() == 1 {
        is_convex(contours[0])
    } else if contours.len() <= 64 && contours.iter().all(|c| is_convex(c)) {
        let same_orient = {
            let s0 = signed_area(contours[0]).signum();
            contours.iter().all(|c| signed_area(c).signum() == s0)
        };
        if rule == FillRule::Winding && same_orient {
            true
        } else {
            let boxes: Vec<[f32; 4]> = contours.iter().map(|c| bbox(c)).collect();
            let mut disjoint = true;
            'o: for i in 0..boxes.len() {
                for j in i + 1..boxes.len() {
                    let (a, b) = (boxes[i], boxes[j]);
                    if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                        disjoint = false;
                        break 'o;
                    }
                }
            }
            disjoint
        }
    } else {
        false
    };
    if fan_ok {
        for c in contours {
            let v0 = out.vertex(c[0]);
            let mut prev = out.vertex(c[1]);
            for p in &c[2..] {
                let cur = out.vertex(*p);
                out.tri(v0, prev, cur);
                prev = cur;
            }
        }
        return;
    }
    use lyon::path::PathEvent;
    use lyon::path::math::point;
    let p = |q: [f32; 2]| point(q[0], q[1]);
    let events = contours.iter().flat_map(|s| {
        let first = p(s[0]);
        let n = s.len();
        std::iter::once(PathEvent::Begin { at: first })
            .chain((1..n).map(move |k| PathEvent::Line { from: p(s[k - 1]), to: p(s[k]) }))
            .chain(std::iter::once(PathEvent::End { last: p(s[n - 1]), first, close: true }))
    });
    vb.vertices.clear();
    vb.indices.clear();
    let opts = lt::FillOptions::default().with_fill_rule(match rule {
        FillRule::OddEven => lt::FillRule::EvenOdd,
        FillRule::Winding => lt::FillRule::NonZero,
    });
    let r =
        lyon.tessellate(events, &opts, &mut lt::BuffersBuilder::new(vb, |v: lt::FillVertex| v.position().to_array()));
    if r.is_err() {
        return;
    }
    let base = out.v.len() as u32;
    for q in &vb.vertices {
        out.vertex(*q);
    }
    for t in vb.indices.as_chunks::<3>().0 {
        out.tri(base + t[0], base + t[1], base + t[2]);
    }
}

// ---------------------------------------------------------------- headless

/// A self-contained device + renderer that draws offscreen and reads back.
pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub renderer: GpuRenderer,
    pub adapter_info: wgpu::AdapterInfo,
    target: Option<(wgpu::Texture, wgpu::TextureView, (u32, u32))>,
}

impl Headless {
    /// `None` when no adapter (not even a software one) is available.
    pub fn new(sample_count: u32) -> Option<Self> {
        pollster::block_on(Self::new_async(sample_count))
    }

    async fn new_async(sample_count: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut adapter = None;
        for fallback in [false, true] {
            let opts = wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: fallback,
                compatible_surface: None,
                ..Default::default()
            };
            if let Ok(a) = instance.request_adapter(&opts).await {
                adapter = Some(a);
                break;
            }
        }
        let adapter = adapter?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("dc-render headless"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .ok()?;
        let renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, sample_count);
        Some(Self { adapter_info: adapter.get_info(), device, queue, renderer, target: None })
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if self.target.as_ref().is_some_and(|t| t.2 == (w, h)) {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dc-render headless target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.target = Some((tex, view, (w, h)));
    }

    /// Renders and waits for the GPU, without reading back (for timing).
    pub fn render_wait(&mut self, scene: &Scene, w: u32, h: u32, clear: Option<Color>) {
        self.ensure_target(w, h);
        let view = self.target.as_ref().unwrap().1.clone();
        let idx = self.renderer.render_inner(scene, &view, (w, h), clear);
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None });
    }

    /// Renders and reads the result back as a premultiplied image.
    pub fn render(&mut self, scene: &Scene, w: u32, h: u32, clear: Option<Color>) -> Image {
        if w == 0 || h == 0 {
            return Image::new(w, h);
        }
        self.ensure_target(w, h);
        let view = self.target.as_ref().unwrap().1.clone();
        self.renderer.render_inner(scene, &view, (w, h), clear);
        let row = (w * 4).next_multiple_of(256);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dc-render readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target.as_ref().unwrap().0,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let idx = self.queue.submit(Some(enc.finish()));
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None });
        let mut out = vec![0u8; (w * h * 4) as usize];
        {
            let Ok(data) = buf.slice(..).get_mapped_range() else { return Image::new(w, h) };
            for y in 0..h as usize {
                let s = y * row as usize;
                out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(&data[s..s + w as usize * 4]);
            }
        }
        buf.unmap();
        Image::from_premultiplied(w, h, out).unwrap()
    }
}
