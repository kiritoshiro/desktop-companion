//! Fonts, metrics and text layout (`QFont`, `QFontMetrics`, `QPainter::drawText`).
//!
//! Font files are loaded lazily from `%WINDIR%\Fonts` (Segoe UI, Consolas,
//! Arial, Tahoma, Segoe UI Symbol/Emoji for fallback glyphs). A family that is
//! missing falls back along `requested → Segoe UI → Arial → Tahoma → any
//! registered face`. Extra faces can be added with [`register_font`].
//!
//! Metrics emulate Qt 5's Windows (GDI) font engine at 96 logical DPI:
//! `pt → px = pt × 96 / 72`, integer ascent/descent, and glyph advances
//! rounded to whole pixels. Kerning comes from the font's `kern` table.

use crate::path::Path;
use ab_glyph::{Font as _, FontArc, GlyphId, OutlineCurve, PxScale};
use dc_core::Rect;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub type FaceId = u16;

/// Logical DPI used for point sizes (Qt on Windows: 96).
static LOGICAL_DPI: AtomicU32 = AtomicU32::new(96);

pub fn set_logical_dpi(dpi: u32) {
    LOGICAL_DPI.store(dpi.max(1), Ordering::Relaxed);
}

pub fn logical_dpi() -> u32 {
    LOGICAL_DPI.load(Ordering::Relaxed)
}

pub(crate) struct Face {
    pub font: FontArc,
    pub upem: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub height_unscaled: f32,
}

impl Face {
    fn new(font: FontArc) -> Face {
        let upem = font.units_per_em().unwrap_or(2048.0);
        Face {
            upem,
            ascent: font.ascent_unscaled(),
            descent: -font.descent_unscaled(),
            line_gap: font.line_gap_unscaled(),
            height_unscaled: font.height_unscaled(),
            font,
        }
    }

    /// The ab_glyph scale whose em size is `px`.
    pub fn px_scale(&self, px: f32) -> PxScale {
        PxScale::from(px * self.height_unscaled / self.upem)
    }
}

#[derive(Default)]
struct Db {
    faces: Vec<Arc<Face>>,
    families: Vec<String>,
    resolved: HashMap<(u16, u16, bool), Option<FaceId>>,
    files: HashMap<PathBuf, Option<FaceId>>,
    registered: Vec<(u16, u16, bool, FaceId)>,
    fallbacks: Option<Vec<FaceId>>,
    glyphs: HashMap<(FaceId, char), (FaceId, u16)>,
}

fn db() -> &'static Mutex<Db> {
    static DB: OnceLock<Mutex<Db>> = OnceLock::new();
    DB.get_or_init(|| Mutex::new(Db::default()))
}

fn lock() -> std::sync::MutexGuard<'static, Db> {
    db().lock().unwrap_or_else(|e| e.into_inner())
}

fn fonts_dir() -> PathBuf {
    let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    win.join("Fonts")
}

/// (css weight, italic, file) for the families the Python app asks for.
fn family_files(family: &str) -> &'static [(u16, bool, &'static str)] {
    match family {
        "segoe ui" => &[
            (300, false, "segoeuil.ttf"),
            (350, false, "segoeuisl.ttf"),
            (400, false, "segoeui.ttf"),
            (600, false, "seguisb.ttf"),
            (700, false, "segoeuib.ttf"),
            (900, false, "seguibl.ttf"),
            (300, true, "seguili.ttf"),
            (350, true, "seguisli.ttf"),
            (400, true, "segoeuii.ttf"),
            (600, true, "seguisbi.ttf"),
            (700, true, "segoeuiz.ttf"),
            (900, true, "seguibli.ttf"),
        ],
        "consolas" => &[
            (400, false, "consola.ttf"),
            (700, false, "consolab.ttf"),
            (400, true, "consolai.ttf"),
            (700, true, "consolaz.ttf"),
        ],
        "arial" => &[
            (400, false, "arial.ttf"),
            (700, false, "arialbd.ttf"),
            (400, true, "ariali.ttf"),
            (700, true, "arialbi.ttf"),
        ],
        "tahoma" => &[(400, false, "tahoma.ttf"), (700, false, "tahomabd.ttf")],
        "segoe ui symbol" => &[(400, false, "seguisym.ttf")],
        "segoe ui emoji" => &[(400, false, "seguiemj.ttf")],
        _ => &[],
    }
}

