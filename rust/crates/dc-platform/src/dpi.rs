//! Port of support/dpi.py: high-DPI / multi-monitor coordinate helpers (DC-14).
//!
//! Two coordinate spaces meet at the edges of the overlay: logical
//! (device-independent, 96 DPI per unit, scaled per screen) and physical
//! (the monitor's real pixel grid, which every raw Win32 call uses). On a
//! 100% monitor they coincide; elsewhere they differ by the screen's device
//! pixel ratio and every crossing must convert explicitly.
//!
//! Rust differences: Qt's `AA_EnableHighDpiScaling` becomes
//! [`enable_high_dpi_scaling`], which makes the process per-monitor-v2 DPI
//! aware (winit does the same at event-loop creation, so this only matters
//! for processes that use Win32 before winit). The ratio lookup asks
//! Windows for the monitor's effective DPI instead of `QScreen`.
//!
//! [`DpiMap`] is the whole-desktop version of the two conversions, for the
//! overlay that spans every monitor: Qt 5's logical layout, where each
//! screen keeps its physical origin and its size is divided by its ratio
//! (`QHighDpi::fromNativeScreenGeometry`), and a point converts about the
//! origin of the screen it is on (`QHighDpi::fromNativePixels`). The helpers
//! above scale about (0, 0), which is only right on a screen at the origin;
//! Python used them for the cursor trap and the window snapshot anyway.
//! Windows never scales below 100%, so logical rectangles sit inside the
//! physical ones: they never overlap, and the logical bounding box has the
//! physical one's top-left corner. Logical points between them (a 150%
//! screen beside a 100% one leaves a gap) belong to no pixel on screen.

use dc_core::{IRect, Rect};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

use crate::monitors::{MonitorInfo, monitor_at, monitors, virtual_screen_geometry_of};

/// Make the process per-monitor (v2) DPI aware, so Win32 reports physical
/// pixels and Windows stops bitmap-stretching the overlay. Safe to call more
/// than once; only the first call in a process can change anything, exactly
/// like Qt's attribute.
pub fn enable_high_dpi_scaling() -> bool {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
}

fn ratio_or_one(dpr: f64) -> f64 {
    if dpr > 0.0 && dpr.is_finite() { dpr } else { 1.0 }
}

/// Convert a logical point to native pixels. A non-positive ratio is treated
/// as 1.0 so a bad lookup degrades to a no-op instead of dividing by zero.
pub fn logical_to_physical(x: f64, y: f64, dpr: f64) -> (f64, f64) {
    let ratio = ratio_or_one(dpr);
    (x * ratio, y * ratio)
}

/// Convert a native-pixel point to logical coordinates (inverse of
/// [`logical_to_physical`]).
pub fn physical_to_logical(x: f64, y: f64, dpr: f64) -> (f64, f64) {
    let ratio = ratio_or_one(dpr);
    (x / ratio, y / ratio)
}

/// Device pixel ratio for a DPI value (96 DPI = 1.0).
pub fn ratio_for_dpi(dpi: u32) -> f64 {
    if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
}

/// Best-effort device pixel ratio of the screen under a (physical) point.
/// Falls back to the nearest screen, then to 1.0.
pub fn screen_device_pixel_ratio_at(x: f64, y: f64) -> f64 {
    let (xi, yi) = (dc_core::math::py_round(x) as i32, dc_core::math::py_round(y) as i32);
    monitor_at(xi, yi).map(|m| m.scale()).unwrap_or(1.0)
}

/// One monitor in both spaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaledScreen {
    /// The whole monitor, physical pixels.
    pub rect: IRect,
    /// Device pixel ratio (1.0 at 100%).
    pub scale: f64,
}

impl ScaledScreen {
    pub fn new(rect: IRect, scale: f64) -> Self {
        Self { rect, scale: ratio_or_one(scale) }
    }

