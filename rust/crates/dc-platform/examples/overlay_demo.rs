//! Opens the transparent overlay over the whole virtual desktop and draws a
//! moving, semi-transparent circle with wgpu (or, with `--software` or when
//! DirectComposition transparency is unavailable, through
//! `UpdateLayeredWindow`). The window is click-through everywhere except
//! over the circle. Never moves the mouse.
//!
//! ```text
//! cargo run -p dc-platform --example overlay_demo -- --seconds 5 [--software] [--verify]
//! ```
//!
//! `--verify` parks the circle for a moment, then checks from outside the
//! renderer that it really is on screen and transparent around it
//! (`BitBlt` of the composed desktop), and that hit-testing answers
//! `HTCLIENT` only over the circle.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dc_platform::desktop_capture::grab_screen;
use dc_platform::gpu::{GpuError, OverlayGpu};
use dc_platform::monitors::{monitors, primary_index};
use dc_platform::overlay::{OverlayBackend, OverlayConfig, OverlayMode, OverlayWindow};
use dc_platform::overlay_win32::{WS_EX_LAYERED, WS_EX_TRANSPARENT};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, SendMessageW, WM_NCHITTEST, WindowFromPoint};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const SHADER: &str = r#"
struct U { center: vec2<f32>, radius: f32, alpha: f32, color: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let d = distance(pos.xy, u.center);
    let a = u.alpha * clamp(u.radius - d + 0.5, 0.0, 1.0);
    // Premultiplied output for CompositeAlphaMode::PreMultiplied.
    return vec4<f32>(u.color.rgb * a, a);
}
"#;

const COLOR: [f32; 3] = [0.95, 0.35, 0.10];
const ALPHA: f32 = 0.6;
const RADIUS: f64 = 70.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    center: [f32; 2],
    radius: f32,
    alpha: f32,
    color: [f32; 4],
}

struct GpuDraw {
    gpu: OverlayGpu,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl GpuDraw {
    fn new(gpu: OverlayGpu) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("circle"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("circle"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        Self { gpu, pipeline, uniforms, bind_group }
    }

    fn draw(&self, cx: f64, cy: f64) {
        let Some(frame) = self.gpu.acquire() else { return };
        let u = Uniforms {
            center: [cx as f32, cy as f32],
            radius: RADIUS as f32,
            alpha: ALPHA,
            color: [COLOR[0], COLOR[1], COLOR[2], 1.0],
        };
        self.gpu.queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&u));
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.queue.present(frame);
    }
}

/// CPU fallback: a premultiplied RGBA buffer the size of the overlay,
/// redrawn only around the circle.
struct SoftDraw {
    w: u32,
    h: u32,
    pixels: Vec<u8>,
    last: Option<(i64, i64, i64, i64)>,
}

impl SoftDraw {
    fn new(w: u32, h: u32) -> Self {
        Self { w, h, pixels: vec![0; w as usize * h as usize * 4], last: None }
    }

    fn clear_box(&mut self, b: (i64, i64, i64, i64)) {
        for y in b.1.max(0)..b.3.min(self.h as i64) {
            for x in b.0.max(0)..b.2.min(self.w as i64) {
                let i = (y as usize * self.w as usize + x as usize) * 4;
                self.pixels[i..i + 4].fill(0);
            }
        }
    }

    fn draw(&mut self, overlay: &OverlayWindow, cx: f64, cy: f64) {
        if let Some(b) = self.last.take() {
            self.clear_box(b);
        }
        let r = RADIUS + 2.0;
        let b = ((cx - r) as i64, (cy - r) as i64, (cx + r) as i64 + 1, (cy + r) as i64 + 1);
        for y in b.1.max(0)..b.3.min(self.h as i64) {
            for x in b.0.max(0)..b.2.min(self.w as i64) {
                let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                let a = ALPHA as f64 * (RADIUS - d + 0.5).clamp(0.0, 1.0);
                let i = (y as usize * self.w as usize + x as usize) * 4;
                self.pixels[i] = (COLOR[0] as f64 * a * 255.0).round() as u8;
                self.pixels[i + 1] = (COLOR[1] as f64 * a * 255.0).round() as u8;
                self.pixels[i + 2] = (COLOR[2] as f64 * a * 255.0).round() as u8;
                self.pixels[i + 3] = (a * 255.0).round() as u8;
            }
        }
        self.last = Some(b);
        overlay.present_premultiplied(&self.pixels, self.w, self.h);
    }
}

enum Drawer {
    Gpu(Box<GpuDraw>),
    Soft(SoftDraw),
}

struct Verify {
    /// Screen pixels before the overlay showed: circle centre, far point.
    before: Option<([u8; 4], [u8; 4])>,
    done: bool,
    passed: bool,
}

