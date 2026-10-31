//! Port of `_refresh_camouflage_samples` (`app/engine.py`): the camouflage
//! strength that dc-paint turns into opacity (`camouflage_opacity()`).
//!
//! Python's comments call it "sampling", but it stopped grabbing desktop
//! pixels long ago ("Camouflage now only animates opacity, so this is cheap
//! and does not grab desktop pixels anymore"): `_camouflage_color` is always
//! cleared, and a camouflage spider only fades out after it has been left
//! alone and still for a while. So no screen capture is ported here either.
//!
//! Every [`CAMOUFLAGE_SAMPLE_MS`]: a dragged or moving spider (body, throw
//! or any leg stepping or lifted) is made visible for a while
//! (`register_camouflage_touch`); a visible one counts its timer down; an
//! idle one ramps towards its `camouflage_strength` after
//! `camouflage_idle_delay` over `camouflage_hide_fade_time` (smootherstep),
//! revealing at `camouflage_reveal_speed` and hiding at
//! `camouflage_fade_speed` per sample.

use dc_creature::Creature;

/// How often the strength is updated.
pub const CAMOUFLAGE_SAMPLE_MS: f64 = 120.0;

/// `_last_camouflage_sample_ms` and the rate limit round the refresh.
#[derive(Debug, Default)]
pub struct CamouflageSampler {
    last_ms: f64,
}

impl CamouflageSampler {
    /// The tick's `if current_ms - self._last_camouflage_sample_ms >=
    /// CAMOUFLAGE_SAMPLE_MS` check. True when it refreshed.
    pub fn tick(&mut self, now_ms: f64, creatures: &mut [Creature]) -> bool {
        if now_ms - self.last_ms < CAMOUFLAGE_SAMPLE_MS {
            return false;
        }
        self.last_ms = now_ms;
        refresh_camouflage_samples(creatures);
        true
    }
}

/// Python's `max(lo, min(hi, x))`.
fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    lo.max(hi.min(x))
}

/// Whether anything of the spider moves: body, throw, or a leg stepping,
/// about to step or lifted.
fn is_moving(c: &Creature) -> bool {
    let k = &c.kin;
    let threshold = 1.2f64.max(k.size * 0.035);
    k.vel_x.abs() > threshold
        || k.vel_y.abs() > threshold
        || k.current_speed > threshold
        || k.inertia_vx.abs() > threshold
        || k.inertia_vy.abs() > threshold
        || k.legs.iter().any(|leg| leg.stepping || leg.pending_step || leg.lift > 0.012)
}

