//! The word finder returns the same boxes as the Python on synthetic
//! desktops (letter strokes drawn without a font, as the Python tests do).
//! Fixture: `tests/fixtures/word_boxes.json`, written by
//! `tools/qt_reference.py` from `app/screen_text.py` and
//! `app/desktop_surface.py`.

use dc_art::desktop_surface::{DesktopSurface, synthetic_snapshot};
use dc_art::screen_text::{MAX_WORDS, find_text_boxes};
use dc_core::Rect;
use serde_json::Value;

fn fixture() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/word_boxes.json"))
        .expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn rect(v: &Value) -> Rect {
    let a: Vec<f64> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    Rect::new(a[0], a[1], a[2], a[3])
}

fn same(a: &Rect, b: &Rect) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.w - b.w).abs() < 1e-9 && (a.h - b.h).abs() < 1e-9
}

#[test]
fn the_same_boxes_as_python_on_synthetic_desktops() {
    let fx = fixture();
    for (n, case) in fx["cases"].as_array().unwrap().iter().enumerate() {
        let screen = rect(&case["screen"]);
        let windows: Vec<Rect> = case["windows"].as_array().unwrap().iter().map(rect).collect();
        let rows = case["rows"].as_i64().unwrap();
        let snap = synthetic_snapshot(&[screen], &windows, 0, rows);
        let mut image = snap.screens[0].image.clone();
        let mut offset = (screen.x, screen.y);
        if let Some(c) = case["crop"].as_array() {
            let c: Vec<i64> = c.iter().map(|v| v.as_i64().unwrap()).collect();
            image = image.copy(c[0] as i32, c[1] as i32, c[2] as u32, c[3] as u32);
            offset = (screen.x + c[0] as f64, screen.y + c[1] as f64);
        }
        let ours = find_text_boxes(&image, offset, MAX_WORDS);
        let theirs: Vec<Rect> = case["boxes"].as_array().unwrap().iter().map(rect).collect();
        assert!(!theirs.is_empty(), "case {n} finds words");
        assert_eq!(ours.len(), theirs.len(), "case {n}: {} boxes vs Python's {}", ours.len(), theirs.len());
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            assert!(same(a, b), "case {n} box {i}: {a:?} vs Python's {b:?}");
        }
    }
}

#[test]
fn the_reclaim_desktop_finds_the_same_words_and_their_backgrounds() {
    let fx = fixture();
    let rc = &fx["reclaim"];
    let snap = synthetic_snapshot(
        &[Rect::new(0.0, 0.0, 1600.0, 1000.0)],
        &[Rect::new(200.0, 150.0, 700.0, 500.0), Rect::new(500.0, 300.0, 700.0, 500.0)],
        0,
        8,
    );
    let s = DesktopSurface::new(snap, Some(2), true);
    let theirs = rc["words"].as_array().unwrap();
    assert_eq!(s.words.len(), theirs.len());
    for (w, t) in s.words.iter().zip(theirs) {
        let t: Vec<f64> = t.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!(same(&w.rect, &Rect::new(t[0], t[1], t[2], t[3])), "{:?} vs {t:?}", w.rect);
        assert_eq!((w.colour.r as f64, w.colour.g as f64, w.colour.b as f64), (t[4], t[5], t[6]));
    }
}