    /// The monitor in logical pixels: same origin, size divided by the ratio.
    pub fn logical(&self) -> Rect {
        let r = self.rect;
        Rect::new(r.x as f64, r.y as f64, r.w as f64 / self.scale, r.h as f64 / self.scale)
    }

    /// A physical point to logical, about this monitor's origin. Exact (the
    /// point itself) at 100%.
    pub fn to_logical(&self, x: f64, y: f64) -> (f64, f64) {
        if self.scale == 1.0 {
            return (x, y);
        }
        let (ox, oy) = (self.rect.x as f64, self.rect.y as f64);
        (ox + (x - ox) / self.scale, oy + (y - oy) / self.scale)
    }

    /// A logical point to physical (inverse of [`Self::to_logical`]).
    pub fn to_physical(&self, x: f64, y: f64) -> (f64, f64) {
        if self.scale == 1.0 {
            return (x, y);
        }
        let (ox, oy) = (self.rect.x as f64, self.rect.y as f64);
        (ox + (x - ox) * self.scale, oy + (y - oy) * self.scale)
    }
}

/// Squared distance from a point to a rectangle (0 inside).
fn distance2(r: &Rect, x: f64, y: f64) -> f64 {
    let dx = (r.x - x).max(0.0).max(x - r.right());
    let dy = (r.y - y).max(0.0).max(y - r.bottom());
    dx * dx + dy * dy
}

/// Every monitor with its scale: converts points and rectangles between
/// physical and logical pixels the way Qt 5 lays a mixed-DPI desktop out
/// (see the module documentation). An empty map, or one where every
/// monitor is at 100%, converts nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DpiMap {
    screens: Vec<ScaledScreen>,
}

impl DpiMap {
    pub fn new(screens: impl IntoIterator<Item = ScaledScreen>) -> Self {
        Self { screens: screens.into_iter().map(|s| ScaledScreen::new(s.rect, s.scale)).collect() }
    }

    /// The monitors as Windows reports them, in their order.
    pub fn from_monitors(list: &[MonitorInfo]) -> Self {
        Self::new(list.iter().map(|m| ScaledScreen::new(m.rect, m.scale())))
    }

    /// The same layout with every monitor moved by `(dx, dy)`: pass minus
    /// the overlay's origin to convert overlay-local points. The origin is
    /// the same in both spaces, so a local point stays local.
    pub fn translated(&self, dx: i32, dy: i32) -> Self {
        let moved = |r: IRect| IRect::new(r.x + dx, r.y + dy, r.w, r.h);
        Self { screens: self.screens.iter().map(|s| ScaledScreen { rect: moved(s.rect), scale: s.scale }).collect() }
    }

    pub fn screens(&self) -> &[ScaledScreen] {
        &self.screens
    }

    /// True when the two spaces are the same (every monitor at 100%).
    pub fn is_identity(&self) -> bool {
        self.screens.iter().all(|s| s.scale == 1.0)
    }

    /// Every monitor's logical rectangle, in the map's order.
    pub fn logical_rects(&self) -> Vec<Rect> {
        self.screens.iter().map(ScaledScreen::logical).collect()
    }

    /// Bounding box of the logical rectangles (`None` for an empty map).
    pub fn logical_bounds(&self) -> Option<Rect> {
        self.screens.iter().map(ScaledScreen::logical).reduce(|a, b| a.united(&b))
    }

    /// The monitor a physical point is on, else the nearest one.
    pub fn screen_at_physical(&self, x: f64, y: f64) -> Option<&ScaledScreen> {
        self.nearest(x, y, |s| s.rect.to_rect())
    }

    /// The monitor whose logical rectangle holds a logical point, else the
    /// nearest one.
    pub fn screen_at_logical(&self, x: f64, y: f64) -> Option<&ScaledScreen> {
        self.nearest(x, y, ScaledScreen::logical)
    }

