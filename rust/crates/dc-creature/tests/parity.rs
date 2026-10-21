//! Frame-by-frame replay of the Python `Creature.update` traces dumped by
//! `tools/dump_fixtures.py`: same construction, same cursor path, same drag
//! events, no worlds attached. Every frame's FSM state must match exactly and
//! every number to 1e-9 (relative, the fixtures keep 13 significant digits).

use dc_creature::world::{Command, Env, Population};
use dc_creature::{Creature, CreatureConfig};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

fn load(name: &str) -> Value {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Build a creature the way `CreatureManager._create_creature` did.
fn build(spec: &Value, id: u64) -> Creature {
    let ctor = &spec["ctor"];
    let kw = &ctor["kw"];
    let mut cfg = CreatureConfig::new(
        ctor["model"].clone(),
        ctor["personality"].clone(),
        f(&ctor["screen_w"]),
        f(&ctor["screen_h"]),
    );
    cfg.id = id;
    cfg.index = kw["index"].as_i64().unwrap();
    cfg.size_scale = f(&kw["size_scale"]);
    cfg.skills = kw["skills"].as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect());
    cfg.gait_style = kw["gait_style"].as_str().unwrap().to_string();
    cfg.color_overrides = Some(kw["color_overrides"].clone()).filter(|v| !v.is_null());
    cfg.progression_state = Some(kw["progression_state"].clone()).filter(|v| !v.is_null());
    cfg.progression_id = kw["progression_id"].as_str().map(str::to_string);
    cfg.job_id = kw["job_id"].as_str().unwrap().to_string();
    cfg.seed = kw["seed"].as_i64();
    let mut c = Creature::new(cfg);
    // `_create_creature`'s follow-up, in its order.
    let team = spec["team"].as_str().unwrap();
    c.set_team(team);
    let stances = dc_state::progression::normalize_team_stances(Some(&spec["team_stances"]));
    c.team_stances = Some(Arc::new(stances));
    if let Some(pos) = spec["pos"].as_array() {
        let (x, y) = (f(&pos[0]), f(&pos[1]));
        c.kin.x = x;
        c.kin.y = y;
        c.kin.target_x = x;
        c.kin.target_y = y;
        let (sw, sh) = (c.screen_w, c.screen_h);
        c.resize_screen(sw, sh);
        c.initialize_legs();
    }
    let mood_mode = spec["mood_mode"].as_str().unwrap_or("auto");
    if !mood_mode.is_empty() && mood_mode != "auto" {
        c.set_mood_mode(mood_mode);
    }
    c.set_allow_social(spec["allow_social"].as_bool().unwrap());
    c
}

/// Largest relative deviation seen by any comparison (as f64 bits).
static MAX_DEV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn close(a: f64, b: f64) -> bool {
    let scale = 1.0f64.max(a.abs()).max(b.abs());
    let dev = (a - b).abs() / scale;
    MAX_DEV.fetch_max(dev.to_bits(), std::sync::atomic::Ordering::Relaxed);
    dev <= 1e-9
}

struct Mismatch {
    frame: usize,
    creature: usize,
    column: String,
    rust: String,
    python: String,
}

fn row(c: &Creature) -> Vec<Value> {
    let m = &c.mood;
    let k = &c.kin;
    vec![
        Value::from(c.state.as_str()),
        k.x.into(),
        k.y.into(),
        k.heading.into(),
        c.state_timer.into(),
        c.decision_timer.into(),
        k.speed.into(),
        k.current_speed.into(),
        k.target_x.into(),
        k.target_y.into(),
        m.valence.into(),
        m.arousal.into(),
        m.affection.into(),
        m.curiosity.into(),
        c.energy.into(),
        c.jump_z.into(),
        c.crouch.into(),
        c.focus_x.into(),
        c.focus_y.into(),
        c.phase_scheduler.current_phase_id().map_or(Value::Null, |p| Value::from(p.as_str())),
        c.hp.into(),
        (c.level() as f64).into(),
        c.flee_timer.into(),
        c.webbed().into(),
    ]
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => close(x.as_f64().unwrap(), y.as_f64().unwrap()),
        _ => a == b,
    }
}

