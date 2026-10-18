//! The Rust art against Qt reference renders of the Python app.
//!
//! `tools/qt_reference.py` renders every backdrop look, map picture, skill
//! medallion, building, HUD, menu emblem and a seeded synthetic Reclaim
//! desktop through Qt into `target-review/qt/`. This test renders the same
//! scenes through `dc-render`'s `CpuRenderer`, compares them (mean channel
//! difference and structural similarity of the grey picture), writes
//! side-by-side sheets (Qt | Rust | difference x4) to `target-review/sheets/`
//! and `target-review/parity.txt`, and holds each group to a tolerance.
//! Without the reference renders (CI, a fresh checkout) it skips.
//!
//! ```text
//! <venv>/python.exe -I rust/crates/dc-art/tools/qt_reference.py
//! CARGO_TARGET_DIR=target-dc-art cargo test -p dc-art --release --test qt_parity -- --nocapture
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use dc_art::desktop_surface::{DesktopSurface, Hit, synthetic_snapshot};
use dc_art::map_art::map_picture;
use dc_art::mission_art::building_art;
use dc_art::mission_backdrop::{LOOKS, Site, paint_backdrop};
use dc_art::mission_ui::*;
use dc_art::skill_art::{SKILL_IDS, SkillLook, branch_crest, skill_icon, stat_icon};
use dc_art::ui_art::{MENU_EMBLEMS, card_frame, menu_emblem};
use dc_core::{Color, Rect};
use dc_render::{Canvas, Font, Image, RenderHint};

const W: u32 = 1600;
const H: u32 = 1000;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn canvas(f: impl FnOnce(&mut Canvas)) -> Image {
    let mut image = Image::new(W, H);
    image.paint(|p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_font(Font::new("Segoe UI", 9.0));
        f(p)
    });
    image
}

fn controller(dead: bool, name: &str) -> HudPlayer {
    HudPlayer {
        display_name: name.into(),
        level: 3,
        dead,
        hp: 46.0,
        max_hp: 100.0,
        energy: 80.0,
        max_energy: 100.0,
        attack_cooldown: 0.4,
        silk: 5.0,
        silk_capacity: 12.0,
        jump_cooldown: 0.0,
        web_cooldown: 1.25,
        jump_energy: 10.0,
        web_energy: 5.0,
        feedback: "Web ready".into(),
        feedback_time: 1.0,
        bindings: Some(["space".into(), "Mouse Left".into(), "Mouse Right".into(), "k".into()]),
    }
}

fn layout() -> UiLayout {
    UiLayout {
        screens: vec![
            UiScreen { index: 0, name: "Main".into(), rect: Rect::new(0.0, 0.0, 800.0, 1000.0) },
            UiScreen { index: 1, name: "Right".into(), rect: Rect::new(800.0, 0.0, 800.0, 1000.0) },
        ],
        links: vec![
            UiLink { a: 0, b: 1, tunnel: false, a_point: (780.0, 500.0), b_point: (820.0, 500.0) },
            UiLink { a: 0, b: 1, tunnel: true, a_point: (300.0, 820.0), b_point: (1300.0, 240.0) },
        ],
    }
}

