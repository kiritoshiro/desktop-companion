//! The tray icon of app/engine.py (`create_tray`: `QSystemTrayIcon` + a
//! `QMenu` of actions, checkable actions, exclusive action groups and
//! submenus, plus `showMessage` balloons) on the `tray-icon` crate.
//!
//! The engine builds a [`TrayMenu`] model, creates a [`Tray`] from it on the
//! UI thread (a Win32 message loop must run there; winit's does), and each
//! frame drains [`Tray::poll_events`]. Exclusive groups (`QActionGroup`)
//! are kept consistent here: clicking one item of a group checks it and
//! unchecks the rest.
//!
//! Balloons: `tray-icon` has no notification API, so [`Tray::show_message`]
//! calls `Shell_NotifyIconW(NIM_MODIFY, NIF_INFO)` on the icon `tray-icon`
//! registered, found by its hidden window and (when the icon has no fixed
//! GUID) by probing the small integer ids it hands out.
//!
//! Not ported: per-item tooltips and icons (`wood_theme.menu_icon`); the
//! native Win32 menu has neither.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};

use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_GUID, NIF_INFO, NIIF_ERROR, NIIF_INFO, NIIF_WARNING, NIM_MODIFY, NOTIFY_ICON_INFOTIP_FLAGS, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::core::GUID;

/// One entry of the menu model.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// A plain action.
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    /// A checkable action; `group` makes it part of an exclusive set.
    Check {
        id: String,
        label: String,
        checked: bool,
        enabled: bool,
        group: Option<String>,
    },
    /// A disabled line of text (`add_note`).
    Note {
        id: String,
        label: String,
    },
    Separator,
    Submenu {
        id: String,
        label: String,
        enabled: bool,
        entries: Vec<MenuEntry>,
    },
}

/// A whole menu, built with the helper methods.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrayMenu {
    pub entries: Vec<MenuEntry>,
}

impl TrayMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, id: &str, label: &str) -> Self {
        self.entries.push(MenuEntry::Item { id: id.into(), label: label.into(), enabled: true });
        self
    }

    pub fn check(mut self, id: &str, label: &str, checked: bool) -> Self {
        self.entries.push(MenuEntry::Check { id: id.into(), label: label.into(), checked, enabled: true, group: None });
        self
    }

    pub fn radio(mut self, id: &str, label: &str, checked: bool, group: &str) -> Self {
        self.entries.push(MenuEntry::Check {
            id: id.into(),
            label: label.into(),
            checked,
            enabled: true,
            group: Some(group.into()),
        });
        self
    }

    pub fn note(mut self, id: &str, label: &str) -> Self {
        self.entries.push(MenuEntry::Note { id: id.into(), label: label.into() });
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
        self
    }

    pub fn submenu(mut self, id: &str, label: &str, menu: TrayMenu) -> Self {
        self.entries.push(MenuEntry::Submenu {
            id: id.into(),
            label: label.into(),
            enabled: true,
            entries: menu.entries,
        });
        self
    }

    /// Every id in the model, depth first.
    pub fn ids(&self) -> Vec<String> {
        fn walk(entries: &[MenuEntry], out: &mut Vec<String>) {
            for e in entries {
                match e {
                    MenuEntry::Item { id, .. } | MenuEntry::Check { id, .. } | MenuEntry::Note { id, .. } => {
                        out.push(id.clone())
                    }
                    MenuEntry::Submenu { id, entries, .. } => {
                        out.push(id.clone());
                        walk(entries, out);
                    }
                    MenuEntry::Separator => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.entries, &mut out);
        out
    }

    /// Groups of mutually exclusive check ids.
    pub fn groups(&self) -> HashMap<String, Vec<String>> {
        fn walk(entries: &[MenuEntry], out: &mut HashMap<String, Vec<String>>) {
            for e in entries {
                match e {
                    MenuEntry::Check { id, group: Some(g), .. } => out.entry(g.clone()).or_default().push(id.clone()),
                    MenuEntry::Submenu { entries, .. } => walk(entries, out),
                    _ => {}
                }
            }
        }
        let mut out = HashMap::new();
        walk(&self.entries, &mut out);
        out
    }
}

/// What happened on the tray.
#[derive(Clone, Debug, PartialEq)]
pub enum TrayEvent {
    /// A menu entry was chosen; `checked` is the new state of a check item.
    Menu {
        id: String,
        checked: Option<bool>,
    },
    /// The icon itself was clicked (button released).
    IconClicked {
        left: bool,
    },
    IconDoubleClicked,
}

/// Balloon severity (`QSystemTrayIcon.MessageIcon`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageIcon {
    Information,
    Warning,
    Critical,
}

enum Handle {
    Item(MenuItem),
    Check(CheckMenuItem),
    Submenu(Submenu),
}

