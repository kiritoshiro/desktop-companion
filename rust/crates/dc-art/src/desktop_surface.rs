//! Port of app/desktop_surface.py: the frozen desktop as something spiders
//! can damage (Reclaim the desktop).
//!
//! Each monitor is three layers, top to bottom: **glass and windows** (the
//! screenshot, with cracks drawn over it), **desktop** (the wallpaper, shown
//! where acid melted through a window) and **void** (what is left when the
//! desktop itself is eaten away). Acid on a window melts the window and shows
//! the desktop behind it; acid on the desktop melts the desktop and shows the
//! void. Words on the frozen screen can be eaten, letter by letter.
//!
//! Heavy landings crack the glass of one window at a time; at its breaking
//! point cracks race across the whole window ([`Break`]) and it falls away as
//! shards ([`Fall`]), showing the windows captured behind it
//! (`_reveal_behind`). Cracks on the bare desktop and acid eating it wear down
//! [`DesktopSurface::desktop_hp`]; at none the desktop breaks.
//!
//! This is only a picture. Simulation (stress, health, words) and painting
//! live together here as in the Python; the Reclaim mission drives it through
//! `melt`, `crack`, `eat`, `update`, the word queries and `paint`.
//!
//! Rust notes: the Qt version composed layers and copied clipped images with
//! `QPainter`; here those steps (`_compose`, the region-clipped copies of
//! `_draw_on_glass` / `_finish_break`, `eat`'s `SourceAtop` fill) are direct
//! integer pixel loops with Qt's own source-over arithmetic, clipped by pixel
//! centre exactly like Qt's aliased rectangle clips. Shapes (acid, cracks,
//! the void, revealed windows) are painted with `dc-render`. Words are
//! referred to by index into [`DesktopSurface::words`] (the Python compared
//! objects). Window regions are [`Region`]s (rectangle unions), which is all
//! the Python's `QPainterPath.subtracted` was ever given.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f64::consts::TAU;

use dc_core::{Color, PyRandom, Rect, Vec2};
use dc_render::{Brush, Canvas, CompositionMode, Image, LinearGradient, Path, Pen, RadialGradient, Region, RenderHint};

use crate::common::round_cap_pen;
use crate::qt::{div255, fill_rect_pixels, intersected, is_empty, over, to_aligned_rect};
use crate::screen_text::{MAX_WORDS, find_text_boxes};

/// How much one crack of full force stresses a window's glass; at 1.0 it breaks.
pub const STRESS_PER_CRACK: f64 = 0.2;
/// How much desktop health one crack of full force takes.
pub const DESKTOP_PER_CRACK: f64 = 0.06;
/// What share of the desktop acid must eat to take all of its health.
pub const ACID_EATS_ALL: f64 = 0.55;
/// The break: cracks race across the whole window, then it falls away.
pub const BREAK_SECONDS: f64 = 0.45;
/// How far one crack's rays can run from the blow.
pub const CRACK_REACH: f64 = 520.0;
pub const BREAK_HOLD: f64 = 0.12;
pub const SHARD_SECONDS: f64 = 2.4;
/// The word finder's buckets, for asking "which words are near here?" fast.
pub const WORD_CELL: f64 = 160.0;
/// Spacing of the grid on which the desktop's remaining area is sampled.
pub const INTEGRITY_STEP: f64 = 32.0;

// -- the snapshot ---------------------------------------------------------------------

/// One monitor's picture (`desktop_capture.ScreenShot`).
#[derive(Clone, Debug)]
pub struct SurfaceShot {
    /// Overlay-local, logical pixels.
    pub rect: Rect,
    /// Native pixels, device pixel ratio set (`Image::from_straight_rgba` of
    /// dc-platform's capture, then `set_device_pixel_ratio`).
    pub image: Image,
    /// The wallpaper at the same size, or None (a plain blue desktop).
    pub wallpaper: Option<Image>,
}

/// The frozen desktop (`desktop_capture.DesktopSnapshot`).
#[derive(Clone, Debug, Default)]
pub struct SurfaceSnapshot {
    pub screens: Vec<SurfaceShot>,
    /// Overlay-local window rectangles, front to back.
    pub windows: Vec<Rect>,
    pub primary: usize,
    /// Each window's own picture, in step with `windows`.
    pub window_images: Vec<Option<Image>>,
}

/// Fill the pixels a non-antialiased `QPainter.fillRect(rect, colour)` covers
/// (opaque colour, so a plain overwrite).
fn fill_opaque(image: &mut Image, rect: Rect, c: Color) {
    let (w, h) = (image.width() as i64, image.height() as i64);
    let (x0, y0, x1, y1) = fill_rect_pixels(rect);
    let (x0, y0, x1, y1) = (x0.clamp(0, w), y0.clamp(0, h), x1.clamp(0, w), y1.clamp(0, h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let px = [c.r, c.g, c.b, 255];
    let data = image.data_mut();
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * w + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
}

/// `_fake_word`: letter-like strokes a text line tall, drawn without a font.
fn fake_word(image: &mut Image, x: f64, y: f64, letters: i64, seed: i64) -> f64 {
    let ink = Color::rgb(30, 30, 30);
    for i in 0..letters {
        let lx = x + (i * 7) as f64;
        let tall = if (seed + i) % 3 == 0 { 10.0 } else { 7.0 };
        fill_opaque(image, Rect::new(lx, y + 10.0 - tall, 1.4, tall), ink);
        fill_opaque(image, Rect::new(lx + 3.6, y + 3.0, 1.4, 7.0), ink);
        fill_opaque(image, Rect::new(lx, y + 3.0 + ((seed + i) % 2 * 6) as f64, 5.0, 1.2), ink);
    }
    (letters * 7) as f64
}

/// `synthetic_snapshot(rects, windows, primary, text_rows)`: a made-up
/// desktop for tests and previews -- a blue desktop, pale windows, rows of
/// word-like marks, and a green gradient wallpaper. The rectangles are filled
/// with Qt's own non-antialiased pixel rule, so the pictures (and the words
/// found on them) match the Python's.
pub fn synthetic_snapshot(rects: &[Rect], windows: &[Rect], primary: usize, text_rows: i64) -> SurfaceSnapshot {
    let mut screens = Vec::new();
    for rect in rects {
        let (w, h) = (rect.w as u32, rect.h as u32);
        let mut image = Image::filled(w, h, Color::rgb(38, 92, 150));
        for win in windows {
            let local = win.translated(-rect.x, -rect.y);
            fill_opaque(&mut image, local, Color::rgb(236, 236, 232));
            fill_opaque(&mut image, Rect::new(local.x, local.y, local.w, 28.0), Color::rgb(52, 56, 64));
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
        let mut paper = Image::new(w, h);
        paper.paint(|p| {
            let g = LinearGradient::new(0.0, 0.0, 0.0, h as f64)
                .with_stop(0.0, Color::rgb(40, 110, 90))
                .with_stop(1.0, Color::rgb(20, 50, 70));
            p.fill_rect(Rect::new(0.0, 0.0, w as f64, h as f64), g);
        });
        screens.push(SurfaceShot { rect: *rect, image, wallpaper: Some(paper) });
    }
    SurfaceSnapshot { screens, windows: windows.to_vec(), primary, window_images: vec![None; windows.len()] }
}

// -- the parts -----------------------------------------------------------------------

/// A word on the frozen screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// Overlay-local.
    pub rect: Rect,
    /// The background it is erased to.
    pub colour: Color,
    pub screen: usize,
    pub eaten: f64,
}

impl Word {
    pub fn done(&self) -> bool {
        self.eaten >= 1.0
    }
    pub fn centre(&self) -> (f64, f64) {
        self.rect.center()
    }
}

/// A falling triangle of glass (screen-local).
#[derive(Clone, Debug)]
pub struct Shard {
    pub polygon: [Vec2; 3],
    pub cx: f64,
    pub cy: f64,
    pub vx: f64,
    pub vy: f64,
    pub spin: f64,
    pub age: f64,
    pub dx: f64,
    pub dy: f64,
    pub angle: f64,
}

/// One open window's glass; the frontmost is order 0.
#[derive(Clone, Debug)]
pub struct Pane {
    /// Overlay-local.
    pub rect: Rect,
    /// Its own picture, or None (drawn plainly).
    pub image: Option<Image>,
    pub order: usize,
    pub stress: f64,
    pub broken: bool,
}

/// One web line of a break: ends (overlay-local) and its distance from the blow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WebLine {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub dist: f64,
}

/// Cracks racing across a region before it falls away.
#[derive(Clone, Debug)]
pub struct Break {
    /// Overlay-local.
    pub region: Region,
    /// Nearest first.
    pub lines: Vec<WebLine>,
    pub reach: f64,
    /// The breaking window's index in `panes`; None: the desktop itself.
    pub pane: Option<usize>,
    pub age: f64,
    pub drawn: usize,
}

/// A broken piece of glass falling away as shards.
#[derive(Clone, Debug)]
pub struct Fall {
    /// Screen-local, transparent outside the piece.
    pub picture: Image,
    pub shards: Vec<Shard>,
}

/// One monitor's layers.
#[derive(Clone, Debug)]
pub struct ScreenLayers {
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub top: Image,
    pub under: Image,
    pub void: Image,
    pub glass: Image,
    /// The composed picture painted each frame.
    pub frame: Image,
    pub shattered: bool,
    pub falls: Vec<Fall>,
}

impl ScreenLayers {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }
    pub fn local(&self, x: f64, y: f64) -> (f64, f64) {
        (x - self.x, y - self.y)
    }
    fn ratio(&self) -> f64 {
        ratio_of(&self.top)
    }
}

