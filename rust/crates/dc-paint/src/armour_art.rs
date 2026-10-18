//! Port of `creature/armour_art.py`: armour pieces painted on the spider,
//! on the anatomy doll and on inventory icons.
//!
//! Body pieces are painted in the body's own frame (x forward, y across);
//! limb pieces take the joints of the limb they sit on.

use crate::color::{hex, qt_darker};
use dc_core::{Color, Rect};
use dc_render::{
    Brush, Canvas, Cap, Image, Join, LinearGradient, Path, Pen, PenStyle, RadialGradient, RenderHint,
};
use std::f64::consts::{PI, TAU};

/// `Look`: how a piece is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// `plate`, `silk` or `fluff`.
    pub style: &'static str,
    pub base: &'static str,
    pub light: &'static str,
    pub dark: &'static str,
    pub trim: Option<&'static str>,
    pub veins: Option<&'static str>,
    pub gem: Option<&'static str>,
    pub glow: Option<&'static str>,
    pub alpha: u8,
    pub facets: bool,
}

const fn look(style: &'static str, base: &'static str, light: &'static str, dark: &'static str) -> Look {
    Look { style, base, light, dark, trim: None, veins: None, gem: None, glow: None, alpha: 255, facets: false }
}

const WARDEN: Look = look("plate", "#a8742f", "#f4d58e", "#3a220c");
const FORAGER: Look = Look { trim: Some("#d8c48c"), ..look("plate", "#3d4a24", "#a9c070", "#161c0b") };
const FROST: Look = Look {
    trim: Some("#e8f8ff"),
    glow: Some("#b8ecff"),
    alpha: 185,
    facets: true,
    ..look("plate", "#86cdee", "#f4fcff", "#2b6688")
};
const BROOD: Look = Look {
    trim: Some("#5b3f86"),
    veins: Some("#c08aff"),
    gem: Some("#d7a6ff"),
    glow: Some("#9a5cf0"),
    ..look("plate", "#231c2e", "#8f70c0", "#060508")
};
const SUN: Look = Look {
    trim: Some("#fff0a0"),
    veins: Some("#ff7418"),
    gem: Some("#ff3a1c"),
    glow: Some("#ffc64d"),
    ..look("plate", "#dba632", "#fff3b4", "#6a3d0a")
};

/// `DEFAULT_LOOK`.
pub const DEFAULT_LOOK: Look = look("silk", "#e7dfcb", "#fffbf1", "#8d846f");

/// `look_for(item_id)`.
pub fn look_for(item_id: &str) -> Look {
    match item_id {
        "silk_carapace" => return look("silk", "#e7dfcb", "#fffbf1", "#8d846f"),
        "fluffy_mantle" => return look("fluff", "#d8cdbd", "#fff7ec", "#877c6c"),
        "leg_guard_set" => return look("silk", "#e2d9c3", "#fffaf0", "#877e69"),
        "pedipalp_cuffs" => return look("silk", "#d9c8a4", "#fff4dc", "#85745a"),
        "chelicerae_cap" => return look("plate", "#6d7378", "#d9dee2", "#24272a"),
        _ => {}
    }
    let prefix = item_id.split('_').next().unwrap_or("");
    match prefix {
        "warden" => WARDEN,
        "forager" => FORAGER,
        "frost" => FROST,
        "brood" => BROOD,
        "sun" => SUN,
        _ => DEFAULT_LOOK,
    }
}

fn c(value: &str, alpha: u8) -> Color {
    hex(value, alpha)
}

fn pen(color: Color, width: f64) -> Pen {
    Pen::full(color, width.max(0.4), PenStyle::Solid, Cap::Round, Join::Round)
}

fn pen_cap(color: Color, width: f64, cap: Cap) -> Pen {
    Pen::full(color, width.max(0.4), PenStyle::Solid, cap, Join::Round)
}

fn fill(l: &Look, value: &str) -> Color {
    c(value, l.alpha)
}

fn edge(l: &Look) -> Color {
    c(l.trim.unwrap_or(l.dark), 255)
}

