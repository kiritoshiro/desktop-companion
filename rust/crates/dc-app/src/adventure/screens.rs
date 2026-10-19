//! Which monitors a raid may use and the picture Reclaim the desktop fights
//! on: the Windows half of `OverlayWindow.__init__` in Adventure
//! (`playable_screens`, `available_rects_local`, `capture_desktop`).

use dc_adventure::adventure_profile::{load_profile, profile_path};
use dc_art::desktop_surface::{SurfaceShot, SurfaceSnapshot};
use dc_core::{IRect, Rect};
use dc_platform::desktop_capture::{DesktopSnapshot, RgbaImage, capture_desktop};
use dc_platform::monitors::{MonitorInfo, available_rects_local, monitors, primary_index};
use dc_platform::screen_activity::{monitor_states, playable_screens};
use dc_render::Image;
use dc_world::ScreenRect;
use std::collections::HashSet;
use std::path::Path;

/// The monitors a raid runs on.
pub struct PlayableScreens {
    /// Usable areas (without the taskbar), overlay-local.
    pub rects: Vec<ScreenRect>,
    pub primary: usize,
    /// The main monitor's usable area, overlay-local (`primary_rect_local`).
    pub primary_rect: Option<IRect>,
    /// The monitors themselves, for the Reclaim capture.
    pub monitors: Vec<MonitorInfo>,
}

/// The main monitor's usable area in overlay pixels.
pub fn primary_rect_local(origin: (i32, i32), list: &[MonitorInfo]) -> Option<IRect> {
    if list.is_empty() {
        return None;
    }
    let w = list[primary_index(list)].work;
    Some(IRect::new(w.x - origin.0, w.y - origin.1, w.w, w.h))
}

/// Only monitors really in use: not asleep, off, or switched off by the
/// player on the Adventure page (`screen_activity.py`). The main one always.
pub fn playable(origin: (i32, i32), state_dir: &Path) -> PlayableScreens {
    let all = monitors();
    let primary_rect = primary_rect_local(origin, &all);
    if all.is_empty() {
        return PlayableScreens { rects: Vec::new(), primary: 0, primary_rect, monitors: all };
    }
    let profile = load_profile(&profile_path(state_dir));
    let disabled: HashSet<String> = profile.disabled_screens.iter().cloned().collect();
    let names: Vec<String> = all.iter().map(|m| m.name.clone()).collect();
    let keep = playable_screens(&names, primary_index(&all), &disabled, &monitor_states());
    let list: Vec<MonitorInfo> = keep.into_iter().map(|i| all[i].clone()).collect();
    let (rects, primary) = available_rects_local(origin, &list);
    let mut rects: Vec<ScreenRect> = rects.into_iter().map(|r| ScreenRect::new(r.x, r.y, r.w, r.h)).collect();
    let mut primary = primary;
    if rects.is_empty()
        && let Some(p) = primary_rect
    {
        rects = vec![ScreenRect::new(p.x as f64, p.y as f64, p.w as f64, p.h as f64)];
        primary = 0;
    }
    PlayableScreens { rects, primary, primary_rect, monitors: list }
}

fn image(rgba: &RgbaImage, ratio: f64) -> Option<Image> {
    let mut image = Image::from_straight_rgba(rgba.width, rgba.height, rgba.pixels.clone())?;
    image.set_device_pixel_ratio(ratio as f32);
    Some(image)
}

/// dc-platform's capture as the frozen desktop. The wallpaper is not
/// decoded (dc-platform has no JPEG decoder), so a broken window shows the
/// plain desktop colour instead of the wallpaper behind it.
pub fn surface_snapshot(snapshot: &DesktopSnapshot) -> SurfaceSnapshot {
    let mut screens = Vec::new();
    for shot in &snapshot.screens {
        let Some(picture) = image(&shot.image, shot.device_pixel_ratio) else { continue };
        let r = shot.rect;
        screens.push(SurfaceShot {
            rect: Rect::new(r.x, r.y, r.w, r.h),
            image: picture,
            wallpaper: shot.wallpaper.as_ref().and_then(|w| image(w, shot.device_pixel_ratio)),
        });
    }
    SurfaceSnapshot {
        screens,
        windows: snapshot.windows.clone(),
        primary: snapshot.primary,
        window_images: snapshot.window_images.iter().map(|i| i.as_ref().and_then(|i| image(i, 1.0))).collect(),
    }
}

/// A picture of the playable monitors taken now (before the overlay shows).
pub fn capture(origin: (i32, i32), list: &[MonitorInfo]) -> Option<SurfaceSnapshot> {
    let snapshot = capture_desktop(origin, list, false)?;
    let surface = surface_snapshot(&snapshot);
    (!surface.screens.is_empty()).then_some(surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_platform::desktop_capture::ScreenShot;

    #[test]
    fn a_capture_becomes_the_frozen_desktop() {
        let shot = ScreenShot {
            rect: Rect::new(0.0, 0.0, 40.0, 30.0),
            image: RgbaImage::filled(80, 60, [10, 20, 30, 255]),
            device_pixel_ratio: 2.0,
            wallpaper: None,
        };
        let snapshot = DesktopSnapshot {
            screens: vec![shot],
            windows: vec![Rect::new(5.0, 5.0, 10.0, 10.0)],
            primary: 0,
            window_images: vec![Some(RgbaImage::filled(10, 10, [200, 0, 0, 255]))],
            wallpaper_path: None,
        };
        let s = surface_snapshot(&snapshot);
        assert_eq!(s.screens.len(), 1);
        assert_eq!(s.screens[0].image.size(), (80, 60));
        assert_eq!(s.screens[0].image.device_pixel_ratio(), 2.0);
        assert_eq!(s.screens[0].image.pixel(3, 3).r, 10);
        assert_eq!(s.windows.len(), 1);
        assert!(s.window_images[0].is_some());
    }

    #[test]
    fn the_main_monitor_area_is_overlay_local() {
        let m = MonitorInfo {
            handle: 1,
            name: "\\\\.\\DISPLAY1".into(),
            rect: IRect::new(-1920, 0, 1920, 1080),
            work: IRect::new(-1920, 0, 1920, 1040),
            dpi: 96,
            primary: true,
        };
        assert_eq!(primary_rect_local((-1920, 0), &[m]), Some(IRect::new(0, 0, 1920, 1040)));
        assert_eq!(primary_rect_local((0, 0), &[]), None);
    }
}
