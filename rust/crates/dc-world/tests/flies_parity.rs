//! `flies.rs` (and `aimed_silk.rs`) against `tools/dump_world_fixtures.py`:
//! two scripted flocks with spiders, finished webs, thrown silk, eating and
//! dragging; plus nests, remains and fresh flies.

mod common;

use common::*;
use dc_core::PyRandom;
use dc_world::creature_view::{CreatureId, FlyId};
use dc_world::flies::{Fly, FlyConfigure, FlyRemains, FlyWorld, FlyWorldConfig, PartKind, Projectile};
use dc_world::webs::{WebState, WebWorld, plan_corner_orb, plan_orb};
use dc_world::{Playfield, ScreenRect};
use serde_json::{Value, json};

const SW: f64 = 1600.0;
const SH: f64 = 900.0;

struct Script {
    x0: f64,
    y0: f64,
    vx: f64,
    vy: f64,
}

fn place(sp: &mut Bug, s: &Script, f: i64) {
    let t = f as f64 * DT;
    sp.x = tri(s.x0, s.vx, 20.0, SW - 20.0, t);
    sp.y = tri(s.y0, s.vy, 20.0, SH - 20.0, t);
    sp.frame = f;
    if sp.webbed && f >= sp.webbed_until {
        sp.webbed = false;
    }
    let i = sp.index;
    sp.airborne = i % 4 == 1 && (f / 50) % 7 == 3;
    sp.dragging = i % 5 == 2 && (200..260).contains(&f);
    sp.hidden = i % 6 == 3 && (400..450).contains(&f);
    sp.aim = if i % 3 == 1 && (f / 60) % 2 == 1 { 0.5 } else { 0.0 };
}

fn state_char(fly: &Fly) -> &'static str {
    fly.state.as_str()
}

#[track_caller]
fn assert_fly_full(fly: &Fly, w: &Value, ctx: &str) {
    assert_eq!(fly.id.0, w["fid"].as_u64().unwrap(), "{ctx}: id");
    let c = |name: &str, got: f64| assert_close(got, &w[name], &format!("{ctx}: {name}"));
    c("x", fly.x);
    c("y", fly.y);
    c("heading", fly.heading);
    c("target_heading", fly.target_heading);
    c("speed", fly.speed);
    c("vx", fly.vx);
    c("vy", fly.vy);
    c("motion_timer", fly.motion_timer);
    c("wander_timer", fly.wander_timer);
    c("panic_level", fly.panic_level);
    c("wing_phase", fly.wing_phase);
    c("buzz_phase", fly.buzz_phase);
    c("struggle_phase", fly.struggle_phase);
    c("pluck_timer", fly.pluck_timer);
    c("trapped_for", fly.trapped_for);
    c("free_progress", fly.free_progress);
    c("web_check_timer", fly.web_check_timer);
    c("draw_jitter", fly.draw_jitter);
    c("length", fly.length);
    c("web_splat", fly.web_splat);
    c("eat_t", fly.eat_t);
    assert_eq!(fly.state.as_str(), w["state"].as_str().unwrap(), "{ctx}: state");
    assert_eq!(fly.motion_mode.as_str(), w["motion_mode"].as_str().unwrap(), "{ctx}: motion_mode");
    assert_eq!(fly.captured_by_web, w["captured_by_web"].as_bool().unwrap(), "{ctx}: captured_by_web");
    assert_pair(fly.tether_from, &w["tether"], &format!("{ctx}: tether"));
    let pegs = w["pegs"].as_array().unwrap();
    assert_eq!(fly.web_pegs.len(), pegs.len(), "{ctx}: pegs");
    for (p, q) in fly.web_pegs.iter().zip(pegs) {
        assert_pair(Some(*p), q, &format!("{ctx}: peg"));
    }
    assert_eq!(fly.trapper.map(|t| t.0), w["trapper"].as_u64(), "{ctx}: trapper");
}

