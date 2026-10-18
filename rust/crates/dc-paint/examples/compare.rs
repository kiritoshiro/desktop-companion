//! Render the cases dumped by `tools/dump_reference.py` through dc-paint
//! (`CpuRenderer`) and compare them with the Qt reference renders.
//!
//! ```text
//! CARGO_TARGET_DIR=target-dc-paint cargo run --release -p dc-paint --example compare [reference_dir]
//! ```
//!
//! Prints, per case and per model family, the mean channel difference (both
//! images composited over a mid-grey desktop, averaged over the pixels
//! either one covers) and the silhouette IoU (alpha > 64). Writes
//! side-by-side contact sheets (Qt | dc-paint | difference ×4) and the
//! dc-paint renders to `target-review/`.

use dc_core::Color;
use dc_paint::compare::{CaseResult, compare_images, render_case};
use dc_paint::{CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Font, Image, Rect, Scene};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reference = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| crate_dir.join("target-review").join("reference"));
    let out_dir = crate_dir.join("target-review");
    let renders = out_dir.join("rust");
    let sheets = out_dir.join("sheets");
    std::fs::create_dir_all(&renders).unwrap();
    std::fs::create_dir_all(&sheets).unwrap();

    let index: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(reference.join("cases.json")).expect("cases.json")).unwrap();
    let mut painter = CreaturePainter::new(PaintSwitches::default());
    let mut cpu = CpuRenderer::new();
    let mut results: Vec<(String, CaseResult, Image, Image)> = Vec::new();
    for entry in &index {
        let name = entry["name"].as_str().unwrap().to_string();
        let case: Value = serde_json::from_str(&std::fs::read_to_string(reference.join(format!("{name}.json"))).unwrap())
            .unwrap();
        let family = entry["family"].as_str().unwrap_or("other").to_string();
        let qt = Image::load_png(reference.join(format!("{name}.png"))).expect("reference png");
        let rust = render_case(&mut painter, &mut cpu, &case);
        rust.save_png(renders.join(format!("{name}.png"))).unwrap();
        let mut r = compare_images(&qt, &rust);
        r.family = family;
        r.name = name.clone();
        results.push((name, r, qt, rust));
    }

    // Per case.
    println!("{:<44} {:>10} {:>8} {:>8} {:>8} {:>8}", "case", "family", "mean", "IoU", "IoU1px", "cover");
    for (name, r, _, _) in &results {
        println!(
            "{:<44} {:>10} {:>8.2} {:>8.4} {:>8.4} {:>8.3}",
            name,
            short(&r.family),
            r.mean_diff,
            r.iou,
            r.iou_1px,
            r.coverage
        );
    }
    // Per family.
    let mut fam: BTreeMap<String, Vec<&CaseResult>> = BTreeMap::new();
    for (_, r, _, _) in &results {
        fam.entry(r.family.clone()).or_default().push(r);
    }
    println!();
    println!("{:<20} {:>5} {:>10} {:>10} {:>10} {:>10}", "family", "cases", "mean diff", "worst", "mean IoU", "min IoU");
    let mut summary = String::from("| family | cases | mean diff | worst diff | mean IoU | min IoU |\n|---|---|---|---|---|---|\n");
    for (f, rs) in &fam {
        let n = rs.len() as f64;
        let mean = rs.iter().map(|r| r.mean_diff).sum::<f64>() / n;
        let worst = rs.iter().map(|r| r.mean_diff).fold(0.0, f64::max);
        let iou = rs.iter().map(|r| r.iou).sum::<f64>() / n;
        let min_iou = rs.iter().map(|r| r.iou).fold(1.0, f64::min);
        println!("{:<20} {:>5} {:>10.2} {:>10.2} {:>10.4} {:>10.4}", f, rs.len(), mean, worst, iou, min_iou);
        summary.push_str(&format!("| {f} | {} | {mean:.2} | {worst:.2} | {iou:.4} | {min_iou:.4} |\n", rs.len()));
    }
    std::fs::write(out_dir.join("summary.md"), summary).unwrap();

    // Contact sheets, one per family.
    for (f, _) in &fam {
        let cases: Vec<&(String, CaseResult, Image, Image)> = results.iter().filter(|r| &r.1.family == f).collect();
        let sheet = contact_sheet(&cases);
        let file = format!("{}.png", f.replace('/', "_"));
        sheet.save_png(sheets.join(&file)).unwrap();
    }
    println!("\nsheets: {}", sheets.display());
}

fn short(s: &str) -> &str {
    if s.len() > 10 { &s[..10] } else { s }
}

/// Rows of `Qt | dc-paint | diff`, two cases per row for small cells.
fn contact_sheet(cases: &[&(String, CaseResult, Image, Image)]) -> Image {
    let cw = cases.iter().map(|c| c.2.width()).max().unwrap_or(1);
    let ch = cases.iter().map(|c| c.2.height()).max().unwrap_or(1) + 16;
    let per_row = if cw <= 260 { 2 } else { 1 };
    let rows = cases.len().div_ceil(per_row);
    let (w, h) = (cw * 3 * per_row as u32 + 8 * per_row as u32, ch * rows as u32);
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.fill_rect(Rect::new(0.0, 0.0, w as f64, h as f64), Color::rgb(40, 40, 46));
        for (i, (name, r, qt, rust)) in cases.iter().map(|c| (&c.0, &c.1, &c.2, &c.3)).enumerate() {
            let col = (i % per_row) as f64 * (cw as f64 * 3.0 + 8.0);
            let row = (i / per_row) as f64 * ch as f64;
            let diff = dc_paint::compare::diff_image(qt, rust);
            for (k, img) in [qt, rust, &diff].into_iter().enumerate() {
                let x = col + k as f64 * cw as f64;
                c.fill_rect(Rect::new(x, row + 16.0, img.width() as f64, img.height() as f64), desk(k));
                c.draw_image_at(x, row + 16.0, img);
            }
            c.set_pen(Color::rgb(235, 235, 235));
            c.set_font(Font::new("Segoe UI", 7.5));
            c.draw_text(col + 3.0, row + 12.0, &format!("{name}  diff {:.2}  IoU {:.4}", r.mean_diff, r.iou));
        }
    }
    CpuRenderer::new().render(&scene, w, h, Some(Color::rgb(40, 40, 46)))
}

fn desk(k: usize) -> Color {
    if k == 2 { Color::rgb(0, 0, 0) } else { Color::rgb(150, 156, 162) }
}