struct App {
    seconds: f64,
    software: bool,
    verify: Option<Verify>,
    start: Instant,
    overlay: Option<OverlayWindow>,
    drawer: Option<Drawer>,
    circle: Rc<Cell<(f64, f64)>>,
    /// Primary monitor, overlay-local: (x, y, w, h).
    area: (f64, f64, f64, f64),
    frames: u64,
    error: Option<String>,
}

impl App {
    /// Where the circle is at `t` seconds; parked at the area's centre
    /// between 0.8 s and 1.6 s when verifying.
    fn position(&self, t: f64) -> (f64, f64) {
        let (x, y, w, h) = self.area;
        let park = (x + w * 0.5, y + h * 0.5);
        if self.verify.is_some() && (0.8..1.6).contains(&t) {
            return park;
        }
        let s = if self.verify.is_some() && t >= 1.6 { t - 0.8 } else { t };
        (park.0 + (w * 0.3) * (s * 1.3).sin(), park.1 + (h * 0.25) * (s * 1.7).sin())
    }

    fn sample_points(&self) -> ((i32, i32), (i32, i32)) {
        let g = self.overlay.as_ref().map(|o| o.geometry()).unwrap_or(dc_core::IRect::new(0, 0, 0, 0));
        let (x, y, w, h) = self.area;
        let c = ((g.x as f64 + x + w * 0.5) as i32, (g.y as f64 + y + h * 0.5) as i32);
        (c, (c.0 - 300, c.1 - 220))
    }

    fn create(&mut self, el: &ActiveEventLoop, backend: OverlayBackend) -> Result<(), String> {
        let overlay = OverlayWindow::create(el, &OverlayConfig::new(OverlayMode::Companion, backend))
            .map_err(|e| e.to_string())?;
        let circle = self.circle.clone();
        overlay.set_hit_test(move |x, y| {
            let (cx, cy) = circle.get();
            ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= RADIUS
        });
        let g = overlay.geometry();
        let drawer = match backend {
            OverlayBackend::Composition => match OverlayGpu::new(overlay.window().clone(), (g.w as u32, g.h as u32)) {
                Ok(gpu) => {
                    println!(
                        "backend: composition (DX12 + DirectComposition), adapter {:?}, format {:?}, alpha {:?}, offered {:?}",
                        gpu.adapter.get_info().name,
                        gpu.config.format,
                        gpu.config.alpha_mode,
                        gpu.alpha_modes
                    );
                    Drawer::Gpu(Box::new(GpuDraw::new(gpu)))
                }
                Err(e @ GpuError::NoTransparency(_)) | Err(e @ GpuError::Adapter(_)) => {
                    return Err(format!("gpu: {e}"));
                }
                Err(e) => return Err(format!("gpu: {e}")),
            },
            OverlayBackend::Layered => {
                println!("backend: layered (software UpdateLayeredWindow)");
                Drawer::Soft(SoftDraw::new(g.w as u32, g.h as u32))
            }
        };
        self.overlay = Some(overlay);
        self.drawer = Some(drawer);
        Ok(())
    }