fn run_scenario(sc: &Value) {
    let name = sc["name"].as_str().unwrap();
    let seed = sc["seed"].as_u64().unwrap();
    let frames = sc["frames"].as_i64().unwrap();
    let config = FlyWorldConfig {
        enabled: true,
        min_interval: 0.4,
        max_interval: 1.2,
        max_flies: sc["max_flies"].as_i64().unwrap(),
        scale: 1.0,
    };
    let mut world = FlyWorld::new(SW, SH, config, PyRandom::from_str_seed(&format!("{seed}:flies")));
    if let Some(rects) = sc["split"].as_array() {
        let rects: Vec<ScreenRect> =
            rects.iter().map(|r| ScreenRect::new(f(&r[0]), f(&r[1]), f(&r[2]), f(&r[3]))).collect();
        world.playfield = Some(Playfield::new(SW, SH, &rects));
    }
    let mut webs = WebWorld::new(SW, SH, PyRandom::from_str_seed(&format!("{seed}:webs")));
    let d = 0.7071067811865475;
    let corner = plan_corner_orb((0.0, 0.0), (d, d), 260.0, 38.0, SW, SH, &mut PyRandom::from_u64(5));
    let orb = plan_orb((700.0, 420.0), 170.0, SW, SH, &mut PyRandom::from_u64(6));
    for (pattern, plan, spec) in [("corner_orb", corner, "corner_nw"), ("orb", orb, "open")] {
        let mut web = webs.make_web(pattern, plan, spec, 38.0);
        web.state = WebState::Complete;
        web.built = web.strands.len();
        webs.webs.push(web);
    }

    let mut scripts = Vec::new();
    let mut spiders = Vec::new();
    for (i, s) in sc["spiders"].as_array().unwrap().iter().enumerate() {
        let script = Script { x0: f(&s["x0"]), y0: f(&s["y0"]), vx: f(&s["vx"]), vy: f(&s["vy"]) };
        let mut b = Bug::new(i as i64, "none", s["team"].as_str().unwrap(), script.x0, script.y0);
        b.neutral_is_nobody = false;
        b.size = f(&s["size"]);
        b.heading = script.vy.atan2(script.vx);
        b.speed = dc_world::pymath::hypot(script.vx, script.vy);
        b.homing = f(&s["homing"]);
        b.hunter = s["hunter"].as_bool().unwrap();
        spiders.push(b);
        scripts.push(script);
    }

    let events: std::collections::HashMap<i64, &Vec<Value>> = sc["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["frame"].as_i64().unwrap(), e["events"].as_array().unwrap()))
        .collect();
    let mut trace = sc["trace"].as_array().unwrap().iter().peekable();

    for fr in 0..frames {
        let ctx = format!("{name} frame {fr}");
        for (sp, s) in spiders.iter_mut().zip(&scripts) {
            place(sp, s, fr);
        }
        if (300..500).contains(&fr) {
            spiders[0].hunting = true;
            spiders[0].prey = world.flies.iter().find(|f| f.alive()).map(|f| f.id);
        } else {
            spiders[0].hunting = false;
            spiders[0].prey = None;
        }
        let prey = spiders[0].prey;

        for ev in events.get(&fr).copied().into_iter().flatten() {
            let fid = ev.get("fid").and_then(Value::as_u64).map(FlyId);
            match ev["kind"].as_str().unwrap() {
                "spawner" => {
                    world.add_spawner(pair(&ev["at"]));
                }
                "shot" => {
                    let shooter = ev["shooter"].as_u64().unwrap() as usize;
                    let ok = world.launch_web_shot(&spiders[shooter], fid.unwrap(), ev["shot"].as_str().unwrap());
                    assert_eq!(ok, ev["ok"].as_bool().unwrap(), "{ctx}: shot");
                }
                "shoot_spider" => {
                    let ok = world.launch_web_shot_at_creature(&spiders[0], &spiders[1], "trap");
                    assert_eq!(ok, ev["ok"].as_bool().unwrap(), "{ctx}: shot at a spider");
                }
                "aimed" => world.launch_aimed_silk(&spiders[1], f(&ev["angle"]), f(&ev["reach"])),
                "eat" => {
                    let fly = world.fly_mut(fid.unwrap()).expect("eaten fly");
                    fly.begin_eaten();
                    let (at, scale) = ((fly.x, fly.y), fly.scale);
                    world.add_remains(at, scale);
                }
                "drag_start" => {
                    let (x, y) = pair(&ev["at"]).unwrap();
                    world.start_drag_fly(fid.unwrap(), x, y);
                }
                "drag_to" => {
                    let (x, y) = pair(&ev["at"]).unwrap();
                    world.drag_fly_to(fid.unwrap(), x, y);
                }
                "drag_release" => {
                    let (x, y) = pair(&ev["at"]).unwrap();
                    world.release_drag_fly(fid.unwrap(), x, y);
                }
                "configure" => world.configure(FlyConfigure {
                    max_flies: Some(4),
                    min_interval: Some(0.2),
                    max_interval: Some(0.3),
                    ..FlyConfigure::default()
                }),
                "clear_hunters" => world.clear_hunters(),
                other => panic!("unknown event {other}"),
            }
        }

        world.update(DT, &mut spiders, Some(&mut webs));
        webs.update(DT, None);

        let Some(row) = trace.next_if(|r| r["frame"].as_i64() == Some(fr)) else {
            continue;
        };
        assert_eq!(prey.map(|p| p.0), row["prey"].as_u64(), "{ctx}: prey");
        let want = row["flies"].as_array().unwrap();
        assert_eq!(world.flies.len(), want.len(), "{ctx}: fly count");
        for (fly, w) in world.flies.iter().zip(want) {
            let fctx = format!("{ctx} fly {}", fly.id.0);
            assert_eq!(fly.id.0, w[0].as_u64().unwrap(), "{fctx}: id");
            assert_close(fly.x, &w[1], &format!("{fctx}: x"));
            assert_close(fly.y, &w[2], &format!("{fctx}: y"));
            assert_close(fly.heading, &w[3], &format!("{fctx}: heading"));
            assert_eq!(state_char(fly), w[4].as_str().unwrap(), "{fctx}: state");
            assert_close(fly.panic_level, &w[5], &format!("{fctx}: panic"));
            assert_close(fly.speed, &w[6], &format!("{fctx}: speed"));
        }
        let proj = row["proj"].as_array().unwrap();
        assert_eq!(world.projectiles.len(), proj.len(), "{ctx}: projectiles");
        for (p, w) in world.projectiles.iter().zip(proj) {
            let travelled = match p {
                Projectile::Shot(s) => s.travelled,
                Projectile::Aimed(a) => a.travelled,
            };
            assert_pair(Some(p.pos()), &json!([w[0], w[1]]), &format!("{ctx}: projectile"));
            assert_close(travelled, &w[2], &format!("{ctx}: travelled"));
        }
        assert_eq!(world.remains.len() as u64, row["remains"].as_u64().unwrap(), "{ctx}: remains");
        let nests = row["spawners"].as_array().unwrap();
        assert_eq!(world.spawners.len(), nests.len(), "{ctx}: nests");
        for (sp, w) in world.spawners.iter().zip(nests) {
            assert_close(sp.x, &w[0], &format!("{ctx}: nest x"));
            assert_close(sp.y, &w[1], &format!("{ctx}: nest y"));
            assert_close(sp.pulse, &w[2], &format!("{ctx}: nest pulse"));
        }
        for (web, w) in webs.webs.iter().zip(row["webs"].as_array().unwrap()) {
            assert_close(web.bounce_amp, w, &format!("{ctx}: web bounce"));
        }
        for (sp, w) in spiders.iter().zip(row["webbed"].as_array().unwrap()) {
            assert_eq!(sp.webbed, w.as_bool().unwrap(), "{ctx}: spider {} webbed", sp.index);
        }
        assert_eq!(world.dirty_rects().len() as u64, row["dirty"].as_u64().unwrap(), "{ctx}: dirty");
        assert_close(world.spawn_timer, &row["timer"], &format!("{ctx}: spawn timer"));
        if let Some(full) = sc["full"].get(fr.to_string()) {
            let full = full.as_array().unwrap();
            assert_eq!(world.flies.len(), full.len(), "{ctx}: full");
            for (fly, w) in world.flies.iter().zip(full) {
                assert_fly_full(fly, w, &format!("{ctx} full fly {}", fly.id.0));
            }
        }
    }
    assert!(trace.next().is_none(), "{name}: trace rows left over");
    for (sp, want) in spiders.iter().zip(sc["pins"].as_array().unwrap()) {
        let want: Vec<(i64, String, u64)> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_i64().unwrap(), p[1].as_str().unwrap().to_string(), p[2].as_u64().unwrap()))
            .collect();
        assert_eq!(sp.pins, want, "{name}: spider {} pins", sp.index);
    }
    assert_json(&Value::Array(world.spawners_to_dict()), &sc["nests"], &format!("{name}: nests"));
    assert_close(world.rng.random(), &sc["next"], &format!("{name}: stream position"));
}

