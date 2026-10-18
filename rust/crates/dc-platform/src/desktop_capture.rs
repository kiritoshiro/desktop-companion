//! Port of app/desktop_capture.py: a frozen picture of the desktop for
//! "Reclaim the desktop".
//!
//! Taken once, before the overlay shows: a screenshot of every monitor at
//! its native resolution (GDI `BitBlt` from the screen DC with
//! `CAPTUREBLT`), the visible windows front to back with a `PrintWindow`
//! picture of each (at most [`MAX_PICTURES`], skipping hung windows and
//! blank results), and the wallpaper path. Nothing is written to disk or
//! sent anywhere.
//!
//! Rust differences: images are plain RGBA8 buffers ([`RgbaImage`],
//! opaque), since dc-render owns textures. The wallpaper is returned as a
//! path because decoding JPEGs is not a dependency here; once the caller has
//! decoded it, [`apply_wallpaper`] scales and crops it like Windows' "Fill"
//! (nearest-neighbour rather than Qt's smooth scaling). `synthetic_snapshot`
//! fills rectangles by pixel centre without antialiasing.

use std::path::PathBuf;

use dc_core::Rect;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsHungAppWindow, IsIconic, IsWindowVisible, SPI_GETDESKWALLPAPER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::BOOL;

use crate::monitors::{MonitorInfo, primary_index};
use crate::overlay_win32::WS_EX_TOOLWINDOW;
use crate::win::{class_name, ex_style, is_dwm_cloaked, rect_from_dwm, rect_from_user32};

/// Pictures are taken of at most this many windows, front first.
pub const MAX_PICTURES: usize = 12;
/// `PW_RENDERFULLCONTENT`: include DirectComposition content.
pub const PW_RENDERFULLCONTENT: u32 = 0x2;
/// PrintWindow is refused above this many pixels.
pub const MAX_PICTURE_PIXELS: i64 = 40_000_000;

/// An RGBA8 picture, rows top to bottom, `width * 4` bytes per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; width as usize * height as usize * 4] }
    }

    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut img = Self::new(width, height);
        for px in img.pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
        img
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    pub fn put(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    /// From 32-bit BGRX rows (GDI), forcing alpha opaque (`Format_RGB32`).
    pub fn from_bgrx(width: u32, height: u32, bgrx: &[u8]) -> Self {
        let mut pixels = Vec::with_capacity(bgrx.len());
        for px in bgrx.chunks_exact(4) {
            pixels.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        Self { width, height, pixels }
    }

    /// The part inside `(x, y, w, h)`, clipped to the image.
    pub fn crop(&self, x: i32, y: i32, w: i32, h: i32) -> RgbaImage {
        let x0 = x.clamp(0, self.width as i32);
        let y0 = y.clamp(0, self.height as i32);
        let x1 = (x + w).clamp(x0, self.width as i32);
        let y1 = (y + h).clamp(y0, self.height as i32);
        let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut out = RgbaImage::new(cw, ch);
        for row in 0..ch {
            let src = (((y0 as u32 + row) * self.width + x0 as u32) * 4) as usize;
            let dst = (row * cw * 4) as usize;
            out.pixels[dst..dst + cw as usize * 4].copy_from_slice(&self.pixels[src..src + cw as usize * 4]);
        }
        out
    }

    /// Fill the pixels whose centres lie inside `rect` (QPainter.fillRect
    /// without antialiasing, approximately).
    pub fn fill_rect(&mut self, rect: Rect, rgba: [u8; 4]) {
        let x0 = ((rect.x - 0.5).ceil().max(0.0)) as u32;
        let y0 = ((rect.y - 0.5).ceil().max(0.0)) as u32;
        let x1 = ((rect.x + rect.w - 0.5).ceil().max(0.0) as u32).min(self.width);
        let y1 = ((rect.y + rect.h - 0.5).ceil().max(0.0) as u32).min(self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                self.put(x, y, rgba);
            }
        }
    }
}

/// One monitor's picture.
#[derive(Clone, Debug)]
pub struct ScreenShot {
    /// Overlay-local rectangle in the caller's coordinate space.
    pub rect: Rect,
    /// Native pixels.
    pub image: RgbaImage,
    /// `image.width / rect.w` (QImage.devicePixelRatio).
    pub device_pixel_ratio: f64,
    /// The wallpaper at the same size, once [`apply_wallpaper`] has run.
    pub wallpaper: Option<RgbaImage>,
}

