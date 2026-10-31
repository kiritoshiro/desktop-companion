//! The overlay window of app/engine.py (`OverlayWindow.__init__`'s window
//! setup, `nativeEvent`, `_update_input_transparency`, the release hotkey,
//! the periodic style refresh and the screen-geometry check), on winit.
//!
//! One window covers the whole virtual desktop (the bounding box of every
//! monitor), exactly like the Python's single `setGeometry(virtual_screen_geometry())`
//! window. It is undecorated, transparent, topmost (not in adventure mode),
//! a tool window (no taskbar entry) and never activated by clicks in
//! companion mode.
//!
//! Input: the window answers `WM_NCHITTEST` from a closure the engine
//! installs ([`OverlayWindow::set_hit_test`]): `HTCLIENT` over something
//! interactive, `HTTRANSPARENT` elsewhere. `HTTRANSPARENT` only forwards to
//! windows of the same thread, so (DC-79) every 50 ms the closure is also
//! asked about the cursor position and `WS_EX_TRANSPARENT` toggled for the
//! whole window. Win32 timers drive that check, the style refresh and the
//! geometry check, but Windows only delivers `WM_TIMER` when no paint is
//! pending, and winit's loop always has one queued while the engine draws
//! every vsync. So the event loop must also call
//! [`OverlayWindow::service_timers`] on every pass; whichever runs first
//! does the job and the other skips it.
//!
//! Styles: `WS_EX_LAYERED | WS_EX_TOOLWINDOW` (+ `WS_EX_NOACTIVATE` in
//! companion mode) are re-applied every second as in the Python, and
//! additionally enforced in `WM_STYLECHANGING`, so winit recomputing its own
//! styles can never strip them.
//!
//! Drawing: [`OverlayBackend::Composition`] creates the window without a
//! redirection bitmap for a wgpu DX12 swapchain on a DirectComposition
//! visual ([`crate::gpu`]), which is what makes per-pixel alpha work.
//! [`OverlayBackend::Layered`] is the software fallback: the caller renders
//! premultiplied RGBA and [`OverlayWindow::present_premultiplied`] hands it
//! to `UpdateLayeredWindow`.
//!
//! Coordinates are physical pixels; overlay-local means relative to
//! [`OverlayWindow::geometry`]'s origin.
//!
//! Threading: everything here must stay on the winit event-loop thread
//! (`OverlayWindow` is `!Send`).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dc_core::IRect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, HTCLIENT, HTTRANSPARENT, KillTimer, LWA_ALPHA, MA_NOACTIVATE, STYLESTRUCT, SWP_NOACTIVATE,
    SWP_NOZORDER, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ULW_ALPHA, UPDATELAYEREDWINDOWINFO,
    UpdateLayeredWindowIndirect, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_MOUSEACTIVATE, WM_NCDESTROY,
    WM_NCHITTEST, WM_STYLECHANGING, WM_TIMER,
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
use winit::window::{Window, WindowLevel};

use crate::input::{cursor_pos, virtual_key_for};
use crate::monitors::virtual_screen_geometry;
use crate::overlay_win32::{apply_click_through, overlay_ex_style, set_cursor_pos};
use crate::win::{ex_style, set_ex_style};

pub const WINDOW_TITLE: &str = "Desktop Bug Companion Overlay";
pub const WINDOW_CLASS: &str = "DesktopBugCompanionOverlay";
/// DC-79: how often the overlay re-decides whether to let the mouse through.
pub const INPUT_CHECK_MS: u32 = 50;
/// Qt/Windows can rewrite extended styles; re-apply this often.
pub const STYLE_REFRESH_MS: u32 = 1000;
/// Monitor geometry is re-checked this often.
pub const SCREEN_GEOMETRY_CHECK_MS: u32 = 1500;
pub const WM_HOTKEY_MSG: u32 = 0x0312;
/// The id of the adventure release hotkey.
pub const RELEASE_HOTKEY_ID: i32 = 0xD0C1;
pub const MOD_NOREPEAT: u32 = 0x4000;

const SUBCLASS_ID: usize = 0xD0C0;
const TIMER_INPUT: usize = 0xD0C2;
const TIMER_STYLE: usize = 0xD0C3;
const TIMER_GEOMETRY: usize = 0xD0C4;

