//! What a creature sees of, and does to, the world outside itself.
//!
//! In Python a creature held direct references: `self.neighbors` (other
//! `Creature`s), `self.social_target`, `self._foe`, `self._prey` (a fly),
//! `self.web_world`, `self.fly_world`, `self.mouse_web_world`,
//! `self.base_world`, `self.cage` and `self.playfield`, and it read and wrote
//! through them during `update`. The port keeps no references. Instead:
//!
//! * other creatures are [`CreatureId`]s, resolved each frame through a
//!   [`Population`] of [`CreatureView`] snapshots the manager maintains;
//! * flies and webs are opaque ids resolved through the world traits below;
//! * the two places where Python wrote to *another* creature (`_update_play`
//!   tagging its playmate) push a [`Command`] the manager applies right after
//!   this creature's update, which is when Python's write became visible to
//!   anyone else.
//!
//! The traits here are the narrow creature-facing surface of
//! `world/webs.py`, `flies.py`, `mouse_webs.py` and `jobs.py`. They are
//! implemented for `dc-world`'s `WebWorld`, `FlyWorld`, `MouseWebWorld` and
//! `BaseWorld` in [`crate::world_impls`], so a manager passes its real worlds;
//! tests can pass small fakes. The ids here are the raw `u64` inside
//! `dc_world::{CreatureId, WebId, FlyId}`.

use crate::mood::MoodBump;
use crate::state::State;
use std::collections::HashMap;
use std::sync::Arc;

/// Stable identity of a creature for the lifetime of a scene (the manager
/// assigns it; Python used object identity).
pub type CreatureId = u64;
/// Opaque handle of a web in the web world.
pub type WebId = u64;
/// Opaque handle of a fly in the fly world.
pub type FlyId = u64;

/// What one creature can read about another (the attributes Python's
/// behaviour code reads off `other`, `mate`, `target`, `foe` and `threat`).
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureView {
    pub id: CreatureId,
    pub index: i64,
    pub progression_id: Arc<str>,
    /// `other.progression.team_id`
    pub team_id: Arc<str>,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub state: State,
    pub dead: bool,
    pub dragging: bool,
    pub airborne: bool,
    pub webbed: bool,
    pub desktop_fully_hidden: bool,
}

/// Every creature's current [`CreatureView`], by id.
///
/// The manager rebuilds or refreshes it as it goes. To match Python's
/// sequential update loop exactly, refresh a creature's entry (with
/// [`Population::set`]) right after updating it, so creatures updated later
/// in the same frame see where it moved to.
#[derive(Clone, Debug, Default)]
pub struct Population {
    views: Vec<CreatureView>,
    slots: HashMap<CreatureId, usize>,
}

impl Population {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_views(views: impl IntoIterator<Item = CreatureView>) -> Self {
        let mut p = Self::new();
        for v in views {
            p.set(v);
        }
        p
    }

    #[inline]
    pub fn get(&self, id: CreatureId) -> Option<&CreatureView> {
        self.slots.get(&id).map(|&i| &self.views[i])
    }

    /// Insert or replace one creature's view.
    pub fn set(&mut self, view: CreatureView) {
        match self.slots.get(&view.id) {
            Some(&i) => self.views[i] = view,
            None => {
                self.slots.insert(view.id, self.views.len());
                self.views.push(view);
            }
        }
    }

    pub fn remove(&mut self, id: CreatureId) {
        if let Some(i) = self.slots.remove(&id) {
            self.views.swap_remove(i);
            if i < self.views.len() {
                let moved = self.views[i].id;
                self.slots.insert(moved, i);
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CreatureView> {
        self.views.iter()
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

/// What a creature reads off a fly (`prey.x`, `.alive`, `.trapped`, ...).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreyView {
    pub id: FlyId,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub alive: bool,
    pub eaten: bool,
    pub trapped: bool,
    pub dragging: bool,
    pub is_moving: bool,
}

/// `"trap"` or `"wall"` silk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ShotKind {
    #[default]
    Trap,
    Wall,
}

impl ShotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ShotKind::Trap => "trap",
            ShotKind::Wall => "wall",
        }
    }
}

/// What a silk projectile needs from the spider that threw it
/// (`WebShotProjectile` reads `shooter.x/.y/.heading/.size`, and the hit
/// callback passes the shooter on as `by`). `homing` is
/// `shooter._progression_effect("web_homing")`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shooter {
    pub id: CreatureId,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub size: f64,
    pub homing: f64,
}