    fn nearest(&self, x: f64, y: f64, rect: impl Fn(&ScaledScreen) -> Rect) -> Option<&ScaledScreen> {
        if let Some(s) = self.screens.iter().find(|s| rect(s).contains(x, y)) {
            return Some(s);
        }
        self.screens.iter().min_by(|a, b| distance2(&rect(a), x, y).total_cmp(&distance2(&rect(b), x, y)))
    }

    /// A physical point to logical, on the monitor it is on (the nearest one
    /// for a point off every screen).
    pub fn to_logical(&self, x: f64, y: f64) -> (f64, f64) {
        self.screen_at_physical(x, y).map_or((x, y), |s| s.to_logical(x, y))
    }

    /// A logical point to physical, on the monitor whose logical rectangle
    /// holds it (the nearest one for a point in a gap between them).
    pub fn to_physical(&self, x: f64, y: f64) -> (f64, f64) {
        self.screen_at_logical(x, y).map_or((x, y), |s| s.to_physical(x, y))
    }

    /// The device pixel ratio under a physical point (1.0 for an empty map).
    pub fn scale_at_physical(&self, x: f64, y: f64) -> f64 {
        self.screen_at_physical(x, y).map_or(1.0, |s| s.scale)
    }

    /// A physical rectangle (a window) to logical, whole, on the monitor
    /// that holds most of it (`MonitorFromRect`; the nearest to its centre
    /// when it is on none). Converting the corners one by one, as Python
    /// did, tears a window that crosses monitors of different scales apart.
    pub fn rect_to_logical(&self, r: Rect) -> Rect {
        let area = |s: &ScaledScreen| s.rect.to_rect().intersection(&r).map_or(0.0, |i| i.w * i.h);
        let mut best: Option<(&ScaledScreen, f64)> = None;
        for s in &self.screens {
            let a = area(s);
            if a > 0.0 && best.is_none_or(|(_, b)| a > b) {
                best = Some((s, a));
            }
        }
        let (cx, cy) = r.center();
        let Some(s) = best.map(|(s, _)| s).or_else(|| self.screen_at_physical(cx, cy)) else { return r };
        if s.scale == 1.0 {
            return r;
        }
        let (x, y) = s.to_logical(r.x, r.y);
        Rect::new(x, y, r.w / s.scale, r.h / s.scale)
    }
}

