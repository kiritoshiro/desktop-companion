//! Port of the drawing half of `world/jobs.py`: `_plan_mounds`,
//! `_ragged_outline`, `patch_recipe`, `_blend`, `BaseSite.radius`,
//! `BaseSite.completion`, `BaseSite.mounds` and `BaseWorld.render`.
//!
//! The mound layout and patch shapes are seeded from string keys
//! (`random.Random(f"{site_id}:mounds")`), so they are reproduced here with
//! `PyRandom::from_str_seed` and cached the way the Python cached them.

use super::Clip;
use dc_core::math::py_round;
use dc_core::{Color, PyRandom, Rect};
use dc_render::{Brush, Canvas, Path, Pen, PenStyle, RenderHint};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Arc, Mutex, OnceLock};

pub const MAX_BUILD_PROGRESS: f64 = 120.0;
pub const MOUNDS_PER_BASE: usize = 12;
pub const PATCH_POINTS: usize = 15;
pub const PATCH_FLATTEN: f64 = 0.58;
pub const TEAM_TINT_SOIL: f64 = 0.05;
pub const TEAM_TINT_BODY: f64 = 0.08;
pub const TEAM_TINT_CROWN: f64 = 0.13;

/// `BaseSite`: what `BaseWorld.render` reads, plus the resolved
/// `team_color(site.team_id, team_profiles)`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct BaseDraw {
    pub id: String,
    pub team_id: String,
    pub x: f64,
    pub y: f64,
    pub build_progress: f64,
    pub level: i64,
    pub alert: f64,
    pub team_color: (u8, u8, u8),
}

impl Default for BaseDraw {
    fn default() -> Self {
        BaseDraw {
            id: String::new(),
            team_id: "neutral".into(),
            x: 0.0,
            y: 0.0,
            build_progress: 0.0,
            level: 0,
            alert: 0.0,
            team_color: (255, 255, 255),
        }
    }
}

impl BaseDraw {
    /// `radius`.
    pub fn radius(&self) -> f64 {
        30.0 + self.level as f64 * 10.0
    }

    /// `completion`.
    pub fn completion(&self) -> f64 {
        (self.build_progress / MAX_BUILD_PROGRESS).clamp(0.0, 1.0)
    }

    /// `mounds()`: `(x, y, size, aspect, built)` for every started mound.
    pub fn mounds(&self) -> Vec<(f64, f64, f64, f64, f64)> {
        let layout = plan_mounds_cached(&self.id, self.radius());
        let filled = self.completion() * MOUNDS_PER_BASE as f64;
        let mut placed = Vec::new();
        for (index, &(mx, my, size, aspect)) in layout.iter().enumerate() {
            let built = (filled - index as f64).clamp(0.0, 1.0);
            if built <= 0.0 {
                continue;
            }
            placed.push((self.x + mx, self.y + my, size, aspect, built));
        }
        placed
    }
}

/// `_plan_mounds(site_id, radius)`.
pub fn plan_mounds(site_id: &str, radius: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut rng = PyRandom::from_str_seed(&format!("{site_id}:mounds"));
    let mut placed = Vec::with_capacity(MOUNDS_PER_BASE);
    for index in 0..MOUNDS_PER_BASE {
        let ring = (index as f64 / (MOUNDS_PER_BASE - 1).max(1) as f64).powf(0.62);
        let distance = radius * 0.62 * ring;
        let angle = index as f64 * 2.39996 + rng.uniform(-0.28, 0.28);
        let jitter = radius * 0.05;
        let x = angle.cos() * distance + rng.uniform(-jitter, jitter);
        let y = angle.sin() * distance * 0.66 + rng.uniform(-jitter, jitter) * 0.66;
        let size = radius * rng.uniform(0.15, 0.21);
        let aspect = rng.uniform(0.85, 1.35);
        placed.push((x, y, size, aspect));
    }
    placed
}

type MoundKey = (String, u64);
/// `(dx, dy, size, aspect)` per mound.
type MoundLayout = Vec<(f64, f64, f64, f64)>;

fn plan_mounds_cached(site_id: &str, radius: f64) -> Arc<MoundLayout> {
    static CACHE: OnceLock<Mutex<HashMap<MoundKey, Arc<MoundLayout>>>> = OnceLock::new();
    let key = (site_id.to_string(), (py_round(radius * 1000.0) / 1000.0).to_bits());
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache.entry(key).or_insert_with(|| Arc::new(plan_mounds(site_id, radius))).clone()
}

