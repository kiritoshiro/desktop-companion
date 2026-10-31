//! Port of app/mission_backdrop.py: a painted map under a mission, instead
//! of the desktop.
//!
//! Every map has its own ground: the meadow of *Take back the desktop*, the
//! flowered meadow of the fly levels, the scorched earth of *Ember hollow*,
//! black glass for *Obsidian deep*, thorns and roots for the Queen, and a
//! dark digital waste for *Reclaim the desktop*. Trodden paths run from home
//! to every building, each building stands in a clearing, and props suit the
//! place.
//!
//! The picture is painted once per screen and size -- at half size, then
//! smoothed up -- and kept, so showing it costs one image draw a frame. The
//! random draws are CPython's, seeded and ordered exactly as the Python.

use std::collections::HashMap;
use std::f64::consts::TAU;

use dc_core::{Color, PyRandom, Rect};
use dc_render::{Brush, Canvas, Image, LinearGradient, Path, Pen, RadialGradient, RenderHint};

use crate::common::{alpha, round_cap_pen, round_pen};
use crate::qt::{darker, hex, lighter};

/// Painted at this share of the screen's size, then smoothed up.
pub const DETAIL: f64 = 0.5;

/// One look: ground top, ground bottom, path, props.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub name: &'static str,
    pub ground: (&'static str, &'static str),
    pub path: &'static str,
    pub props: &'static [&'static str],
}

pub const LOOKS: [Look; 7] = [
    Look {
        name: "territory",
        ground: ("#76853f", "#5c6b33"),
        path: "#8f7248",
        props: &["grass", "pebbles", "logs", "flowers"],
    },
    Look {
        name: "swarm",
        ground: ("#7c9444", "#5f7a36"),
        path: "#94784c",
        props: &["grass", "flowers", "flowers", "pebbles"],
    },
    Look { name: "ember", ground: ("#5c301d", "#3c1d12"), path: "#7c4c2c", props: &["ash", "lava", "embers", "rocks"] },
    Look { name: "reclaim", ground: ("#12264a", "#081328"), path: "#1e4a7c", props: &["shards", "glow"] },
    Look { name: "storm", ground: ("#3f4c37", "#2b3528"), path: "#5c5242", props: &["grass", "puddles", "rocks"] },
    Look { name: "obsidian", ground: ("#211b2c", "#120e1a"), path: "#3c3250", props: &["rocks", "crystals", "cracks"] },
    Look {
        name: "queen",
        ground: ("#43201c", "#2a1110"),
        path: "#6c3c2a",
        props: &["roots", "thorns", "bones", "rocks"],
    },
];

/// The look by name (falls back to territory).
pub fn look(name: &str) -> &'static Look {
    LOOKS.iter().find(|l| l.name == name).unwrap_or(&LOOKS[0])
}

/// A map made in the editor borrows the look of its kind.
pub fn kind_look(kind: &str) -> Option<&'static str> {
    match kind {
        "raid" => Some("territory"),
        "reclaim" => Some("reclaim"),
        "swarm" => Some("swarm"),
        _ => None,
    }
}

/// `look_for(map_id, kind)`.
pub fn look_for(map_id: &str, kind: &str) -> &'static str {
    if let Some(l) = LOOKS.iter().find(|l| l.name == map_id) {
        return l.name;
    }
    kind_look(kind).unwrap_or("territory")
}

/// `_shade(color, amount)`.
fn shade(c: Color, amount: i32) -> Color {
    if amount >= 100 { lighter(c, amount) } else { darker(c, (10000.0 / amount.max(1) as f64) as i32) }
}

/// How many of each prop per million painted pixels.
pub fn density(name: &str) -> f64 {
    match name {
        "grass" => 900.0,
        "flowers" => 260.0,
        "pebbles" => 240.0,
        "rocks" => 22.0,
        "logs" => 5.0,
        "ash" => 200.0,
        "lava" => 12.0,
        "embers" => 160.0,
        "crystals" => 22.0,
        "cracks" => 140.0,
        "roots" => 40.0,
        "thorns" => 160.0,
        "bones" => 9.0,
        "puddles" => 30.0,
        "shards" => 90.0,
        "glow" => 6.0,
        _ => 0.0,
    }
}

