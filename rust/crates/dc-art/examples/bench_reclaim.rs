//! Timings on the owner's monitors (a 2560 x 1440 main screen and a 4K one
//! beside it): backdrop painting and the Reclaim desktop surface.
//!
//! ```text
//! CARGO_TARGET_DIR=target-dc-art cargo run --release -p dc-art --example bench_reclaim
//! ```

use std::time::Instant;

use dc_art::desktop_surface::{DesktopSurface, Hit, synthetic_snapshot};
use dc_art::mission_backdrop::{BackdropScreen, BackdropSite, LOOKS, MissionBackdrop, paint_backdrop};
use dc_core::Rect;
use dc_render::Scene;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let screens = [Rect::new(0.0, 718.0, 2560.0, 1440.0), Rect::new(2560.0, 0.0, 3840.0, 2160.0)];

    println!("-- backdrop (paint at DETAIL 0.5, smooth up) --");
    for look in LOOKS {
        let mut line = format!("{:9}", look.name);
        for r in &screens {
            let t = Instant::now();
            let img = paint_backdrop(look.name, r.w as u32, r.h as u32, &[], 0);
            line += &format!("  {}x{} {:7.1} ms", img.width(), img.height(), ms(t));
        }
        println!("{line}");
    }
    let mut backdrop = MissionBackdrop::new();
    let bs: Vec<BackdropScreen> =
        screens.iter().enumerate().map(|(index, rect)| BackdropScreen { index, rect: *rect }).collect();
    let sites = vec![
        BackdropSite { x: 600.0, y: 1500.0, kind: "home".into(), screen: 0 },
        BackdropSite { x: 4200.0, y: 900.0, kind: "nest".into(), screen: 1 },
    ];
    let t = Instant::now();
    backdrop.pixmaps("territory", "raid", &bs, &sites);
    let first = ms(t);
    let t = Instant::now();
    backdrop.pixmaps("territory", "raid", &bs, &sites);
    println!("MissionBackdrop both screens: first {first:.1} ms, cached {:.3} ms", ms(t));

    println!("\n-- Reclaim surface --");
    let windows = [
        Rect::new(200.0, 900.0, 1400.0, 900.0),
        Rect::new(900.0, 1200.0, 1300.0, 800.0),
        Rect::new(3000.0, 200.0, 1800.0, 1200.0),
        Rect::new(4200.0, 900.0, 1600.0, 1000.0),
    ];
    let t = Instant::now();
    let snap = synthetic_snapshot(&screens, &windows, 0, 30);
    println!("synthetic snapshot        {:8.1} ms", ms(t));
    let t = Instant::now();
    let mut s = DesktopSurface::new(snap.clone(), Some(2), true);
    println!("new (layers + words)      {:8.1} ms  ({} words)", ms(t), s.words.len());
    let t = Instant::now();
    let _ = DesktopSurface::new(snap, Some(2), false);
    println!("new without word finding  {:8.1} ms", ms(t));

    let t = Instant::now();
    for i in 0..20 {
        s.melt(1800.0 + i as f64 * 37.0, 1900.0, 30.0);
    }
    println!("melt                      {:8.2} ms each", ms(t) / 20.0);

    let t = Instant::now();
    let mut n = 0;
    for _ in 0..1000 {
        n += s.words_within(4000.0, 700.0, 300.0).len();
        n += s.word_near(1000.0, 1100.0, 40.0).is_some() as usize;
    }
    println!("words_within + word_near  {:8.4} ms per pair ({n})", ms(t) / 1000.0);

    let t = Instant::now();
    let mut cracks = 0;
    while s.crack(4500.0, 600.0, 1.0) != Some(Hit::Window) {
        cracks += 1;
    }
    println!("crack on a 4K window      {:8.2} ms each ({} cracks)", ms(t) / (cracks + 1) as f64, cracks + 1);
    let mut frames = Vec::new();
    while s.busy() {
        let t = Instant::now();
        s.update(1.0 / 60.0);
        frames.push(ms(t));
    }
    report("update, a window breaking", &frames);

    let t = Instant::now();
    s.break_desktop(Some((1000.0, 1500.0)));
    println!("break_desktop             {:8.2} ms", ms(t));
    let mut frames = Vec::new();
    while s.busy() {
        let t = Instant::now();
        s.update(1.0 / 60.0);
        frames.push(ms(t));
    }
    report("update, the desktop breaking", &frames);

    let mut scene = Scene::new();
    let t = Instant::now();
    for _ in 0..100 {
        scene.clear();
        let mut c = scene.canvas();
        s.paint(&mut c);
    }
    println!("paint (record the scene)  {:8.4} ms", ms(t) / 100.0);
}

fn report(what: &str, frames: &[f64]) {
    let mut sorted = frames.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = frames.iter().sum::<f64>() / frames.len().max(1) as f64;
    let p95 = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);
    println!("{what:26}{mean:8.2} ms mean, p95 {p95:.2}, max {max:.2} over {} frames", frames.len());
}