#[test]
fn a_small_flock_matches_python() {
    let data = load("flies.json");
    run_scenario(&data["scenarios"][0]);
}

#[test]
fn a_crowd_on_two_monitors_matches_python() {
    // Forty spiders: the fear scan goes through the spatial grid.
    let data = load("flies.json");
    run_scenario(&data["scenarios"][1]);
}

#[test]
fn nests_remains_and_new_flies_match_python() {
    let data = load("flies.json");
    let p = &data["parts"];
    let mut world = FlyWorld::new(1200.0, 700.0, FlyWorldConfig::default(), PyRandom::from_str_seed("5:flies"));
    world.ensure_spawner();
    world.add_spawner(None);
    world.add_spawner(Some((5000.0, -40.0)));
    assert_json(&Value::Array(world.spawners_to_dict()), &p["nests"], "nests");

    let mut restored = FlyWorld::new(1200.0, 700.0, FlyWorldConfig::default(), PyRandom::from_u64(9));
    let entries = json!([{"x": 10, "y": "20"}, {"x": 400.0, "y": 300.0, "scale": 1.7},
                         {"y": 3}, "nope", {"x": "a", "y": 1}, {"x": 9e9, "y": -9e9, "scale": 0.1}]);
    assert_eq!(restored.restore_spawners(&entries) as u64, p["restored_count"].as_u64().unwrap());
    assert_json(&Value::Array(restored.spawners_to_dict()), &p["restored"], "restored nests");

    let mut rng = PyRandom::from_u64(42);
    for (i, w) in p["remains"].as_array().unwrap().iter().enumerate() {
        let r = FlyRemains::new(100.0 * i as f64, 50.0, 0.4 + i as f64 * 0.4, &mut rng);
        assert_close(r.scale, &w["scale"], "remains scale");
        let fp = r.footprint();
        assert_json(&json!([fp.0, fp.1, fp.2, fp.3]), &w["footprint"], "remains footprint");
        let parts = w["parts"].as_array().unwrap();
        assert_eq!(r.parts.len(), parts.len());
        for (part, q) in r.parts.iter().zip(parts) {
            let kind = match part.kind {
                PartKind::Leg => "leg",
                PartKind::Wing => "wing",
                PartKind::Crumb => "crumb",
            };
            assert_json(&json!([part.x, part.y, part.rot, part.len, kind]), q, "remains part");
        }
    }
    for (i, w) in p["flies"].as_array().unwrap().iter().enumerate() {
        let heading = if i % 2 == 1 { None } else { Some(0.5) };
        let fly = Fly::new(FlyId(i as u64), 300.0, 200.0, 1200.0, 700.0, heading, 0.3 + i as f64 * 0.3, &mut rng);
        assert_fly_full(&fly, w, &format!("new fly {i}"));
    }
    assert_close(rng.random(), &p["next"], "stream position");
    let _ = CreatureId(0);
}
