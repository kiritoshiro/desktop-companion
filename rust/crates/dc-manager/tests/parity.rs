//! Python-oracle parity for `CreatureManager`: replays the scenarios dumped
//! by `tools/dump_fixtures.py` (colony, fight, hunt, cages, stalk, duel,
//! surfaces; seeds 3, 7, 11) and compares every recorded frame's trace with
//! the unmodified Python manager.

use dc_core::PyRandom;
use dc_creature::{ApproxLabelMetrics, LabelMetrics};
use dc_manager::paint::ScenePainter;
use dc_manager::{CreatureManager, MouseInput};
use dc_paint::PaintSwitches;
use dc_paint::compare::{compare_images, diff_image};
use dc_render::{CpuRenderer, Image, RenderHint, Scene};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

const TOL: f64 = 1e-6;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn size_key(point_size: f64) -> i64 {
    (point_size * 1000.0).round() as i64
}

/// The oracle's Qt font metrics, read back from the fixture's `label` entries,
/// so a label measures exactly as wide as it did in Python. Unseen texts fall
/// back to the estimate.
#[derive(Clone, Default)]
struct OracleMetrics {
    advance: HashMap<(String, i64), f64>,
    height: HashMap<i64, f64>,
}

impl OracleMetrics {
    fn from_fixture(fx: &Value) -> Self {
        let mut m = Self::default();
        for frame in fx["frames"].as_array().unwrap() {
            for c in frame["state"]["creatures"].as_array().unwrap() {
                let l = c["label"].as_array().unwrap();
                let key = size_key(l[1].as_f64().unwrap());
                m.advance.insert((l[0].as_str().unwrap().to_string(), key), l[2].as_f64().unwrap());
                m.height.insert(key, l[3].as_f64().unwrap());
            }
        }
        m
    }
}

impl LabelMetrics for OracleMetrics {
    fn horizontal_advance(&self, text: &str, point_size: f64, demibold: bool) -> f64 {
        match self.advance.get(&(text.to_string(), size_key(point_size))) {
            Some(&a) if demibold => a,
            _ => ApproxLabelMetrics.horizontal_advance(text, point_size, demibold),
        }
    }
    fn height(&self, point_size: f64, demibold: bool) -> f64 {
        match self.height.get(&size_key(point_size)) {
            Some(&h) if demibold => h,
            _ => ApproxLabelMetrics.height(point_size, demibold),
        }
    }
}

/// The Rust side of `trace(m)` in the generator.
fn trace(m: &CreatureManager, metrics: &OracleMetrics) -> Value {
    let creatures: Vec<Value> = m
        .creatures
        .iter()
        .map(|c| {
            let foe = c.foe.and_then(|id| m.creature(id)).map(|f| f.progression_id.clone());
            let (text, size) = (c.label_text(), c.label_point_size());
            let prey = c.prey.as_ref().and_then(|p| m.fly_world.flies.iter().position(|f| f.id.0 == p.id));
            json!({
                "id": c.progression_id, "name": c.name, "x": c.kin.x, "y": c.kin.y,
                "heading": c.kin.heading, "state": c.state.as_str(), "hp": c.hp,
                "xp": c.progression.total_xp, "foe": foe, "prey": prey,
                "cage": c.cage.map(|r| r.id), "feed_cooldown": c.feed_cooldown,
                "prey_recheck": c.prey_recheck, "flee_timer": c.flee_timer,
                "attack_cooldown": c.attack_cooldown, "hovered": c.hovered,
                "hidden": c.desktop_fully_hidden, "hide_kind": c.desktop_hide_kind,
                "label": [text, size, metrics.horizontal_advance(&text, size, true), metrics.height(size, true)],
            })
        })
        .collect();
    json!({
        "creatures": creatures,
        "cages": m.cages.iter().map(|c| c.to_dict()).collect::<Vec<_>>(),
        "bases": m.base_world.to_dict(),
        "flies": m.fly_world.flies.iter().map(|f| json!({"x": f.x, "y": f.y, "state": f.state.as_str()})).collect::<Vec<_>>(),
        "carcasses": m.carcasses.iter().map(|c| json!({"x": c.x, "y": c.y, "t": c.t})).collect::<Vec<_>>(),
    })
}

