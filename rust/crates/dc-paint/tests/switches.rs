//! The render switches, as `tests/test_leg_batching.py` and
//! `tests/test_leg_detail.py` check them on the Python side: batching and
//! the detail levels move only a few pixels, and a render is deterministic.

use dc_paint::compare::render_case;
use dc_paint::{CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Image};
use serde_json::Value;

fn case() -> Value {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/reference/rest__tarantula.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn render(sw: PaintSwitches) -> Image {
    render_case(&mut CreaturePainter::new(sw), &mut CpuRenderer::new(), &case())
}

fn moved(a: &Image, b: &Image) -> (usize, usize) {
    let (mut n, mut total) = (0, 0);
    for y in 0..a.height() {
        for x in 0..a.width() {
            // Over the whole frame, as the Python tests count.
            let (p, q) = (a.pixel_premultiplied(x, y), b.pixel_premultiplied(x, y));
            total += 1;
            if p.iter().zip(q.iter()).any(|(u, v)| u.abs_diff(*v) > 2) {
                n += 1;
            }
        }
    }
    (n, total)
}

#[test]
fn the_batched_render_is_deterministic() {
    let a = render(PaintSwitches::default());
    let b = render(PaintSwitches::default());
    assert_eq!(a.data(), b.data());
}

#[test]
fn batching_moves_only_a_little() {
    let batched = render(PaintSwitches::default());
    let direct = render(PaintSwitches { batch_legs: false, ..PaintSwitches::default() });
    let (n, total) = moved(&batched, &direct);
    assert!(n > 0, "nothing moved, so this proves nothing");
    assert!((n as f64) / (total as f64) < 0.02, "{n} of {total} pixels moved");
}

#[test]
fn the_detail_levels_move_only_a_little() {
    let levelled = render(PaintSwitches::default());
    let full = render(PaintSwitches { detail_enabled: false, ..PaintSwitches::default() });
    let (n, total) = moved(&levelled, &full);
    assert!(n > 0, "nothing changed, so this proves nothing");
    assert!((n as f64) / (total as f64) < 0.12, "{n} of {total} pixels moved");
}