/// `--mode` of the overlay process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Companion,
    Adventure,
}

/// How the overlay's pixels reach the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayBackend {
    /// wgpu DX12 swapchain on a DirectComposition visual (premultiplied
    /// alpha). The window has no redirection bitmap.
    Composition,
    /// Software: premultiplied RGBA through `UpdateLayeredWindow`.
    Layered,
}

#[derive(Clone, Debug)]
pub struct OverlayConfig {
    pub title: String,
    pub mode: OverlayMode,
    pub backend: OverlayBackend,
    /// `None` covers the virtual desktop and follows it when monitors change.
    pub geometry: Option<IRect>,
    /// Run the DC-79 input-transparency timer (on by default).
    pub auto_input_transparency: bool,
    /// Never activate on click. Defaults to `mode == Companion`
    /// (`WA_ShowWithoutActivating`).
    pub no_activate: Option<bool>,
}

impl OverlayConfig {
    pub fn new(mode: OverlayMode, backend: OverlayBackend) -> Self {
        Self {
            title: WINDOW_TITLE.into(),
            mode,
            backend,
            geometry: None,
            auto_input_transparency: true,
            no_activate: None,
        }
    }
}

/// Something the window procedure noticed, drained with
/// [`OverlayWindow::drain_events`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverlayEvent {
    /// `WM_HOTKEY` with this id (pressed anywhere on the desktop).
    Hotkey(i32),
    /// The window was moved/resized to a new virtual-desktop geometry.
    GeometryChanged(IRect),
    /// The DC-79 timer flipped whole-window input transparency.
    InputTransparencyChanged(bool),
}

type HitTest = Box<dyn FnMut(f64, f64) -> bool>;

struct State {
    hwnd: HWND,
    adventure: bool,
    no_activate: bool,
    follow_desktop: bool,
    composition: bool,
    geometry: Cell<IRect>,
    input_transparent: Cell<bool>,
    auto_input: Cell<bool>,
    hit_test: RefCell<Option<HitTest>>,
    last_hit: Cell<bool>,
    events: RefCell<VecDeque<OverlayEvent>>,
    waker: RefCell<Option<Box<dyn Fn()>>>,
    hotkeys: RefCell<HashMap<i32, (u32, u32)>>,
    alive: Cell<bool>,
    /// When the input, style and geometry checks are next due.
    next_input: Cell<Instant>,
    next_style: Cell<Instant>,
    next_geometry: Cell<Instant>,
}

impl State {
    fn push(&self, e: OverlayEvent) {
        self.events.borrow_mut().push_back(e);
        if let Ok(w) = self.waker.try_borrow()
            && let Some(w) = w.as_ref()
        {
            w();
        }
    }

    /// Ask the engine's closure about an overlay-local point. Re-entrant
    /// calls (the closure itself pumping messages) get the last answer.
    fn wants_mouse(&self, x: f64, y: f64) -> bool {
        match self.hit_test.try_borrow_mut() {
            Ok(mut hook) => {
                let answer = hook.as_mut().map(|f| f(x, y)).unwrap_or(false);
                self.last_hit.set(answer);
                answer
            }
            Err(_) => self.last_hit.get(),
        }
    }

    fn set_input_transparent(&self, transparent: bool) -> bool {
        self.input_transparent.set(transparent);
        // WM_STYLECHANGING (below) enforces the bit from the cell.
        crate::overlay_win32::set_input_transparent(self.hwnd, transparent)
    }

    fn apply_styles(&self) -> bool {
        let ok = apply_click_through(self.hwnd, self.input_transparent.get(), self.adventure, self.no_activate);
        if self.composition {
            ensure_layered_visible(self.hwnd);
        }
        ok
    }

    /// `_update_input_transparency`.
    fn update_input_transparency(&self) {
        let Some((cx, cy)) = cursor_pos() else { return };
        let g = self.geometry.get();
        let transparent = !self.wants_mouse((cx - g.x) as f64, (cy - g.y) as f64);
        if transparent != self.input_transparent.get() {
            self.set_input_transparent(transparent);
            self.push(OverlayEvent::InputTransparencyChanged(transparent));
        }
    }

