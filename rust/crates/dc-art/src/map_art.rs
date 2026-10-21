//! Port of app/map_art.py: small painted scenes for the map cards on the
//! level selection.
//!
//! Each map gets a banner: its ground and light, a few of its wooden
//! buildings from [`crate::mission_art`], and what makes it itself -- flies,
//! embers, acid on a frozen desktop, black crystal, thorns. Painted once at
//! twice the size and cached.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Mutex, OnceLock};

use dc_core::{Color, PyRandom, Rect};
use dc_render::{CompositionMode, Image, LinearGradient, Path, Pen, RadialGradient, RenderHint};

use crate::mission_art::{ART_H, ART_W, building_art};
use crate::qt::hex;

pub const PICTURE_W: u32 = 216;
pub const PICTURE_H: u32 = 92;
pub const SCALE: u32 = 2;

/// sky top, sky bottom / horizon, ground near, ground far
pub const THEMES: [(&str, [&str; 4]); 8] = [
    ("territory", ["#9cc3d8", "#e8d9a8", "#6f7a3c", "#8d8a4c"]),
    ("swarm", ["#b7d3c4", "#efe2a6", "#6c7e3a", "#94904e"]),
    ("ember", ["#3a1a14", "#d8702c", "#4e2e1c", "#7a4424"]),
    ("reclaim", ["#1d4f86", "#3f7fbf", "#1a3c64", "#2a5a8c"]),
    ("storm", ["#2c3440", "#6a7482", "#3e4636", "#565c48"]),
    ("obsidian", ["#140f22", "#4a3a6a", "#221c2a", "#3a3044"]),
    ("queen", ["#2a0c14", "#9a2c34", "#3e1e1a", "#5e2e24"]),
    ("burrow", ["#6a8aa0", "#d8c49a", "#5e4a30", "#7e6644"]),
];

/// Buildings on the banner: kind, owned, x centre (0..1), scale.
pub fn scene(look: &str) -> &'static [(&'static str, bool, f64, f64)] {
    match look {
        "swarm" => &[("flynest", false, 0.30, 0.40), ("food", true, 0.78, 0.36)],
        "ember" => &[("venom", false, 0.26, 0.40), ("nest", false, 0.74, 0.44)],
        "reclaim" => &[("nest", false, 0.70, 0.40)],
        "storm" => &[("flynest", false, 0.24, 0.40), ("flynest", false, 0.74, 0.36)],
        "obsidian" => &[("infestation", false, 0.28, 0.40), ("nest", false, 0.74, 0.42)],
        "queen" => &[("nest", false, 0.22, 0.36), ("hatchery", false, 0.62, 0.48)],
        "burrow" => &[("outpost", true, 0.18, 0.34), ("home", true, 0.50, 0.46), ("lookout", true, 0.82, 0.38)],
        _ => &[("home", true, 0.22, 0.42), ("nest", false, 0.76, 0.40)],
    }
}

fn theme(look: &str) -> Option<[Color; 4]> {
    THEMES.iter().find(|(n, _)| *n == look).map(|(_, c)| c.map(hex))
}

fn sky(p: &mut dc_render::Canvas, look: &str, w: f64, h: f64, horizon: f64) {
    let [top, low, near, far] = theme(look).unwrap_or_else(|| theme("territory").unwrap());
    let sky = LinearGradient::new(0.0, 0.0, 0.0, horizon).with_stop(0.0, top).with_stop(1.0, low);
    p.fill_rect(Rect::new(0.0, 0.0, w, horizon + 1.0), sky);
    let ground = LinearGradient::new(0.0, horizon, 0.0, h).with_stop(0.0, far).with_stop(1.0, near);
    p.fill_rect(Rect::new(0.0, horizon, w, h - horizon), ground);
}

