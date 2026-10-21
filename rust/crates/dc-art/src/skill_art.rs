//! Port of app/skill_art.py: painted emblems for the skill tree.
//!
//! Every skill gets a round brass medallion with its own picture -- a heart
//! for Vitality, a shell for Hardened carapace, fangs for Power strike, a web
//! for Web crafter -- and every branch of the tree a crest over its column.
//! Locked skills show the same picture dark and drained; learned ones glow.
//! Painted once at twice the size and cached. (`dc-ui` draws the same
//! pictures straight into egui; these are the `Image` versions for the
//! overlay and anything else that wants a picture.)

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Mutex, OnceLock};

use dc_core::{Color, Rect};
use dc_render::{
    Brush, Canvas, Cap, CompositionMode, Image, Join, LinearGradient, Path, Pen, PenStyle, RadialGradient, RenderHint,
};

use crate::common::round_pen as pen;
use crate::qt::{darker, div255, hex, lighter};

pub const SCALE: u32 = 2;
/// Which branch each column of the tree is (character_ui.TREE_LAYOUT).
pub const BRANCHES: [(&str, &str); 4] = [("body", "Body"), ("fangs", "Fangs"), ("legs", "Legs"), ("silk", "Silk")];

pub fn skill_branch(ability_id: &str) -> Option<&'static str> {
    Some(match ability_id {
        "vitality" | "carapace_harden" => "body",
        "power_strike" | "apex_predator" => "fangs",
        "quick_step" | "long_stride" => "legs",
        "silk_sense" | "web_crafter" | "silk_tracking" => "silk",
        _ => return None,
    })
}

pub fn branch_colour(branch: &str) -> Option<&'static str> {
    Some(match branch {
        "body" => "#c0503c",
        "fangs" => "#d89a2c",
        "legs" => "#5e9c4c",
        "silk" => "#6aa6c8",
        _ => return None,
    })
}

const INK: Color = Color::rgb(40, 24, 12);
const CREAM: Color = Color::rgb(250, 238, 210);

/// A medallion's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkillLook {
    Learned,
    Available,
    Locked,
}

impl SkillLook {
    pub fn from_name(name: &str) -> SkillLook {
        match name {
            "learned" => SkillLook::Learned,
            "locked" => SkillLook::Locked,
            _ => SkillLook::Available,
        }
    }
}

/// The pictures, drawn in a 64 x 64 box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Heart,
    Shell,
    Fangs,
    Crown,
    Legs,
    Stride,
    Eye,
    Web,
    Thread,
}

fn heart(p: &mut Canvas) {
    let mut path = Path::new();
    path.move_to(32.0, 48.0);
    path.cubic_to(12.0, 36.0, 14.0, 16.0, 26.0, 18.0);
    path.cubic_to(30.0, 18.0, 32.0, 22.0, 32.0, 24.0);
    path.cubic_to(32.0, 22.0, 34.0, 18.0, 38.0, 18.0);
    path.cubic_to(50.0, 16.0, 52.0, 36.0, 32.0, 48.0);
    p.set_brush(hex("#d8453a"));
    p.set_pen(pen(INK, 2.2));
    p.draw_path(&path);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(255, 220, 210, 170));
    p.draw_ellipse_at(25.0, 25.0, 3.5, 2.5);
}

fn shell(p: &mut Canvas) {
    let mut path = Path::new();
    path.move_to(32.0, 14.0);
    path.cubic_to(46.0, 16.0, 50.0, 30.0, 46.0, 40.0);
    path.cubic_to(42.0, 48.0, 36.0, 51.0, 32.0, 52.0);
    path.cubic_to(28.0, 51.0, 22.0, 48.0, 18.0, 40.0);
    path.cubic_to(14.0, 30.0, 18.0, 16.0, 32.0, 14.0);
    let grad =
        LinearGradient::new(20.0, 14.0, 44.0, 52.0).with_stop(0.0, hex("#a07a4c")).with_stop(1.0, hex("#4e3218"));
    p.set_brush(grad);
    p.set_pen(pen(INK, 2.2));
    p.draw_path(&path);
    p.set_pen(pen(hex("#e8c890"), 1.6));
    p.draw_line(32.0, 17.0, 32.0, 49.0);
    for (y, w) in [(26.0, 10.0), (34.0, 12.0), (42.0, 9.0)] {
        p.draw_line(32.0 - w, y - 3.0, 32.0, y);
        p.draw_line(32.0 + w, y - 3.0, 32.0, y);
    }
}