#[derive(Clone, Debug, Default)]
pub struct DesktopSnapshot {
    pub screens: Vec<ScreenShot>,
    /// Overlay-local window rectangles, front to back.
    pub windows: Vec<Rect>,
    pub primary: usize,
    /// Each window's own picture, in step with `windows`.
    pub window_images: Vec<Option<RgbaImage>>,
    /// Where Windows keeps the current wallpaper, if anywhere.
    pub wallpaper_path: Option<PathBuf>,
}

/// A monitor in the caller's coordinate space: `x, y, w, h` and the ratio
/// from it to physical pixels (1.0 when the caller works in physical
/// pixels, the monitor's scale for Qt-style logical pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenGeometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub dpr: f64,
}

impl ScreenGeometry {
    pub fn of(m: &MonitorInfo, logical: bool) -> Self {
        let dpr = if logical { m.scale() } else { 1.0 };
        Self { x: m.rect.x as f64, y: m.rect.y as f64, w: m.rect.w as f64 / dpr, h: m.rect.h as f64 / dpr, dpr }
    }
}

/// A physical point to overlay-local coordinates (`physical_to_local`).
pub fn physical_to_local(px: f64, py: f64, screens: &[ScreenGeometry], origin: (f64, f64)) -> (f64, f64) {
    for g in screens {
        let dpr = if g.dpr > 0.0 { g.dpr } else { 1.0 };
        if g.x <= px && px < g.x + g.w * dpr && g.y <= py && py < g.y + g.h * dpr {
            return (g.x + (px - g.x) / dpr - origin.0, g.y + (py - g.y) / dpr - origin.1);
        }
    }
    (px - origin.0, py - origin.1)
}

/// Every monitor as it is now. `logical` selects Qt-style logical pixels for
/// the rectangles; images are always native. `None` when there are no
/// monitors.
pub fn capture_desktop(origin: (i32, i32), monitors: &[MonitorInfo], logical: bool) -> Option<DesktopSnapshot> {
    if monitors.is_empty() {
        return None;
    }
    let mut shots = Vec::new();
    for m in monitors {
        let g = ScreenGeometry::of(m, logical);
        let image = grab_screen(m.rect.x, m.rect.y, m.rect.w, m.rect.h).unwrap_or_else(|| {
            log::warn!(target: "desktop_capture", "Could not capture {}", m.name);
            RgbaImage::filled(m.rect.w.max(1) as u32, m.rect.h.max(1) as u32, [0, 0, 0, 255])
        });
        let ratio = image.width as f64 / g.w.max(1.0);
        shots.push(ScreenShot {
            rect: Rect::new(g.x - origin.0 as f64, g.y - origin.1 as f64, g.w, g.h),
            image,
            device_pixel_ratio: if ratio > 0.0 { ratio } else { 1.0 },
            wallpaper: None,
        });
    }
    let geoms: Vec<ScreenGeometry> = monitors.iter().map(|m| ScreenGeometry::of(m, logical)).collect();
    let found = windows_front_to_back((origin.0 as f64, origin.1 as f64), &geoms, true);
    Some(DesktopSnapshot {
        screens: shots,
        windows: found.iter().map(|(r, _)| *r).collect(),
        primary: primary_index(monitors),
        window_images: found.into_iter().map(|(_, img)| img).collect(),
        wallpaper_path: wallpaper_path(),
    })
}

/// A screen region in physical pixels, as RGBA.
pub fn grab_screen(x: i32, y: i32, w: i32, h: i32) -> Option<RgbaImage> {
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let mut bmi = dib_header(w, h);
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let result = match CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) if !bits.is_null() => {
                let old = SelectObject(mem, HGDIOBJ(bitmap.0));
                let ok = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
                let out = ok.then(|| {
                    let slice = std::slice::from_raw_parts(bits as *const u8, w as usize * h as usize * 4);
                    RgbaImage::from_bgrx(w as u32, h as u32, slice)
                });
                SelectObject(mem, old);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                out
            }
            _ => None,
        };
        let _ = &mut bmi;
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}