/// A building on a screen: position in that screen's own pixels, and kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub x: f64,
    pub y: f64,
    pub kind: String,
}

impl Site {
    pub fn new(x: f64, y: f64, kind: &str) -> Self {
        Site { x, y, kind: kind.to_string() }
    }
}

// -- the ground -----------------------------------------------------------------

fn ground(p: &mut Canvas, w: f64, h: f64, lk: &Look, rng: &mut PyRandom) {
    let (top, bottom) = (hex(lk.ground.0), hex(lk.ground.1));
    let grad = LinearGradient::new(0.0, 0.0, 0.0, h).with_stop(0.0, top).with_stop(1.0, bottom);
    p.fill_rect(Rect::new(0.0, 0.0, w, h), grad);
    p.set_pen(Pen::NONE);
    // Mottling: soft patches lighter and darker than the ground.
    for _ in 0..((w * h / 900.0) as i64) {
        let x = rng.uniform(0.0, w);
        let y = rng.uniform(0.0, h);
        let r = rng.uniform(4.0, 26.0);
        let base = if rng.random() < 0.5 { top } else { bottom };
        let tone = if rng.random() < 0.5 {
            lighter(base, rng.randint(108, 130) as i32)
        } else {
            darker(base, rng.randint(110, 140) as i32)
        };
        let tone = alpha(tone, rng.randint(18, 45));
        p.set_brush(tone);
        let ry = r * rng.uniform(0.5, 1.0);
        p.draw_ellipse_at(x, y, r, ry);
    }
}

fn paths(p: &mut Canvas, sites: &[Site], lk: &Look, scale: f64, rng: &mut PyRandom) {
    if sites.is_empty() {
        return;
    }
    let colour = hex(lk.path);
    let home = sites.iter().find(|s| s.kind == "home").unwrap_or(&sites[0]).clone();
    let (hx, hy) = (home.x * scale, home.y * scale);
    for site in sites {
        if *site == home {
            continue;
        }
        let (x, y) = (site.x * scale, site.y * scale);
        let mx = (hx + x) / 2.0 + rng.uniform(-60.0, 60.0) * scale * 2.0;
        let my = (hy + y) / 2.0 + rng.uniform(-50.0, 50.0) * scale * 2.0;
        let mut path = Path::new();
        path.move_to(hx, hy);
        path.quad_to(mx, my, x, y);
        for (width, a, tone) in [(58.0, 90, 90), (40.0, 150, 100), (22.0, 110, 118)] {
            let c = alpha(shade(colour, tone), a);
            p.set_pen(round_pen(c, width * scale));
            p.set_brush(Brush::None);
            p.draw_path(&path);
        }
    }
    p.set_pen(Pen::NONE);
    for site in sites {
        let (x, y) = (site.x * scale, site.y * scale);
        let inner = alpha(colour, 170);
        let outer = alpha(colour, 0);
        let glow = RadialGradient::new(x, y + 20.0 * scale, 150.0 * scale)
            .with_stop(0.0, inner)
            .with_stop(0.55, inner)
            .with_stop(1.0, outer);
        p.set_brush(glow);
        p.draw_ellipse_at(x, y + 20.0 * scale, 150.0 * scale, 95.0 * scale);
    }
}

// -- props ------------------------------------------------------------------------

fn grass(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, lk: &Look) {
    let base = hex(lk.ground.0);
    for _ in 0..rng.randint(4, 8) {
        let c = if rng.random() < 0.6 { lighter(base, rng.randint(110, 150) as i32) } else { darker(base, 125) };
        let c = alpha(c, 210);
        p.set_pen(round_cap_pen(c, 1.2 * s));
        let dx = rng.uniform(-5.0, 5.0) * s;
        let x2 = x + dx + rng.uniform(-3.0, 3.0) * s;
        let y2 = y - rng.uniform(5.0, 11.0) * s;
        p.draw_line(x + dx, y, x2, y2);
    }
}

