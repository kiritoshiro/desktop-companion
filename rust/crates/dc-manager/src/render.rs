//! Port of `manager/core.py::render`: ordered inputs, without painting.
use crate::CreatureManager;
use dc_creature::RenderSnapshot;
/// Python's layers, including its final separate damage-number pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Cages,
    Bases,
    Webs,
    MouseWebs,
    Flies,
    Carcasses,
    Creatures,
    CarcassDamageNumbers,
    CreatureDamageNumbers,
}
pub const LAYER_ORDER: [Layer; 9] = [
    Layer::Cages,
    Layer::Bases,
    Layer::Webs,
    Layer::MouseWebs,
    Layer::Flies,
    Layer::Carcasses,
    Layer::Creatures,
    Layer::CarcassDamageNumbers,
    Layer::CreatureDamageNumbers,
];
pub struct CreatureFrame<'a> {
    pub id: u64,
    pub snapshot: RenderSnapshot,
    pub creature: &'a dc_creature::Creature,
    pub fully_covered: bool,
    pub hidden_regions: Vec<dc_render::Region>,
}
/// Equivalent inputs to `SceneDraw`; geometry remains available for the painter.
/// Iterate `layers` and `creatures` in order. Damage passes include hidden creatures.
pub struct FrameDraw<'a> {
    pub screen_w: f64,
    pub screen_h: f64,
    pub always_show_names: bool,
    pub layers: &'static [Layer],
    pub cages: &'a [dc_world::cage::Cage],
    pub cage_counts: Vec<usize>,
    pub active_cage: Option<usize>,
    pub bases: &'a dc_world::BaseWorld,
    pub webs: &'a dc_world::WebWorld,
    pub mouse_webs: &'a dc_world::MouseWebWorld,
    pub flies: &'a dc_world::FlyWorld,
    pub carcasses: &'a [dc_world::carcass::Carcass],
    pub creatures: Vec<CreatureFrame<'a>>,
}
impl CreatureManager {
    pub fn draw_output(&self) -> FrameDraw<'_> {
        let order = if self.render_order.is_empty() {
            self.creatures.iter().map(|c| c.id).collect()
        } else {
            self.render_order.clone()
        };
        let creatures = order
            .into_iter()
            .filter_map(|id| self.creature(id))
            .map(|c| {
                let (fully_covered, hidden_regions) = self.creature_occlusion(&mut c.clone());
                CreatureFrame { id: c.id, snapshot: c.render_snapshot(), creature: c, fully_covered, hidden_regions }
            })
            .collect();
        let mut counts = vec![0; self.cages.len()];
        for c in &self.creatures {
            if let Some(r) = c.cage {
                if let Some(count) = counts.get_mut(r.id as usize) {
                    *count += 1;
                }
            }
        }
        FrameDraw {
            screen_w: self.screen_w,
            screen_h: self.screen_h,
            always_show_names: self.always_show_names,
            layers: &LAYER_ORDER,
            cages: &self.cages,
            cage_counts: counts,
            active_cage: self.cage_drag.map(|d| d.index),
            bases: &self.base_world,
            webs: &self.web_world,
            mouse_webs: &self.mouse_web_world,
            flies: &self.fly_world,
            carcasses: &self.carcasses,
            creatures,
        }
    }
    pub fn carcass_dirty_rects(&self) -> Vec<(f64, f64, f64, f64)> {
        self.carcasses.iter().map(|c| c.footprint()).collect()
    }
}
