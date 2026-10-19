//! Keyboard and mouse input for Adventure, as `app/engine.py`'s
//! `keyPressEvent` / `keyReleaseEvent` / `mouse*Event` saw it.
//!
//! winit's events are turned into plain values here ([`KeyInput`],
//! [`MouseButton`]) so the game logic in `super` can be driven from tests
//! with synthetic keys and no window.

use dc_adventure::controls::{Key, MouseButton};
use winit::event::{ElementState, KeyEvent, MouseButton as WinitButton};
use winit::keyboard::{Key as WinitKey, KeyCode, KeyLocation, NamedKey, PhysicalKey};

/// One key press or release, reduced to what the Python read off a
/// `QKeyEvent`: the key (`event.key()`), whether it is the key under Esc
/// (`Key_AsciiTilde` or `nativeVirtualKey() == VK_OEM_3`), and auto-repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: Key,
    pub under_esc: bool,
    pub repeat: bool,
    pub pressed: bool,
}

#[cfg(test)]
impl KeyInput {
    /// A first press (no auto-repeat).
    pub fn press(key: Key) -> Self {
        Self { key, under_esc: key == Key::Char('~') || key == Key::Char('`'), repeat: false, pressed: true }
    }

    pub fn release(key: Key) -> Self {
        Self { pressed: false, ..Self::press(key) }
    }

    pub fn repeated(self) -> Self {
        Self { repeat: true, ..self }
    }
}

/// Qt's `event.key()` for a winit key: letters upper-case whatever the
/// Shift state, symbols as typed (Shift+1 is "!", as `Key_Exclam` was).
pub fn key_from_winit(logical: &WinitKey, location: KeyLocation) -> Option<Key> {
    match logical {
        WinitKey::Character(text) => {
            let mut chars = text.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            let upper = c.to_uppercase().next().filter(|_| c.to_uppercase().count() == 1).unwrap_or(c);
            Some(Key::Char(upper))
        }
        WinitKey::Named(named) => named_key(*named, location),
        _ => None,
    }
}

fn named_key(named: NamedKey, location: KeyLocation) -> Option<Key> {
    Some(match named {
        NamedKey::Shift => Key::Shift,
        NamedKey::Control => Key::Ctrl,
        NamedKey::Alt | NamedKey::AltGraph => Key::Alt,
        NamedKey::Super | NamedKey::Meta => Key::Meta,
        NamedKey::Space => Key::Space,
        NamedKey::Escape => Key::Esc,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        // Qt: the main Enter key is Key_Return, the keypad's Key_Enter.
        NamedKey::Enter if location == KeyLocation::Numpad => Key::Enter,
        NamedKey::Enter => Key::Return,
        NamedKey::Insert => Key::Ins,
        NamedKey::Delete => Key::Del,
        NamedKey::Pause => Key::Pause,
        NamedKey::PrintScreen => Key::Print,
        NamedKey::Clear => Key::Clear,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::PageUp => Key::PgUp,
        NamedKey::PageDown => Key::PgDown,
        NamedKey::CapsLock => Key::CapsLock,
        NamedKey::NumLock => Key::NumLock,
        NamedKey::ScrollLock => Key::ScrollLock,
        NamedKey::ContextMenu => Key::Menu,
        NamedKey::Help => Key::Help,
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        NamedKey::F13 => Key::F(13),
        NamedKey::F14 => Key::F(14),
        NamedKey::F15 => Key::F(15),
        NamedKey::F16 => Key::F(16),
        NamedKey::F17 => Key::F(17),
        NamedKey::F18 => Key::F(18),
        NamedKey::F19 => Key::F(19),
        NamedKey::F20 => Key::F(20),
        NamedKey::F21 => Key::F(21),
        NamedKey::F22 => Key::F(22),
        NamedKey::F23 => Key::F(23),
        NamedKey::F24 => Key::F(24),
        _ => return None,
    })
}

/// A winit keyboard event as a [`KeyInput`]; None for keys the controls
/// cannot name (dead keys, IME composition).
pub fn key_input(event: &KeyEvent) -> Option<KeyInput> {
    // The key under Esc, whatever the layout calls it (` ~ or a national
    // letter): Windows' VK_OEM_3 is the Backquote position.
    let under_esc = event.physical_key == PhysicalKey::Code(KeyCode::Backquote)
        || matches!(&event.logical_key, WinitKey::Character(t) if t.as_str() == "~");
    let key = key_from_winit(&event.logical_key, event.location).or(under_esc.then_some(Key::Char('`')))?;
    Some(KeyInput { key, under_esc, repeat: event.repeat, pressed: event.state == ElementState::Pressed })
}

/// winit's button as the controls name it (Qt's `MouseButton` values).
pub fn mouse_button(button: WinitButton) -> Option<MouseButton> {
    Some(match button {
        WinitButton::Left => MouseButton::Left,
        WinitButton::Right => MouseButton::Right,
        WinitButton::Middle => MouseButton::Middle,
        WinitButton::Back => MouseButton::Back,
        WinitButton::Forward => MouseButton::Forward,
        WinitButton::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    #[test]
    fn keys_read_as_qt_named_them() {
        let k = |s: &str| key_from_winit(&WinitKey::Character(SmolStr::new(s)), KeyLocation::Standard);
        assert_eq!(k("w"), Some(Key::Char('W')));
        assert_eq!(k("W"), Some(Key::Char('W')));
        assert_eq!(k("`"), Some(Key::Char('`')));
        assert_eq!(k("~"), Some(Key::Char('~')));
        assert_eq!(k("ab"), None);
        let n = |named| key_from_winit(&WinitKey::Named(named), KeyLocation::Standard);
        assert_eq!(n(NamedKey::Escape), Some(Key::Esc));
        assert_eq!(n(NamedKey::F8), Some(Key::F(8)));
        assert_eq!(n(NamedKey::Shift), Some(Key::Shift));
        assert_eq!(n(NamedKey::Space), Some(Key::Space));
        assert_eq!(n(NamedKey::ArrowUp), Some(Key::Up));
        assert_eq!(n(NamedKey::Enter), Some(Key::Return));
        assert_eq!(key_from_winit(&WinitKey::Named(NamedKey::Enter), KeyLocation::Numpad), Some(Key::Enter));
        // The names round-trip through the bindings' own parser.
        for key in [Key::Esc, Key::F(8), Key::Shift, Key::Space, Key::Up, Key::Char('W'), Key::Char('`')] {
            assert_eq!(Key::from_name(&key.name()), Some(key));
        }
    }

    #[test]
    fn mouse_buttons_map_to_the_controls() {
        assert_eq!(mouse_button(WinitButton::Left), Some(MouseButton::Left));
        assert_eq!(mouse_button(WinitButton::Forward), Some(MouseButton::Forward));
        assert_eq!(mouse_button(WinitButton::Other(9)), None);
    }

    #[test]
    fn the_tilde_counts_as_the_key_under_esc() {
        assert!(KeyInput::press(Key::Char('~')).under_esc);
        assert!(!KeyInput::press(Key::Char('W')).under_esc);
        assert!(KeyInput::press(Key::Char('W')).repeated().repeat);
        assert!(!KeyInput::release(Key::Esc).pressed);
    }
}
