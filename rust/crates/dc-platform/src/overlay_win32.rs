//! Port of app/overlay_win32.py: the layered/topmost styles of the overlay
//! window, its whole-window input transparency (DC-79) and `SetCursorPos`.
//!
//! The Python applied these to a `QWidget` and read `widget.mode` /
//! `widget._input_transparent` off it; here the caller passes the two facts
//! explicitly, and [`crate::overlay::OverlayWindow`] keeps them.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursorPos, SetWindowPos,
};

use crate::win::{ex_style, set_ex_style};

pub const GWL_EXSTYLE: i32 = -20;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;

/// Whether this build can talk to Win32 at all (`is_windows()`).
pub fn is_windows() -> bool {
    cfg!(windows)
}

/// The extended style the overlay should have, given the current style.
///
/// Pure so the bit logic is testable: layered and tool window always, the
/// taskbar's app-window bit never, `WS_EX_TRANSPARENT` exactly when the
/// window should let every mouse event through, and `WS_EX_NOACTIVATE` when
/// asked (the Rust port's way of honouring `WA_ShowWithoutActivating` for
/// clicks as well as for the first show).
pub fn overlay_ex_style(style: u32, input_transparent: bool, no_activate: bool) -> u32 {
    let mut style = (style | WS_EX_LAYERED | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
    if input_transparent {
        style |= WS_EX_TRANSPARENT;
    } else {
        style &= !WS_EX_TRANSPARENT;
    }
    if no_activate {
        style |= WS_EX_NOACTIVATE;
    } else {
        style &= !WS_EX_NOACTIVATE;
    }
    style
}

/// Apply layered/topmost styles to the overlay window.
///
/// The historical function name is kept. `input_transparent` is the DC-79
/// flag the engine maintains from whether the cursor is over something
/// interactive; `adventure` selects `HWND_NOTOPMOST` as the Python did for
/// `mode == "adventure"`. Returns whether the styles were written.
pub fn apply_click_through(hwnd: HWND, input_transparent: bool, adventure: bool, no_activate: bool) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let style = overlay_ex_style(ex_style(hwnd), input_transparent, no_activate);
    set_ex_style(hwnd, style);
    // Keep the window available above normal apps without activating it.
    let insert_after = if adventure { HWND_NOTOPMOST } else { HWND_TOPMOST };
    let ok = unsafe {
        SetWindowPos(hwnd, Some(insert_after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW)
    };
    if let Err(err) = ok {
        log::warn!(target: "overlay", "Failed to apply Windows overlay styles: {err}");
        return false;
    }
    true
}

/// Let every mouse event fall through the whole window, or none (DC-79).
///
/// Cheap enough to call on change only: one Get/SetWindowLong pair. Returns
/// whether the style was written.
pub fn set_input_transparent(hwnd: HWND, transparent: bool) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let style = ex_style(hwnd);
    let style = if transparent { style | WS_EX_TRANSPARENT } else { style & !WS_EX_TRANSPARENT };
    set_ex_style(hwnd, style);
    true
}

/// Move the OS pointer to global screen pixel `(x, y)` (physical pixels).
///
/// Used by the cursor-trapping silk. Returns true on success; a failure is
/// "the user escaped" to the trap logic.
pub fn set_cursor_pos(x: i32, y: i32) -> bool {
    match unsafe { SetCursorPos(x, y) } {
        Ok(()) => true,
        Err(err) => {
            log::debug!(target: "overlay", "SetCursorPos failed: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_always_layered_tool_window_never_app_window() {
        let s = overlay_ex_style(WS_EX_APPWINDOW, false, false);
        assert_eq!(s & WS_EX_LAYERED, WS_EX_LAYERED);
        assert_eq!(s & WS_EX_TOOLWINDOW, WS_EX_TOOLWINDOW);
        assert_eq!(s & WS_EX_APPWINDOW, 0);
        assert_eq!(s & WS_EX_TRANSPARENT, 0);
    }

    #[test]
    fn transparent_bit_follows_the_flag() {
        let on = overlay_ex_style(0, true, false);
        assert_ne!(on & WS_EX_TRANSPARENT, 0);
        let off = overlay_ex_style(on, false, false);
        assert_eq!(off & WS_EX_TRANSPARENT, 0);
        assert_ne!(overlay_ex_style(0, false, true) & WS_EX_NOACTIVATE, 0);
        assert_eq!(overlay_ex_style(WS_EX_NOACTIVATE, false, false) & WS_EX_NOACTIVATE, 0);
    }

    #[test]
    fn null_window_is_refused() {
        assert!(!apply_click_through(HWND::default(), true, false, true));
        assert!(!set_input_transparent(HWND::default(), true));
    }
}