/// `_ragged_outline(rng, points, squash, wobble)`.
fn ragged_outline(rng: &mut PyRandom, points: usize, squash: f64, wobble: f64) -> Vec<(f64, f64)> {
    (0..points)
        .map(|index| {
            let angle = (index as f64 / points as f64) * TAU;
            let radius = 1.0 + rng.uniform(-wobble, wobble * 0.78);
            (angle.cos() * radius, angle.sin() * radius * squash)
        })
        .collect()
}

/// `patch_recipe(key)`: the fixed shape of one dirt patch.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchRecipe {
    pub body: Vec<(f64, f64)>,
    pub crown: Vec<(f64, f64)>,
    pub crown_offset: (f64, f64),
    /// `(x, y, size, threshold)` in unit coordinates.
    pub clods: Vec<(f64, f64, f64, f64)>,
}

fn build_recipe(key: &str) -> PatchRecipe {
    let mut rng = PyRandom::from_str_seed(key);
    let mut clods = Vec::with_capacity(6);
    for _ in 0..6 {
        let angle = rng.uniform(0.0, TAU);
        let distance = rng.uniform(0.55, 1.20);
        let x = angle.cos() * distance;
        let y = angle.sin() * distance * PATCH_FLATTEN;
        let size = rng.uniform(0.06, 0.15);
        let threshold = rng.uniform(0.0, 0.95);
        clods.push((x, y, size, threshold));
    }
    let body = ragged_outline(&mut rng, PATCH_POINTS, PATCH_FLATTEN, 0.24);
    let crown = ragged_outline(&mut rng, PATCH_POINTS - 5, PATCH_FLATTEN * 1.12, 0.30);
    let ox = rng.uniform(-0.18, 0.04);
    let oy = rng.uniform(-0.28, -0.14);
    PatchRecipe { body, crown, crown_offset: (ox, oy), clods }
}

/// `patch_recipe(key)`, cached (`_PATCH_CACHE`).
pub fn patch_recipe(key: &str) -> Arc<PatchRecipe> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<PatchRecipe>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(r) = cache.get(key) {
        return r.clone();
    }
    let recipe = Arc::new(build_recipe(key));
    cache.insert(key.to_string(), recipe.clone());
    recipe
}

/// `_blend(base, towards, amount)`.
pub fn blend(base: (u8, u8, u8), towards: (u8, u8, u8), amount: f64) -> (u8, u8, u8) {
    let m = |c: u8, t: u8| py_round(c as f64 + (t as f64 - c as f64) * amount) as u8;
    (m(base.0, towards.0), m(base.1, towards.1), m(base.2, towards.2))
}

/// `patch(outline, cx, cy, rx, ry)`: a closed path smoothed through the
/// outline's midpoints.
fn patch(outline: &[(f64, f64)], cx: f64, cy: f64, rx: f64, ry: f64) -> Path {
    let points: Vec<(f64, f64)> = outline.iter().map(|&(px, py)| (cx + px * rx, cy + py * ry)).collect();
    let count = points.len();
    let mut path = Path::new();
    let (first_x, first_y) = points[0];
    let (last_x, last_y) = points[count - 1];
    path.move_to((last_x + first_x) * 0.5, (last_y + first_y) * 0.5);
    for index in 0..count {
        let (px, py) = points[index];
        let (nx, ny) = points[(index + 1) % count];
        path.quad_to(px, py, (px + nx) * 0.5, (py + ny) * 0.5);
    }
    path.close_subpath();
    path
}

fn rgba(c: (u8, u8, u8), a: i64) -> Color {
    Color::rgba(c.0, c.1, c.2, a.clamp(0, 255) as u8)
}