impl Db {
    fn family_id(&mut self, name: &str) -> u16 {
        let key = name.trim().to_ascii_lowercase();
        if let Some(i) = self.families.iter().position(|f| *f == key) {
            return i as u16;
        }
        self.families.push(key);
        (self.families.len() - 1) as u16
    }

    fn load_file(&mut self, file: &str) -> Option<FaceId> {
        let path = fonts_dir().join(file);
        if let Some(r) = self.files.get(&path) {
            return *r;
        }
        let r = std::fs::read(&path).ok().and_then(|bytes| FontArc::try_from_vec(bytes).ok()).map(|f| {
            self.faces.push(Arc::new(Face::new(f)));
            (self.faces.len() - 1) as FaceId
        });
        if r.is_none() {
            log::debug!("font file {} not available", path.display());
        }
        self.files.insert(path, r);
        r
    }

    fn resolve_family(&mut self, family: u16, weight: u16, italic: bool) -> Option<FaceId> {
        // Registered faces first.
        let reg = self
            .registered
            .iter()
            .filter(|r| r.0 == family)
            .min_by_key(|r| (r.2 != italic) as u32 * 10_000 + (r.1 as i32 - weight as i32).unsigned_abs())
            .map(|r| r.3);
        if reg.is_some() {
            return reg;
        }
        let name = self.families[family as usize].clone();
        let mut cands: Vec<(u16, bool, &str)> = family_files(&name).to_vec();
        cands.sort_by_key(|c| (c.1 != italic) as u32 * 10_000 + (c.0 as i32 - weight as i32).unsigned_abs());
        for (_, _, file) in cands {
            if let Some(id) = self.load_file(file) {
                return Some(id);
            }
        }
        None
    }

    fn resolve(&mut self, family: u16, weight: u16, italic: bool) -> Option<FaceId> {
        if let Some(r) = self.resolved.get(&(family, weight, italic)) {
            return *r;
        }
        let mut chain = vec![family];
        for f in ["segoe ui", "arial", "tahoma"] {
            let id = self.family_id(f);
            if !chain.contains(&id) {
                chain.push(id);
            }
        }
        let mut r = None;
        for fam in chain {
            r = self.resolve_family(fam, weight, italic);
            if r.is_some() {
                break;
            }
        }
        if r.is_none() {
            r = self.registered.first().map(|r| r.3).or_else(|| (!self.faces.is_empty()).then_some(0));
        }
        self.resolved.insert((family, weight, italic), r);
        r
    }

    fn fallback_faces(&mut self) -> Vec<FaceId> {
        if let Some(f) = &self.fallbacks {
            return f.clone();
        }
        let mut v = Vec::new();
        for file in ["segoeui.ttf", "seguisym.ttf", "seguiemj.ttf", "arial.ttf"] {
            if let Some(id) = self.load_file(file) {
                v.push(id);
            }
        }
        for r in &self.registered {
            if !v.contains(&r.3) {
                v.push(r.3);
            }
        }
        self.fallbacks = Some(v.clone());
        v
    }

    fn map_char(&mut self, face: FaceId, c: char) -> (FaceId, u16) {
        if let Some(&r) = self.glyphs.get(&(face, c)) {
            return r;
        }
        let id = self.faces[face as usize].font.glyph_id(c);
        let mut r = (face, id.0);
        if id.0 == 0 && !c.is_control() {
            for f in self.fallback_faces() {
                let g = self.faces[f as usize].font.glyph_id(c);
                if g.0 != 0 {
                    r = (f, g.0);
                    break;
                }
            }
        }
        self.glyphs.insert((face, c), r);
        r
    }
}

