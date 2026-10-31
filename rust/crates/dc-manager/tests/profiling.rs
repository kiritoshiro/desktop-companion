//! `check_manager_records_its_systems` from `tests/test_profiling.py`: the
//! instrumentation must be in the manager, not only in the profiler. Testing
//! the profiler alone would pass with every section deleted from `update`.

use dc_core::PyRandom;
use dc_core::profiling;
use dc_manager::paint::ScenePainter;
use dc_manager::{CreatureManager, MouseInput};
use dc_paint::PaintSwitches;
use dc_render::Scene;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

#[test]
fn manager_records_its_systems() {
    let state = std::env::temp_dir().join(format!("dc-manager-profiling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let mut rng = PyRandom::from_int(4);
    let preset = root().join("presets").join("colony.json");
    let mut m = CreatureManager::new(&root(), &preset, (1600.0, 900.0), Some(4), &state, false, &mut rng).unwrap();
    let idle = MouseInput::default();

    profiling::start_profiling(30);
    for _ in 0..12 {
        let _frame = profiling::frame();
        m.update(1.0 / 60.0, -100000.0, -100000.0, idle, &mut rng);
    }
    // Painting, after the frame as in the overlay.
    ScenePainter::new(PaintSwitches::default()).paint(&mut m, &mut Scene::new().canvas(), None);
    let p = profiling::stop_profiling().unwrap();

    let names = p.names();
    for name in
        ["frame", "creatures", "behaviour", "jobs", "webs", "mouse-webs", "flies", "desktop", "combat", "render"]
    {
        assert!(names.contains(&name), "the manager never timed {name}: {names:?}");
    }
    // Simulating four spiders has to cost something, or the clock is not running.
    assert!(p.stats("creatures").0 > 0.0, "the creature section recorded no time");
    assert!(p.stats("frame").0 >= p.stats("creatures").0, "a frame cannot be cheaper than a system inside it");
    assert_eq!(p.frames, 12);

    // And with profiling off, the same update records nothing at all.
    for _ in 0..4 {
        let _frame = profiling::frame();
        m.update(1.0 / 60.0, -100000.0, -100000.0, idle, &mut rng);
    }
    assert!(!profiling::enabled());
    assert!(profiling::hud_lines().is_empty());
    let _ = std::fs::remove_dir_all(&state);
}
