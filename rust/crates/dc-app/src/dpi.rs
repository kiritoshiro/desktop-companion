//! The overlay's high-DPI scaling: what `enable_high_dpi_scaling` (Qt's
//! `AA_EnableHighDpiScaling`) and the `support/dpi.py` crossings in
//! app/engine.py give the Python overlay.
//!
//! The engine works in **logical** pixels, as Qt does: the manager's size
//! and screen rectangles, every creature, the cursor, the hit test, the
//! desktop windows, the Adventure screens and the Reclaim picture's
//! rectangles. On a monitor at 150% a spider, its label and bars, and the
//! Adventure HUD are therefore half as big again in physical pixels, as
//! they would be in a Qt window on that monitor. The window, its GPU
//! surface and every Win32 call stay **physical**. The layout is Qt 5's
//! (see `dc_platform::dpi`): each monitor keeps its physical corner and
//! shrinks by its scale, and a point converts on the monitor it is on.
//!
//! The scene is recorded once in logical pixels and drawn once per monitor
//! ([`physical_scene`]), scaled about that monitor's corner and clipped to
//! it, so a monitor's physical pixels show exactly its logical rectangle.
//! The Reclaim capture keeps its native pixels, drawn into the monitor's
//! logical rectangle, so it comes out 1:1 again on screen.
//!
//! Rust difference: a Qt 5 window has one device pixel ratio, its screen's,
//! so Python's single overlay window spanning a 100% and a 150% monitor
//! drew everything at one of the two scales and covered the other monitor
//! only partly (and converted the cursor trap and the window snapshot with
//! that single ratio about (0, 0)). This draws each monitor at its own scale,
//! the behaviour the Python's comments describe.
//!
//! With every monitor at 100% the map is the identity and nothing here
//! converts anything: the overlay is pixel-identical to the physical one.

use dc_core::{IRect, Rect};
use dc_manager::DesktopSurface;
use dc_platform::dpi::{DpiMap, ScaledScreen};
use dc_render::{Scene, Transform};
use dc_world::playfield::ScreenRect;

/// The manager's size: the logical bounding box of the monitors (the
/// window's own size at 100%).
pub fn logical_size(map: &DpiMap, window: IRect) -> (f64, f64) {
    match map.logical_bounds() {
        Some(b) if !map.is_identity() => (b.right(), b.bottom()),
        _ => (window.w as f64, window.h as f64),
    }
}

/// Every monitor as an overlay-local logical rectangle (`screen_rects_local`).
pub fn screen_rects(map: &DpiMap) -> Vec<ScreenRect> {
    map.logical_rects().into_iter().map(|r| ScreenRect::new(r.x, r.y, r.w, r.h)).collect()
}

/// An overlay-local physical rectangle (a monitor's work area) in logical
/// pixels, rounded like a `QRect`.
pub fn irect_to_logical(map: &DpiMap, r: IRect) -> IRect {
    if map.is_identity() {
        return r;
    }
    let l = map.rect_to_logical(r.to_rect());
    IRect::new(l.x.round() as i32, l.y.round() as i32, l.w.round() as i32, l.h.round() as i32)
}

/// Desktop windows from dc-platform's snapshot (overlay-local physical) in
/// logical pixels, each whole on the monitor holding most of it. Python
/// divided every rectangle by one ratio, that of the overlay's corner.
pub fn surfaces_to_logical(map: &DpiMap, surfaces: &mut [DesktopSurface]) {
    if map.is_identity() {
        return;
    }
    for s in surfaces {
        let r = map.rect_to_logical(Rect::new(s.x, s.y, s.w, s.h));
        (s.x, s.y, s.w, s.h) = (r.x, r.y, r.w, r.h);
    }
}