/// Registers font bytes under a family name (for bundled fonts or tests).
pub fn register_font(family: &str, weight: Weight, italic: bool, bytes: Vec<u8>) -> Option<FaceId> {
    let font = FontArc::try_from_vec(bytes).ok()?;
    let mut d = lock();
    let fam = d.family_id(family);
    d.faces.push(Arc::new(Face::new(font)));
    let id = (d.faces.len() - 1) as FaceId;
    d.registered.push((fam, weight.to_css(), italic, id));
    d.resolved.clear();
    d.fallbacks = None;
    d.glyphs.clear();
    Some(id)
}

pub(crate) fn face(id: FaceId) -> Option<Arc<Face>> {
    lock().faces.get(id as usize).cloned()
}

/// True when at least one font face can be loaded (text will render).
pub fn fonts_available() -> bool {
    Font::default().face_id().is_some()
}

/// `QFont::Weight` (Qt 5 scale).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Weight(pub u8);

impl Weight {
    pub const THIN: Weight = Weight(0);
    pub const EXTRA_LIGHT: Weight = Weight(12);
    pub const LIGHT: Weight = Weight(25);
    pub const NORMAL: Weight = Weight(50);
    pub const MEDIUM: Weight = Weight(57);
    pub const DEMI_BOLD: Weight = Weight(63);
    pub const BOLD: Weight = Weight(75);
    pub const EXTRA_BOLD: Weight = Weight(81);
    pub const BLACK: Weight = Weight(87);

    /// CSS/OpenType weight (100..900), Qt's own mapping.
    pub fn to_css(self) -> u16 {
        match self.0 {
            0..=6 => 100,
            7..=18 => 200,
            19..=37 => 300,
            38..=53 => 400,
            54..=59 => 500,
            60..=68 => 600,
            69..=77 => 700,
            78..=84 => 800,
            _ => 900,
        }
    }
}

impl Default for Weight {
    fn default() -> Self {
        Weight::NORMAL
    }
}

/// `QFont`. `Copy`; the family name is interned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    family: u16,
    point_size: f32,
    pixel_size: f32,
    weight: Weight,
    italic: bool,
    letter_spacing: f32,
    kerning: bool,
}

impl Default for Font {
    /// The application font: Segoe UI 9 pt.
    fn default() -> Self {
        Font::new("Segoe UI", 9.0)
    }
}

impl Font {
    /// `QFont(family, pointSize)`.
    pub fn new(family: &str, point_size: f64) -> Self {
        let family = lock().family_id(family);
        Font {
            family,
            point_size: point_size as f32,
            pixel_size: -1.0,
            weight: Weight::NORMAL,
            italic: false,
            letter_spacing: 0.0,
            kerning: true,
        }
    }

    /// `QFont(family, pointSize, weight)`.
    pub fn with_weight(family: &str, point_size: f64, weight: Weight) -> Self {
        let mut f = Font::new(family, point_size);
        f.weight = weight;
        f
    }