/// What acid melted, or what a crack broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Window,
    Desktop,
}

/// The layer seen at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seen {
    Window,
    Desktop,
    Void,
}

type Buckets = (HashMap<(i64, i64), usize>, Vec<Vec<usize>>);

struct Living {
    showing: Vec<usize>,
    buckets: Buckets,
    reach: f64,
}

/// `QRectF::contains(QPointF)`: edges included.
fn qcontains(r: &Rect, x: f64, y: f64) -> bool {
    r.x <= x && x <= r.right() && r.y <= y && y <= r.bottom()
}

fn ratio_of(image: &Image) -> f64 {
    let r = image.device_pixel_ratio() as f64;
    if r > 0.0 { r } else { 1.0 }
}

/// `QPainter p(&image)` on an image with a device pixel ratio: logical coordinates.
fn paint_logical(image: &mut Image, f: impl FnOnce(&mut Canvas)) {
    let r = ratio_of(image);
    image.paint(|p| {
        if r != 1.0 {
            p.scale(r, r);
        }
        f(p)
    });
}

/// The device pixels (x0, y0, x1, y1) whose centres lie inside logical `r`
/// (Qt's aliased rectangle clip), clamped to the image.
fn device_span(r: &Rect, ratio: f64, w: u32, h: u32) -> (usize, usize, usize, usize) {
    let lo = |v: f64| ((v * ratio - 0.5).ceil().max(0.0)) as usize;
    let hi = |v: f64, max: u32| ((v * ratio - 0.5).ceil().max(0.0) as usize).min(max as usize);
    (lo(r.x).min(w as usize), lo(r.y).min(h as usize), hi(r.right(), w), hi(r.bottom(), h))
}

/// The DesktopSurface: every monitor's layers, the windows, breaks and words.
pub struct DesktopSurface {
    pub rng: PyRandom,
    /// Kept for Retry: a fresh surface from the same picture.
    pub snapshot: SurfaceSnapshot,
    pub panes: Vec<Pane>,
    /// The unbroken windows, front to back.
    pub windows: Vec<Rect>,
    pub breaks: Vec<Break>,
    pub desktop_cracks: f64,
    pub desktop_broken: bool,
    pub hit_flash: f64,
    pub find_text: bool,
    pub primary: usize,
    pub layers: Vec<ScreenLayers>,
    pub words: Vec<Word>,
    /// Bumped on every recomposition (for texture uploads).
    pub version: u64,
    integrity: Cell<Option<f64>>,
    living: RefCell<Option<Living>>,
}

impl DesktopSurface {
    /// `DesktopSurface(snapshot, seed, find_text)`. `seed` None seeds from entropy.
    pub fn new(snapshot: SurfaceSnapshot, seed: Option<i64>, find_text: bool) -> Self {
        let rng = match seed {
            Some(s) => PyRandom::from_int(s as i128),
            None => PyRandom::from_entropy(),
        };
        let panes = snapshot
            .windows
            .iter()
            .enumerate()
            .map(|(i, rect)| Pane {
                rect: *rect,
                image: snapshot.window_images.get(i).cloned().flatten(),
                order: i,
                stress: 0.0,
                broken: false,
            })
            .collect::<Vec<_>>();
        let windows = panes.iter().map(|p| p.rect).collect();
        let mut s = DesktopSurface {
            rng,
            snapshot: snapshot.clone(),
            panes,
            windows,
            breaks: Vec::new(),
            desktop_cracks: 0.0,
            desktop_broken: false,
            hit_flash: 0.0,
            find_text,
            primary: snapshot.primary,
            layers: Vec::new(),
            words: Vec::new(),
            version: 0,
            integrity: Cell::new(None),
            living: RefCell::new(None),
        };
        for (index, shot) in snapshot.screens.iter().enumerate() {
            let ratio = ratio_of(&shot.image);
            let mut top = shot.image.clone();
            top.set_device_pixel_ratio(ratio as f32);
            let under = desktop_layer(shot, &top);
            let void = void_layer(top.width(), top.height(), ratio, &mut s.rng);
            let mut glass = Image::new(top.width(), top.height());
            glass.set_device_pixel_ratio(ratio as f32);
            let mut frame = Image::filled(top.width(), top.height(), Color::BLACK);
            frame.set_device_pixel_ratio(ratio as f32);
            let r = shot.rect;
            let layer = ScreenLayers {
                index,
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
                top,
                under,
                void,
                glass,
                frame,
                shattered: false,
                falls: Vec::new(),
            };
            s.layers.push(layer);
            s.compose(index, Rect::new(0.0, 0.0, r.w, r.h));
            if find_text {
                let boxes = find_text_boxes(&s.layers[index].top, (r.x, r.y), MAX_WORDS);
                for b in boxes {
                    let colour = s.background_of(index, &b);
                    s.words.push(Word { rect: b, colour, screen: index, eaten: 0.0 });
                }
            }
        }
        s
    }

    /// A fresh surface from the same picture (Retry).
    pub fn fresh(&self, seed: Option<i64>) -> DesktopSurface {
        DesktopSurface::new(self.snapshot.clone(), seed, self.find_text)
    }

    // -- queries ---------------------------------------------------------------------

    pub fn layer_at(&self, x: f64, y: f64) -> Option<usize> {
        self.layers.iter().position(|l| l.x <= x && x < l.x + l.w && l.y <= y && y < l.y + l.h)
    }

    /// `_alpha(image, lx, ly)`.
    fn alpha(image: &Image, lx: f64, ly: f64) -> u8 {
        let ratio = ratio_of(image);
        // int() truncates toward zero, as in the Python.
        let (px, py) = ((lx * ratio) as i64, (ly * ratio) as i64);
        if 0 <= px && px < image.width() as i64 && 0 <= py && py < image.height() as i64 {
            image.pixel_premultiplied(px as u32, py as u32)[3]
        } else {
            0
        }
    }

    pub fn in_window(&self, x: f64, y: f64) -> bool {
        self.windows.iter().any(|w| qcontains(w, x, y))
    }

    /// `what_is_at(x, y)`: the layer seen at a point.
    pub fn what_is_at(&self, x: f64, y: f64) -> Seen {
        let Some(i) = self.layer_at(x, y) else { return Seen::Void };
        let layer = &self.layers[i];
        let (lx, ly) = layer.local(x, y);
        if Self::alpha(&layer.top, lx, ly) > 0 {
            return if self.in_window(x, y) { Seen::Window } else { Seen::Desktop };
        }
        if Self::alpha(&layer.under, lx, ly) > 0 { Seen::Desktop } else { Seen::Void }
    }