/// The overlay's rectangle (physical, the bounding box of every monitor)
/// and the monitors in overlay-local coordinates, read now.
pub fn desktop_dpi_map() -> (IRect, DpiMap) {
    let list = monitors();
    let rect = virtual_screen_geometry_of(&list);
    (rect, DpiMap::from_monitors(&list).translated(-rect.x, -rect.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATIOS: [f64; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 3.0];

    #[test]
    fn logical_to_physical_then_back_round_trips() {
        for ratio in RATIOS {
            for (x, y) in [(0.0, 0.0), (123.4, -56.7), (1919.0, 1079.0), (-2560.0, 1440.0)] {
                let (px, py) = logical_to_physical(x, y, ratio);
                let (lx, ly) = physical_to_logical(px, py, ratio);
                assert!((lx - x).abs() < 1e-9 && (ly - y).abs() < 1e-9, "{ratio} {x} {y}");
            }
        }
    }

    #[test]
    fn physical_to_logical_then_back_round_trips() {
        for ratio in RATIOS {
            let (lx, ly) = physical_to_logical(3000.0, -15.0, ratio);
            let (px, py) = logical_to_physical(lx, ly, ratio);
            assert!((px - 3000.0).abs() < 1e-9 && (py + 15.0).abs() < 1e-9);
        }
    }

    #[test]
    fn ratio_above_one_makes_physical_the_larger_number() {
        for ratio in RATIOS.into_iter().filter(|r| *r > 1.0) {
            let (px, py) = logical_to_physical(100.0, 50.0, ratio);
            assert!(px > 100.0 && py > 50.0);
        }
    }

    #[test]
    fn non_positive_ratio_degrades_to_identity() {
        for bad in [0.0, -1.0, -2.5, f64::NAN] {
            assert_eq!(logical_to_physical(10.0, 20.0, bad), (10.0, 20.0));
            assert_eq!(physical_to_logical(10.0, 20.0, bad), (10.0, 20.0));
        }
    }

    #[test]
    fn ratio_lookup_has_a_safe_fallback() {
        let r = screen_device_pixel_ratio_at(-1.0e7, -1.0e7);
        assert!(r > 0.0);
        assert_eq!(ratio_for_dpi(0), 1.0);
        assert_eq!(ratio_for_dpi(144), 1.5);
    }

    /// 2560x1440 at 100% on the left (main), 3840x2160 at 150% on the right.
    fn mixed() -> DpiMap {
        DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 2560, 1440), 1.0),
            ScaledScreen::new(IRect::new(2560, 0, 3840, 2160), 1.5),
        ])
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_scaled_monitor_keeps_its_origin_and_shrinks() {
        let map = mixed();
        assert!(!map.is_identity());
        assert_eq!(
            map.logical_rects(),
            vec![Rect::new(0.0, 0.0, 2560.0, 1440.0), Rect::new(2560.0, 0.0, 2560.0, 1440.0)]
        );
        assert_eq!(map.logical_bounds(), Some(Rect::new(0.0, 0.0, 5120.0, 1440.0)));
        // The 100% monitor converts nothing; the 4K one about its own origin.
        assert_eq!(map.to_logical(1000.0, 700.0), (1000.0, 700.0));
        assert!(close(map.to_logical(2560.0 + 1500.0, 900.0), (2560.0 + 1000.0, 600.0)));
        assert!(close(map.to_logical(6399.0, 2159.0), (2560.0 + 3839.0 / 1.5, 2159.0 / 1.5)));
        assert!(close(map.to_physical(3560.0, 600.0), (4060.0, 900.0)));
        assert_eq!(map.scale_at_physical(2559.0, 0.0), 1.0);
        assert_eq!(map.scale_at_physical(2560.0, 0.0), 1.5);
    }

    #[test]
    fn points_round_trip_on_every_monitor() {
        let maps = [
            mixed(),
            // A 150% monitor left of and above the main one: negative origins.
            DpiMap::new([
                ScaledScreen::new(IRect::new(0, 0, 1920, 1080), 1.0),
                ScaledScreen::new(IRect::new(-3840, -400, 3840, 2160), 1.5),
                ScaledScreen::new(IRect::new(1920, -1440, 2560, 1440), 1.25),
            ]),
        ];
        for map in &maps {
            for s in map.screens() {
                let r = s.rect;
                for (fx, fy) in [(0.0, 0.0), (0.25, 0.5), (0.999, 0.999)] {
                    let p = (r.x as f64 + r.w as f64 * fx, r.y as f64 + r.h as f64 * fy);
                    let l = map.to_logical(p.0, p.1);
                    assert!(s.logical().contains(l.0, l.1), "{p:?} -> {l:?} not on {:?}", s.logical());
                    assert!(close(map.to_physical(l.0, l.1), p), "{p:?}");
                }
            }
        }
    }

    #[test]
    fn negative_origins_scale_about_the_monitors_own_corner() {
        let map = DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 1920, 1080), 1.0),
            ScaledScreen::new(IRect::new(-3840, -400, 3840, 2160), 1.5),
        ]);
        assert_eq!(map.logical_rects()[1], Rect::new(-3840.0, -400.0, 2560.0, 1440.0));
        // The gap between the shrunken left monitor and the main one.
        assert_eq!(map.logical_bounds(), Some(Rect::new(-3840.0, -400.0, 5760.0, 1480.0)));
        assert!(close(map.to_logical(-1.0, 0.0), (-3840.0 + 3839.0 / 1.5, -400.0 + 400.0 / 1.5)));
        // A logical point in the gap goes to the nearest monitor (the left
        // one, 100 px away) instead of jumping somewhere arbitrary.
        let (px, _) = map.to_physical(-1180.0, 0.0);
        assert!(close((px, 0.0), (-3840.0 + 2660.0 * 1.5, 0.0)), "{px}");
        // Overlay-local: the same layout moved so the bounding box starts at 0.
        let local = map.translated(3840, 400);
        assert_eq!(local.logical_bounds(), Some(Rect::new(0.0, 0.0, 5760.0, 1480.0)));
        assert!(close(local.to_logical(3840.0 + 100.0, 400.0 + 50.0), (3840.0 + 100.0, 400.0 + 50.0)));
        assert!(close(local.to_logical(1500.0, 900.0), (1000.0, 600.0)));
    }

    #[test]
    fn windows_convert_whole_on_the_monitor_holding_most_of_them() {
        let map = mixed();
        // Maximised on the 4K monitor: exactly its logical rectangle (Python's
        // corner-by-corner conversion missed the right and bottom edges).
        let r = map.rect_to_logical(Rect::new(2560.0, 0.0, 3840.0, 2160.0));
        assert_eq!(r, Rect::new(2560.0, 0.0, 2560.0, 1440.0));
        // Mostly on the 100% monitor: left alone.
        let r = map.rect_to_logical(Rect::new(2000.0, 100.0, 800.0, 300.0));
        assert_eq!(r, Rect::new(2000.0, 100.0, 800.0, 300.0));
        // Mostly on the 4K one: scaled about its origin, not torn.
        let r = map.rect_to_logical(Rect::new(2500.0, 300.0, 900.0, 600.0));
        assert!(close((r.x, r.y), (2560.0 - 60.0 / 1.5, 200.0)) && close((r.w, r.h), (600.0, 400.0)), "{r:?}");
        // Off every screen: the nearest one.
        let r = map.rect_to_logical(Rect::new(7000.0, 0.0, 300.0, 150.0));
        assert!(close((r.w, r.h), (200.0, 100.0)), "{r:?}");
    }

    #[test]
    fn at_one_hundred_percent_nothing_changes() {
        let map = DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 2560, 1440), 1.0),
            ScaledScreen::new(IRect::new(2560, 0, 3840, 2160), 1.0),
            ScaledScreen::new(IRect::new(-1920, -300, 1920, 1080), 1.0),
        ])
        .translated(1920, 300);
        assert!(map.is_identity());
        assert!(DpiMap::default().is_identity());
        // Bit for bit, fractional and off-screen points included.
        for p in [(0.0, 0.0), (123.456, 789.012), (-5.5, -7.25), (99999.9, 3.3), (4479.999, 1439.5)] {
            assert_eq!(map.to_logical(p.0, p.1), p);
            assert_eq!(map.to_physical(p.0, p.1), p);
            assert_eq!(DpiMap::default().to_logical(p.0, p.1), p);
        }
        let w = Rect::new(2400.3, 10.0, 640.7, 480.0);
        assert_eq!(map.rect_to_logical(w), w);
        let physical: Vec<Rect> = map.screens().iter().map(|s| s.rect.to_rect()).collect();
        assert_eq!(map.logical_rects(), physical);
        // A bad ratio reads as 100%.
        assert!(DpiMap::new([ScaledScreen { rect: IRect::new(0, 0, 10, 10), scale: 0.0 }]).is_identity());
    }

    #[test]
    fn this_desktop_maps_onto_itself() {
        let (rect, map) = desktop_dpi_map();
        if map.screens().is_empty() {
            return;
        }
        let b = map.logical_bounds().unwrap();
        assert_eq!((b.x, b.y), (0.0, 0.0), "the logical box shares the overlay's corner");
        assert!(b.w <= rect.w as f64 && b.h <= rect.h as f64);
    }
}
