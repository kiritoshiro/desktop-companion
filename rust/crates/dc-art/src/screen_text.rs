//! Port of app/screen_text.py: find the words on a frozen screen, without
//! reading them.
//!
//! The spiders need *where* text is, not *what* it says, so nothing is read:
//! no OCR, no text leaves the picture. Text is found by its shape -- small,
//! dense, high-contrast marks in rows a line tall.
//!
//! How: the picture's horizontal edges (a copy shifted one pixel,
//! differenced) are averaged into 4-pixel cells; cells busy with edges are
//! marked; marked cells that touch form blobs, and a blob a line tall and
//! wider than high is a word. Photos and icons are too tall or too solid.
//!
//! The Qt version drew the grey picture, then drew it again shifted with
//! `CompositionMode_Difference`, then smooth-scaled it down by four. Here the
//! same numbers are computed directly on one byte a pixel: the difference is
//! `|g(x) - g(x-1)|` (column 0 keeps `g(0)`), and the downscale is Qt's own
//! area-averaging arithmetic (`qt::smooth_scale_gray`), so the cells match
//! Qt value for value.

use std::collections::VecDeque;

use dc_core::Rect;
use dc_core::math::py_round;
use dc_render::Image;

use crate::qt::smooth_scale_gray;

/// Logical pixels per cell.
pub const CELL: usize = 4;
/// Mean edge strength (0..255) that marks a cell.
pub const EDGE_THRESHOLD: u8 = 34;
/// A word is 8 to 28 px tall ...
pub const MIN_H: i64 = 2;
pub const MAX_H: i64 = 7;
/// ... and at least 12 px wide.
pub const MIN_W: i64 = 3;
/// Longer runs are split into word-sized pieces.
pub const MAX_W: i64 = 28;
pub const MIN_FILL: f64 = 0.30;
pub const MAX_WORDS: usize = 700;

/// Boxes around word-like marks in `image`, overlay-local (`offset` is the
/// image's top-left there). `image` may carry a device pixel ratio; boxes are
/// in logical pixels.
pub fn find_text_boxes(image: &Image, offset: (f64, f64), limit: usize) -> Vec<Rect> {
    if image.is_null() {
        return Vec::new();
    }
    let grey = image.to_gray8();
    let ratio = match image.device_pixel_ratio() as f64 {
        r if r > 0.0 => r,
        _ => 1.0,
    };
    find_text_boxes_gray(&grey, image.width() as usize, image.height() as usize, ratio, offset, limit)
}