fn mission() -> MissionView {
    let site = |x, y, kind: &str, name: &str| UiSite { x, y, kind: kind.into(), name: name.into(), ..Default::default() };
    MissionView {
        area: Rect::new(0.0, 0.0, 1600.0, 1000.0),
        layout: Some(layout()),
        here: 0,
        sites: vec![
            UiSite { owned: true, ..site(300.0, 420.0, "home", "Home") },
            UiSite { reserves: 3, progress: 0.4, ..site(1150.0, 380.0, "hatchery", "Hatchery") },
            UiSite { reserves: 2, warning: 2.25, ..site(1180.0, 760.0, "outpost", "Outpost") },
        ],
        actors: vec![UiActor { x: 900.0, y: 600.0, size: 20.0, windup: 0.3 }],
        hazards: vec![UiGlob { x: 700.0, y: 640.0, height: 30.0 }],
        splashes: vec![UiSplash { x: 760.0, y: 700.0, age: 0.3 }],
        threads: vec![UiThread { x0: 600.0, y0: 300.0, spider_x: 650.0, spider_y: 360.0, age: 0.2 }],
        creatures: vec![
            UiCreature { x: 420.0, y: 560.0, dead: false, relation: Relation::Hero },
            UiCreature { x: 1200.0, y: 400.0, dead: false, relation: Relation::Foe },
            UiCreature { x: 460.0, y: 600.0, dead: false, relation: Relation::Friend },
        ],
        objective: "Seal the hatchery and take the outpost before the swarm grows".into(),
        command: "defend".into(),
        allies: vec![UiAlly { display_name: "Silky".into(), hp: 30.0, max_hp: 60.0, dead: false }],
        hero_name: "Webby".into(),
        hero_level: 4,
        words_eaten: 12,
        ..Default::default()
    }
}

fn reclaim() -> (Image, Image) {
    let snap = synthetic_snapshot(
        &[Rect::new(0.0, 0.0, W as f64, H as f64)],
        &[Rect::new(200.0, 150.0, 700.0, 500.0), Rect::new(500.0, 300.0, 700.0, 500.0)],
        0,
        8,
    );
    let mut s = DesktopSurface::new(snap, Some(2), true);
    s.melt(1300.0, 800.0, 34.0);
    s.melt(1000.0, 500.0, 26.0);
    s.crack(1400.0, 250.0, 0.7);
    for (i, amount) in [(0, 1.0), (3, 0.5), (5, 0.5)] {
        s.eat(i, amount);
    }
    while s.crack(400.0, 400.0, 1.0) != Some(Hit::Window) {}
    for _ in 0..15 {
        s.update(0.02);
    }
    let mut shots = Vec::new();
    for _ in 0..2 {
        let mut image = Image::new(W, H);
        image.paint(|p| s.paint(p));
        shots.push(image);
        for _ in 0..20 {
            s.update(0.02);
        }
    }
    let after = shots.pop().unwrap();
    (shots.pop().unwrap(), after)
}