const FLOWER_COLOURS: [&str; 5] = ["#f4e9c8", "#f2c94c", "#e07a8a", "#b9a6ee", "#ffffff"];

fn flowers(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.set_pen(Pen::NONE);
    for _ in 0..rng.randint(2, 5) {
        let c = hex(rng.choice(&FLOWER_COLOURS));
        let px = x + rng.uniform(-8.0, 8.0) * s;
        let py = y + rng.uniform(-5.0, 5.0) * s;
        p.set_brush(c);
        p.draw_ellipse_at(px, py, 1.6 * s, 1.6 * s);
    }
}

fn pebbles(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    for _ in 0..rng.randint(1, 4) {
        let r = rng.uniform(1.5, 4.5) * s;
        let px = x + rng.uniform(-9.0, 9.0) * s;
        let py = y + rng.uniform(-6.0, 6.0) * s;
        let cr = rng.randint(130, 170) as u8;
        let cg = rng.randint(120, 150) as u8;
        let cb = rng.randint(100, 130) as u8;
        let base = Color::rgb(cr, cg, cb);
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(0, 0, 0, 60));
        p.draw_ellipse_at(px + r * 0.3, py + r * 0.35, r, r * 0.7);
        p.set_brush(base);
        p.draw_ellipse_at(px, py, r, r * 0.72);
        p.set_brush(lighter(base, 140));
        p.draw_ellipse_at(px - r * 0.3, py - r * 0.25, r * 0.35, r * 0.22);
    }
}

fn rocks(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, lk: &Look) {
    let dark = ["obsidian", "ember", "queen", "storm"].contains(&lk.name);
    let r = rng.uniform(7.0, 16.0) * s;
    let mut path = Path::new();
    for i in 0..7 {
        let a = i as f64 / 7.0 * TAU;
        let px = x + a.cos() * r * rng.uniform(0.75, 1.15);
        let py = y + a.sin() * r * 0.62 * rng.uniform(0.8, 1.1);
        if i == 0 {
            path.move_to(px, py);
        } else {
            path.line_to(px, py);
        }
    }
    path.close_subpath();
    let base = if lk.name == "obsidian" {
        Color::rgb(52, 44, 58)
    } else if dark {
        Color::rgb(78, 66, 58)
    } else {
        Color::rgb(128, 116, 98)
    };
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(0, 0, 0, 70));
    p.draw_path(&path.translated(r * 0.35, r * 0.3));
    let grad = LinearGradient::new(x - r, y - r, x + r, y + r)
        .with_stop(0.0, lighter(base, 150))
        .with_stop(1.0, darker(base, 130));
    p.set_brush(grad);
    p.draw_path(&path);
}

fn logs(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    let length = rng.uniform(22.0, 40.0) * s;
    let r = rng.uniform(3.5, 6.0) * s;
    let angle = rng.uniform(-0.5, 0.5);
    p.save();
    p.translate(x, y);
    p.rotate(angle.to_degrees());
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(0, 0, 0, 60));
    p.draw_rounded_rect(Rect::new(-length / 2.0 + 3.0 * s, -r + 3.0 * s, length, r * 2.0), r, r);
    let grad = LinearGradient::new(0.0, -r, 0.0, r).with_stop(0.0, hex("#8a6440")).with_stop(1.0, hex("#4a3220"));
    p.set_brush(grad);
    p.draw_rounded_rect(Rect::new(-length / 2.0, -r, length, r * 2.0), r, r);
    p.set_brush(hex("#c8a476"));
    p.draw_ellipse_at(length / 2.0 - r * 0.4, 0.0, r * 0.55, r * 0.9);
    p.restore();
}

fn ash(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.set_pen(Pen::NONE);
    let c = Color::rgba(120, 110, 104, rng.randint(40, 80) as u8);
    p.set_brush(c);
    let rx = rng.uniform(8.0, 22.0) * s;
    let ry = rng.uniform(4.0, 10.0) * s;
    p.draw_ellipse_at(x, y, rx, ry);
}