fn dib_header(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The current wallpaper file, if Windows reports one that exists.
pub fn wallpaper_path() -> Option<PathBuf> {
    let mut buf = [0u16; 520];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    let path = crate::win::from_wide(&buf);
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    path.exists().then_some(path)
}

/// The wallpaper scaled to cover `w x h` and centre-cropped, as Windows'
/// "Fill" (`_fill`). Nearest-neighbour sampling.
pub fn fill_cover(wallpaper: &RgbaImage, w: u32, h: u32) -> Option<RgbaImage> {
    if wallpaper.is_empty() || w == 0 || h == 0 {
        return None;
    }
    let scale = (w as f64 / wallpaper.width as f64).max(h as f64 / wallpaper.height as f64);
    let sw = (wallpaper.width as f64 * scale).round().max(w as f64) as u32;
    let sh = (wallpaper.height as f64 * scale).round().max(h as f64) as u32;
    let ox = (sw - w) / 2;
    let oy = (sh - h) / 2;
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        let sy = (((y + oy) as f64 + 0.5) / scale).floor().min(wallpaper.height as f64 - 1.0) as u32;
        for x in 0..w {
            let sx = (((x + ox) as f64 + 0.5) / scale).floor().min(wallpaper.width as f64 - 1.0) as u32;
            let mut px = wallpaper.pixel(sx, sy);
            px[3] = 255;
            out.put(x, y, px);
        }
    }
    Some(out)
}

/// Give every screen its "Fill" wallpaper from an already decoded picture.
pub fn apply_wallpaper(snapshot: &mut DesktopSnapshot, wallpaper: &RgbaImage) {
    for shot in &mut snapshot.screens {
        shot.wallpaper = fill_cover(wallpaper, shot.image.width, shot.image.height);
    }
}

/// Rectangles of the visible top-level windows, front to back,
/// overlay-local (`list_windows`). Minimised, cloaked and tool windows, this
/// process's own windows and the desktop itself are skipped.
pub fn list_windows(origin: (f64, f64), screens: &[ScreenGeometry]) -> Vec<Rect> {
    windows_front_to_back(origin, screens, false).into_iter().map(|(r, _)| r).collect()
}

struct WindowWalk<'a> {
    origin: (f64, f64),
    screens: &'a [ScreenGeometry],
    pictures: bool,
    pid: u32,
    found: Vec<(Rect, Option<RgbaImage>)>,
}

unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
    let walk = unsafe { &mut *(data.0 as *mut WindowWalk) };
    visit_one(hwnd, walk);
    BOOL(1)
}

fn visit_one(hwnd: HWND, walk: &mut WindowWalk) {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return;
        }
    }
    if ex_style(hwnd) & WS_EX_TOOLWINDOW != 0 {
        return;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == walk.pid {
        return;
    }
    let name = class_name(hwnd);
    if matches!(name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return;
    }
    if is_dwm_cloaked(hwnd) {
        return;
    }
    let Some(rect) = rect_from_dwm(hwnd).or_else(|| rect_from_user32(hwnd)) else { return };
    if rect.right - rect.left < 40 || rect.bottom - rect.top < 30 {
        return;
    }
    let (x0, y0) = physical_to_local(rect.left as f64, rect.top as f64, walk.screens, walk.origin);
    let (x1, y1) = physical_to_local(rect.right as f64, rect.bottom as f64, walk.screens, walk.origin);
    let mut picture = None;
    if walk.pictures && walk.found.len() < MAX_PICTURES && !unsafe { IsHungAppWindow(hwnd) }.as_bool() {
        picture = print_window(hwnd, (rect.left, rect.top, rect.right, rect.bottom));
    }
    walk.found.push((Rect::new(x0, y0, x1 - x0, y1 - y0), picture));
}

fn windows_front_to_back(
    origin: (f64, f64),
    screens: &[ScreenGeometry],
    pictures: bool,
) -> Vec<(Rect, Option<RgbaImage>)> {
    let mut walk = WindowWalk { origin, screens, pictures, pid: std::process::id(), found: Vec::new() };
    if unsafe { EnumWindows(Some(visit), LPARAM(&mut walk as *mut WindowWalk as isize)) }.is_err() {
        return Vec::new();
    }
    walk.found
}

