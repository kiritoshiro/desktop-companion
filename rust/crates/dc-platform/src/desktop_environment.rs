//! Port of world/desktop_environment.py: lightweight desktop/window
//! awareness.
//!
//! The overlay is always on top, so real windows cannot occlude it; the
//! animation layer uses this snapshot to *simulate* depth (a spider walking
//! into a window rectangle is clipped as if it passed behind it). File
//! Explorer windows and desktop folder icons can be portals.
//!
//! Defensive like the Python: any failing call drops that window, and a
//! failing enumeration returns an empty list.
//!
//! Desktop icons are read from Explorer's `SysListView32` with a remote
//! memory round trip (`VirtualAllocEx` / `ReadProcessMemory`). That stays
//! **off by default**: [`SnapshotOptions::default`] leaves
//! `include_desktop_icons` false, matching `CreatureManager`'s
//! `desktop_icons_enabled = False` (the Python function's own keyword
//! default was True, but nothing called it that way).

use std::collections::HashSet;
use std::path::PathBuf;

use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};
use windows::Win32::UI::Controls::{LVIF_TEXT, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMPOSITION, LVM_GETITEMTEXTW};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetLayeredWindowAttributes, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, SendMessageW,
};
use windows::core::{BOOL, PCWSTR};

use crate::overlay_win32::{WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT};
use crate::win::{
    class_name, ex_style, hwnd_to_raw, is_dwm_cloaked, pcwstr, rect_from_dwm, rect_from_user32, wide, window_text,
};

/// What a surface is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Window,
    Folder,
}

impl SurfaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SurfaceKind::Window => "window",
            SurfaceKind::Folder => "folder",
        }
    }
}

/// A rectangular desktop object spiders can react to.
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopSurface {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub title: String,
    pub class_name: String,
    pub kind: SurfaceKind,
    /// The real window handle (0 for desktop icons). Not in the Python
    /// dataclass; useful to tell snapshots apart.
    pub hwnd: isize,
}

impl DesktopSurface {
    pub fn left(&self) -> f64 {
        self.x
    }
    pub fn top(&self) -> f64 {
        self.y
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// The same surface with every coordinate divided by `dpr` (the engine's
    /// physical-to-logical step after a snapshot).
    pub fn scaled_down(&self, dpr: f64) -> Self {
        let d = if dpr > 0.0 { dpr } else { 1.0 };
        Self { x: self.x / d, y: self.y / d, w: self.w / d, h: self.h / d, ..self.clone() }
    }
}

/// Desktop infrastructure classes, never useful hide-behind surfaces.
pub const SKIP_CLASSES: [&str; 12] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "DV2ControlHost",
    "MsgrIMEWindowClass",
    "IME",
    "ToolTips_Class32",
    "NotifyIconOverflowWindow",
    "ApplicationFrameInputSinkWindow",
    "Windows.UI.Composition.DesktopWindowContentBridge",
    "Xaml_WindowedPopupClass",
];

/// File Explorer folder window classes.
pub const FOLDER_CLASSES: [&str; 2] = ["CabinetWClass", "ExploreWClass"];

pub const MAX_WINDOWS: usize = 40;
pub const MAX_ICONS: usize = 32;
pub const MAX_ICON_ITEMS: i32 = 96;
pub const MIN_WINDOW_W: i32 = 90;
pub const MIN_WINDOW_H: i32 = 70;

/// Arguments of `snapshot_desktop_surfaces`, all in physical pixels.
#[derive(Clone, Debug, Default)]
pub struct SnapshotOptions {
    /// The overlay's own window.
    pub exclude_hwnd: Option<isize>,
    pub origin_x: i32,
    pub origin_y: i32,
    pub screen_w: Option<i32>,
    pub screen_h: Option<i32>,
    /// Off by default (see the module notes).
    pub include_desktop_icons: bool,
}

/// Skip alpha-0 helper overlays that would otherwise be invisible walls.
pub fn layered_window_is_effectively_invisible(hwnd: HWND, ex_style: u32) -> bool {
    if ex_style & WS_EX_LAYERED == 0 {
        return false;
    }
    let mut key = COLORREF(0);
    let mut alpha: u8 = 255;
    let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
    if unsafe { GetLayeredWindowAttributes(hwnd, Some(&mut key), Some(&mut alpha), Some(&mut flags)) }.is_ok() {
        return flags.0 & LWA_ALPHA.0 != 0 && alpha <= 8;
    }
    false
}

