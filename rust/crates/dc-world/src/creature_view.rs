//! The creature-facing interface of the world modules.
//!
//! The Python worlds (`webs.py`, `flies.py`, `jobs.py`, `aimed_silk.py`,
//! `carcass.py`) read and mutate creatures duck-typed through `getattr`.
//! In Rust that surface is the [`WorldCreature`] trait, which `dc-creature`
//! implements for its `Creature`. Each method names the Python attribute it
//! stands for. Worlds take creatures as `&[C]` / `&mut [C]` and keep only
//! [`CreatureId`]s across frames (Python kept object references).
//!
//! Defaults are given where a Python `getattr(c, name, default)` supplied one,
//! so test dummies only implement what the Python dummies had.

use dc_core::PyRandom;

/// Identity of a creature, standing in for Python's `is`. Must be unique among
/// live creatures and never reused while a world may still hold it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CreatureId(pub u64);

/// Identity of a fly inside one [`crate::flies::FlyWorld`] (assigned there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FlyId(pub u64);

/// Identity of a web inside one [`crate::webs::WebWorld`] (assigned there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WebId(pub u64);

/// `Creature.relation_to(other)`: `"friend"`, `"foe"` or `"neutral"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Friend,
    Foe,
    Neutral,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Friend => "friend",
            Relation::Foe => "foe",
            Relation::Neutral => "neutral",
        }
    }
    pub fn parse(s: &str) -> Relation {
        match s {
            "friend" => Relation::Friend,
            "foe" => Relation::Foe,
            _ => Relation::Neutral,
        }
    }
}

/// `creature.job_mode`, as written by `BaseWorld._set_intent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobMode {
    Idle,
    Build,
    BuildTravel,
    GuardAlert,
    Patrol,
    ScoutTravel,
    ScoutReport,
    WebTravel,
    WebRepair,
    WebWeave,
    Hunting,
    HuntReturn,
    HuntPatrol,
}

impl JobMode {
    pub const ALL: [JobMode; 13] = [
        JobMode::Idle,
        JobMode::Build,
        JobMode::BuildTravel,
        JobMode::GuardAlert,
        JobMode::Patrol,
        JobMode::ScoutTravel,
        JobMode::ScoutReport,
        JobMode::WebTravel,
        JobMode::WebRepair,
        JobMode::WebWeave,
        JobMode::Hunting,
        JobMode::HuntReturn,
        JobMode::HuntPatrol,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            JobMode::Idle => "idle",
            JobMode::Build => "build",
            JobMode::BuildTravel => "build_travel",
            JobMode::GuardAlert => "guard_alert",
            JobMode::Patrol => "patrol",
            JobMode::ScoutTravel => "scout_travel",
            JobMode::ScoutReport => "scout_report",
            JobMode::WebTravel => "web_travel",
            JobMode::WebRepair => "web_repair",
            JobMode::WebWeave => "web_weave",
            JobMode::Hunting => "hunting",
            JobMode::HuntReturn => "hunt_return",
            JobMode::HuntPatrol => "hunt_patrol",
        }
    }

    pub fn parse(s: &str) -> Option<JobMode> {
        JobMode::ALL.iter().copied().find(|m| m.as_str() == s)
    }
}

/// Everything `BaseWorld._set_intent(creature, mode, target, alert_target,
/// base_id, facing)` writes, in one call. `facing: None` means "face the way
/// you walk".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JobIntent<'a> {
    pub mode: JobMode,
    pub target: Option<(f64, f64)>,
    pub alert_target: Option<CreatureId>,
    pub base_id: Option<&'a str>,
    pub facing: Option<f64>,
}

impl JobIntent<'_> {
    pub const IDLE: JobIntent<'static> =
        JobIntent { mode: JobMode::Idle, target: None, alert_target: None, base_id: None, facing: None };
}

/// What the worlds need from a creature. Implemented by `dc-creature`.
pub trait WorldCreature {
    // -- identity -----------------------------------------------------------
    /// Object identity (`is`).
    fn id(&self) -> CreatureId;
    /// `creature.index`.
    fn index(&self) -> i64;
    /// `str(getattr(creature, "progression_id", creature.index))`.
    fn progression_id(&self) -> &str;
    /// `str(getattr(creature.progression, "team_id", "neutral") or "neutral")`:
    /// never empty, `"neutral"` for no team.
    fn team_id(&self) -> &str;
    /// `getattr(creature, "job_id", "none")`.
    fn job_id(&self) -> &str;

    // -- pose ---------------------------------------------------------------
    fn x(&self) -> f64;
    fn y(&self) -> f64;
    fn heading(&self) -> f64;
    /// `getattr(creature, "size", 24.0)`.
    fn size(&self) -> f64;
    /// `float(getattr(creature, "level", 1))`.
    fn level(&self) -> f64 {
        1.0
    }

    // -- flags --------------------------------------------------------------
    fn dead(&self) -> bool {
        false
    }
    fn dragging(&self) -> bool {
        false
    }
    fn airborne(&self) -> bool {
        false
    }
    /// `job_busy`: a personality state outranks the job this frame.
    fn job_busy(&self) -> bool {
        false
    }
    /// `getattr(creature, "player_control", None) is not None`.
    fn player_controlled(&self) -> bool {
        false
    }
    /// `creature.webbed`: currently pinned by silk.
    fn webbed(&self) -> bool {
        false
    }
    /// `str(creature.state) == "Feed"`.
    fn is_feeding(&self) -> bool {
        false
    }
    /// `creature._hunting_prey`.
    fn hunting_prey(&self) -> bool {
        false
    }

    // -- what a fly reads ---------------------------------------------------
    /// `creature._acts_as_hunter()`.
    fn acts_as_hunter(&self) -> bool {
        false
    }
    /// `creature._prey`, when it is a fly (`_prey is fly`).
    fn prey_fly(&self) -> Option<FlyId> {
        None
    }
    /// `creature._desktop_fully_hidden`.
    fn desktop_fully_hidden(&self) -> bool {
        false
    }
    /// `creature.current_speed`.
    fn current_speed(&self) -> f64 {
        0.0
    }
    /// `creature.aim_intent`.
    fn aim_intent(&self) -> f64 {
        0.0
    }
    /// `creature._progression_effect("web_homing")` (0 if absent).
    fn web_homing(&self) -> f64 {
        0.0
    }

    // -- relations ----------------------------------------------------------
    /// `self.relation_to(other)`.
    fn relation_to(&self, other: &Self) -> Relation;

    // -- what the worlds write ----------------------------------------------
    /// `BaseWorld._set_intent`: sets `job_mode`, `job_target`,
    /// `job_alert_target`, `job_base_id` and `job_facing` together.
    fn set_job_intent(&mut self, intent: &JobIntent<'_>);
    /// `creature.job_base_id = ...` on its own.
    fn set_job_base_id(&mut self, base_id: Option<&str>);
    /// `creature.heal(amount)`; the default is a creature without `heal`.
    fn heal(&mut self, _amount: f64) -> f64 {
        0.0
    }
    /// `creature.web_pinned(kind, by)`: hit by trapping silk.
    fn web_pinned(&mut self, _kind: &str, _by: CreatureId) {}
    /// `creature.rng`: the creature's own stream (Webber's `claim_site`).
    fn rng(&mut self) -> &mut PyRandom;
}

/// Index of the creature with `id` in `creatures`.
pub fn find_index<C: WorldCreature>(creatures: &[C], id: CreatureId) -> Option<usize> {
    creatures.iter().position(|c| c.id() == id)
}