/// The creature-facing half of `world/webs.py::WebWorld` (and `Web`).
pub trait WebWorldApi {
    // -- perception (Perception.repairable_web & co) --------------------
    fn find_repairable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId>;
    fn find_adoptable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId>;
    fn find_walkable_web(&self, x: f64, y: f64, max_dist: f64) -> Option<WebId>;
    fn intact_complete_count(&self) -> usize;
    // -- claiming --------------------------------------------------------
    /// `claim_site(creature, prefer_corner=...)`; `size` is the creature's.
    fn claim_site(&mut self, builder: CreatureId, size: f64, prefer_corner: bool) -> Option<WebId>;
    fn adopt(&mut self, web: WebId, builder: CreatureId) -> bool;
    fn abandon(&mut self, web: WebId);
    fn claim_repair(&mut self, web: WebId, repairer: CreatureId) -> bool;
    fn release_repair(&mut self, web: WebId);
    // -- one web ---------------------------------------------------------
    /// `web in web_world.webs`
    fn contains(&self, web: WebId) -> bool;
    fn is_complete(&self, web: WebId) -> bool;
    fn is_damaged(&self, web: WebId) -> bool;
    fn damaged_centroid(&self, web: WebId) -> Option<(f64, f64)>;
    fn reach_inset(&self, web: WebId) -> f64;
    fn hub(&self, web: WebId) -> (f64, f64);
    /// `web.working_point(screen_w=, screen_h=)`: (point, drawing).
    fn working_point(&self, web: WebId, screen_w: f64, screen_h: f64) -> ((f64, f64), bool);
    fn advance(&mut self, web: WebId, dt: f64, weave_speed: f64, spider: (f64, f64), lead_max: f64);
    fn repair_near(&mut self, web: WebId, x: f64, y: f64, dt: f64);
    /// `web.walkable_point(sw, sh)` (draws from the web's own stream).
    fn walkable_point(&mut self, web: WebId, screen_w: f64, screen_h: f64) -> (f64, f64);
    fn pluck(&mut self, web: WebId, at: (f64, f64), strength: f64);
}

/// The creature-facing half of `world/flies.py::FlyWorld`.
pub trait FlyWorldApi {
    /// The live state of a fly, `None` once it no longer exists.
    fn fly(&self, id: FlyId) -> Option<PreyView>;
    fn launch_web_shot(&mut self, shooter: &Shooter, fly: FlyId, kind: ShotKind) -> bool;
    fn launch_web_shot_at_creature(&mut self, shooter: &Shooter, target: &CreatureView, kind: ShotKind) -> bool;
}

/// The creature-facing half of `world/mouse_webs.py::MouseWebWorld`.
pub trait MouseWebApi {
    fn enabled(&self) -> bool;
    fn busy(&self) -> bool;
    fn shoot(&mut self, origin: (f64, f64), target: (f64, f64), kind: ShotKind, homing: f64, lead: (f64, f64)) -> bool;
}

/// `base_world.bases.get(f"team:{team}")` -> `(site.x, site.y)`.
pub trait BaseLookup {
    fn base_point(&self, key: &str) -> Option<(f64, f64)>;
}

/// A cage's rectangle (`world/cage.py::Cage`), copied onto the creature by
/// the manager whenever the cage moves or is resized.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CageRect {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl CageRect {
    /// `Cage.interior(inset)`: (left, top, right, bottom).
    pub fn interior(&self, inset: f64) -> (f64, f64, f64, f64) {
        let mut left = self.x + inset;
        let mut right = self.x + self.w - inset;
        let mut top = self.y + inset;
        let mut bottom = self.y + self.h - inset;
        if right < left {
            let cx = self.x + self.w * 0.5;
            left = cx;
            right = cx;
        }
        if bottom < top {
            let cy = self.y + self.h * 0.5;
            top = cy;
            bottom = cy;
        }
        (left, top, right, bottom)
    }

    /// `Cage.clamp_center(px, py, inset)`
    pub fn clamp_center(&self, px: f64, py: f64, inset: f64) -> (f64, f64) {
        let (l, t, r, b) = self.interior(inset);
        (l.max(r.min(px)), t.max(b.min(py)))
    }
}

/// A play role (`social_role`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SocialRole {
    #[default]
    None,
    Chase,
    Flee,
    Play,
}

impl SocialRole {
    pub fn as_str(self) -> &'static str {
        match self {
            SocialRole::None => "none",
            SocialRole::Chase => "chase",
            SocialRole::Flee => "flee",
            SocialRole::Play => "play",
        }
    }
}

/// A write to another creature, applied by the manager with
/// [`crate::Creature::apply_command`] right after the issuing creature's
/// update (and before the next creature updates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// `target.mood.bump(...)`
    BumpMood { target: CreatureId, bump: MoodBump },
    /// `if target.state != "Play": target.enter_play(partner, role=role)`
    EnterPlay { target: CreatureId, partner: CreatureId, role: SocialRole },
}

/// Everything outside the creature that one `update` may read or change.
/// `None` stands for Python's `self.web_world is None` and friends.
pub struct Env<'a> {
    pub population: &'a Population,
    pub webs: Option<&'a mut dyn WebWorldApi>,
    pub flies: Option<&'a mut dyn FlyWorldApi>,
    pub mouse_webs: Option<&'a mut dyn MouseWebApi>,
    pub bases: Option<&'a dyn BaseLookup>,
    pub commands: &'a mut Vec<Command>,
}

impl<'a> Env<'a> {
    /// No worlds at all: a creature constructed on its own, as most Python
    /// tests do.
    pub fn bare(population: &'a Population, commands: &'a mut Vec<Command>) -> Self {
        Env { population, webs: None, flies: None, mouse_webs: None, bases: None, commands }
    }

    #[inline]
    pub fn creature(&self, id: CreatureId) -> Option<&'a CreatureView> {
        self.population.get(id)
    }

    /// The web world, re-borrowed for one call.
    pub fn webs_mut(&mut self) -> Option<&mut dyn WebWorldApi> {
        match &mut self.webs {
            Some(w) => Some(&mut **w),
            None => None,
        }
    }

    /// The live view of a fly through the fly world.
    pub fn fly(&self, id: Option<FlyId>) -> Option<PreyView> {
        let id = id?;
        self.flies.as_ref().and_then(|f| f.fly(id))
    }
}