fn fangs(p: &mut Canvas) {
    for side in [-1.0, 1.0] {
        let mut path = Path::new();
        path.move_to(32.0 + side * 4.0, 16.0);
        path.cubic_to(32.0 + side * 16.0, 18.0, 32.0 + side * 16.0, 36.0, 32.0 + side * 6.0, 50.0);
        path.cubic_to(32.0 + side * 8.0, 38.0, 32.0 + side * 6.0, 28.0, 32.0 + side * 2.0, 24.0);
        path.close_subpath();
        let grad =
            LinearGradient::new(32.0, 16.0, 32.0, 50.0).with_stop(0.0, hex("#f4ead0")).with_stop(1.0, hex("#b89868"));
        p.set_brush(grad);
        p.set_pen(pen(INK, 2.0));
        p.draw_path(&path);
    }
    p.set_pen(Pen::NONE);
    p.set_brush(hex("#7ad04a"));
    p.draw_ellipse_at(26.0, 52.0, 2.2, 3.0);
    p.draw_ellipse_at(38.0, 52.0, 2.2, 3.0);
}

fn crown(p: &mut Canvas) {
    let mut path = Path::new();
    path.move_to(14.0, 44.0);
    for (x, y) in [(14.0, 22.0), (22.0, 32.0), (32.0, 16.0), (42.0, 32.0), (50.0, 22.0), (50.0, 44.0)] {
        path.line_to(x, y);
    }
    path.close_subpath();
    let grad =
        LinearGradient::new(14.0, 16.0, 50.0, 44.0).with_stop(0.0, hex("#ffe08a")).with_stop(1.0, hex("#b07818"));
    p.set_brush(grad);
    p.set_pen(pen(INK, 2.2));
    p.draw_path(&path);
    p.draw_rect(Rect::new(14.0, 44.0, 36.0, 6.0));
    p.set_pen(Pen::NONE);
    for (x, colour) in [(22.0, "#c83040"), (32.0, "#3a8ad8"), (42.0, "#c83040")] {
        p.set_brush(hex(colour));
        p.draw_ellipse_at(x, 39.0, 3.0, 3.0);
    }
}

fn legs(p: &mut Canvas, stride: bool) {
    p.set_pen(pen(INK, 3.2));
    let base = (30.0, 34.0);
    for (dx, dy) in [(-16.0, -10.0), (-18.0, 2.0), (-15.0, 13.0), (-8.0, 20.0)] {
        let knee = (base.0 + dx * 0.6, base.1 + dy - 8.0);
        p.draw_line_p(base, knee);
        p.draw_line_p(knee, (base.0 + dx, base.1 + dy));
    }
    p.set_brush(hex("#5a3a20"));
    p.draw_ellipse_at(34.0, 32.0, 8.0, 6.0);
    p.set_pen(pen(hex("#f0d8a0"), 2.0));
    for (x0, y0, x1, y1) in [(42.0, 22.0, 56.0, 22.0), (44.0, 30.0, 58.0, 30.0), (42.0, 38.0, 54.0, 38.0)] {
        p.draw_line(x0, y0, x1, y1);
    }
    if stride {
        p.set_pen(Pen::full(hex("#f0d8a0"), 1.8, PenStyle::Dash, Cap::Round, Join::Bevel));
        let mut arc = Path::new();
        arc.move_to(12.0, 52.0);
        arc.quad_to(32.0, 4.0, 54.0, 50.0);
        // The brush is still the body's brown, as in the Python.
        p.draw_path(&arc);
    }
}