/// First difference between two JSON values, as a path and both sides.
fn diff(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            ((x - y).abs() > TOL * (1.0 + x.abs().max(y.abs()))).then(|| format!("{path}: python {x} rust {y}"))
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                let Some(w) = y.get(k) else { return Some(format!("{path}.{k}: missing in rust")) };
                if let Some(d) = diff(&format!("{path}.{k}"), v, w) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: python len {} rust len {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (v, w))| diff(&format!("{path}[{i}]"), v, w))
        }
        _ => (a != b).then(|| format!("{path}: python {a} rust {b}")),
    }
}

/// Keys the Rust side writes that Python does not (`diff` only walks Python's).
fn extra_keys(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => y.iter().find_map(|(k, w)| match x.get(k) {
            None => Some(format!("{path}.{k}: only in rust")),
            Some(v) => extra_keys(&format!("{path}.{k}"), v, w),
        }),
        (Value::Array(x), Value::Array(y)) => {
            x.iter().zip(y).enumerate().find_map(|(i, (v, w))| extra_keys(&format!("{path}[{i}]"), v, w))
        }
        _ => None,
    }
}

/// Replays one fixture; returns the first diverging frame, if any.
fn replay(name: &str) -> Result<usize, String> {
    let fx = fixture(name);
    let seed = fx["seed"].as_i64().unwrap();
    let scenario = fx["scenario"].as_str().unwrap();
    let scratch = std::env::temp_dir().join(format!("dc-manager-parity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let preset = scratch.join("fixture.json");
    std::fs::write(&preset, fx["preset"].to_string()).unwrap();
    let state = scratch.join(format!("{scenario}-{seed}"));
    std::fs::create_dir_all(&state).unwrap();

    let mut rng = PyRandom::from_int(seed as i128);
    let mut m = CreatureManager::new(&root(), &preset, (1280.0, 720.0), Some(seed), &state, false, &mut rng)
        .map_err(|e| format!("{name}: construct: {e:?}"))?;
    let metrics = OracleMetrics::from_fixture(&fx);
    m.set_label_metrics(Box::new(metrics.clone()));

    for action in fx["actions"].as_array().unwrap() {
        match action["kind"].as_str().unwrap() {
            "positions" => {
                for (c, p) in m.creatures.iter_mut().zip(action["positions"].as_array().unwrap()) {
                    let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
                    c.kin.x = x;
                    c.kin.y = y;
                    c.kin.target_x = x;
                    c.kin.target_y = y;
                    c.initialize_legs();
                }
            }
            "fly" => {
                let id = m.fly_world.spawn(true).expect("forced fly");
                let f = m.fly_world.flies.iter_mut().find(|f| f.id == id).unwrap();
                f.x = action["x"].as_f64().unwrap();
                f.y = action["y"].as_f64().unwrap();
            }
            "cage" => {
                m.add_cage(Some(300.0), Some(350.0), 360.0, 260.0);
            }
            "name" => {
                let id = m.creatures[0].id;
                m.name_creature(id, "  Charlotte  ");
                m.set_creature_level_pin(id, true);
            }
            "surfaces" => {
                let surfaces = serde_json::from_value(action["surfaces"].clone()).unwrap();
                m.set_desktop_surfaces(surfaces);
            }
            other => return Err(format!("{name}: unknown action {other}")),
        }
    }

    let frames = fx["frames"].as_array().unwrap();
    if let Some(d) = diff("frame0", &frames[0]["state"], &trace(&m, &metrics)) {
        return Err(format!("{name}: {d}"));
    }
    for (i, frame) in frames.iter().enumerate().skip(1) {
        let input = frame["input"].as_array().unwrap();
        let (x, y) = (input[0].as_f64().unwrap(), input[1].as_f64().unwrap());
        let mouse = MouseInput {
            down: input[2].as_bool().unwrap(),
            pressed: input[3].as_bool().unwrap(),
            released: input[4].as_bool().unwrap(),
        };
        for _ in 0..frame["steps"].as_u64().unwrap_or(1) {
            m.update(1.0 / 60.0, x, y, mouse, &mut rng);
        }
        if scenario == "cages" {
            m.update_hover(x, y);
        }
        if let Some(d) = diff(&format!("frame{i}"), &frame["state"], &trace(&m, &metrics)) {
            return Err(format!("{name}: {d}"));
        }
    }
    let next: Vec<f64> = (0..4).map(|_| rng.random()).collect();
    let want: Vec<f64> = fx["rng_next"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    if next != want {
        return Err(format!("{name}: manager RNG drifted: python {want:?} rust {next:?}"));
    }
    m.save_runtime_state().map_err(|e| format!("{name}: save: {e}"))?;
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(state.join("creatures.json")).unwrap()).unwrap();
    if let Some(d) = diff("saved", &fx["saved"], &saved).or_else(|| extra_keys("saved", &fx["saved"], &saved)) {
        return Err(format!("{name}: creatures.json {d}"));
    }
    let png = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/frames").join(format!("{name}.png"));
    if png.exists() {
        compare_frame(&mut m, &png, name)?;
    }
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(frames.len())
}

/// The last frame painted through `ScenePainter` on the CPU renderer against
/// the Python manager's `render(painter)` (labels off on both sides). The
/// painters themselves differ slightly in anti-aliasing (see dc-paint's
/// comparison), so this asks for close, not identical: the same silhouettes
/// and colours, which a wrong field in the bridge would break.
fn compare_frame(m: &mut CreatureManager, png: &std::path::Path, name: &str) -> Result<(), String> {
    m.set_always_show_names(false);
    m.set_always_show_levels(false);
    m.set_always_show_health(false);
    m.set_always_show_xp(false);
    m.set_always_show_stamina(false);
    // The generator cleared these too: offscreen Qt draws text as boxes.
    for c in &mut m.creatures {
        c.damage_numbers.clear();
    }
    for c in &mut m.carcasses {
        c.damage_numbers.clear();
    }
    let mut painter = ScenePainter::new(PaintSwitches::default());
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        painter.paint(m, &mut c, None);
    }
    let rust = CpuRenderer::new().render(&scene, 1280, 720, None);
    let qt = Image::load_png(png).map_err(|e| format!("{name}: {e:?}"))?;
    let r = compare_images(&qt, &rust);
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frames");
    let _ = std::fs::create_dir_all(&out);
    let _ = rust.save_png(out.join(format!("{name}-rust.png")));
    let _ = diff_image(&qt, &rust).save_png(out.join(format!("{name}-diff.png")));
    println!(
        "{name}: mean diff {:.2}, IoU {:.4}, 1px {:.4}, coverage {:.3}",
        r.mean_diff, r.iou, r.iou_1px, r.coverage
    );
    if r.iou_1px < 0.97 || r.mean_diff > 8.0 || !(0.9..=1.1).contains(&r.coverage) {
        return Err(format!(
            "{name}: frame differs: mean diff {:.2}, IoU {:.4}, 1px {:.4}, coverage {:.3} (images in {})",
            r.mean_diff,
            r.iou,
            r.iou_1px,
            r.coverage,
            out.display()
        ));
    }
    Ok(())
}

fn check(scenario: &str) {
    let failures: Vec<String> =
        [3, 7, 11].iter().filter_map(|seed| replay(&format!("{scenario}-{seed}")).err()).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn colony_matches_python() {
    check("colony");
}

#[test]
fn fight_matches_python() {
    check("fight");
}

#[test]
fn hunt_matches_python() {
    check("hunt");
}

#[test]
fn cages_matches_python() {
    check("cages");
}

#[test]
fn stalk_matches_python() {
    check("stalk");
}

#[test]
fn duel_to_the_death_matches_python() {
    check("duel");
}

#[test]
fn desktop_surfaces_match_python() {
    check("surfaces");
}