    fn run_input_check(&self, now: Instant) {
        self.next_input.set(now + Duration::from_millis(INPUT_CHECK_MS.into()));
        if self.auto_input.get() {
            self.update_input_transparency();
        }
    }

    fn run_style_refresh(&self, now: Instant) {
        self.next_style.set(now + Duration::from_millis(STYLE_REFRESH_MS.into()));
        self.apply_styles();
    }

    fn run_geometry_check(&self, now: Instant) {
        self.next_geometry.set(now + Duration::from_millis(SCREEN_GEOMETRY_CHECK_MS.into()));
        self.check_geometry();
    }

    /// Run whichever periodic checks are due; returns when the next one is.
    fn service_timers(&self, now: Instant) -> Instant {
        if now >= self.next_input.get() {
            self.run_input_check(now);
        }
        if now >= self.next_style.get() {
            self.run_style_refresh(now);
        }
        if now >= self.next_geometry.get() {
            self.run_geometry_check(now);
        }
        self.next_input.get().min(self.next_style.get()).min(self.next_geometry.get())
    }

    fn check_geometry(&self) {
        if !self.follow_desktop {
            return;
        }
        let rect = virtual_screen_geometry();
        if rect != self.geometry.get() {
            self.geometry.set(rect);
            move_window(self.hwnd, rect);
            self.apply_styles();
            self.push(OverlayEvent::GeometryChanged(rect));
        }
    }
}

/// A redirected layered window shows nothing until it has layered
/// attributes or an `UpdateLayeredWindow`. On the composition path the
/// pixels come from the DirectComposition visual, and measured on Windows 11
/// the visual shows with or without this call; it is kept as a safeguard
/// (full opacity changes nothing about the visual's own alpha).
fn ensure_layered_visible(hwnd: HWND) {
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
    }
}