fn eye(p: &mut Canvas) {
    let mut path = Path::new();
    path.move_to(10.0, 32.0);
    path.quad_to(32.0, 12.0, 54.0, 32.0);
    path.quad_to(32.0, 52.0, 10.0, 32.0);
    p.set_brush(CREAM);
    p.set_pen(pen(INK, 2.2));
    p.draw_path(&path);
    let glow = RadialGradient::new(32.0, 32.0, 10.0).with_stop(0.0, hex("#bff0ff")).with_stop(1.0, hex("#2a78a8"));
    p.set_brush(glow);
    p.draw_ellipse_at(32.0, 32.0, 9.0, 9.0);
    p.set_brush(INK);
    p.draw_ellipse_at(32.0, 32.0, 3.5, 3.5);
    p.set_pen(pen(hex("#e8f4ff"), 1.2));
    for a in (0..360).step_by(60) {
        let r = (a as f64).to_radians();
        p.draw_line(
            32.0 + r.cos() * 13.0,
            32.0 + r.sin() * 13.0 * 0.7,
            32.0 + r.cos() * 20.0,
            32.0 + r.sin() * 20.0 * 0.7,
        );
    }
}

fn web(p: &mut Canvas, thread: bool) {
    p.set_pen(pen(hex("#f4f0e4"), 1.6));
    let spokes = 8;
    for i in 0..spokes {
        let a = i as f64 / spokes as f64 * TAU;
        p.draw_line(32.0, 32.0, 32.0 + a.cos() * 22.0, 32.0 + a.sin() * 22.0);
    }
    p.set_pen(pen(hex("#e8e0cc"), 1.2));
    for r in [7.0, 13.0, 19.0] {
        let mut poly = Path::new();
        for i in 0..=spokes {
            let a = i as f64 / spokes as f64 * TAU;
            let (x, y) = (32.0 + a.cos() * r, 32.0 + a.sin() * r);
            if i == 0 {
                poly.move_to(x, y);
            } else {
                poly.line_to(x, y);
            }
        }
        p.draw_path(&poly);
    }
    if thread {
        p.set_pen(Pen::full(hex("#ffe890"), 2.2, PenStyle::Solid, Cap::Round, Join::Bevel));
        let mut path = Path::new();
        path.move_to(32.0, 32.0);
        path.cubic_to(44.0, 36.0, 40.0, 50.0, 54.0, 52.0);
        p.draw_path(&path);
        p.set_pen(Pen::NONE);
        p.set_brush(hex("#ffe890"));
        p.draw_ellipse_at(54.0, 52.0, 3.2, 3.2);
    }
    p.set_pen(Pen::NONE);
    p.set_brush(INK);
    p.draw_ellipse_at(32.0, 32.0, 4.0, 4.0);
}

/// Draw a picture in the 64 x 64 box.
pub fn draw_picture(p: &mut Canvas, picture: Picture) {
    match picture {
        Picture::Heart => heart(p),
        Picture::Shell => shell(p),
        Picture::Fangs => fangs(p),
        Picture::Crown => crown(p),
        Picture::Legs => legs(p, false),
        Picture::Stride => legs(p, true),
        Picture::Eye => eye(p),
        Picture::Web => web(p, false),
        Picture::Thread => web(p, true),
    }
}

/// `PICTURES`: a skill's own picture.
pub fn skill_picture(ability_id: &str) -> Option<Picture> {
    Some(match ability_id {
        "vitality" => Picture::Heart,
        "carapace_harden" => Picture::Shell,
        "power_strike" => Picture::Fangs,
        "apex_predator" => Picture::Crown,
        "quick_step" => Picture::Legs,
        "long_stride" => Picture::Stride,
        "silk_sense" => Picture::Eye,
        "web_crafter" => Picture::Web,
        "silk_tracking" => Picture::Thread,
        _ => return None,
    })
}