    pub fn family(&self) -> String {
        lock().families[self.family as usize].clone()
    }
    pub fn set_family(&mut self, family: &str) {
        self.family = lock().family_id(family);
    }
    pub fn set_point_size_f(&mut self, pt: f64) {
        self.point_size = pt as f32;
        self.pixel_size = -1.0;
    }
    pub fn set_point_size(&mut self, pt: i32) {
        self.set_point_size_f(pt as f64);
    }
    pub fn set_pixel_size(&mut self, px: i32) {
        if px > 0 {
            self.pixel_size = px as f32;
            self.point_size = -1.0;
        }
    }
    /// Fractional pixel size (no Qt equivalent; handy for scaled art).
    pub fn set_pixel_size_f(&mut self, px: f64) {
        if px > 0.0 {
            self.pixel_size = px as f32;
            self.point_size = -1.0;
        }
    }
    pub fn set_bold(&mut self, bold: bool) {
        self.weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    }
    pub fn bold(&self) -> bool {
        self.weight.0 > Weight::MEDIUM.0
    }
    pub fn set_weight(&mut self, w: Weight) {
        self.weight = w;
    }
    pub fn weight(&self) -> Weight {
        self.weight
    }
    pub fn set_italic(&mut self, i: bool) {
        self.italic = i;
    }
    /// `setLetterSpacing(QFont::AbsoluteSpacing, px)`.
    pub fn set_letter_spacing_absolute(&mut self, px: f64) {
        self.letter_spacing = px as f32;
    }
    pub fn set_kerning(&mut self, k: bool) {
        self.kerning = k;
    }
    /// `pointSizeF()`; derived from the pixel size when that was set.
    pub fn point_size_f(&self) -> f64 {
        if self.point_size > 0.0 {
            self.point_size as f64
        } else {
            self.pixel_size as f64 * 72.0 / logical_dpi() as f64
        }
    }
    /// Em size in logical pixels.
    pub fn pixel_size_f(&self) -> f32 {
        if self.pixel_size > 0.0 { self.pixel_size } else { self.point_size.max(0.0) * logical_dpi() as f32 / 72.0 }
    }

    pub fn face_id(&self) -> Option<FaceId> {
        lock().resolve(self.family, self.weight.to_css(), self.italic)
    }

    pub fn metrics(&self) -> FontMetrics {
        FontMetrics::new(self)
    }
}

/// `Qt::AlignmentFlag` / `Qt::TextFlag` bits (Qt's values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TextFlags(pub u32);

impl TextFlags {
    pub const ALIGN_LEFT: TextFlags = TextFlags(0x1);
    pub const ALIGN_RIGHT: TextFlags = TextFlags(0x2);
    pub const ALIGN_HCENTER: TextFlags = TextFlags(0x4);
    pub const ALIGN_JUSTIFY: TextFlags = TextFlags(0x8);
    pub const ALIGN_TOP: TextFlags = TextFlags(0x20);
    pub const ALIGN_BOTTOM: TextFlags = TextFlags(0x40);
    pub const ALIGN_VCENTER: TextFlags = TextFlags(0x80);
    pub const ALIGN_CENTER: TextFlags = TextFlags(0x84);
    pub const TEXT_SINGLE_LINE: TextFlags = TextFlags(0x100);
    pub const TEXT_DONT_CLIP: TextFlags = TextFlags(0x200);
    pub const TEXT_WORD_WRAP: TextFlags = TextFlags(0x1000);
    pub const TEXT_WRAP_ANYWHERE: TextFlags = TextFlags(0x2000);

    pub fn has(self, f: TextFlags) -> bool {
        self.0 & f.0 == f.0
    }
}

impl std::ops::BitOr for TextFlags {
    type Output = TextFlags;
    fn bitor(self, o: TextFlags) -> TextFlags {
        TextFlags(self.0 | o.0)
    }
}

/// `Qt::TextElideMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElideMode {
    Left,
    Right,
    Middle,
    None,
}

/// A laid-out glyph: face, glyph id and baseline origin in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphPos {
    pub face: FaceId,
    pub glyph: u16,
    pub x: f32,
    pub y: f32,
}

/// `QFontMetrics` / `QFontMetricsF` for one font.
#[derive(Clone)]
pub struct FontMetrics {
    face_id: Option<FaceId>,
    face: Option<Arc<Face>>,
    px: f32,
    spacing: f32,
    kerning: bool,
}

/// Glyph advances are whole pixels, like Qt's GDI engine.
const ROUND_ADVANCES: bool = true;

