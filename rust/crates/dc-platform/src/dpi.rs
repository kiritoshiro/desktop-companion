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

use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

use crate::monitors::monitor_at;

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
}