    /// How much of the desktop is left, 0..1: the share of the screens
    /// where the void does not show.
    pub fn integrity(&self) -> f64 {
        if let Some(v) = self.integrity.get() {
            return v;
        }
        let (mut total, mut left) = (0u64, 0u64);
        for layer in &self.layers {
            let mut y = INTEGRITY_STEP / 2.0;
            while y < layer.h {
                let mut x = INTEGRITY_STEP / 2.0;
                while x < layer.w {
                    total += 1;
                    if Self::alpha(&layer.under, x, y) > 0 || Self::alpha(&layer.top, x, y) > 0 {
                        left += 1;
                    }
                    x += INTEGRITY_STEP;
                }
                y += INTEGRITY_STEP;
            }
        }
        let v = if total > 0 { left as f64 / total as f64 } else { 1.0 };
        self.integrity.set(Some(v));
        v
    }

    /// The desktop's health, 0..1: worn down by cracks on the bare desktop
    /// and by acid eating it.
    pub fn desktop_hp(&self) -> f64 {
        if self.desktop_broken {
            return 0.0;
        }
        let eaten = (1.0 - self.integrity()).max(0.0) / ACID_EATS_ALL;
        (1.0 - self.desktop_cracks - eaten).clamp(0.0, 1.0)
    }

    /// What the mission HUD shows of the desktop.
    pub fn hud(&self) -> crate::mission_ui::SurfaceHud {
        crate::mission_ui::SurfaceHud {
            desktop_hp: self.desktop_hp(),
            intact_windows: self.intact_panes().count(),
            hit_flash: self.hit_flash,
        }
    }

    pub fn intact_panes(&self) -> impl Iterator<Item = &Pane> {
        self.panes.iter().filter(|p| !p.broken)
    }

    /// The frontmost unbroken window at a point.
    pub fn pane_at(&self, x: f64, y: f64) -> Option<usize> {
        self.panes.iter().position(|p| !p.broken && qcontains(&p.rect, x, y))
    }

    /// `_visible(pane)`: the part of a window not covered by an unbroken window in front.
    pub fn visible(&self, pane: usize) -> Region {
        let rect = self.panes[pane].rect;
        let mut region = Region::from_rect(rect);
        for other in &self.panes[..self.panes[pane].order] {
            if !other.broken && other.rect.intersects(&rect) {
                region.subtract_rect(&other.rect);
            }
        }
        region
    }

    /// `_bare_desktop()`: the screens minus every unbroken window.
    pub fn bare_desktop(&self) -> Region {
        let mut region = Region::new();
        for layer in &self.layers {
            region.unite_rect(&layer.rect());
        }
        for pane in self.intact_panes() {
            region.subtract_rect(&pane.rect);
        }
        region
    }

    /// Words still showing and not eaten.
    pub fn living_words(&self) -> Vec<usize> {
        self.with_living(|l| l.showing.iter().copied().filter(|&i| !self.words[i].done()).collect())
    }

    fn word_visible(&self, word: &Word) -> bool {
        let layer = &self.layers[word.screen];
        let (cx, cy) = word.centre();
        let (lx, ly) = layer.local(cx, cy);
        Self::alpha(&layer.top, lx, ly) > 0
    }

    /// `_visible_words()`: the words still showing, kept in buckets by where
    /// they are until acid melts or glass breaks.
    fn with_living<T>(&self, f: impl FnOnce(&Living) -> T) -> T {
        if self.living.borrow().is_none() {
            let showing: Vec<usize> =
                (0..self.words.len()).filter(|&i| !self.words[i].done() && self.word_visible(&self.words[i])).collect();
            let mut keys: HashMap<(i64, i64), usize> = HashMap::new();
            let mut lists: Vec<Vec<usize>> = Vec::new();
            let mut reach = 0.0f64;
            for &i in &showing {
                let word = &self.words[i];
                let (cx, cy) = word.centre();
                let key = ((cx / WORD_CELL).floor() as i64, (cy / WORD_CELL).floor() as i64);
                let slot = *keys.entry(key).or_insert_with(|| {
                    lists.push(Vec::new());
                    lists.len() - 1
                });
                lists[slot].push(i);
                reach = reach.max(word.rect.w / 2.0).max(word.rect.h / 2.0);
            }
            *self.living.borrow_mut() = Some(Living { showing, buckets: (keys, lists), reach });
        }
        f(self.living.borrow().as_ref().expect("living words"))
    }

