//! `QImage` / `QPixmap` replacement: premultiplied RGBA8 pixels
//! (`Format_ARGB32_Premultiplied` semantics, byte order R, G, B, A).
//!
//! Clones share pixels (`Arc`); mutation copies on write. Every image has an
//! `id` and a `version` so the GPU backend can cache textures and re-upload
//! only when pixels changed.

use crate::paint::{premultiply, unpremultiply};
use dc_core::Color;
use std::path::Path as FsPath;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub enum ImageError {
    Io(std::io::Error),
    Decode(String),
    Encode(String),
    Size,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Io(e) => write!(f, "io error: {e}"),
            ImageError::Decode(e) => write!(f, "decode error: {e}"),
            ImageError::Encode(e) => write!(f, "encode error: {e}"),
            ImageError::Size => write!(f, "invalid image size"),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<std::io::Error> for ImageError {
    fn from(e: std::io::Error) -> Self {
        ImageError::Io(e)
    }
}

/// `Qt::TransformationMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TransformMode {
    #[default]
    Fast,
    Smooth,
}

#[derive(Clone)]
pub struct Image {
    id: u64,
    version: u64,
    width: u32,
    height: u32,
    dpr: f32,
    data: Arc<Vec<u8>>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{}, id {}, v{})", self.width, self.height, self.id, self.version)
    }
}

impl Default for Image {
    /// A null image (`QImage()`).
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl Image {
    /// Transparent image (`QImage(w, h, ARGB32_Premultiplied)` + `fill(Qt.transparent)`).
    pub fn new(width: u32, height: u32) -> Self {
        Self::from_premultiplied(width, height, vec![0; width as usize * height as usize * 4]).unwrap()
    }

    pub fn filled(width: u32, height: u32, c: Color) -> Self {
        let mut img = Self::new(width, height);
        img.fill(c);
        img
    }

    /// Wraps premultiplied RGBA8 bytes (length must be `w * h * 4`).
    pub fn from_premultiplied(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize * 4).then(|| Self {
            id: next_id(),
            version: next_id(),
            width,
            height,
            dpr: 1.0,
            data: Arc::new(data),
        })
    }

    /// From straight (non-premultiplied) RGBA8 bytes.
    pub fn from_straight_rgba(width: u32, height: u32, mut data: Vec<u8>) -> Option<Self> {
        for px in data.as_chunks_mut::<4>().0 {
            let p = premultiply(Color::rgba(px[0], px[1], px[2], px[3]));
            px.copy_from_slice(&p);
        }
        Self::from_premultiplied(width, height, data)
    }

    /// From BGRA premultiplied bytes (what GDI / layered windows use).
    pub fn from_bgra_premultiplied(width: u32, height: u32, mut data: Vec<u8>) -> Option<Self> {
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        Self::from_premultiplied(width, height, data)
    }