fn check_init(c: &Creature, init: &Value) -> Vec<String> {
    let mut bad = Vec::new();
    let mut num = |name: &str, rust: f64| {
        let py = f(&init[name]);
        if !close(rust, py) {
            bad.push(format!("init {name}: rust {rust} python {py}"));
        }
    };
    num("x", c.kin.x);
    num("y", c.kin.y);
    num("heading", c.kin.heading);
    num("size", c.kin.size);
    num("turn_rate", c.kin.turn_rate);
    num("state_timer", c.state_timer);
    num("decision_timer", c.decision_timer);
    num("hop_timer", c.hop_timer);
    num("blink_timer", c.blink_timer);
    num("social_cooldown", c.social_cooldown);
    num("weave_cooldown", c.weave_cooldown);
    num("web_walk_cooldown", c.web_walk_cooldown);
    num("web_shot_cooldown", c.web_shot_cooldown);
    num("pounce_cooldown", c.pounce_cooldown);
    num("trap_shot_cooldown", c.trap_shot_cooldown);
    num("weave_speed", c.weave_speed);
    num("max_hp", c.max_hp);
    num("damage", c.damage);
    num("armor", c.armor);
    let deck: Vec<String> = c.phase_scheduler.deck.iter().map(|p| p.as_str().to_string()).collect();
    let py_deck: Vec<String> =
        init["deck"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    if deck != py_deck {
        bad.push(format!("init deck: rust {deck:?} python {py_deck:?}"));
    }
    let mood = [c.mood.valence, c.mood.arousal, c.mood.affection, c.mood.curiosity];
    for (i, m) in mood.iter().enumerate() {
        if !close(*m, f(&init["mood"][i])) {
            bad.push(format!("init mood[{i}]: rust {m} python {}", init["mood"][i]));
        }
    }
    if c.active_gait_index != init["active_gait_index"].as_i64().unwrap() {
        bad.push("init active_gait_index".into());
    }
    for (i, leg) in init["legs"].as_array().unwrap().iter().enumerate() {
        let l = &c.kin.legs[i];
        for (j, v) in [l.foot_x, l.foot_y, l.phase_seed, l.gait_phase_offset].iter().enumerate() {
            if !close(*v, f(&leg[j])) {
                bad.push(format!("init leg {i}.{j}: rust {v} python {}", leg[j]));
            }
        }
    }
    bad
}

/// Replay one fixture; returns the frame count that matched before the first
/// divergence (all frames when none).
fn replay(name: &str) -> (usize, usize, Option<Mismatch>) {
    let data = load(name);
    let specs = data["creatures"].as_array().unwrap();
    let mut creatures: Vec<Creature> = specs.iter().enumerate().map(|(i, s)| build(s, i as u64 + 1)).collect();
    for (i, c) in creatures.iter().enumerate() {
        let bad = check_init(c, &data["init"][i]);
        assert!(bad.is_empty(), "{name} creature {i}: {}", bad.join("\n"));
    }
    let dt = f(&data["dt"]);
    let (sw, sh) = (f(&data["screen"][0]), f(&data["screen"][1]));
    let group = data["group"].as_bool().unwrap();
    let columns: Vec<String> = data["columns"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
    let path = data["path"].as_array().unwrap();
    let trace = data["trace"].as_array().unwrap();
    let events = data["events"].as_object().unwrap();
    let ids: Vec<u64> = creatures.iter().map(|c| c.id).collect();
    let mut pop = Population::from_views(creatures.iter().map(|c| c.view()));
    let mut dragging: Vec<usize> = Vec::new();
    let mut commands: Vec<Command> = Vec::new();
    let mut web_world =
        data["web_seed"].as_str().map(|seed| dc_world::WebWorld::new(sw, sh, dc_core::PyRandom::from_str_seed(seed)));
    let webs_trace = data["webs"].as_array().cloned().unwrap_or_default();
    for (frame, expected) in trace.iter().enumerate() {
        let (mx, my) = (f(&path[frame][0]), f(&path[frame][1]));
        if let Some(evs) = events.get(&frame.to_string()) {
            for ev in evs.as_array().unwrap() {
                let i = ev[1].as_u64().unwrap() as usize;
                match ev[0].as_str().unwrap() {
                    "start" => {
                        creatures[i].start_drag(mx, my);
                        dragging.push(i);
                    }
                    "release" => {
                        creatures[i].release_drag(mx, my);
                        dragging.retain(|&d| d != i);
                    }
                    "foe" => {
                        let j = ev[2].as_i64().unwrap();
                        creatures[i].foe = if j < 0 { None } else { Some(ids[j as usize]) };
                    }
                    "pin" => creatures[i].web_pinned(None),
                    "cage" => {
                        creatures[i].cage = Some(dc_creature::world::CageRect {
                            id: 1,
                            x: f(&ev[2]),
                            y: f(&ev[3]),
                            w: f(&ev[4]).max(90.0),
                            h: f(&ev[5]).max(90.0),
                        })
                    }
                    "damage" => {
                        creatures[i].take_damage(f(&ev[2]), None);
                    }
                    "xp" => {
                        creatures[i].gain_experience(f(&ev[2]));
                    }
                    "flee" => {
                        creatures[i].flee_timer = f(&ev[2]);
                        creatures[i].flee_from = Some(ids[ev[3].as_u64().unwrap() as usize]);
                    }
                    other => panic!("event {other}"),
                }
                pop.set(creatures[i].view());
            }
        }
        dragging.sort_unstable();
        for &i in &dragging {
            creatures[i].drag_to(dt, mx, my);
            pop.set(creatures[i].view());
        }
        for i in 0..creatures.len() {
            let c = &mut creatures[i];
            c.neighbors = if group { ids.clone() } else { vec![c.id] };
            {
                let mut env = Env::bare(&pop, &mut commands);
                if let Some(ww) = web_world.as_mut() {
                    env.webs = Some(ww);
                }
                c.update(dt, mx, my, sw, sh, &mut env);
            }
            pop.set(creatures[i].view());
            for cmd in commands.drain(..) {
                for t in creatures.iter_mut() {
                    t.apply_command(&cmd, &pop);
                }
                for t in &creatures {
                    pop.set(t.view());
                }
            }
        }
        if let Some(ww) = web_world.as_mut() {
            ww.update(dt, Some((mx, my)));
            let want = webs_trace[frame].as_array().unwrap();
            let got: Vec<Value> =
                ww.webs.iter().map(|w| serde_json::json!([w.state_str(), w.progress(), w.intact_fraction()])).collect();
            let ok = got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (0..3).all(|k| same(&g[k], &w[k])));
            if !ok {
                return (
                    frame,
                    trace.len(),
                    Some(Mismatch {
                        frame,
                        creature: 0,
                        column: "webs".into(),
                        rust: serde_json::to_string(&got).unwrap(),
                        python: serde_json::to_string(want).unwrap(),
                    }),
                );
            }
        }
        for (ci, c) in creatures.iter().enumerate() {
            let got = row(c);
            let want = expected[ci].as_array().unwrap();
            for (k, (g, w)) in got.iter().zip(want.iter()).enumerate() {
                if !same(g, w) {
                    return (
                        frame,
                        trace.len(),
                        Some(Mismatch {
                            frame,
                            creature: ci,
                            column: columns[k].clone(),
                            rust: g.to_string(),
                            python: w.to_string(),
                        }),
                    );
                }
            }
        }
    }
    (trace.len(), trace.len(), None)
}

fn assert_parity(name: &str) {
    let (matched, total, mismatch) = replay(name);
    if let Some(m) = mismatch {
        panic!(
            "{name}: diverged at frame {} of {total} (creature {}, {}): rust {} python {}",
            m.frame, m.creature, m.column, m.rust, m.python
        );
    }
    assert_eq!(matched, total);
    let dev = f64::from_bits(MAX_DEV.load(std::sync::atomic::Ordering::Relaxed));
    eprintln!("{name}: {total} frames matched; max relative deviation so far {dev:.2e}");
}

macro_rules! parity_tests {
    ($($test:ident => $name:expr),* $(,)?) => {
        $(#[test] fn $test() { assert_parity($name); })*
    };
}

parity_tests! {
    colony_solo_0 => "colony_solo_0",
    colony_solo_1 => "colony_solo_1",
    colony_solo_2 => "colony_solo_2",
    colony_solo_3 => "colony_solo_3",
    colony_solo_4 => "colony_solo_4",
    personality_balanced => "personality_balanced",
    personality_bashful => "personality_bashful",
    personality_bold => "personality_bold",
    personality_camouflage => "personality_camouflage",
    personality_cautious => "personality_cautious",
    personality_clingy => "personality_clingy",
    personality_cuddly => "personality_cuddly",
    personality_curious => "personality_curious",
    personality_drifter => "personality_drifter",
    personality_explorer => "personality_explorer",
    personality_grumpy => "personality_grumpy",
    personality_hunter => "personality_hunter",
    personality_jumper => "personality_jumper",
    personality_mellow => "personality_mellow",
    personality_nope => "personality_nope",
    personality_observer => "personality_observer",
    personality_playful => "personality_playful",
    personality_shy => "personality_shy",
    personality_skittish => "personality_skittish",
    personality_sleepy => "personality_sleepy",
    personality_trapper => "personality_trapper",
    personality_webber => "personality_webber",
    personality_zoomy => "personality_zoomy",
    colony_group => "colony_group",
    social_group => "social_group",
    drag_throw => "drag_throw",
    fight => "fight",
    caged => "caged",
    webs_webber => "webs_webber",
    webs_group => "webs_group",
}