impl FontMetrics {
    pub fn new(font: &Font) -> Self {
        let face_id = font.face_id();
        FontMetrics {
            face_id,
            face: face_id.and_then(face),
            px: font.pixel_size_f(),
            spacing: font.letter_spacing,
            kerning: font.kerning,
        }
    }

    fn scale(&self) -> f32 {
        self.face.as_ref().map_or(0.0, |f| self.px / f.upem)
    }

    pub fn ascent_f(&self) -> f64 {
        self.face.as_ref().map_or(self.px as f64 * 0.8, |f| (f.ascent * self.scale()) as f64)
    }
    pub fn descent_f(&self) -> f64 {
        self.face.as_ref().map_or(self.px as f64 * 0.2, |f| (f.descent * self.scale()) as f64)
    }
    pub fn leading_f(&self) -> f64 {
        self.face.as_ref().map_or(0.0, |f| (f.line_gap * self.scale()) as f64)
    }
    pub fn ascent(&self) -> i32 {
        self.ascent_f().round() as i32
    }
    pub fn descent(&self) -> i32 {
        self.descent_f().round() as i32
    }
    pub fn leading(&self) -> i32 {
        self.leading_f().round() as i32
    }
    /// `height() = ascent() + descent()`.
    pub fn height(&self) -> i32 {
        self.ascent() + self.descent()
    }
    pub fn line_spacing(&self) -> i32 {
        self.height() + self.leading()
    }
    pub fn height_f(&self) -> f64 {
        self.ascent_f() + self.descent_f()
    }

    /// Lays out one line at the origin; appends glyphs and returns the advance.
    pub(crate) fn layout_line(&self, text: &str, x: f32, y: f32, out: Option<&mut Vec<GlyphPos>>) -> f32 {
        let Some(primary) = self.face_id else {
            return text.chars().count() as f32 * self.px * 0.5;
        };
        let mut db = lock();
        let mut out = out;
        let mut pen = x;
        let mut prev: Option<(FaceId, u16)> = None;
        for c in text.chars() {
            if c == '\n' || c == '\r' {
                continue;
            }
            let c = if c == '\t' || c.is_control() { ' ' } else { c };
            let (fid, gid) = db.map_char(primary, c);
            let f = &db.faces[fid as usize];
            let scale = self.px / f.upem;
            if self.kerning
                && let Some((pf, pg)) = prev
                && pf == fid
            {
                let k = f.font.kern_unscaled(GlyphId(pg), GlyphId(gid)) * scale;
                pen += if ROUND_ADVANCES { k.round() } else { k };
            }
            if let Some(o) = out.as_deref_mut() {
                o.push(GlyphPos { face: fid, glyph: gid, x: pen, y });
            }
            let adv = f.font.h_advance_unscaled(GlyphId(gid)) * scale;
            pen += if ROUND_ADVANCES { adv.round() } else { adv } + self.spacing;
            prev = Some((fid, gid));
        }
        pen - x
    }

    pub fn horizontal_advance_f(&self, text: &str) -> f64 {
        self.layout_line(text, 0.0, 0.0, None) as f64
    }

    /// `horizontalAdvance(text)`.
    pub fn horizontal_advance(&self, text: &str) -> i32 {
        self.horizontal_advance_f(text).round() as i32
    }

    /// `width(text)` (Qt 5 name of `horizontalAdvance`).
    pub fn width(&self, text: &str) -> i32 {
        self.horizontal_advance(text)
    }

    /// `boundingRect(text)`: the text drawn with its baseline at y = 0.
    pub fn bounding_rect(&self, text: &str) -> Rect {
        Rect::new(0.0, -self.ascent() as f64, self.horizontal_advance_f(text), self.height() as f64)
    }

    /// `boundingRect(rect, flags, text)`.
    pub fn bounding_rect_in(&self, rect: Rect, flags: TextFlags, text: &str) -> Rect {
        layout_block(self, rect, flags, text, None)
    }