fn aura(p: &mut Canvas, cx: f64, cy: f64, rx: f64, ry: f64, l: &Look) {
    let Some(glow) = l.glow else { return };
    let halo = RadialGradient::new(0.0, 0.0, rx * 1.25).with_stop(0.55, c(glow, 120)).with_stop(1.0, c(glow, 0));
    p.save();
    p.set_pen(Pen::NONE);
    p.set_brush(halo);
    p.translate(cx, cy);
    p.scale(1.0, ry / rx.max(1e-3));
    p.draw_ellipse_at(0.0, 0.0, rx * 1.25, rx * 1.25);
    p.restore();
}

fn vein(p: &mut Canvas, a: (f64, f64), b: (f64, f64), l: &Look, width: f64) {
    let Some(v) = l.veins else { return };
    p.set_pen(pen(c(v, 90), width * 2.6));
    p.draw_line_p(a, b);
    p.set_pen(pen(c(v, 245), width));
    p.draw_line_p(a, b);
}

fn gem(p: &mut Canvas, x: f64, y: f64, r: f64, l: &Look) {
    let Some(g) = l.gem else { return };
    let mut stone = Path::new();
    stone.move_to(x + r, y);
    stone.line_to(x, y - r * 0.8);
    stone.line_to(x - r, y);
    stone.line_to(x, y + r * 0.8);
    stone.close_subpath();
    let shine = RadialGradient::new(x + r * 0.3, y - r * 0.25, r * 1.2)
        .with_stop(0.0, Color::rgb(255, 255, 255))
        .with_stop(0.35, c(g, 255))
        .with_stop(1.0, qt_darker(c(g, 255), 170));
    p.set_pen(pen(edge(l), r * 0.25));
    p.set_brush(shine);
    p.draw_path(&stone);
}

fn silk_fill(p: &mut Canvas, path: &Path, l: &Look, scale: f64) {
    let bounds = path.bounding_rect();
    p.set_pen(Pen::NONE);
    p.set_brush(c(l.base, 185));
    p.draw_path(path);
    p.save();
    p.set_clip_path(path);
    let step = (scale * 0.11).max(1.6);
    p.set_pen(pen(c(l.light, 190), scale * 0.018));
    let (left, top, right, bottom, h) = (bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h, bounds.h);
    let mut x = left - h;
    while x < right {
        p.draw_line(x, top, x + h, bottom);
        p.draw_line(x + h, top, x, bottom);
        x += step;
    }
    p.restore();
    p.set_brush(Brush::None);
    p.set_pen(pen(c(l.dark, 200), scale * 0.028));
    p.draw_path(path);
}

fn fluff_fill(p: &mut Canvas, rect: Rect, l: &Look) {
    p.set_pen(Pen::NONE);
    p.set_brush(c(l.base, 200));
    p.draw_ellipse(rect);
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (rx, ry) = (rect.w / 2.0, rect.h / 2.0);
    p.set_brush(c(l.light, 215));
    for i in 0..14 {
        let a = i as f64 * TAU / 14.0;
        p.draw_ellipse_at(cx + a.cos() * rx * 0.86, cy + a.sin() * ry * 0.86, rx * 0.2, ry * 0.2);
    }
    p.set_brush(c(l.dark, 90));
    for i in 0..7 {
        let a = i as f64 * TAU / 7.0 + 0.3;
        p.draw_ellipse_at(cx + a.cos() * rx * 0.42, cy + a.sin() * ry * 0.42, rx * 0.08, ry * 0.08);
    }
}

