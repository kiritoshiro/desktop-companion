//! Backend regression tests: a CPU fixture image and CPU-vs-GPU agreement.
//!
//! Re-bless the CPU fixture with `DC_RENDER_BLESS=1 cargo test -p dc-render`.

use dc_render::{Color, CpuRenderer, Font, Headless, Image, Pen, Rect, Scene, TextFlags, Transform, demo};
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

/// 150% about (40, -20): how the overlay draws its logical scene on a
/// scaled monitor whose corner is there.
const DPI: Transform = Transform::new(1.5, 0.0, 0.0, 1.5, 40.0 * (1.0 - 1.5), -20.0 * (1.0 - 1.5));
const BIG: (u32, u32) = (480, 360);

fn recorded_under(t: &Transform) -> Scene {
    let mut s = Scene::new();
    {
        let mut c = s.canvas();
        c.set_transform(t, false);
        demo::feature_scene(&mut c);
    }
    s
}

#[test]
fn an_appended_scene_draws_like_one_recorded_under_the_transform() {
    let mut cpu = CpuRenderer::new();
    let want = cpu.render(&recorded_under(&DPI), BIG.0, BIG.1, Some(Color::TRANSPARENT));
    let mut mapped = Scene::new();
    mapped.append_transformed(&feature_scene(), &DPI, None);
    let got = cpu.render(&mapped, BIG.0, BIG.1, Some(Color::TRANSPARENT));
    let d = diff(&want, &got, 3);
    eprintln!("appended vs recorded: mean {:.4} max {} over3 {:.5}", d.mean, d.max, d.over);
    // Only the dashes differ (they are cut at recording time, with the
    // recording scale's tolerance).
    assert!(d.mean < 0.05 && d.over < 0.002, "mean {} max {} over {}", d.mean, d.max, d.over);
    // The clip path and the cleared hole moved with the scene.
    assert_eq!(got.pixel(300, 20).a, want.pixel(300, 20).a);
    let (x, y) = DPI.map(200.0, 180.0);
    assert!(got.pixel(x as u32, y as u32).g > 100);

    // Through the identity: the same pixels as the scene itself.
    let mut same = Scene::new();
    same.append_transformed(&feature_scene(), &Transform::IDENTITY, None);
    let a = cpu.render(&feature_scene(), W, H, Some(Color::TRANSPARENT));
    let b = cpu.render(&same, W, H, Some(Color::TRANSPARENT));
    assert_eq!(a.data(), b.data());
}

#[test]
fn an_appended_scene_stays_inside_its_clip() {
    let mut cpu = CpuRenderer::new();
    let mut whole = Scene::new();
    whole.append_transformed(&feature_scene(), &DPI, None);
    let whole = cpu.render(&whole, BIG.0, BIG.1, Some(Color::TRANSPARENT));
    // Two monitors' worth: the left half at 150%, the right half through
    // the identity, each clipped to its own half.
    let mut halves = Scene::new();
    halves.append_transformed(&feature_scene(), &DPI, Some(Rect::new(0.0, 0.0, 240.0, 360.0)));
    halves.append_transformed(&feature_scene(), &Transform::IDENTITY, Some(Rect::new(240.0, 0.0, 240.0, 360.0)));
    let halves = cpu.render(&halves, BIG.0, BIG.1, Some(Color::TRANSPARENT));
    let plain = cpu.render(&feature_scene(), BIG.0, BIG.1, Some(Color::TRANSPARENT));
    // Drawing through a clip mask rounds a gradient a level differently.
    let near = |a: Color, b: Color| {
        a.r.abs_diff(b.r).max(a.g.abs_diff(b.g)).max(a.b.abs_diff(b.b)).max(a.a.abs_diff(b.a)) <= 2
    };
    for y in (0..BIG.1).step_by(7) {
        for x in (0..BIG.0).step_by(5) {
            let want = if x < 240 { whole.pixel(x, y) } else { plain.pixel(x, y) };
            assert!(near(halves.pixel(x, y), want), "({x}, {y}): {:?} vs {want:?}", halves.pixel(x, y));
        }
    }
    // Nothing of the 150% copy spills past its half: what the identity copy
    // leaves empty there stays empty.
    assert_eq!(plain.pixel(400, 300).a, 0);
    assert_eq!(halves.pixel(400, 300).a, 0);
    assert!(whole.pixel(400, 300).a > 0, "the scaled copy does reach there unclipped");
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
fn gpu_draws_appended_scenes_like_cpu() {
    let Some(mut gpu) = headless() else { return };
    let mut halves = Scene::new();
    halves.append_transformed(&feature_scene(), &DPI, Some(Rect::new(0.0, 0.0, 240.0, 360.0)));
    halves.append_transformed(&feature_scene(), &Transform::IDENTITY, Some(Rect::new(240.0, 0.0, 240.0, 360.0)));
    let cpu = CpuRenderer::new().render(&halves, BIG.0, BIG.1, Some(Color::TRANSPARENT));
    let g = gpu.render(&halves, BIG.0, BIG.1, Some(Color::TRANSPARENT));
    let d = diff(&cpu, &g, 48);
    eprintln!("appended cpu vs gpu: mean {:.3} max {} over48 {:.4}", d.mean, d.max, d.over);
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