    /// `elidedText(text, mode, width)` with "…".
    pub fn elided_text(&self, text: &str, mode: ElideMode, width: f64) -> String {
        if mode == ElideMode::None || self.horizontal_advance_f(text) <= width {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let ell = '\u{2026}';
        let fits = |s: &str| self.horizontal_advance_f(s) <= width;
        match mode {
            ElideMode::Right => {
                for n in (0..chars.len()).rev() {
                    let s: String = chars[..n].iter().chain(std::iter::once(&ell)).collect();
                    if fits(&s) {
                        return s;
                    }
                }
            }
            ElideMode::Left => {
                for n in (0..chars.len()).rev() {
                    let s: String = std::iter::once(&ell).chain(chars[chars.len() - n..].iter()).collect();
                    if fits(&s) {
                        return s;
                    }
                }
            }
            ElideMode::Middle => {
                for n in (0..chars.len()).rev() {
                    let l = n.div_ceil(2);
                    let r = n / 2;
                    let s: String =
                        chars[..l].iter().chain(std::iter::once(&ell)).chain(chars[chars.len() - r..].iter()).collect();
                    if fits(&s) {
                        return s;
                    }
                }
            }
            ElideMode::None => {}
        }
        String::new()
    }
}

/// Splits text into display lines for a block layout.
fn break_lines(fm: &FontMetrics, rect_w: f64, flags: TextFlags, text: &str) -> Vec<String> {
    let text = if flags.has(TextFlags::TEXT_SINGLE_LINE) { text.replace('\n', " ") } else { text.to_string() };
    let wrap = flags.has(TextFlags::TEXT_WORD_WRAP) || flags.has(TextFlags::TEXT_WRAP_ANYWHERE);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if !wrap || fm.horizontal_advance_f(para) <= rect_w {
            lines.push(para.to_string());
            continue;
        }
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if fm.horizontal_advance_f(&cand) <= rect_w || cur.is_empty() && !flags.has(TextFlags::TEXT_WRAP_ANYWHERE) {
                cur = cand;
                continue;
            }
            if cur.is_empty() {
                // Wrap anywhere: break the word itself.
                let mut piece = String::new();
                for ch in word.chars() {
                    piece.push(ch);
                    if fm.horizontal_advance_f(&piece) > rect_w && piece.chars().count() > 1 {
                        piece.pop();
                        lines.push(std::mem::take(&mut piece));
                        piece.push(ch);
                    }
                }
                cur = piece;
                continue;
            }
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
            if flags.has(TextFlags::TEXT_WRAP_ANYWHERE) && fm.horizontal_advance_f(&cur) > rect_w {
                let w = std::mem::take(&mut cur);
                let mut piece = String::new();
                for ch in w.chars() {
                    piece.push(ch);
                    if fm.horizontal_advance_f(&piece) > rect_w && piece.chars().count() > 1 {
                        piece.pop();
                        lines.push(std::mem::take(&mut piece));
                        piece.push(ch);
                    }
                }
                cur = piece;
            }
        }
        lines.push(cur);
    }
    lines
}

/// `qt_format_text`: lays text out inside `rect`; returns the bounding rect.
pub(crate) fn layout_block(
    fm: &FontMetrics,
    rect: Rect,
    flags: TextFlags,
    text: &str,
    mut out: Option<&mut Vec<GlyphPos>>,
) -> Rect {
    let lines = break_lines(fm, rect.w, flags, text);
    let asc = fm.ascent() as f64;
    let lh = fm.height() as f64;
    let leading = fm.leading() as f64;
    let n = lines.len().max(1) as f64;
    let total = n * lh + (n - 1.0) * leading;
    let top = if flags.0 & TextFlags::ALIGN_BOTTOM.0 != 0 {
        rect.bottom() - total
    } else if flags.0 & TextFlags::ALIGN_VCENTER.0 != 0 {
        rect.y + (rect.h - total) / 2.0
    } else {
        rect.y
    };
    let mut bounds: Option<Rect> = None;
    for (i, line) in lines.iter().enumerate() {
        let w = fm.horizontal_advance_f(line);
        let x = if flags.0 & TextFlags::ALIGN_RIGHT.0 != 0 {
            rect.right() - w
        } else if flags.0 & TextFlags::ALIGN_HCENTER.0 != 0 {
            rect.x + (rect.w - w) / 2.0
        } else {
            rect.x
        };
        let ly = top + i as f64 * (lh + leading);
        if let Some(o) = out.as_deref_mut() {
            fm.layout_line(line, x as f32, (ly + asc) as f32, Some(o));
        }
        let r = Rect::new(x, ly, w, lh);
        bounds = Some(bounds.map_or(r, |b| b.united(&r)));
    }
    bounds.unwrap_or(Rect::new(rect.x, top, 0.0, lh))
}