/// `paint_carapace(p, cx, w, h, look)`.
pub fn paint_carapace(p: &mut Canvas, cx: f64, w: f64, h: f64, l: &Look) {
    let (pw, ph) = (w * 0.80, h * 0.78);
    let rect = Rect::new(cx - pw / 2.0, -ph / 2.0, pw, ph);
    p.save();
    if l.style == "plate" {
        aura(p, cx, 0.0, pw / 2.0, ph / 2.0, l);
        let shade = RadialGradient::new(cx + pw * 0.14, -ph * 0.18, pw * 0.66)
            .with_stop(0.0, fill(l, l.light))
            .with_stop(0.45, fill(l, l.base))
            .with_stop(1.0, fill(l, l.dark));
        p.set_pen(pen(edge(l), w * 0.04));
        p.set_brush(shade);
        p.draw_ellipse(rect);
        let fx = cx - pw * 0.06;
        if l.facets {
            p.set_pen(pen(c(l.light, 200), w * 0.016));
            let ring: Vec<(f64, f64)> = (0..7)
                .map(|i| {
                    let a = i as f64 * PI / 3.0;
                    (fx + a.cos() * pw * 0.2, a.sin() * ph * 0.2)
                })
                .collect();
            for k in 0..6 {
                p.draw_line_p(ring[k], ring[k + 1]);
            }
            for (i, r) in ring.iter().enumerate().take(6) {
                let a = i as f64 * PI / 3.0;
                p.draw_line_p(*r, (cx + a.cos() * pw * 0.48, a.sin() * ph * 0.47));
            }
        } else {
            for i in 0..8 {
                let a = (i as f64 + 0.5) * PI / 4.0;
                let start = (fx + a.cos() * pw * 0.11, a.sin() * ph * 0.11);
                let end = (fx + a.cos() * pw * 0.39, a.sin() * ph * 0.39);
                if l.veins.is_some() {
                    vein(p, start, end, l, w * 0.016);
                } else {
                    p.set_pen(pen(c(l.dark, 150), w * 0.018));
                    p.draw_line_p(start, end);
                }
            }
        }
        p.set_pen(pen(c(l.light, 170), w * 0.014));
        p.set_brush(c(l.dark, 255));
        p.draw_ellipse_at(fx, 0.0, pw * 0.065, ph * 0.05);
        p.set_pen(Pen::NONE);
        p.set_brush(c(l.trim.unwrap_or(l.light), 230));
        for a in [0.62f64, 1.57, 2.52, -0.62, -1.57, -2.52] {
            p.draw_ellipse_at(cx + a.cos() * pw * 0.43, a.sin() * ph * 0.42, w * 0.024, w * 0.024);
        }
        gem(p, cx + pw * 0.3, 0.0, w * 0.075, l);
        p.set_brush(Brush::None);
        p.set_pen(pen(c(l.light, 170), w * 0.03));
        p.draw_arc(rect.adjusted(pw * 0.07, ph * 0.07, -pw * 0.07, -ph * 0.07), 20 * 16, 80 * 16);
    } else if l.style == "fluff" {
        fluff_fill(p, rect, l);
    } else {
        let mut path = Path::new();
        path.add_ellipse(rect);
        silk_fill(p, &path, l, w);
    }
    p.restore();
}

/// `paint_abdomen(p, ax, w, h, wag, look)`.
pub fn paint_abdomen(p: &mut Canvas, ax: f64, w: f64, h: f64, wag: f64, l: &Look) {
    let shell = Rect::new(ax - w * 0.47, -h * 0.47 + wag, w * 0.94, h * 0.94);
    let mut outline = Path::new();
    outline.add_ellipse(shell);
    p.save();
    if l.style == "plate" {
        aura(p, ax + w * 0.08, wag, w * 0.42, h * 0.45, l);
        p.set_clip_path(&outline);
        let (front, back) = (ax + w * 0.46, ax - w * 0.27);
        let count = 4;
        let step = (front - back) / count as f64;
        let band_w = step * 1.32;
        for i in 0..count {
            let x0 = back + i as f64 * step;
            let band = Rect::new(x0, -h * 0.6 + wag, band_w, h * 1.2);
            let shade = LinearGradient::new(band.x, 0.0, band.x + band.w, 0.0)
                .with_stop(0.0, fill(l, l.dark))
                .with_stop(0.35, fill(l, l.base))
                .with_stop(0.78, fill(l, l.light))
                .with_stop(1.0, fill(l, l.base));
            p.set_pen(pen(edge(l), w * 0.024));
            p.set_brush(shade);
            p.draw_rounded_rect(band, band_w * 0.3, band_w * 0.3);
            if l.facets {
                p.set_pen(pen(c(l.light, 190), w * 0.012));
                p.draw_line(x0 + band_w * 0.2, -h * 0.5 + wag, x0 + band_w * 0.7, wag);
                p.draw_line(x0 + band_w * 0.7, wag, x0 + band_w * 0.2, h * 0.5 + wag);
            }
            if l.veins.is_some() {
                vein(p, (x0 + band_w * 0.55, -h * 0.4 + wag), (x0 + band_w * 0.55, h * 0.4 + wag), l, w * 0.012);
            }
        }
        p.set_clipping(false);
        if l.veins.is_some() {
            vein(p, (back + step * 0.4, wag), (front - step * 0.2, wag), l, w * 0.016);
        } else {
            p.set_pen(pen(c(l.light, 150), w * 0.02));
            p.draw_line(back + step * 0.4, wag, front - step * 0.2, wag);
        }
        p.set_pen(Pen::NONE);
        p.set_brush(c(l.trim.unwrap_or(l.light), 220));
        for i in 0..count {
            let x = back + (i as f64 + 0.62) * step;
            for side in [-1.0, 1.0] {
                let t = (1.0 - ((x - ax) / (w * 0.47)).powi(2)).max(0.0);
                p.draw_ellipse_at(x, wag + side * h * 0.47 * t.sqrt() * 0.72, w * 0.02, w * 0.02);
            }
        }
    } else if l.style == "fluff" {
        fluff_fill(p, shell.adjusted(w * 0.05, h * 0.05, -w * 0.05, -h * 0.05), l);
    } else {
        silk_fill(p, &outline, l, w);
    }
    p.restore();
}

