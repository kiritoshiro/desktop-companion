//! Port of `world/cage.py`: draggable, resizable containment cages.
//!
//! A cage is a rectangle plus hit-testing; the manager owns the list and
//! decides membership. Drawing is out of scope: a painter needs `x, y, w, h`,
//! [`Cage::HANDLE`] (corner grips), the member count and whether the cage is
//! the active one. [`Cage::corner_radius`] is the rounded-rect radius.

use dc_core::math::clamp;
use serde_json::{Value, json};

use crate::pymath::py_float;
use crate::{Bbox, Xywh};

/// `"nw"`, `"ne"`, `"sw"`, `"se"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    Nw,
    Ne,
    Sw,
    Se,
}

impl Corner {
    pub fn as_str(self) -> &'static str {
        match self {
            Corner::Nw => "nw",
            Corner::Ne => "ne",
            Corner::Sw => "sw",
            Corner::Se => "se",
        }
    }

    pub fn parse(s: &str) -> Option<Corner> {
        match s {
            "nw" => Some(Corner::Nw),
            "ne" => Some(Corner::Ne),
            "sw" => Some(Corner::Sw),
            "se" => Some(Corner::Se),
            _ => None,
        }
    }

    fn west(self) -> bool {
        matches!(self, Corner::Nw | Corner::Sw)
    }
    fn north(self) -> bool {
        matches!(self, Corner::Nw | Corner::Ne)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cage {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Cage {
    /// Half the side of the square corner grips.
    pub const HANDLE: f64 = 11.0;
    /// Thickness of the draggable border band.
    pub const BORDER: f64 = 14.0;
    pub const MIN_W: f64 = 90.0;
    pub const MIN_H: f64 = 90.0;

    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Cage {
        Cage { x, y, w: Self::MIN_W.max(w), h: Self::MIN_H.max(h) }
    }

    // -- persistence ------------------------------------------------------
    pub fn to_dict(&self) -> Value {
        json!({"x": self.x, "y": self.y, "w": self.w, "h": self.h})
    }

    pub fn from_dict(data: &Value) -> Option<Cage> {
        if !data.is_object() {
            return None;
        }
        let f = |k: &str| data.get(k).and_then(py_float);
        Some(Cage::new(f("x")?, f("y")?, f("w")?, f("h")?))
    }

    // -- geometry ---------------------------------------------------------
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn rect(&self) -> Xywh {
        (self.x, self.y, self.w, self.h)
    }

    /// `(x0, y0, x1, y1)` including the grips.
    pub fn outer_bbox(&self) -> Bbox {
        (self.x - Self::HANDLE, self.y - Self::HANDLE, self.right() + Self::HANDLE, self.bottom() + Self::HANDLE)
    }

    /// Everything the cage paints, as `(x, y, w, h)`.
    pub fn footprint_rect(&self, margin: f64) -> Xywh {
        let m = Self::HANDLE + margin;
        (self.x - m, self.y - m, self.w + 2.0 * m, self.h + 2.0 * m)
    }

    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        self.x <= px && px <= self.right() && self.y <= py && py <= self.bottom()
    }

    pub fn contains_center(&self, px: f64, py: f64) -> bool {
        self.contains_point(px, py)
    }

    /// Clamp bounds for a body centre: `(left, top, right, bottom)`.
    pub fn interior(&self, inset: f64) -> Bbox {
        let mut left = self.x + inset;
        let mut right = self.right() - inset;
        let mut top = self.y + inset;
        let mut bottom = self.bottom() - inset;
        if right < left {
            let cx = self.x + self.w * 0.5;
            left = cx;
            right = cx;
        }
        if bottom < top {
            let cy = self.y + self.h * 0.5;
            top = cy;
            bottom = cy;
        }
        (left, top, right, bottom)
    }

    pub fn clamp_center(&self, px: f64, py: f64, inset: f64) -> (f64, f64) {
        let (l, t, r, b) = self.interior(inset);
        (clamp(px, l, r), clamp(py, t, b))
    }

    // -- hit testing --------------------------------------------------------
    pub fn corner_at(&self, px: f64, py: f64) -> Option<Corner> {
        let h = Self::HANDLE + 3.0;
        [
            (Corner::Nw, self.x, self.y),
            (Corner::Ne, self.right(), self.y),
            (Corner::Sw, self.x, self.bottom()),
            (Corner::Se, self.right(), self.bottom()),
        ]
        .into_iter()
        .find(|&(_, cx, cy)| (px - cx).abs() <= h && (py - cy).abs() <= h)
        .map(|c| c.0)
    }

    /// True on the draggable frame band (not the open interior).
    pub fn on_border(&self, px: f64, py: f64) -> bool {
        let b = Self::BORDER;
        if !(self.x - b <= px && px <= self.right() + b && self.y - b <= py && py <= self.bottom() + b) {
            return false;
        }
        let inside_hole = self.x + b < px && px < self.right() - b && self.y + b < py && py < self.bottom() - b;
        !inside_hole
    }

    // -- mutation -----------------------------------------------------------
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Drag `corner` to `(px, py)`; the opposite corner stays fixed.
    pub fn resize_corner(&mut self, corner: Corner, px: f64, py: f64) {
        let mut left = self.x;
        let mut top = self.y;
        let mut right = self.right();
        let mut bottom = self.bottom();
        if corner.west() {
            left = px.min(right - Self::MIN_W);
        } else {
            right = px.max(left + Self::MIN_W);
        }
        if corner.north() {
            top = py.min(bottom - Self::MIN_H);
        } else {
            bottom = py.max(top + Self::MIN_H);
        }
        self.x = left;
        self.y = top;
        self.w = Self::MIN_W.max(right - left);
        self.h = Self::MIN_H.max(bottom - top);
    }

    pub fn clamp_to_screen(&mut self, screen_w: f64, screen_h: f64, margin: f64) {
        self.w = self.w.min(Self::MIN_W.max(screen_w - margin * 2.0));
        self.h = self.h.min(Self::MIN_H.max(screen_h - margin * 2.0));
        self.x = clamp(self.x, margin, margin.max(screen_w - self.w - margin));
        self.y = clamp(self.y, margin, margin.max(screen_h - self.h - margin));
    }

    // -- repaint helpers ------------------------------------------------------
    /// Thin bands tracing the fence plus the corner grips.
    pub fn border_rects(&self) -> [Xywh; 8] {
        let b = Self::BORDER + 2.0;
        let hg = Self::HANDLE + 4.0;
        let (x0, y0, x1, y1) = self.outer_bbox();
        let ow = x1 - x0;
        let g = |cx: f64, cy: f64| (cx - hg, cy - hg, hg * 2.0, hg * 2.0);
        [
            (x0, y0, ow, b + Self::HANDLE),
            (x0, y1 - b - Self::HANDLE, ow, b + Self::HANDLE),
            (x0, y0, b + Self::HANDLE, y1 - y0),
            (x1 - b - Self::HANDLE, y0, b + Self::HANDLE, y1 - y0),
            g(self.x, self.y),
            g(self.right(), self.y),
            g(self.x, self.bottom()),
            g(self.right(), self.bottom()),
        ]
    }

    /// The rounded-rect radius `Cage.draw` uses.
    pub fn corner_radius(&self) -> f64 {
        22.0f64.min(self.w * 0.12).min(self.h * 0.12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cage_round_trips_and_rejects_junk() {
        let c = Cage::new(200.0, 150.0, 320.0, 240.0);
        assert_eq!(Cage::from_dict(&c.to_dict()), Some(c));
        assert_eq!(Cage::from_dict(&json!({"x": 1})), None);
        assert_eq!(Cage::from_dict(&json!([1, 2])), None);
        assert_eq!(Cage::new(0.0, 0.0, 10.0, 10.0).rect(), (0.0, 0.0, 90.0, 90.0));
    }

    #[test]
    fn hit_testing_and_resizing() {
        let mut c = Cage::new(100.0, 100.0, 200.0, 200.0);
        assert_eq!(c.corner_at(102.0, 98.0), Some(Corner::Nw));
        assert_eq!(c.corner_at(200.0, 200.0), None);
        assert!(c.on_border(105.0, 200.0));
        assert!(!c.on_border(200.0, 200.0));
        c.resize_corner(Corner::Se, 150.0, 150.0);
        assert_eq!((c.w, c.h), (90.0, 90.0));
        c.resize_corner(Corner::Nw, 0.0, 50.0);
        assert_eq!((c.x, c.y, c.w, c.h), (0.0, 50.0, 190.0, 140.0));
        c.clamp_to_screen(150.0, 150.0, 4.0);
        assert_eq!((c.x, c.y, c.w, c.h), (4.0, 6.0, 142.0, 140.0));
        assert_eq!(c.clamp_center(-50.0, 500.0, 20.0), (24.0, 126.0));
    }
}