fn move_window(hwnd: HWND, rect: IRect) {
    unsafe {
        let _ = SetWindowPos(hwnd, None, rect.x, rect.y, rect.w, rect.h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn signed_lo(l: isize) -> i32 {
    (l as u32 & 0xFFFF) as u16 as i16 as i32
}

fn signed_hi(l: isize) -> i32 {
    ((l as u32 >> 16) & 0xFFFF) as u16 as i16 as i32
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let state = unsafe { &*(data as *const State) };
    if !state.alive.get() {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }
    match msg {
        WM_NCHITTEST => {
            let g = state.geometry.get();
            let (sx, sy) = (signed_lo(lparam.0), signed_hi(lparam.0));
            let local = ((sx - g.x) as f64, (sy - g.y) as f64);
            if state.wants_mouse(local.0, local.1) {
                LRESULT(HTCLIENT as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_HOTKEY => {
            state.push(OverlayEvent::Hotkey(wparam.0 as i32));
            LRESULT(0)
        }
        WM_MOUSEACTIVATE if state.no_activate => LRESULT(MA_NOACTIVATE as isize),
        WM_STYLECHANGING if wparam.0 as isize as i32 == GWL_EXSTYLE.0 => {
            let ss = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
            ss.styleNew = overlay_ex_style(ss.styleNew, state.input_transparent.get(), state.no_activate);
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_TIMER => match wparam.0 {
            TIMER_INPUT => {
                state.run_input_check(Instant::now());
                LRESULT(0)
            }
            TIMER_STYLE => {
                state.run_style_refresh(Instant::now());
                LRESULT(0)
            }
            TIMER_GEOMETRY => {
                state.run_geometry_check(Instant::now());
                LRESULT(0)
            }
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        },
        WM_DPICHANGED => {
            // winit resizes to the suggested rectangle; the overlay keeps
            // covering the desktop instead.
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            move_window(hwnd, state.geometry.get());
            r
        }
        WM_DISPLAYCHANGE => {
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            state.check_geometry();
            r
        }
        WM_NCDESTROY => {
            state.alive.set(false);
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
        }
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Error creating the overlay.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("winit could not create the overlay window: {0}")]
    Create(#[from] winit::error::OsError),
    #[error("the overlay window has no Win32 handle")]
    NoHandle,
    #[error("could not subclass the overlay window")]
    Subclass,
}

/// The transparent desktop overlay.
pub struct OverlayWindow {
    window: Arc<Window>,
    state: Rc<State>,
    mode: OverlayMode,
    backend: OverlayBackend,
    layered: RefCell<Option<LayeredBuffer>>,
}

impl OverlayWindow {
    /// Create (hidden), style and show the overlay. Call from
    /// `ApplicationHandler::resumed`.
    pub fn create(event_loop: &ActiveEventLoop, config: &OverlayConfig) -> Result<Self, OverlayError> {
        let geometry = config.geometry.unwrap_or_else(virtual_screen_geometry);
        let adventure = config.mode == OverlayMode::Adventure;
        let no_activate = config.no_activate.unwrap_or(!adventure);
        let composition = config.backend == OverlayBackend::Composition;
        let attrs = Window::default_attributes()
            .with_title(config.title.clone())
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_visible(false)
            .with_active(!no_activate)
            .with_window_level(if adventure { WindowLevel::Normal } else { WindowLevel::AlwaysOnTop })
            .with_position(PhysicalPosition::new(geometry.x, geometry.y))
            .with_inner_size(PhysicalSize::new(geometry.w.max(1) as u32, geometry.h.max(1) as u32))
            .with_skip_taskbar(true)
            .with_no_redirection_bitmap(composition)
            .with_undecorated_shadow(false)
            .with_drag_and_drop(false)
            .with_corner_preference(CornerPreference::DoNotRound)
            .with_class_name(WINDOW_CLASS);
        let window = Arc::new(event_loop.create_window(attrs)?);
        let hwnd = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => HWND(h.hwnd.get() as *mut core::ffi::c_void),
            _ => return Err(OverlayError::NoHandle),
        };
        let state = Rc::new(State {
            hwnd,
            adventure,
            no_activate,
            follow_desktop: config.geometry.is_none(),
            composition,
            geometry: Cell::new(geometry),
            // Companion starts out letting the desktop keep its mouse; the
            // input timer takes it back only over something to grab.
            input_transparent: Cell::new(!adventure),
            auto_input: Cell::new(config.auto_input_transparency),
            hit_test: RefCell::new(None),
            last_hit: Cell::new(false),
            events: RefCell::new(VecDeque::new()),
            waker: RefCell::new(None),
            hotkeys: RefCell::new(HashMap::new()),
            alive: Cell::new(true),
            next_input: Cell::new(Instant::now()),
            next_style: Cell::new(Instant::now()),
            next_geometry: Cell::new(Instant::now()),
        });
        let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, Rc::as_ptr(&state) as usize) };
        if !ok.as_bool() {
            return Err(OverlayError::Subclass);
        }
        unsafe {
            SetTimer(Some(hwnd), TIMER_INPUT, INPUT_CHECK_MS, None);
            SetTimer(Some(hwnd), TIMER_STYLE, STYLE_REFRESH_MS, None);
            SetTimer(Some(hwnd), TIMER_GEOMETRY, SCREEN_GEOMETRY_CHECK_MS, None);
        }
        state.apply_styles();
        // The exact geometry, whatever winit made of position + inner size.
        move_window(hwnd, geometry);
        window.set_visible(true);
        state.apply_styles();
        Ok(Self { window, state, mode: config.mode, backend: config.backend, layered: RefCell::new(None) })
    }

    /// The winit window (clone the `Arc` for `wgpu::Instance::create_surface`).
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn hwnd(&self) -> HWND {
        self.state.hwnd
    }

    /// The handle as an integer (for `exclude_hwnd` and the frame policy).
    pub fn hwnd_raw(&self) -> isize {
        self.state.hwnd.0 as isize
    }

    pub fn mode(&self) -> OverlayMode {
        self.mode
    }

    pub fn backend(&self) -> OverlayBackend {
        self.backend
    }

    /// The window rectangle: physical pixels, virtual-desktop coordinates.
    pub fn geometry(&self) -> IRect {
        self.state.geometry.get()
    }

    /// Install the hit test: `f(x, y)` with overlay-local physical
    /// coordinates returns whether the overlay wants the mouse there
    /// (`manager.wants_mouse`, `_adventure_captures_mouse`). Keep it cheap;
    /// it runs on every `WM_NCHITTEST` and every 50 ms.
    pub fn set_hit_test(&self, f: impl FnMut(f64, f64) -> bool + 'static) {
        *self.state.hit_test.borrow_mut() = Some(Box::new(f));
    }

    pub fn clear_hit_test(&self) {
        *self.state.hit_test.borrow_mut() = None;
    }

    /// Called whenever an [`OverlayEvent`] is queued (wake a winit loop with
    /// an `EventLoopProxy` here).
    pub fn set_waker(&self, f: impl Fn() + 'static) {
        *self.state.waker.borrow_mut() = Some(Box::new(f));
    }

    pub fn drain_events(&self) -> Vec<OverlayEvent> {
        self.state.events.borrow_mut().drain(..).collect()
    }

    /// Let every mouse event fall through the whole window, or none.
    pub fn set_input_transparent(&self, transparent: bool) -> bool {
        self.state.set_input_transparent(transparent)
    }

    pub fn is_input_transparent(&self) -> bool {
        self.state.input_transparent.get()
    }

    /// Turn the DC-79 timer on or off (off: the caller drives
    /// [`Self::set_input_transparent`] itself).
    pub fn set_auto_input_transparency(&self, on: bool) {
        self.state.auto_input.set(on);
    }

    /// Run the input, style and geometry checks that are due and return when
    /// the next one is. Call it on every pass of the event loop: while the
    /// overlay redraws every frame, `WM_TIMER` never arrives (see the module
    /// documentation).
    pub fn service_timers(&self) -> Instant {
        self.state.service_timers(Instant::now())
    }

    /// Run the DC-79 decision now instead of waiting for the timer.
    pub fn update_input_transparency(&self) {
        self.state.update_input_transparency();
    }

    /// Re-apply layered/tool/topmost styles (`apply_click_through`).
    pub fn apply_click_through(&self) -> bool {
        self.state.apply_styles()
    }

    /// Re-check the monitors now (normally every 1.5 s).
    pub fn sync_geometry(&self) {
        self.state.check_geometry();
    }

    /// The cursor in overlay-local physical pixels.
    pub fn cursor_local(&self) -> Option<(f64, f64)> {
        let g = self.geometry();
        cursor_pos().map(|(x, y)| ((x - g.x) as f64, (y - g.y) as f64))
    }

    /// Move the OS pointer to an overlay-local point (the cursor-trapping
    /// silk). Physical pixels throughout, so no DPI conversion is needed.
    pub fn set_cursor_local(&self, x: f64, y: f64) -> bool {
        let g = self.geometry();
        set_cursor_pos(dc_core::math::py_round(g.x as f64 + x) as i32, dc_core::math::py_round(g.y as f64 + y) as i32)
    }

    /// `RegisterHotKey(hwnd, id, MOD_NOREPEAT | modifiers, vk)`.
    pub fn register_hotkey(&self, id: i32, modifiers: u32, vk: u32) -> bool {
        self.unregister_hotkey(id);
        let ok = unsafe { RegisterHotKey(Some(self.hwnd()), id, HOT_KEY_MODIFIERS(MOD_NOREPEAT | modifiers), vk) };
        match ok {
            Ok(()) => {
                self.state.hotkeys.borrow_mut().insert(id, (modifiers, vk));
                true
            }
            Err(_) => {
                log::warn!(target: "engine", "Could not register the release hotkey (taken by another program?)");
                false
            }
        }
    }

    pub fn unregister_hotkey(&self, id: i32) {
        if self.state.hotkeys.borrow_mut().remove(&id).is_some() {
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd()), id);
            }
        }
    }

    /// `_register_release_hotkey`: the adventure release key system-wide.
    /// Companion mode registers nothing.
    pub fn register_release_hotkey(&self, binding: &str) -> bool {
        self.unregister_hotkey(RELEASE_HOTKEY_ID);
        if self.mode != OverlayMode::Adventure {
            return false;
        }
        match virtual_key_for(binding) {
            Some(vk) => self.register_hotkey(RELEASE_HOTKEY_ID, 0, vk),
            None => false,
        }
    }

    /// Give the overlay the keyboard (adventure mode's `activateWindow`).
    pub fn activate(&self) {
        self.window.focus_window();
    }

    /// Software path: show a premultiplied RGBA frame of `width x height`
    /// (normally the geometry's size) with `UpdateLayeredWindow`. Pixels
    /// with alpha 0 are click-through by themselves on this path.
    pub fn present_premultiplied(&self, rgba: &[u8], width: u32, height: u32) -> bool {
        self.present_premultiplied_rects(rgba, width, height, None)
    }

    /// [`OverlayWindow::present_premultiplied`] for a frame of which only
    /// `dirty` changed since the last one shown (`None`: all of it). Only
    /// those rects are copied into the window's bitmap, and the window is
    /// told to update their bounding rect (`prcDirty`); the rest keeps the
    /// pixels it has. A new size always shows the whole frame.
    pub fn present_premultiplied_rects(&self, rgba: &[u8], width: u32, height: u32, dirty: Option<&[IRect]>) -> bool {
        if self.backend != OverlayBackend::Layered || rgba.len() < width as usize * height as usize * 4 {
            return false;
        }
        let mut dirty = dirty;
        let mut slot = self.layered.borrow_mut();
        if slot.as_ref().map(|b| (b.width, b.height) != (width, height)).unwrap_or(true) {
            *slot = LayeredBuffer::new(width, height);
            dirty = None;
        }
        let Some(buf) = slot.as_mut() else { return false };
        let g = self.geometry();
        let bounds = match dirty {
            None => {
                buf.fill_from_rgba(rgba);
                None
            }
            Some(rects) => {
                let mut bounds: Option<RECT> = None;
                for r in rects {
                    let Some(r) = buf.fill_rect_from_rgba(rgba, r) else { continue };
                    bounds = Some(match bounds {
                        None => r,
                        Some(b) => RECT {
                            left: b.left.min(r.left),
                            top: b.top.min(r.top),
                            right: b.right.max(r.right),
                            bottom: b.bottom.max(r.bottom),
                        },
                    });
                }
                // Nothing changed: the window already shows this frame.
                let Some(b) = bounds else { return true };
                Some(b)
            }
        };
        buf.present(self.hwnd(), (g.x, g.y), bounds)
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        let hwnd = self.state.hwnd;
        unsafe {
            for id in self.state.hotkeys.borrow().keys() {
                let _ = UnregisterHotKey(Some(hwnd), *id);
            }
            for t in [TIMER_INPUT, TIMER_STYLE, TIMER_GEOMETRY] {
                let _ = KillTimer(Some(hwnd), t);
            }
            if self.state.alive.get() {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
            }
        }
        self.state.alive.set(false);
    }
}

/// A top-down 32-bit DIB section used by the software path.
struct LayeredBuffer {
    width: u32,
    height: u32,
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
}

impl LayeredBuffer {
    fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, HGDIOBJ(bitmap.0));
            Some(Self { width, height, dc, bitmap, old, bits: bits as *mut u8 })
        }
    }

    fn fill_from_rgba(&mut self, rgba: &[u8]) {
        let n = self.width as usize * self.height as usize * 4;
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, n) };
        for (d, s) in dst.chunks_exact_mut(4).zip(rgba[..n].chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
    }

    /// Copy one rect of `rgba` (clamped to the bitmap); the rect copied,
    /// or `None` when it was empty.
    fn fill_rect_from_rgba(&mut self, rgba: &[u8], r: &IRect) -> Option<RECT> {
        let (w, h) = (self.width as i32, self.height as i32);
        let (x0, x1) = (r.x.clamp(0, w), r.right().clamp(0, w));
        let (y0, y1) = (r.y.clamp(0, h), r.bottom().clamp(0, h));
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let n = self.width as usize * self.height as usize * 4;
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, n) };
        let stride = self.width as usize * 4;
        for y in y0 as usize..y1 as usize {
            let row = y * stride + x0 as usize * 4..y * stride + x1 as usize * 4;
            for (d, s) in dst[row.clone()].chunks_exact_mut(4).zip(rgba[row].chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
        }
        Some(RECT { left: x0, top: y0, right: x1, bottom: y1 })
    }

    /// `UpdateLayeredWindowIndirect`, updating only `dirty` when given.
    fn present(&self, hwnd: HWND, pos: (i32, i32), dirty: Option<RECT>) -> bool {
        let dst = POINT { x: pos.0, y: pos.1 };
        let size = SIZE { cx: self.width as i32, cy: self.height as i32 };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            let screen = GetDC(None);
            let info = UPDATELAYEREDWINDOWINFO {
                cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                hdcDst: screen,
                pptDst: &dst,
                psize: &size,
                hdcSrc: self.dc,
                pptSrc: &src,
                crKey: COLORREF(0),
                pblend: &blend,
                dwFlags: ULW_ALPHA,
                prcDirty: dirty.as_ref().map_or(std::ptr::null(), |d| d as *const RECT),
            };
            let r = UpdateLayeredWindowIndirect(hwnd, &info);
            ReleaseDC(None, screen);
            r.as_bool()
        }
    }
}