fn lava(p: &mut Canvas, mut x: f64, mut y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    let mut path = Path::new();
    path.move_to(x, y);
    let mut a = rng.uniform(0.0, TAU);
    for _ in 0..rng.randint(3, 6) {
        a += rng.uniform(-0.8, 0.8);
        let nx = x + a.cos() * rng.uniform(8.0, 18.0) * s;
        let ny = y + a.sin() * rng.uniform(6.0, 14.0) * s;
        x = nx;
        y = ny;
        path.line_to(x, y);
    }
    for (width, colour) in [
        (6.0, Color::rgba(255, 90, 20, 60)),
        (2.6, Color::rgba(255, 140, 40, 200)),
        (1.0, Color::rgba(255, 230, 150, 230)),
    ] {
        p.set_pen(round_pen(colour, width * s));
        p.set_brush(Brush::None);
        p.draw_path(&path);
    }
}

fn embers(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.set_pen(Pen::NONE);
    for _ in 0..rng.randint(1, 3) {
        let px = x + rng.uniform(-10.0, 10.0) * s;
        let py = y + rng.uniform(-8.0, 8.0) * s;
        p.set_brush(Color::rgba(255, 120, 30, 70));
        p.draw_ellipse_at(px, py, 4.0 * s, 4.0 * s);
        p.set_brush(Color::rgba(255, rng.randint(170, 220) as u8, 80, 230));
        p.draw_ellipse_at(px, py, 1.3 * s, 1.3 * s);
    }
}

fn crystals(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    for _ in 0..rng.randint(2, 4) {
        let h = rng.uniform(10.0, 24.0) * s;
        let wdt = h * rng.uniform(0.25, 0.4);
        let lean = rng.uniform(-0.35, 0.35);
        let bx = x + rng.uniform(-8.0, 8.0) * s;
        let mut path = Path::new();
        path.move_to(bx - wdt, y);
        path.line_to(bx - wdt * 0.6, y - h * 0.7);
        path.line_to(bx + lean * h, y - h);
        path.line_to(bx + wdt * 0.6, y - h * 0.7);
        path.line_to(bx + wdt, y);
        path.close_subpath();
        let grad = LinearGradient::new(bx - wdt, y - h, bx + wdt, y)
            .with_stop(0.0, Color::rgba(196, 150, 255, 235))
            .with_stop(1.0, Color::rgba(70, 40, 120, 235));
        p.set_pen(Pen::new(Color::rgba(230, 210, 255, 140), 0.8 * s));
        p.set_brush(grad);
        p.draw_path(&path);
    }
}

fn cracks(p: &mut Canvas, mut x: f64, mut y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.set_pen(round_cap_pen(Color::rgba(150, 110, 220, 110), 1.1 * s));
    let mut a = rng.uniform(0.0, TAU);
    for _ in 0..rng.randint(2, 4) {
        let nx = x + a.cos() * rng.uniform(10.0, 24.0) * s;
        let ny = y + a.sin() * rng.uniform(8.0, 18.0) * s;
        p.draw_line(x, y, nx, ny);
        x = nx;
        y = ny;
        a += rng.uniform(-0.9, 0.9);
    }
}

fn roots(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    let mut path = Path::new();
    path.move_to(x, y);
    let c1x = x + rng.uniform(-30.0, 30.0) * s;
    let c1y = y + rng.uniform(-20.0, 20.0) * s;
    let c2x = x + rng.uniform(-40.0, 40.0) * s;
    let c2y = y + rng.uniform(-20.0, 20.0) * s;
    let ex = x + rng.uniform(-60.0, 60.0) * s;
    let ey = y + rng.uniform(-30.0, 30.0) * s;
    path.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
    p.set_pen(round_cap_pen(Color::rgba(30, 14, 10, 200), rng.uniform(2.5, 4.5) * s));
    p.set_brush(Brush::None);
    p.draw_path(&path);
    p.set_pen(round_cap_pen(Color::rgba(90, 50, 34, 150), 1.0 * s));
    p.draw_path(&path.translated(-0.8 * s, -0.8 * s));
}