/// A window's own picture, even where other windows cover it, cropped to
/// its visible frame (`frame`, physical `l, t, r, b`). `None` if Windows
/// gives back nothing.
pub fn print_window(hwnd: HWND, frame: (i32, i32, i32, i32)) -> Option<RgbaImage> {
    let whole = rect_from_user32(hwnd)?;
    let (w, h) = (whole.right - whole.left, whole.bottom - whole.top);
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > MAX_PICTURE_PIXELS {
        return None;
    }
    let image = unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let printed = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        // GetDIBits wants the bitmap deselected.
        SelectObject(mem, old);
        let mut out = None;
        if printed {
            let mut bmi = dib_header(w, h);
            let mut buf = vec![0u8; w as usize * h as usize * 4];
            let lines = GetDIBits(mem, bitmap, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
            if lines != 0 {
                out = Some(RgbaImage::from_bgrx(w as u32, h as u32, &buf));
            }
        }
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        out
    }?;
    let (left, top) = (frame.0 - whole.left, frame.1 - whole.top);
    let image = image.crop(left.max(0), top.max(0), frame.2 - frame.0, frame.3 - frame.1);
    if is_blank(&image) { None } else { Some(image) }
}

/// True when every sampled pixel is the same colour: nothing was drawn.
pub fn is_blank(image: &RgbaImage) -> bool {
    if image.width < 2 || image.height < 2 {
        return true;
    }
    let first = image.pixel(0, 0);
    for j in 1..12 {
        for i in 1..12 {
            if image.pixel(image.width * i / 12, image.height * j / 12) != first {
                return false;
            }
        }
    }
    true
}

fn fake_word(img: &mut RgbaImage, x: f64, y: f64, letters: i32, seed: i32) -> f64 {
    let ink = [30, 30, 30, 255];
    for i in 0..letters {
        let lx = x + (i * 7) as f64;
        let tall = if (seed + i) % 3 == 0 { 10.0 } else { 7.0 };
        img.fill_rect(Rect::new(lx, y + 10.0 - tall, 1.4, tall), ink);
        img.fill_rect(Rect::new(lx + 3.6, y + 3.0, 1.4, 7.0), ink);
        img.fill_rect(Rect::new(lx, y + 3.0 + ((seed + i) % 2 * 6) as f64, 5.0, 1.2), ink);
    }
    (letters * 7) as f64
}

