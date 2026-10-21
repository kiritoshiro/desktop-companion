//! The windows behind the UI layer: one winit top-level window per open
//! [`Surface`], made and driven inside the engine's own event loop with
//! dc-ui's [`HostedWindow`] (egui-winit + egui-wgpu, one shared wgpu device).
//!
//! * Menus are frameless, always-on-top pop-ups out of the taskbar, sized to
//!   their content and put at the cursor on the monitor under it (flipped
//!   left or up at the edge of its work area, as `QMenu.popup` did). They
//!   close on Esc, when focus leaves them, or on any mouse press outside
//!   them (watched globally, so this works even if Windows would not give
//!   the pop-up the focus).
//! * Dialogs are framed, always-on-top windows centred on the monitor under
//!   the cursor (or the menu that opened them), shrunk to fit its work area.
//!
//! Always-on-top windows made after the overlay sit above it.

use std::collections::HashSet;

use dc_core::IRect;
use dc_ui::host::{GpuShare, HostedWindow, headless_pass};
use egui::Vec2;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowId, WindowLevel};

use super::surfaces::{Placement, Sizing, SurfaceId, UiCore};

/// Space kept free around a dialog inside the monitor's work area.
const DIALOG_MARGIN: i32 = 40;

/// `QMenu.popup(pos)`: the top-left corner at `at`, flipped to the left of
/// or above it where the menu would leave the work area, and kept inside it.
pub fn popup_position(at: (i32, i32), size: (i32, i32), work: IRect) -> (i32, i32) {
    let (w, h) = size;
    let right = work.x + work.w;
    let bottom = work.y + work.h;
    let mut x = at.0;
    let mut y = at.1;
    if x + w > right {
        x = at.0 - w;
    }
    if y + h > bottom {
        y = at.1 - h;
    }
    (x.clamp(work.x, (right - w).max(work.x)), y.clamp(work.y, (bottom - h).max(work.y)))
}

/// A dialog's size, shrunk to fit inside the work area with a margin.
pub fn fit_size(size: (i32, i32), work: IRect) -> (i32, i32) {
    (size.0.min(work.w - 2 * DIALOG_MARGIN).max(200), size.1.min(work.h - 2 * DIALOG_MARGIN).max(120))
}

/// Centred in the work area.
pub fn centered_position(size: (i32, i32), work: IRect) -> (i32, i32) {
    (work.x + (work.w - size.0) / 2, work.y + (work.h - size.1) / 2)
}

fn physical(size: Vec2, scale: f64) -> (i32, i32) {
    ((size.x as f64 * scale).ceil() as i32, (size.y as f64 * scale).ceil() as i32)
}

struct Hosted {
    surface: SurfaceId,
    window: HostedWindow,
    popup: bool,
    had_focus: bool,
    title: String,
    scale: f64,
    work: IRect,
    /// The pop-up's anchor (its own top-left, before flipping).
    anchor: Option<(i32, i32)>,
    size: (i32, i32),
}

/// The winit side of the UI layer.
#[derive(Default)]
pub struct Shell {
    gpu: GpuShare,
    windows: Vec<Hosted>,
    /// Surfaces whose window could not be made (dismissed, not retried).
    failed: HashSet<SurfaceId>,
    buttons_down: bool,
}

