//! Port of `world/jobs.py`: data-driven jobs, colony bases and their economy.
//!
//! The job table itself lives in `dc-content::jobs` (content validates preset
//! slots against it) and is re-exported here. [`BaseSite`] is one persistent
//! colony base; [`BaseWorld`] founds bases, runs the Builder, Guard, Scout,
//! Web tender and Hunter shifts, banks food, and heals creatures near a
//! levelled base.
//!
//! Randomness: Python's `BaseWorld` shares the manager's `_rng`, so
//! [`BaseWorld::update`] takes it as `&mut PyRandom`. The mound layout and dirt
//! patch recipes are seeded from strings (`f"{site_id}:mounds"`, the patch
//! key), exactly as in Python, so a base piles its earth identically in every
//! process.
//!
//! Python's `bases` dict keeps insertion order and ties in "nearest base"
//! resolve to the first; `bases` here is a `Vec<BaseSite>` in the same order.
//!
//! Drawing (`BaseWorld.render`) is out of scope. Painter state:
//! `BaseSite::{x, y, radius(), completion(), alert, team_id}`,
//! [`BaseSite::mounds`], [`patch_recipe`] (keys `"{id}:field"`,
//! `"{id}:grit"`, `"{id}:patch:{dx:.2}:{dy:.2}"`), [`blend`] and the `TEAM_TINT_*`
//! / `PATCH_*` constants; team colours come from `dc-state::teams`.

use std::cell::RefCell;
use std::collections::HashMap;

use dc_core::PyRandom;
use dc_core::math::{py_floordiv, py_mod, py_round};
use serde_json::{Map, Value};

use crate::creature_view::{CreatureId, JobIntent, JobMode, Relation, WebId, WorldCreature};
use crate::playfield::Playfield;
use crate::pymath::{self, hypot, py_pow, py_sum, round_n};
use crate::spatial::{GRID_MIN_ITEMS, SpatialGrid};
use crate::webs::{self, WebWorld};

pub use dc_content::jobs::{JOB_DEFINITIONS, JOB_IDS, JobDefinition, job_by_id, normalize_job_id};

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;

/// `job_definition(value)`.
pub fn job_definition(value: Option<&str>) -> &'static JobDefinition {
    job_by_id(normalize_job_id(value)).expect("normalized ids exist")
}

/// `job_ability_ids(value)`.
pub fn job_ability_ids(value: Option<&str>) -> &'static [&'static str] {
    job_definition(value).ability_ids
}

/// `JOB_OPTIONS`: `(display name, id)` pairs.
pub fn job_options() -> [(&'static str, &'static str); 6] {
    let mut out = [("", ""); 6];
    for (slot, job) in out.iter_mut().zip(JOB_DEFINITIONS.iter()) {
        *slot = (job.display_name, job.id);
    }
    out
}

pub const MAX_BUILD_PROGRESS: f64 = 120.0;
pub const BUILD_DUTY_ON: (f64, f64) = (7.0, 12.0);
pub const BUILD_DUTY_OFF: (f64, f64) = (4.0, 8.0);
pub const PATROL_DUTY_ON: (f64, f64) = (9.0, 16.0);
pub const PATROL_DUTY_OFF: (f64, f64) = (5.0, 9.0);
pub const SCOUT_DUTY_ON: (f64, f64) = (9.0, 14.0);
pub const SCOUT_DUTY_OFF: (f64, f64) = (3.0, 5.0);
pub const WEBBER_DUTY_ON: (f64, f64) = (10.0, 15.0);
pub const WEBBER_DUTY_OFF: (f64, f64) = (3.0, 5.0);
pub const HUNTER_DUTY_ON: (f64, f64) = (8.0, 14.0);
pub const HUNTER_DUTY_OFF: (f64, f64) = (4.0, 7.0);
pub const SCOUT_SECTOR_COLS: usize = 4;
pub const SCOUT_SECTOR_ROWS: usize = 3;
pub const SCOUT_ARRIVE_DIST: f64 = 42.0;
pub const SCOUT_REPORT_DWELL: (f64, f64) = (1.0, 2.0);
pub const WEBBER_ARRIVE_DIST: f64 = 26.0;
pub const WEBBER_NEAR_BASE_PAD: f64 = 260.0;
pub const WEBBER_TARGET_WEB_COUNT: usize = 2;
pub const WEBBER_WEAVE_SPEED: f64 = 42.0;
pub const HUNTER_PATROL_RADIUS_PAD: f64 = 70.0;
pub const HUNTER_RANGE_RADIUS: f64 = 430.0;
pub const HUNTER_RANGE_BREATHE: f64 = 0.24;
pub const HUNTER_FOE_SCAN_PAD: f64 = 240.0;
pub const HUNTER_CARRY_FOOD_AMOUNT: f64 = 12.0;
pub const FLY_CATCH_RESOURCE_AMOUNT: f64 = 4.0;
pub const REPAIR_URGENT_INTEGRITY: f64 = 0.5;
pub const BUILD_RESOURCE_COST_PER_PROGRESS: f64 = 1.0;
pub const GROWTH_FOOD_SHARE: f64 = 0.30;
pub const BASE_REGEN_RADIUS_PAD: f64 = 90.0;
pub const BASE_REGEN_HP_PER_LEVEL: f64 = 1.4;
pub const GUARD_ALERT_RADIUS_PAD: f64 = 70.0;
pub const GUARD_ALERT_RADIUS_PER_LEVEL: f64 = 15.0;
pub const GUARD_STANDOFF_PAD: f64 = 46.0;
pub const GUARD_LINE_ARRIVE: f64 = 14.0;
pub const GUARD_LINE_DWELL: (f64, f64) = (2.5, 4.5);
pub const GUARD_POST_DRIFT: f64 = 0.012;
pub const GUARD_FACE_OUT_DIST: f64 = 34.0;
pub const MOUNDS_PER_BASE: usize = 12;
pub const PATCH_POINTS: usize = 15;
pub const PATCH_FLATTEN: f64 = 0.58;
pub const TEAM_TINT_SOIL: f64 = 0.05;
pub const TEAM_TINT_BODY: f64 = 0.08;
pub const TEAM_TINT_CROWN: f64 = 0.13;

// ======================================================================
// Mounds and dirt patches
// ======================================================================