/// Glyph outline in em units with y down (1.0 = one em). `None` for blank glyphs.
pub(crate) fn glyph_outline_em(face: &Face, glyph: u16) -> Option<Path> {
    let outline = face.font.outline(GlyphId(glyph))?;
    let s = 1.0 / face.upem as f64;
    let mut p = Path::new();
    p.set_fill_rule(crate::path::FillRule::Winding);
    let mut last: Option<ab_glyph::Point> = None;
    let mp = |q: ab_glyph::Point| (q.x as f64 * s, -(q.y as f64) * s);
    for c in &outline.curves {
        let (start, end) = match c {
            OutlineCurve::Line(a, b) => (*a, *b),
            OutlineCurve::Quad(a, _, b) => (*a, *b),
            OutlineCurve::Cubic(a, _, _, b) => (*a, *b),
        };
        if last != Some(start) {
            if last.is_some() {
                p.close_subpath();
            }
            let (x, y) = mp(start);
            p.move_to(x, y);
        }
        match c {
            OutlineCurve::Line(_, b) => {
                let (x, y) = mp(*b);
                p.line_to(x, y);
            }
            OutlineCurve::Quad(_, c1, b) => {
                let (cx, cy) = mp(*c1);
                let (x, y) = mp(*b);
                p.quad_to(cx, cy, x, y);
            }
            OutlineCurve::Cubic(_, c1, c2, b) => {
                let (ax, ay) = mp(*c1);
                let (bx, by) = mp(*c2);
                let (x, y) = mp(*b);
                p.cubic_to(ax, ay, bx, by, x, y);
            }
        }
        last = Some(end);
    }
    p.close_subpath();
    Some(p)
}

/// A rasterised glyph: coverage bytes and the offset of its top-left pixel
/// from the (subpixel-shifted) glyph origin.
pub(crate) struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub coverage: Vec<u8>,
}