/// From a monitor's logical pixels to its physical ones (overlay-local):
/// its scale about its corner, which both spaces share.
pub fn screen_transform(s: &ScaledScreen) -> Transform {
    let (ox, oy) = (s.rect.x as f64, s.rect.y as f64);
    Transform::new(s.scale, 0.0, 0.0, s.scale, ox * (1.0 - s.scale), oy * (1.0 - s.scale))
}

/// The logical scene as the window's physical pixels: one copy per monitor
/// through [`screen_transform`], clipped to that monitor. `None` when the
/// map is the identity (draw the scene itself).
pub fn physical_scene(map: &DpiMap, logical: &Scene) -> Option<Scene> {
    if map.is_identity() {
        return None;
    }
    let mut out = Scene::new();
    for s in map.screens() {
        out.append_transformed(logical, &screen_transform(s), Some(s.rect.to_rect()));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_render::{Color, CpuRenderer};

    /// 2560x1440 at 100% + 3840x2160 at 150%, overlay-local.
    fn mixed() -> DpiMap {
        DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 2560, 1440), 1.0),
            ScaledScreen::new(IRect::new(2560, 0, 3840, 2160), 1.5),
        ])
    }

    /// A 150% monitor up and to the left of a 100% main one, made
    /// overlay-local (the desktop origin was (-3840, -400)).
    fn negative() -> DpiMap {
        DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 1920, 1080), 1.0),
            ScaledScreen::new(IRect::new(-3840, -400, 3840, 2160), 1.5),
        ])
        .translated(3840, 400)
    }

    #[test]
    fn the_manager_sees_the_logical_desktop() {
        let window = IRect::new(0, 0, 6400, 2160);
        assert_eq!(logical_size(&mixed(), window), (5120.0, 1440.0));
        let rects = screen_rects(&mixed());
        assert_eq!((rects[1].x, rects[1].y, rects[1].w, rects[1].h), (2560.0, 0.0, 2560.0, 1440.0));
        assert_eq!(logical_size(&negative(), IRect::new(-3840, -400, 5760, 2160)), (5760.0, 1480.0));
        // A work area without the taskbar, on the 150% monitor.
        assert_eq!(irect_to_logical(&mixed(), IRect::new(2560, 0, 3840, 2088)), IRect::new(2560, 0, 2560, 1392));
    }

    #[test]
    fn desktop_windows_shrink_on_the_scaled_monitor_only() {
        let surface = |x, y, w, h| DesktopSurface { kind: "window".into(), class_name: "X".into(), x, y, w, h };
        let mut list = vec![surface(100.0, 100.0, 800.0, 600.0), surface(2860.0, 300.0, 1500.0, 900.0)];
        surfaces_to_logical(&mixed(), &mut list);
        assert_eq!((list[0].x, list[0].y, list[0].w, list[0].h), (100.0, 100.0, 800.0, 600.0));
        assert_eq!((list[1].x, list[1].y, list[1].w, list[1].h), (2760.0, 200.0, 1000.0, 600.0));
    }

    #[test]
    fn each_monitor_shows_its_logical_rectangle() {
        for map in [mixed(), negative()] {
            for s in map.screens() {
                let t = screen_transform(s);
                let l = s.logical();
                let p = t.map_rect(l);
                let r = s.rect.to_rect();
                for (a, b) in [(p.x, r.x), (p.y, r.y), (p.w, r.w), (p.h, r.h)] {
                    assert!((a - b).abs() < 1e-9, "{p:?} vs {r:?}");
                }
                // The transform agrees with the point conversion.
                let (lx, ly) = (l.x + l.w * 0.3, l.y + l.h * 0.7);
                let (px, py) = t.map(lx, ly);
                let (bx, by) = map.to_physical(lx, ly);
                assert!((px - bx).abs() < 1e-9 && (py - by).abs() < 1e-9);
            }
        }
    }

    /// A logical scene drawn on a small mixed desktop: 40x20 at 100% beside
    /// 60x30 at 150%. A logical square on each monitor lands where the
    /// cursor conversion says it is, at that monitor's size.
    #[test]
    fn the_scene_is_drawn_per_monitor_at_its_scale() {
        let map = DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 40, 20), 1.0),
            ScaledScreen::new(IRect::new(40, 0, 60, 30), 1.5),
        ]);
        assert_eq!(logical_size(&map, IRect::new(0, 0, 100, 30)), (80.0, 20.0));
        let mut logical = Scene::new();
        {
            let mut c = logical.canvas();
            c.fill_rect(Rect::new(10.0, 4.0, 8.0, 8.0), Color::rgb(255, 0, 0));
            c.fill_rect(Rect::new(50.0, 4.0, 8.0, 8.0), Color::rgb(0, 0, 255));
            // Straddles the border: each half drawn at its monitor's scale.
            c.fill_rect(Rect::new(36.0, 16.0, 8.0, 2.0), Color::rgb(0, 255, 0));
        }
        let scene = physical_scene(&map, &logical).unwrap();
        let img = CpuRenderer::new().render(&scene, 100, 30, Some(Color::TRANSPARENT));
        let red = |x, y| img.pixel(x, y).r == 255 && img.pixel(x, y).a == 255;
        let blue = |x, y| img.pixel(x, y).b == 255 && img.pixel(x, y).a == 255;
        // 100%: 8x8 at (10, 4).
        assert!(red(10, 4) && red(17, 11) && !red(18, 11) && !red(9, 4));
        // 150%: logical (50, 4) is physical (55, 6), and 12x12.
        assert_eq!(map.to_physical(50.0, 4.0), (55.0, 6.0));
        assert!(blue(55, 6) && blue(66, 17) && !blue(67, 17) && !blue(54, 6) && !blue(55, 5));
        // Physical (55, 6) reads back as the square's corner.
        assert_eq!(map.to_logical(55.0, 6.0), (50.0, 4.0));
        // The straddling bar: 4 px on the left monitor, 6 px (4 * 1.5) on the right.
        let green = |x, y| img.pixel(x, y).g == 255;
        assert!(green(36, 16) && green(39, 16) && green(40, 24) && green(45, 25) && !green(46, 25));
        // Nothing beyond the 100% monitor's bottom edge (the logical gap
        // below it is not on screen) and nothing outside either monitor.
        assert_eq!(img.pixel(39, 20).a, 0);
    }

    /// At 100% everywhere the engine draws the logical scene as it is.
    #[test]
    fn at_one_hundred_percent_nothing_is_converted() {
        let map = DpiMap::new([
            ScaledScreen::new(IRect::new(0, 0, 2560, 1440), 1.0),
            ScaledScreen::new(IRect::new(2560, 0, 3840, 2160), 1.0),
        ]);
        let mut scene = Scene::new();
        scene.canvas().fill_rect(Rect::new(1.0, 2.0, 3.0, 4.0), Color::rgb(1, 2, 3));
        assert!(physical_scene(&map, &scene).is_none());
        assert!(physical_scene(&DpiMap::default(), &scene).is_none());
        let window = IRect::new(0, 0, 6400, 2160);
        assert_eq!(logical_size(&map, window), (6400.0, 2160.0));
        assert_eq!(logical_size(&DpiMap::default(), window), (6400.0, 2160.0));
        let surface = DesktopSurface { kind: "window".into(), class_name: "X".into(), x: 2.5, y: 3.5, w: 7.25, h: 9.0 };
        let mut list = vec![surface.clone()];
        surfaces_to_logical(&map, &mut list);
        assert_eq!(list[0], surface);
        assert_eq!(irect_to_logical(&map, IRect::new(2560, 0, 3840, 2088)), IRect::new(2560, 0, 3840, 2088));
        let rects = screen_rects(&map);
        assert_eq!((rects[1].x, rects[1].w, rects[1].h), (2560.0, 3840.0, 2160.0));
    }
}