/// Every scene: (group, name, Rust picture).
fn scenes() -> Vec<(&'static str, String, Image)> {
    let mut out: Vec<(&'static str, String, Image)> = Vec::new();
    let sites = [Site::new(120.0, 200.0, "home"), Site::new(480.0, 160.0, "nest")];
    for look in LOOKS {
        out.push(("backdrop", format!("backdrop_{}", look.name), paint_backdrop(look.name, 640, 360, &sites, 0)));
    }
    for (id, kind) in [
        ("territory", "raid"),
        ("swarm", "swarm"),
        ("ember", "raid"),
        ("reclaim", "reclaim"),
        ("storm", "raid"),
        ("obsidian", "raid"),
        ("queen", "raid"),
        ("burrow", "raid"),
        ("custom-x", "swarm"),
    ] {
        out.push(("map", format!("map_{id}"), map_picture(id, kind, false)));
        out.push(("map", format!("map_{id}_locked"), map_picture(id, kind, true)));
    }
    for id in SKILL_IDS {
        for (look, name) in [(SkillLook::Learned, "learned"), (SkillLook::Available, "available"), (SkillLook::Locked, "locked")] {
            out.push(("skill", format!("skill_{id}_{name}"), skill_icon(id, look, 46)));
        }
    }
    for b in ["body", "fangs", "legs", "silk"] {
        out.push(("skill", format!("crest_{b}"), branch_crest(b, 34)));
    }
    for k in ["armor", "damage", "points", "worn", "speed", "silk"] {
        out.push(("skill", format!("stat_{k}"), stat_icon(k, 30)));
    }
    for kind in dc_art::mission_art::BUILDING_KINDS {
        out.push(("building", format!("building_{kind}"), building_art(kind, false)));
        out.push(("building", format!("building_{kind}_owned"), building_art(kind, true)));
    }
    let single = HudWindow::new(W as i32, H as i32);
    out.push(("hud", "hud_player".into(), canvas(|p| draw_hud(p, &single, &controller(false, "Webby"), 0))));
    let coop = HudWindow { players: 2, ..single.clone() };
    out.push(("hud", "hud_player_coop".into(), canvas(|p| draw_hud(p, &coop, &controller(true, "Second"), 1))));
    let mut m = mission();
    m.surface = Some(SurfaceHud { desktop_hp: 0.42, intact_windows: 1, hit_flash: 0.3 });
    m.intro_time = 0.5;
    m.command_bindings = Some(["1".into(), "2".into(), "3".into()]);
    let one = HudWindow { players: 1, ..single.clone() };
    out.push((
        "hud",
        "hud_mission".into(),
        canvas(|p| {
            draw_buildings(p, &m);
            draw_mission_hud(p, &one, &m);
        }),
    ));
    let crashed =
        MissionView { state: "crashed".into(), blue_screen: 2.0, crash_screen_seconds: 7.0, ended: true, ..mission() };
    out.push(("hud", "hud_bluescreen".into(), canvas(|p| draw_blue_screen(p, &crashed))));
    let won = MissionView { state: "victory".into(), ended: true, end_clock: 1.0, found: 2, amber_mined: 5, ..mission() };
    out.push(("hud", "hud_end".into(), canvas(|p| draw_end_title(p, &won))));
    out.push((
        "hud",
        "hud_released".into(),
        canvas(|p| draw_released_banner(p, &single, "F8", Some(Rect::new(0.0, 0.0, 1600.0, 1000.0)))),
    ));
    let (mid, after) = reclaim();
    out.push(("reclaim", "reclaim_mid".into(), mid));
    out.push(("reclaim", "reclaim_after".into(), after));
    for name in MENU_EMBLEMS {
        out.push(("emblem", format!("menu_{name}"), menu_emblem(name)));
    }
    out.push(("emblem", "card_frame".into(), card_frame()));
    out
}

/// Straight RGBA of a premultiplied image composited over mid grey (so
/// transparent areas compare as equal whatever their colour bits).
fn over_grey(image: &Image) -> Vec<[f64; 3]> {
    image
        .data()
        .chunks_exact(4)
        .map(|p| {
            let a = p[3] as f64 / 255.0;
            [p[0] as f64 + 128.0 * (1.0 - a), p[1] as f64 + 128.0 * (1.0 - a), p[2] as f64 + 128.0 * (1.0 - a)]
        })
        .collect()
}

struct Score {
    mean: f64,
    ssim: f64,
    bad: f64,
}

/// Mean absolute channel difference (0..255), mean SSIM of the grey
/// pictures over 8 x 8 blocks, and the share of pixels off by more than 48.
fn score(a: &Image, b: &Image) -> Score {
    let (w, h) = (a.width() as usize, a.height() as usize);
    let (pa, pb) = (over_grey(a), over_grey(b));
    let mut sum = 0.0;
    let mut bad = 0usize;
    for (x, y) in pa.iter().zip(&pb) {
        let d = (0..3).map(|c| (x[c] - y[c]).abs()).fold(0.0, f64::max);
        sum += (0..3).map(|c| (x[c] - y[c]).abs()).sum::<f64>() / 3.0;
        bad += (d > 48.0) as usize;
    }
    let grey = |p: &[f64; 3]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let (mut total, mut n) = (0.0, 0);
    for by in (0..h.saturating_sub(7)).step_by(8) {
        for bx in (0..w.saturating_sub(7)).step_by(8) {
            let (mut ma, mut mb, mut va, mut vb, mut cov) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for y in by..by + 8 {
                for x in bx..bx + 8 {
                    let (ga, gb) = (grey(&pa[y * w + x]), grey(&pb[y * w + x]));
                    ma += ga;
                    mb += gb;
                    va += ga * ga;
                    vb += gb * gb;
                    cov += ga * gb;
                }
            }
            let k = 64.0;
            let (ma, mb) = (ma / k, mb / k);
            let (va, vb, cov) = (va / k - ma * ma, vb / k - mb * mb, cov / k - ma * mb);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
        }
    }
    Score { mean: sum / pa.len() as f64, ssim: if n > 0 { total / n as f64 } else { 1.0 }, bad: bad as f64 / pa.len() as f64 }
}

/// Qt | Rust | |difference| x 4, side by side.
fn sheet(qt: &Image, ours: &Image) -> Image {
    let (w, h) = qt.size();
    let mut diff = Image::new(w, h);
    {
        let (a, b) = (over_grey(qt), over_grey(ours));
        let d = diff.data_mut();
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            for c in 0..3 {
                d[i * 4 + c] = ((x[c] - y[c]).abs() * 4.0).min(255.0) as u8;
            }
            d[i * 4 + 3] = 255;
        }
    }
    let mut out = Image::filled(w * 3 + 16, h, Color::rgb(128, 128, 128));
    out.paint(|p| {
        let (mut q, mut o) = (qt.clone(), ours.clone());
        q.set_device_pixel_ratio(1.0);
        o.set_device_pixel_ratio(1.0);
        p.draw_image_at(0.0, 0.0, &q);
        p.draw_image_at((w + 8) as f64, 0.0, &o);
        p.draw_image_at((2 * (w + 8)) as f64, 0.0, &diff);
    });
    out
}

