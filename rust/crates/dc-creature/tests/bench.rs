//! `cargo test --release -p dc-creature --test bench -- --ignored --nocapture`:
//! the cost of one `Creature::update` in a crowd.

mod common;

use dc_creature::Creature;
use dc_creature::world::{Command, Env, Population};
use std::time::Instant;

#[test]
#[ignore]
fn crowd_update_cost() {
    for &n in &[50usize, 200] {
        let model = common::load_model("tarantula");
        let names = ["mellow", "playful", "hunter", "drifter", "curious", "jumper"];
        let mut crowd: Vec<Creature> = (0..n)
            .map(|i| {
                let p = common::load_personality(names[i % names.len()]);
                common::spider_with(model.clone(), p, 7, |cfg| {
                    cfg.id = i as u64 + 1;
                    cfg.index = i as i64;
                    cfg.progression_id = Some(format!("bench:{i}"));
                })
            })
            .collect();
        let ids: Vec<u64> = crowd.iter().map(|c| c.id).collect();
        let mut pop = Population::from_views(crowd.iter().map(|c| c.view()));
        let mut cmds: Vec<Command> = Vec::new();
        let frames = 300;
        let start = Instant::now();
        for f in 0..frames {
            let t = f as f64 / 60.0;
            let (mx, my) = (800.0 + 500.0 * (t * 0.7).cos(), 450.0 + 300.0 * (t * 1.1).sin());
            for c in crowd.iter_mut() {
                c.neighbors.clear();
                c.neighbors.extend_from_slice(&ids);
                let mut env = Env::bare(&pop, &mut cmds);
                c.update(1.0 / 60.0, mx, my, 1600.0, 900.0, &mut env);
                pop.set(c.view());
            }
            cmds.clear();
        }
        let per = start.elapsed().as_secs_f64() * 1e6 / (frames * n) as f64;
        println!("{n} creatures: {per:.1} us per creature per frame");
    }
}