    pub fn decode_png(bytes: &[u8]) -> Result<Self, ImageError> {
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|e| ImageError::Decode(e.to_string()))?
            .into_rgba8();
        let (w, h) = img.dimensions();
        Self::from_straight_rgba(w, h, img.into_raw()).ok_or(ImageError::Size)
    }

    /// `QImage(path)` for PNG files.
    pub fn load_png(path: impl AsRef<FsPath>) -> Result<Self, ImageError> {
        Self::decode_png(&std::fs::read(path)?)
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        let mut out = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut out);
        use image::ImageEncoder;
        enc.write_image(&self.to_straight_rgba(), self.width, self.height, image::ExtendedColorType::Rgba8)
            .map_err(|e| ImageError::Encode(e.to_string()))?;
        Ok(out)
    }

    pub fn save_png(&self, path: impl AsRef<FsPath>) -> Result<(), ImageError> {
        std::fs::write(path, self.encode_png()?)?;
        Ok(())
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// `QImage::isNull`.
    pub fn is_null(&self) -> bool {
        self.width == 0 || self.height == 0
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn device_pixel_ratio(&self) -> f32 {
        self.dpr
    }
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) {
        self.dpr = if dpr > 0.0 { dpr } else { 1.0 };
    }
    /// Size in logical pixels (`size / devicePixelRatio`).
    pub fn logical_size(&self) -> (f64, f64) {
        (self.width as f64 / self.dpr as f64, self.height as f64 / self.dpr as f64)
    }

    /// Premultiplied RGBA8 bytes, row-major, no padding.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable pixels; copies if shared and bumps the version.
    pub fn data_mut(&mut self) -> &mut [u8] {
        if Arc::get_mut(&mut self.data).is_none() {
            self.data = Arc::new((*self.data).clone());
            self.id = next_id();
        }
        self.version = next_id();
        Arc::get_mut(&mut self.data).unwrap()
    }

    pub fn into_data(self) -> Vec<u8> {
        Arc::try_unwrap(self.data).unwrap_or_else(|a| (*a).clone())
    }

    #[inline]
    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    /// `QImage::pixelColor`: straight colour; transparent when out of range.
    pub fn pixel(&self, x: u32, y: u32) -> Color {
        unpremultiply(self.pixel_premultiplied(x, y))
    }

    /// Raw premultiplied RGBA (what `QImage::pixel` returns for premultiplied formats).
    pub fn pixel_premultiplied(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let o = self.offset(x, y);
        [self.data[o], self.data[o + 1], self.data[o + 2], self.data[o + 3]]
    }

    /// `QImage::setPixelColor` with a straight colour.
    pub fn set_pixel(&mut self, x: u32, y: u32, c: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let o = self.offset(x, y);
        self.data_mut()[o..o + 4].copy_from_slice(&premultiply(c));
    }

    /// `QImage::fill(color)`.
    pub fn fill(&mut self, c: Color) {
        let p = premultiply(c);
        for px in self.data_mut().as_chunks_mut::<4>().0 {
            px.copy_from_slice(&p);
        }
    }

    /// `QImage::copy(x, y, w, h)`: areas outside the image come back transparent.
    pub fn copy(&self, x: i32, y: i32, w: u32, h: u32) -> Image {
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for row in 0..h as i32 {
            let sy = y + row;
            if sy < 0 || sy >= self.height as i32 {
                continue;
            }
            let x0 = x.max(0);
            let x1 = (x + w as i32).min(self.width as i32);
            if x1 <= x0 {
                continue;
            }
            let src = self.offset(x0 as u32, sy as u32);
            let len = (x1 - x0) as usize * 4;
            let dst = (row as usize * w as usize + (x0 - x) as usize) * 4;
            out[dst..dst + len].copy_from_slice(&self.data[src..src + len]);
        }
        let mut img = Image::from_premultiplied(w, h, out).unwrap();
        img.dpr = self.dpr;
        img
    }

    /// Alias of [`Image::copy`].
    pub fn sub_image(&self, x: i32, y: i32, w: u32, h: u32) -> Image {
        self.copy(x, y, w, h)
    }

    /// `scaled(w, h, IgnoreAspectRatio, mode)`.
    pub fn scaled(&self, w: u32, h: u32, mode: TransformMode) -> Image {
        match mode {
            TransformMode::Fast => self.scaled_nearest(w, h),
            TransformMode::Smooth => self.smooth_scaled(w, h),
        }
    }

    pub fn scaled_nearest(&self, w: u32, h: u32) -> Image {
        if self.is_null() || w == 0 || h == 0 {
            return Image::new(w, h);
        }
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for y in 0..h {
            let sy = ((y as u64 * self.height as u64) / h as u64) as u32;
            for x in 0..w {
                let sx = ((x as u64 * self.width as u64) / w as u64) as u32;
                let s = self.offset(sx, sy);
                let d = (y as usize * w as usize + x as usize) * 4;
                out[d..d + 4].copy_from_slice(&self.data[s..s + 4]);
            }
        }
        Image::from_premultiplied(w, h, out).unwrap()
    }

    /// `Qt::SmoothTransformation`: area averaging when shrinking, bilinear
    /// when enlarging (per axis), on premultiplied pixels.
    pub fn smooth_scaled(&self, w: u32, h: u32) -> Image {
        if self.is_null() || w == 0 || h == 0 {
            return Image::new(w, h);
        }
        let src: Vec<f32> = self.data.iter().map(|&b| b as f32).collect();
        let xs = weights(self.width as usize, w as usize);
        let ys = weights(self.height as usize, h as usize);
        // Horizontal pass: height x w.
        let sw = self.width as usize;
        let mut tmp = vec![0f32; self.height as usize * w as usize * 4];
        for y in 0..self.height as usize {
            for (x, ws) in xs.iter().enumerate() {
                let mut acc = [0f32; 4];
                for &(i, wt) in ws {
                    let o = (y * sw + i) * 4;
                    for c in 0..4 {
                        acc[c] += src[o + c] * wt;
                    }
                }
                tmp[(y * w as usize + x) * 4..][..4].copy_from_slice(&acc);
            }
        }
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for (y, ws) in ys.iter().enumerate() {
            for x in 0..w as usize {
                let mut acc = [0f32; 4];
                for &(i, wt) in ws {
                    let o = (i * w as usize + x) * 4;
                    for c in 0..4 {
                        acc[c] += tmp[o + c] * wt;
                    }
                }
                let d = (y * w as usize + x) * 4;
                let a = acc[3].round().clamp(0.0, 255.0);
                out[d + 3] = a as u8;
                for c in 0..3 {
                    out[d + c] = acc[c].round().clamp(0.0, a) as u8;
                }
            }
        }
        Image::from_premultiplied(w, h, out).unwrap()
    }

    /// `convertToFormat(Format_Grayscale8)`: Qt's `qGray` = (11r + 16g + 5b) / 32
    /// of the straight colour.
    pub fn to_gray8(&self) -> Vec<u8> {
        self.data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let c = unpremultiply([p[0], p[1], p[2], p[3]]);
                ((c.r as u32 * 11 + c.g as u32 * 16 + c.b as u32 * 5) / 32) as u8
            })
            .collect()
    }

    /// An opaque image from 8-bit grey values (`Format_Grayscale8` drawn as RGB).
    pub fn from_gray8(width: u32, height: u32, grey: &[u8]) -> Option<Image> {
        if grey.len() != width as usize * height as usize {
            return None;
        }
        let mut data = Vec::with_capacity(grey.len() * 4);
        for &g in grey {
            data.extend_from_slice(&[g, g, g, 255]);
        }
        Image::from_premultiplied(width, height, data)
    }

    /// Straight RGBA8 bytes (for PNG export or straight-alpha consumers).
    pub fn to_straight_rgba(&self) -> Vec<u8> {
        let mut v = self.data.to_vec();
        for px in v.as_chunks_mut::<4>().0 {
            let c = unpremultiply([px[0], px[1], px[2], px[3]]);
            px.copy_from_slice(&[c.r, c.g, c.b, c.a]);
        }
        v
    }

    /// BGRA premultiplied bytes (for `UpdateLayeredWindow` / DIB sections).
    pub fn to_bgra_premultiplied(&self) -> Vec<u8> {
        let mut v = self.data.to_vec();
        for px in v.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        v
    }

    /// `QPainter p(&image); ...; p.end()`: records the closure into a scene
    /// and rasterises it onto this image with the CPU backend.
    pub fn paint(&mut self, f: impl FnOnce(&mut crate::Canvas<'_>)) {
        let mut scene = crate::Scene::new();
        {
            let mut c = scene.canvas();
            f(&mut c);
        }
        crate::cpu::with_thread_renderer(|r| r.render_into(&scene, self, None));
    }
}