    fn candidates(&self, x: f64, y: f64, reach: f64) -> Vec<usize> {
        self.with_living(|l| {
            let (keys, lists) = &l.buckets;
            let x0 = ((x - reach) / WORD_CELL).floor() as i64;
            let x1 = ((x + reach) / WORD_CELL).floor() as i64;
            let y0 = ((y - reach) / WORD_CELL).floor() as i64;
            let y1 = ((y + reach) / WORD_CELL).floor() as i64;
            if (x1 - x0 + 1) * (y1 - y0 + 1) > keys.len() as i64 {
                return lists.iter().flatten().copied().collect();
            }
            let mut out = Vec::new();
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    if let Some(&slot) = keys.get(&(cx, cy)) {
                        out.extend_from_slice(&lists[slot]);
                    }
                }
            }
            out
        })
    }

    /// Showing words whose middle is within `reach` of a point.
    pub fn words_within(&self, x: f64, y: f64, reach: f64) -> Vec<usize> {
        self.candidates(x, y, reach)
            .into_iter()
            .filter(|&i| {
                let w = &self.words[i];
                let (cx, cy) = w.centre();
                !w.done() && (cx - x).hypot(cy - y) <= reach
            })
            .collect()
    }

    /// The nearest showing word whose box is within `reach` of a point.
    pub fn word_near(&self, x: f64, y: f64, reach: f64) -> Option<usize> {
        let extra = self.with_living(|l| l.reach);
        let (mut best, mut best_d) = (None, reach);
        for i in self.candidates(x, y, reach + extra) {
            let word = &self.words[i];
            if word.done() {
                continue;
            }
            let r = word.rect;
            let dx = (r.x - x).max(0.0).max(x - r.right());
            let dy = (r.y - y).max(0.0).max(y - r.bottom());
            let d = dx.hypot(dy);
            if d <= best_d {
                best = Some(i);
                best_d = d;
            }
        }
        best
    }

    fn forget(&mut self) {
        self.integrity.set(None);
        *self.living.get_mut() = None;
    }

    // -- damage ----------------------------------------------------------------------

    /// Acid at a point: melts what is on top there. Returns what was melted,
    /// or None when only the void was left.
    pub fn melt(&mut self, x: f64, y: f64, radius: f64) -> Option<Hit> {
        let i = self.layer_at(x, y)?;
        let (lx, ly) = self.layers[i].local(x, y);
        let blob = blob(lx, ly, radius, &mut self.rng);
        let result;
        if Self::alpha(&self.layers[i].top, lx, ly) > 0 {
            result = if self.in_window(x, y) { Hit::Window } else { Hit::Desktop };
            burn_away(&mut self.layers[i].top, &blob, radius, &mut self.rng);
            if result == Hit::Desktop {
                burn_away(&mut self.layers[i].under, &blob, radius, &mut self.rng);
            }
        } else if Self::alpha(&self.layers[i].under, lx, ly) > 0 {
            result = Hit::Desktop;
            burn_away(&mut self.layers[i].under, &blob, radius, &mut self.rng);
        } else {
            return None;
        }
        let pad = radius * 1.7;
        self.compose(i, Rect::new(lx - pad, ly - pad, pad * 2.0, pad * 2.0 + radius * 1.5));
        self.forget();
        Some(result)
    }

    /// A heavy landing cracks the glass of the window under it, or the bare
    /// desktop. Returns `Window` when that window starts to break, `Desktop`
    /// when the desktop's health runs out, else None.
    pub fn crack(&mut self, x: f64, y: f64, force: f64) -> Option<Hit> {
        if self.desktop_broken || self.layer_at(x, y).is_none() {
            return None;
        }
        let force = force.clamp(0.05, 1.0);
        let pane = self.pane_at(x, y);
        if let Some(pi) = pane
            && self.breaks.iter().any(|b| b.pane == Some(pi))
        {
            return None;
        }
        let region = match pane {
            Some(pi) => self.visible(pi),
            None => self.bare_desktop(),
        };
        // A crack reaches a few hundred pixels at most: draw only round it.
        let mut near = region.clone();
        near.intersect_rect(&Rect::new(x - CRACK_REACH, y - CRACK_REACH, CRACK_REACH * 2.0, CRACK_REACH * 2.0));
        self.draw_on_glass(&near, |glass, ox, oy, rng| Some(draw_cracks(glass, x - ox, y - oy, force, rng)));
        if let Some(pi) = pane {
            self.panes[pi].stress += force * STRESS_PER_CRACK;
            if self.panes[pi].stress >= 1.0 {
                self.begin_break(region, x, y, Some(pi));
                return Some(Hit::Window);
            }
            return None;
        }
        self.desktop_cracks += force * DESKTOP_PER_CRACK;
        self.hit_flash = 0.5;
        if self.desktop_hp() <= 0.0 {
            self.break_desktop(Some((x, y)));
            return Some(Hit::Desktop);
        }
        None
    }

    /// The desktop's health is gone: cracks race over every screen, then all
    /// of its glass falls away.
    pub fn break_desktop(&mut self, at: Option<(f64, f64)>) {
        if self.desktop_broken {
            return;
        }
        self.desktop_broken = true;
        self.breaks.retain(|b| b.pane.is_none());
        for i in 0..self.layers.len() {
            let rect = self.layers[i].rect();
            let (cx, cy) = match at {
                Some((x, y)) if qcontains(&rect, x, y) => (x, y),
                _ => (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
            };
            self.begin_break(Region::from_rect(rect), cx, cy, None);
        }
    }

    fn begin_break(&mut self, region: Region, x: f64, y: f64, pane: Option<usize>) {
        let b = region.bounding_rect();
        let reach = [(b.x, b.y), (b.right(), b.y), (b.x, b.bottom()), (b.right(), b.bottom())]
            .iter()
            .map(|&(px, py)| (px - x).hypot(py - y))
            .fold(f64::NEG_INFINITY, f64::max);
        let lines = web_lines(x, y, reach, &mut self.rng);
        self.breaks.push(Break { region, lines, reach, pane, age: 0.0, drawn: 0 });
    }

    /// `_draw_on_glass(region, draw)`: `draw(scratch, origin_x, origin_y, rng)`
    /// on a scratch picture the size of the region's part of each screen;
    /// copied onto that screen's glass inside the region, and the screen is
    /// recomposed where it was drawn (`draw` returns that area, scratch-local).
    fn draw_on_glass(
        &mut self,
        region: &Region,
        mut draw: impl FnMut(&mut Image, f64, f64, &mut PyRandom) -> Option<Rect>,
    ) {
        for i in 0..self.layers.len() {
            let (lrect, ratio) = (self.layers[i].rect(), self.layers[i].ratio());
            if !region.intersects_rect(&lrect) {
                continue;
            }
            let local = region.translated(-lrect.x, -lrect.y);
            // Only as big as the region: a whole 4K screen each frame of a
            // break was most of its cost.
            let bounds = to_aligned_rect(intersected(local.bounding_rect(), Rect::new(0.0, 0.0, lrect.w, lrect.h)));
            if is_empty(bounds) {
                continue;
            }
            let mut scratch = Image::new(((bounds.w * ratio) as u32).max(1), ((bounds.h * ratio) as u32).max(1));
            scratch.set_device_pixel_ratio(ratio as f32);
            let area = draw(&mut scratch, lrect.x + bounds.x, lrect.y + bounds.y, &mut self.rng);
            let (ox, oy) = ((bounds.x * ratio).round() as i64, (bounds.y * ratio).round() as i64);
            blit_over_clipped(&mut self.layers[i].glass, &scratch, ox, oy, &local, ratio);
            let area = match area {
                Some(a) => intersected(a.translated(bounds.x, bounds.y), bounds),
                None => bounds,
            };
            self.compose(i, area);
        }
    }

    /// `_draw_on_glass(brk.region, _draw_lines(...))` for a break's new
    /// lines. Rust: drawn straight onto the glass inside the region (source-
    /// over is associative, so this equals drawing them on a clear scratch
    /// and laying that over the glass), and each line's own small box is
    /// recomposed rather than the box round all of them -- a ring of cracks
    /// spans most of a 4K screen, and composing that every frame cost 50 ms.
    fn draw_break_lines(&mut self, region: &Region, lines: &[WebLine]) {
        if lines.is_empty() {
            return;
        }
        for i in 0..self.layers.len() {
            let lrect = self.layers[i].rect();
            if !region.intersects_rect(&lrect) {
                continue;
            }
            let local = region.translated(-lrect.x, -lrect.y);
            let (ox, oy) = (lrect.x, lrect.y);
            paint_logical(&mut self.layers[i].glass, |p| {
                p.set_clip_region(&local);
                draw_lines_on(p, lines, ox, oy);
            });
            let screen = Rect::new(0.0, 0.0, lrect.w, lrect.h);
            let bounds = local.bounding_rect();
            for l in lines {
                let r = lines_extent(std::slice::from_ref(l)).translated(-ox, -oy);
                self.compose(i, intersected(intersected(r, bounds), screen));
            }
        }
    }

    /// The cracked piece falls away as shards. Under a window: the windows
    /// behind it, or the desktop; under the desktop's glass: nothing.
    fn finish_break(&mut self, brk: &Break) {
        for i in 0..self.layers.len() {
            let (lrect, ratio) = (self.layers[i].rect(), self.layers[i].ratio());
            if !brk.region.intersects_rect(&lrect) {
                continue;
            }
            let local = brk.region.translated(-lrect.x, -lrect.y);
            let bx = intersected(local.bounding_rect(), Rect::new(0.0, 0.0, lrect.w, lrect.h));
            let layer = &mut self.layers[i];
            let (w, h) = layer.top.size();
            let mut picture = Image::new(w, h);
            picture.set_device_pixel_ratio(ratio as f32);
            {
                let (top, glass, under) = (layer.top.data(), layer.glass.data(), layer.under.data());
                let out = picture.data_mut();
                for_region_pixels(&local, None, ratio, w, h, |o| {
                    let mut px = [0u8; 4];
                    if brk.pane.is_none() {
                        px.copy_from_slice(&under[o..o + 4]);
                    }
                    over(&mut px, &top[o..o + 4]);
                    over(&mut px, &glass[o..o + 4]);
                    out[o..o + 4].copy_from_slice(&px);
                });
            }
            let shards = shards(bx, &mut self.rng);
            let layer = &mut self.layers[i];
            layer.falls.push(Fall { picture, shards });
            let clear = |img: &mut Image| {
                let data = img.data_mut();
                for_region_pixels(&local, Some(bx), ratio, w, h, |o| data[o..o + 4].fill(0));
            };
            clear(&mut layer.top);
            clear(&mut layer.glass);
            if brk.pane.is_none() {
                clear(&mut layer.under);
            }
            match brk.pane {
                Some(pi) => self.reveal_behind(i, pi, &local),
                None => self.layers[i].shattered = true,
            }
            self.compose(i, bx);
        }
        if let Some(pi) = brk.pane {
            self.panes[pi].broken = true;
            self.windows = self.intact_panes().map(|p| p.rect).collect();
        }
        self.forget();
    }

    /// Paint the unbroken windows behind a broken one into its hole, back to
    /// front; where none is left the wallpaper shows. Its words go with it,
    /// and the revealed windows' words can be eaten next.
    fn reveal_behind(&mut self, li: usize, pane: usize, local: &Region) {
        let hole = local.bounding_rect();
        let (lx, ly, lw, lh) = (self.layers[li].x, self.layers[li].y, self.layers[li].w, self.layers[li].h);
        let order = self.panes[pane].order;
        let behind: Vec<(Rect, Option<Image>)> = self.panes[order + 1..]
            .iter()
            .rev()
            .filter(|o| !o.broken)
            .map(|o| (o.rect.translated(-lx, -ly), o.image.clone()))
            .filter(|(t, _)| t.intersects(&hole))
            .collect();
        if !behind.is_empty() {
            let clip = local.to_path();
            paint_logical(&mut self.layers[li].top, |p| {
                p.set_render_hint(RenderHint::SmoothPixmapTransform, true);
                p.set_clip_path(&clip);
                for (target, image) in &behind {
                    match image.as_ref().filter(|i| !i.is_null()) {
                        Some(img) => p.draw_image_rect(*target, img),
                        None => plain_window(p, *target),
                    }
                }
            });
        }
        let index = self.layers[li].index;
        for word in &mut self.words {
            let (cx, cy) = word.centre();
            if word.screen == index && !word.done() && local.contains(cx - lx, cy - ly) {
                word.eaten = 1.0;
            }
        }
        if !self.find_text {
            return;
        }
        let bx = to_aligned_rect(intersected(hole, Rect::new(0.0, 0.0, lw, lh)));
        if bx.w < 40.0 || bx.h < 20.0 {
            return;
        }
        let ratio = self.layers[li].ratio();
        let mut crop = self.layers[li].top.copy(
            (bx.x * ratio) as i32,
            (bx.y * ratio) as i32,
            (bx.w * ratio) as u32,
            (bx.h * ratio) as u32,
        );
        crop.set_device_pixel_ratio(ratio as f32);
        for found in find_text_boxes(&crop, (lx + bx.x, ly + bx.y), MAX_WORDS) {
            let (cx, cy) = found.center();
            if local.contains(cx - lx, cy - ly) && self.in_window(cx, cy) {
                let colour = self.background_of(li, &found);
                self.words.push(Word { rect: found, colour, screen: index, eaten: 0.0 });
            }
        }
    }

    /// Eat `amount` (0..1) more of a word, left to right. True when gone.
    pub fn eat(&mut self, word: usize, amount: f64) -> bool {
        if self.words[word].done() {
            return true;
        }
        let w = &mut self.words[word];
        let li = w.screen;
        let before = w.eaten;
        w.eaten = (w.eaten + amount).min(1.0);
        let (lx, ly) = (self.layers[li].x, self.layers[li].y);
        let r = w.rect.translated(-lx, -ly);
        let bite = Rect::new(r.x + r.w * before - 1.0, r.y - 1.0, r.w * (w.eaten - before) + 2.0, r.h + 2.0);
        let colour = w.colour;
        let done = w.done();
        // `fillRect(bite, colour)` in `SourceAtop`: the colour, only where the top layer is.
        let top = &mut self.layers[li].top;
        let ratio = ratio_of(top);
        let (tw, th) = (top.width() as i64, top.height() as i64);
        let (x0, y0, x1, y1) =
            fill_rect_pixels(Rect::new(bite.x * ratio, bite.y * ratio, bite.w * ratio, bite.h * ratio));
        let (x0, y0, x1, y1) = (x0.clamp(0, tw), y0.clamp(0, th), x1.clamp(0, tw), y1.clamp(0, th));
        let data = top.data_mut();
        for y in y0..y1 {
            for x in x0..x1 {
                let o = ((y * tw + x) * 4) as usize;
                let da = data[o + 3] as u32;
                data[o] = div255(colour.r as u32 * da) as u8;
                data[o + 1] = div255(colour.g as u32 * da) as u8;
                data[o + 2] = div255(colour.b as u32 * da) as u8;
            }
        }
        self.compose(li, bite.adjusted(-2.0, -2.0, 2.0, 2.0));
        done
    }

    /// Glass still cracking or falling.
    pub fn busy(&self) -> bool {
        !self.breaks.is_empty() || self.layers.iter().any(|l| !l.falls.is_empty())
    }

    /// Advance the flash, the racing cracks and the falling shards.
    pub fn update(&mut self, dt: f64) {
        self.hit_flash = (self.hit_flash - dt).max(0.0);
        let mut k = 0;
        while k < self.breaks.len() {
            self.breaks[k].age += dt;
            let brk = &self.breaks[k];
            // The cracks race out from the blow to the far corners.
            let front = brk.reach * (brk.age / BREAK_SECONDS).min(1.0);
            let mut upto = brk.drawn;
            while upto < brk.lines.len() && brk.lines[upto].dist <= front {
                upto += 1;
            }
            if upto > brk.drawn {
                let lines = brk.lines[brk.drawn..upto].to_vec();
                let region = brk.region.clone();
                self.breaks[k].drawn = upto;
                self.draw_break_lines(&region, &lines);
            }
            if self.breaks[k].age >= BREAK_SECONDS + BREAK_HOLD {
                let brk = self.breaks.remove(k);
                self.finish_break(&brk);
            } else {
                k += 1;
            }
        }
        for layer in &mut self.layers {
            for fall in &mut layer.falls {
                for shard in &mut fall.shards {
                    shard.age += dt;
                    shard.vy += 900.0 * dt;
                    shard.dx += shard.vx * dt;
                    shard.dy += shard.vy * dt;
                    shard.angle += shard.spin * dt;
                }
            }
            layer.falls.retain(|f| !f.shards.iter().all(|s| s.age >= SHARD_SECONDS));
        }
    }

    // -- painting --------------------------------------------------------------------

    /// `_compose(layer, area)`: void, desktop, top and glass into the frame,
    /// inside `area` (screen-local, logical).
    fn compose(&mut self, li: usize, area: Rect) {
        let layer = &mut self.layers[li];
        let area = intersected(area, Rect::new(0.0, 0.0, layer.w, layer.h));
        if is_empty(area) {
            return;
        }
        let a = to_aligned_rect(area);
        let ratio = ratio_of(&layer.top);
        let (w, h) = layer.frame.size();
        let x0 = ((a.x * ratio).floor().max(0.0) as usize).min(w as usize);
        let y0 = ((a.y * ratio).floor().max(0.0) as usize).min(h as usize);
        let x1 = ((a.right() * ratio).ceil().max(0.0) as usize).min(w as usize);
        let y1 = ((a.bottom() * ratio).ceil().max(0.0) as usize).min(h as usize);
        let (void, under, top, glass) = (layer.void.data(), layer.under.data(), layer.top.data(), layer.glass.data());
        let frame = layer.frame.data_mut();
        for y in y0..y1 {
            for x in x0..x1 {
                let o = (y * w as usize + x) * 4;
                let mut px = [0u8; 4];
                px.copy_from_slice(&void[o..o + 4]);
                over(&mut px, &under[o..o + 4]);
                over(&mut px, &top[o..o + 4]);
                over(&mut px, &glass[o..o + 4]);
                px[3] = 255; // Format_RGB32
                frame[o..o + 4].copy_from_slice(&px);
            }
        }
        self.version += 1;
    }

    /// `paint(painter)`: every screen's picture, then its falling shards.
    pub fn paint(&self, p: &mut Canvas) {
        for layer in &self.layers {
            p.draw_image_at(layer.x, layer.y, &layer.frame);
            for fall in &layer.falls {
                paint_shards(p, layer, fall);
            }
        }
    }

    /// `_background_of(layer, box)`: the average of the pixels just outside a word.
    fn background_of(&self, li: usize, bx: &Rect) -> Color {
        let layer = &self.layers[li];
        let r = bx.translated(-layer.x, -layer.y);
        let ratio = layer.ratio();
        let (w, h) = (layer.top.width() as i64, layer.top.height() as i64);
        let (mut n, mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32, 0u32);
        for fx in [0.0, 0.25, 0.5, 0.75, 1.0] {
            for y in [r.y - 2.0, r.bottom() + 2.0] {
                let px = ((r.x + r.w * fx) * ratio) as i64;
                let py = (y * ratio) as i64;
                if 0 <= px && px < w && 0 <= py && py < h {
                    // QColor(image.pixel(...)): the stored (premultiplied) value.
                    let c = layer.top.pixel_premultiplied(px as u32, py as u32);
                    sr += c[0] as u32;
                    sg += c[1] as u32;
                    sb += c[2] as u32;
                    n += 1;
                }
            }
        }
        if n == 0 {
            return Color::rgb(240, 240, 240);
        }
        Color::rgb((sr / n) as u8, (sg / n) as u8, (sb / n) as u8)
    }
}