impl Shell {
    pub fn owns(&self, id: WindowId) -> bool {
        self.windows.iter().any(|h| h.window.id() == id)
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// One event of one of our windows.
    pub fn window_event(&mut self, core: &mut UiCore, el: &ActiveEventLoop, id: WindowId, event: &WindowEvent) {
        let Some(hosted) = self.windows.iter_mut().find(|h| h.window.id() == id) else { return };
        let surface = hosted.surface;
        match event {
            WindowEvent::CloseRequested => core.dismiss(surface),
            WindowEvent::RedrawRequested => {
                hosted.window.redraw(&mut |ctx| core.draw(surface, ctx));
            }
            WindowEvent::Focused(true) => {
                hosted.had_focus = true;
                hosted.window.on_event(event);
            }
            WindowEvent::Focused(false) => {
                hosted.window.on_event(event);
                if hosted.popup && hosted.had_focus {
                    core.dismiss(surface);
                }
            }
            other => {
                hosted.window.on_event(other);
            }
        }
        self.sync_windows(core, el);
    }

    /// Once per engine tick.
    pub fn frame(&mut self, core: &mut UiCore, el: &ActiveEventLoop) {
        core.promote_pending();
        self.sync_windows(core, el);
        // A press anywhere outside an open pop-up closes it.
        let down = dc_platform::input::left_mouse_button_down() || dc_platform::input::right_mouse_button_down();
        if down
            && !self.buttons_down
            && let Some((cx, cy)) = dc_platform::input::cursor_pos()
        {
            let outside: Vec<SurfaceId> = self
                .windows
                .iter()
                .filter(|h| h.popup && !contains(h.window.window(), cx, cy))
                .map(|h| h.surface)
                .collect();
            for surface in outside {
                core.dismiss(surface);
            }
        }
        self.buttons_down = down;
        for h in &mut self.windows {
            let dirty = core.surface_mut(h.surface).is_some_and(|s| std::mem::take(&mut s.dirty));
            h.window.poll(dirty);
        }
        self.sync_windows(core, el);
    }

    /// Make windows for new surfaces, drop those of closed ones, and follow
    /// size and title changes.
    fn sync_windows(&mut self, core: &mut UiCore, el: &ActiveEventLoop) {
        self.windows.retain(|h| core.surface(h.surface).is_some());
        self.failed.retain(|id| core.surface(*id).is_some());
        let new: Vec<SurfaceId> = core
            .surfaces
            .iter()
            .filter(|s| s.ready && !self.failed.contains(&s.id) && !self.windows.iter().any(|h| h.surface == s.id))
            .map(|s| s.id)
            .collect();
        for id in new {
            if let Err(e) = self.open(core, el, id) {
                log::warn!("Could not open a {} window: {e}", core.surface(id).map_or("UI", |s| s.view.kind()));
                self.failed.insert(id);
                core.dismiss(id);
            }
        }
        self.windows.retain(|h| core.surface(h.surface).is_some());
        for h in &mut self.windows {
            let Some(s) = core.surface(h.surface) else { continue };
            if s.title != h.title {
                h.title = s.title.clone();
                h.window.window().set_title(&h.title);
            }
            if matches!(s.sizing, Sizing::Content | Sizing::FitHeight(_)) {
                let wanted = physical(s.size, h.scale);
                let wanted = if h.popup { wanted } else { fit_size(wanted, h.work) };
                if (wanted.0 - h.size.0).abs() > 1 || (wanted.1 - h.size.1).abs() > 1 {
                    h.size = wanted;
                    let _ = h.window.window().request_inner_size(PhysicalSize::new(wanted.0 as u32, wanted.1 as u32));
                    let at = match h.anchor {
                        Some(anchor) => popup_position(anchor, wanted, h.work),
                        None => centered_position(wanted, h.work),
                    };
                    h.window.window().set_outer_position(PhysicalPosition::new(at.0, at.1));
                    h.window.window().request_redraw();
                }
            }
        }
    }

    fn open(&mut self, core: &mut UiCore, el: &ActiveEventLoop, id: SurfaceId) -> Result<(), String> {
        let Some(s) = core.surface(id) else { return Ok(()) };
        let point = match s.placement {
            Placement::AtPoint(x, y) => Some((x.round() as i32, y.round() as i32)),
            Placement::Centered(Some((x, y))) => Some((x.round() as i32, y.round() as i32)),
            // `window_placement.center_on_primary`: the main monitor.
            Placement::Centered(None) => None,
        };
        let monitor = point
            .and_then(|(x, y)| dc_platform::monitors::monitor_at(x, y))
            .or_else(|| {
                let list = dc_platform::monitors::monitors();
                list.get(dc_platform::monitors::primary_index(&list)).cloned()
            })
            .ok_or("no monitor")?;
        let scale = monitor.scale();
        let work = monitor.work;
        let ctx = s.ctx.clone();
        let popup = s.popup;
        let sizing = s.sizing;
        // Measure content-sized surfaces before the window exists.
        let mut textures = egui::TexturesDelta::default();
        if let Sizing::Content | Sizing::FitHeight(_) = sizing {
            let width = match sizing {
                Sizing::FitHeight(w) => w,
                _ => 2000.0,
            };
            textures = headless_pass(&ctx, egui::vec2(width, 4000.0), scale as f32, &mut |c| core.draw(id, c));
            // A second pass settles widths that depend on the first.
            textures.append(headless_pass(&ctx, egui::vec2(width, 4000.0), scale as f32, &mut |c| core.draw(id, c)));
        }
        let Some(s) = core.surface(id) else {
            textures.clear();
            return Ok(());
        };
        let wanted = physical(s.size, scale);
        let size = if popup { wanted } else { fit_size(wanted, work) };
        let (anchor, at) = match s.placement {
            Placement::AtPoint(x, y) => {
                let anchor = (x.round() as i32, y.round() as i32);
                (Some(anchor), popup_position(anchor, size, work))
            }
            Placement::Centered(_) => (None, centered_position(size, work)),
        };
        let mut attributes = Window::default_attributes()
            .with_title(s.title.clone())
            .with_inner_size(PhysicalSize::new(size.0 as u32, size.1 as u32))
            .with_position(PhysicalPosition::new(at.0, at.1))
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(true)
            .with_visible(true);
        if popup {
            attributes = attributes.with_decorations(false).with_resizable(false).with_skip_taskbar(true);
        } else {
            attributes = attributes.with_resizable(!matches!(sizing, Sizing::FitHeight(_)));
            if let Some(image) = dc_ui::assets::UiAssets::app_icon_rgba()
                && let Ok(icon) = winit::window::Icon::from_rgba(image.rgba, image.width, image.height)
            {
                attributes = attributes.with_window_icon(Some(icon));
            }
        }
        let title = s.title.clone();
        let window = HostedWindow::open(el, attributes, ctx, &mut self.gpu, textures).map_err(|e| e.to_string())?;
        window.window().focus_window();
        self.windows.push(Hosted { surface: id, window, popup, had_focus: false, title, scale, work, anchor, size });
        // The click that opened a menu is not a click outside it.
        self.buttons_down =
            dc_platform::input::left_mouse_button_down() || dc_platform::input::right_mouse_button_down();
        Ok(())
    }
}

fn contains(window: &Window, x: i32, y: i32) -> bool {
    let Ok(pos) = window.outer_position() else { return false };
    let size = window.outer_size();
    x >= pos.x && y >= pos.y && x < pos.x + size.width as i32 && y < pos.y + size.height as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: IRect = IRect::new(0, 0, 1920, 1040);

    #[test]
    fn a_menu_opens_below_right_of_the_cursor() {
        assert_eq!(popup_position((500, 300), (300, 400), WORK), (500, 300));
    }

    #[test]
    fn a_menu_flips_at_the_right_and_bottom_edges() {
        assert_eq!(popup_position((1800, 900), (300, 400), WORK), (1500, 500));
    }

    #[test]
    fn a_menu_on_a_second_monitor_stays_on_it() {
        let right = IRect::new(1920, 0, 2560, 1400);
        let (x, y) = popup_position((1925, 10), (300, 400), right);
        assert_eq!((x, y), (1925, 10));
        let left = IRect::new(-1280, 0, 1280, 984);
        let (x, _) = popup_position((-10, 100), (300, 400), left);
        assert!((-1280..=-300).contains(&x), "{x}");
    }

    #[test]
    fn a_too_tall_menu_is_kept_on_screen() {
        let (_, y) = popup_position((100, 500), (300, 2000), WORK);
        assert_eq!(y, 0);
    }

    #[test]
    fn dialogs_fit_and_centre_in_the_work_area() {
        let size = fit_size((1200, 780), IRect::new(0, 0, 1280, 680));
        assert_eq!(size, (1200, 600));
        assert_eq!(centered_position(size, IRect::new(100, 0, 1280, 680)), (140, 40));
    }
}
