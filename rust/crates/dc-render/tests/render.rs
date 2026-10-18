//! Backend regression tests: a CPU fixture image and CPU-vs-GPU agreement.
//!
//! Re-bless the CPU fixture with `DC_RENDER_BLESS=1 cargo test -p dc-render`.

use dc_render::{Color, CpuRenderer, Font, Headless, Image, Pen, Rect, Scene, TextFlags, demo};
use std::path::PathBuf;

const W: u32 = 320;
const H: u32 = 240;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

fn feature_scene() -> Scene {
    let mut s = Scene::new();
    {
        let mut c = s.canvas();
        demo::feature_scene(&mut c);
    }
    s
}

struct Diff {
    mean: f64,
    max: u8,
    over: f64,
}

/// Mean absolute channel difference, max difference and the fraction of
/// pixels whose largest channel difference exceeds `limit`.
fn diff(a: &Image, b: &Image, limit: u8) -> Diff {
    assert_eq!(a.size(), b.size());
    let (mut sum, mut max, mut over) = (0u64, 0u8, 0usize);
    for (p, q) in a.data().as_chunks::<4>().0.iter().zip(b.data().as_chunks::<4>().0.iter()) {
        let mut m = 0u8;
        for k in 0..4 {
            let d = p[k].abs_diff(q[k]);
            sum += d as u64;
            m = m.max(d);
        }
        max = max.max(m);
        if m > limit {
            over += 1;
        }
    }
    let n = (a.width() * a.height()) as f64;
    Diff { mean: sum as f64 / (n * 4.0), max, over: over as f64 / n }
}

#[test]
fn cpu_regression_fixture() {
    let img = CpuRenderer::new().render(&feature_scene(), W, H, Some(Color::TRANSPARENT));
    let path = fixture("cpu_features.png");
    if std::env::var_os("DC_RENDER_BLESS").is_some() || !path.exists() {
        img.save_png(&path).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    let want = Image::load_png(&path).unwrap();
    let d = diff(&img, &want, 3);
    assert!(
        d.over < 0.001 && d.mean < 0.2,
        "CPU render drifted from fixture: mean {} max {} over {}",
        d.mean,
        d.max,
        d.over
    );
}

#[test]
fn cpu_draws_expected_pixels() {
    let img = CpuRenderer::new().render(&feature_scene(), W, H, Some(Color::TRANSPARENT));
    // Cleared hole is transparent, background is opaque.
    assert_eq!(img.pixel(300, 20).a, 0);
    assert_eq!(img.pixel(5, 5).a, 255);
    // The clipped checkerboard shows inside the circle, not outside it.
    let inside = img.pixel(200, 180);
    assert!(inside.g > 100, "{inside:?}");
}

fn headless() -> Option<Headless> {
    let h = Headless::new(4);
    if h.is_none() {
        eprintln!("no wgpu adapter available; skipping GPU test");
    }
    h
}

#[test]
fn gpu_matches_cpu() {
    let Some(mut gpu) = headless() else { return };
    let scene = feature_scene();
    let cpu = CpuRenderer::new().render(&scene, W, H, Some(Color::TRANSPARENT));
    let g = gpu.render(&scene, W, H, Some(Color::TRANSPARENT));
    if std::env::var_os("DC_RENDER_DUMP").is_some() {
        g.save_png(fixture("gpu_features_dump.png")).unwrap();
    }
    let d = diff(&cpu, &g, 48);
    eprintln!("cpu vs gpu ({}): mean {:.3} max {} over48 {:.4}", gpu.adapter_info.name, d.mean, d.max, d.over);
    assert!(d.mean < 2.0 && d.over < 0.01, "GPU differs from CPU: mean {} over {}", d.mean, d.over);
}

#[test]
fn gpu_text_and_batching() {
    let Some(mut gpu) = headless() else { return };
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        demo::spider_field(&mut c, 60, W as f64, H as f64, 7);
        c.set_pen(Pen::new(Color::WHITE, 1.0));
        c.set_font(Font::new("Segoe UI", 11.0));
        c.draw_text_in(Rect::new(0.0, 0.0, W as f64, 40.0), TextFlags::ALIGN_CENTER, "Reclaim the desktop");
    }
    let g = gpu.render(&scene, W, H, Some(Color::TRANSPARENT));
    let stats = gpu.renderer.stats();
    // Hundreds of commands, a couple of draw calls.
    assert!(stats.commands > 3000, "{stats:?}");
    assert!(stats.draw_calls <= 3, "{stats:?}");
    let cpu = CpuRenderer::new().render(&scene, W, H, Some(Color::TRANSPARENT));
    let d = diff(&cpu, &g, 64);
    eprintln!("spiders+text cpu vs gpu: mean {:.3} over64 {:.4}", d.mean, d.over);
    assert!(d.mean < 3.0 && d.over < 0.02);
    if dc_render::text::fonts_available() {
        // Some text pixels were drawn in the title band on both backends.
        let lit = |img: &Image| {
            (100..220).flat_map(|x| (10..30).map(move |y| (x, y))).filter(|&(x, y)| img.pixel(x, y).r > 200).count()
        };
        assert!(lit(&g) > 40 && lit(&cpu) > 40, "{} {}", lit(&g), lit(&cpu));
    }
}

#[test]
fn gpu_keeps_contents_without_clear() {
    let Some(mut gpu) = headless() else { return };
    let mut a = Scene::new();
    a.canvas().fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::rgb(255, 0, 0));
    let mut b = Scene::new();
    b.canvas().fill_rect(Rect::new(10.0, 0.0, 10.0, 10.0), Color::rgb(0, 0, 255));
    gpu.render(&a, 32, 16, Some(Color::TRANSPARENT));
    let img = gpu.render(&b, 32, 16, None);
    assert_eq!(img.pixel(5, 5), Color::rgb(255, 0, 0));
    assert_eq!(img.pixel(15, 5), Color::rgb(0, 0, 255));
    assert_eq!(img.pixel(25, 5).a, 0);
}