/// Every device pixel whose centre lies in `region` (logical, screen-local)
/// and, when given, inside `limit` too; calls `f` with its byte offset.
fn for_region_pixels(region: &Region, limit: Option<Rect>, ratio: f64, w: u32, h: u32, mut f: impl FnMut(usize)) {
    for r in region.rects() {
        let r = match limit {
            Some(l) => match r.intersection(&l) {
                Some(i) => i,
                None => continue,
            },
            None => *r,
        };
        let (x0, y0, x1, y1) = device_span(&r, ratio, w, h);
        for y in y0..y1 {
            for x in x0..x1 {
                f((y * w as usize + x) * 4);
            }
        }
    }
}

/// `p.setClipPath(region); p.drawImage(QPointF(ox, oy), src)` onto `dst`
/// (device offset `ox, oy`), source-over.
fn blit_over_clipped(dst: &mut Image, src: &Image, ox: i64, oy: i64, region: &Region, ratio: f64) {
    let (dw, dh) = dst.size();
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let s = src.data();
    let d = dst.data_mut();
    for r in region.rects() {
        let (x0, y0, x1, y1) = device_span(r, ratio, dw, dh);
        let (x0, y0) = ((x0 as i64).max(ox), (y0 as i64).max(oy));
        let (x1, y1) = ((x1 as i64).min(ox + sw), (y1 as i64).min(oy + sh));
        for y in y0..y1 {
            for x in x0..x1 {
                let so = (((y - oy) * sw + (x - ox)) * 4) as usize;
                if s[so + 3] == 0 {
                    continue;
                }
                let o = ((y * dw as i64 + x) * 4) as usize;
                over(&mut d[o..o + 4], &s[so..so + 4]);
            }
        }
    }
}