/// `BaseWorld.render(painter, clip)`.
pub fn render_bases(p: &mut Canvas, bases: &[BaseDraw], clip: Option<Clip>) {
    for site in bases {
        let radius = site.radius();
        if let Some(c) = clip
            && (site.x + radius < c.0 || site.x - radius > c.2 || site.y + radius < c.1 || site.y - radius > c.3)
        {
            continue;
        }
        let completion = site.completion();
        if completion <= 0.01 && site.alert <= 0.01 {
            continue;
        }
        let team = site.team_color;
        let soil = blend((46, 34, 25), team, TEAM_TINT_SOIL);
        let earth = blend((86, 62, 42), team, TEAM_TINT_BODY);
        let crown = blend((118, 91, 63), team, TEAM_TINT_CROWN);
        let shade = blend((38, 27, 19), team, TEAM_TINT_SOIL);
        p.save();
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_pen(Pen::NONE);
        if completion > 0.01 {
            let field = patch_recipe(&format!("{}:field", site.id));
            let spread = radius * (0.52 + 0.46 * completion);
            p.set_brush(rgba(soil, (60.0 + 55.0 * completion) as i64));
            p.draw_path(&patch(&field.body, site.x, site.y + radius * 0.06, spread, spread * 1.12));
            let grit = patch_recipe(&format!("{}:grit", site.id));
            for &(gx, gy, gsize, gthreshold) in field.clods.iter().chain(grit.clods.iter()) {
                if completion < gthreshold {
                    continue;
                }
                let r = (spread * gsize * 0.16).max(0.7);
                let grit_x = site.x + gx * spread * 0.78;
                let grit_y = site.y + gy * spread * 0.78;
                p.set_brush(rgba(soil, 150));
                p.draw_ellipse(Rect::new(grit_x - r, grit_y - r, r * 2.0, r * 2.0));
            }
            let mut mounds = site.mounds();
            // `sorted(..., key=lambda m: m[1])`: stable, by y.
            mounds.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for (mx, my, size, aspect, built) in mounds {
                let grown = size * (0.35 + 0.65 * built);
                let half = grown * aspect;
                let recipe = patch_recipe(&format!("{}:patch:{:.2}:{:.2}", site.id, mx - site.x, my - site.y));
                p.set_brush(rgba(earth, (82.0 + 62.0 * built) as i64));
                p.draw_path(&patch(&recipe.body, mx, my, half * 1.62, grown * 1.48));
                let (offset_x, offset_y) = recipe.crown_offset;
                p.set_brush(rgba(crown, (20.0 + 34.0 * built) as i64));
                p.draw_path(&patch(
                    &recipe.crown,
                    mx + half * offset_x,
                    my + grown * offset_y,
                    half * 1.05,
                    grown * 0.94,
                ));
                p.set_brush(rgba(shade, (30.0 + 46.0 * built) as i64));
                p.draw_path(&patch(
                    &recipe.crown,
                    mx - half * offset_x * 0.6,
                    my + grown * 0.46,
                    half * 1.18,
                    grown * 0.52,
                ));
                for &(cx_unit, cy_unit, clod_size, threshold) in &recipe.clods {
                    if built < threshold {
                        continue;
                    }
                    let r = grown * clod_size;
                    p.set_brush(rgba(earth, (55.0 + 60.0 * built) as i64));
                    p.draw_ellipse(Rect::new(
                        mx + cx_unit * half - r,
                        my + cy_unit * grown - r * 0.62,
                        r * 2.0,
                        r * 1.24,
                    ));
                }
            }
        }
        if site.alert > 0.01 {
            p.set_pen(Pen::full(
                Color::rgba(245, 92, 72, (90.0 + site.alert * 130.0).clamp(0.0, 255.0) as u8),
                2.0,
                PenStyle::Solid,
                dc_render::Cap::Square,
                dc_render::Join::Bevel,
            ));
            p.set_brush(Brush::None);
            p.draw_ellipse(Rect::new(
                site.x - radius - 5.0,
                site.y - radius - 5.0,
                (radius + 5.0) * 2.0,
                (radius + 5.0) * 2.0,
            ));
        }
        p.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_rounds_like_python() {
        assert_eq!(blend((46, 34, 25), (255, 255, 255), 0.05), (56, 45, 36));
    }

    #[test]
    fn seeded_layout_matches_python() {
        // Values printed by the Python `_plan_mounds` / `patch_recipe`.
        let m = plan_mounds("base-1", 40.0);
        assert!((m[0].0 - -0.8459061900029954).abs() < 1e-12);
        assert!((m[1].1 - 4.017607863954536).abs() < 1e-12);
        assert!((m[1].2 - 8.058368447177202).abs() < 1e-12);
        let r = patch_recipe("base-1:field");
        assert!((r.crown_offset.0 - -0.1203931227049086).abs() < 1e-12);
        assert!((r.crown_offset.1 - -0.23474764144579388).abs() < 1e-12);
        assert_eq!(format!("{:.2}", -0.001f64), "-0.00");
        assert_eq!(format!("{:.2}", 2.675f64), "2.67");
        assert_eq!(format!("{:.2}", 0.125f64), "0.12");
    }
}
