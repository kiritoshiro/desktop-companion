//! Monitor geometry: the window parts of app/engine.py
//! (`virtual_screen_geometry`, `screen_rects_local`, `available_rects_local`)
//! on `EnumDisplayMonitors` instead of `QGuiApplication.screens()`.
//!
//! Coordinates: the Rust overlay is per-monitor-DPI aware (winit sets
//! PER_MONITOR_AWARE_V2), so everything here is in **physical** pixels, the
//! space every raw Win32 call already uses. Qt's logical space is recovered
//! with [`MonitorInfo::logical_rect`] where a caller wants it.

use dc_core::{IRect, Rect};
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW,
    MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, MONITORINFOF_PRIMARY, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};
use windows::core::BOOL;

use crate::win::from_wide;

/// One monitor as Windows reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    /// `HMONITOR` as an integer (valid until the display layout changes).
    pub handle: isize,
    /// Device name, `\\.\DISPLAY1`: the same string `QScreen.name()` gave.
    pub name: String,
    /// Whole monitor, physical pixels, virtual-desktop coordinates.
    pub rect: IRect,
    /// Work area (without the taskbar), physical pixels.
    pub work: IRect,
    /// Effective DPI (96 = 100%).
    pub dpi: u32,
    pub primary: bool,
}

impl MonitorInfo {
    /// Device pixel ratio, as `QScreen.devicePixelRatio()`.
    pub fn scale(&self) -> f64 {
        if self.dpi == 0 { 1.0 } else { self.dpi as f64 / 96.0 }
    }

    /// The monitor in Qt's logical space: same origin, size divided by the
    /// ratio (how Qt 5 lays out a scaled screen).
    pub fn logical_rect(&self) -> Rect {
        let s = self.scale();
        Rect::new(self.rect.x as f64, self.rect.y as f64, self.rect.w as f64 / s, self.rect.h as f64 / s)
    }
}

fn irect(r: &RECT) -> IRect {
    IRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

fn monitor_info(hmonitor: HMONITOR) -> Option<MonitorInfo> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if !ok.as_bool() {
        return None;
    }
    let (mut dx, mut dy) = (96u32, 96u32);
    if unsafe { GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) }.is_err() {
        dx = 96;
    }
    Some(MonitorInfo {
        handle: hmonitor.0 as isize,
        name: from_wide(&info.szDevice),
        rect: irect(&info.monitorInfo.rcMonitor),
        work: irect(&info.monitorInfo.rcWork),
        dpi: dx,
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

unsafe extern "system" fn collect_monitor(hmonitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, data: LPARAM) -> BOOL {
    let out = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };
    if let Some(info) = monitor_info(hmonitor) {
        out.push(info);
    }
    BOOL(1)
}

/// Every monitor in the desktop layout, in `EnumDisplayMonitors` order.
pub fn monitors() -> Vec<MonitorInfo> {
    let mut out: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Index of the primary monitor in `list` (0 when none is flagged).
pub fn primary_index(list: &[MonitorInfo]) -> usize {
    list.iter().position(|m| m.primary).unwrap_or(0)
}

/// The monitor nearest to a physical point.
pub fn monitor_at(x: i32, y: i32) -> Option<MonitorInfo> {
    let hmonitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    if hmonitor.0.is_null() { None } else { monitor_info(hmonitor) }
}

/// Union of every monitor's rectangle, physical pixels. Falls back to
/// 1280x720 at the origin when nothing is reported, as the Python did.
pub fn virtual_screen_geometry() -> IRect {
    let list = monitors();
    if let Some(rect) = union_of(list.iter().map(|m| m.rect)) {
        return rect;
    }
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if w > 0 && h > 0 { IRect::new(x, y, w, h) } else { IRect::new(0, 0, 1280, 720) }
}

/// Bounding box of rectangles (`QRect.united` folded).
pub fn union_of(rects: impl IntoIterator<Item = IRect>) -> Option<IRect> {
    let mut it = rects.into_iter();
    let first = it.next()?;
    let (mut l, mut t, mut r, mut b) = (first.x, first.y, first.right(), first.bottom());
    for rc in it {
        l = l.min(rc.x);
        t = t.min(rc.y);
        r = r.max(rc.right());
        b = b.max(rc.bottom());
    }
    Some(IRect::new(l, t, r - l, b - t))
}

/// Every monitor as an overlay-local rectangle (DC-65): the overlay spans the
/// bounding box, which on a mixed layout contains regions no screen shows.
pub fn screen_rects_local(origin: (i32, i32), list: &[MonitorInfo]) -> Vec<Rect> {
    list.iter()
        .map(|m| {
            Rect::new((m.rect.x - origin.0) as f64, (m.rect.y - origin.1) as f64, m.rect.w as f64, m.rect.h as f64)
        })
        .collect()
}

/// Every monitor's usable area (without the taskbar), overlay-local, and the
/// index of the main one.
pub fn available_rects_local(origin: (i32, i32), list: &[MonitorInfo]) -> (Vec<Rect>, usize) {
    let rects = list
        .iter()
        .map(|m| {
            Rect::new((m.work.x - origin.0) as f64, (m.work.y - origin.1) as f64, m.work.w as f64, m.work.h as f64)
        })
        .collect();
    (rects, primary_index(list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_is_the_bounding_box() {
        let u = union_of([IRect::new(0, 0, 2560, 1440), IRect::new(2560, 360, 1920, 1080)]).unwrap();
        assert_eq!(u, IRect::new(0, 0, 4480, 1440));
        let u = union_of([IRect::new(-1920, 0, 1920, 1080), IRect::new(0, 0, 1920, 1080)]).unwrap();
        assert_eq!(u, IRect::new(-1920, 0, 3840, 1080));
        assert!(union_of([]).is_none());
    }

    #[test]
    fn local_rects_subtract_the_origin() {
        let m = MonitorInfo {
            handle: 0,
            name: "\\\\.\\DISPLAY2".into(),
            rect: IRect::new(-1920, 100, 1920, 1080),
            work: IRect::new(-1920, 100, 1920, 1040),
            dpi: 144,
            primary: false,
        };
        let rects = screen_rects_local((-1920, 0), std::slice::from_ref(&m));
        assert_eq!(rects[0], Rect::new(0.0, 100.0, 1920.0, 1080.0));
        let (avail, primary) = available_rects_local((-1920, 0), std::slice::from_ref(&m));
        assert_eq!(avail[0].h, 1040.0);
        assert_eq!(primary, 0);
        assert!((m.scale() - 1.5).abs() < 1e-12);
        assert_eq!(m.logical_rect(), Rect::new(-1920.0, 100.0, 1280.0, 720.0));
    }

    #[test]
    fn smoke_enumerates_monitors() {
        let list = monitors();
        assert!(!list.is_empty(), "no monitors reported");
        assert!(list.iter().any(|m| m.primary), "no primary monitor");
        for m in &list {
            assert!(m.rect.w > 0 && m.rect.h > 0);
            assert!(m.name.starts_with("\\\\.\\"), "{}", m.name);
        }
        let v = virtual_screen_geometry();
        for m in &list {
            assert!(v.x <= m.rect.x && v.right() >= m.rect.right());
        }
        assert!(monitor_at(list[0].rect.x + 1, list[0].rect.y + 1).is_some());
    }
}