/// Forwarders installed once per process (muda/tray-icon handlers are
/// process-global and settable only once).
struct Forward {
    menu: Mutex<Receiver<MenuEvent>>,
    icon: Mutex<Receiver<TrayIconEvent>>,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

static FORWARD: OnceLock<Forward> = OnceLock::new();

fn forward() -> &'static Forward {
    FORWARD.get_or_init(|| {
        let (mtx, mrx) = mpsc::channel::<MenuEvent>();
        let (itx, irx) = mpsc::channel::<TrayIconEvent>();
        let mtx = Mutex::new(mtx);
        let itx = Mutex::new(itx);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Ok(tx) = mtx.lock() {
                let _ = tx.send(e);
            }
            wake_tray();
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let Ok(tx) = itx.lock() {
                let _ = tx.send(e);
            }
            wake_tray();
        }));
        Forward { menu: Mutex::new(mrx), icon: Mutex::new(irx), waker: Mutex::new(None) }
    })
}

fn wake_tray() {
    if let Some(f) = FORWARD.get()
        && let Ok(w) = f.waker.lock()
        && let Some(w) = w.as_ref()
    {
        w();
    }
}

/// The tray icon and its menu.
pub struct Tray {
    icon: TrayIcon,
    _menu: Menu,
    handles: HashMap<String, Handle>,
    groups: HashMap<String, Vec<String>>,
    guid: Option<u128>,
    notify_uid: Option<u32>,
}

impl Tray {
    /// Build the icon from an RGBA image and the menu model. `guid` gives
    /// the icon a stable shell identity (keeps the user's "always show"
    /// choice across updates); note Windows ties a GUID to the executable's
    /// path, so leave it `None` for development builds.
    pub fn new(
        tooltip: &str,
        icon_rgba: Vec<u8>,
        icon_size: (u32, u32),
        model: &TrayMenu,
        guid: Option<u128>,
    ) -> Result<Self, String> {
        forward();
        let menu = Menu::new();
        let mut handles = HashMap::new();
        append_entries(&model.entries, &mut handles, &|item| menu.append(item))?;
        let icon = Icon::from_rgba(icon_rgba, icon_size.0, icon_size.1).map_err(|e| e.to_string())?;
        let mut builder = TrayIconBuilder::new()
            .with_tooltip(tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false);
        if let Some(g) = guid {
            builder = builder.with_guid(g);
        }
        let icon = builder.build().map_err(|e| e.to_string())?;
        Ok(Self { icon, _menu: menu, handles, groups: model.groups(), guid, notify_uid: None })
    }

    /// Called from a tray thread whenever an event is queued (to wake a
    /// winit loop through its `EventLoopProxy`).
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = forward().waker.lock() {
            *w = Some(Arc::new(waker));
        }
    }

    pub fn set_tooltip(&self, text: &str) {
        let _ = self.icon.set_tooltip(Some(text));
    }

    pub fn set_icon(&self, rgba: Vec<u8>, size: (u32, u32)) {
        if let Ok(icon) = Icon::from_rgba(rgba, size.0, size.1) {
            let _ = self.icon.set_icon(Some(icon));
        }
    }

    pub fn set_visible(&self, visible: bool) {
        let _ = self.icon.set_visible(visible);
    }

    pub fn set_text(&self, id: &str, text: &str) {
        match self.handles.get(id) {
            Some(Handle::Item(i)) => i.set_text(text),
            Some(Handle::Check(c)) => c.set_text(text),
            Some(Handle::Submenu(s)) => s.set_text(text),
            None => {}
        }
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) {
        match self.handles.get(id) {
            Some(Handle::Item(i)) => i.set_enabled(enabled),
            Some(Handle::Check(c)) => c.set_enabled(enabled),
            Some(Handle::Submenu(s)) => s.set_enabled(enabled),
            None => {}
        }
    }

    /// Set a check item; checking a grouped item unchecks its siblings.
    pub fn set_checked(&self, id: &str, checked: bool) {
        if let Some(Handle::Check(c)) = self.handles.get(id) {
            c.set_checked(checked);
        }
        if checked {
            self.uncheck_siblings(id);
        }
    }

    pub fn is_checked(&self, id: &str) -> Option<bool> {
        match self.handles.get(id) {
            Some(Handle::Check(c)) => Some(c.is_checked()),
            _ => None,
        }
    }

    fn uncheck_siblings(&self, id: &str) {
        for members in self.groups.values().filter(|m| m.iter().any(|x| x == id)) {
            for other in members.iter().filter(|x| *x != id) {
                if let Some(Handle::Check(c)) = self.handles.get(other) {
                    c.set_checked(false);
                }
            }
        }
    }

    fn in_group(&self, id: &str) -> bool {
        self.groups.values().any(|m| m.iter().any(|x| x == id))
    }

    /// Everything that happened since the last call.
    pub fn poll_events(&self) -> Vec<TrayEvent> {
        let f = forward();
        let mut out = Vec::new();
        if let Ok(rx) = f.menu.lock() {
            for e in rx.try_iter() {
                let id = e.id.0.clone();
                let checked = match self.handles.get(&id) {
                    Some(Handle::Check(c)) => {
                        if self.in_group(&id) {
                            // An exclusive action stays checked when clicked again.
                            c.set_checked(true);
                            self.uncheck_siblings(&id);
                        }
                        Some(c.is_checked())
                    }
                    _ => None,
                };
                out.push(TrayEvent::Menu { id, checked });
            }
        }
        if let Ok(rx) = f.icon.lock() {
            for e in rx.try_iter() {
                match e {
                    TrayIconEvent::Click { button, button_state: MouseButtonState::Up, .. } => {
                        out.push(TrayEvent::IconClicked { left: button == MouseButton::Left })
                    }
                    TrayIconEvent::DoubleClick { .. } => out.push(TrayEvent::IconDoubleClicked),
                    _ => {}
                }
            }
        }
        out
    }

    /// A balloon notification (`QSystemTrayIcon.showMessage`). Returns
    /// whether the shell accepted it. `timeout_ms` is a hint Windows mostly
    /// ignores (it uses the accessibility timeout).
    pub fn show_message(&mut self, title: &str, body: &str, icon: MessageIcon, timeout_ms: u32) -> bool {
        let hwnd = HWND(self.icon.window_handle());
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uFlags: NIF_INFO,
            dwInfoFlags: match icon {
                MessageIcon::Information => NIIF_INFO,
                MessageIcon::Warning => NIIF_WARNING,
                MessageIcon::Critical => NIIF_ERROR,
            } | NOTIFY_ICON_INFOTIP_FLAGS(0),
            ..Default::default()
        };
        nid.Anonymous.uTimeout = timeout_ms;
        copy_wide(&mut nid.szInfo, body);
        copy_wide(&mut nid.szInfoTitle, title);
        if let Some(g) = self.guid {
            nid.uFlags |= NIF_GUID;
            nid.guidItem = GUID::from_u128(g);
            return unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool();
        }
        if let Some(uid) = self.notify_uid {
            nid.uID = uid;
            if unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool() {
                return true;
            }
        }
        // tray-icon numbers its icons 1, 2, ... per process; each has its
        // own hidden window, so only the right id matches this window.
        for uid in 1..=256 {
            nid.uID = uid;
            if unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool() {
                self.notify_uid = Some(uid);
                return true;
            }
        }
        false
    }
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let max = dst.len() - 1;
    for (i, c) in s.encode_utf16().take(max).enumerate() {
        dst[i] = c;
    }
    dst[s.encode_utf16().take(max).count()] = 0;
}

