//! Python-oracle parity for the missions running on the real manager:
//! `dc-adventure/tools/dump_fixtures.py::missions` builds every map on one
//! and two screens with the unmodified Python `create_mission`, holds a
//! foothold, runs the hatchery's waves and rolls the guardian's loot.

use dc_adventure::adventure::SilkShot;
use dc_adventure::adventure_profile::{fresh_profile, profile_path, save_profile};
use dc_adventure::campaign::MAPS;
use dc_adventure::controls::ControlSettings;
use dc_adventure::mission::{MissionHost, SpawnRequest, SpiderId, TerritoryMission};
use dc_adventure::mission_factory::{Mission, create_mission};
use dc_core::PyRandom;
use dc_creature::Creature;
use dc_manager::CreatureManager;
use dc_world::{FlyId, FlyWorld, ScreenLayout, ScreenRect};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const TOL: f64 = 1e-6;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

fn fixture() -> Value {
    let path = root().join("rust/crates/dc-adventure/tests/fixtures/missions.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn diff(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            ((x - y).abs() > TOL * (1.0 + x.abs().max(y.abs()))).then(|| format!("{path}: python {x} rust {y}"))
        }
        (Value::Object(x), Value::Object(y)) => x.iter().find_map(|(k, v)| match y.get(k) {
            None => Some(format!("{path}.{k}: missing in rust")),
            Some(w) => diff(&format!("{path}.{k}"), v, w),
        }),
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: python len {} rust len {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (v, w))| diff(&format!("{path}[{i}]"), v, w))
        }
        _ => (a != b).then(|| format!("{path}: python {a} rust {b}")),
    }
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("dc-missions-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

/// The generator's `make(map_id, seed, count, two)`: a fresh admin profile on
/// that map, the colony preset on a seed-4 manager, `create_mission` with the
/// mission RNG seeded `seed`.
fn make(state: &Path, map_id: &str, seed: i64, count: usize, two: bool) -> (CreatureManager, Mission, PyRandom) {
    let mut profile = fresh_profile();
    profile.selected_map = map_id.into();
    profile.admin = true;
    profile.two_player = two;
    assert!(save_profile(&profile, &profile_path(state)));
    let rects = [ScreenRect::new(0.0, 0.0, 1600.0, 1000.0), ScreenRect::new(1600.0, 0.0, 1600.0, 1000.0)];
    let mut manager_rng = PyRandom::from_int(4);
    let mut manager = CreatureManager::new(
        &root(),
        &root().join("presets").join("colony.json"),
        (1600.0 * count as f64, 1000.0),
        Some(4),
        state,
        true,
        &mut manager_rng,
    )
    .unwrap();
    let mut rng = PyRandom::from_int(seed as i128);
    let mission =
        create_mission(&mut manager, ControlSettings::default(), &rects[..count], 0, state, None, &mut rng).unwrap();
    (manager, mission, rng)
}

fn snapshot(m: &TerritoryMission, host: &CreatureManager) -> Value {
    let sites: Vec<Value> = m
        .sites
        .iter()
        .map(|s| {
            json!({"kind": s.kind, "name": s.name, "x": s.x, "y": s.y, "owned": s.owned,
                   "reserves": s.reserves, "supply": s.supply, "screen": s.screen})
        })
        .collect();
    let spiders: Vec<Value> = host
        .creatures
        .iter()
        .map(|c| {
            let state = m.spider_state.get(&dc_world::CreatureId(c.id));
            json!({"serial": c.index, "x": c.kin.x, "y": c.kin.y, "name": c.display_name(),
                   "kind": state.and_then(|s| s.enemy_kind.clone()),
                   "worn": state.map(|s| s.mission_loot.clone()).unwrap_or_default(),
                   "progression": c.progression.to_value()})
        })
        .collect();
    let actors: Vec<Value> = m
        .actors
        .iter()
        .map(|a| {
            json!({"role": a.role, "style": a.style, "home": [a.home.0, a.home.1], "silk": a.control.silk,
                   "capacity": a.control.silk_capacity, "from_outpost": a.from_outpost,
                   "aggro_range": a.aggro_range})
        })
        .collect();
    let flies: Vec<Value> = host
        .fly_world
        .flies
        .iter()
        .map(|f| {
            json!({"x": f.x, "y": f.y, "heading": f.heading, "length": f.length, "size": f.size,
                   "cruise": f.cruise, "panic": f.panic})
        })
        .collect();
    json!({"sites": sites, "spiders": spiders, "cap": m.cap, "wave_every": m.wave_every,
           "actors": actors, "flies": flies})
}

#[test]
fn every_map_opens_as_in_python() {
    let fx = fixture();
    let mut failures = Vec::new();
    for case in fx["openings"].as_array().unwrap() {
        let map = case["map"].as_str().unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let screens = case["screens"].as_u64().unwrap() as usize;
        let two = case["two"].as_bool().unwrap();
        let dir = scratch(&format!("open-{map}-{seed}-{screens}"));
        let (manager, mission, mut rng) = make(&dir.0, map, seed, screens, two);
        let label = format!("{map} seed {seed} on {screens} screen(s)");
        if let Some(d) = diff("snapshot", &case["snapshot"], &snapshot(mission.territory(), &manager)) {
            failures.push(format!("{label}: {d}"));
            continue;
        }
        let next = rng.random();
        if (next - case["next"].as_f64().unwrap()).abs() > 0.0 {
            failures.push(format!("{label}: mission RNG drifted"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} openings differ:\n{}",
        failures.len(),
        fx["openings"].as_array().unwrap().len(),
        failures.join("\n")
    );
}

/// The generator froze every spider (`c.update = lambda *a: None`): no
/// creature frame and no controller ran. This host forwards everything else.
struct Frozen<'a>(&'a mut CreatureManager);

impl MissionHost for Frozen<'_> {
    type Spider = Creature;
    fn begin_mission(&mut self) -> Result<(), String> {
        self.0.begin_mission()
    }
    fn screen_size(&self) -> (f64, f64) {
        MissionHost::screen_size(self.0)
    }
    fn models_contains(&self, model_id: &str) -> bool {
        self.0.models_contains(model_id)
    }
    fn creatures(&self) -> Vec<SpiderId> {
        MissionHost::creatures(self.0)
    }
    fn creature(&self, id: SpiderId) -> &Creature {
        MissionHost::creature(self.0, id)
    }
    fn creature_mut(&mut self, id: SpiderId) -> &mut Creature {
        MissionHost::creature_mut(self.0, id)
    }
    fn create_creature(&mut self, request: SpawnRequest) -> SpiderId {
        MissionHost::create_creature(self.0, request)
    }
    fn refresh_neighbor_links(&mut self) {
        MissionHost::refresh_neighbor_links(self.0)
    }
    fn refresh_render_order(&mut self) {
        MissionHost::refresh_render_order(self.0)
    }
    fn update_creature(&mut self, _id: SpiderId, _dt: f64, _struggling: bool) -> bool {
        false
    }
    fn bury_the_dead(&mut self) {
        MissionHost::bury_the_dead(self.0)
    }
    fn update_carcasses(&mut self, dt: f64) {
        MissionHost::update_carcasses(self.0, dt)
    }
    fn trade_blow(&mut self, attacker: SpiderId, target: SpiderId) {
        MissionHost::trade_blow(self.0, attacker, target)
    }
    fn splash(&mut self, attacker: SpiderId, target: SpiderId, damage: f64) {
        MissionHost::splash(self.0, attacker, target, damage)
    }
    fn shoot(&mut self, shooter: SpiderId, shot: SilkShot) {
        MissionHost::shoot(self.0, shooter, shot)
    }
    fn update_projectiles(&mut self, dt: f64) {
        MissionHost::update_projectiles(self.0, dt)
    }
    fn fly_world(&self) -> &FlyWorld {
        MissionHost::fly_world(self.0)
    }
    fn fly_world_mut(&mut self) -> &mut FlyWorld {
        MissionHost::fly_world_mut(self.0)
    }
    fn update_fly(&mut self, id: FlyId, dt: f64, rng: &mut PyRandom, layout: &ScreenLayout) {
        MissionHost::update_fly(self.0, id, dt, rng, layout)
    }
}

#[test]
fn holding_the_food_cache_matches_python() {
    let fx = fixture();
    let dir = scratch("capture");
    let (mut manager, mut mission, mut rng) = make(&dir.0, "territory", 6, 1, false);
    for c in &mut manager.creatures {
        if c.progression.team_id == "rivals" {
            c.dead = true;
        }
    }
    MissionHost::bury_the_dead(&mut manager);
    let Mission::Territory(m) = &mut mission else { panic!("territory is a raid") };
    m.actors.retain(|a| !manager.creatures.iter().all(|c| c.id != a.creature.0));
    let food = m.site("food").unwrap();
    let (fx_x, fx_y) = (m.sites[food].x, m.sites[food].y);
    let hero = MissionHost::creature_mut(&mut manager, m.hero);
    hero.kin.x = fx_x;
    hero.kin.y = fx_y;
    let nest = m.site("nest").unwrap();
    for (tick, want) in fx["capture"].as_array().unwrap().iter().enumerate() {
        let mut host = Frozen(&mut manager);
        m.update(&mut host, 0.05, &mut rng);
        let got = json!({"tick": tick + 1, "progress": m.sites[food].progress, "owned": m.sites[food].owned,
                         "warning": m.sites[nest].warning, "pending": m.pending, "counter": m.counter_started,
                         "elapsed": m.elapsed,
                         "xp": MissionHost::creature(&manager, m.hero).progression.total_xp});
        if let Some(d) = diff(&format!("tick{}", tick + 1), want, &got) {
            panic!("{d}");
        }
    }
}

#[test]
fn hatchery_waves_match_python() {
    let fx = fixture();
    let dir = scratch("waves");
    let (mut manager, mut mission, mut rng) = make(&dir.0, "territory", 6, 1, false);
    let Mission::Territory(m) = &mut mission else { panic!("territory is a raid") };
    m.wave_clock = 0.0;
    let hatchery = m.site("hatchery").unwrap();
    let mut want = fx["waves"].as_array().unwrap().iter();
    for tick in 1..=1500 {
        m.spawning(&mut manager, 0.05, &mut rng);
        if tick == 1 || tick % 10 == 0 {
            let got = json!({"tick": tick, "warning": m.sites[hatchery].warning,
                             "reserves": m.sites[hatchery].reserves, "pending": m.pending,
                             "count": manager.creatures.len(), "clock": m.wave_clock});
            if let Some(d) = diff(&format!("tick{tick}"), want.next().unwrap(), &got) {
                panic!("{d}");
            }
        }
    }
}

#[test]
fn guardian_loot_matches_python() {
    let fx = fixture();
    for case in fx["loot"].as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let dir = scratch(&format!("loot-{seed}"));
        let (mut manager, mut mission, _) = make(&dir.0, "queen", seed, 1, false);
        let Mission::Territory(m) = &mut mission else { panic!("queen is a raid") };
        let mut rng = PyRandom::from_int(seed as i128);
        let guardian = m.spawn(&mut manager, "guardian", (800.0, 500.0), false, None, None, &mut rng).unwrap();
        m.guardian = Some(guardian);
        MissionHost::creature_mut(&mut manager, guardian).dead = true;
        m.drop_loot(&manager, &mut rng);
        let got = json!({"seed": seed, "loot": serde_json::to_value(&m.loot).unwrap(), "next": rng.random()});
        if let Some(d) = diff(&format!("seed{seed}"), case, &got) {
            panic!("{d}");
        }
    }
}