/// A made-up desktop for tests and previews: a blue desktop, pale windows,
/// optionally rows of word-like marks, and a green gradient wallpaper.
pub fn synthetic_snapshot(rects: &[Rect], windows: &[Rect], primary: usize, text_rows: i32) -> DesktopSnapshot {
    let mut shots = Vec::new();
    for rect in rects {
        let (w, h) = (rect.w as u32, rect.h as u32);
        let mut image = RgbaImage::filled(w, h, [38, 92, 150, 255]);
        for win in windows {
            let local = win.translated(-rect.x, -rect.y);
            image.fill_rect(local, [236, 236, 232, 255]);
            image.fill_rect(Rect::new(local.x, local.y, local.w, 28.0), [52, 56, 64, 255]);
            for row in 0..text_rows {
                let y = local.y + 50.0 + (row * 26) as f64;
                if y + 16.0 > local.bottom() {
                    break;
                }
                let mut x = local.x + 20.0;
                let mut word = 0;
                loop {
                    let letters = 3 + (row * 7 + word * 5) % 6;
                    if x + (letters * 7) as f64 > local.right() - 20.0 {
                        break;
                    }
                    x += fake_word(&mut image, x, y, letters, row + word) + 8.0;
                    word += 1;
                }
            }
        }
        let mut paper = RgbaImage::new(w, h);
        for y in 0..h {
            let t = if h == 0 { 0.0 } else { (y as f64 + 0.5) / h as f64 };
            let lerp = |a: f64, b: f64| (a + (b - a) * t).round() as u8;
            let c = [lerp(40.0, 20.0), lerp(110.0, 50.0), lerp(90.0, 70.0), 255];
            for x in 0..w {
                paper.put(x, y, c);
            }
        }
        shots.push(ScreenShot { rect: *rect, image, device_pixel_ratio: 1.0, wallpaper: Some(paper) });
    }
    DesktopSnapshot {
        screens: shots,
        windows: windows.to_vec(),
        primary,
        window_images: vec![None; windows.len()],
        wallpaper_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_detection() {
        let flat = RgbaImage::filled(100, 80, [1, 2, 3, 255]);
        assert!(is_blank(&flat));
        let mut marked = flat.clone();
        marked.put(50, 40, [9, 9, 9, 255]);
        assert!(!is_blank(&marked), "a sampled pixel differs");
        assert!(is_blank(&RgbaImage::filled(1, 50, [0, 0, 0, 255])));
    }

    #[test]
    fn crop_clips_to_the_image() {
        let mut img = RgbaImage::filled(10, 10, [0, 0, 0, 255]);
        img.put(9, 9, [255, 0, 0, 255]);
        let c = img.crop(5, 5, 20, 20);
        assert_eq!((c.width, c.height), (5, 5));
        assert_eq!(c.pixel(4, 4), [255, 0, 0, 255]);
        assert!(img.crop(20, 20, 5, 5).is_empty());
    }

    #[test]
    fn physical_to_local_uses_the_screen_ratio() {
        let screens = [
            ScreenGeometry { x: 0.0, y: 0.0, w: 1280.0, h: 720.0, dpr: 1.5 },
            ScreenGeometry { x: 1920.0, y: 0.0, w: 1920.0, h: 1080.0, dpr: 1.0 },
        ];
        assert_eq!(physical_to_local(960.0, 540.0, &screens, (0.0, 0.0)), (640.0, 360.0));
        assert_eq!(physical_to_local(2000.0, 10.0, &screens, (0.0, 0.0)), (2000.0, 10.0));
        assert_eq!(physical_to_local(-50.0, 10.0, &screens, (-100.0, 0.0)), (50.0, 10.0));
    }

    #[test]
    fn fill_cover_crops_the_centre() {
        let mut wp = RgbaImage::filled(4, 2, [0, 0, 0, 255]);
        wp.put(1, 0, [255, 255, 255, 255]);
        wp.put(2, 1, [255, 255, 255, 255]);
        let out = fill_cover(&wp, 2, 2).unwrap();
        assert_eq!((out.width, out.height), (2, 2));
        assert_eq!(out.pixel(0, 0), [255, 255, 255, 255]);
        assert_eq!(out.pixel(1, 1), [255, 255, 255, 255]);
        assert_eq!(out.pixel(1, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn synthetic_snapshot_draws_windows_and_words() {
        let snap =
            synthetic_snapshot(&[Rect::new(0.0, 0.0, 400.0, 300.0)], &[Rect::new(50.0, 40.0, 300.0, 200.0)], 0, 3);
        let img = &snap.screens[0].image;
        assert_eq!(img.pixel(5, 5), [38, 92, 150, 255]);
        assert_eq!(img.pixel(60, 50), [52, 56, 64, 255], "title bar");
        assert_eq!(img.pixel(300, 230), [236, 236, 232, 255]);
        let inked = (0..img.width)
            .flat_map(|x| (0..img.height).map(move |y| (x, y)))
            .any(|(x, y)| img.pixel(x, y) == [30, 30, 30, 255]);
        assert!(inked, "no word marks were drawn");
        assert!(snap.screens[0].wallpaper.is_some());
    }

    #[test]
    fn smoke_capture_and_window_list() {
        let monitors = crate::monitors::monitors();
        let primary = &monitors[primary_index(&monitors)];
        let shot = grab_screen(primary.rect.x, primary.rect.y, 64, 32).expect("screen grab failed");
        assert_eq!((shot.width, shot.height), (64, 32));
        assert!(shot.pixels.chunks_exact(4).all(|p| p[3] == 255));
        let geoms: Vec<_> = monitors.iter().map(|m| ScreenGeometry::of(m, false)).collect();
        let rects = list_windows((0.0, 0.0), &geoms);
        for r in &rects {
            assert!(r.w >= 40.0 && r.h >= 30.0);
        }
        let _ = wallpaper_path();
    }
}
