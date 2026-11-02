//! The job cue (Rust-only): a spider's job reaches its label, and "Always
//! show jobs" (on by default) pins the badge of every spider with a job.

use dc_core::PyRandom;
use dc_creature::ApproxLabelMetrics;
use dc_manager::CreatureManager;
use dc_manager::paint::creature_draw;
use dc_paint::{CreatureDraw, CreatureLook, CreaturePainter, PaintSwitches};
use dc_render::{Color, CpuRenderer, RenderHint, Scene};
use serde_json::json;
use std::path::PathBuf;

const JOBS: [&str; 6] = ["none", "hunter", "builder", "guard", "scout", "webber"];

fn manager(tag: &str, settings: serde_json::Value) -> CreatureManager {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..");
    let scratch = std::env::temp_dir().join(format!("dc-manager-job-cue-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let slots: Vec<_> = JOBS
        .iter()
        .enumerate()
        .map(|(i, j)| json!({"model": "tarantula", "personality": "bold", "count": 1, "slot_id": format!("s{i}"), "job": j}))
        .collect();
    let preset = scratch.join("preset.json");
    std::fs::write(&preset, json!({"name": "jobs", "slots": slots, "settings": settings}).to_string()).unwrap();
    let mut rng = PyRandom::from_int(3);
    CreatureManager::new(&root, &preset, (1280.0, 720.0), Some(3), &scratch.join("state"), false, &mut rng).unwrap()
}

#[test]
fn each_job_reaches_the_label_and_the_switch_pins_it() {
    let mut m = manager("pins", json!({"flies": {"enabled": false}, "always_show_names": false}));
    assert!(m.always_show_jobs, "on by default");
    let mut jobs: Vec<&str> = m.creatures.iter().map(|c| c.job_id.as_str()).collect();
    jobs.sort_unstable();
    let mut want = JOBS.to_vec();
    want.sort_unstable();
    assert_eq!(jobs, want, "every job is in play");
    for c in &mut m.creatures {
        let look = CreatureLook::from_model_json(&c.model, true);
        let d = creature_draw(c, &look);
        assert_eq!(d.label.job_id, c.job_id);
        assert!(d.label.job_pinned);
        let job = c.job_id.clone();
        assert_eq!(c.has_job_icon(), job != "none", "{job}");
        assert_eq!(d.label_visible(false), c.label_visible(false), "{job}: paint and creature agree");
    }
    m.set_always_show_jobs(false);
    assert!(m.creatures.iter().all(|c| !c.force_show_job));
    assert_eq!(m.session_snapshot()["always_show_jobs"], json!(false));
}

/// The badge's own pixels: the spider rendered with the badge minus the
/// same spider without it, as `(x0, y0, x1, y1)`.
fn badge_pixels(d: &CreatureDraw, look: &CreatureLook) -> (f64, f64, f64, f64) {
    let (w, h) = (1280, 720);
    let render = |pinned: bool| {
        let mut d = d.clone();
        d.label.job_pinned = pinned;
        let mut scene = Scene::new();
        {
            let mut c = scene.canvas();
            c.set_render_hint(RenderHint::Antialiasing, true);
            CreaturePainter::new(PaintSwitches::default()).render(&mut c, &d, look, false, false);
        }
        CpuRenderer::new().render(&scene, w, h, Some(Color::TRANSPARENT))
    };
    let (on, off) = (render(true), render(false));
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if on.pixel_premultiplied(x, y) != off.pixel_premultiplied(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    assert!(x1 > x0, "the badge drew nothing");
    (x0 as f64, y0 as f64, x1 as f64, y1 as f64)
}

#[test]
fn the_badge_stays_inside_both_bounding_rects() {
    // Bars off (the badge alone shows the label) and the default bars on.
    for (tag, bars) in [("bare", false), ("bars", true)] {
        let mut m = manager(
            tag,
            json!({"flies": {"enabled": false}, "always_show_names": false, "always_show_levels": false,
                   "always_show_health": bars, "always_show_stamina": bars}),
        );
        for c in m.creatures.iter_mut().filter(|c| c.job_id != "none") {
            let look = CreatureLook::from_model_json(&c.model, true);
            let d = creature_draw(c, &look);
            let job = c.job_id.clone();
            let badge = badge_pixels(&d, &look);
            let rects =
                [("dc-paint", d.bounding_rect(false)), ("dc-creature", c.bounding_rect(false, &ApproxLabelMetrics))];
            for (who, (x0, y0, x1, y1)) in rects {
                assert!(
                    x0 <= badge.0 && y0 <= badge.1 && badge.2 <= x1 && badge.3 <= y1,
                    "{tag} {job}: badge {badge:?} outside the {who} rect {:?}",
                    (x0, y0, x1, y1)
                );
            }
        }
    }
}

#[test]
fn a_preset_can_turn_the_switch_off() {
    let m = manager("off", json!({"flies": {"enabled": false}, "always_show_jobs": false}));
    assert!(!m.always_show_jobs);
    assert!(m.creatures.iter().all(|c| !c.force_show_job));
}