/// `_plan_mounds`: every mound of a finished base, centre outwards, as
/// offsets `(dx, dy, size, aspect)` in build order.
pub fn plan_mounds(site_id: &str, radius: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut rng = PyRandom::from_str_seed(&format!("{site_id}:mounds"));
    let mut placed = Vec::with_capacity(MOUNDS_PER_BASE);
    for index in 0..MOUNDS_PER_BASE {
        let ring = py_pow(index as f64 / 1.max(MOUNDS_PER_BASE - 1) as f64, 0.62);
        let distance = radius * 0.62 * ring;
        let angle = index as f64 * 2.39996 + rng.uniform(-0.28, 0.28);
        let jitter = radius * 0.05;
        let x = angle.cos() * distance + rng.uniform(-jitter, jitter);
        let y = angle.sin() * distance * 0.66 + rng.uniform(-jitter, jitter) * 0.66;
        let size = radius * rng.uniform(0.15, 0.21);
        let aspect = rng.uniform(0.85, 1.35);
        placed.push((x, y, size, aspect));
    }
    placed
}

fn ragged_outline(rng: &mut PyRandom, points: usize, squash: f64, wobble: f64) -> Vec<(f64, f64)> {
    (0..points)
        .map(|index| {
            let angle = (index as f64 / points as f64) * TAU;
            let radius = 1.0 + rng.uniform(-wobble, wobble * 0.78);
            (angle.cos() * radius, angle.sin() * radius * squash)
        })
        .collect()
}

/// The fixed shape of one dirt patch, in unit coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchRecipe {
    pub body: Vec<(f64, f64)>,
    pub crown: Vec<(f64, f64)>,
    pub crown_offset: (f64, f64),
    /// `(x, y, size, threshold)` per loose clod.
    pub clods: Vec<(f64, f64, f64, f64)>,
}

/// `patch_recipe(key)` (uncached; see [`PatchCache`]).
pub fn patch_recipe(key: &str) -> PatchRecipe {
    let mut rng = PyRandom::from_str_seed(key);
    let clods = (0..6)
        .map(|_| {
            let angle = rng.uniform(0.0, TAU);
            let distance = rng.uniform(0.55, 1.20);
            let x = angle.cos() * distance;
            let y = angle.sin() * distance * PATCH_FLATTEN;
            (x, y, rng.uniform(0.06, 0.15), rng.uniform(0.0, 0.95))
        })
        .collect();
    let body = ragged_outline(&mut rng, PATCH_POINTS, PATCH_FLATTEN, 0.24);
    let crown = ragged_outline(&mut rng, PATCH_POINTS - 5, PATCH_FLATTEN * 1.12, 0.30);
    let crown_offset = (rng.uniform(-0.18, 0.04), rng.uniform(-0.28, -0.14));
    PatchRecipe { body, crown, crown_offset, clods }
}

/// `_PATCH_CACHE`, owned by whoever paints.
#[derive(Debug, Default)]
pub struct PatchCache(HashMap<String, PatchRecipe>);

impl PatchCache {
    pub fn get(&mut self, key: &str) -> &PatchRecipe {
        if !self.0.contains_key(key) {
            self.0.insert(key.to_string(), patch_recipe(key));
        }
        &self.0[key]
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// `_blend`: mix one RGB triple towards another.
pub fn blend(base: (u8, u8, u8), towards: (u8, u8, u8), amount: f64) -> (u8, u8, u8) {
    let ch = |c: u8, t: u8| py_round(c as f64 + (t as f64 - c as f64) * amount).clamp(0.0, 255.0) as u8;
    (ch(base.0, towards.0), ch(base.1, towards.1), ch(base.2, towards.2))
}

// ======================================================================
// Duty and base sites
// ======================================================================

/// Whether one worker is on shift, and for how much longer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DutyCycle {
    pub on_duty: bool,
    pub remaining: f64,
}

/// One mound as drawn: `(x, y, size, aspect, built)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mound {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub aspect: f64,
    /// 0..1: finished mounds are 1, at most one is part-way.
    pub built: f64,
}

/// The cached mound plan: `(site id, rounded radius, offsets)`.
type MoundLayout = (String, f64, Vec<(f64, f64, f64, f64)>);

/// Persistent colony base state; geometry derives from its level.
#[derive(Clone, Debug)]
pub struct BaseSite {
    pub id: String,
    pub owner_id: String,
    pub team_id: String,
    pub x: f64,
    pub y: f64,
    pub build_progress: f64,
    pub level: i64,
    pub integrity: f64,
    pub max_integrity: f64,
    pub alert: f64,
    pub last_alert: f64,
    pub patrol_angle: f64,
    pub resources: f64,
    /// Food set aside for raising spiders, which building may not spend.
    pub larder: f64,
    /// The team blackboard: plain dicts, at most 8.
    pub points_of_interest: Vec<Map<String, Value>>,
    mound_layout: RefCell<Option<MoundLayout>>,
}

impl PartialEq for BaseSite {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
            && self.owner_id == o.owner_id
            && self.team_id == o.team_id
            && self.x == o.x
            && self.y == o.y
            && self.build_progress == o.build_progress
            && self.level == o.level
            && self.integrity == o.integrity
            && self.max_integrity == o.max_integrity
            && self.alert == o.alert
            && self.last_alert == o.last_alert
            && self.patrol_angle == o.patrol_angle
            && self.resources == o.resources
            && self.larder == o.larder
            && self.points_of_interest == o.points_of_interest
    }
}

impl BaseSite {
    pub fn new(id: &str, owner_id: &str, team_id: &str, x: f64, y: f64) -> BaseSite {
        BaseSite {
            id: id.to_string(),
            owner_id: owner_id.to_string(),
            team_id: team_id.to_string(),
            x,
            y,
            build_progress: 0.0,
            level: 0,
            integrity: 100.0,
            max_integrity: 100.0,
            alert: 0.0,
            last_alert: 0.0,
            patrol_angle: 0.0,
            resources: 0.0,
            larder: 0.0,
            points_of_interest: Vec::new(),
            mound_layout: RefCell::new(None),
        }
    }

    pub fn radius(&self) -> f64 {
        30.0 + self.level as f64 * 10.0
    }

    /// Fraction of the whole base that is built.
    pub fn completion(&self) -> f64 {
        0.0f64.max(1.0f64.min(self.build_progress / MAX_BUILD_PROGRESS))
    }

    /// Where the mounds sit, in build order, only those begun.
    pub fn mounds(&self) -> Vec<Mound> {
        let key = round_n(self.radius(), 3);
        let mut cache = self.mound_layout.borrow_mut();
        if !cache.as_ref().is_some_and(|c| c.0 == self.id && c.1 == key) {
            *cache = Some((self.id.clone(), key, plan_mounds(&self.id, self.radius())));
        }
        let filled = self.completion() * MOUNDS_PER_BASE as f64;
        let layout = &cache.as_ref().unwrap().2;
        layout
            .iter()
            .enumerate()
            .filter_map(|(index, &(mx, my, size, aspect))| {
                let built = 0.0f64.max(1.0f64.min(filled - index as f64));
                (built > 0.0).then_some(Mound { x: self.x + mx, y: self.y + my, size, aspect, built })
            })
            .collect()
    }

