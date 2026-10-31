//! Partial repaints for the software overlay (`request_repaint`,
//! `_current_paint_region` and `_paint`'s clipped clear in `app/engine.py`).
//!
//! Each Companion frame repaints only the area round the spiders, flies,
//! cursor silk, animating webs and cage fences: this frame's footprints
//! united with last frame's, so whatever moved away is cleared. A full
//! repaint runs on request ([`SoftOverlay::request_full`]: monitors, preset
//! reloads, menu actions) and every [`FULL_REPAINT_SAFETY_FRAMES`] frames.
//! Adventure repaints everything every frame, as Python did with a mission.
//!
//! Python clipped the painter to the region; here the scene is drawn
//! unclipped (creatures outside the region's bounding rect culled, as
//! Python's `clipBoundingRect` did) into a frame whose dirty rects were
//! cleared first, and only those rects reach the window
//! (`OverlayWindow::present_premultiplied_rects`). Inside them the pixels
//! are exactly a full repaint's; outside them the window keeps its own.
//!
//! One addition to Python: the carcasses' footprints (`carcass_dirty_rects`,
//! which Python defined for this and never used) are repainted while they
//! fade, and once after they go. Bases are left out, as in Python.

use dc_core::{IRect, Rect};
use dc_manager::CreatureManager;
use dc_paint::world::Clip;
use dc_render::{CpuRenderer, Image, Region, Scene};

/// Extra slack round each creature: antennae and leg tips swing past the
/// body's box in fast animation.
pub const CREATURE_REPAINT_EXTRA_PAD_PX: f64 = 28.0;
pub const CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT: f64 = 1.9;
/// The periodic whole-window repaint, a safety net.
pub const FULL_REPAINT_SAFETY_FRAMES: u32 = 150;

type Xywh = (f64, f64, f64, f64);

/// `_rect_from_bbox`: `(x0, y0, x1, y1)` to a `QRect` one pixel larger.
pub fn rect_from_bbox((x0, y0, x1, y1): (f64, f64, f64, f64)) -> IRect {
    let (left, top) = (x0.floor() as i32, y0.floor() as i32);
    let (right, bottom) = (x1.ceil() as i32, y1.ceil() as i32);
    IRect::new(left, top, (right - left).max(1) + 1, (bottom - top).max(1) + 1)
}

/// `_expand_rect`: `rect.adjusted(-pad, -pad, pad, pad)`.
pub fn expand_rect(r: IRect, pad: i32) -> IRect {
    if pad <= 0 {
        return r;
    }
    IRect::new(r.x - pad, r.y - pad, r.w + 2 * pad, r.h + 2 * pad)
}

/// `_rect_from_xywh`.
pub fn rect_from_xywh((x, y, w, h): Xywh) -> IRect {
    IRect::new(x.floor() as i32, y.floor() as i32, w.ceil() as i32 + 1, h.ceil() as i32 + 1)
}

fn is_empty(r: &IRect) -> bool {
    r.w <= 0 || r.h <= 0
}

