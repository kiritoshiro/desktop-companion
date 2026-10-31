//! Port of tools/generate_ui_art.py: writes the carved-wood interface art.
//!
//! ```text
//! CARGO_TARGET_DIR=target-dc-art cargo run --release -p dc-art --example generate_ui_art [out_dir]
//! ```
//!
//! Writes `wood_tile.png`, `wood_dark_tile.png`, `card_frame.png` and every
//! `menu_*.png` emblem into `out_dir` (default
//! `crates/dc-art/target-review/ui`; pass `../assets/ui` to regenerate the
//! shipped assets). The mode pictures and the app icon need the procedural
//! spider (dc-creature / dc-paint) and are still made by the Python tool.

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target-review").join("ui"));
    std::fs::create_dir_all(&out).expect("create the output directory");
    let start = Instant::now();
    for (name, image) in dc_art::ui_art::generate_all() {
        image.save_png(out.join(&name)).unwrap_or_else(|e| panic!("write {name}: {e}"));
        println!("wrote {name}");
    }
    println!("done in {:.0} ms -> {}", start.elapsed().as_secs_f64() * 1000.0, out.display());
}