fn desktop_layer(shot: &SurfaceShot, top: &Image) -> Image {
    let mut under = match shot.wallpaper.as_ref().filter(|w| !w.is_null()) {
        Some(w) => w.clone(),
        None => {
            // No wallpaper file: a plain desktop in the screenshot's own tint.
            let mut under = Image::new(top.width(), top.height());
            let h = top.height() as f64;
            under.paint(|p| {
                let g = LinearGradient::new(0.0, 0.0, 0.0, h)
                    .with_stop(0.0, Color::rgb(28, 72, 104))
                    .with_stop(1.0, Color::rgb(12, 34, 58));
                p.fill_rect(Rect::new(0.0, 0.0, top.width() as f64, h), g);
            });
            under
        }
    };
    under.set_device_pixel_ratio(top.device_pixel_ratio());
    under
}

// -- drawing helpers ---------------------------------------------------------------------

/// `_blob`: an irregular round hole.
fn blob(x: f64, y: f64, radius: f64, rng: &mut PyRandom) -> Path {
    let points = 16;
    let mut path = Path::new();
    let mut first = 0.0;
    for i in 0..=points {
        let a = i as f64 / points as f64 * TAU;
        if i == 0 {
            let r = radius * (0.78 + 0.3 * rng.random());
            first = r;
            path.move_to(x + a.cos() * r, y + a.sin() * r);
        } else if i == points {
            path.line_to(x + first, y);
        } else {
            let r = radius * (0.78 + 0.3 * rng.random());
            path.line_to(x + a.cos() * r, y + a.sin() * r);
        }
    }
    path
}

/// `_burn_away`: scorch a ring round the hole, run a few drips down, then melt it through.
fn burn_away(image: &mut Image, blob: &Path, radius: f64, rng: &mut PyRandom) {
    let (cx, cy) = blob.bounding_rect().center();
    let drips: Vec<(f64, f64, f64)> = (0..rng.randint(2, 4))
        .map(|_| {
            let dx = (rng.random() - 0.5) * radius * 1.3;
            let length = radius * (0.6 + rng.random() * 1.4);
            let width = 2.0 + rng.random() * 2.5;
            (dx, length, width)
        })
        .collect();
    paint_logical(image, |p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_composition_mode(CompositionMode::SourceAtop);
        let scorch = RadialGradient::new(cx, cy, radius * 1.45)
            .with_stop(0.0, Color::rgba(40, 60, 10, 230))
            .with_stop(0.62, Color::rgba(58, 72, 18, 210))
            .with_stop(0.8, Color::rgba(90, 70, 20, 120))
            .with_stop(1.0, Color::rgba(90, 70, 20, 0));
        p.set_pen(Pen::NONE);
        p.set_brush(scorch);
        p.draw_ellipse_at(cx, cy, radius * 1.45, radius * 1.45);
        for (dx, length, width) in &drips {
            let (tx, ty) = (cx + dx, cy + radius * 0.5);
            p.set_pen(round_cap_pen(Color::rgba(120, 170, 30, 190), *width));
            p.draw_line(tx, ty, tx, ty + length);
        }
        p.set_composition_mode(CompositionMode::Clear);
        p.set_pen(Pen::NONE);
        p.set_brush(Color::BLACK);
        p.draw_path(blob);
        // A glowing rim, where anything is left to glow.
        p.set_composition_mode(CompositionMode::SourceAtop);
        p.set_brush(Brush::None);
        p.set_pen(Pen::new(Color::rgba(190, 255, 70, 210), 3.0));
        p.draw_path(blob);
    });
}

/// `_draw_cracks`: jagged cracks radiating from a point; returns the area drawn over.
fn draw_cracks(glass: &mut Image, x: f64, y: f64, force: f64, rng: &mut PyRandom) -> Rect {
    let rays = 3 + (force * 6.0) as i64;
    let mut lines: Vec<Segment> = Vec::new();
    for i in 0..rays {
        let angle = (i as f64 + rng.random() * 0.8) / rays as f64 * TAU;
        let length = (60.0 + rng.random() * 90.0) * (0.6 + force * 1.6);
        crack_ray(&mut lines, x, y, angle, length, rng, 0);
    }
    paint_logical(glass, |p| {
        p.set_render_hint(RenderHint::Antialiasing, true);
        p.set_pen(round_cap_pen(Color::rgba(0, 0, 0, 120), 2.6));
        for &((ax, ay), (bx, by)) in &lines {
            p.draw_line(ax + 0.8, ay + 0.8, bx + 0.8, by + 0.8);
        }
        p.set_pen(round_cap_pen(Color::rgba(236, 246, 255, 215), 1.1));
        for &((ax, ay), (bx, by)) in &lines {
            p.draw_line(ax, ay, bx, by);
        }
        // The impact: a small crushed star.
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(255, 255, 255, 70));
        p.draw_ellipse_at(x, y, 4.0 + force * 8.0, 4.0 + force * 8.0);
    });
    let xs = lines.iter().flat_map(|(a, b)| [a.0, b.0]).chain([x]);
    let ys = lines.iter().flat_map(|(a, b)| [a.1, b.1]).chain([y]);
    let (x0, x1) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (y0, y1) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    Rect::new(x0 - 4.0, y0 - 4.0, x1 - x0 + 8.0, y1 - y0 + 8.0)
}

/// `_web_lines`: a spider-web of cracks from a blow out to `reach`: long
/// rays and rings joining them, each line with its distance from the blow.
pub fn web_lines(x: f64, y: f64, reach: f64, rng: &mut PyRandom) -> Vec<WebLine> {
    let count = 14;
    let mut rays: Vec<Vec<(f64, f64)>> = Vec::with_capacity(count);
    for i in 0..count {
        let angle = (i as f64 + rng.random() * 0.6) / count as f64 * TAU;
        let mut points = vec![(x, y)];
        let mut r = 0.0;
        while r < reach * 1.05 {
            r += 14.0 + rng.random() * 18.0;
            let a = angle + (rng.random() - 0.5) * 0.16;
            points.push((x + a.cos() * r, y + a.sin() * r));
        }
        rays.push(points);
    }
    let dist = |a: (f64, f64)| (a.0 - x).hypot(a.1 - y);
    let mut lines = Vec::new();
    for points in &rays {
        for pair in points.windows(2) {
            lines.push(WebLine { a: pair[0], b: pair[1], dist: dist(pair[0]) });
        }
    }
    // Rings: every few steps, a chord across to the next ray.
    for (i, points) in rays.iter().enumerate() {
        let nxt = &rays[(i + 1) % count];
        for k in (2..points.len().min(nxt.len())).step_by(3) {
            if rng.random() < 0.8 {
                lines.push(WebLine { a: points[k], b: nxt[k], dist: dist(points[k]) });
            }
        }
    }
    lines.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    lines
}

