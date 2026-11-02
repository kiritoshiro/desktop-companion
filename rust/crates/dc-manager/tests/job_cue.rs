//! The job cue (Rust-only): a spider's job reaches its label, and "Always
//! show jobs" (on by default) pins the badge of every spider with a job.

use dc_core::PyRandom;
use dc_creature::ApproxLabelMetrics;
use dc_manager::CreatureManager;
use dc_manager::paint::creature_draw;
use dc_paint::CreatureLook;
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
        // Off: the bounding rect must cover the badge whenever it is drawn.
        if job != "none" {
            let (_, top_pinned, _, _) = c.bounding_rect(false, &ApproxLabelMetrics);
            assert!(c.label_visible(false), "{job}: badge pinned");
            c.force_show_job = false;
            let (_, top_plain, _, _) = c.bounding_rect(false, &ApproxLabelMetrics);
            c.force_show_job = true;
            assert!(top_pinned <= top_plain, "{job}: the badge is inside the dirty rect");
        }
    }
    m.set_always_show_jobs(false);
    assert!(m.creatures.iter().all(|c| !c.force_show_job));
    assert_eq!(m.session_snapshot()["always_show_jobs"], json!(false));
}

#[test]
fn a_preset_can_turn_the_switch_off() {
    let m = manager("off", json!({"flies": {"enabled": false}, "always_show_jobs": false}));
    assert!(!m.always_show_jobs);
    assert!(m.creatures.iter().all(|c| !c.force_show_job));
}