/// `paint_head(p, hx, w, h, look)`.
pub fn paint_head(p: &mut Canvas, hx: f64, w: f64, h: f64, l: &Look) {
    p.save();
    let mut crest = Path::new();
    crest.move_to(hx - w * 0.95, 0.0);
    crest.quad_to(hx - w * 0.35, -h * 0.5, hx + w * 0.12, -h * 0.4);
    crest.line_to(hx + w * 0.46, -h * 0.14);
    crest.line_to(hx + w * 0.46, h * 0.14);
    crest.line_to(hx + w * 0.12, h * 0.4);
    crest.quad_to(hx - w * 0.35, h * 0.5, hx - w * 0.95, 0.0);
    let (cap_color, cap_edge);
    if l.style == "plate" {
        aura(p, hx - w * 0.2, 0.0, w * 0.75, h * 0.5, l);
        if l.gem.is_some() || l.veins.is_some() {
            p.set_pen(pen(edge(l), w * 0.06));
            p.set_brush(fill(l, l.base));
            for side in [-1.0, 1.0] {
                for (k, x) in [hx - w * 0.6, hx - w * 0.15, hx + w * 0.25].into_iter().enumerate() {
                    let reach = h * if k == 1 { 0.72 } else { 0.6 };
                    let mut spike = Path::new();
                    spike.move_to(x - w * 0.14, side * h * 0.3);
                    spike.line_to(x, side * reach);
                    spike.line_to(x + w * 0.14, side * h * 0.3);
                    spike.close_subpath();
                    p.draw_path(&spike);
                }
            }
        }
        let shade = LinearGradient::new(hx, -h * 0.5, hx, h * 0.5)
            .with_stop(0.0, fill(l, l.light))
            .with_stop(0.5, fill(l, l.base))
            .with_stop(1.0, fill(l, l.dark));
        p.set_pen(pen(edge(l), w * 0.08));
        p.set_brush(shade);
        p.draw_path(&crest);
        if l.veins.is_some() {
            vein(p, (hx - w * 0.8, 0.0), (hx + w * 0.4, 0.0), l, w * 0.05);
        } else {
            p.set_pen(pen(c(l.light, 200), w * 0.06));
            p.draw_line(hx - w * 0.8, 0.0, hx + w * 0.4, 0.0);
        }
        gem(p, hx - w * 0.45, 0.0, w * 0.2, l);
        cap_color = fill(l, l.base);
        cap_edge = edge(l);
    } else {
        silk_fill(p, &crest, l, w * 2.2);
        cap_color = c(l.light, 230);
        cap_edge = c(l.dark, 255);
    }
    p.set_pen(pen(cap_edge, w * 0.06));
    p.set_brush(cap_color);
    for side in [-1.0, 1.0] {
        p.draw_ellipse_at(hx + w * 0.56, side * h * 0.2, w * 0.14, h * 0.11);
    }
    p.restore();
}

