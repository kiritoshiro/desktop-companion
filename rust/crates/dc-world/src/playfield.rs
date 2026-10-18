//! Port of world/playfield.py
//!
//! Where a spider is allowed to be: the screens, not their bounding box
//! (DC-65). The overlay spans the bounding rectangle of every monitor; on a
//! mixed layout that box contains dead space no screen shows. The habitable
//! area is the union of the actual screen rectangles.
//!
//! **The fast path matters.** One monitor is the common case and must cost
//! nothing: `simple` is true whenever the rectangles exactly tile their own
//! bounding box, and every method then falls through to plain arithmetic.
//!
//! Coordinates are overlay-local. Translating monitor geometry into that
//! space is the caller's job ([`rects_from_geometry`]).

/// One monitor, in overlay-local pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl ScreenRect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// Whether the point lies inside, `margin` clear of every edge. The
    /// margin is capped at half the rectangle so a narrow monitor stays
    /// habitable.
    pub fn contains(&self, px: f64, py: f64, margin: f64) -> bool {
        let mx = margin.min(self.w * 0.5);
        let my = margin.min(self.h * 0.5);
        self.x + mx <= px && px <= self.right() - mx && self.y + my <= py && py <= self.bottom() - my
    }

    /// `contains` with no margin.
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        self.contains(px, py, 0.0)
    }

    pub fn clamp(&self, px: f64, py: f64, margin: f64) -> (f64, f64) {
        let mx = margin.min(self.w * 0.5);
        let my = margin.min(self.h * 0.5);
        (px.max(self.x + mx).min(self.right() - mx), py.max(self.y + my).min(self.bottom() - my))
    }
}

/// The habitable area of the overlay: a union of screen rectangles.
///
/// Constructed with the overlay's own size and no rectangles, which means
/// "the whole window is habitable".
#[derive(Debug, Clone, PartialEq)]
pub struct Playfield {
    pub width: f64,
    pub height: f64,
    rects: Vec<ScreenRect>,
    simple: bool,
}

impl Playfield {
    pub fn new(width: f64, height: f64, rects: &[ScreenRect]) -> Self {
        let mut field = Self { width, height, rects: Vec::new(), simple: true };
        field.set_rects(rects);
        field
    }

    /// No rectangles: the whole window is habitable.
    pub fn whole(width: f64, height: f64) -> Self {
        Self::new(width, height, &[])
    }

    // -- definition ---------------------------------------------------------

    pub fn set_rects(&mut self, rects: &[ScreenRect]) {
        let cleaned: Vec<ScreenRect> = rects.iter().copied().filter(|r| r.w > 1.0 && r.h > 1.0).collect();
        self.simple = self.is_whole_window(&cleaned);
        self.rects = cleaned;
    }

    pub fn set_size(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        self.simple = self.is_whole_window(&self.rects);
    }

    pub fn rects(&self) -> &[ScreenRect] {
        &self.rects
    }

    /// Whether the screens tile the whole overlay, so no work is needed.
    pub fn simple(&self) -> bool {
        self.simple
    }

    fn is_whole_window(&self, rects: &[ScreenRect]) -> bool {
        if rects.is_empty() {
            return true;
        }
        // Compared by area rather than by shape.
        let covered: f64 = rects.iter().map(|r| r.w * r.h).sum();
        (covered - self.width * self.height).abs() < 1.0
    }

    // -- queries --------------------------------------------------------------

    fn covered(&self, px: f64, py: f64) -> bool {
        self.rects.iter().any(|r| r.contains(px, py, 0.0))
    }

    /// Whether a body of half-size `margin` at the point is on the screens,
    /// judged against the monitors together.
    pub fn contains(&self, px: f64, py: f64, margin: f64) -> bool {
        if self.simple {
            return margin <= px && px <= self.width - margin && margin <= py && py <= self.height - margin;
        }
        if margin <= 0.0 {
            return self.covered(px, py);
        }
        let m = margin;
        for dx in [-m, 0.0, m] {
            for dy in [-m, 0.0, m] {
                if !self.covered(px + dx, py + dy) {
                    return false;
                }
            }
        }
        true
    }

    /// (left, right, top, bottom): which edges another monitor continues.
    fn open_sides(&self, index: usize) -> (bool, bool, bool, bool) {
        fn overlaps(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
            a1.min(b1) - a0.max(b0) > 1.0
        }
        let rect = self.rects[index];
        let (mut left, mut right, mut top, mut bottom) = (false, false, false, false);
        for (i, other) in self.rects.iter().enumerate() {
            if i == index {
                continue;
            }
            if overlaps(rect.y, rect.bottom(), other.y, other.bottom()) {
                left = left || (other.right() - rect.x).abs() <= 1.5;
                right = right || (other.x - rect.right()).abs() <= 1.5;
            }
            if overlaps(rect.x, rect.right(), other.x, other.right()) {
                top = top || (other.bottom() - rect.y).abs() <= 1.5;
                bottom = bottom || (other.y - rect.bottom()).abs() <= 1.5;
            }
        }
        (left, right, top, bottom)
    }