/// `QRect::intersects`.
fn intersects(a: &IRect, b: &IRect) -> bool {
    !is_empty(a) && !is_empty(b) && a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// `QRect::united`.
fn united(a: &IRect, b: &IRect) -> IRect {
    if is_empty(a) {
        return *b;
    }
    if is_empty(b) {
        return *a;
    }
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    IRect::new(x, y, a.right().max(b.right()) - x, a.bottom().max(b.bottom()) - y)
}

/// `region += rect`.
fn add(region: &mut Region, r: IRect) {
    if !is_empty(&r) {
        region.unite_rect(&r.to_rect());
    }
}

/// What the partial repaint remembers between frames.
#[derive(Default)]
pub struct RepaintState {
    prev_region: Region,
    full_pending: bool,
    frames_since_full: u32,
    /// `_cage_fp_prev`, by cage index (Python keyed it by `id(cage)`; adding
    /// or removing a cage asks for a full repaint here).
    cage_fp_prev: Vec<Xywh>,
    /// The cage drawn with its active wash last frame.
    cage_active_prev: Option<usize>,
    carcass_fp_prev: Vec<Xywh>,
    /// Each base's footprint and what changes its look, by base id.
    base_prev: Vec<(String, BaseLook)>,
}

/// What a base's pixels depend on: its place, size, build and alert.
type BaseLook = (Xywh, f64, f64);

impl RepaintState {
    pub fn new() -> Self {
        RepaintState { full_pending: true, ..Default::default() }
    }

    /// `_request_full_repaint`.
    pub fn request_full(&mut self) {
        self.full_pending = true;
    }

    /// `request_repaint` for one Companion frame: `None` for a full repaint,
    /// else the dirty region (this frame's footprints and last frame's)
    /// inside the `size` window.
    pub fn next(&mut self, m: &mut CreatureManager, size: (u32, u32)) -> Option<Region> {
        self.frames_since_full += 1;
        if self.full_pending || self.frames_since_full >= FULL_REPAINT_SAFETY_FRAMES {
            self.full_pending = false;
            self.frames_since_full = 0;
            self.prev_region = self.current_paint_region(m);
            return None;
        }
        let cur = self.current_paint_region(m);
        let mut dirty = cur.united(&self.prev_region);
        self.prev_region = cur;
        dirty.intersect_rect(&Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64));
        Some(dirty)
    }

    /// `_current_paint_region`. Updates each creature's `bbox_prev`, the
    /// cages' and carcasses' last footprints, and drains the cursor silk's
    /// dirty rects.
    pub fn current_paint_region(&mut self, m: &mut CreatureManager) -> Region {
        let mut region = Region::new();
        let show_names = m.always_show_names;
        for c in m.creatures.iter_mut() {
            let bbox = c.bounding_rect(show_names, m.label_metrics.as_ref());
            // Generous on purpose: an antenna's old pixels show on a dark
            // desktop if the rect misses by a pixel or two.
            let extra_pad = CREATURE_REPAINT_EXTRA_PAD_PX.max(c.kin.size * CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT) as i32;
            let r = expand_rect(rect_from_bbox(bbox), extra_pad);
            add(&mut region, r);
            if let Some(prev) = c.bbox_prev {
                let pr = expand_rect(rect_from_bbox(prev), extra_pad);
                if !intersects(&r, &pr) {
                    // The streak between two far-apart positions (a throw,
                    // a panic jump), so no one-frame trail is left.
                    add(&mut region, united(&r, &pr));
                } else {
                    add(&mut region, pr);
                }
            }
            c.bbox_prev = Some(bbox);
        }
        let mut cage_fp = Vec::with_capacity(m.cages.len());
        for (i, cage) in m.cages.iter().enumerate() {
            let fp = cage.footprint_rect(6.0);
            match self.cage_fp_prev.get(i) {
                // Moved or resized: the whole old and new footprints, so the
                // interior wash leaves no ghost.
                Some(&prev) if prev != fp => {
                    add(&mut region, rect_from_xywh(prev));
                    add(&mut region, rect_from_xywh(fp));
                }
                // Standing still: only the fence bands.
                _ => {
                    for band in cage.border_rects() {
                        add(&mut region, rect_from_xywh(band));
                    }
                }
            }
            cage_fp.push(fp);
        }
        // A cage starting or ending a drag gains or loses its interior wash.
        let active = m.dragged_cage();
        if active != self.cage_active_prev {
            for i in [active, self.cage_active_prev].into_iter().flatten() {
                if let Some(&fp) = cage_fp.get(i).or(self.cage_fp_prev.get(i)) {
                    add(&mut region, rect_from_xywh(fp));
                }
            }
        }
        self.cage_active_prev = active;
        self.cage_fp_prev = cage_fp;
        // Bases: moved, rebuilt, grown or alerting (the alert ring fades),
        // old and new footprints. Python left bases out of the region; on
        // the software path that left a carried base's old pixels behind.
        let bases: Vec<(String, BaseLook)> = m
            .base_world
            .bases
            .iter()
            .map(|b| {
                let half = (30.0 + b.level as f64 * 10.0) * 1.25 + 8.0;
                (b.id.clone(), ((b.x - half, b.y - half, half * 2.0, half * 2.0), b.build_progress, b.alert))
            })
            .collect();
        for (id, look) in &bases {
            let prev = self.base_prev.iter().find(|(p, _)| p == id).map(|(_, l)| *l);
            if prev != Some(*look) || look.2 > 0.0 {
                add(&mut region, rect_from_xywh(look.0));
                if let Some(p) = prev {
                    add(&mut region, rect_from_xywh(p.0));
                }
            }
        }
        for (id, look) in &self.base_prev {
            if !bases.iter().any(|(b, _)| b == id) {
                add(&mut region, rect_from_xywh(look.0));
            }
        }
        self.base_prev = bases;
        // Fly spawners breathe and can be dragged: always their footprint
        // (the previous one comes from last frame's region). A nest far from
        // everything else widens the culling box, so the software path then
        // draws more than it repaints; pixels stay right.
        for sp in &m.fly_world.spawners {
            let half = sp.radius * 1.2 + 4.0;
            add(&mut region, rect_from_xywh((sp.x - half, sp.y - half, half * 2.0, half * 2.0)));
        }
        let carcass_fp = m.carcass_dirty_rects();
        for fp in self.carcass_fp_prev.iter().chain(&carcass_fp) {
            add(&mut region, rect_from_xywh(*fp));
        }
        self.carcass_fp_prev = carcass_fp;
        // Building, bouncing or just-removed webs; idle ones stay on the
        // window and are redrawn only under something else's rect.
        for fp in m.web_world.dirty_rects() {
            add(&mut region, rect_from_xywh(*fp));
        }
        // Cursor silk moves with the pointer while active.
        for fp in m.mouse_web_world.dirty_rects() {
            add(&mut region, rect_from_xywh(fp));
        }
        // Each live fly's current and previous footprint; removed flies' last.
        for fp in m.fly_world.dirty_rects() {
            add(&mut region, rect_from_xywh(*fp));
        }
        region
    }
}