/// `_refresh_camouflage_samples`.
pub fn refresh_camouflage_samples(creatures: &mut [Creature]) {
    let sample_dt = CAMOUFLAGE_SAMPLE_MS / 1000.0;
    for c in creatures {
        let max_strength = clamp(c.p().f64_or("camouflage_strength", 0.0), 0.0, 1.0);
        if max_strength <= 0.0 && c.camouflage_strength <= 0.001 {
            c.camouflage_strength = 0.0;
            c.camouflage_color = None;
            continue;
        }
        // Dragged or recently touched: fully visible.
        if c.dragging {
            c.register_camouflage_touch(Some(4.5));
        }
        // Moving reveals it too, unless it is wholly behind a window.
        if !c.desktop_fully_hidden && is_moving(c) {
            let visible_time = c.p().f64_or("camouflage_motion_visible_time", 1.25);
            c.register_camouflage_touch(Some(clamp(visible_time, 0.35, 3.0)));
        }

        let visible_timer = c.camouflage_visible_timer.max(0.0);
        let target_strength = if visible_timer > 0.0 {
            c.camouflage_visible_timer = (visible_timer - sample_dt).max(0.0);
            c.camouflage_idle_timer = 0.0;
            0.0
        } else {
            c.camouflage_idle_timer = c.camouflage_idle_timer.max(0.0) + sample_dt;
            let idle_delay = c.p().f64_or("camouflage_idle_delay", 5.0).max(0.0);
            let fade_in_time = c.p().f64_or("camouflage_hide_fade_time", 3.0).max(0.25);
            let p = clamp((c.camouflage_idle_timer - idle_delay) / fade_in_time, 0.0, 1.0);
            // Smootherstep.
            let p = p * p * p * (p * (p * 6.0 - 15.0) + 10.0);
            max_strength * p
        };

        let current = clamp(c.camouflage_strength, 0.0, 1.0);
        let speed = if target_strength <= current {
            // A touch or a reveal is immediate.
            clamp(c.p().f64_or("camouflage_reveal_speed", 0.90), 0.08, 1.0)
        } else {
            clamp(c.p().f64_or("camouflage_fade_speed", 0.18), 0.02, 1.0)
        };
        c.camouflage_strength = current + (target_strength - current) * speed;
        if c.camouflage_strength.abs() < 0.002 {
            c.camouflage_strength = 0.0;
        }
        c.camouflage_color = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_core::PyRandom;
    use dc_manager::CreatureManager;
    use serde_json::Value;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// `fixtures/gen_camouflage.py`'s trace of Python's refresh.
    const PYTHON: &str = include_str!("fixtures/camouflage.json");

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// A spider of the default preset, its manager's state folder a
    /// throw-away one.
    fn spider() -> Creature {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let state = std::env::temp_dir().join(format!("dc-camouflage-{}-{nanos}", std::process::id()));
        let mut rng = PyRandom::from_int(3);
        let preset = root().join("presets/default.json");
        let m = CreatureManager::new(&root(), &preset, (1280.0, 720.0), Some(3), &state, false, &mut rng)
            .expect("the default preset loads");
        let c = m.creatures[0].clone();
        let _ = std::fs::remove_dir_all(&state);
        c
    }

    fn personality(file: &str, overrides: &Value) -> Value {
        let path = root().join("personalities").join(format!("{file}.json"));
        let mut data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for (k, v) in overrides.as_object().into_iter().flatten() {
            data[k] = v.clone();
        }
        data
    }

    /// Put a spider at rest, then apply what the trace says it is doing.
    fn apply(c: &mut Creature, d: &Value) {
        let num = |k: &str| d.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        c.dragging = d.get("dragging").and_then(Value::as_bool).unwrap_or(false);
        c.desktop_fully_hidden = d.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        c.kin.vel_x = num("vel_x");
        c.kin.vel_y = num("vel_y");
        c.kin.current_speed = num("current_speed");
        c.kin.inertia_vx = num("inertia_vx");
        c.kin.inertia_vy = num("inertia_vy");
        for leg in &mut c.kin.legs {
            leg.stepping = false;
            leg.pending_step = false;
            leg.lift = 0.0;
        }
        if let Some(leg) = d.get("leg").and_then(Value::as_array) {
            let l = &mut c.kin.legs[leg[0].as_u64().unwrap() as usize];
            l.stepping = leg[1].as_bool().unwrap();
            l.pending_step = leg[2].as_bool().unwrap();
            l.lift = leg[3].as_f64().unwrap();
        }
    }

    #[test]
    fn strength_trace_matches_python() {
        let oracle: Value = serde_json::from_str(PYTHON).unwrap();
        let spiders = oracle["spiders"].as_object().unwrap();
        let base = spider();
        let names: Vec<&String> = spiders.keys().collect();
        let mut creatures: Vec<Creature> = names
            .iter()
            .map(|name| {
                let s = &spiders[name.as_str()];
                let mut c = base.clone();
                c.personality = Arc::new(personality(s["personality"].as_str().unwrap(), &s["overrides"]));
                c.kin.size = s["size"].as_f64().unwrap();
                c.camouflage_strength = s["strength"].as_f64().unwrap();
                c.camouflage_visible_timer = s["visible_timer"].as_f64().unwrap();
                c.camouflage_idle_timer = 0.0;
                c.camouflage_color = Some([1, 2, 3]);
                assert!(c.kin.legs.len() >= 8);
                c
            })
            .collect();
        let mut deepest = 0.0f64;
        for i in 0..oracle["samples"].as_u64().unwrap() as usize {
            for (c, name) in creatures.iter_mut().zip(&names) {
                apply(c, &spiders[name.as_str()]["trace"][i]["in"]);
            }
            refresh_camouflage_samples(&mut creatures);
            for (c, name) in creatures.iter().zip(&names) {
                let want = &spiders[name.as_str()]["trace"][i];
                let at = format!("{name} sample {i}");
                assert_eq!(c.camouflage_strength, want["strength"].as_f64().unwrap(), "{at}: strength");
                assert_eq!(c.camouflage_visible_timer, want["visible_timer"].as_f64().unwrap(), "{at}: visible");
                assert_eq!(c.camouflage_idle_timer, want["idle_timer"].as_f64().unwrap(), "{at}: idle");
                assert_eq!(c.camouflage_color, None, "{at}: colour");
                deepest = deepest.max(c.camouflage_strength);
            }
        }
        // The trace is worth something: a spider did fade most of the way.
        assert!(deepest > 0.85, "{deepest}");
    }

    #[test]
    fn samples_every_120_ms() {
        let mut creatures = vec![spider()];
        let mut sampler = CamouflageSampler::default();
        let ticks: Vec<bool> = [0.0, 60.0, 119.9, 120.0, 200.0, 239.0, 240.0, 1000.0]
            .iter()
            .map(|&t| sampler.tick(t, &mut creatures))
            .collect();
        assert_eq!(ticks, [false, false, false, true, false, false, true, true]);
    }

    #[test]
    fn a_still_camouflage_spider_fades_and_a_drag_shows_it() {
        let mut c = spider();
        c.personality = Arc::new(personality("camouflage", &Value::Null));
        apply(&mut c, &Value::Null);
        c.camouflage_visible_timer = 0.0;
        let mut creatures = vec![c];
        // A 5 s idle delay and a 3 s fade, then 0.16 of the way per sample.
        for _ in 0..120 {
            refresh_camouflage_samples(&mut creatures);
        }
        let hidden = creatures[0].camouflage_strength;
        assert!((hidden - 0.9).abs() < 0.01, "{hidden}");
        // What dc-paint draws it at: 1 - 0.9 x 0.82.
        let drop = creatures[0].p().f64_or("camouflage_opacity_drop", 0.72);
        assert!((1.0 - hidden * drop).clamp(0.12, 1.0) < 0.3);
        creatures[0].dragging = true;
        refresh_camouflage_samples(&mut creatures);
        assert_eq!(creatures[0].camouflage_strength, 0.0, "a drag shows it at once");
    }
}