#[test]
fn the_art_matches_qt_within_tolerance() {
    let refs = crate_dir().join("target-review").join("qt");
    if !refs.join("hud_player.png").exists() {
        eprintln!("no Qt reference renders in {}; run tools/qt_reference.py first (skipped)", refs.display());
        return;
    }
    let sheets = crate_dir().join("target-review").join("sheets");
    std::fs::create_dir_all(&sheets).unwrap();
    // Per group: (largest mean channel difference, smallest SSIM) allowed.
    let tolerance = |group: &str| match group {
        "backdrop" => (6.0, 0.80),
        "map" => (6.0, 0.80),
        "skill" => (6.0, 0.85),
        "building" => (2.0, 0.95),
        "hud" => (3.0, 0.90),
        "reclaim" => (3.0, 0.90),
        _ => (6.0, 0.85),
    };
    let mut report = String::new();
    let mut failures = Vec::new();
    let mut groups: std::collections::BTreeMap<&str, (f64, f64, f64, usize)> = Default::default();
    for (group, name, ours) in scenes() {
        let qt = Image::load_png(refs.join(format!("{name}.png"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(qt.size(), ours.size(), "{name}: size");
        let s = score(&qt, &ours);
        sheet(&qt, &ours).save_png(sheets.join(format!("{name}.png"))).unwrap();
        writeln!(report, "{group:9} {name:34} mean {:6.2}  ssim {:.3}  off>48 {:6.3}%", s.mean, s.ssim, s.bad * 100.0)
            .unwrap();
        let g = groups.entry(group).or_insert((0.0, 1.0, 0.0, 0));
        g.0 = g.0.max(s.mean);
        g.1 = g.1.min(s.ssim);
        g.2 += s.mean;
        g.3 += 1;
        let (max_mean, min_ssim) = tolerance(group);
        if s.mean > max_mean || s.ssim < min_ssim {
            failures.push(format!("{name}: mean {:.2} ssim {:.3}", s.mean, s.ssim));
        }
    }
    writeln!(report, "\ngroup      scenes  worst mean  average mean  worst ssim").unwrap();
    for (g, (worst, ssim, total, n)) in &groups {
        writeln!(report, "{g:9} {n:7}  {worst:10.2}  {:12.2}  {ssim:10.3}", total / *n as f64).unwrap();
    }
    std::fs::write(crate_dir().join("target-review").join("parity.txt"), &report).unwrap();
    println!("{report}");
    assert!(failures.is_empty(), "outside tolerance:\n{}", failures.join("\n"));
}
