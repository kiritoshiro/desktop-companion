//! Global input state and process identity: the `GetAsyncKeyState` polling
//! of app/engine.py (`left_mouse_button_down`), `QCursor.pos()`, the
//! release-hotkey key mapping (`virtual_key_for`) and
//! `SetCurrentProcessExplicitAppUserModelID` from app/wood_theme.py.
//!
//! All positions are physical pixels in virtual-desktop coordinates.

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::win::{pcwstr, wide};

pub const VK_LBUTTON: i32 = 0x01;
pub const VK_RBUTTON: i32 = 0x02;
pub const VK_MBUTTON: i32 = 0x04;
/// The key under Esc: ` and ~ on a US layout.
pub const VK_OEM_3: u32 = 0xC0;
/// The process's own app id, so the taskbar uses its icon (wood_theme.py).
pub const APP_USER_MODEL_ID: &str = "DesktopBugCompanion.App";

/// The OS pointer position, or `None` if Windows will not say (secure
/// desktop, locked session).
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

/// Whether a virtual key or mouse button is held right now, wherever the
/// focus is (`GetAsyncKeyState(vk) & 0x8000`).
pub fn key_down(vk: i32) -> bool {
    (unsafe { GetAsyncKeyState(vk) } as u16) & 0x8000 != 0
}

/// The global left-button state, even while the overlay is click-through.
pub fn left_mouse_button_down() -> bool {
    key_down(VK_LBUTTON)
}

pub fn right_mouse_button_down() -> bool {
    key_down(VK_RBUTTON)
}

/// The Windows virtual-key code for a binding name, for `RegisterHotKey`:
/// F1-F24, letters, digits and the ` key. `None` for anything else.
pub fn virtual_key_for(name: &str) -> Option<u32> {
    let upper = name.to_uppercase();
    if let Some(rest) = upper.strip_prefix('F') {
        // Python's str.isdigit(): non-empty, all digits.
        if !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = rest.parse::<u32>()
            && (1..=24).contains(&n)
        {
            return Some(0x6F + n);
        }
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        // Python: `name.isalnum()` then `ord(name.upper())`, Unicode included.
        if c.is_alphanumeric() {
            let mut up = c.to_uppercase();
            if let (Some(u), None) = (up.next(), up.next()) {
                return Some(u as u32);
            }
        }
    }
    if name == "`" || name == "~" {
        return Some(VK_OEM_3);
    }
    None
}

/// Give the process its own taskbar identity (otherwise a source run shows
/// the host executable's icon). Returns whether Windows accepted it.
pub fn set_app_user_model_id(id: &str) -> bool {
    let w = wide(id);
    unsafe { SetCurrentProcessExplicitAppUserModelID(pcwstr(&w)) }.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_key_maps_like_python() {
        assert_eq!(virtual_key_for("F8"), Some(0x77));
        assert_eq!(virtual_key_for("F1"), Some(0x70));
        assert_eq!(virtual_key_for("F24"), Some(0x87));
        assert_eq!(virtual_key_for("f12"), Some(0x7B));
        assert_eq!(virtual_key_for("F25"), None);
        assert_eq!(virtual_key_for("F0"), None);
        assert_eq!(virtual_key_for("P"), Some('P' as u32));
        assert_eq!(virtual_key_for("p"), Some('P' as u32));
        assert_eq!(virtual_key_for("7"), Some('7' as u32));
        assert_eq!(virtual_key_for("`"), Some(VK_OEM_3));
        assert_eq!(virtual_key_for("~"), Some(VK_OEM_3));
        // A single "F" is a letter, not a function key.
        assert_eq!(virtual_key_for("F"), Some('F' as u32));
        assert_eq!(virtual_key_for("Mouse Left"), None);
        assert_eq!(virtual_key_for(""), None);
        assert_eq!(virtual_key_for("é"), Some(0xC9));
        assert_eq!(virtual_key_for("!"), None);
    }

    #[test]
    fn global_state_reads_without_moving_anything() {
        // Reading only; never SetCursorPos in tests.
        let _ = cursor_pos();
        let _ = left_mouse_button_down();
        let _ = key_down(VK_MBUTTON);
    }
}