fn append_entries(
    entries: &[MenuEntry],
    handles: &mut HashMap<String, Handle>,
    append: &dyn Fn(&dyn IsMenuItem) -> tray_icon::menu::Result<()>,
) -> Result<(), String> {
    for e in entries {
        match e {
            MenuEntry::Item { id, label, enabled } => {
                let item = MenuItem::with_id(id.as_str(), label, *enabled, None);
                append(&item).map_err(|e| e.to_string())?;
                handles.insert(id.clone(), Handle::Item(item));
            }
            MenuEntry::Note { id, label } => {
                let item = MenuItem::with_id(id.as_str(), label, false, None);
                append(&item).map_err(|e| e.to_string())?;
                handles.insert(id.clone(), Handle::Item(item));
            }
            MenuEntry::Check { id, label, checked, enabled, .. } => {
                let item = CheckMenuItem::with_id(id.as_str(), label, *enabled, *checked, None);
                append(&item).map_err(|e| e.to_string())?;
                handles.insert(id.clone(), Handle::Check(item));
            }
            MenuEntry::Separator => {
                append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
            }
            MenuEntry::Submenu { id, label, enabled, entries } => {
                let sub = Submenu::with_id(id.as_str(), label, *enabled);
                append_entries(entries, handles, &|item| sub.append(item))?;
                append(&sub).map_err(|e| e.to_string())?;
                handles.insert(id.clone(), Handle::Submenu(sub));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> TrayMenu {
        TrayMenu::new()
            .note("status", "Running - clicks pass through spiders.")
            .separator()
            .submenu(
                "performance",
                "Performance",
                TrayMenu::new()
                    .radio("fps60", "Smooth 60 FPS", true, "fps")
                    .radio("fps45", "Still smooth 45 FPS", false, "fps")
                    .radio("fps30", "Balanced 30 FPS", false, "fps"),
            )
            .check("dragging", "Allow dragging", false)
            .separator()
            .item("quit", "Quit overlay")
    }

    #[test]
    fn model_lists_ids_and_groups() {
        let m = model();
        assert_eq!(m.ids(), vec!["status", "performance", "fps60", "fps45", "fps30", "dragging", "quit"]);
        let g = m.groups();
        assert_eq!(g["fps"], vec!["fps60", "fps45", "fps30"]);
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn wide_copy_truncates_and_terminates() {
        let mut buf = [0xFFFFu16; 4];
        copy_wide(&mut buf, "hello");
        assert_eq!(buf, [b'h' as u16, b'e' as u16, b'l' as u16, 0]);
    }
}
