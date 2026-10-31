//! Affine transforms with `QTransform` semantics.
//!
//! Points are mapped as row vectors, like Qt:
//! `x' = m11·x + m21·y + dx`, `y' = m12·x + m22·y + dy`.
//! `a * b` (or [`Transform::then`]) applies `a` first and `b` second, which is
//! what `QTransform::operator*` does. The in-place `translate` / `rotate` /
//! `scale` methods act in *local* coordinates, exactly like
//! `QTransform::translate` and `QPainter::translate`.

use dc_core::{Rect, Vec2};
use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub m11: f64,
    pub m12: f64,
    pub m21: f64,
    pub m22: f64,
    pub dx: f64,
    pub dy: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, dx: 0.0, dy: 0.0 };

    pub const fn new(m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64) -> Self {
        Self { m11, m12, m21, m22, dx, dy }
    }

    pub const fn from_translate(dx: f64, dy: f64) -> Self {
        Self { m11: 1.0, m12: 0.0, m21: 0.0, m22: 1.0, dx, dy }
    }

    pub const fn from_scale(sx: f64, sy: f64) -> Self {
        Self { m11: sx, m12: 0.0, m21: 0.0, m22: sy, dx: 0.0, dy: 0.0 }
    }

    /// Rotation by `deg` degrees, clockwise on a y-down screen (`QTransform::rotate`).
    /// Multiples of 90° are exact, as in Qt.
    pub fn from_rotate_deg(deg: f64) -> Self {
        let (s, c) = sin_cos_deg(deg);
        Self { m11: c, m12: s, m21: -s, m22: c, dx: 0.0, dy: 0.0 }
    }

    pub fn from_rotate_rad(rad: f64) -> Self {
        Self::from_rotate_deg(rad.to_degrees())
    }

    /// `self` first, then `other`.
    pub fn then(&self, o: &Transform) -> Transform {
        Transform {
            m11: self.m11 * o.m11 + self.m12 * o.m21,
            m12: self.m11 * o.m12 + self.m12 * o.m22,
            m21: self.m21 * o.m11 + self.m22 * o.m21,
            m22: self.m21 * o.m12 + self.m22 * o.m22,
            dx: self.dx * o.m11 + self.dy * o.m21 + o.dx,
            dy: self.dx * o.m12 + self.dy * o.m22 + o.dy,
        }
    }

    /// Local translation (`QTransform::translate`).
    pub fn translate(&mut self, dx: f64, dy: f64) -> &mut Self {
        *self = Transform::from_translate(dx, dy).then(self);
        self
    }

    /// Local rotation in degrees (`QTransform::rotate`).
    pub fn rotate(&mut self, deg: f64) -> &mut Self {
        *self = Transform::from_rotate_deg(deg).then(self);
        self
    }

    /// Local scale (`QTransform::scale`).
    pub fn scale(&mut self, sx: f64, sy: f64) -> &mut Self {
        *self = Transform::from_scale(sx, sy).then(self);
        self
    }

    /// Local shear (`QTransform::shear`).
    pub fn shear(&mut self, sh: f64, sv: f64) -> &mut Self {
        *self = Transform::new(1.0, sv, sh, 1.0, 0.0, 0.0).then(self);
        self
    }

    #[inline]
    pub fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (self.m11 * x + self.m21 * y + self.dx, self.m12 * x + self.m22 * y + self.dy)
    }

    #[inline]
    pub fn map_point(&self, p: Vec2) -> Vec2 {
        let (x, y) = self.map(p.x, p.y);
        Vec2::new(x, y)
    }

    /// Maps a direction (no translation).
    #[inline]
    pub fn map_vector(&self, x: f64, y: f64) -> (f64, f64) {
        (self.m11 * x + self.m21 * y, self.m12 * x + self.m22 * y)
    }

    /// Bounding box of the mapped rectangle (`QTransform::mapRect`).
    pub fn map_rect(&self, r: Rect) -> Rect {
        let pts =
            [self.map(r.x, r.y), self.map(r.right(), r.y), self.map(r.right(), r.bottom()), self.map(r.x, r.bottom())];
        let (mut l, mut t, mut rr, mut b) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (x, y) in pts {
            l = l.min(x);
            t = t.min(y);
            rr = rr.max(x);
            b = b.max(y);
        }
        Rect::from_ltrb(l, t, rr, b)
    }

    pub fn determinant(&self) -> f64 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    pub fn is_invertible(&self) -> bool {
        self.determinant().abs() > 1e-12
    }

    pub fn inverted(&self) -> Option<Transform> {
        let det = self.determinant();
        if det.abs() <= 1e-12 || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        let m11 = self.m22 * inv;
        let m12 = -self.m12 * inv;
        let m21 = -self.m21 * inv;
        let m22 = self.m11 * inv;
        Some(Transform {
            m11,
            m12,
            m21,
            m22,
            dx: -(self.dx * m11 + self.dy * m21),
            dy: -(self.dx * m12 + self.dy * m22),
        })
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Only a translation (`QTransform::TxTranslate` or less).
    pub fn is_translate_only(&self) -> bool {
        self.m11 == 1.0 && self.m12 == 0.0 && self.m21 == 0.0 && self.m22 == 1.0
    }

    /// No rotation or shear: rectangles stay axis-aligned.
    pub fn is_axis_aligned(&self) -> bool {
        self.m12 == 0.0 && self.m21 == 0.0
    }

    /// Geometric-mean scale, `sqrt(|det|)`; exact for similarity transforms.
    pub fn scale_factor(&self) -> f64 {
        self.determinant().abs().sqrt()
    }

    /// Largest singular value: the most a unit vector can be stretched.
    pub fn max_scale(&self) -> f64 {
        let a = self.m11 * self.m11 + self.m12 * self.m12;
        let b = self.m11 * self.m21 + self.m12 * self.m22;
        let c = self.m21 * self.m21 + self.m22 * self.m22;
        let tr = (a + c) * 0.5;
        let d = ((a - c) * 0.5).hypot(b);
        (tr + d).max(0.0).sqrt()
    }

    /// Smallest singular value.
    pub fn min_scale(&self) -> f64 {
        let a = self.m11 * self.m11 + self.m12 * self.m12;
        let b = self.m11 * self.m21 + self.m12 * self.m22;
        let c = self.m21 * self.m21 + self.m22 * self.m22;
        let tr = (a + c) * 0.5;
        let d = ((a - c) * 0.5).hypot(b);
        (tr - d).max(0.0).sqrt()
    }

    pub(crate) fn to_f32(self) -> [f32; 6] {
        [self.m11 as f32, self.m12 as f32, self.m21 as f32, self.m22 as f32, self.dx as f32, self.dy as f32]
    }

    pub(crate) fn from_f32(m: [f32; 6]) -> Self {
        Self::new(m[0] as f64, m[1] as f64, m[2] as f64, m[3] as f64, m[4] as f64, m[5] as f64)
    }
}