/// Resampling weights for one axis: for each destination index, the source
/// indices and weights (summing to 1).
fn weights(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            let mut v = Vec::new();
            if scale > 1.0 {
                // Box filter over [i*scale, (i+1)*scale).
                let a = i as f64 * scale;
                let b = a + scale;
                let mut j = a.floor() as usize;
                while (j as f64) < b && j < src {
                    let lo = a.max(j as f64);
                    let hi = b.min(j as f64 + 1.0);
                    if hi > lo {
                        v.push((j, ((hi - lo) / scale) as f32));
                    }
                    j += 1;
                }
            } else {
                let c = (i as f64 + 0.5) * scale - 0.5;
                let j0 = c.floor();
                let f = (c - j0) as f32;
                let clampi = |k: f64| k.clamp(0.0, src as f64 - 1.0) as usize;
                let (a, b) = (clampi(j0), clampi(j0 + 1.0));
                if a == b || f == 0.0 {
                    v.push((a, 1.0));
                } else {
                    v.push((a, 1.0 - f));
                    v.push((b, f));
                }
            }
            v
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_copy_and_versions() {
        let mut img = Image::new(4, 3);
        let v0 = img.version();
        img.set_pixel(1, 1, Color::rgba(200, 100, 50, 128));
        assert_ne!(img.version(), v0);
        assert_eq!(img.pixel_premultiplied(1, 1), premultiply(Color::rgba(200, 100, 50, 128)));
        assert_eq!(img.pixel(9, 9), Color::TRANSPARENT);
        let shared = img.clone();
        img.fill(Color::WHITE);
        assert_ne!(shared.id(), img.id());
        assert_eq!(shared.pixel(1, 1).a, 128);
        let sub = shared.copy(0, 0, 3, 3);
        assert_eq!(sub.pixel(1, 1).a, 128);
        let outside = shared.copy(-1, -1, 3, 3);
        assert_eq!(outside.pixel(0, 0), Color::TRANSPARENT);
        assert_eq!(outside.pixel(2, 2).a, 128);
    }

    #[test]
    fn smooth_scale_averages() {
        let mut img = Image::new(4, 1);
        img.set_pixel(0, 0, Color::WHITE);
        img.set_pixel(1, 0, Color::WHITE);
        let s = img.smooth_scaled(2, 1);
        assert_eq!(s.pixel_premultiplied(0, 0), [255, 255, 255, 255]);
        assert_eq!(s.pixel_premultiplied(1, 0), [0, 0, 0, 0]);
        let q = img.smooth_scaled(1, 1);
        assert_eq!(q.pixel_premultiplied(0, 0)[3], 128);
        let up = Image::filled(2, 2, Color::rgb(10, 20, 30)).smooth_scaled(5, 5);
        assert_eq!(up.pixel(3, 3), Color::rgb(10, 20, 30));
    }

    #[test]
    fn png_round_trip_and_gray() {
        let mut img = Image::new(3, 2);
        img.set_pixel(0, 0, Color::rgba(255, 0, 0, 255));
        img.set_pixel(2, 1, Color::rgba(0, 0, 255, 100));
        let back = Image::decode_png(&img.encode_png().unwrap()).unwrap();
        assert_eq!(back.data(), img.data());
        assert_eq!(img.to_gray8()[0], (255 * 11 / 32) as u8);
    }
}