    /// `BaseSite.to_dict` (the on-disk `bases[*]` entry).
    pub fn to_dict(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), Value::from(self.id.clone()));
        m.insert("owner_id".into(), Value::from(self.owner_id.clone()));
        m.insert("team_id".into(), Value::from(self.team_id.clone()));
        m.insert("x".into(), Value::from(self.x));
        m.insert("y".into(), Value::from(self.y));
        m.insert("build_progress".into(), Value::from(round_n(self.build_progress, 3)));
        m.insert("level".into(), Value::from(self.level));
        m.insert("integrity".into(), Value::from(round_n(self.integrity, 3)));
        m.insert("max_integrity".into(), Value::from(self.max_integrity));
        m.insert("alert".into(), Value::from(round_n(self.alert, 3)));
        m.insert("last_alert".into(), Value::from(round_n(self.last_alert, 3)));
        m.insert("patrol_angle".into(), Value::from(round_n(self.patrol_angle, 5)));
        m.insert("resources".into(), Value::from(round_n(self.resources, 3)));
        m.insert("larder".into(), Value::from(round_n(self.larder, 3)));
        m.insert(
            "points_of_interest".into(),
            Value::Array(self.points_of_interest.iter().cloned().map(Value::Object).collect()),
        );
        Value::Object(m)
    }

    /// `BaseSite.from_dict`; `None` for anything unusable.
    pub fn from_dict(value: &Value) -> Option<BaseSite> {
        if !value.is_object() {
            return None;
        }
        let id = pymath::get_str(value, "id", "");
        let owner_id = pymath::get_str(value, "owner_id", "");
        let team: String = pymath::get_str(value, "team_id", "neutral").chars().take(32).collect();
        let team_id = if team.is_empty() { "neutral".to_string() } else { team };
        let x = pymath::get_float(value, "x", 0.0)?;
        let y = pymath::get_float(value, "y", 0.0)?;
        let mut site = BaseSite::new(&id, &owner_id, &team_id, x, y);
        site.build_progress = 0.0f64.max(MAX_BUILD_PROGRESS.min(pymath::get_float(value, "build_progress", 0.0)?));
        site.level = pymath::get_int(value, "level", 0)?.clamp(0, 5);
        site.integrity = 0.0f64.max(pymath::get_float(value, "integrity", 100.0)?);
        site.max_integrity = 1.0f64.max(pymath::get_float(value, "max_integrity", 100.0)?);
        site.alert = 0.0f64.max(1.0f64.min(pymath::get_float(value, "alert", 0.0)?));
        site.last_alert = 0.0f64.max(pymath::get_float(value, "last_alert", 0.0)?);
        site.patrol_angle = pymath::get_float(value, "patrol_angle", 0.0)?;
        site.resources = 0.0f64.max(1000.0f64.min(pymath::get_float(value, "resources", 0.0)?));
        site.larder = 0.0f64.max(1000.0f64.min(pymath::get_float(value, "larder", 0.0)?));
        // `[dict(p) for p in (value.get(...) or ()) if isinstance(p, dict)][-8:]`
        let pois: Vec<Map<String, Value>> = match value.get("points_of_interest") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a.iter().filter_map(|p| p.as_object().cloned()).collect(),
            Some(Value::String(_)) | Some(Value::Object(_)) => Vec::new(),
            Some(v) if !pymath::truthy(v) => Vec::new(),
            Some(_) => return None, // iterating a number raises TypeError
        };
        let skip = pois.len().saturating_sub(8);
        site.points_of_interest = pois.into_iter().skip(skip).collect();
        if site.id.is_empty() || site.owner_id.is_empty() { None } else { Some(site) }
    }
}

// ======================================================================
// BaseWorld
// ======================================================================

/// What a webber is doing for its claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebberMode {
    Repair,
    Weave,
}

/// `_webber_claims[key]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebberClaim {
    pub web: WebId,
    pub mode: WebberMode,
}

/// `_guard_line[key]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuardLine {
    pub end: f64,
    pub dwell: f64,
    pub anchor: Option<(f64, f64)>,
}

fn site_key_matches(id: &str, team: &str, pid: &str) -> bool {
    if team != "neutral" { id.strip_prefix("team:") == Some(team) } else { id.strip_prefix("owner:") == Some(pid) }
}

fn site_key<C: WorldCreature>(c: &C) -> String {
    let team = c.team_id();
    if team != "neutral" { format!("team:{team}") } else { format!("owner:{}", c.progression_id()) }
}

fn can_work<C: WorldCreature>(c: &C) -> bool {
    !c.player_controlled() && !(c.dragging() || c.airborne() || c.job_busy())
}

fn intent<'a>(mode: JobMode, target: Option<(f64, f64)>, base_id: Option<&'a str>) -> JobIntent<'a> {
    JobIntent { mode, target, alert_target: None, base_id, facing: None }
}

/// The colony layer used by every job.
#[derive(Debug)]
pub struct BaseWorld {
    pub screen_w: f64,
    pub screen_h: f64,
    /// In Python dict insertion order; `id` is the key.
    pub bases: Vec<BaseSite>,
    pub clock: f64,
    duty: HashMap<String, DutyCycle>,
    scout_targets: HashMap<String, usize>,
    scout_report_timers: HashMap<String, f64>,
    scout_coverage: HashMap<String, HashMap<usize, f64>>,
    webber_claims: HashMap<String, WebberClaim>,
    hunt_carry: HashMap<String, bool>,
    hunt_fed_seen: HashMap<String, bool>,
    hunt_patrol_angle: HashMap<String, f64>,
    hunt_home: HashMap<String, (f64, f64)>,
    guard_line: HashMap<String, GuardLine>,
    /// The real monitors (DC-85); `None` means the whole window counts.
    pub playfield: Option<Playfield>,
    // Per-frame scratch, reused.
    grid: SpatialGrid,
    scratch: Vec<u32>,
    guard_posts: Vec<(usize, Vec<usize>)>,
}

/// `max(200, int(v))`.
fn screen_dim(v: f64) -> f64 {
    200.0f64.max(v.trunc())
}