/// Thirty seconds of the real mission loop with the hero left idle: creature
/// frames, controllers, enemy AI, combat, deaths, waves and swarm flies.
#[test]
fn live_raids_match_python() {
    let fx = fixture();
    let mut failures = Vec::new();
    for case in fx["battles"].as_array().unwrap() {
        let map = case["map"].as_str().unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let dir = scratch(&format!("battle-{map}"));
        let (mut manager, mut mission, mut rng) = make(&dir.0, map, seed, 1, false);
        let mut want = case["trace"].as_array().unwrap().iter();
        let mut failed = false;
        for tick in 1..=600 {
            mission.update(&mut manager, 0.05, &mut rng);
            if tick % 10 != 0 {
                continue;
            }
            let m = mission.territory();
            let got = json!({
                "tick": tick, "state": m.state, "elapsed": m.elapsed, "pending": m.pending,
                "spiders": manager.creatures.iter().map(|c| json!({"serial": c.index, "x": c.kin.x, "y": c.kin.y,
                    "heading": c.kin.heading, "hp": c.hp, "energy": c.energy, "airborne": c.airborne})).collect::<Vec<_>>(),
                "sites": m.sites.iter().map(|s| json!({"progress": s.progress, "owned": s.owned,
                    "warning": s.warning, "reserves": s.reserves})).collect::<Vec<_>>(),
                "flies": manager.fly_world.flies.iter().map(|f| json!({"x": f.x, "y": f.y})).collect::<Vec<_>>(),
                "carcasses": manager.carcasses.len(),
            });
            if let Some(d) = diff(&format!("tick{tick}"), want.next().unwrap(), &got) {
                failures.push(format!("{map} seed {seed}: {d}"));
                failed = true;
                break;
            }
        }
        if !failed && rng.random() != case["next"].as_f64().unwrap() {
            failures.push(format!("{map} seed {seed}: mission RNG drifted"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn maps_are_all_covered() {
    let fx = fixture();
    let covered: std::collections::BTreeSet<&str> =
        fx["openings"].as_array().unwrap().iter().map(|c| c["map"].as_str().unwrap()).collect();
    assert_eq!(covered.len(), MAPS.len());
}