#[allow(clippy::too_many_arguments)]
fn limb_plate(
    p: &mut Canvas,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    width: f64,
    l: &Look,
    start: f64,
    end: f64,
    straps: usize,
) {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let length = dx.hypot(dy);
    if length < 1e-3 {
        return;
    }
    p.save();
    p.translate(x1, y1);
    p.rotate(dy.atan2(dx).to_degrees());
    let (a, b) = (length * start, length * end);
    let hw = (width * 0.68).max(0.8);
    if l.style == "plate" {
        if let Some(glow) = l.glow {
            p.set_pen(pen(c(glow, 70), hw * 3.2));
            p.draw_line(a, 0.0, b, 0.0);
        }
        let mut plate = Path::new();
        plate.move_to(a, -hw * 0.7);
        plate.line_to(a + length * 0.1, -hw);
        plate.line_to(b - length * 0.08, -hw * 0.9);
        plate.line_to(b, -hw * 0.55);
        plate.line_to(b, hw * 0.55);
        plate.line_to(b - length * 0.08, hw * 0.9);
        plate.line_to(a + length * 0.1, hw);
        plate.line_to(a, hw * 0.7);
        plate.close_subpath();
        let shade = LinearGradient::new(0.0, -hw, 0.0, hw)
            .with_stop(0.0, fill(l, l.light))
            .with_stop(0.45, fill(l, l.base))
            .with_stop(1.0, fill(l, l.dark));
        p.set_pen(pen(edge(l), width * 0.14));
        p.set_brush(shade);
        p.draw_path(&plate);
        if l.facets {
            p.set_pen(pen(c(l.light, 210), width * 0.1));
            let mut zig = Path::new();
            zig.move_to(a + length * 0.08, -hw * 0.5);
            for k in 1..5 {
                zig.line_to(a + (b - a) * k as f64 / 4.0, hw * if k % 2 == 1 { 0.5 } else { -0.5 });
            }
            p.set_brush(Brush::None);
            p.draw_path(&zig);
        } else if l.veins.is_some() {
            vein(p, (a + length * 0.1, 0.0), (b - length * 0.08, 0.0), l, width * 0.14);
        } else {
            p.set_pen(pen(c(l.light, 190), width * 0.12));
            p.draw_line(a + length * 0.12, -hw * 0.25, b - length * 0.1, -hw * 0.2);
        }
        p.set_pen(pen_cap(c(l.trim.unwrap_or(l.dark), 210), width * 0.12, Cap::Flat));
        for i in 0..straps {
            let x = a + (b - a) * (i + 1) as f64 / (straps + 1) as f64;
            p.draw_line(x, -hw * 0.95, x, hw * 0.95);
        }
    } else {
        p.set_pen(pen_cap(c(l.light, 215), width * 0.3, Cap::Flat));
        let wraps = straps + 2;
        for i in 0..wraps {
            let x = a + (b - a) * (i as f64 + 0.5) / wraps as f64;
            p.draw_line(x - length * 0.02, -hw * 1.05, x + length * 0.02, hw * 1.05);
        }
    }
    p.restore();
}

fn knee_cop(p: &mut Canvas, x: f64, y: f64, width: f64, l: &Look) {
    let r = (width * 0.72).max(0.9);
    if l.style != "plate" {
        p.set_pen(pen(c(l.light, 215), width * 0.3));
        p.set_brush(Brush::None);
        p.draw_ellipse_at(x, y, r * 0.8, r * 0.8);
        return;
    }
    if let Some(glow) = l.glow {
        p.set_pen(Pen::NONE);
        p.set_brush(c(glow, 80));
        p.draw_ellipse_at(x, y, r * 1.7, r * 1.7);
    }
    let shine = RadialGradient::new(x - r * 0.3, y - r * 0.3, r * 1.3)
        .with_stop(0.0, fill(l, l.light))
        .with_stop(0.5, fill(l, l.base))
        .with_stop(1.0, fill(l, l.dark));
    p.set_pen(pen(edge(l), width * 0.14));
    p.set_brush(shine);
    p.draw_ellipse_at(x, y, r, r);
    if l.gem.is_some() {
        gem(p, x, y, r * 0.5, l);
    }
}

/// `paint_leg_armour(p, points, widths, look)`.
pub fn paint_leg_armour(p: &mut Canvas, points: &[(f64, f64)], widths: &[f64], l: &Look) {
    if points.len() < 3 || widths.is_empty() {
        return;
    }
    let segments = points.len() - 1;
    for i in 0..segments - 1 {
        let ((x1, y1), (x2, y2)) = (points[i], points[i + 1]);
        let width = widths[i.min(widths.len() - 1)];
        let (start, end) = if i == 0 { (0.14, 0.9) } else { (0.04, 0.96) };
        limb_plate(p, x1, y1, x2, y2, width, l, start, end, if i == 0 { 2 } else { 1 });
    }
    let (kx, ky) = points[1];
    knee_cop(p, kx, ky, widths[1.min(widths.len() - 1)], l);
}