/// The per-window filter of the snapshot, minus the Win32 lookups: given
/// what was read about a window, keep it?
pub fn keep_window(cls: &str, ex: u32, title: &str, cloaked: bool, invisible_layered: bool) -> bool {
    if cls.is_empty() || SKIP_CLASSES.contains(&cls) || cloaked {
        return false;
    }
    let folder = FOLDER_CLASSES.contains(&cls);
    if ex & WS_EX_TOOLWINDOW != 0 {
        return false;
    }
    if ex & WS_EX_TRANSPARENT != 0 && !folder {
        return false;
    }
    if invisible_layered {
        return false;
    }
    // Untitled utility windows are frequently invisible helpers; Explorer
    // can briefly have an empty caption, so folder classes stay.
    !(title.is_empty() && !folder)
}

/// A window rectangle in overlay-local coordinates, or `None` when it is too
/// small or entirely off the overlay.
pub fn local_window_rect(rect: (i32, i32, i32, i32), opts: &SnapshotOptions) -> Option<(i32, i32, i32, i32)> {
    let left = rect.0 - opts.origin_x;
    let top = rect.1 - opts.origin_y;
    let right = rect.2 - opts.origin_x;
    let bottom = rect.3 - opts.origin_y;
    let (w, h) = (right - left, bottom - top);
    if w < MIN_WINDOW_W || h < MIN_WINDOW_H {
        return None;
    }
    if let (Some(sw), Some(sh)) = (opts.screen_w, opts.screen_h)
        && (right <= 0 || bottom <= 0 || left >= sw || top >= sh)
    {
        return None;
    }
    Some((left, top, w, h))
}

struct EnumState<'a> {
    opts: &'a SnapshotOptions,
    pid: u32,
    out: Vec<DesktopSurface>,
}

unsafe extern "system" fn visit_window(hwnd: HWND, data: LPARAM) -> BOOL {
    let state = unsafe { &mut *(data.0 as *mut EnumState) };
    if let Some(s) = surface_for(hwnd, state.opts, state.pid) {
        state.out.push(s);
    }
    BOOL(1)
}

fn surface_for(hwnd: HWND, opts: &SnapshotOptions, own_pid: u32) -> Option<DesktopSurface> {
    if Some(hwnd_to_raw(hwnd)) == opts.exclude_hwnd {
        return None;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == own_pid {
            return None;
        }
    }
    let cls = class_name(hwnd);
    if cls.is_empty() || SKIP_CLASSES.contains(&cls.as_str()) || is_dwm_cloaked(hwnd) {
        return None;
    }
    let ex = ex_style(hwnd);
    let invisible = layered_window_is_effectively_invisible(hwnd, ex);
    let title = window_text(hwnd);
    if !keep_window(&cls, ex, &title, false, invisible) {
        return None;
    }
    let rect = rect_from_dwm(hwnd).or_else(|| rect_from_user32(hwnd))?;
    let (left, top, w, h) = local_window_rect((rect.left, rect.top, rect.right, rect.bottom), opts)?;
    let kind = if FOLDER_CLASSES.contains(&cls.as_str()) { SurfaceKind::Folder } else { SurfaceKind::Window };
    Some(DesktopSurface {
        x: left as f64,
        y: top as f64,
        w: w as f64,
        h: h as f64,
        title,
        class_name: cls,
        kind,
        hwnd: hwnd_to_raw(hwnd),
    })
}

/// Visible top-level windows in overlay-local coordinates, front to back
/// (EnumWindows Z order), at most [`MAX_WINDOWS`], followed by desktop
/// folder icons (at most [`MAX_ICONS`]) when asked for. Windows of this
/// process are skipped so menus/tool windows never become occluders.
pub fn snapshot_desktop_surfaces(opts: &SnapshotOptions) -> Vec<DesktopSurface> {
    let mut state = EnumState { opts, pid: std::process::id(), out: Vec::new() };
    if unsafe { EnumWindows(Some(visit_window), LPARAM(&mut state as *mut EnumState as isize)) }.is_err() {
        return Vec::new();
    }
    let mut windows = state.out;
    windows.truncate(MAX_WINDOWS);
    if opts.include_desktop_icons {
        let mut icons = desktop_folder_icon_surfaces(opts);
        icons.truncate(MAX_ICONS);
        windows.extend(icons);
    }
    windows
}

