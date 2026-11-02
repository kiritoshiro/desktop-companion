//! The job icon (Rust-only): every job but `"none"` gets its own icon in the
//! name label, `"none"` gets none, and "Always show jobs" turns a hidden
//! name into a small badge. The CPU and GPU backends draw it alike.

use dc_content::jobs::JOB_IDS;
use dc_core::Color;
use dc_paint::job_icon::{draw_job_icon, has_job_icon};
use dc_paint::{CreatureDraw, CreatureLook, CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Headless, Image, RenderHint, Scene};
use serde_json::Value;

const W: u32 = 320;
const H: u32 = 260;

fn case() -> (CreatureDraw, CreatureLook) {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/reference/rest__tarantula.json");
    let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut draw: CreatureDraw = serde_json::from_value(case["draw"].clone()).unwrap();
    draw.label.name = "Ada".into();
    draw.label.display_name = "Ada".into();
    (draw, CreatureLook::from_model_json(&case["model"], true))
}

/// The spider with `job`, its name shown or not, through the full painter.
fn render(job: &str, names: bool, pinned: bool) -> Image {
    let (mut d, look) = case();
    d.label.job_id = job.into();
    d.label.job_pinned = pinned;
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        c.translate(W as f64 * 0.5 - d.x, H as f64 * 0.7 - d.y);
        CreaturePainter::new(PaintSwitches::default()).render(&mut c, &d, &look, names, false);
    }
    CpuRenderer::new().render(&scene, W, H, Some(Color::TRANSPARENT))
}

fn label_visible(job: &str, names: bool, pinned: bool) -> bool {
    let (mut d, _) = case();
    d.label.job_id = job.into();
    d.label.job_pinned = pinned;
    d.label_visible(names)
}

#[test]
fn every_job_but_none_has_an_icon() {
    for job in JOB_IDS {
        assert_eq!(has_job_icon(job), job != "none", "{job}");
    }
    assert!(!has_job_icon("unknown"));
}

#[test]
fn each_job_gets_its_own_cue_in_the_label_and_none_gets_none() {
    let plain = render("none", true, true);
    assert_eq!(plain.data(), render("unknown", true, true).data(), "an unknown job draws like none");
    let mut seen: Vec<(&str, Image)> = Vec::new();
    for job in JOB_IDS.iter().copied().filter(|j| *j != "none") {
        let img = render(job, true, false);
        assert_ne!(img.data(), plain.data(), "{job} has no cue");
        for (other, prev) in &seen {
            assert_ne!(img.data(), prev.data(), "{job} looks like {other}");
        }
        seen.push((job, img));
    }
}

#[test]
fn always_show_jobs_badges_a_hidden_name_only_with_a_job() {
    let hidden = render("none", false, false);
    for job in JOB_IDS.iter().copied().filter(|j| *j != "none") {
        assert!(!label_visible(job, false, false), "{job}: no badge with the switch off");
        assert!(label_visible(job, false, true), "{job}: the switch shows a badge");
        assert_eq!(render(job, false, false).data(), hidden.data(), "{job}: nothing drawn with the switch off");
        assert_ne!(render(job, false, true).data(), hidden.data(), "{job}: the badge is drawn");
    }
    assert!(!label_visible("none", false, true), "no job, no badge");
    assert_eq!(render("none", false, true).data(), hidden.data());
}

#[test]
fn the_label_box_grows_by_the_icon() {
    let (mut d, _) = case();
    // Long enough that the label, not the legs, sets the width.
    d.label.display_name = "Bartholomew the Patient Weaver".into();
    let (x0, _, x1, _) = d.bounding_rect(true);
    d.label.job_id = "guard".into();
    let (g0, _, g1, _) = d.bounding_rect(true);
    assert!(g1 - g0 > x1 - x0, "the bounding rect leaves room for the icon");
}

#[test]
fn draw_job_icon_draws_nothing_for_none() {
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        assert!(!draw_job_icon(&mut c, "none", 10.0, 10.0, 6.0, Color::WHITE));
    }
    assert_eq!(scene.stats().commands, 0);
}

#[test]
fn gpu_draws_the_icons_like_cpu() {
    let Some(mut gpu) = Headless::new(4) else {
        eprintln!("no wgpu adapter available; skipping GPU test");
        return;
    };
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        for (i, job) in JOB_IDS.iter().enumerate() {
            draw_job_icon(&mut c, job, 20.0 + i as f64 * 40.0, 30.0, 14.0, Color::WHITE);
        }
    }
    let cpu = CpuRenderer::new().render(&scene, 260, 60, Some(Color::TRANSPARENT));
    let g = gpu.render(&scene, 260, 60, Some(Color::TRANSPARENT));
    let (mut sum, mut n) = (0u64, 0u64);
    for y in 0..cpu.height() {
        for x in 0..cpu.width() {
            let (a, b) = (cpu.pixel_premultiplied(x, y), g.pixel_premultiplied(x, y));
            sum += a.iter().zip(b.iter()).map(|(u, v)| u.abs_diff(*v) as u64).sum::<u64>();
            n += 4;
        }
    }
    let mean = sum as f64 / n as f64;
    assert!(mean < 2.0, "GPU icons differ from CPU: mean {mean:.3}");
}