impl BaseWorld {
    /// `BaseWorld(screen_w, screen_h, saved)`.
    pub fn new(screen_w: f64, screen_h: f64, saved: &[Value]) -> BaseWorld {
        let mut world = BaseWorld {
            screen_w: screen_dim(screen_w),
            screen_h: screen_dim(screen_h),
            bases: Vec::new(),
            clock: 0.0,
            duty: HashMap::new(),
            scout_targets: HashMap::new(),
            scout_report_timers: HashMap::new(),
            scout_coverage: HashMap::new(),
            webber_claims: HashMap::new(),
            hunt_carry: HashMap::new(),
            hunt_fed_seen: HashMap::new(),
            hunt_patrol_angle: HashMap::new(),
            hunt_home: HashMap::new(),
            guard_line: HashMap::new(),
            playfield: None,
            grid: SpatialGrid::new(),
            scratch: Vec::new(),
            guard_posts: Vec::new(),
        };
        for raw in saved {
            if let Some(site) = BaseSite::from_dict(raw) {
                world.insert_base(site);
            }
        }
        world
    }

    /// `bases[site.id] = site`: replaces in place, else appends.
    pub fn insert_base(&mut self, site: BaseSite) {
        match self.bases.iter().position(|b| b.id == site.id) {
            Some(i) => self.bases[i] = site,
            None => self.bases.push(site),
        }
    }

    pub fn base(&self, id: &str) -> Option<&BaseSite> {
        self.bases.iter().find(|b| b.id == id)
    }

    pub fn base_mut(&mut self, id: &str) -> Option<&mut BaseSite> {
        self.bases.iter_mut().find(|b| b.id == id)
    }

    fn team_base_index(&self, team: &str) -> Option<usize> {
        self.bases.iter().position(|b| b.id.strip_prefix("team:") == Some(team))
    }

    pub fn set_screen(&mut self, screen_w: f64, screen_h: f64) {
        self.screen_w = screen_dim(screen_w);
        self.screen_h = screen_dim(screen_h);
        self.keep_all_on_screen();
    }

    /// How far a base must sit inside a monitor's edge (DC-85).
    pub fn screen_margin(site: &BaseSite) -> f64 {
        site.radius() + GUARD_STANDOFF_PAD + 24.0
    }

    fn keep_on_screen(&self, site: &mut BaseSite) {
        let margin = Self::screen_margin(site);
        if let Some(p) = &self.playfield {
            if !p.contains(site.x, site.y, margin) {
                (site.x, site.y) = p.clamp(site.x, site.y, margin);
            }
            return;
        }
        site.x = margin.max((self.screen_w - margin).min(site.x));
        site.y = margin.max((self.screen_h - margin).min(site.y));
    }

    /// Move every base onto a real monitor, clear of its edges.
    pub fn keep_all_on_screen(&mut self) {
        let mut bases = std::mem::take(&mut self.bases);
        for site in &mut bases {
            self.keep_on_screen(site);
        }
        self.bases = bases;
    }

    pub fn remove_base(&mut self, site_id: &str) -> bool {
        match self.bases.iter().position(|b| b.id == site_id) {
            Some(i) => {
                self.bases.remove(i);
                true
            }
            None => false,
        }
    }

    /// Pick a base up and put it down elsewhere (clamped on screen).
    pub fn move_base(&mut self, site_id: &str, x: f64, y: f64) -> bool {
        let Some(i) = self.bases.iter().position(|b| b.id == site_id) else {
            return false;
        };
        let mut site = self.bases[i].clone();
        site.x = x;
        site.y = y;
        self.keep_on_screen(&mut site);
        self.bases[i] = site;
        true
    }

    pub fn clear(&mut self) {
        self.bases.clear();
        self.duty.clear();
        self.scout_targets.clear();
        self.scout_report_timers.clear();
        self.scout_coverage.clear();
        self.webber_claims.clear();
        self.hunt_carry.clear();
        self.hunt_fed_seen.clear();
        self.hunt_patrol_angle.clear();
        self.hunt_home.clear();
        self.guard_line.clear();
        self.clock = 0.0;
    }

    /// `ensure_site`: the creature's own base, founded here if missing.
    pub fn ensure_site<C: WorldCreature>(&mut self, c: &C) -> &mut BaseSite {
        let i = self.ensure_site_index(c);
        &mut self.bases[i]
    }

    fn ensure_site_index<C: WorldCreature>(&mut self, c: &C) -> usize {
        let (team, pid) = (c.team_id(), c.progression_id());
        if let Some(i) = self.bases.iter().position(|b| site_key_matches(&b.id, team, pid)) {
            return i;
        }
        let mut site = BaseSite::new(
            &site_key(c),
            pid,
            team,
            42.0f64.max((self.screen_w - 42.0).min(c.x())),
            42.0f64.max((self.screen_h - 42.0).min(c.y())),
        );
        site.patrol_angle = py_mod(c.index() as f64 * 0.83, TAU);
        self.keep_on_screen(&mut site);
        self.bases.push(site);
        self.bases.len() - 1
    }

    fn needs_work(site: &BaseSite) -> bool {
        site.build_progress < MAX_BUILD_PROGRESS || site.integrity < site.max_integrity
    }