    fn run_checks(&mut self) {
        let Some(overlay) = self.overlay.as_ref() else { return };
        let Some(v) = self.verify.as_mut() else { return };
        if v.done {
            return;
        }
        v.done = true;
        let g = overlay.geometry();
        let (cx, cy) = self.circle.get();
        let centre = (g.x + cx as i32, g.y + cy as i32);
        let far = (centre.0 - 300, centre.1 - 220);
        let mut ok = true;

        // 1. What the compositor shows.
        let now_c = grab_screen(centre.0, centre.1, 1, 1).map(|i| i.pixel(0, 0));
        let now_f = grab_screen(far.0, far.1, 1, 1).map(|i| i.pixel(0, 0));
        if let (Some((bc, bf)), Some(nc), Some(nf)) = (v.before, now_c, now_f) {
            let expect: Vec<f64> =
                (0..3).map(|k| COLOR[k] as f64 * ALPHA as f64 * 255.0 + bc[k] as f64 * (1.0 - ALPHA as f64)).collect();
            let blended = (0..3).all(|k| (nc[k] as f64 - expect[k]).abs() <= 14.0);
            let untouched = (0..3).all(|k| (nf[k] as i32 - bf[k] as i32).abs() <= 2);
            println!(
                "screen @circle: before {:?} now {:?} expected ~{:?} -> {}",
                &bc[..3],
                &nc[..3],
                expect.iter().map(|e| e.round() as i32).collect::<Vec<_>>(),
                if blended { "semi-transparent circle visible" } else { "MISMATCH" }
            );
            println!(
                "screen @far:    before {:?} now {:?} -> {}",
                &bf[..3],
                &nf[..3],
                if untouched { "desktop shows through" } else { "MISMATCH (or the desktop changed)" }
            );
            ok &= blended && untouched;
        } else {
            println!("screen capture unavailable");
            ok = false;
        }

        // 2. Hit-testing.
        let hwnd = overlay.hwnd();
        let lp = |p: (i32, i32)| LPARAM(((p.0 as u32 & 0xFFFF) | ((p.1 as u32 & 0xFFFF) << 16)) as isize);
        let ht_c = unsafe { SendMessageW(hwnd, WM_NCHITTEST, Some(WPARAM(0)), Some(lp(centre))) }.0;
        let ht_f = unsafe { SendMessageW(hwnd, WM_NCHITTEST, Some(WPARAM(0)), Some(lp(far))) }.0;
        println!("WM_NCHITTEST circle={ht_c} (HTCLIENT=1) far={ht_f} (HTTRANSPARENT=-1)");
        ok &= ht_c == 1 && ht_f == -1;

        // 3. Whole-window transparency from the DC-79 timer, and what
        //    Windows would deliver a click at the far point to.
        let style = dc_platform::overlay::current_ex_style(hwnd);
        let over_circle = overlay
            .cursor_local()
            .map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= RADIUS)
            .unwrap_or(false);
        println!(
            "ex style {style:#010x}: layered={} transparent={} (cursor over circle: {over_circle})",
            style & WS_EX_LAYERED != 0,
            style & WS_EX_TRANSPARENT != 0
        );
        ok &= style & WS_EX_LAYERED != 0 && (style & WS_EX_TRANSPARENT != 0) == !over_circle;
        let under = unsafe { GetAncestor(WindowFromPoint(POINT { x: far.0, y: far.1 }), GA_ROOT) };
        println!("WindowFromPoint(far) is the overlay: {}", under == hwnd);
        ok &= under != hwnd;
        v.passed = ok;
        println!("verify: {}", if ok { "PASS" } else { "FAIL" });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }
        let mons = monitors();
        let virt = dc_platform::monitors::virtual_screen_geometry();
        let p = &mons[primary_index(&mons)];
        self.area = ((p.rect.x - virt.x) as f64, (p.rect.y - virt.y) as f64, p.rect.w as f64, p.rect.h as f64);
        self.circle.set(self.position(0.0));
        if let Some(v) = self.verify.as_mut() {
            let (cx, cy) =
                (virt.x as f64 + self.area.0 + self.area.2 * 0.5, virt.y as f64 + self.area.1 + self.area.3 * 0.5);
            let c = grab_screen(cx as i32, cy as i32, 1, 1).map(|i| i.pixel(0, 0));
            let f = grab_screen(cx as i32 - 300, cy as i32 - 220, 1, 1).map(|i| i.pixel(0, 0));
            v.before = c.zip(f);
        }
        let first = if self.software { OverlayBackend::Layered } else { OverlayBackend::Composition };
        if let Err(e) = self.create(el, first) {
            println!("{e}; falling back to the software overlay");
            self.overlay = None;
            if let Err(e) = self.create(el, OverlayBackend::Layered) {
                self.error = Some(e);
                el.exit();
                return;
            }
        }
        self.start = Instant::now();
        let _ = self.sample_points();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                let (cx, cy) = self.circle.get();
                match (self.drawer.as_mut(), self.overlay.as_ref()) {
                    (Some(Drawer::Gpu(d)), _) => d.draw(cx, cy),
                    (Some(Drawer::Soft(d)), Some(o)) => d.draw(o, cx, cy),
                    _ => {}
                }
                self.frames += 1;
            }
            WindowEvent::Resized(size) => {
                if let Some(Drawer::Gpu(d)) = self.drawer.as_mut() {
                    d.gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::CloseRequested => el.exit(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let t = self.start.elapsed().as_secs_f64();
        if t >= self.seconds {
            el.exit();
            return;
        }
        if self.verify.is_some() && t >= 1.45 {
            self.run_checks();
        }
        self.circle.set(self.position(t));
        if let Some(o) = self.overlay.as_ref() {
            for e in o.drain_events() {
                println!("overlay event: {e:?}");
            }
            o.window().request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(5.0);
    let mut app = App {
        seconds,
        software: args.iter().any(|a| a == "--software"),
        verify: args.iter().any(|a| a == "--verify").then_some(Verify { before: None, done: false, passed: false }),
        start: Instant::now(),
        overlay: None,
        drawer: None,
        circle: Rc::new(Cell::new((0.0, 0.0))),
        area: (0.0, 0.0, 800.0, 600.0),
        frames: 0,
        error: None,
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut app).expect("event loop failed");
    let elapsed = app.start.elapsed().as_secs_f64();
    println!("frames: {} in {:.2} s ({:.1} fps)", app.frames, elapsed, app.frames as f64 / elapsed.max(0.001));
    // Drop the GPU objects before the window, then the window.
    app.drawer = None;
    app.overlay = None;
    if let Some(e) = app.error {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    if let Some(v) = app.verify {
        std::process::exit(if v.passed { 0 } else { 2 });
    }
}