/// A frozen desktop: windows, a melted hole and cracked glass.
fn desktop(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom) {
    let back = LinearGradient::new(0.0, 0.0, w, h).with_stop(0.0, hex("#1d4f86")).with_stop(1.0, hex("#3a7cc0"));
    p.fill_rect(Rect::new(0.0, 0.0, w, h), back);
    for (x, y, ww, hh) in [(10.0, 8.0, 92.0, 56.0), (70.0, 22.0, 88.0, 50.0)] {
        p.fill_rect(Rect::new(x, y, ww, hh), hex("#eef1f5"));
        p.fill_rect(Rect::new(x, y, ww, 8.0), hex("#3b3f46"));
        p.set_pen(Pen::new(Color::rgba(90, 96, 110, 160), 1.2));
        for row in 0..3 {
            let length = ww - 18.0 - rng.uniform(0.0, 30.0);
            let ly = y + 16.0 + row as f64 * 9.0;
            p.draw_line(x + 6.0, ly, x + 6.0 + length, ly);
        }
    }
    // Acid hole through the window into the void.
    let mut hole = Path::new();
    let (cx, cy) = (118.0, 52.0);
    for i in 0..18 {
        let a = i as f64 / 18.0 * TAU;
        let r = 13.0 + rng.uniform(-3.0, 3.0);
        let (px, py) = (cx + a.cos() * r * 1.3, cy + a.sin() * r);
        if i == 0 {
            hole.move_to(px, py);
        } else {
            hole.line_to(px, py);
        }
    }
    hole.close_subpath();
    let glow = RadialGradient::new(cx, cy, 22.0)
        .with_stop(0.0, Color::rgb(20, 6, 30))
        .with_stop(0.75, Color::rgb(40, 16, 50))
        .with_stop(1.0, Color::rgb(150, 230, 60));
    p.set_pen(Pen::new(Color::rgb(160, 240, 70), 1.6));
    p.set_brush(glow);
    p.draw_path(&hole);
    // Cracks from a heavy landing.
    p.set_pen(Pen::new(Color::rgba(235, 245, 255, 190), 0.9));
    let (ox, oy) = (40.0, 70.0);
    for i in 0..7 {
        let mut a = i as f64 / 7.0 * TAU + rng.uniform(-0.2, 0.2);
        let (mut x, mut y) = (ox, oy);
        for _ in 0..3 {
            let nx = x + a.cos() * rng.uniform(8.0, 16.0);
            let ny = y + a.sin() * rng.uniform(6.0, 12.0);
            p.draw_line(x, y, nx, ny);
            x = nx;
            y = ny;
            a += rng.uniform(-0.5, 0.5);
        }
    }
}

fn flies(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom, count: usize, dark: bool) {
    for _ in 0..count {
        let x = rng.uniform(6.0, w - 6.0);
        let y = rng.uniform(6.0, h * 0.62);
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(255, 255, 255, 150));
        p.draw_ellipse_at(x - 2.0, y - 1.5, 2.2, 1.3);
        p.draw_ellipse_at(x + 2.0, y - 1.5, 2.2, 1.3);
        p.set_brush(if dark { Color::rgb(12, 12, 10) } else { Color::rgb(30, 30, 24) });
        p.draw_ellipse_at(x, y, 1.8, 1.4);
    }
}

fn embers(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom) {
    for _ in 0..26 {
        let x = rng.uniform(0.0, w);
        let y = rng.uniform(0.0, h * 0.8);
        let r = rng.uniform(0.6, 1.6);
        p.set_pen(Pen::NONE);
        let g = rng.randint(120, 200) as u8;
        let a = rng.randint(140, 230) as u8;
        p.set_brush(Color::rgba(255, g, 60, a));
        p.draw_ellipse_at(x, y, r, r);
    }
}

fn rain(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom) {
    p.set_pen(Pen::new(Color::rgba(200, 214, 230, 90), 0.8));
    for _ in 0..46 {
        let x = rng.uniform(-10.0, w);
        let y = rng.uniform(-10.0, h);
        p.draw_line(x, y, x - 4.0, y + 11.0);
    }
}

fn thorns(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom) {
    p.set_pen(Pen::new(Color::rgb(40, 14, 12), 1.6));
    let mut x = 0i64;
    while x < w as i64 + 8 {
        let xf = x as f64;
        let base = h - rng.uniform(0.0, 3.0);
        let top = base - rng.uniform(8.0, 16.0);
        let lean = rng.uniform(-3.0, 3.0);
        p.draw_line(xf, base, xf + lean, top);
        let mid = (base + top) / 2.0;
        p.draw_line(xf + lean * 0.5, mid, xf + lean * 0.5 + 3.0, mid - 3.0);
        x += 9;
    }
}

fn shards(p: &mut dc_render::Canvas, w: f64, h: f64, rng: &mut PyRandom) {
    for _ in 0..6 {
        let x = rng.uniform(0.0, w);
        let y = h - rng.uniform(4.0, 18.0);
        let s = rng.uniform(5.0, 11.0);
        let mut path = Path::new();
        path.move_to(x - s * 0.4, y);
        path.line_to(x, y - s * 1.6);
        path.line_to(x + s * 0.4, y);
        path.close_subpath();
        p.set_pen(Pen::new(Color::rgba(150, 120, 220, 170), 0.7));
        p.set_brush(Color::rgba(40, 26, 64, 230));
        p.draw_path(&path);
    }
}