fn draw_lines_on(p: &mut Canvas, lines: &[WebLine], ox: f64, oy: f64) {
    p.set_render_hint(RenderHint::Antialiasing, true);
    p.translate(-ox, -oy);
    p.set_pen(round_cap_pen(Color::rgba(0, 0, 0, 110), 2.4));
    for l in lines {
        p.draw_line(l.a.0 + 0.8, l.a.1 + 0.8, l.b.0 + 0.8, l.b.1 + 0.8);
    }
    p.set_pen(round_cap_pen(Color::rgba(236, 246, 255, 225), 1.1));
    for l in lines {
        p.draw_line(l.a.0, l.a.1, l.b.0, l.b.1);
    }
}

/// The overlay-local area web lines can touch (ends, shadow offset and pen).
fn lines_extent(lines: &[WebLine]) -> Rect {
    let xs = lines.iter().flat_map(|l| [l.a.0, l.b.0]);
    let ys = lines.iter().flat_map(|l| [l.a.1, l.b.1]);
    let (x0, x1) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (y0, y1) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    Rect::new(x0 - 4.0, y0 - 4.0, x1 - x0 + 8.0, y1 - y0 + 8.0)
}

/// A crack segment: its two ends.
type Segment = ((f64, f64), (f64, f64));

fn crack_ray(
    lines: &mut Vec<Segment>,
    mut x: f64,
    mut y: f64,
    mut angle: f64,
    length: f64,
    rng: &mut PyRandom,
    depth: u32,
) {
    let mut travelled = 0.0;
    while travelled < length {
        let step = 10.0 + rng.random() * 16.0;
        angle += (rng.random() - 0.5) * 0.7;
        let (nx, ny) = (x + angle.cos() * step, y + angle.sin() * step);
        lines.push(((x, y), (nx, ny)));
        x = nx;
        y = ny;
        travelled += step;
        if depth < 2 && rng.random() < 0.14 {
            let branch = angle + (rng.random() - 0.5) * 1.6;
            crack_ray(lines, x, y, branch, (length - travelled) * 0.6, rng, depth + 1);
        }
    }
}

/// `_shards(box, rng)`: the glass of `box` (screen-local) broken into
/// triangles on a jittered grid.
pub fn shards(bx: Rect, rng: &mut PyRandom) -> Vec<Shard> {
    let (x0, y0, w, h) = (bx.x, bx.y, bx.w, bx.h);
    let cols = ((w / 90.0) as i64).clamp(2, 8);
    let rows = ((h / 90.0) as i64).clamp(2, 6);
    let mut grid: Vec<Vec<Vec2>> = Vec::new();
    for j in 0..=rows {
        let mut row = Vec::new();
        for i in 0..=cols {
            let jx = if i == 0 || i == cols { 0.0 } else { (rng.random() - 0.5) * w / cols as f64 * 0.7 };
            let jy = if j == 0 || j == rows { 0.0 } else { (rng.random() - 0.5) * h / rows as f64 * 0.7 };
            row.push(Vec2::new(x0 + w * i as f64 / cols as f64 + jx, y0 + h * j as f64 / rows as f64 + jy));
        }
        grid.push(row);
    }
    let mut out = Vec::new();
    for j in 0..rows as usize {
        for i in 0..cols as usize {
            let (a, b, c, d) = (grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]);
            let tris = if rng.random() < 0.5 { [[a, b, c], [a, c, d]] } else { [[a, b, d], [b, c, d]] };
            for tri in tris {
                let cx = tri.iter().map(|p| p.x).sum::<f64>() / 3.0;
                let cy = tri.iter().map(|p| p.y).sum::<f64>() / 3.0;
                let vx = (cx - x0 - w / 2.0) / w.max(1.0) * 220.0 + (rng.random() - 0.5) * 80.0;
                let vy = -60.0 - rng.random() * 120.0;
                let spin = (rng.random() - 0.5) * 3.0;
                let age = -(rng.random() * 0.35);
                out.push(Shard { polygon: tri, cx, cy, vx, vy, spin, age, dx: 0.0, dy: 0.0, angle: 0.0 });
            }
        }
    }
    out
}

fn paint_shards(p: &mut Canvas, layer: &ScreenLayers, fall: &Fall) {
    for shard in &fall.shards {
        if shard.age >= SHARD_SECONDS {
            continue;
        }
        p.save();
        p.translate(layer.x + shard.cx + shard.dx, layer.y + shard.cy + shard.dy);
        p.rotate(shard.angle.to_degrees());
        p.translate(-shard.cx, -shard.cy);
        p.set_opacity((1.0 - shard.age / SHARD_SECONDS).clamp(0.0, 1.0));
        let mut path = Path::new();
        path.add_polygon(&shard.polygon);
        path.close_subpath();
        p.set_clip_path(&path);
        p.draw_image_at(0.0, 0.0, &fall.picture);
        p.set_clipping(false);
        p.set_pen(Pen::new(Color::rgba(230, 245, 255, 150), 1.2));
        p.set_brush(Brush::None);
        p.draw_path(&path);
        p.restore();
    }
}

/// `_plain_window`: a window with no picture of its own.
fn plain_window(p: &mut Canvas, rect: Rect) {
    p.fill_rect(rect, Color::rgb(236, 236, 232));
    p.fill_rect(Rect::new(rect.x, rect.y, rect.w, rect.h.min(30.0)), Color::rgb(52, 56, 64));
    p.set_pen(Pen::new(Color::rgb(120, 124, 132), 1.0));
    p.set_brush(Brush::None);
    p.draw_rect(rect.adjusted(0.5, 0.5, -0.5, -0.5));
}

