//! Record and render 300 real spiders: the 62 shipped models in the pose
//! dumped by `tools/dump_reference.py`, tiled over a 1920x1080 overlay.
//! Prints ms per frame for recording (dc-paint → `Scene`), the GPU backend
//! (`Headless`, 4x MSAA, waits for the GPU) and the CPU backend.
//!
//! ```text
//! CARGO_TARGET_DIR=target-dc-paint cargo run --release -p dc-paint --example bench [spiders] [frames]
//! ```

use dc_core::Color;
use dc_paint::{CreatureDraw, CreatureLook, CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Headless, Image, RenderHint, Scene};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let spiders: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let (w, h) = (1920u32, 1080u32);
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target-review/reference");
    let index: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(reference.join("cases.json")).expect("run tools/dump_reference.py first"),
    )
    .unwrap();
    let mut models: Vec<(CreatureDraw, CreatureLook)> = Vec::new();
    for e in &index {
        let name = e["name"].as_str().unwrap();
        if !name.starts_with("rest__") {
            continue;
        }
        let case: Value =
            serde_json::from_str(&std::fs::read_to_string(reference.join(format!("{name}.json"))).unwrap()).unwrap();
        let draw: CreatureDraw = serde_json::from_value(case["draw"].clone()).unwrap();
        let look = CreatureLook::from_model_json(&case["model"], true);
        models.push((draw, look));
    }
    let sprite_count = models.iter().filter(|m| m.1.render_mode == dc_paint::RenderMode::SpriteRig).count();
    let cols = 20usize;
    let placements: Vec<(usize, f64, f64)> = (0..spiders)
        .map(|i| {
            let (draw, _) = &models[i % models.len()];
            let x = 48.0 + (i % cols) as f64 * (w as f64 - 96.0) / (cols - 1) as f64;
            let y = 60.0 + (i / cols) as f64 * 64.0 % (h as f64 - 100.0);
            (i % models.len(), x - draw.x, y - draw.y)
        })
        .collect();

    let mut painter = CreaturePainter::new(PaintSwitches::default());
    let mut scene = Scene::new();
    let record = |scene: &mut Scene, painter: &mut CreaturePainter| {
        scene.clear();
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        for &(m, dx, dy) in &placements {
            let (draw, look) = &models[m];
            c.save();
            c.translate(dx, dy);
            painter.render(&mut c, draw, look, false, false);
            c.restore();
        }
    };
    for _ in 0..3 {
        record(&mut scene, &mut painter);
    }
    let t = Instant::now();
    for _ in 0..frames {
        record(&mut scene, &mut painter);
    }
    let record_ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let stats = scene.stats();
    println!(
        "scene: {spiders} spiders ({} models, {sprite_count} sprite rigs), {} commands, {:.0} commands/spider",
        models.len(),
        stats.commands,
        stats.commands as f64 / spiders as f64
    );
    println!("record : {record_ms:7.3} ms/frame");

    match Headless::new(4) {
        None => println!("gpu    : no adapter available"),
        Some(mut gpu) => {
            for _ in 0..5 {
                gpu.render_wait(&scene, w, h, Some(Color::TRANSPARENT));
            }
            let t = Instant::now();
            for _ in 0..frames {
                gpu.render_wait(&scene, w, h, Some(Color::TRANSPARENT));
            }
            let gpu_ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
            let s = gpu.renderer.stats();
            println!("gpu    : {gpu_ms:7.3} ms/frame ({} draw calls, {} vertices)", s.draw_calls, s.vertices);
            let t = Instant::now();
            for _ in 0..frames {
                record(&mut scene, &mut painter);
                gpu.render_wait(&scene, w, h, Some(Color::TRANSPARENT));
            }
            let both = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
            println!("record + gpu : {both:7.3} ms/frame");
            let img = gpu.render(&scene, w, h, Some(Color::TRANSPARENT));
            let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target-review/bench_gpu.png");
            let _ = img.save_png(out);
        }
    }
    let mut cpu = CpuRenderer::new();
    let mut img = Image::new(w, h);
    let cpu_frames = frames.clamp(1, 5);
    cpu.render_into(&scene, &mut img, Some(Color::TRANSPARENT));
    let t = Instant::now();
    for _ in 0..cpu_frames {
        cpu.render_into(&scene, &mut img, Some(Color::TRANSPARENT));
    }
    println!("cpu    : {:7.3} ms/frame (tiny-skia)", t.elapsed().as_secs_f64() * 1000.0 / cpu_frames as f64);
}