impl Drop for LayeredBuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Read the current extended style (diagnostics and tests).
pub fn current_ex_style(hwnd: HWND) -> u32 {
    ex_style(hwnd)
}

/// Force an extended style (diagnostics and tests).
pub fn force_ex_style(hwnd: HWND, style: u32) {
    set_ex_style(hwnd, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lparam_points_are_signed() {
        let l = ((-5i32 as u32 & 0xFFFF) | ((-1200i32 as u32 & 0xFFFF) << 16)) as isize;
        assert_eq!((signed_lo(l), signed_hi(l)), (-5, -1200));
        let l = (1919 | (1079 << 16)) as isize;
        assert_eq!((signed_lo(l), signed_hi(l)), (1919, 1079));
    }

    #[test]
    fn config_defaults() {
        let c = OverlayConfig::new(OverlayMode::Companion, OverlayBackend::Composition);
        assert!(c.auto_input_transparency);
        assert_eq!(c.no_activate, None);
        assert_eq!(c.title, WINDOW_TITLE);
        assert_eq!(RELEASE_HOTKEY_ID, 0xD0C1);
        assert_eq!(WM_HOTKEY_MSG, WM_HOTKEY);
    }

    /// The live run of 2026-10-25: WM_TIMER never arrived while the overlay
    /// drew every vsync, so the window never took the mouse over a spider.
    /// The event loop's pass must run the checks on its own schedule.
    #[test]
    fn service_timers_runs_due_checks_without_wm_timer() {
        if cursor_pos().is_none() {
            return; // no interactive desktop to read the cursor from
        }
        let start = Instant::now();
        let state = State {
            hwnd: HWND::default(),
            adventure: false,
            no_activate: true,
            follow_desktop: false,
            composition: false,
            geometry: Cell::new(IRect::new(0, 0, 100, 100)),
            input_transparent: Cell::new(true),
            auto_input: Cell::new(true),
            hit_test: RefCell::new(None),
            last_hit: Cell::new(false),
            events: RefCell::new(VecDeque::new()),
            waker: RefCell::new(None),
            hotkeys: RefCell::new(HashMap::new()),
            alive: Cell::new(true),
            next_input: Cell::new(start),
            next_style: Cell::new(start),
            next_geometry: Cell::new(start),
        };
        let asked = Rc::new(Cell::new(0));
        let counter = asked.clone();
        *state.hit_test.borrow_mut() = Some(Box::new(move |_, _| {
            counter.set(counter.get() + 1);
            true
        }));
        let input = Duration::from_millis(INPUT_CHECK_MS.into());

        assert_eq!(state.service_timers(start), start + input, "the input check is the next one due");
        assert_eq!(asked.get(), 1);
        assert!(!state.input_transparent.get(), "the window takes the mouse where the hit test wants it");
        assert_eq!(state.events.borrow().front(), Some(&OverlayEvent::InputTransparencyChanged(false)));

        state.service_timers(start + input / 2);
        assert_eq!(asked.get(), 1, "not due yet");
        state.service_timers(start + input);
        assert_eq!(asked.get(), 2);
        assert_eq!(state.next_style.get(), start + Duration::from_millis(STYLE_REFRESH_MS.into()));

        state.auto_input.set(false);
        state.service_timers(start + input * 2);
        assert_eq!(asked.get(), 2, "auto input off leaves the decision to the caller");
    }
}