fn thorns(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.set_pen(Pen::NONE);
    for _ in 0..rng.randint(2, 4) {
        let bx = x + rng.uniform(-10.0, 10.0) * s;
        let h = rng.uniform(8.0, 16.0) * s;
        let lean = rng.uniform(-4.0, 4.0) * s;
        let mut path = Path::new();
        path.move_to(bx - 2.0 * s, y);
        path.quad_to(bx + lean * 0.3, y - h * 0.5, bx + lean, y - h);
        path.quad_to(bx + lean * 0.4, y - h * 0.4, bx + 2.0 * s, y);
        path.close_subpath();
        p.set_brush(Color::rgba(28, 10, 10, 230));
        p.draw_path(&path);
    }
}

fn bones(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    p.save();
    p.translate(x, y);
    p.rotate(rng.uniform(0.0, 180.0));
    p.set_pen(Pen::new(Color::rgba(60, 50, 40, 160), 0.8 * s));
    p.set_brush(Color::rgb(226, 214, 190));
    let length = rng.uniform(8.0, 14.0) * s;
    p.draw_rounded_rect(Rect::new(-length / 2.0, -1.2 * s, length, 2.4 * s), 1.2 * s, 1.2 * s);
    for end in [-1.0, 1.0] {
        for side in [-1.0, 1.0] {
            p.draw_ellipse_at(end * length / 2.0, side * 1.6 * s, 1.6 * s, 1.6 * s);
        }
    }
    p.restore();
}

fn puddles(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    let rx = rng.uniform(10.0, 26.0) * s;
    let ry = rng.uniform(5.0, 11.0) * s;
    let grad = RadialGradient::new(x - rx * 0.3, y - ry * 0.3, rx)
        .with_stop(0.0, Color::rgba(150, 170, 190, 170))
        .with_stop(1.0, Color::rgba(60, 76, 90, 170));
    p.set_pen(Pen::new(Color::rgba(40, 36, 30, 120), 1.0 * s));
    p.set_brush(grad);
    p.draw_ellipse_at(x, y, rx, ry);
}

fn grid(p: &mut Canvas, w: f64, h: f64, s: f64) {
    p.set_pen(Pen::new(Color::rgba(70, 140, 220, 38), 1.0));
    let step = 48.0 * s;
    let mut x = 0.0;
    while x < w {
        p.draw_line(x, 0.0, x, h);
        x += step;
    }
    let mut y = 0.0;
    while y < h {
        p.draw_line(0.0, y, w, y);
        y += step;
    }
}

fn shards(p: &mut Canvas, x: f64, y: f64, s: f64, rng: &mut PyRandom, _lk: &Look) {
    let mut path = Path::new();
    let r = rng.uniform(8.0, 20.0) * s;
    for i in 0..3 {
        let a = rng.uniform(0.0, TAU);
        let (px, py) = (x + a.cos() * r, y + a.sin() * r);
        if i == 0 {
            path.move_to(px, py);
        } else {
            path.line_to(px, py);
        }
    }
    path.close_subpath();
    p.set_pen(Pen::new(Color::rgba(150, 210, 255, 150), 0.8 * s));
    p.set_brush(Color::rgba(40, 90, 160, 110));
    p.draw_path(&path);
}

fn glow(p: &mut Canvas, x: f64, y: f64, s: f64, _rng: &mut PyRandom, _lk: &Look) {
    let g = RadialGradient::new(x, y, 30.0 * s)
        .with_stop(0.0, Color::rgba(120, 230, 90, 70))
        .with_stop(1.0, Color::rgba(120, 230, 90, 0));
    p.set_pen(Pen::NONE);
    p.set_brush(g);
    p.draw_ellipse_at(x, y, 30.0 * s, 30.0 * s);
}

type PropFn = fn(&mut Canvas, f64, f64, f64, &mut PyRandom, &Look);