/// `_void_layer`: what lies under the desktop -- black depth, a faint grid,
/// stray pixels.
fn void_layer(width: u32, height: u32, ratio: f64, rng: &mut PyRandom) -> Image {
    let mut image = Image::new(width, height);
    image.set_device_pixel_ratio(ratio as f32);
    let (w, h) = (width as f64 / ratio, height as f64 / ratio);
    let count = (w * h / 9000.0) as i64;
    let mut specks = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let r = *rng.choice(&[150u8, 90, 60]);
        let g = rng.randint(40, 200) as u8;
        let b = rng.randint(120, 255) as u8;
        let a = rng.randint(40, 140) as u8;
        let size = *rng.choice(&[2.0, 2.0, 3.0, 4.0]);
        let x = rng.random() * w;
        let y = rng.random() * h;
        specks.push((Color::rgba(r, g, b, a), Rect::new(x, y, size, size)));
    }
    paint_logical(&mut image, |p| {
        let g = RadialGradient::new(w / 2.0, h / 2.0, w.max(h) * 0.7)
            .with_stop(0.0, Color::rgb(18, 10, 30))
            .with_stop(1.0, Color::rgb(3, 2, 8));
        p.fill_rect(Rect::new(0.0, 0.0, w, h), g);
        p.set_pen(Pen::new(Color::rgba(80, 40, 140, 38), 1.0));
        let step = 48.0;
        let mut x = 0.0;
        while x < w {
            p.draw_line(x, 0.0, x, h);
            x += step;
        }
        let mut y = 0.0;
        while y < h {
            p.draw_line(0.0, y, w, y);
            y += step;
        }
        p.set_pen(Pen::NONE);
        for (c, r) in &specks {
            p.set_brush(*c);
            p.draw_rect(*r);
        }
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect::new(200.0, 150.0, 700.0, 500.0);
    const BEHIND: Rect = Rect::new(500.0, 300.0, 700.0, 500.0);

    fn surface(text_rows: i64) -> DesktopSurface {
        let snap = synthetic_snapshot(&[Rect::new(0.0, 0.0, 1600.0, 1000.0)], &[WINDOW], 0, text_rows);
        DesktopSurface::new(snap, Some(2), text_rows > 0)
    }

    fn settle(s: &mut DesktopSurface, seconds: f64) {
        for _ in 0..(seconds / 0.02) as i64 {
            s.update(0.02);
        }
    }

    #[test]
    fn acid_melts_a_window_then_the_desktop_behind_it_then_nothing_is_left() {
        let mut s = surface(0);
        let (x, y) = (500.0, 400.0);
        assert_eq!(s.what_is_at(x, y), Seen::Window);
        let full = s.integrity();
        assert_eq!(s.melt(x, y, 30.0), Some(Hit::Window));
        assert_eq!(s.what_is_at(x, y), Seen::Desktop, "the wallpaper shows through the hole");
        assert_eq!(s.integrity(), full, "a hole in a window does not eat the desktop");
        assert_eq!(s.melt(x, y, 30.0), Some(Hit::Desktop));
        assert_eq!(s.what_is_at(x, y), Seen::Void);
        assert_eq!(s.melt(x, y, 30.0), None);
        assert!(s.integrity() < full);
    }

    #[test]
    fn acid_on_the_bare_desktop_breaks_it_at_once() {
        let mut s = surface(0);
        assert_eq!(s.what_is_at(1300.0, 800.0), Seen::Desktop);
        assert_eq!(s.melt(1300.0, 800.0, 30.0), Some(Hit::Desktop));
        assert_eq!(s.what_is_at(1300.0, 800.0), Seen::Void);
    }

    fn picture(s: &DesktopSurface) -> Image {
        let mut image = Image::new(1600, 1000);
        image.paint(|p| s.paint(p));
        image
    }

    #[test]
    fn the_picture_shows_the_hole() {
        let mut s = surface(0);
        let before = picture(&s);
        s.melt(500.0, 400.0, 30.0);
        let after = picture(&s);
        assert_ne!(before.pixel(500, 400), after.pixel(500, 400));
        assert_eq!(before.pixel(1400, 900), after.pixel(1400, 900), "the rest is untouched");
    }

    #[test]
    fn a_window_cracks_until_its_glass_breaks_and_the_window_behind_shows() {
        let snap = synthetic_snapshot(&[Rect::new(0.0, 0.0, 1600.0, 1000.0)], &[WINDOW, BEHIND], 0, 0);
        let mut s = DesktopSurface::new(snap, Some(2), false);
        assert_eq!(s.crack(400.0, 400.0, 0.5), None);
        assert!(0.0 < s.panes[0].stress && s.panes[0].stress < 1.0 && s.panes[1].stress == 0.0);
        let mut result = None;
        for _ in 0..10 {
            result = s.crack(400.0, 400.0, 1.0);
            if result.is_some() {
                break;
            }
        }
        assert_eq!(result, Some(Hit::Window));
        assert!(!s.breaks.is_empty(), "the cracks race across it first");
        assert!(!s.panes[0].broken);
        assert_eq!(s.crack(420.0, 400.0, 1.0), None, "a window already breaking takes no more");
        settle(&mut s, 0.7);
        assert!(s.panes[0].broken && !s.panes[1].broken);
        assert_eq!(s.windows, vec![BEHIND]);
        assert_eq!(s.what_is_at(600.0, 400.0), Seen::Window, "the window behind shows where they overlapped");
        assert_eq!(s.what_is_at(300.0, 400.0), Seen::Desktop, "where none is behind, the desktop");
        assert!(!s.layers[0].falls.is_empty(), "its glass falls as shards");
        settle(&mut s, 4.0);
        assert!(s.layers[0].falls.is_empty(), "the falling shards finish");
        assert_eq!(s.desktop_hp(), 1.0, "breaking a window costs the desktop nothing");
    }

    #[test]
    fn cracks_on_the_bare_desktop_wear_down_its_health_until_it_breaks() {
        let mut s = surface(0);
        assert_eq!(s.desktop_hp(), 1.0);
        assert_eq!(s.crack(1300.0, 800.0, 1.0), None);
        assert!(s.desktop_hp() < 1.0 && s.hit_flash > 0.0);
        assert_eq!(s.panes[0].stress, 0.0, "the window is not touched");
        let mut result = None;
        for _ in 0..40 {
            result = s.crack(1300.0, 800.0, 1.0);
            if result.is_some() {
                break;
            }
        }
        assert_eq!(result, Some(Hit::Desktop));
        assert!(s.desktop_broken && s.desktop_hp() == 0.0);
        settle(&mut s, 0.8);
        assert!(s.layers[0].shattered);
        assert_eq!(s.what_is_at(1300.0, 800.0), Seen::Void);
        assert_eq!(s.crack(500.0, 400.0, 1.0), None, "nothing is left to crack");
    }

    #[test]
    fn acid_eating_the_desktop_also_takes_its_health() {
        let mut s = surface(0);
        for x in (1000..1600).step_by(40) {
            for y in (0..1000).step_by(40) {
                s.melt(x as f64, y as f64, 30.0);
            }
        }
        assert!(s.desktop_hp() < 1.0);
    }

    #[test]
    fn words_are_found_by_their_shape_and_eaten_to_the_background() {
        let mut s = surface(8);
        assert!(s.words.len() >= 10, "{}", s.words.len());
        let inside = WINDOW.adjusted(-4.0, -4.0, 4.0, 4.0);
        assert!(
            s.words.iter().all(|w| inside.x <= w.rect.x
                && w.rect.right() <= inside.right()
                && inside.y <= w.rect.y
                && w.rect.bottom() <= inside.bottom()),
            "only the window has text"
        );
        let (cx, cy) = s.words[0].centre();
        assert_eq!(s.word_near(cx, cy, 5.0), Some(0));
        while !s.eat(0, 0.3) {}
        let after = s.layers[0].top.pixel(cx as u32, cy as u32);
        assert!((after.r as i32 - s.words[0].colour.r as i32).abs() < 10, "the word is erased to the colour around it");
        assert_ne!(s.word_near(cx, cy, 5.0), Some(0));
    }

    #[test]
    fn the_fast_word_lookup_agrees_with_a_full_scan_and_follows_melting() {
        let mut s = surface(8);
        let slow = |s: &DesktopSurface, x: f64, y: f64, reach: f64| {
            let mut v: Vec<usize> = (0..s.words.len())
                .filter(|&i| {
                    let w = &s.words[i];
                    let (cx, cy) = w.centre();
                    !w.done() && s.word_visible(w) && (cx - x).hypot(cy - y) <= reach
                })
                .collect();
            v.sort();
            v
        };
        let fast = |s: &DesktopSurface, x: f64, y: f64, reach: f64| {
            let mut v = s.words_within(x, y, reach);
            v.sort();
            v
        };
        for (x, y, reach) in [(500.0, 400.0, 120.0), (300.0, 200.0, 600.0), (1400.0, 900.0, 80.0)] {
            assert_eq!(fast(&s, x, y, reach), slow(&s, x, y, reach));
        }
        let word = s.word_near(400.0, 250.0, 40.0).expect("a word near");
        let (cx, cy) = s.words[word].centre();
        s.melt(cx, cy, 30.0);
        assert!(!s.words_within(cx, cy, 5.0).contains(&word), "a melted word is gone at once");
        assert_eq!(fast(&s, 500.0, 400.0, 300.0), slow(&s, 500.0, 400.0, 300.0));
    }

    #[test]
    fn breaking_a_window_reveals_the_words_behind_it() {
        let mut snap = synthetic_snapshot(&[Rect::new(0.0, 0.0, 1600.0, 1000.0)], &[WINDOW, BEHIND], 0, 8);
        // The window behind has its own captured picture, with text on it.
        let own = synthetic_snapshot(&[BEHIND], &[BEHIND], 0, 8).screens.remove(0).image;
        snap.window_images = vec![None, Some(own)];
        let mut s = DesktopSurface::new(snap, Some(3), true);
        let before = s.words.len();
        assert!(before > 0);
        while s.crack(400.0, 400.0, 1.0) != Some(Hit::Window) {}
        settle(&mut s, 0.7);
        assert!(s.panes[0].broken);
        assert!(s.words.iter().take(before).any(|w| w.done()), "the broken window's words went with it");
        assert!(s.words.len() > before, "the window behind shows its own words");
        assert!(s.words[before..].iter().all(|w| qcontains(&BEHIND, w.centre().0, w.centre().1)));
    }

    #[test]
    fn the_same_seed_breaks_the_same_way() {
        let a = web_lines(100.0, 100.0, 400.0, &mut PyRandom::from_int(5));
        let b = web_lines(100.0, 100.0, 400.0, &mut PyRandom::from_int(5));
        assert_eq!(a, b);
        assert!(a.windows(2).all(|w| w[0].dist <= w[1].dist), "nearest first");
        let sh = shards(Rect::new(0.0, 0.0, 700.0, 500.0), &mut PyRandom::from_int(1));
        assert_eq!(sh.len(), 7 * 5 * 2);
        assert!(sh.iter().all(|s| s.age <= 0.0 && s.vy < 0.0));
    }
}
