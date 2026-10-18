//! Shared helpers for the ported Python tests (`tests/support.py`).
#![allow(dead_code)]

use dc_creature::world::{Command, CreatureView, Env, Population};
use dc_creature::{Creature, CreatureConfig};
use serde_json::Value;
use std::path::PathBuf;

/// The worktree root (`ROOT` in `tests/support.py`).
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

/// `load_model(name)`: a shipped model with its body plan resolved.
pub fn load_model(name: &str) -> Value {
    let path = root().join("models").join(name).join("model.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut v: Value = serde_json::from_str(&text).unwrap();
    dc_content::body_plans::resolve_body_plan(&mut v);
    v
}

/// `load_personality(name)`
pub fn load_personality(name: &str) -> Value {
    let path = root().join("personalities").join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// `Creature(model, personality, 1600, 900, seed=seed)` with a stable id.
pub fn spider(model: &str, personality: &str, seed: i64) -> Creature {
    spider_with(load_model(model), load_personality(personality), seed, |_| {})
}

/// A creature from raw dicts, with a hook to tweak the config.
pub fn spider_with(model: Value, personality: Value, seed: i64, tweak: impl FnOnce(&mut CreatureConfig)) -> Creature {
    let mut cfg = CreatureConfig::new(model, personality, 1600.0, 900.0);
    cfg.seed = Some(seed);
    cfg.id = seed as u64 + 1;
    tweak(&mut cfg);
    Creature::new(cfg)
}

/// One `creature.update(dt, mx, my, sw, sh)` with no worlds, the creature
/// alone in its population (plus any `others`).
pub fn step(c: &mut Creature, dt: f64, mx: f64, my: f64, others: &[CreatureView]) -> Vec<Command> {
    let mut pop = Population::from_views(others.iter().cloned());
    pop.set(c.view());
    let mut commands = Vec::new();
    {
        let mut env = Env::bare(&pop, &mut commands);
        let (sw, sh) = (c.screen_w, c.screen_h);
        c.update(dt, mx, my, sw, sh, &mut env);
    }
    commands
}

/// `n` frames of `step` at 60 Hz with a fixed cursor.
pub fn run(c: &mut Creature, frames: usize, mx: f64, my: f64) {
    for _ in 0..frames {
        step(c, 1.0 / 60.0, mx, my, &[]);
    }
}