/// `BRANCH_PICTURES`.
pub fn branch_picture(branch: &str) -> Option<Picture> {
    Some(match branch {
        "body" => Picture::Heart,
        "fangs" => Picture::Fangs,
        "legs" => Picture::Legs,
        "silk" => Picture::Web,
        _ => return None,
    })
}

/// The round plate a picture sits on.
fn medallion(p: &mut Canvas, rim: Color, look: SkillLook) {
    let face = if look == SkillLook::Locked {
        RadialGradient::new(28.0, 24.0, 34.0)
            .with_stop(0.0, Color::rgb(70, 56, 44))
            .with_stop(1.0, Color::rgb(30, 22, 16))
    } else {
        RadialGradient::new(28.0, 24.0, 34.0).with_stop(0.0, lighter(rim, 115)).with_stop(1.0, darker(rim, 260))
    };
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(0, 0, 0, 90));
    p.draw_ellipse_at(33.0, 34.0, 29.0, 29.0);
    p.set_brush(face);
    p.draw_ellipse_at(32.0, 32.0, 29.0, 29.0);
    let ring = if look == SkillLook::Locked {
        LinearGradient::new(0.0, 0.0, 64.0, 64.0)
            .with_stop(0.0, Color::rgb(110, 96, 80))
            .with_stop(1.0, Color::rgb(60, 50, 40))
    } else {
        LinearGradient::new(0.0, 0.0, 64.0, 64.0).with_stop(0.0, hex("#fbe3a0")).with_stop(1.0, hex("#8a5a1c"))
    };
    p.set_brush(Brush::None);
    p.set_pen(Pen::with_brush(ring, 3.4));
    p.draw_ellipse_at(32.0, 32.0, 28.0, 28.0);
}

/// `_render(picture, rim, look, size)`: a medallion `size` logical pixels
/// across, painted at twice that (device pixel ratio 2).
pub fn render_medallion(picture: Picture, rim: Color, look: SkillLook, size: u32) -> Image {
    let mut image = Image::new(size * SCALE, size * SCALE);
    image.paint(|p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        let k = (size * SCALE) as f64 / 64.0;
        p.scale(k, k);
        if look == SkillLook::Learned {
            let glow = RadialGradient::new(32.0, 32.0, 32.0)
                .with_stop(0.7, Color::rgba(255, 220, 120, 150))
                .with_stop(1.0, Color::rgba(255, 220, 120, 0));
            p.set_pen(Pen::NONE);
            p.set_brush(glow);
            p.draw_ellipse_at(32.0, 32.0, 32.0, 32.0);
        }
        medallion(p, rim, look);
        p.save();
        p.translate(32.0, 32.0);
        p.scale(0.78, 0.78);
        p.translate(-32.0, -32.0);
        draw_picture(p, picture);
        p.restore();
    });
    if look == SkillLook::Locked {
        // Grey with the picture's own alpha (setAlphaChannel), then darkened.
        let grey = image.to_gray8();
        let mut data = image.data().to_vec();
        for (px, g) in data.as_chunks_mut::<4>().0.iter_mut().zip(grey) {
            let a = px[3] as u32;
            let v = div255(g as u32 * a) as u8;
            px[0] = v;
            px[1] = v;
            px[2] = v;
        }
        let mut drained = Image::from_premultiplied(image.width(), image.height(), data).expect("size");
        drained.paint(|p| {
            p.set_composition_mode(CompositionMode::SourceAtop);
            p.fill_rect(
                Rect::new(0.0, 0.0, (size * SCALE) as f64, (size * SCALE) as f64),
                Color::rgba(30, 20, 10, 120),
            );
        });
        image = drained;
    }
    image.set_device_pixel_ratio(SCALE as f32);
    image
}

type Key = (String, SkillLook, u32);

fn cache(which: u8) -> &'static Mutex<HashMap<Key, Image>> {
    static C: OnceLock<[Mutex<HashMap<Key, Image>>; 3]> = OnceLock::new();
    &C.get_or_init(Default::default)[which as usize]
}

