//! Renders a crude spider plus some text with both backends and writes
//! `spider_cpu.png` / `spider_gpu.png` to the current directory (or the
//! directory given as the first argument).

use dc_render::{Color, CpuRenderer, Font, Headless, Pen, Rect, Scene, TextFlags, Weight, demo};
use std::path::PathBuf;

fn main() {
    let dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.fill_rect(Rect::new(0.0, 0.0, 256.0, 256.0), Color::rgb(235, 230, 220));
        demo::spider_field(&mut c, 0, 256.0, 256.0, 1);
        demo::draw_spider(&mut c, 128.0, 120.0, 70.0, -90.0, 0.4, 25.0);
        c.set_pen(Pen::new(Color::rgb(40, 30, 20), 1.0));
        c.set_font(Font::with_weight("Segoe UI", 12.0, Weight::BOLD));
        c.draw_text_in(Rect::new(0.0, 214.0, 256.0, 36.0), TextFlags::ALIGN_CENTER, "Desktop Bug Companion");
    }
    let cpu = CpuRenderer::new().render(&scene, 256, 256, Some(Color::TRANSPARENT));
    cpu.save_png(dir.join("spider_cpu.png")).expect("write png");
    println!("wrote {}", dir.join("spider_cpu.png").display());
    if let Some(mut gpu) = Headless::new(4) {
        let img = gpu.render(&scene, 256, 256, Some(Color::TRANSPARENT));
        img.save_png(dir.join("spider_gpu.png")).expect("write png");
        println!("wrote {} ({})", dir.join("spider_gpu.png").display(), gpu.adapter_info.name);
    }
}