/// Best-effort set of folder names likely shown on the desktop (casefolded).
pub fn desktop_folder_names() -> HashSet<String> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["USERPROFILE", "OneDrive"] {
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            roots.push(PathBuf::from(v).join("Desktop"));
        }
    }
    if let Some(home) = std::env::home_dir() {
        roots.push(home.join("Desktop"));
    }
    let public = std::env::var_os("PUBLIC")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    roots.push(public.join("Desktop"));

    let mut names = HashSet::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                names.insert(entry.file_name().to_string_lossy().to_lowercase());
            }
        }
    }
    names
}

fn find_child(parent: HWND, class: &str) -> HWND {
    let w = wide(class);
    unsafe { FindWindowExW(Some(parent), None, pcwstr(&w), PCWSTR::null()) }.unwrap_or_default()
}

unsafe extern "system" fn find_listview(hwnd: HWND, data: LPARAM) -> BOOL {
    let found = unsafe { &mut *(data.0 as *mut HWND) };
    let defview = find_child(hwnd, "SHELLDLL_DefView");
    if !defview.0.is_null() {
        let listview = find_child(defview, "SysListView32");
        if !listview.0.is_null() {
            *found = listview;
            return BOOL(0);
        }
    }
    BOOL(1)
}

/// The desktop icon `SysListView32`, under Progman or (after a wallpaper
/// slideshow moved it) under a WorkerW.
pub fn desktop_listview_hwnd() -> Option<HWND> {
    let progman_class = wide("Progman");
    let progman = unsafe { FindWindowW(pcwstr(&progman_class), PCWSTR::null()) }.unwrap_or_default();
    if !progman.0.is_null() {
        let defview = find_child(progman, "SHELLDLL_DefView");
        if !defview.0.is_null() {
            let listview = find_child(defview, "SysListView32");
            if !listview.0.is_null() {
                return Some(listview);
            }
        }
    }
    let mut found = HWND::default();
    unsafe {
        let _ = EnumWindows(Some(find_listview), LPARAM(&mut found as *mut HWND as isize));
    }
    if found.0.is_null() { None } else { Some(found) }
}

struct RemoteProcess(HANDLE);

impl Drop for RemoteProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn read_remote<T: Copy>(process: HANDLE, address: usize, out: &mut T) -> bool {
    unsafe {
        ReadProcessMemory(
            process,
            address as *const core::ffi::c_void,
            out as *mut T as *mut core::ffi::c_void,
            std::mem::size_of::<T>(),
            None,
        )
    }
    .is_ok()
}

fn write_remote<T: Copy>(process: HANDLE, address: usize, value: &T) -> bool {
    unsafe {
        WriteProcessMemory(
            process,
            address as *const core::ffi::c_void,
            value as *const T as *const core::ffi::c_void,
            std::mem::size_of::<T>(),
            None,
        )
    }
    .is_ok()
}

