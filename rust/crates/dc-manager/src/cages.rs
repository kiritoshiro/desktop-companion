//! Port of `manager/cages.py` and the pointer dispatch in `manager/core.py`.
use crate::{CreatureManager, MouseInput};
use dc_creature::world::CageRect;
use dc_world::cage::{Cage, Corner};
#[derive(Clone, Copy, Debug)]
pub(crate) struct CageDrag {
    pub(crate) index: usize,
    corner: Option<Corner>,
    offset: (f64, f64),
}
fn rect(i: usize, c: &Cage) -> CageRect {
    CageRect { id: i as u64, x: c.x, y: c.y, w: c.w, h: c.h }
}
impl CreatureManager {
    pub fn add_cage(&mut self, cx: Option<f64>, cy: Option<f64>, w: f64, h: f64) -> String {
        let mut c =
            Cage::new(cx.unwrap_or(self.screen_w * 0.5) - w * 0.5, cy.unwrap_or(self.screen_h * 0.5) - h * 0.5, w, h);
        c.clamp_to_screen(self.screen_w, self.screen_h, 8.0);
        let i = self.cages.len();
        self.cages.push(c);
        let count = self.capture_inside(i);
        if count > 0 {
            format!("Cage added; {count} spider(s) now enclosed.")
        } else {
            "Cage added. Drag a spider inside to keep it there.".into()
        }
    }
    pub fn remove_cages(&mut self) -> String {
        if self.cages.is_empty() {
            return "There are no cages to remove.".into();
        }
        let count = self.cages.len();
        for c in &mut self.creatures {
            c.cage = None;
        }
        self.cages.clear();
        self.cage_drag = None;
        format!("Removed {count} cage(s); all spiders roam freely again.")
    }
    pub fn capture_inside(&mut self, i: usize) -> usize {
        let cage = &self.cages[i];
        let mut n = 0;
        for c in &mut self.creatures {
            if c.cage.is_none() && cage.contains_center(c.kin.x, c.kin.y) {
                c.cage = Some(rect(i, cage));
                n += 1;
            }
        }
        n
    }
    pub fn cage_handle_at(&self, x: f64, y: f64) -> Option<(usize, Option<Corner>)> {
        self.cages.iter().enumerate().rev().find_map(|(i, c)| {
            let corner = c.corner_at(x, y);
            if corner.is_some() || c.on_border(x, y) { Some((i, corner)) } else { None }
        })
    }
    pub fn cage_hover_kind(&self, x: f64, y: f64) -> Option<String> {
        self.cage_handle_at(x, y).map(|(_, c)| c.map_or_else(|| "move".into(), |c| format!("resize-{}", c.as_str())))
    }
    pub fn start_cage_drag(&mut self, x: f64, y: f64) -> bool {
        let Some((index, corner)) = self.cage_handle_at(x, y) else {
            return false;
        };
        let c = &self.cages[index];
        self.cage_drag =
            Some(CageDrag { index, corner, offset: if corner.is_some() { (0.0, 0.0) } else { (c.x - x, c.y - y) } });
        true
    }
    pub fn drag_cage(&mut self, x: f64, y: f64) {
        let Some(d) = self.cage_drag else {
            return;
        };
        let c = &mut self.cages[d.index];
        let (old_x, old_y) = (c.x, c.y);
        if let Some(corner) = d.corner {
            c.resize_corner(corner, x, y);
        } else {
            c.move_to(x + d.offset.0, y + d.offset.1);
        }
        c.clamp_to_screen(self.screen_w, self.screen_h, 8.0);
        let (dx, dy) = (c.x - old_x, c.y - old_y);
        for spider in &mut self.creatures {
            if spider.cage.is_none_or(|r| r.id != d.index as u64) {
                continue;
            }
            spider.cage = Some(rect(d.index, c));
            if spider.dragging {
                continue;
            }
            let k = &mut spider.kin;
            if d.corner.is_some() {
                let (nx, ny) = c.clamp_center(k.x, k.y, k.size * 0.85);
                k.translate_leg_world_points(nx - k.x, ny - k.y, 0.6);
                k.x = nx;
                k.y = ny;
                k.target_x = nx;
                k.target_y = ny;
            } else if dx != 0.0 || dy != 0.0 {
                k.x += dx;
                k.y += dy;
                k.target_x += dx;
                k.target_y += dy;
                k.translate_leg_world_points(dx, dy, 1.0);
            }
        }
    }
    pub(crate) fn sync_cages(&mut self) {
        for c in &mut self.creatures {
            c.cage = c.cage.and_then(|r| self.cages.get(r.id as usize).map(|c| rect(r.id as usize, c)));
        }
    }
    pub fn settle_creature_membership(&mut self, id: u64) {
        let cage = self.creature(id).and_then(|c| {
            self.cages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, r)| r.contains_center(c.kin.x, c.kin.y))
                .map(|(i, c)| rect(i, c))
        });
        if let Some(c) = self.creature_mut(id) {
            c.cage = cage;
        }
    }
    pub fn cage_dirty_rects(&self) -> Vec<(f64, f64, f64, f64)> {
        self.cages.iter().flat_map(Cage::border_rects).collect()
    }
    pub fn base_at(&self, x: f64, y: f64) -> Option<&dc_world::jobs::BaseSite> {
        self.pick_base(x, y, false)
    }
    pub fn base_grab_at(&self, x: f64, y: f64) -> Option<&dc_world::jobs::BaseSite> {
        if !self.interferable {
            return None;
        }
        self.pick_base(x, y, true)
    }
    fn pick_base(&self, x: f64, y: f64, grab: bool) -> Option<&dc_world::jobs::BaseSite> {
        let mut best = None;
        let mut bd = 0.0;
        for s in &self.base_world.bases {
            let reach =
                if grab { 22.0f64.max(s.radius() * (0.52 + 0.46 * s.completion())) } else { s.radius().max(26.0) };
            let d = (x - s.x).hypot(y - s.y);
            if d <= reach && (best.is_none() || d < bd) {
                best = Some(s);
                bd = d;
            }
        }
        best
    }
    pub fn remove_base(&mut self, id: &str) -> String {
        let Some(s) = self.base_world.base(id) else {
            return "There is no base there to remove.".into();
        };
        let team = s.team_id.clone();
        self.base_world.remove_base(id);
        self.save_or_warn();
        format!("Removed the {team} base.")
    }
    pub fn move_base(&mut self, id: &str, x: f64, y: f64) -> String {
        let Some(s) = self.base_world.base(id) else {
            return "There is no base there to move.".into();
        };
        let team = s.team_id.clone();
        self.base_world.move_base(id, x, y);
        self.save_or_warn();
        format!("Moved the {team} base.")
    }
    pub fn remove_bases(&mut self) -> String {
        let ids: Vec<_> = self.base_world.bases.iter().map(|s| s.id.clone()).collect();
        if ids.is_empty() {
            return "There are no bases to remove.".into();
        }
        for id in &ids {
            self.remove_base(id);
        }
        format!("Removed {} base(s).", ids.len())
    }
    pub fn start_base_drag(&mut self, x: f64, y: f64) -> bool {
        let Some(s) = self.base_grab_at(x, y) else {
            return false;
        };
        let offset = (s.x - x, s.y - y);
        self.dragged_base = Some(s.id.clone());
        self.base_drag_offset = offset;
        true
    }
    pub fn drag_base_to(&mut self, x: f64, y: f64) {
        if let Some(id) = &self.dragged_base {
            self.base_world.move_base(id, x + self.base_drag_offset.0, y + self.base_drag_offset.1);
        }
    }
    pub fn release_base_drag(&mut self) {
        if self.dragged_base.take().is_some() {
            self.base_drag_offset = (0.0, 0.0);
            self.save_or_warn();
        }
    }
    pub(crate) fn handle_input(&mut self, dt: f64, x: f64, y: f64, input: MouseInput) {
        if input.pressed {
            let hit = self.cage_handle_at(x, y);
            let mut handled = false;
            if hit.is_some_and(|(_, c)| c.is_some()) {
                handled = self.start_cage_drag(x, y);
            }
            if !handled && self.interferable {
                self.dragged_creature = self.creature_at(x, y);
                if let Some(c) = self.dragged_creature.and_then(|id| self.creature_mut(id)) {
                    c.start_drag(x, y);
                    handled = true;
                }
            }
            if !handled && self.interferable {
                if let Some(id) = self.fly_world.hit_fly_at(x, y) {
                    self.fly_world.start_drag_fly(id, x, y);
                    let hunters = self.fly_world.fly(id).unwrap().hunters.clone();
                    for c in &mut self.creatures {
                        if hunters.contains(&dc_world::CreatureId(c.id)) {
                            c.prey = None;
                            c.hunting_prey = false;
                        }
                    }
                    self.fly_world.fly_mut(id).unwrap().hunters.clear();
                    self.dragged_fly = Some(id);
                    handled = true;
                }
            }
            if !handled && self.interferable {
                if let Some(i) = self.fly_world.hit_spawner_at(x, y) {
                    self.fly_world.spawners[i].start_drag(x, y);
                    self.dragged_spawner = Some(i);
                    handled = true;
                }
            }
            if !handled && hit.is_some_and(|(_, c)| c.is_none()) {
                handled = self.start_cage_drag(x, y);
            }
            if !handled && self.interferable {
                self.start_base_drag(x, y);
            }
        }
        if input.down {
            if self.cage_drag.is_some() {
                self.drag_cage(x, y);
            } else if self.interferable && self.dragged_creature.is_some() {
                if let Some(c) = self.dragged_creature.and_then(|id| self.creature_mut(id)) {
                    c.drag_to(dt, x, y);
                }
            } else if let Some(id) = self.dragged_fly {
                self.fly_world.drag_fly_to(id, x, y);
            } else if let Some(i) = self.dragged_spawner {
                let s = &mut self.fly_world.spawners[i];
                s.drag_to(x, y, self.screen_w, self.screen_h);
                if !self.playfield.simple() {
                    (s.x, s.y) = self.playfield.clamp(s.x, s.y, s.radius);
                }
            } else if self.dragged_base.is_some() {
                self.drag_base_to(x, y);
            }
        }
        if input.released {
            self.cage_drag = None;
            if let Some(id) = self.dragged_fly.take() {
                self.fly_world.release_drag_fly(id, x, y);
            }
            if let Some(i) = self.dragged_spawner.take() {
                self.fly_world.spawners[i].release_drag();
            }
        }
        if input.released || !self.interferable {
            self.release_base_drag();
            if let Some(id) = self.dragged_creature.take() {
                if let Some(c) = self.creature_mut(id) {
                    c.release_drag(x, y);
                }
                self.settle_creature_membership(id);
            }
        }
    }
}