/// The culling clip for the painter: `clipBoundingRect()` grown by 2 px.
pub fn painter_clip(region: &Region) -> Clip {
    let b = region.bounding_rect();
    (b.x - 2.0, b.y - 2.0, b.right() + 2.0, b.bottom() + 2.0)
}

/// The region as integer rects.
pub fn region_rects(region: &Region) -> Vec<IRect> {
    region.rects().iter().map(|r| IRect::new(r.x as i32, r.y as i32, r.w as i32, r.h as i32)).collect()
}

/// Clear `rects` of a premultiplied RGBA frame (`CompositionMode_Clear`).
pub fn clear_rects(image: &mut Image, rects: &[IRect]) {
    let (w, h) = image.size();
    let stride = w as usize * 4;
    let data = image.data_mut();
    for r in rects {
        let (x0, x1) = (r.x.clamp(0, w as i32) as usize, r.right().clamp(0, w as i32) as usize);
        let (y0, y1) = (r.y.clamp(0, h as i32) as usize, r.bottom().clamp(0, h as i32) as usize);
        if x0 >= x1 {
            continue;
        }
        for y in y0..y1 {
            data[y * stride + x0 * 4..y * stride + x1 * 4].fill(0);
        }
    }
}

/// The software overlay's renderer: the frame it draws into and the
/// partial-repaint state.
pub struct SoftOverlay {
    cpu: CpuRenderer,
    frame: Image,
    state: RepaintState,
    /// This frame's dirty rects (from [`SoftOverlay::plan`]); `None` repaints
    /// everything.
    dirty: Option<Vec<IRect>>,
}

impl Default for SoftOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftOverlay {
    pub fn new() -> Self {
        SoftOverlay { cpu: CpuRenderer::new(), frame: Image::new(0, 0), state: RepaintState::new(), dirty: None }
    }

    /// `_request_full_repaint`.
    pub fn request_full(&mut self) {
        self.state.request_full();
    }

    /// Decide this Companion frame's repaint before the scene is built: the
    /// clip to cull with, `None` for a full repaint. Without a call the next
    /// [`SoftOverlay::render`] repaints everything (Adventure).
    pub fn plan(&mut self, m: &mut CreatureManager, size: (u32, u32)) -> Option<Clip> {
        let region = self.state.next(m, size)?;
        let clip = painter_clip(&region);
        self.dirty = Some(region_rects(&region));
        // An empty region still culls everything.
        Some(clip)
    }