    /// Nearest habitable point, which is `(px, py)` when already on one.
    pub fn clamp(&self, px: f64, py: f64, margin: f64) -> (f64, f64) {
        if self.simple {
            return (
                px.max(margin).min(margin.max(self.width - margin)),
                py.max(margin).min(margin.max(self.height - margin)),
            );
        }
        if self.rects.is_empty() {
            return (px, py);
        }
        if self.contains(px, py, margin) {
            return (px, py);
        }
        let mut best: Option<(f64, f64)> = None;
        let mut best_d = f64::INFINITY;
        for (index, rect) in self.rects.iter().enumerate() {
            let mut candidates = vec![rect.clamp(px, py, margin)];
            if margin > 0.0 {
                let (left, right, top, bottom) = self.open_sides(index);
                let mx = margin.min(rect.w * 0.5);
                let my = margin.min(rect.h * 0.5);
                let x0 = rect.x + if left { 0.0 } else { mx };
                let x1 = rect.right() - if right { 0.0 } else { mx };
                let y0 = rect.y + if top { 0.0 } else { my };
                let y1 = rect.bottom() - if bottom { 0.0 } else { my };
                let seam = (px.max(x0).min(x1), py.max(y0).min(y1));
                if self.contains(seam.0, seam.1, margin) {
                    candidates.push(seam);
                }
            }
            for (cx, cy) in candidates {
                let d = (cx - px).powi(2) + (cy - py).powi(2);
                if d < best_d {
                    best_d = d;
                    best = Some((cx, cy));
                }
            }
        }
        best.unwrap_or((px, py))
    }
}

/// Translate absolute monitor rectangles `(x, y, w, h)` into overlay-local
/// ones, given the overlay window's top-left in the same space.
pub fn rects_from_geometry(screens: &[(f64, f64, f64, f64)], origin_x: f64, origin_y: f64) -> Vec<ScreenRect> {
    screens.iter().map(|&(x, y, w, h)| ScreenRect::new(x - origin_x, y - origin_y, w, h)).collect()
}

#[cfg(test)]
mod tests {
    //! Port of the geometry part of tests/test_two_screens.py.
    use super::*;

    const BIG: ScreenRect = ScreenRect::new(0.0, 0.0, 800.0, 600.0);
    const SMALL: ScreenRect = ScreenRect::new(800.0, 0.0, 500.0, 360.0);
    const DEAD: (f64, f64) = (1100.0, 500.0);

    fn mixed() -> Playfield {
        Playfield::new(1300.0, 600.0, &[BIG, SMALL])
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * b.abs().max(1.0)
    }

    #[test]
    fn one_monitor_costs_nothing() {
        let field = Playfield::new(800.0, 600.0, &[ScreenRect::new(0.0, 0.0, 800.0, 600.0)]);
        assert!(field.simple());
        assert_eq!(field.clamp(400.0, 300.0, 0.0), (400.0, 300.0));
    }

    #[test]
    fn knowing_nothing_excludes_nothing() {
        let field = Playfield::whole(1300.0, 600.0);
        assert!(field.simple());
        assert!(field.contains(DEAD.0, DEAD.1, 0.0));
    }

    #[test]
    fn two_monitors_that_tile_are_still_simple() {
        let field = Playfield::new(
            1600.0,
            600.0,
            &[ScreenRect::new(0.0, 0.0, 800.0, 600.0), ScreenRect::new(800.0, 0.0, 800.0, 600.0)],
        );
        assert!(field.simple());
    }

    #[test]
    fn mixed_monitors_leave_a_hole() {
        let field = mixed();
        assert!(!field.simple());
        assert!(field.contains(400.0, 300.0, 0.0));
        assert!(field.contains(1000.0, 100.0, 0.0));
        assert!(!field.contains(DEAD.0, DEAD.1, 0.0));
    }

    #[test]
    fn a_point_in_the_hole_is_pushed_to_the_nearest_screen() {
        let field = mixed();
        let (x, y) = field.clamp(DEAD.0, DEAD.1, 0.0);
        assert!(field.contains(x, y, 0.0));
        assert!(approx(y, SMALL.bottom()));
        assert!(approx(x, DEAD.0));
    }

    #[test]
    fn a_margin_keeps_a_body_clear_of_the_edge() {
        let field = mixed();
        let (x, y) = field.clamp(DEAD.0, DEAD.1, 20.0);
        assert!(field.contains(x, y, 20.0));
        assert!(approx(y, SMALL.bottom() - 20.0));
    }

    #[test]
    fn a_margin_wider_than_a_monitor_does_not_make_it_uninhabitable() {
        let sliver = ScreenRect::new(0.0, 0.0, 30.0, 600.0);
        let field = Playfield::new(1300.0, 600.0, &[sliver, SMALL]);
        let (x, _) = field.clamp(10.0, 300.0, 200.0);
        assert!((0.0..=30.0).contains(&x));
    }

    #[test]
    fn geometry_is_translated_into_overlay_local_pixels() {
        let rects = rects_from_geometry(&[(-1920.0, 0.0, 1920.0, 1080.0), (0.0, 0.0, 2560.0, 1440.0)], -1920.0, 0.0);
        assert_eq!(rects[0], ScreenRect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(rects[1], ScreenRect::new(1920.0, 0.0, 2560.0, 1440.0));
    }

    #[test]
    fn the_join_between_two_monitors_is_not_an_edge() {
        let field = mixed();
        let margin = 40.0;
        let on_join = (BIG.right(), 150.0);
        assert!(field.contains(on_join.0, on_join.1, margin));
        assert_eq!(field.clamp(on_join.0, on_join.1, margin), on_join);
        assert!(!field.contains(BIG.right() - 10.0, 590.0, margin));
        assert!(!field.contains(DEAD.0, DEAD.1, margin));
        let (x, y) = field.clamp(BIG.right() + 5.0, 590.0, margin);
        assert!(field.contains(x, y, margin));
    }
}