/// `paint_bracer(p, x1, y1, x2, y2, width, look)`.
#[allow(clippy::too_many_arguments)]
pub fn paint_bracer(p: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, l: &Look) {
    limb_plate(p, x1, y1, x2, y2, width, l, 0.18, 0.82, 1);
    if l.gem.is_some() {
        gem(p, (x1 + x2) / 2.0, (y1 + y2) / 2.0, (width * 0.4).max(0.8), l);
    }
}

const BODY: Color = Color::rgb(34, 28, 26);
const BAND: Color = Color::rgb(226, 110, 40);
const RIM: Color = Color::rgb(196, 120, 64);

fn limb(p: &mut Canvas, points: &[(f64, f64)], widths: &[f64], band_index: usize) {
    for i in 0..points.len() - 1 {
        let color = if i == band_index { BAND } else { BODY };
        p.set_pen(pen(color, widths[i]));
        p.draw_line_p(points[i], points[i + 1]);
    }
}

/// `armour_icon(item_id, slot, size)`: an inventory picture. Not cached
/// here; callers keep the image.
pub fn armour_icon(item_id: &str, slot: &str, size: u32) -> Image {
    let mut image = Image::new(size, size);
    let l = look_for(item_id);
    let s = size as f64;
    image.paint(|p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.translate(s / 2.0, s / 2.0);
        p.rotate(-90.0);
        match slot {
            "carapace" => {
                let (w, h) = (s * 0.72, s * 0.62);
                p.set_pen(Pen::NONE);
                p.set_brush(RIM);
                p.draw_ellipse(Rect::new(-w / 2.0, -h / 2.0, w, h));
                p.set_brush(BODY);
                p.draw_ellipse(Rect::new(-w * 0.4, -h * 0.4, w * 0.8, h * 0.8));
                paint_carapace(p, 0.0, w, h, &l);
            }
            "abdomen" => {
                let (w, h) = (s * 0.78, s * 0.62);
                p.set_pen(Pen::NONE);
                p.set_brush(BODY);
                p.draw_ellipse(Rect::new(-w / 2.0, -h / 2.0, w, h));
                paint_abdomen(p, 0.0, w, h, 0.0, &l);
            }
            "head" => {
                let (w, h) = (s * 0.3, s * 0.4);
                p.set_pen(Pen::NONE);
                p.set_brush(BODY);
                p.draw_ellipse(Rect::new(-s * 0.36, -s * 0.28, s * 0.56, s * 0.56));
                p.draw_ellipse(Rect::new(-w / 2.0 + s * 0.08, -h / 2.0, w, h));
                paint_head(p, s * 0.08, w, h, &l);
                p.set_brush(Color::rgb(90, 80, 70));
                for (ex, ey) in [(0.0, -0.05), (0.0, 0.05), (0.04, -0.1), (0.04, 0.1)] {
                    p.draw_ellipse_at(s * (0.02 + ex), s * ey, s * 0.022, s * 0.022);
                }
            }
            "legs" => {
                let pts = [
                    (-s * 0.4, s * 0.3),
                    (-s * 0.06, s * 0.22),
                    (s * 0.06, s * 0.1),
                    (s * 0.2, -s * 0.1),
                    (s * 0.33, -s * 0.26),
                    (s * 0.43, -s * 0.36),
                ];
                let widths = [s * 0.11, s * 0.105, s * 0.09, s * 0.075, s * 0.06];
                limb(p, &pts, &widths, 1);
                paint_leg_armour(p, &pts, &widths, &l);
            }
            _ => {
                let width = s * 0.1;
                for side in [-1.0, 1.0] {
                    let pts = [
                        (-s * 0.34, side * s * 0.1),
                        (-s * 0.1, side * s * 0.2),
                        (s * 0.14, side * s * 0.18),
                        (s * 0.36, side * s * 0.08),
                    ];
                    limb(p, &pts, &[width; 3], 0);
                    paint_bracer(p, pts[1].0, pts[1].1, pts[2].0, pts[2].1, width, &l);
                }
            }
        }
    });
    image
}