impl Mul for Transform {
    type Output = Transform;
    fn mul(self, rhs: Transform) -> Transform {
        self.then(&rhs)
    }
}

/// `(sin, cos)` of an angle in degrees, exact at multiples of 90°.
pub(crate) fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let r = deg.rem_euclid(360.0);
    if r == 0.0 {
        (0.0, 1.0)
    } else if r == 90.0 {
        (1.0, 0.0)
    } else if r == 180.0 {
        (0.0, -1.0)
    } else if r == 270.0 {
        (-1.0, 0.0)
    } else {
        deg.to_radians().sin_cos()
    }
}

#[inline]
pub(crate) fn map_f32(m: &[f32; 6], x: f32, y: f32) -> [f32; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn local_ops_follow_qpainter_order() {
        // painter.translate(10, 0); painter.rotate(90); point (1, 0) -> (10, 1)
        let mut t = Transform::IDENTITY;
        t.translate(10.0, 0.0).rotate(90.0);
        assert!(close(t.map(1.0, 0.0), (10.0, 1.0)));
        // scale then translate: translate happens in scaled space
        let mut s = Transform::IDENTITY;
        s.scale(2.0, 3.0).translate(1.0, 1.0);
        assert!(close(s.map(0.0, 0.0), (2.0, 3.0)));
    }

    #[test]
    fn then_and_mul_agree() {
        let a = Transform::from_rotate_deg(30.0);
        let b = Transform::from_translate(5.0, -2.0);
        let p = (3.0, 4.0);
        let (x, y) = a.map(p.0, p.1);
        assert!(close((a * b).map(p.0, p.1), b.map(x, y)));
    }

    #[test]
    fn inverse_round_trips() {
        let mut t = Transform::IDENTITY;
        t.translate(12.0, -7.0).rotate(33.0).scale(1.5, 0.5).shear(0.2, 0.0);
        let inv = t.inverted().unwrap();
        let (x, y) = t.map(3.0, 9.0);
        assert!(close(inv.map(x, y), (3.0, 9.0)));
        assert!((t.then(&inv)).map(1.0, 2.0).0 - 1.0 < 1e-9);
        assert!(Transform::from_scale(0.0, 1.0).inverted().is_none());
    }

    #[test]
    fn right_angles_are_exact() {
        let t = Transform::from_rotate_deg(180.0);
        assert_eq!(t.map(2.0, 3.0), (-2.0, -3.0));
        let t = Transform::from_rotate_deg(-90.0);
        assert_eq!(t.map(1.0, 0.0), (0.0, -1.0));
    }

    #[test]
    fn map_rect_and_scales() {
        let t = Transform::from_rotate_deg(45.0);
        let r = t.map_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
        assert!((r.w - 2.0 * 2f64.sqrt()).abs() < 1e-9);
        let s = Transform::from_scale(2.0, 0.5);
        assert!((s.max_scale() - 2.0).abs() < 1e-12);
        assert!((s.min_scale() - 0.5).abs() < 1e-12);
        assert!((s.scale_factor() - 1.0).abs() < 1e-12);
        assert!(s.is_axis_aligned() && !t.is_axis_aligned());
    }
}