    /// `_site_for_team`: the nearest base of the creature's own team
    /// (a built one beats an empty one).
    pub fn site_for_team<C: WorldCreature>(&self, c: &C) -> Option<usize> {
        let (team, pid) = (c.team_id(), c.progression_id());
        let (cx, cy) = (c.x(), c.y());
        let neutral = team == "neutral";
        let own = if neutral { self.bases.iter().position(|b| site_key_matches(&b.id, team, pid)) } else { None };
        let include_others = own.is_none_or(|i| self.bases[i].build_progress <= 0.0);
        // Python's candidate list, in order: (own base), then the others.
        let candidates = || {
            own.into_iter().chain((0..self.bases.len()).filter(move |&i| {
                let b = &self.bases[i];
                if neutral {
                    include_others && Some(i) != own && b.team_id == "neutral" && b.build_progress > 0.0
                } else {
                    b.team_id == team
                }
            }))
        };
        let pick = |built_only: bool| -> Option<usize> {
            let mut best: Option<(usize, f64)> = None;
            for i in candidates() {
                let b = &self.bases[i];
                if built_only && b.build_progress <= 0.0 {
                    continue;
                }
                let d = hypot(b.x - cx, b.y - cy);
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            best.map(|b| b.0)
        };
        pick(true).or_else(|| pick(false))
    }

    /// Drop lone spiders' never-built bases whose spider is gone (DC-85).
    pub fn prune_orphan_sites<C: WorldCreature>(&mut self, creatures: &[C]) {
        self.bases.retain(|site| {
            !(site.team_id == "neutral"
                && site.build_progress <= 0.0
                && !creatures.iter().any(|c| c.progression_id() == site.owner_id))
        });
    }

    pub fn duty_state<C: WorldCreature>(&self, c: &C) -> Option<DutyCycle> {
        self.duty.get(c.progression_id()).copied()
    }

    #[allow(clippy::too_many_arguments)]
    fn on_duty(
        &mut self,
        key: &str,
        dt: f64,
        on: (f64, f64),
        off: (f64, f64),
        urgent: bool,
        productive: bool,
        rng: &mut PyRandom,
    ) -> bool {
        let duty = match self.duty.get_mut(key) {
            Some(d) => d,
            None => {
                let d = DutyCycle { on_duty: true, remaining: rng.uniform(on.0, on.1) };
                self.duty.entry(key.to_string()).or_insert(d)
            }
        };
        if urgent {
            duty.on_duty = true;
            duty.remaining = duty.remaining.max(rng.uniform(on.0, on.1));
            return true;
        }
        if duty.on_duty && !productive {
            return true;
        }
        duty.remaining -= 0.0f64.max(dt);
        if duty.remaining <= 0.0 {
            duty.on_duty = !duty.on_duty;
            let r = if duty.on_duty { on } else { off };
            duty.remaining = rng.uniform(r.0, r.1);
        }
        duty.on_duty
    }

    fn prune_duty<C: WorldCreature>(&mut self, creatures: &[C]) {
        if self.duty.len() <= creatures.len() {
            return;
        }
        let live = |k: &String| creatures.iter().any(|c| c.progression_id() == k);
        self.duty.retain(|k, _| live(k));
        self.scout_targets.retain(|k, _| live(k));
        self.scout_report_timers.retain(|k, _| live(k));
        self.webber_claims.retain(|k, _| live(k));
        self.hunt_carry.retain(|k, _| live(k));
        self.hunt_fed_seen.retain(|k, _| live(k));
        self.hunt_patrol_angle.retain(|k, _| live(k));
        self.hunt_home.retain(|k, _| live(k));
        self.guard_line.retain(|k, _| live(k));
    }

    /// Every named team on the desktop owns exactly one base, founded at
    /// the team's centre.
    pub fn ensure_team_sites<C: WorldCreature>(&mut self, creatures: &[C]) {
        // Teams in first-seen order (skipping dead and neutral).
        for (n, c) in creatures.iter().enumerate() {
            if c.dead() || c.team_id() == "neutral" {
                continue;
            }
            let team = c.team_id();
            if self.team_base_index(team).is_some() {
                continue;
            }
            if creatures[..n].iter().any(|o| !o.dead() && o.team_id() == team) {
                continue; // not the first member
            }
            let members: Vec<&C> = creatures[n..].iter().filter(|o| !o.dead() && o.team_id() == team).collect();
            let len = members.len() as f64;
            let mx = py_sum(members.iter().map(|m| m.x())) / len;
            let my = py_sum(members.iter().map(|m| m.y())) / len;
            let mut site = BaseSite::new(
                &format!("team:{team}"),
                members[0].progression_id(),
                team,
                42.0f64.max((self.screen_w - 42.0).min(mx)),
                42.0f64.max((self.screen_h - 42.0).min(my)),
            );
            site.patrol_angle = py_mod(members[0].index() as f64 * 0.83, TAU);
            self.bases.push(site);
        }
    }

    /// Credit a team's base with incidentally gathered food.
    pub fn credit_team_food(&mut self, team_id: Option<&str>, amount: f64) {
        let team = team_id.filter(|t| !t.is_empty()).unwrap_or("neutral");
        if team == "neutral" {
            return;
        }
        let Some(i) = self.team_base_index(team) else {
            return;
        };
        let site = &mut self.bases[i];
        let amount = 0.0f64.max(amount);
        let growth = amount * GROWTH_FOOD_SHARE;
        site.larder = 1000.0f64.min(site.larder + growth);
        site.resources = 1000.0f64.min(site.resources + amount - growth);
    }

    /// `to_dict`: sorted by id, for the state file's `bases`.
    pub fn to_dict(&self) -> Vec<Value> {
        let mut sorted: Vec<&BaseSite> = self.bases.iter().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        sorted.into_iter().map(BaseSite::to_dict).collect()
    }

    // -- the frame ------------------------------------------------------------

    /// `BaseWorld.update(dt, creatures, web_world)`; `rng` is the manager's.
    pub fn update<C: WorldCreature>(
        &mut self,
        dt: f64,
        creatures: &mut [C],
        mut web_world: Option<&mut WebWorld>,
        rng: &mut PyRandom,
    ) {
        self.clock += 0.0f64.max(dt);
        self.ensure_team_sites(creatures);
        self.prune_duty(creatures);
        if !creatures.is_empty() {
            self.prune_orphan_sites(creatures);
        }
        self.keep_all_on_screen();
        for c in creatures.iter_mut() {
            c.set_job_intent(&JobIntent::IDLE);
        }

        // Builders.
        for c in creatures.iter_mut() {
            if c.job_id() == "builder" {
                self.update_builder(dt, c, rng);
            }
        }

        // Guards, grouped by the base they answer to (first-seen order).
        for (_, list) in self.guard_posts.iter_mut() {
            list.clear();
        }
        let mut used = 0;
        for (i, c) in creatures.iter_mut().enumerate() {
            if c.job_id() != "guard" {
                continue;
            }
            let Some(s) = self.site_for_team(c) else {
                continue;
            };
            let id = self.bases[s].id.clone();
            c.set_job_base_id(Some(&id));
            match self.guard_posts[..used].iter_mut().find(|(site, _)| *site == s) {
                Some((_, list)) => list.push(i),
                None => {
                    if used == self.guard_posts.len() {
                        self.guard_posts.push((s, Vec::new()));
                    }
                    self.guard_posts[used].0 = s;
                    self.guard_posts[used].1.clear();
                    self.guard_posts[used].1.push(i);
                    used += 1;
                }
            }
        }
        if used > 0 {
            if creatures.len() >= GRID_MIN_ITEMS {
                self.grid.rebuild(128.0, creatures.iter().map(|c| (c.x(), c.y())));
            }
            let mut posts = std::mem::take(&mut self.guard_posts);
            for (s, posted) in posts[..used].iter_mut() {
                let site = &mut self.bases[*s];
                site.patrol_angle = py_mod(site.patrol_angle + dt * GUARD_POST_DRIFT, TAU);
                posted.sort_by(|&a, &b| creatures[a].progression_id().cmp(creatures[b].progression_id()));
            }
            for (s, posted) in posts[..used].iter() {
                let count = posted.len();
                for (slot, &g) in posted.iter().enumerate() {
                    self.update_guard(dt, g, *s, slot, count, creatures, rng);
                }
            }
            self.guard_posts = posts;
        }

        for i in 0..creatures.len() {
            if creatures[i].job_id() == "scout" {
                self.update_scout(dt, i, creatures, rng);
            }
        }
        for c in creatures.iter_mut() {
            if c.job_id() == "webber" {
                self.update_webber(dt, c, web_world.as_deref_mut(), rng);
            }
        }
        for i in 0..creatures.len() {
            if creatures[i].job_id() == "hunter" {
                self.update_hunter(dt, i, creatures, rng);
            }
        }
        self.apply_base_regen(dt, creatures);
    }

    fn update_builder<C: WorldCreature>(&mut self, dt: f64, builder: &mut C, rng: &mut PyRandom) {
        let s = self.ensure_site_index(builder);
        let (sx, sy) = (self.bases[s].x, self.bases[s].y);
        let dist = hypot(sx - builder.x(), sy - builder.y());
        let id = self.bases[s].id.clone();
        builder.set_job_base_id(Some(&id));
        if !Self::needs_work(&self.bases[s]) {
            return;
        }
        let site = &self.bases[s];
        let urgent = site.integrity < site.max_integrity * REPAIR_URGENT_INTEGRITY;
        let at_site = dist <= 22.0f64.max(site.radius() * 0.52);
        let able = can_work(builder);
        let key = builder.progression_id().to_string();
        if !self.on_duty(&key, dt, BUILD_DUTY_ON, BUILD_DUTY_OFF, urgent, at_site && able, rng) {
            return;
        }
        if !able {
            return;
        }
        if !at_site {
            builder.set_job_intent(&intent(JobMode::BuildTravel, Some((sx, sy)), Some(&id)));
            return;
        }
        let rate = 6.0 + 8.0f64.min(builder.level() * 0.25);
        let site = &mut self.bases[s];
        if site.build_progress < MAX_BUILD_PROGRESS && site.resources > 0.0 {
            let step = (dt * rate).min(site.resources / BUILD_RESOURCE_COST_PER_PROGRESS);
            if step > 0.0 {
                site.build_progress = MAX_BUILD_PROGRESS.min(site.build_progress + step);
                site.resources = 0.0f64.max(site.resources - step * BUILD_RESOURCE_COST_PER_PROGRESS);
            }
        }
        site.level = 5.min(py_floordiv(site.build_progress, 100.0) as i64);
        site.max_integrity = 100.0 + site.level as f64 * 35.0;
        site.integrity = site.max_integrity.min(site.integrity.max(site.max_integrity * 0.72) + dt * rate);
        builder.set_job_intent(&intent(JobMode::Build, Some((sx, sy)), Some(&id)));
    }

    #[allow(clippy::too_many_arguments)]
    fn update_guard<C: WorldCreature>(
        &mut self,
        dt: f64,
        g: usize,
        s: usize,
        slot: usize,
        count: usize,
        creatures: &mut [C],
        rng: &mut PyRandom,
    ) {
        let (sx, sy, radius, level) = {
            let site = &self.bases[s];
            (site.x, site.y, site.radius(), site.level)
        };
        let ring = radius + GUARD_ALERT_RADIUS_PAD + level as f64 * GUARD_ALERT_RADIUS_PER_LEVEL;
        // The nearest foe inside the ring (first found on a tie).
        let mut hostile: Option<usize> = None;
        let mut hostile_dist = f64::INFINITY;
        {
            let guard = &creatures[g];
            let mut consider = |i: usize| {
                if i == g {
                    return;
                }
                let other = &creatures[i];
                if other.dragging() || other.dead() {
                    return;
                }
                let d = hypot(other.x() - sx, other.y() - sy);
                if d <= ring && d < hostile_dist && guard.relation_to(other) == Relation::Foe {
                    hostile = Some(i);
                    hostile_dist = d;
                }
            };
            if creatures.len() >= GRID_MIN_ITEMS {
                self.grid.query(sx, sy, ring + 1.0, &mut self.scratch);
                for &i in &self.scratch {
                    consider(i as usize);
                }
            } else {
                for i in 0..creatures.len() {
                    consider(i);
                }
            }
        }
        let able = can_work(&creatures[g]);
        let key = creatures[g].progression_id().to_string();
        let on = self.on_duty(&key, dt, PATROL_DUTY_ON, PATROL_DUTY_OFF, hostile.is_some(), able, rng);
        let id = self.bases[s].id.clone();
        if let Some(h) = hostile {
            let site = &mut self.bases[s];
            site.alert = 1.0f64.min(site.alert + dt * 1.8);
            site.last_alert = self.clock;
            if able {
                let (hx, hy, hid): (f64, f64, CreatureId) = (creatures[h].x(), creatures[h].y(), creatures[h].id());
                creatures[g].set_job_intent(&JobIntent {
                    mode: JobMode::GuardAlert,
                    target: Some((hx, hy)),
                    alert_target: Some(hid),
                    base_id: Some(&id),
                    facing: None,
                });
            }
            return;
        }
        let site = &mut self.bases[s];
        site.alert = 0.0f64.max(site.alert - dt * 0.22);
        let patrol_angle = site.patrol_angle;
        if !on || !able {
            return;
        }
        let guard = &mut creatures[g];
        let (gx, gy) = (guard.x(), guard.y());
        let line = match self.guard_line.get_mut(&key) {
            Some(l) => l,
            None => {
                let end = if rng.random() < 0.5 { 1.0 } else { -1.0 };
                self.guard_line.entry(key.clone()).or_insert(GuardLine { end, dwell: 0.0, anchor: None })
            }
        };
        let axis = patrol_angle + (slot as f64 / 1.max(count) as f64) * PI;
        let standoff = radius + GUARD_STANDOFF_PAD;
        if let Some((ax, ay)) = line.anchor
            && hypot(gx - ax, gy - ay) > GUARD_LINE_ARRIVE * 2.0
        {
            line.anchor = None;
        }
        if let Some((ax, ay)) = line.anchor {
            line.dwell -= dt;
            if line.dwell > 0.0 {
                let facing = (ay - sy).atan2(ax - sx);
                guard.set_job_intent(&JobIntent {
                    mode: JobMode::Patrol,
                    target: Some((ax, ay)),
                    alert_target: None,
                    base_id: Some(&id),
                    facing: Some(facing),
                });
                return;
            }
            line.end = -line.end;
            line.anchor = None;
        }
        let end = (sx + axis.cos() * standoff * line.end, sy + axis.sin() * standoff * line.end);
        if hypot(end.0 - gx, end.1 - gy) <= GUARD_LINE_ARRIVE {
            line.anchor = Some((gx, gy));
            line.dwell = rng.uniform(GUARD_LINE_DWELL.0, GUARD_LINE_DWELL.1);
            let facing = (gy - sy).atan2(gx - sx);
            guard.set_job_intent(&JobIntent {
                mode: JobMode::Patrol,
                target: Some((gx, gy)),
                alert_target: None,
                base_id: Some(&id),
                facing: Some(facing),
            });
            return;
        }
        guard.set_job_intent(&intent(JobMode::Patrol, Some(end), Some(&id)));
    }

    /// `_sector_center`.
    pub fn sector_center(&self, sector: usize) -> (f64, f64) {
        let col = sector % SCOUT_SECTOR_COLS;
        let row = (sector / SCOUT_SECTOR_COLS) % SCOUT_SECTOR_ROWS;
        let cell_w = self.screen_w / SCOUT_SECTOR_COLS as f64;
        let cell_h = self.screen_h / SCOUT_SECTOR_ROWS as f64;
        (cell_w * (col as f64 + 0.5), cell_h * (row as f64 + 0.5))
    }

    fn pick_scout_sector(&mut self, team_id: &str, exclude_key: &str, rng: &mut PyRandom) -> usize {
        let total = SCOUT_SECTOR_COLS * SCOUT_SECTOR_ROWS;
        let coverage = self.scout_coverage.entry(team_id.to_string()).or_default();
        let taken = |s: usize| self.scout_targets.iter().any(|(k, v)| k != exclude_key && *v == s);
        let mut candidates: Vec<usize> = (0..total).filter(|&s| !taken(s)).collect();
        if candidates.is_empty() {
            candidates = (0..total).collect();
        }
        candidates.sort_by(|a, b| {
            let ca = coverage.get(a).copied().unwrap_or(-1.0);
            let cb = coverage.get(b).copied().unwrap_or(-1.0);
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let pool = 1.max(candidates.len() / 3);
        candidates[rng.choice_index(pool)]
    }

    fn report_sector<C: WorldCreature>(&mut self, s: usize, scout: usize, sector: usize, creatures: &[C]) {
        let (cx, cy) = self.sector_center(sector);
        let mut kind = "clear";
        let (mut px, mut py) = (cx, cy);
        let mut best_d = 150.0;
        let me = &creatures[scout];
        for (i, other) in creatures.iter().enumerate() {
            if i == scout || other.dragging() || other.dead() {
                continue;
            }
            let d = hypot(other.x() - cx, other.y() - cy);
            if d < best_d && me.relation_to(other) == Relation::Foe {
                best_d = d;
                kind = "foe";
                px = other.x();
                py = other.y();
            }
        }
        let mut poi = Map::new();
        poi.insert("sector".into(), Value::from(sector as i64));
        poi.insert("x".into(), Value::from(round_n(px, 1)));
        poi.insert("y".into(), Value::from(round_n(py, 1)));
        poi.insert("kind".into(), Value::from(kind));
        poi.insert("reported_at".into(), Value::from(round_n(self.clock, 2)));
        let site = &mut self.bases[s];
        site.points_of_interest.push(poi);
        if site.points_of_interest.len() > 8 {
            let extra = site.points_of_interest.len() - 8;
            site.points_of_interest.drain(..extra);
        }
    }

    fn update_scout<C: WorldCreature>(&mut self, dt: f64, i: usize, creatures: &mut [C], rng: &mut PyRandom) {
        let Some(s) = self.site_for_team(&creatures[i]) else {
            return;
        };
        let id = self.bases[s].id.clone();
        creatures[i].set_job_base_id(Some(&id));
        let able = can_work(&creatures[i]);
        let key = creatures[i].progression_id().to_string();
        let report_timer = self.scout_report_timers.get(&key).copied().unwrap_or(0.0);
        let reporting = report_timer > 0.0;
        let on = self.on_duty(&key, dt, SCOUT_DUTY_ON, SCOUT_DUTY_OFF, false, able, rng);
        if !on || !able {
            return;
        }
        if reporting {
            let timer = 0.0f64.max(report_timer - dt);
            self.scout_report_timers.insert(key.clone(), timer);
            let target = self.sector_center(self.scout_targets.get(&key).copied().unwrap_or(0));
            creatures[i].set_job_intent(&intent(JobMode::ScoutReport, Some(target), Some(&id)));
            if timer <= 0.0 {
                self.scout_targets.remove(&key);
            }
            return;
        }
        let sector = match self.scout_targets.get(&key) {
            Some(&s) => s,
            None => {
                let team = self.bases[s].team_id.clone();
                let sector = self.pick_scout_sector(&team, &key, rng);
                self.scout_targets.insert(key.clone(), sector);
                sector
            }
        };
        let target = self.sector_center(sector);
        if hypot(target.0 - creatures[i].x(), target.1 - creatures[i].y()) > SCOUT_ARRIVE_DIST {
            creatures[i].set_job_intent(&intent(JobMode::ScoutTravel, Some(target), Some(&id)));
        } else {
            let team = self.bases[s].team_id.clone();
            let clock = self.clock;
            self.scout_coverage.entry(team).or_default().insert(sector, clock);
            self.report_sector(s, i, sector, creatures);
            self.scout_report_timers.insert(key, rng.uniform(SCOUT_REPORT_DWELL.0, SCOUT_REPORT_DWELL.1));
            creatures[i].set_job_intent(&intent(JobMode::ScoutReport, Some(target), Some(&id)));
        }
    }

    fn find_webber_work<C: WorldCreature>(
        &self,
        webber: &mut C,
        s: usize,
        world: &mut WebWorld,
    ) -> Option<WebberClaim> {
        let site = &self.bases[s];
        let near = WEBBER_NEAR_BASE_PAD + site.radius();
        let (wx, wy) = (webber.x(), webber.y());
        if let Some(id) = world.find_repairable_web(wx, wy, 1e9) {
            let hub = world.web(id).unwrap().hub;
            if hypot(hub.0 - site.x, hub.1 - site.y) <= near && world.claim_repair(id, webber.id()) {
                return Some(WebberClaim { web: id, mode: WebberMode::Repair });
            }
        }
        if let Some(id) = world.find_adoptable_web(wx, wy, 1e9) {
            let hub = world.web(id).unwrap().hub;
            if hypot(hub.0 - site.x, hub.1 - site.y) <= near && world.adopt(id, webber.id()) {
                return Some(WebberClaim { web: id, mode: WebberMode::Weave });
            }
        }
        let team_webs = world
            .webs
            .iter()
            .filter(|w| w.is_complete() && !w.is_damaged() && hypot(w.hub.0 - site.x, w.hub.1 - site.y) <= near)
            .count();
        if team_webs < WEBBER_TARGET_WEB_COUNT {
            let (id, size) = (webber.id(), webber.size());
            if let Some(web) = world.claim_site(id, size, false, None, Some(webber.rng())) {
                return Some(WebberClaim { web, mode: WebberMode::Weave });
            }
        }
        None
    }

    fn update_webber<C: WorldCreature>(
        &mut self,
        dt: f64,
        webber: &mut C,
        mut world: Option<&mut WebWorld>,
        rng: &mut PyRandom,
    ) {
        let Some(s) = self.site_for_team(webber) else {
            return;
        };
        let id = self.bases[s].id.clone();
        webber.set_job_base_id(Some(&id));
        let able = can_work(webber);
        let key = webber.progression_id().to_string();
        let mut claim = self.webber_claims.get(&key).copied();
        if let Some(c) = claim
            && !world.as_deref().is_some_and(|w| w.contains(c.web))
        {
            claim = None;
            self.webber_claims.remove(&key);
        }
        if claim.is_none()
            && let Some(w) = world.as_deref_mut()
        {
            claim = self.find_webber_work(webber, s, w);
            if let Some(c) = claim {
                self.webber_claims.insert(key.clone(), c);
            }
        }
        let productive = able && claim.is_some();
        let urgent = claim.is_some_and(|c| c.mode == WebberMode::Repair);
        let on = self.on_duty(&key, dt, WEBBER_DUTY_ON, WEBBER_DUTY_OFF, urgent, productive, rng);
        let (Some(claim), true, true) = (claim, on, able) else {
            return;
        };
        let Some(world) = world else {
            return;
        };
        let (wx, wy) = (webber.x(), webber.y());
        let web = world.web_mut(claim.web).expect("claimed web exists");
        let hub = web.hub;
        if hypot(hub.0 - wx, hub.1 - wy) > WEBBER_ARRIVE_DIST {
            webber.set_job_intent(&intent(JobMode::WebTravel, Some(hub), Some(&id)));
            return;
        }
        match claim.mode {
            WebberMode::Repair => {
                web.repair_near(wx, wy, dt, webs::REPAIR_RATE, 1e9);
                let damaged = web.is_damaged();
                webber.set_job_intent(&intent(JobMode::WebRepair, Some(hub), Some(&id)));
                if !damaged {
                    world.release_repair(Some(claim.web));
                    self.webber_claims.remove(&key);
                }
            }
            WebberMode::Weave => {
                let (tip, _drawing) = web.working_point();
                web.advance(dt, WEBBER_WEAVE_SPEED, tip, webs::DEFAULT_LEAD_MAX);
                let complete = web.is_complete();
                webber.set_job_intent(&intent(JobMode::WebWeave, Some(hub), Some(&id)));
                if complete {
                    self.webber_claims.remove(&key);
                }
            }
        }
    }

    fn update_hunter<C: WorldCreature>(&mut self, dt: f64, i: usize, creatures: &mut [C], rng: &mut PyRandom) {
        let key = creatures[i].progression_id().to_string();
        let site = self.site_for_team(&creatures[i]);
        let (home_x, home_y, home_radius, base_id) = match site {
            Some(s) => {
                let b = &self.bases[s];
                (b.x, b.y, b.radius(), Some(b.id.clone()))
            }
            None => {
                let pos = (creatures[i].x(), creatures[i].y());
                let &mut (hx, hy) = self.hunt_home.entry(key.clone()).or_insert(pos);
                (hx, hy, 60.0, None)
            }
        };
        let base = base_id.as_deref();
        creatures[i].set_job_base_id(base);
        let able = can_work(&creatures[i]);
        let hunting_now = creatures[i].hunting_prey();
        let fed_now = creatures[i].is_feeding();
        if fed_now && !self.hunt_fed_seen.get(&key).copied().unwrap_or(false) {
            self.hunt_carry.insert(key.clone(), true);
        }
        self.hunt_fed_seen.insert(key.clone(), fed_now);
        let carrying = self.hunt_carry.get(&key).copied().unwrap_or(false);

        if hunting_now && !carrying {
            creatures[i].set_job_intent(&intent(JobMode::Hunting, None, base));
            return;
        }
        if carrying {
            let dist = hypot(home_x - creatures[i].x(), home_y - creatures[i].y());
            if dist > 30.0 {
                creatures[i].set_job_intent(&intent(JobMode::HuntReturn, Some((home_x, home_y)), base));
            } else {
                if let Some(s) = site {
                    let b = &mut self.bases[s];
                    b.resources = 1000.0f64.min(b.resources + HUNTER_CARRY_FOOD_AMOUNT);
                }
                self.hunt_carry.insert(key, false);
            }
            return;
        }
        let on = self.on_duty(&key, dt, HUNTER_DUTY_ON, HUNTER_DUTY_OFF, false, able, rng);
        if !on || !able {
            return;
        }
        let angle = match self.hunt_patrol_angle.get(&key) {
            Some(&a) => a,
            None => rng.uniform(0.0, TAU),
        };
        let angle = py_mod(angle + dt * 0.6, TAU);
        self.hunt_patrol_angle.insert(key, angle);
        let breathe = 1.0 + (angle * 2.7).sin() * HUNTER_RANGE_BREATHE;
        let radius = 80.0f64.max(home_radius + HUNTER_PATROL_RADIUS_PAD) + HUNTER_RANGE_RADIUS * breathe;
        let mut target = (home_x + angle.cos() * radius, home_y + angle.sin() * radius);
        let mut alert_target = None;
        let mut best_d = radius + HUNTER_FOE_SCAN_PAD;
        {
            let hunter = &creatures[i];
            for (j, other) in creatures.iter().enumerate() {
                if j == i || other.dragging() || other.dead() {
                    continue;
                }
                let d = hypot(other.x() - home_x, other.y() - home_y);
                if d <= best_d && hunter.relation_to(other) == Relation::Foe {
                    best_d = d;
                    alert_target = Some(other.id());
                    target = (other.x(), other.y());
                }
            }
        }
        creatures[i].set_job_intent(&JobIntent {
            mode: JobMode::HuntPatrol,
            target: Some(target),
            alert_target,
            base_id: base,
            facing: None,
        });
    }

    fn apply_base_regen<C: WorldCreature>(&self, dt: f64, creatures: &mut [C]) {
        for c in creatures.iter_mut() {
            if c.dragging() {
                continue;
            }
            let team = c.team_id();
            if team == "neutral" {
                continue;
            }
            let Some(i) = self.team_base_index(team) else {
                continue;
            };
            let site = &self.bases[i];
            if site.level <= 0 {
                continue;
            }
            if hypot(c.x() - site.x, c.y() - site.y) <= site.radius() + BASE_REGEN_RADIUS_PAD {
                c.heal(dt * BASE_REGEN_HP_PER_LEVEL * site.level as f64);
            }
        }
    }

    /// The webber claim a worker holds, if any.
    pub fn webber_claim<C: WorldCreature>(&self, c: &C) -> Option<WebberClaim> {
        self.webber_claims.get(c.progression_id()).copied()
    }
}
