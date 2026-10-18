//! Port of `creature/perception.py`: a creature's per-tick, read-only view of
//! the shared web world.
//!
//! Python captured references (`web_world`, `fly_world`, the creature) and
//! ran each query lazily; here [`Perception`] borrows the web world out of
//! the frame's [`Env`] and copies the creature's position, which is all the
//! queries read.

use crate::creature::Creature;
use crate::world::{Env, PreyView, WebId, WebWorldApi};

/// `Perception`
pub struct Perception<'a> {
    webs: Option<&'a dyn WebWorldApi>,
    /// `fly_world is not None`
    has_fly_world: bool,
    x: f64,
    y: f64,
    prey: Option<PreyView>,
    hunting_prey: bool,
    /// `has_web_world`
    pub has_web_world: bool,
}

impl<'a> Perception<'a> {
    /// `build_perception(creature)`
    pub fn new(c: &Creature, env: &'a Env<'_>) -> Self {
        let webs: Option<&'a dyn WebWorldApi> = match &env.webs {
            Some(w) => Some(&**w),
            None => None,
        };
        Perception {
            has_web_world: webs.is_some(),
            webs,
            has_fly_world: env.flies.is_some(),
            x: c.kin.x,
            y: c.kin.y,
            prey: c.prey,
            hunting_prey: c.hunting_prey,
        }
    }

    /// The nearest torn, finished web worth mending.
    pub fn repairable_web(&self, max_dist: f64) -> Option<WebId> {
        self.webs?.find_repairable_web(self.x, self.y, max_dist)
    }

    /// The nearest abandoned, unfinished web worth taking over.
    pub fn adoptable_web(&self, max_dist: f64) -> Option<WebId> {
        self.webs?.find_adoptable_web(self.x, self.y, max_dist)
    }

    /// The nearest finished web worth walking onto.
    pub fn walkable_web(&self, max_dist: f64) -> Option<WebId> {
        self.webs?.find_walkable_web(self.x, self.y, max_dist)
    }

    /// How many complete, undamaged webs exist.
    pub fn intact_web_count(&self) -> usize {
        self.webs.map_or(0, |w| w.intact_complete_count())
    }

    /// Whether a fly world exists to fire a trapping shot through.
    pub fn can_shoot_prey_web(&self) -> bool {
        self.has_fly_world
    }

    /// The fly this creature is locked onto.
    pub fn prey(&self) -> Option<PreyView> {
        self.prey
    }

    pub fn is_hunting_prey(&self) -> bool {
        self.hunting_prey
    }
}
