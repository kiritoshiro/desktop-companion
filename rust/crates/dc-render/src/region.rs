//! `QRegion`: a set of disjoint axis-aligned rectangles.
//!
//! Also stands in for `QPainterPath::subtracted` / `united` / `intersected`,
//! which the Python code only ever applies to rectangles (window panes in
//! `app/desktop_surface.py`): build a `Region` and call [`Region::to_path`].

use crate::path::Path;
use dc_core::Rect;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Region {
    rects: Vec<Rect>,
}

impl Region {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_rect(r: Rect) -> Self {
        let mut s = Self::default();
        if !r.is_empty() {
            s.rects.push(r);
        }
        s
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The disjoint rectangles (`QRegion::rects`).
    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    pub fn bounding_rect(&self) -> Rect {
        self.rects.iter().fold(Rect::default(), |a, r| a.united(r))
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.rects.iter().any(|r| r.contains(x, y))
    }

    pub fn intersects_rect(&self, r: &Rect) -> bool {
        self.rects.iter().any(|q| q.intersects(r))
    }

    /// `region -= rect`.
    pub fn subtract_rect(&mut self, cut: &Rect) {
        if cut.is_empty() {
            return;
        }
        let mut out = Vec::with_capacity(self.rects.len() + 4);
        for r in &self.rects {
            subtract_one(r, cut, &mut out);
        }
        self.rects = out;
    }

    /// `region += rect`.
    pub fn unite_rect(&mut self, add: &Rect) {
        if add.is_empty() {
            return;
        }
        let mut pieces = vec![*add];
        for r in &self.rects {
            let mut next = Vec::with_capacity(pieces.len());
            for p in &pieces {
                subtract_one(p, r, &mut next);
            }
            pieces = next;
            if pieces.is_empty() {
                return;
            }
        }
        self.rects.extend(pieces);
    }

    /// `region &= rect`.
    pub fn intersect_rect(&mut self, r: &Rect) {
        self.rects = self.rects.iter().filter_map(|q| q.intersection(r)).collect();
    }

    pub fn united(&self, o: &Region) -> Region {
        let mut s = self.clone();
        for r in &o.rects {
            s.unite_rect(r);
        }
        s
    }

    pub fn subtracted(&self, o: &Region) -> Region {
        let mut s = self.clone();
        for r in &o.rects {
            s.subtract_rect(r);
        }
        s
    }

    pub fn intersected(&self, o: &Region) -> Region {
        let mut out = Region::new();
        for a in &self.rects {
            for b in &o.rects {
                if let Some(i) = a.intersection(b) {
                    out.rects.push(i);
                }
            }
        }
        out
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Region {
        Region { rects: self.rects.iter().map(|r| r.translated(dx, dy)).collect() }
    }

    /// Total covered area.
    pub fn area(&self) -> f64 {
        self.rects.iter().map(|r| r.w * r.h).sum()
    }

    /// The region as a path of disjoint rectangles.
    pub fn to_path(&self) -> Path {
        let mut p = Path::with_capacity(self.rects.len() * 5);
        for r in &self.rects {
            p.add_rect(*r);
        }
        p
    }
}

fn subtract_one(r: &Rect, cut: &Rect, out: &mut Vec<Rect>) {
    let Some(i) = r.intersection(cut) else {
        out.push(*r);
        return;
    };
    // Top band, bottom band, then left and right of the middle band.
    if i.y > r.y {
        out.push(Rect::from_ltrb(r.x, r.y, r.right(), i.y));
    }
    if i.bottom() < r.bottom() {
        out.push(Rect::from_ltrb(r.x, i.bottom(), r.right(), r.bottom()));
    }
    if i.x > r.x {
        out.push(Rect::from_ltrb(r.x, i.y, i.x, i.bottom()));
    }
    if i.right() < r.right() {
        out.push(Rect::from_ltrb(i.right(), i.y, r.right(), i.bottom()));
    }
}

impl From<Rect> for Region {
    fn from(r: Rect) -> Self {
        Region::from_rect(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtract_and_unite() {
        let mut r = Region::from_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        r.subtract_rect(&Rect::new(25.0, 25.0, 50.0, 50.0));
        assert_eq!(r.area(), 10000.0 - 2500.0);
        assert!(!r.contains(50.0, 50.0) && r.contains(10.0, 50.0));
        r.unite_rect(&Rect::new(0.0, 0.0, 60.0, 60.0));
        assert_eq!(r.area(), 10000.0 - 2500.0 + 35.0 * 35.0);
        let p = r.to_path();
        assert!(p.contains(30.0, 30.0) && !p.contains(70.0, 70.0));
        let i = r.intersected(&Region::from_rect(Rect::new(90.0, 90.0, 50.0, 50.0)));
        assert_eq!(i.area(), 100.0);
    }
}
