//! Records and renders 300 procedural spiders (radial-gradient bodies, eight
//! curved three-segment legs each) and prints ms per frame for recording,
//! the CPU backend and the GPU backend.
//!
//! `cargo run --release -p dc-render --example bench [spiders] [frames]`

use dc_render::{Color, CpuRenderer, Headless, Scene, demo};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let spiders: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let (w, h) = (1920u32, 1080u32);

    let mut scene = Scene::new();
    // Warm up buffers, then time recording.
    for _ in 0..3 {
        scene.clear();
        demo::spider_field(&mut scene.canvas(), spiders, w as f64, h as f64, 1);
    }
    let t = Instant::now();
    for f in 0..frames {
        scene.clear();
        demo::spider_field(&mut scene.canvas(), spiders, w as f64, h as f64, 1 + (f as u64 % 2));
    }
    let record_ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let stats = scene.stats();
    println!(
        "scene: {spiders} spiders, {} commands ({} fills, {} strokes), {:.0} commands/spider",
        stats.commands,
        stats.fills,
        stats.strokes,
        stats.commands as f64 / spiders as f64
    );
    println!("record : {record_ms:7.3} ms/frame");

    let mut cpu = CpuRenderer::new();
    let mut img = dc_render::Image::new(w, h);
    let cpu_frames = frames.clamp(1, 10);
    cpu.render_into(&scene, &mut img, Some(Color::TRANSPARENT));
    let t = Instant::now();
    for _ in 0..cpu_frames {
        cpu.render_into(&scene, &mut img, Some(Color::TRANSPARENT));
    }
    let cpu_ms = t.elapsed().as_secs_f64() * 1000.0 / cpu_frames as f64;
    println!("cpu    : {cpu_ms:7.3} ms/frame (tiny-skia, {w}x{h})");

    match Headless::new(4) {
        None => println!("gpu    : no adapter available"),
        Some(mut gpu) => {
            for _ in 0..5 {
                gpu.render_wait(&scene, w, h, Some(Color::TRANSPARENT));
            }
            let t = Instant::now();
            let mut prep = 0u64;
            for _ in 0..frames {
                gpu.render_wait(&scene, w, h, Some(Color::TRANSPARENT));
                prep += gpu.renderer.stats().prepare_us as u64;
            }
            let gpu_ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
            let s = gpu.renderer.stats();
            println!(
                "gpu    : {gpu_ms:7.3} ms/frame incl. wait ({:.3} ms CPU tessellation+upload), {} draw calls, {} vertices, 4x MSAA on {}",
                prep as f64 / 1000.0 / frames as f64,
                s.draw_calls,
                s.vertices,
                gpu.adapter_info.name
            );
            println!("total  : {:7.3} ms/frame record + GPU", record_ms + gpu_ms);
        }
    }
}