fn cached(which: u8, key: Key, make: impl FnOnce() -> Image) -> Image {
    if let Some(img) = cache(which).lock().unwrap().get(&key) {
        return img.clone();
    }
    let img = make();
    cache(which).lock().unwrap().entry(key).or_insert(img).clone()
}

/// `skill_icon(ability_id, look, size=46)`: a skill's medallion.
pub fn skill_icon(ability_id: &str, look: SkillLook, size: u32) -> Image {
    cached(0, (ability_id.to_string(), look, size), || {
        let branch = skill_branch(ability_id).unwrap_or("body");
        let picture = skill_picture(ability_id).or_else(|| branch_picture(branch)).unwrap_or(Picture::Heart);
        render_medallion(picture, hex(branch_colour(branch).unwrap_or("#c0503c")), look, size)
    })
}

/// `branch_crest(branch, size=34)`: the crest over a branch's column.
pub fn branch_crest(branch: &str, size: u32) -> Image {
    cached(1, (branch.to_string(), SkillLook::Available, size), || {
        let picture = branch_picture(branch).unwrap_or(Picture::Heart);
        render_medallion(picture, hex(branch_colour(branch).unwrap_or("#c0503c")), SkillLook::Available, size)
    })
}

/// `STAT_PICTURES` and the stat tiles' colours.
pub fn stat_picture(kind: &str) -> (Picture, &'static str) {
    match kind {
        "armor" => (Picture::Shell, "#8a6a44"),
        "damage" => (Picture::Fangs, "#d89a2c"),
        "points" => (Picture::Crown, "#c9a044"),
        "worn" => (Picture::Shell, "#6a7a8c"),
        "speed" => (Picture::Legs, "#5e9c4c"),
        "silk" => (Picture::Web, "#6aa6c8"),
        _ => (Picture::Shell, "#8a6a44"),
    }
}

/// `stat_icon(kind, size=30)`: small pictures for the inspector's stat tiles.
pub fn stat_icon(kind: &str, size: u32) -> Image {
    cached(2, (kind.to_string(), SkillLook::Available, size), || {
        let (picture, colour) = stat_picture(kind);
        render_medallion(picture, hex(colour), SkillLook::Available, size)
    })
}

/// Every skill id with a picture of its own.
pub const SKILL_IDS: [&str; 9] = [
    "vitality",
    "carapace_harden",
    "power_strike",
    "apex_predator",
    "quick_step",
    "long_stride",
    "silk_sense",
    "web_crafter",
    "silk_tracking",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medallions_are_twice_their_size_and_cached() {
        let a = skill_icon("vitality", SkillLook::Available, 46);
        assert_eq!((a.width(), a.height(), a.device_pixel_ratio()), (92, 92, 2.0));
        assert_eq!(skill_icon("vitality", SkillLook::Available, 46).id(), a.id());
        assert_eq!(a.pixel(0, 0).a, 0, "transparent outside the plate");
        assert_eq!(a.pixel(46, 46).a, 255);
    }

    #[test]
    fn locked_is_grey_and_learned_glows() {
        let locked = skill_icon("power_strike", SkillLook::Locked, 46);
        let c = locked.pixel(46, 30);
        assert!(c.r.abs_diff(c.g) <= 6 && c.g.abs_diff(c.b) <= 6, "{c:?}");
        let learned = skill_icon("power_strike", SkillLook::Learned, 46);
        let available = skill_icon("power_strike", SkillLook::Available, 46);
        assert!(learned.pixel(46, 1).a > available.pixel(46, 1).a);
    }

    #[test]
    fn every_skill_and_branch_has_a_distinct_picture() {
        let mut seen = std::collections::HashSet::new();
        for id in SKILL_IDS {
            seen.insert(skill_icon(id, SkillLook::Available, 46).data().to_vec());
        }
        assert_eq!(seen.len(), SKILL_IDS.len());
        for (b, _) in BRANCHES {
            assert!(!branch_crest(b, 34).is_null());
        }
        for k in ["armor", "damage", "points", "worn", "speed", "silk"] {
            assert_eq!(stat_icon(k, 30).width(), 60);
        }
    }
}