fn paint_scene(look: &str, w: f64, h: f64, rng: &mut PyRandom) -> Image {
    let mut img = Image::new((w * SCALE as f64) as u32, (h * SCALE as f64) as u32);
    img.paint(|p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_render_hint(RenderHint::SmoothPixmapTransform, true);
        p.scale(SCALE as f64, SCALE as f64);
        let horizon = h * 0.42;
        if look == "reclaim" {
            desktop(p, w, h, rng);
        } else {
            sky(p, look, w, h, horizon);
        }
        if look == "ember" {
            embers(p, w, h, rng);
        }
        if look == "storm" {
            rain(p, w, h, rng);
        }
        for &(kind, owned, fx, scale) in scene(look) {
            let art = building_art(kind, owned);
            let (bw, bh) = (ART_W as f64 * scale, ART_H as f64 * scale);
            // The art's ground point (120, 139) sits a little above the banner's foot.
            let x = fx * w - 120.0 * scale;
            let y = h - 8.0 - 139.0 * scale;
            p.draw_image_rect(Rect::new(x, y, bw, bh), &art);
        }
        if look == "swarm" || look == "storm" {
            flies(p, w, h, rng, if look == "swarm" { 14 } else { 24 }, look == "storm");
        }
        if look == "queen" {
            thorns(p, w, h, rng);
        }
        if look == "obsidian" {
            shards(p, w, h, rng);
        }
        // A soft vignette so the picture sits in the card.
        let edge = RadialGradient::new(w / 2.0, h / 2.0, w * 0.62)
            .with_stop(0.6, Color::rgba(0, 0, 0, 0))
            .with_stop(1.0, Color::rgba(0, 0, 0, 120));
        p.fill_rect(Rect::new(0.0, 0.0, w, h), edge);
    });
    img
}

/// The look a map's banner uses.
pub fn picture_look(map_id: &str, kind: &str) -> &'static str {
    if let Some((n, _)) = THEMES.iter().find(|(n, _)| *n == map_id) {
        return n;
    }
    crate::mission_backdrop::kind_look(kind).unwrap_or("territory")
}

type PictureKey = (String, String, bool, u32, u32);

fn cache() -> &'static Mutex<HashMap<PictureKey, Image>> {
    static C: OnceLock<Mutex<HashMap<PictureKey, Image>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// `map_picture(map_id, kind, locked)` at the card size (216 x 92 logical).
pub fn map_picture(map_id: &str, kind: &str, locked: bool) -> Image {
    map_picture_sized(map_id, kind, locked, PICTURE_W, PICTURE_H)
}

/// The card banner for a map; a map from the editor looks like its kind. A
/// locked map's picture is dimmed and drained of colour. Device pixel ratio
/// 2; cached (same arguments return the same image).
pub fn map_picture_sized(map_id: &str, kind: &str, locked: bool, width: u32, height: u32) -> Image {
    let key = (map_id.to_string(), kind.to_string(), locked, width, height);
    if let Some(img) = cache().lock().unwrap().get(&key) {
        return img.clone();
    }
    let look = picture_look(map_id, kind);
    let mut rng = PyRandom::from_str_seed(&format!("card-{map_id}"));
    let mut img = paint_scene(look, width as f64, height as f64, &mut rng);
    if locked {
        let grey = Image::from_gray8(img.width(), img.height(), &img.to_gray8()).expect("size");
        img.paint(|p| {
            p.set_opacity(0.7);
            p.draw_image_at(0.0, 0.0, &grey);
            p.set_opacity(1.0);
            p.set_composition_mode(CompositionMode::SourceOver);
            p.fill_rect(Rect::new(0.0, 0.0, grey.width() as f64, grey.height() as f64), Color::rgba(20, 10, 4, 90));
        });
    }
    img.set_device_pixel_ratio(SCALE as f32);
    cache().lock().unwrap().entry(key).or_insert(img).clone()
}

/// Every look the banners know (for previews and tests).
pub fn picture_looks() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_map_card_has_its_own_picture() {
        let maps = [
            ("territory", "raid"),
            ("swarm", "swarm"),
            ("ember", "raid"),
            ("reclaim", "reclaim"),
            ("storm", "raid"),
            ("obsidian", "raid"),
            ("queen", "raid"),
            ("burrow", "raid"),
        ];
        let mut seen = std::collections::HashSet::new();
        for (id, kind) in maps {
            let picture = map_picture(id, kind, false);
            assert_eq!((picture.width(), picture.height()), (PICTURE_W * 2, PICTURE_H * 2));
            let mut key = Vec::new();
            for x in [40, 200, 380] {
                for y in [20, 100, 170] {
                    key.push(picture.pixel_premultiplied(x, y));
                }
            }
            seen.insert(key);
        }
        assert_eq!(seen.len(), maps.len(), "each map looks different");
        assert!(!map_picture("custom-x", "swarm", false).is_null());
        let bright = crate::qt::lightness(map_picture("territory", "raid", false).pixel(200, 30));
        let dim = crate::qt::lightness(map_picture("territory", "raid", true).pixel(200, 30));
        assert!(dim < bright);
        let a = map_picture("queen", "raid", false);
        assert_eq!(map_picture("queen", "raid", false).id(), a.id());
    }
}
