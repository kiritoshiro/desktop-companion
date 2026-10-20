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
    let states = monitor_states();
    let keep = playable_screens(&names, primary_index(&all), &disabled, &states);
    if keep.len() < all.len() {
        let dropped: Vec<_> = (0..all.len())
            .filter(|i| !keep.contains(i))
            .map(|i| (names[i].as_str(), states.get(&names[i]), disabled.contains(&names[i])))
            .collect();
        log::info!("Screens left out of the mission (name, power state, switched off): {dropped:?}");
    }
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
/// `_wallpaper_image`: the wallpaper file, decoded.
pub fn load_wallpaper(path: &Path) -> Result<image::RgbaImage, String> {
    image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| e.to_string())?
        .decode()
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}

/// `_fill`: the wallpaper scaled smoothly to cover `w x h` and centre-cropped,
/// as Windows' "Fill" and Qt's `KeepAspectRatioByExpanding`.
pub fn fill(paper: &image::RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (pw, ph) = (paper.width().max(1) as f64, paper.height().max(1) as f64);
    let scale = (w as f64 / pw).max(h as f64 / ph);
    let sw = ((pw * scale).round() as u32).max(w);
    let sh = ((ph * scale).round() as u32).max(h);
    let scaled = image::imageops::resize(paper, sw, sh, image::imageops::FilterType::Triangle);
    let cropped = image::imageops::crop_imm(&scaled, (sw - w) / 2, (sh - h) / 2, w, h).to_image();
    let mut pixels = cropped.into_raw();
    for px in pixels.chunks_exact_mut(4) {
        px[3] = 255;
    }
    RgbaImage { width: w, height: h, pixels }
}

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
    let mut snapshot = capture_desktop(origin, list, false)?;
    let started = std::time::Instant::now();
    match snapshot.wallpaper_path.as_deref().map(load_wallpaper) {
        Some(Ok(paper)) => {
            for shot in &mut snapshot.screens {
                shot.wallpaper = Some(fill(&paper, shot.image.width, shot.image.height));
            }
            log::info!("Wallpaper {}x{} filled in {:?}", paper.width(), paper.height(), started.elapsed());
        }
        Some(Err(e)) => log::warn!("Wallpaper not read ({e}); a broken window shows a plain desktop"),
        None => {}
    }
    let surface = surface_snapshot(&snapshot);
    log::info!(
        "Captured {} of {} playable screen(s): {:?}",
        surface.screens.len(),
        list.len(),
        snapshot.screens.iter().map(|s| (s.rect, s.image.width, s.image.height)).collect::<Vec<_>>()
    );
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
    fn the_wallpaper_fills_each_screen_like_windows() {
        // 4:1 picture, left half red, right half blue, on a 2:1 screen: it
        // is scaled to cover the height and the middle is kept.
        let mut paper = image::RgbaImage::new(400, 100);
        for (x, _, px) in paper.enumerate_pixels_mut() {
            *px = if x < 200 { image::Rgba([255, 0, 0, 128]) } else { image::Rgba([0, 0, 255, 128]) };
        }
        let out = fill(&paper, 100, 50);
        assert_eq!((out.width, out.height), (100, 50));
        assert_eq!(out.pixels.len(), 100 * 50 * 4);
        assert_eq!(out.pixel(5, 25), [255, 0, 0, 255], "left edge is the picture's red half, opaque");
        assert_eq!(out.pixel(94, 25), [0, 0, 255, 255]);
        // Only the middle 100 of 200 scaled columns: the halves meet mid-screen.
        let mid = out.pixel(50, 25);
        assert!(mid[0] > 0 && mid[2] > 0, "{mid:?}");
    }

    #[test]
    fn a_png_wallpaper_decodes_and_garbage_is_an_error() {
        let dir = std::env::temp_dir().join(format!("dc-wallpaper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("paper.png");
        image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255])).save(&png).unwrap();
        // No extension, as Windows' TranscodedWallpaper: the format is sniffed.
        let bare = dir.join("TranscodedWallpaper");
        std::fs::copy(&png, &bare).unwrap();
        for path in [&png, &bare] {
            let paper = load_wallpaper(path).unwrap();
            assert_eq!(paper.dimensions(), (8, 4));
        }
        let junk = dir.join("junk.jpg");
        std::fs::write(&junk, b"not a picture").unwrap();
        assert!(load_wallpaper(&junk).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real wallpaper of this machine, when there is one (JPEG on stock
    /// Windows).
    #[test]
    fn this_machines_wallpaper_decodes() {
        let Some(path) = dc_platform::desktop_capture::wallpaper_path() else { return };
        let paper = load_wallpaper(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(paper.width() > 0 && paper.height() > 0);
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
