//! dc-paint against Qt: a few committed reference cases from
//! `tools/dump_reference.py` (procedural tarantula, a stressed procedural
//! spider with armour, net, startle and camouflage, the regal jumper, a
//! sprite rig and the bases), rendered with `CpuRenderer` and compared with
//! the Qt image within a perceptual tolerance. The full 62-model run is
//! `examples/compare.rs`.

use dc_paint::compare::{compare_images, render_case};
use dc_paint::{CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Image};
use serde_json::Value;
use std::path::PathBuf;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn check(name: &str) {
    let dir = crate_dir().join("tests/fixtures/reference");
    let mut case: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
    // The dump records an absolute model folder; point it at this checkout.
    if let Some(folder) = case.pointer("/model/_folder").and_then(Value::as_str) {
        let leaf = folder.rsplit(['\\', '/']).next().unwrap().to_string();
        let local = crate_dir().join("../../../models").join(leaf);
        case["model"]["_folder"] = Value::String(local.to_string_lossy().into_owned());
    }
    let qt = Image::load_png(dir.join(format!("{name}.png"))).unwrap();
    let mut painter = CreaturePainter::new(PaintSwitches::default());
    let rust = render_case(&mut painter, &mut CpuRenderer::new(), &case);
    let r = compare_images(&qt, &rust);
    eprintln!("{name}: mean {:.2} IoU {:.4} IoU(1px) {:.4} coverage {:.3}", r.mean_diff, r.iou, r.iou_1px, r.coverage);
    assert!(r.mean_diff < 6.0, "{name}: mean channel difference {:.2}", r.mean_diff);
    assert!(r.iou > 0.94, "{name}: silhouette IoU {:.4}", r.iou);
    assert!(r.iou_1px > 0.995, "{name}: 1 px tolerant IoU {:.4}", r.iou_1px);
    assert!((r.coverage - 1.0).abs() < 0.03, "{name}: coverage ratio {:.3}", r.coverage);
}

#[test]
fn procedural_tarantula_matches_qt() {
    check("rest__tarantula");
}

#[test]
fn stressed_spider_matches_qt() {
    check("stress__spider");
}

#[test]
fn regal_jumper_matches_qt() {
    check("rest__regal_fluff");
}

#[test]
fn sprite_rig_matches_qt() {
    check("rest__plush_curly_hybrid");
}

#[test]
fn bases_match_qt() {
    check("world__bases");
}