fn prop(name: &str) -> Option<PropFn> {
    Some(match name {
        "grass" => grass,
        "flowers" => flowers,
        "pebbles" => pebbles,
        "rocks" => rocks,
        "logs" => logs,
        "ash" => ash,
        "lava" => lava,
        "embers" => embers,
        "crystals" => crystals,
        "cracks" => cracks,
        "roots" => roots,
        "thorns" => thorns,
        "bones" => bones,
        "puddles" => puddles,
        "shards" => shards,
        "glow" => glow,
        _ => return None,
    })
}

fn vignette(p: &mut Canvas, w: f64, h: f64) {
    let g = RadialGradient::new(w / 2.0, h / 2.0, w.max(h) * 0.72)
        .with_stop(0.55, Color::rgba(0, 0, 0, 0))
        .with_stop(1.0, Color::rgba(0, 0, 0, 120));
    p.fill_rect(Rect::new(0.0, 0.0, w, h), g);
}

/// The ground of one screen at half size (`DETAIL`), before smoothing up.
/// Exposed for timing; [`paint_backdrop`] is the full call.
pub fn paint_backdrop_detail(look_name: &str, width: u32, height: u32, sites: &[Site], seed: i64) -> Image {
    let lk = look(look_name);
    let w = ((width as f64 * DETAIL) as i64).max(8) as f64;
    let h = ((height as f64 * DETAIL) as i64).max(8) as f64;
    let mut image = Image::new(w as u32, h as u32);
    let mut rng = PyRandom::from_str_seed(&format!("{look_name}-{seed}-{width}x{height}"));
    image.paint(|p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        ground(p, w, h, lk, &mut rng);
        let s = DETAIL * 2.0; // prop sizes are in full-size pixels / 2
        if lk.name == "reclaim" {
            grid(p, w, h, s);
        }
        paths(p, sites, lk, DETAIL, &mut rng);
        let megapixels = w * h / 1_000_000.0;
        let near: Vec<(f64, f64)> = sites.iter().map(|st| (st.x * DETAIL, st.y * DETAIL)).collect();
        let clear = (95.0 * DETAIL * 2.0) * (95.0 * DETAIL * 2.0);
        for name in lk.props {
            let Some(painter) = prop(name) else { continue };
            for _ in 0..((density(name) * megapixels * 4.0) as i64) {
                let x = rng.uniform(0.0, w);
                let y = rng.uniform(0.0, h);
                // Keep the clearings round the buildings clear of props.
                if near.iter().any(|&(sx, sy)| (x - sx).powi(2) + ((y - sy) * 1.5).powi(2) < clear) {
                    continue;
                }
                painter(p, x, y, s, &mut rng, lk);
            }
        }
        vignette(p, w, h);
    });
    image
}

/// `paint_backdrop(look, width, height, sites, seed)`: the ground of one
/// screen, `width` x `height`. `sites`: the buildings on it, in this screen's
/// own pixels. Opaque.
pub fn paint_backdrop(look_name: &str, width: u32, height: u32, sites: &[Site], seed: i64) -> Image {
    paint_backdrop_detail(look_name, width, height, sites, seed).smooth_scaled(width, height)
}

/// One screen of a mission for the backdrop: its index and overlay rect.
#[derive(Clone, Copy, Debug)]
pub struct BackdropScreen {
    pub index: usize,
    pub rect: Rect,
}

/// One mission site for the backdrop: overlay position, kind and screen.
#[derive(Clone, Debug)]
pub struct BackdropSite {
    pub x: f64,
    pub y: f64,
    pub kind: String,
    pub screen: usize,
}

type CacheKey = (String, usize, i64, i64, Vec<(i64, i64, String)>);

/// Paints each screen's map under a mission, cached per screen and size.
#[derive(Default)]
pub struct MissionBackdrop {
    cache: HashMap<CacheKey, Image>,
}

impl MissionBackdrop {
    pub fn new() -> Self {
        Self::default()
    }