pub(crate) fn rasterize(face: &Face, glyph: u16, px: f32, subpx_x: f32, subpx_y: f32) -> Option<GlyphBitmap> {
    let g =
        ab_glyph::Glyph { id: GlyphId(glyph), scale: face.px_scale(px), position: ab_glyph::point(subpx_x, subpx_y) };
    let og = face.font.outline_glyph(g)?;
    let b = og.px_bounds();
    let (w, h) = (b.width() as u32, b.height() as u32);
    if w == 0 || h == 0 {
        return None;
    }
    let mut cov = vec![0u8; (w * h) as usize];
    og.draw(|x, y, c| {
        if x < w && y < h {
            cov[(y * w + x) as usize] = (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    });
    Some(GlyphBitmap { width: w, height: h, left: b.min.x as i32, top: b.min.y as i32, coverage: cov })
}

impl Path {
    /// `QPainterPath::addText(point, font, text)`: glyph outlines with the
    /// baseline at `y`. Sets the winding fill rule, as Qt does.
    pub fn add_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
        let fm = FontMetrics::new(font);
        let mut glyphs = Vec::new();
        fm.layout_line(text, x as f32, y as f32, Some(&mut glyphs));
        let px = font.pixel_size_f() as f64;
        let mut cache: HashMap<(FaceId, u16), Option<Path>> = HashMap::new();
        for g in glyphs {
            let outline = cache
                .entry((g.face, g.glyph))
                .or_insert_with(|| face(g.face).and_then(|f| glyph_outline_em(&f, g.glyph)));
            if let Some(o) = outline {
                let t = crate::Transform::new(px, 0.0, 0.0, px, g.x as f64, g.y as f64);
                self.add_path(&o.transformed(&t));
            }
        }
        self.set_fill_rule(crate::path::FillRule::Winding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_map_like_qt() {
        assert_eq!(Weight::NORMAL.to_css(), 400);
        assert_eq!(Weight::BOLD.to_css(), 700);
        assert_eq!(Weight::DEMI_BOLD.to_css(), 600);
        assert_eq!(Weight::BLACK.to_css(), 900);
        assert_eq!(Weight::LIGHT.to_css(), 300);
    }

    #[test]
    fn sizes_convert_at_96_dpi() {
        let mut f = Font::new("Segoe UI", 9.0);
        assert!((f.pixel_size_f() - 12.0).abs() < 1e-6);
        f.set_pixel_size(20);
        assert!((f.point_size_f() - 15.0).abs() < 1e-9);
        f.set_bold(true);
        assert!(f.bold());
    }

    #[test]
    fn segoe_metrics_match_qt() {
        if !fonts_available() {
            eprintln!("no fonts; skipping");
            return;
        }
        let fm = Font::new("Segoe UI", 9.0).metrics();
        // Qt on Windows: Segoe UI 9pt -> ascent 13, descent 3, height 16.
        assert_eq!((fm.ascent(), fm.descent(), fm.height()), (13, 3, 16));
        let w = fm.horizontal_advance("Hello world");
        assert!((55..=70).contains(&w), "{w}");
        assert_eq!(fm.horizontal_advance(""), 0);
        let bold = Font::with_weight("Segoe UI", 9.0, Weight::BOLD).metrics();
        assert!(bold.horizontal_advance("Hello world") > w);
        // Doubling the size roughly doubles the advance.
        let big = Font::new("Segoe UI", 18.0).metrics();
        let w2 = big.horizontal_advance("Hello world");
        assert!((w2 - 2 * w).abs() <= 6, "{w} {w2}");
        let e = fm.elided_text("A fairly long label that will not fit", ElideMode::Right, 60.0);
        assert!(e.ends_with('\u{2026}') && fm.horizontal_advance_f(&e) <= 60.0);
        // Unknown family falls back to Segoe UI.
        let fb = Font::new("No Such Family", 9.0).metrics();
        assert_eq!(fb.height(), 16);
    }

    #[test]
    fn block_layout_wraps_and_aligns() {
        if !fonts_available() {
            return;
        }
        let fm = Font::new("Segoe UI", 9.0).metrics();
        let r = Rect::new(0.0, 0.0, 80.0, 200.0);
        let b = fm.bounding_rect_in(
            r,
            TextFlags::TEXT_WORD_WRAP | TextFlags::ALIGN_HCENTER,
            "one two three four five six seven",
        );
        assert!(b.w <= 80.0 && b.h >= 32.0, "{b:?}");
        let c = fm.bounding_rect_in(r, TextFlags::ALIGN_CENTER, "hi");
        assert!((c.center().0 - 40.0).abs() < 1.0 && (c.center().1 - 100.0).abs() < 1.0);
        let mut p = Path::new();
        p.add_text(0.0, 20.0, &Font::new("Segoe UI", 12.0), "O");
        let bb = p.bounding_rect();
        assert!(bb.bottom() <= 20.5 && bb.y > 2.0 && bb.w > 5.0, "{bb:?}");
        // The 'O' has a hole: its centre is outside the glyph.
        let (cx, cy) = bb.center();
        assert!(!p.contains(cx, cy));
    }
}