/// [`find_text_boxes`] on a grey picture (one byte a pixel, `width` x `height`
/// device pixels at `ratio`).
pub fn find_text_boxes_gray(
    grey: &[u8],
    dev_w: usize,
    dev_h: usize,
    ratio: f64,
    offset: (f64, f64),
    limit: usize,
) -> Vec<Rect> {
    let width = (dev_w as f64 / ratio) as usize;
    let height = (dev_h as f64 / ratio) as usize;
    let (cols, rows) = (width / CELL, height / CELL);
    if (cols as i64) < MIN_W || (rows as i64) < MIN_H {
        return Vec::new();
    }
    // The grey picture at its logical size. (The Qt code scaled it but kept
    // the device pixel ratio, so on a HiDPI screen it was drawn at a quarter
    // of the area; here it covers the edge image as intended.)
    let scaled;
    let grey = if dev_w == width && dev_h == height {
        grey
    } else {
        scaled = smooth_scale_gray(grey, dev_w, dev_h, width, height);
        &scaled[..]
    };
    let mut edges = vec![0u8; width * height];
    for y in 0..height {
        let row = &grey[y * width..(y + 1) * width];
        let out = &mut edges[y * width..(y + 1) * width];
        out[0] = row[0];
        for x in 1..width {
            out[x] = row[x].abs_diff(row[x - 1]);
        }
    }
    let cells = smooth_scale_gray(&edges, width, height, cols, rows);
    let marked: Vec<bool> = cells.iter().map(|&v| v >= EDGE_THRESHOLD).collect();
    let mut seen = vec![false; cols * rows];
    let mut boxes: Vec<(i64, i64, i64, i64)> = Vec::new();
    let mut queue = VecDeque::new();
    // Python walks a set (hash order); the blobs do not depend on the order
    // unless one grows past the height cut-off, so row-major order is used.
    for start in 0..cols * rows {
        if !marked[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        queue.clear();
        queue.push_back(start);
        let (sx, sy) = ((start % cols) as i64, (start / cols) as i64);
        let (mut x0, mut x1, mut y0, mut y1) = (sx, sx, sy, sy);
        let mut count = 0i64;
        while let Some(i) = queue.pop_front() {
            let (x, y) = ((i % cols) as i64, (i / cols) as i64);
            count += 1;
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            if y1 - y0 >= MAX_H + 3 {
                continue; // already far too tall to be text; stop growing it
            }
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= cols as i64 || ny >= rows as i64 {
                        continue;
                    }
                    let n = ny as usize * cols + nx as usize;
                    if marked[n] && !seen[n] {
                        seen[n] = true;
                        queue.push_back(n);
                    }
                }
            }
        }
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        if !((MIN_H..=MAX_H).contains(&h) && w >= MIN_W && w as f64 >= h as f64 * 1.2) {
            continue;
        }
        if (count as f64) / ((w * h) as f64) < MIN_FILL {
            continue;
        }
        boxes.push((x0, y0, w, h));
    }
    let boxes = merge_rows(boxes);
    let (ox, oy) = offset;
    let cell = CELL as f64;
    let mut out = Vec::new();
    for (x0, y0, w, h) in boxes {
        let pieces = if w > MAX_W { (py_round(w as f64 / MAX_W as f64) as i64).max(1) } else { 1 };
        let step = w as f64 / pieces as f64;
        for i in 0..pieces {
            out.push(Rect::new(
                ox + (x0 as f64 + i as f64 * step) * cell,
                oy + y0 as f64 * cell,
                step * cell,
                h as f64 * cell,
            ));
        }
    }
    out.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    out.truncate(limit);
    out
}

/// Join blobs on one line with at most one empty cell between: letters of a
/// word that did not quite touch.
fn merge_rows(mut boxes: Vec<(i64, i64, i64, i64)>) -> Vec<(i64, i64, i64, i64)> {
    boxes.sort_by_key(|b| (b.1, b.0));
    let mut merged: Vec<(i64, i64, i64, i64)> = Vec::new();
    for bx in boxes {
        let (x0, y0, w, h) = bx;
        let mut joined = false;
        for m in merged.iter_mut() {
            let (mx, my, mw, mh) = *m;
            let overlap = (y0 + h).min(my + mh) - y0.max(my);
            let gap = x0 - (mx + mw);
            if overlap as f64 >= h.min(mh) as f64 * 0.6
                && -mw <= gap
                && gap <= 1
                && (y0 + h).max(my + mh) - y0.min(my) <= MAX_H
            {
                let (nx0, ny0) = (mx.min(x0), my.min(y0));
                *m = (nx0, ny0, (mx + mw).max(x0 + w) - nx0, (my + mh).max(y0 + h) - ny0);
                joined = true;
                break;
            }
        }
        if !joined {
            merged.push(bx);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_core::Color;

    #[test]
    fn a_plain_picture_has_no_words() {
        let image = Image::filled(800, 600, Color::rgb(60, 90, 140));
        assert!(find_text_boxes(&image, (0.0, 0.0), MAX_WORDS).is_empty());
    }

    #[test]
    fn rows_merge_across_a_one_cell_gap() {
        let merged = merge_rows(vec![(10, 5, 3, 3), (0, 5, 4, 3), (20, 5, 3, 3)]);
        assert_eq!(merged, vec![(0, 5, 4, 3), (10, 5, 3, 3), (20, 5, 3, 3)]);
        let merged = merge_rows(vec![(0, 5, 4, 3), (5, 5, 3, 3)]);
        assert_eq!(merged, vec![(0, 5, 8, 3)]);
    }
}