    /// (screen rect, picture) for every screen of the mission. `map_id` and
    /// `kind` are the mission map's (`look_for`).
    pub fn pixmaps(
        &mut self,
        map_id: &str,
        kind: &str,
        screens: &[BackdropScreen],
        sites: &[BackdropSite],
    ) -> Vec<(Rect, Image)> {
        let look_name = look_for(map_id, kind);
        let mut result = Vec::new();
        for screen in screens {
            let r = screen.rect;
            let local: Vec<(i64, i64, String)> = sites
                .iter()
                .filter(|s| s.screen == screen.index)
                .map(|s| {
                    (
                        dc_core::math::py_round(s.x - r.x) as i64,
                        dc_core::math::py_round(s.y - r.y) as i64,
                        s.kind.clone(),
                    )
                })
                .collect();
            let key: CacheKey = (map_id.to_string(), screen.index, r.w as i64, r.h as i64, local.clone());
            let image = self
                .cache
                .entry(key)
                .or_insert_with(|| {
                    let sites: Vec<Site> =
                        local.iter().map(|(x, y, k)| Site { x: *x as f64, y: *y as f64, kind: k.clone() }).collect();
                    paint_backdrop(look_name, r.w as u32, r.h as u32, &sites, screen.index as i64)
                })
                .clone();
            result.push((r, image));
        }
        result
    }

    /// Draw every screen's map.
    pub fn paint(
        &mut self,
        painter: &mut Canvas,
        map_id: &str,
        kind: &str,
        screens: &[BackdropScreen],
        sites: &[BackdropSite],
    ) {
        for (r, image) in self.pixmaps(map_id, kind, screens, sites) {
            let src = Rect::new(0.0, 0.0, image.width() as f64, image.height() as f64);
            painter.draw_pixmap(r, &image, src);
        }
    }

    /// Number of cached pictures.
    pub fn cached(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(c: Color) -> (u8, u8, u8) {
        (c.r, c.g, c.b)
    }

    #[test]
    fn every_map_has_its_own_painted_ground() {
        let sites = [Site::new(120.0, 200.0, "home"), Site::new(480.0, 160.0, "nest")];
        let mut colours = std::collections::HashSet::new();
        for lk in LOOKS {
            let image = paint_backdrop(lk.name, 640, 360, &sites, 0);
            assert_eq!((image.width(), image.height()), (640, 360));
            colours.insert(name(image.pixel(320, 330)));
        }
        assert_eq!(colours.len(), LOOKS.len());
    }

    #[test]
    fn a_path_runs_from_home_to_each_building() {
        let bare = paint_backdrop("territory", 800, 450, &[], 0);
        let with_sites = paint_backdrop(
            "territory",
            800,
            450,
            &[Site::new(100.0, 225.0, "home"), Site::new(700.0, 225.0, "nest")],
            0,
        );
        assert_ne!(name(with_sites.pixel(700, 245)), name(bare.pixel(700, 245)));
    }

    #[test]
    fn an_editor_map_borrows_its_kinds_look() {
        assert_eq!(look_for("custom-abc", "swarm"), "swarm");
        assert_eq!(look_for("queen", "raid"), "queen");
        assert_eq!(look_for("custom-abc", "raid"), "territory");
    }

    #[test]
    fn the_backdrop_is_painted_once_and_reused() {
        let mut backdrop = MissionBackdrop::new();
        let screens = [
            BackdropScreen { index: 0, rect: Rect::new(0.0, 0.0, 320.0, 200.0) },
            BackdropScreen { index: 1, rect: Rect::new(320.0, 0.0, 300.0, 200.0) },
        ];
        let sites = [BackdropSite { x: 100.4, y: 50.6, kind: "home".into(), screen: 0 }];
        let first = backdrop.pixmaps("territory", "raid", &screens, &sites);
        let second = backdrop.pixmaps("territory", "raid", &screens, &sites);
        assert_eq!(
            first.iter().map(|(_, i)| i.id()).collect::<Vec<_>>(),
            second.iter().map(|(_, i)| i.id()).collect::<Vec<_>>()
        );
        assert_eq!(backdrop.cached(), 2);
        let mut image = Image::new(64, 64);
        image.paint(|p| backdrop.paint(p, "territory", "raid", &screens, &sites));
        assert!(image.pixel(10, 10).a == 255);
    }
}
