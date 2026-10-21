//! Reference-render comparison support: rebuild a case dumped by
//! `tools/dump_reference.py`, render it with dc-paint, and measure it
//! against the Qt image (used by `examples/compare.rs` and the fixture
//! tests).

use crate::creature::CreaturePainter;
use crate::draw::CreatureDraw;
use crate::look::CreatureLook;
use crate::world::{
    BaseDraw, CageDraw, CarcassDraw, FlyWorldDraw, MouseWebDraw, WebDraw, draw_cage, draw_carcass, render_bases,
    render_flies, render_mouse_webs, render_webs,
};
use dc_core::Color;
use dc_render::{CpuRenderer, Image, RenderHint, Scene};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// How one case compares.
#[derive(Clone, Debug, Default)]
pub struct CaseResult {
    pub name: String,
    pub family: String,
    /// Mean absolute channel difference (0..255) over the pixels either
    /// image covers, both composited over a mid-grey desktop.
    pub mean_diff: f64,
    /// Silhouette intersection over union, alpha > 64.
    pub iou: f64,
    /// Total dc-paint coverage over total Qt coverage (sum of alpha): below
    /// 1 means dc-paint draws thinner or fainter.
    pub coverage: f64,
    /// Silhouette agreement allowing a one-pixel displacement: the share of
    /// each mask's pixels that have a pixel of the other mask within their
    /// 3x3 neighbourhood (a boundary F-score). Anti-aliasing differences on
    /// 1-2 px legs flip whole pixels in the strict IoU; this does not.
    pub iou_1px: f64,
}

fn get<T: DeserializeOwned + Default>(v: &Value, key: &str) -> T {
    v.get(key).cloned().and_then(|x| serde_json::from_value(x).ok()).unwrap_or_default()
}

/// Rebuild and render one dumped case (`kind` = creature, webs, flies,
/// mouse_webs, bases, props).
pub fn render_case(painter: &mut CreaturePainter, cpu: &mut CpuRenderer, case: &Value) -> Image {
    let w = case["w"].as_u64().unwrap_or(220) as u32;
    let h = case["h"].as_u64().unwrap_or(220) as u32;
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        match case["kind"].as_str().unwrap_or("") {
            "creature" => {
                let draw: CreatureDraw = serde_json::from_value(case["draw"].clone()).expect("CreatureDraw");
                let look = CreatureLook::from_model_json(&case["model"], true);
                let t = case["translate"].as_array().unwrap();
                c.translate(t[0].as_f64().unwrap(), t[1].as_f64().unwrap());
                let names = case["always_show_names"].as_bool().unwrap_or(false);
                let numbers = case["numbers"].as_bool().unwrap_or(false);
                painter.render(&mut c, &draw, &look, names, numbers);
            }
            "webs" => {
                let webs: Vec<WebDraw> = get(case, "webs");
                render_webs(&mut c, &webs, None);
            }
            "flies" => {
                let flies: FlyWorldDraw = get(case, "flies");
                render_flies(&mut c, &flies, None);
            }
            "mouse_webs" => {
                let worlds: Vec<MouseWebDraw> = get(case, "mouse_webs");
                for world in &worlds {
                    render_mouse_webs(&mut c, world, None);
                }
            }
            "bases" => {
                let bases: Vec<BaseDraw> = get(case, "bases");
                render_bases(&mut c, &bases, None);
            }
            "props" => {
                let cages: Vec<CageDraw> = get(case, "cages");
                let carcasses: Vec<CarcassDraw> = get(case, "carcasses");
                for cage in &cages {
                    draw_cage(&mut c, cage);
                }
                for carcass in &carcasses {
                    draw_carcass(&mut c, carcass, false);
                }
            }
            other => panic!("unknown case kind {other:?}"),
        }
    }
    cpu.render(&scene, w, h, Some(Color::TRANSPARENT))
}

const DESK: [f64; 3] = [150.0, 156.0, 162.0];

/// Straight RGB over the grey desktop, plus alpha.
fn over_desk(img: &Image, x: u32, y: u32) -> ([f64; 3], u8) {
    let [r, g, b, a] = img.pixel_premultiplied(x, y);
    let inv = 1.0 - a as f64 / 255.0;
    ([r as f64 + DESK[0] * inv, g as f64 + DESK[1] * inv, b as f64 + DESK[2] * inv], a)
}

/// Mean channel difference over the covered pixels and the silhouette IoU.
pub fn compare_images(qt: &Image, rust: &Image) -> CaseResult {
    let w = qt.width().min(rust.width());
    let h = qt.height().min(rust.height());
    let (mut sum, mut n) = (0.0, 0u64);
    let (mut inter, mut union) = (0u64, 0u64);
    let (mut cov_q, mut cov_r) = (0u64, 0u64);
    for y in 0..h {
        for x in 0..w {
            let (a, aa) = over_desk(qt, x, y);
            let (b, ba) = over_desk(rust, x, y);
            if aa > 8 || ba > 8 {
                sum += (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
                n += 3;
            }
            cov_q += aa as u64;
            cov_r += ba as u64;
            let (ma, mb) = (aa > 64, ba > 64);
            if ma && mb {
                inter += 1;
            }
            if ma || mb {
                union += 1;
            }
        }
    }
    CaseResult {
        mean_diff: if n > 0 { sum / n as f64 } else { 0.0 },
        iou: if union > 0 { inter as f64 / union as f64 } else { 1.0 },
        coverage: if cov_q > 0 { cov_r as f64 / cov_q as f64 } else { 1.0 },
        iou_1px: tolerant_overlap(qt, rust, w, h),
        ..CaseResult::default()
    }
}

fn tolerant_overlap(qt: &Image, rust: &Image, w: u32, h: u32) -> f64 {
    let mask = |img: &Image| -> Vec<bool> {
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| img.pixel_premultiplied(x, y)[3] > 64).collect()
    };
    let (a, b) = (mask(qt), mask(rust));
    let near = |m: &[bool], x: u32, y: u32| {
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 && m[(ny as u32 * w + nx as u32) as usize] {
                    return true;
                }
            }
        }
        false
    };
    let (mut total, mut matched) = (0u64, 0u64);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if a[i] {
                total += 1;
                matched += near(&b, x, y) as u64;
            }
            if b[i] {
                total += 1;
                matched += near(&a, x, y) as u64;
            }
        }
    }
    if total == 0 { 1.0 } else { matched as f64 / total as f64 }
}

/// `|qt - rust|` over the desktop, amplified ×4, as an opaque image.
pub fn diff_image(qt: &Image, rust: &Image) -> Image {
    let w = qt.width().min(rust.width());
    let h = qt.height().min(rust.height());
    let mut out = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (a, _) = over_desk(qt, x, y);
            let (b, _) = over_desk(rust, x, y);
            let d = (a[0] - b[0]).abs().max((a[1] - b[1]).abs()).max((a[2] - b[2]).abs());
            let v = (d * 4.0).min(255.0) as u8;
            out.set_pixel(x, y, Color::rgba(v, v / 3, v / 3, 255));
        }
    }
    out
}
