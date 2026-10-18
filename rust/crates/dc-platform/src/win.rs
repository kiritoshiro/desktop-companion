//! Small Win32 helpers shared by the platform modules (no Python counterpart:
//! these replace the `ctypes` boilerplate repeated in every Python module).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    SetWindowLongPtrW,
};
use windows::core::PCWSTR;

/// A NUL-terminated UTF-16 copy of `s`.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A `PCWSTR` view of a buffer made by [`wide`]; the buffer must outlive it.
pub fn pcwstr(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}

/// The text before the first NUL of a fixed UTF-16 buffer.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// `HWND` from the integer handle other crates carry around.
pub fn hwnd_from_raw(raw: isize) -> HWND {
    HWND(raw as *mut core::ffi::c_void)
}

/// The integer form of an `HWND`.
pub fn hwnd_to_raw(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

/// `GetWindowLongPtrW(hwnd, GWL_EXSTYLE)`.
pub fn ex_style(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

/// `SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style)`.
pub fn set_ex_style(hwnd: HWND, style: u32) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
    }
}

/// The window's class name, or "" (Python `_class_name`).
pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 { String::new() } else { String::from_utf16_lossy(&buf[..n as usize]) }
}

/// The window's caption, or "" (Python `_window_text`).
pub fn window_text(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; length as usize + 1];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 { String::new() } else { String::from_utf16_lossy(&buf[..n as usize]) }
}

/// True for hidden/virtualised DWM windows that still enumerate as visible.
pub fn is_dwm_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    ok.is_ok() && cloaked != 0
}

/// The DWM extended frame bounds (the visible frame without the drop
/// shadow), or `None` on failure.
pub fn rect_from_dwm(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut core::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    ok.ok().map(|_| rect)
}

/// `GetWindowRect`, or `None` on failure.
pub fn rect_from_user32(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok().map(|_| rect)
}

/// Milliseconds on a monotonic clock, for the timers the Python ran on
/// `QElapsedTimer`.
pub fn tick_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}