/// Best-effort visible desktop folder icon rectangles (86x86 footprint
/// around each item cell), overlay-local. Any failure returns no icons.
pub fn desktop_folder_icon_surfaces(opts: &SnapshotOptions) -> Vec<DesktopSurface> {
    let Some(hwnd) = desktop_listview_hwnd() else { return Vec::new() };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return Vec::new();
    }
    let access = PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE;
    let Ok(process) = (unsafe { OpenProcess(access, false, pid) }) else { return Vec::new() };
    let process = RemoteProcess(process);

    let count = unsafe { SendMessageW(hwnd, LVM_GETITEMCOUNT, Some(WPARAM(0)), Some(LPARAM(0))) }.0 as i32;
    let count = count.clamp(0, MAX_ICON_ITEMS);
    if count <= 0 {
        return Vec::new();
    }

    const TEXT_CHARS: usize = 260;
    let lvitem_size = std::mem::size_of::<LVITEMW>();
    let block_size = std::mem::size_of::<POINT>().max(lvitem_size + TEXT_CHARS * 2);
    let remote = unsafe { VirtualAllocEx(process.0, None, block_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
    if remote.is_null() {
        return Vec::new();
    }
    let remote = remote as usize;
    let remote_text = remote + lvitem_size;

    let mut origin = POINT::default();
    unsafe {
        let _ = ClientToScreen(hwnd, &mut origin);
    }
    let folder_names = desktop_folder_names();
    let mut surfaces = Vec::new();
    for i in 0..count {
        let got =
            unsafe { SendMessageW(hwnd, LVM_GETITEMPOSITION, Some(WPARAM(i as usize)), Some(LPARAM(remote as isize))) };
        if got.0 == 0 {
            continue;
        }
        let mut pt = POINT::default();
        if !read_remote(process.0, remote, &mut pt) {
            continue;
        }
        let item = LVITEMW {
            mask: LVIF_TEXT,
            iItem: i,
            iSubItem: 0,
            pszText: windows::core::PWSTR(remote_text as *mut u16),
            cchTextMax: TEXT_CHARS as i32,
            ..Default::default()
        };
        let name = if write_remote(process.0, remote, &item) {
            unsafe { SendMessageW(hwnd, LVM_GETITEMTEXTW, Some(WPARAM(i as usize)), Some(LPARAM(remote as isize))) };
            let mut buf = [0u16; TEXT_CHARS];
            if read_remote(process.0, remote_text, &mut buf) { crate::win::from_wide(&buf) } else { String::new() }
        } else {
            String::new()
        };
        // With no folder names known, every item counts (Python's
        // include_unknown_when_no_folder_names).
        if !folder_names.is_empty() && !folder_names.contains(&name.to_lowercase()) {
            continue;
        }
        let x = origin.x + pt.x - opts.origin_x - 6;
        let y = origin.y + pt.y - opts.origin_y - 6;
        let (w, h) = (86, 86);
        if let (Some(sw), Some(sh)) = (opts.screen_w, opts.screen_h)
            && (x + w <= 0 || y + h <= 0 || x >= sw || y >= sh)
        {
            continue;
        }
        surfaces.push(DesktopSurface {
            x: x as f64,
            y: y as f64,
            w: w as f64,
            h: h as f64,
            title: name,
            class_name: "DesktopFolderIcon".into(),
            kind: SurfaceKind::Folder,
            hwnd: 0,
        });
        if surfaces.len() >= MAX_ICONS {
            break;
        }
    }
    unsafe {
        let _ = VirtualFreeEx(process.0, remote as *mut core::ffi::c_void, 0, MEM_RELEASE);
    }
    surfaces
}

/// `RECT` to `(l, t, r, b)`.
pub fn ltrb(r: &RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_python_rules() {
        assert!(keep_window("Notepad", 0, "Untitled - Notepad", false, false));
        assert!(!keep_window("Progman", 0, "Program Manager", false, false));
        assert!(!keep_window("", 0, "x", false, false));
        assert!(!keep_window("Notepad", 0, "x", true, false), "cloaked windows hide");
        assert!(!keep_window("Notepad", WS_EX_TOOLWINDOW, "x", false, false));
        assert!(!keep_window("Notepad", WS_EX_TRANSPARENT, "x", false, false));
        assert!(keep_window("CabinetWClass", WS_EX_TRANSPARENT, "", false, false), "folders survive both");
        assert!(!keep_window("Notepad", 0, "", false, false), "untitled helpers are skipped");
        assert!(!keep_window("Notepad", WS_EX_LAYERED, "x", false, true));
    }

    #[test]
    fn local_rects_are_clipped_to_the_overlay() {
        let opts = SnapshotOptions {
            origin_x: -1920,
            origin_y: 0,
            screen_w: Some(3840),
            screen_h: Some(1080),
            ..Default::default()
        };
        assert_eq!(local_window_rect((-1900, 10, -1500, 400), &opts), Some((20, 10, 400, 390)));
        assert_eq!(local_window_rect((0, 0, 80, 500), &opts), None, "too narrow");
        assert_eq!(local_window_rect((0, 0, 500, 60), &opts), None, "too short");
        assert_eq!(local_window_rect((1920, 0, 2500, 500), &opts), None, "beyond the right edge");
        assert!(!SnapshotOptions::default().include_desktop_icons, "icon probing must be opt-in");
    }

    #[test]
    fn smoke_snapshot_enumerates_windows() {
        let opts = SnapshotOptions::default();
        let surfaces = snapshot_desktop_surfaces(&opts);
        assert!(surfaces.len() <= MAX_WINDOWS);
        for s in &surfaces {
            assert!(s.w >= MIN_WINDOW_W as f64 && s.h >= MIN_WINDOW_H as f64);
            assert!(!SKIP_CLASSES.contains(&s.class_name.as_str()));
            assert!(s.hwnd != 0);
        }
        let s = DesktopSurface {
            x: 30.0,
            y: 60.0,
            w: 300.0,
            h: 150.0,
            title: String::new(),
            class_name: String::new(),
            kind: SurfaceKind::Window,
            hwnd: 1,
        };
        assert_eq!(s.scaled_down(1.5).w, 200.0);
        assert_eq!(s.right(), 330.0);
    }
}