    /// Draw `scene` and return the frame with the rects that changed (`None`:
    /// all of it). The frame is premultiplied RGBA, `size` large.
    pub fn render(&mut self, scene: &Scene, size: (u32, u32)) -> (&[u8], Option<Vec<IRect>>) {
        let mut dirty = self.dirty.take();
        if self.frame.size() != size {
            self.frame = Image::new(size.0, size.1);
            dirty = None;
        }
        match &dirty {
            Some(rects) => clear_rects(&mut self.frame, rects),
            None => self.frame.data_mut().fill(0),
        }
        if dirty.as_ref().is_none_or(|r| !r.is_empty()) {
            self.cpu.render_into(scene, &mut self.frame, None);
        }
        (self.frame.data(), dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_core::PyRandom;
    use dc_manager::MouseInput;
    use dc_manager::paint::ScenePainter;
    use dc_paint::PaintSwitches;
    use dc_render::RenderHint;
    use std::path::{Path, PathBuf};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("dc-repaint-{name}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// The default preset's spiders on a small desktop, saving into `state`.
    fn manager(state: &Path, size: (f64, f64)) -> (CreatureManager, PyRandom) {
        let mut rng = PyRandom::from_int(11);
        let preset = root().join("presets/default.json");
        let m = CreatureManager::new(&root(), &preset, size, Some(11), state, false, &mut rng)
            .expect("the default preset loads");
        assert!(!m.creatures.is_empty());
        (m, rng)
    }

    fn step(m: &mut CreatureManager, rng: &mut PyRandom) {
        m.update(1.0 / 60.0, -100000.0, -100000.0, MouseInput::default(), rng);
    }

    fn scene(painter: &mut ScenePainter, m: &mut CreatureManager, clip: Option<Clip>) -> Scene {
        let mut scene = Scene::new();
        {
            let mut canvas = scene.canvas();
            canvas.set_render_hint(RenderHint::Antialiasing, true);
            painter.paint(m, &mut canvas, clip);
        }
        scene
    }

    /// What `present_premultiplied_rects` does to the window's bitmap,
    /// less the BGRA swizzle.
    fn present(window: &mut Image, frame: &[u8], dirty: Option<&[IRect]>) {
        let (w, h) = window.size();
        let full = [IRect::new(0, 0, w as i32, h as i32)];
        let stride = w as usize * 4;
        let data = window.data_mut();
        for r in dirty.unwrap_or(&full) {
            let (x0, x1) = (r.x.clamp(0, w as i32) as usize, r.right().clamp(0, w as i32) as usize);
            for y in r.y.clamp(0, h as i32) as usize..r.bottom().clamp(0, h as i32) as usize {
                let row = y * stride + x0 * 4..y * stride + x1.max(x0) * 4;
                data[row.clone()].copy_from_slice(&frame[row]);
            }
        }
    }

    fn covers(region: &Region, r: IRect) -> bool {
        let mut rest = Region::from_rect(r.to_rect());
        for q in region.rects() {
            rest.subtract_rect(q);
        }
        rest.is_empty()
    }

    /// The first creature's padded repaint rect where it stands now.
    fn creature_rect(m: &mut CreatureManager) -> IRect {
        let c = &mut m.creatures[0];
        let pad = CREATURE_REPAINT_EXTRA_PAD_PX.max(c.kin.size * CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT) as i32;
        let bbox = c.bounding_rect(false, m.label_metrics.as_ref());
        expand_rect(rect_from_bbox(bbox), pad)
    }

    #[test]
    fn rects_round_like_qrect() {
        // QRect(floor, floor, max(1, ceil - floor) + 1, ...).
        assert_eq!(rect_from_bbox((10.2, 20.9, 30.1, 40.0)), IRect::new(10, 20, 22, 21));
        assert_eq!(rect_from_bbox((5.0, 5.0, 5.0, 5.0)), IRect::new(5, 5, 2, 2));
        assert_eq!(rect_from_bbox((-3.5, -0.5, 1.5, 0.5)), IRect::new(-4, -1, 7, 3));
        assert_eq!(expand_rect(IRect::new(10, 20, 5, 6), 3), IRect::new(7, 17, 11, 12));
        assert_eq!(expand_rect(IRect::new(10, 20, 5, 6), 0), IRect::new(10, 20, 5, 6));
        assert_eq!(rect_from_xywh((1.5, 2.5, 10.2, 3.0)), IRect::new(1, 2, 12, 4));
        // QRect::intersects and united.
        let a = IRect::new(0, 0, 10, 10);
        assert!(intersects(&a, &IRect::new(9, 9, 5, 5)));
        assert!(!intersects(&a, &IRect::new(10, 0, 5, 5)));
        assert!(!intersects(&a, &IRect::new(5, 5, 0, 5)));
        assert_eq!(united(&a, &IRect::new(20, 30, 5, 5)), IRect::new(0, 0, 25, 35));
        assert_eq!(united(&IRect::new(3, 3, 0, 0), &a), a);
    }

    #[test]
    fn first_frame_requests_and_every_150th_repaint_everything() {
        let state = TempDir::new("requests");
        let (mut m, mut rng) = manager(&state.0, (800.0, 500.0));
        let mut s = RepaintState::new();
        assert!(s.next(&mut m, (800, 500)).is_none(), "the first frame is whole");
        step(&mut m, &mut rng);
        let region = s.next(&mut m, (800, 500)).expect("then partial");
        assert!(!region.is_empty());
        assert!(region.area() < 800.0 * 500.0);
        s.request_full();
        assert!(s.next(&mut m, (800, 500)).is_none());
        for i in 1..FULL_REPAINT_SAFETY_FRAMES {
            step(&mut m, &mut rng);
            assert!(s.next(&mut m, (800, 500)).is_some(), "frame {i} is partial");
        }
        assert!(s.next(&mut m, (800, 500)).is_none(), "frame 150 is whole");
    }

    #[test]
    fn region_is_this_frames_footprints_and_last_frames() {
        let state = TempDir::new("footprints");
        let (mut m, _) = manager(&state.0, (1600.0, 900.0));
        m.creatures.truncate(1);
        // Body and feet moved together.
        let place = |m: &mut CreatureManager, x: f64, y: f64| {
            let kin = &mut m.creatures[0].kin;
            let (dx, dy) = (x - kin.x, y - kin.y);
            kin.x = x;
            kin.y = y;
            for leg in &mut kin.legs {
                leg.foot_x += dx;
                leg.foot_y += dy;
            }
        };
        place(&mut m, 300.0, 450.0);
        let mut s = RepaintState::new();
        s.next(&mut m, (1600, 900));
        let old = creature_rect(&mut m);
        // A throw: far away in one frame.
        place(&mut m, 1300.0, 450.0);
        let new = creature_rect(&mut m);
        assert!(!intersects(&old, &new));
        let region = s.next(&mut m, (1600, 900)).unwrap();
        assert!(covers(&region, old), "where it was");
        assert!(covers(&region, new), "where it is");
        assert!(covers(&region, united(&old, &new)), "and the streak between");
        // Standing still: last frame's region once more, then only its box.
        s.next(&mut m, (1600, 900));
        let region = s.next(&mut m, (1600, 900)).unwrap();
        assert!(covers(&region, new));
        assert!(!region.intersects_rect(&old.to_rect()), "the old place is done");
    }

    #[test]
    fn a_still_cage_repaints_its_fence_and_a_moved_one_both_footprints() {
        let state = TempDir::new("cages");
        let (mut m, _) = manager(&state.0, (1600.0, 900.0));
        m.creatures.clear();
        m.add_cage(Some(800.0), Some(450.0), 400.0, 300.0);
        let mut s = RepaintState::new();
        s.next(&mut m, (1600, 900));
        let region = s.next(&mut m, (1600, 900)).unwrap();
        for band in m.cages[0].border_rects() {
            assert!(covers(&region, rect_from_xywh(band)));
        }
        assert!(!region.contains(800.0, 450.0), "not the open interior");
        let before = m.cages[0].footprint_rect(6.0);
        m.cages[0].x += 200.0;
        let region = s.next(&mut m, (1600, 900)).unwrap();
        assert!(covers(&region, rect_from_xywh(before)), "the old footprint");
        assert!(covers(&region, rect_from_xywh(m.cages[0].footprint_rect(6.0))), "the new one");
    }

    #[test]
    fn partial_frames_match_full_repaints_pixel_for_pixel() {
        let state = TempDir::new("pixels");
        let size = (720u32, 460u32);
        let (mut m, mut rng) = manager(&state.0, (size.0 as f64, size.1 as f64));
        m.add_cage(Some(360.0), Some(230.0), 260.0, 180.0);
        m.add_fly_spawner();
        let mut soft = SoftOverlay::new();
        let mut painter = ScenePainter::new(PaintSwitches::default());
        let mut reference = ScenePainter::new(PaintSwitches::default());
        let mut cpu = CpuRenderer::new();
        let mut window = Image::new(size.0, size.1);
        let (frames, mut partial, mut share) = (200, 0, 0.0);
        for frame in 0..frames {
            step(&mut m, &mut rng);
            let clip = soft.plan(&mut m, size);
            let s = scene(&mut painter, &mut m, clip);
            let (pixels, dirty) = soft.render(&s, size);
            if let Some(d) = &dirty {
                partial += 1;
                share += d.iter().map(|r| (r.w * r.h) as f64).sum::<f64>() / (size.0 * size.1) as f64;
            }
            present(&mut window, pixels, dirty.as_deref());
            let full = cpu.render(&scene(&mut reference, &mut m, None), size.0, size.1, None);
            let differing = window
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(full.data().as_chunks::<4>().0)
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(differing, 0, "frame {frame}: pixels differing from a full repaint");
        }
        assert_eq!(partial, frames - 2, "all but the first and the 150th");
        assert!(share / (partial as f64) < 0.9, "partial frames repainted {:.0}%", 100.0 * share / partial as f64);
    }

    /// A carried and rebuilt base, a dragged spawner and a cage taking and
    /// losing its drag wash leave nothing behind on a partial frame.
    #[test]
    fn bases_spawners_and_cage_drags_leave_no_stale_pixels() {
        let state = TempDir::new("movers");
        let size = (720u32, 460u32);
        let (mut m, mut rng) = manager(&state.0, (size.0 as f64, size.1 as f64));
        m.creatures.truncate(1);
        m.add_cage(Some(500.0), Some(300.0), 200.0, 140.0);
        m.add_fly_spawner();
        let mut base = dc_world::jobs::BaseSite::new("test-base", "owner", "neutral", 150.0, 150.0);
        base.build_progress = 100.0;
        m.base_world.bases.push(base);
        let mut soft = SoftOverlay::new();
        let mut painter = ScenePainter::new(PaintSwitches::default());
        let mut reference = ScenePainter::new(PaintSwitches::default());
        let mut cpu = CpuRenderer::new();
        let mut window = Image::new(size.0, size.1);
        for frame in 0..120 {
            match frame {
                10..=30 => m.base_world.bases[0].x += 6.0,
                31 => m.base_world.bases[0].level += 1,
                35 => m.base_world.bases[0].alert = 1.0,
                40..=60 => {
                    let sp = &mut m.fly_world.spawners[0];
                    sp.x = 120.0 + (frame - 40) as f64 * 9.0;
                    sp.y = 380.0;
                }
                70 => {
                    let c = &m.cages[0];
                    let (x, y) = (c.x, c.y + c.h * 0.5);
                    assert!(m.start_cage_drag(x, y), "the left fence takes the drag");
                }
                80 => {
                    let input = MouseInput { released: true, ..MouseInput::default() };
                    m.update(1.0 / 60.0, -100000.0, -100000.0, input, &mut rng);
                }
                _ => {}
            }
            step(&mut m, &mut rng);
            let clip = soft.plan(&mut m, size);
            let s = scene(&mut painter, &mut m, clip);
            let (pixels, dirty) = soft.render(&s, size);
            present(&mut window, pixels, dirty.as_deref());
            let full = cpu.render(&scene(&mut reference, &mut m, None), size.0, size.1, None);
            let differing = window
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(full.data().as_chunks::<4>().0)
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(differing, 0, "frame {frame}: pixels differing from a full repaint");
        }
    }
}
