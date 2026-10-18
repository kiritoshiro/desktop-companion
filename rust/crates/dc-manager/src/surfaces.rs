//! Port of `manager/surfaces.py`. Window snapshots are explicit OS inputs.
use crate::{CreatureManager, constants::clamp};
use dc_core::{PyRandom, math::py_round};
use dc_creature::{Creature, LabelMetrics, State};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSurface {
    pub kind: String,
    pub class_name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
type Bbox = (f64, f64, f64, f64);
impl DesktopSurface {
    pub fn key(&self) -> String {
        serde_json::to_string(&(
            self.kind.as_str(),
            self.class_name.as_str(),
            py_round(self.x / 8.0) as i64,
            py_round(self.y / 8.0) as i64,
            py_round(self.w / 8.0) as i64,
            py_round(self.h / 8.0) as i64,
        ))
        .unwrap()
    }
    pub fn contains(&self, x: f64, y: f64, inset: f64) -> bool {
        self.x + inset <= x && x <= self.x + self.w - inset && self.y + inset <= y && y <= self.y + self.h - inset
    }
    pub fn inside_depth(&self, x: f64, y: f64) -> f64 {
        if !self.contains(x, y, 0.0) {
            0.0
        } else {
            (x - self.x).min(self.x + self.w - x).min(y - self.y).min(self.y + self.h - y).max(0.0)
        }
    }
    pub fn intersects(&self, b: Bbox) -> bool {
        !(b.2 <= self.x || b.0 >= self.x + self.w || b.3 <= self.y || b.1 >= self.y + self.h)
    }
    pub fn covers(&self, b: Bbox) -> bool {
        b.0 >= self.x && b.2 <= self.x + self.w && b.1 >= self.y && b.3 <= self.y + self.h
    }
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        (self.x - x).max(0.0).max(x - self.x - self.w).hypot((self.y - y).max(0.0).max(y - self.y - self.h))
    }
    pub fn rect(&self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.w, self.h)
    }
}
pub fn personality_float(c: &Creature, key: &str, default: f64) -> f64 {
    c.personality.get(key).and_then(dc_content::util::py_float).unwrap_or(default)
}
pub fn personality_range(c: &Creature, key: &str, low: f64, high: f64, rng: &mut PyRandom) -> f64 {
    if let Some(v) = c.personality.get(key) {
        if let Some(a) = v.as_array() {
            if a.len() >= 2 {
                if let (Some(a), Some(b)) = (dc_content::util::py_float(&a[0]), dc_content::util::py_float(&a[1])) {
                    return rng.uniform(a.min(b), a.max(b));
                }
            }
        } else if let Some(n) = dc_content::util::py_float(v) {
            return n;
        }
    }
    rng.uniform(low, high)
}
pub fn clear_desktop_hide_intent(c: &mut Creature) {
    c.desktop_hide_surface_key = None;
    c.desktop_hide_kind = None;
    c.desktop_hide_timer = 0.0;
    c.folder_portal_dwell = 0.0;
}
struct Context<'a> {
    surfaces: &'a [DesktopSurface],
    w: f64,
    h: f64,
    mx: f64,
    my: f64,
    down: bool,
    metrics: &'a dyn LabelMetrics,
}
impl Context<'_> {
    fn can_occlude(&self, s: &DesktopSurface) -> bool {
        s.w * s.h / (self.w * self.h).max(1.0) < 0.86
            && !(s.w >= self.w * 0.96 && s.h >= self.h * 0.82)
            && !(s.w >= self.w * 0.82 && s.h >= self.h * 0.96)
    }
    fn higher(&self, i: usize) -> impl Iterator<Item = &DesktopSurface> {
        self.surfaces[..i].iter().filter(|s| self.can_occlude(s))
    }
    fn visible_at(&self, i: usize, x: f64, y: f64) -> bool {
        self.surfaces[i].contains(x, y, 0.0) && !self.higher(i).any(|s| s.contains(x, y, 0.0))
    }
    fn visible_area(&self, i: usize, b: Bbox) -> bool {
        let s = &self.surfaces[i];
        let (x0, y0, x1, y1) = (b.0.max(s.x), b.1.max(s.y), b.2.min(s.x + s.w), b.3.min(s.y + s.h));
        if x1 <= x0 || y1 <= y0 {
            return false;
        }
        [
            ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
            (x0 + 1.0, y0 + 1.0),
            (x1 - 1.0, y0 + 1.0),
            (x0 + 1.0, y1 - 1.0),
            (x1 - 1.0, y1 - 1.0),
        ]
        .iter()
        .any(|&(x, y)| self.visible_at(i, x, y))
    }
    fn allows_hide(&self, c: &Creature) -> bool {
        if c.dragging || c.airborne || c.cage.is_some() {
            return false;
        }
        if matches!(
            c.state,
            State::Alert
                | State::Approach
                | State::Chase
                | State::Retreat
                | State::Startled
                | State::Aim
                | State::Inspect
                | State::Observe
                | State::Jump
                | State::Coil
                | State::Land
                | State::Roll
                | State::Catch
                | State::Play
                | State::Cuddle
                | State::Dragged
                | State::Zoom
        ) {
            return false;
        }
        if !c.desktop_fully_hidden {
            let safety =
                personality_float(c, "desktop_hide_mouse_safety_radius", 260.0) * if self.down { 1.7 } else { 1.0 };
            if (c.kin.x - self.mx).hypot(c.kin.y - self.my) < safety.max(80.0) {
                return false;
            }
        }
        true
    }
    fn occluder(&self, c: &mut Creature) -> Option<usize> {
        if c.dragging || c.desktop_spawn_grace > 0.0 {
            return None;
        }
        let key = c.desktop_hide_surface_key.as_ref()?;
        let Some(i) = self.surfaces.iter().position(|s| s.key() == *key) else {
            clear_desktop_hide_intent(c);
            return None;
        };
        let s = &self.surfaces[i];
        if !self.can_occlude(s) || (!self.allows_hide(c) && !c.desktop_fully_hidden) {
            clear_desktop_hide_intent(c);
            return None;
        }
        let bbox = c.bounding_rect(false, self.metrics);
        if !s.intersects(bbox) {
            if !s.contains(c.kin.x, c.kin.y, 0.0) {
                clear_desktop_hide_intent(c);
            }
            return None;
        }
        self.visible_area(i, bbox).then_some(i)
    }
    fn fully_covered(&self, c: &mut Creature, i: usize) -> bool {
        if c.dragging {
            return false;
        }
        let b = c.bounding_rect(false, self.metrics);
        self.surfaces[i].covers(b) && !self.higher(i).any(|s| s.intersects(b))
    }
    fn folders(&self) -> Vec<usize> {
        self.surfaces
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                s.kind == "folder" && self.can_occlude(s) && self.visible_at(*i, s.x + s.w * 0.5, s.y + s.h * 0.5)
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn containing(&self, c: &Creature, kind: Option<&str>, depth: f64) -> Option<usize> {
        let mut best = None;
        let mut bd = -1.0;
        for (i, s) in self.surfaces.iter().enumerate() {
            if kind.is_some_and(|k| s.kind != k) {
                continue;
            }
            let d = s.inside_depth(c.kin.x, c.kin.y);
            if d >= depth && d > bd {
                best = Some(i);
                bd = d;
            }
        }
        best
    }
    fn point_inside(&self, s: &DesktopSurface, c: &Creature, rng: &mut PyRandom) -> (f64, f64) {
        let m = (c.kin.size * 1.2).clamp(18.0, 80.0);
        if s.w <= m * 2.0 || s.h <= m * 2.0 {
            (s.x + s.w * 0.5, s.y + s.h * 0.5)
        } else {
            (rng.uniform(s.x + m, s.x + s.w - m), rng.uniform(s.y + m, s.y + s.h - m))
        }
    }
    fn exit_target(&self, c: &Creature, s: &DesktopSurface) -> (f64, f64) {
        let distances =
            [(c.kin.x - s.x).abs(), (s.x + s.w - c.kin.x).abs(), (c.kin.y - s.y).abs(), (s.y + s.h - c.kin.y).abs()];
        let mut side = 0;
        for i in 1..4 {
            if distances[i] < distances[side] {
                side = i;
            }
        }
        let push = (c.kin.size * 2.8).max(60.0);
        let (x, y) = match side {
            0 => (s.x - push, c.kin.y),
            1 => (s.x + s.w + push, c.kin.y),
            2 => (c.kin.x, s.y - push),
            _ => (c.kin.x, s.y + s.h + push),
        };
        (clamp(x, c.margin, self.w - c.margin), clamp(y, c.margin, self.h - c.margin))
    }
    fn seek(&self, c: &mut Creature, dt: f64, rng: &mut PyRandom) {
        if self.surfaces.is_empty() || c.dragging || c.airborne || c.cage.is_some() {
            clear_desktop_hide_intent(c);
            return;
        }
        if c.desktop_fully_hidden || self.occluder(c).is_some() {
            return;
        }
        if !self.allows_hide(c) {
            clear_desktop_hide_intent(c);
            return;
        }
        if c.desktop_hide_surface_key.is_some() {
            c.desktop_hide_timer = (c.desktop_hide_timer - dt).max(0.0);
            if c.desktop_hide_timer <= 0.0 {
                clear_desktop_hide_intent(c);
            } else {
                return;
            }
        }
        c.desktop_seek_timer = (c.desktop_seek_timer - dt).max(0.0);
        if c.desktop_seek_timer > 0.0 {
            return;
        }
        c.desktop_seek_timer = personality_range(c, "desktop_hide_seek_interval", 34.0, 86.0, rng);
        if rng.random() > clamp(personality_float(c, "desktop_hide_chance", 0.055), 0.0, 1.0) {
            return;
        }
        let area = self.w * self.h * personality_float(c, "desktop_hide_max_area_ratio", 0.56);
        let fc = self.folders().len();
        let mut candidates = vec![];
        for (i, s) in self.surfaces.iter().enumerate() {
            if !self.can_occlude(s)
                || s.w * s.h > area
                || s.contains(c.kin.x, c.kin.y, 0.0)
                || !self.visible_at(i, s.x + s.w * 0.5, s.y + s.h * 0.5)
                || s.distance(c.kin.x, c.kin.y) >= personality_float(c, "desktop_hide_max_distance", 420.0)
            {
                continue;
            }
            let weight = if s.kind == "folder" {
                let fw = personality_float(c, "desktop_folder_weight", 0.34);
                if fc >= 2 && c.folder_portal_cooldown <= 0.0 {
                    fw
                } else if c.personality_flag("folder_hider", false) {
                    (fw * 0.35).max(0.18)
                } else {
                    continue;
                }
            } else if s.kind == "window" {
                personality_float(c, "desktop_window_weight", 1.0)
            } else {
                continue;
            };
            candidates.push((i, weight.max(0.01)));
        }
        if candidates.is_empty() {
            return;
        }
        let mut pick = rng.random() * candidates.iter().map(|(_, w)| w).sum::<f64>();
        let mut i = candidates.last().unwrap().0;
        for (candidate, weight) in candidates {
            pick -= weight;
            if pick <= 0.0 {
                i = candidate;
                break;
            }
        }
        let s = &self.surfaces[i];
        c.desktop_hide_surface_key = Some(s.key());
        c.desktop_hide_kind = Some(s.kind.clone());
        c.desktop_hide_timer = personality_range(c, "desktop_hide_duration", 7.0, 12.0, rng);
        (c.kin.target_x, c.kin.target_y) = self.point_inside(s, c, rng);
        c.kin.target_heading = (c.kin.target_y - c.kin.y).atan2(c.kin.target_x - c.kin.x);
        c.state = State::Wander;
        c.kin.motion_paused = false;
        c.kin.speed = 38.0 * c.speed_mult() * personality_float(c, "desktop_hide_speed_multiplier", 1.0);
        c.state_timer = personality_range(c, "desktop_hide_approach_time", 2.4, 4.8, rng);
    }
    fn teleport(&self, c: &mut Creature, source: usize, rng: &mut PyRandom) -> bool {
        let folders: Vec<_> =
            self.folders().into_iter().filter(|i| self.surfaces[*i] != self.surfaces[source]).collect();
        if folders.is_empty() {
            return false;
        }
        let dest = &self.surfaces[*rng.choice(&folders)];
        let side = *rng.choice(&[0, 1, 2, 3]);
        let margin = (c.kin.size * 1.1).max(24.0);
        let push = (c.kin.size * 3.2).max(70.0);
        let (inside, outside) = if side < 2 {
            let y = if dest.h > margin * 2.0 {
                rng.uniform(dest.y + margin, dest.y + margin.max(dest.h - margin))
            } else {
                dest.y + dest.h * 0.5
            };
            if side == 0 {
                ((dest.x + margin, y), (dest.x - push, y + rng.uniform(-35.0, 35.0)))
            } else {
                ((dest.x + dest.w - margin, y), (dest.x + dest.w + push, y + rng.uniform(-35.0, 35.0)))
            }
        } else {
            let x = if dest.w > margin * 2.0 {
                rng.uniform(dest.x + margin, dest.x + margin.max(dest.w - margin))
            } else {
                dest.x + dest.w * 0.5
            };
            if side == 2 {
                ((x, dest.y + margin), (x + rng.uniform(-45.0, 45.0), dest.y - push))
            } else {
                ((x, dest.y + dest.h - margin), (x + rng.uniform(-45.0, 45.0), dest.y + dest.h + push))
            }
        };
        let ix = clamp(inside.0, c.margin * 0.4, self.w - c.margin * 0.4);
        let iy = clamp(inside.1, c.margin * 0.4, self.h - c.margin * 0.4);
        let (dx, dy) = (ix - c.kin.x, iy - c.kin.y);
        c.kin.x = ix;
        c.kin.y = iy;
        c.kin.target_x = clamp(outside.0, c.margin, self.w - c.margin);
        c.kin.target_y = clamp(outside.1, c.margin, self.h - c.margin);
        c.kin.target_heading = (c.kin.target_y - iy).atan2(c.kin.target_x - ix);
        c.kin.heading = c.kin.target_heading;
        c.state = State::Wander;
        c.kin.motion_paused = false;
        c.kin.speed = 54.0 * c.speed_mult() * personality_float(c, "desktop_hide_speed_multiplier", 1.0);
        c.kin.current_speed = c.kin.current_speed.min(c.kin.speed);
        c.state_timer = personality_range(c, "folder_portal_exit_time", 1.5, 2.8, rng);
        c.kin.inertia_timer = 0.0;
        c.kin.inertia_vx = 0.0;
        c.kin.inertia_vy = 0.0;
        c.kin.translate_leg_world_points(dx, dy, 1.0);
        c.desktop_hide_surface_key = Some(dest.key());
        c.desktop_hide_kind = Some("folder".into());
        c.desktop_hide_timer = personality_range(c, "desktop_hide_duration", 6.0, 10.0, rng);
        c.desktop_visibility_alpha = 1.0;
        c.desktop_fully_hidden = true;
        c.desktop_hidden_timer = 0.0;
        c.folder_portal_dwell = 0.0;
        c.folder_portal_threshold = personality_range(c, "folder_portal_dwell", 0.85, 1.39, rng);
        c.folder_portal_cooldown = personality_range(c, "folder_portal_exit_cooldown", 38.0, 80.0, rng);
        true
    }
    fn portal(&self, c: &mut Creature, dt: f64, rng: &mut PyRandom) -> bool {
        c.folder_portal_cooldown = (c.folder_portal_cooldown - dt).max(0.0);
        if c.dragging || c.airborne || c.cage.is_some() {
            c.folder_portal_dwell = 0.0;
            return false;
        }
        if self.folders().len() < 2 || c.folder_portal_cooldown > 0.0 {
            c.folder_portal_dwell = (c.folder_portal_dwell - dt * 2.0).max(0.0);
            return false;
        }
        let source = self.containing(c, Some("folder"), (c.kin.size * 0.45).max(14.0));
        let Some(i) = source.filter(|i| self.visible_at(*i, c.kin.x, c.kin.y)) else {
            c.folder_portal_dwell = (c.folder_portal_dwell - dt * 2.0).max(0.0);
            return false;
        };
        if c.desktop_hide_surface_key.as_deref() != Some(self.surfaces[i].key().as_str())
            || c.desktop_hide_kind.as_deref() != Some("folder")
        {
            c.folder_portal_dwell = 0.0;
            return false;
        }
        c.folder_portal_dwell += dt;
        if c.folder_portal_dwell >= c.folder_portal_threshold { self.teleport(c, i, rng) } else { false }
    }
    fn awareness(&self, c: &mut Creature, dt: f64, rng: &mut PyRandom) {
        if self.surfaces.is_empty() {
            c.desktop_visibility_alpha = 1.0;
            c.desktop_fully_hidden = false;
            c.desktop_hidden_timer = 0.0;
            clear_desktop_hide_intent(c);
            return;
        }
        if c.desktop_spawn_grace > 0.0 {
            c.desktop_spawn_grace = (c.desktop_spawn_grace - dt).max(0.0);
            c.desktop_visibility_alpha = 1.0;
            c.desktop_fully_hidden = false;
            c.desktop_hidden_timer = 0.0;
            c.folder_portal_dwell = 0.0;
            return;
        }
        if self.portal(c, dt, rng) {
            return;
        }
        let occluder = self.occluder(c);
        let hidden = occluder.is_some_and(|i| self.fully_covered(c, i));
        c.desktop_visibility_alpha = 1.0;
        c.desktop_fully_hidden = hidden;
        c.desktop_occluding_keys = occluder.map(|i| self.surfaces[i].key()).into_iter().collect();
        if hidden && !c.dragging && c.cage.is_none() {
            c.social_target = None;
            c.desktop_hidden_timer += dt;
            if c.desktop_hidden_timer > personality_float(c, "desktop_hidden_linger", 1.8) && !c.airborne {
                if let Some(i) = occluder.or_else(|| self.containing(c, None, 0.0)) {
                    (c.kin.target_x, c.kin.target_y) = self.exit_target(c, &self.surfaces[i]);
                    c.kin.target_heading = (c.kin.target_y - c.kin.y).atan2(c.kin.target_x - c.kin.x);
                    c.state = State::Wander;
                    c.kin.motion_paused = false;
                    c.kin.speed = 48.0 * c.speed_mult();
                    c.state_timer = rng.uniform(1.0, 2.0);
                    c.desktop_hidden_timer = 0.0;
                }
            }
        } else {
            c.desktop_hidden_timer = 0.0;
        }
    }
}
impl CreatureManager {
    fn surface_context(&self) -> Context<'_> {
        Context {
            surfaces: &self.desktop_surfaces,
            w: self.screen_w,
            h: self.screen_h,
            mx: self.mouse_x,
            my: self.mouse_y,
            down: self.mouse_down,
            metrics: self.label_metrics.as_ref(),
        }
    }
    pub fn set_desktop_surfaces(&mut self, surfaces: Vec<DesktopSurface>) {
        self.desktop_surfaces = surfaces;
    }
    pub fn surface_can_occlude(&self, s: &DesktopSurface) -> bool {
        self.surface_context().can_occlude(s)
    }
    pub(crate) fn point_occluded(&self, c: &Creature, x: f64, y: f64) -> bool {
        let ctx = self.surface_context();
        let mut copy = c.clone();
        ctx.occluder(&mut copy).is_some_and(|i| ctx.visible_at(i, x, y))
    }
    pub(crate) fn maybe_seek_desktop_cover(&mut self, i: usize, dt: f64, rng: &mut PyRandom) {
        let ctx = Context {
            surfaces: &self.desktop_surfaces,
            w: self.screen_w,
            h: self.screen_h,
            mx: self.mouse_x,
            my: self.mouse_y,
            down: self.mouse_down,
            metrics: self.label_metrics.as_ref(),
        };
        ctx.seek(&mut self.creatures[i], dt, rng);
    }
    pub fn update_desktop_awareness(&mut self, i: usize, dt: f64, rng: &mut PyRandom) {
        let ctx = Context {
            surfaces: &self.desktop_surfaces,
            w: self.screen_w,
            h: self.screen_h,
            mx: self.mouse_x,
            my: self.mouse_y,
            down: self.mouse_down,
            metrics: self.label_metrics.as_ref(),
        };
        ctx.awareness(&mut self.creatures[i], dt, rng);
    }
    /// `creature_occlusion` for the creature at `i`, in place: Python's render
    /// asked `_occluding_surfaces_for_creature`, which can end a stale hide.
    pub fn occlusion_at(&mut self, i: usize) -> (bool, Vec<dc_render::Region>) {
        if self.desktop_surfaces.is_empty() {
            return (false, vec![]);
        }
        let ctx = Context {
            surfaces: &self.desktop_surfaces,
            w: self.screen_w,
            h: self.screen_h,
            mx: self.mouse_x,
            my: self.mouse_y,
            down: self.mouse_down,
            metrics: self.label_metrics.as_ref(),
        };
        let c = &mut self.creatures[i];
        let Some(s) = ctx.occluder(c) else {
            return (false, vec![]);
        };
        let higher = ctx.higher(s).map(DesktopSurface::rect).collect::<Vec<_>>();
        (
            ctx.fully_covered(c, s),
            vec![dc_paint::scene::surface_visible_region(self.desktop_surfaces[s].rect(), &higher)],
        )
    }

    pub fn creature_occlusion(&self, c: &mut Creature) -> (bool, Vec<dc_render::Region>) {
        let ctx = self.surface_context();
        let Some(i) = ctx.occluder(c) else {
            return (false, vec![]);
        };
        let higher = ctx.higher(i).map(DesktopSurface::rect).collect::<Vec<_>>();
        (
            ctx.fully_covered(c, i),
            vec![dc_paint::scene::surface_visible_region(self.desktop_surfaces[i].rect(), &higher)],
        )
    }
}
