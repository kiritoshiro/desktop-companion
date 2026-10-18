//! Shared helpers for the Python-oracle comparisons.
#![allow(dead_code)]

use dc_core::PyRandom;
use dc_world::creature_view::{CreatureId, JobIntent, JobMode, Relation, WorldCreature};
use serde_json::Value;

pub const DT: f64 = 1.0 / 60.0;

pub fn load(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

pub fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("number expected, got {v}"))
}

pub fn pair(v: &Value) -> Option<(f64, f64)> {
    v.as_array().map(|a| (f(&a[0]), f(&a[1])))
}

pub fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[track_caller]
pub fn assert_close(got: f64, want: &Value, ctx: &str) {
    let w = f(want);
    assert!(close(got, w), "{ctx}: got {got}, want {w}");
}

#[track_caller]
pub fn assert_pair(got: Option<(f64, f64)>, want: &Value, ctx: &str) {
    match (got, pair(want)) {
        (None, None) => {}
        (Some(g), Some(w)) => assert!(close(g.0, w.0) && close(g.1, w.1), "{ctx}: got {g:?}, want {w:?}"),
        (g, w) => panic!("{ctx}: got {g:?}, want {w:?}"),
    }
}

/// Deep JSON comparison: same shape and key set, integers exact, floats
/// within 1e-9 (relative), and an integer never standing in for a float.
#[track_caller]
pub fn assert_json(got: &Value, want: &Value, ctx: &str) {
    json_close(got, want, ctx);
}

fn json_close(got: &Value, want: &Value, path: &str) {
    match (got, want) {
        (Value::Number(g), Value::Number(w)) => {
            assert_eq!(g.is_f64(), w.is_f64(), "{path}: number kind differs (got {g}, want {w})");
            if w.is_f64() {
                let (g, w) = (g.as_f64().unwrap(), w.as_f64().unwrap());
                assert!(close(g, w), "{path}: got {g}, want {w}");
            } else {
                assert_eq!(g, w, "{path}");
            }
        }
        (Value::Array(g), Value::Array(w)) => {
            assert_eq!(g.len(), w.len(), "{path}: length (got {got}, want {want})");
            for (i, (a, b)) in g.iter().zip(w).enumerate() {
                json_close(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(g), Value::Object(w)) => {
            let mut gk: Vec<&String> = g.keys().collect();
            let mut wk: Vec<&String> = w.keys().collect();
            gk.sort();
            wk.sort();
            assert_eq!(gk, wk, "{path}: keys");
            for (k, b) in w {
                json_close(&g[k], b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(got, want, "{path}"),
    }
}

/// Python `x % y` for floats.
pub fn py_mod(a: f64, b: f64) -> f64 {
    dc_core::math::py_mod(a, b)
}

/// `tri` in the generator: a point bouncing between `lo` and `hi`.
pub fn tri(x0: f64, v: f64, lo: f64, hi: f64, t: f64) -> f64 {
    let span = hi - lo;
    let p = py_mod(x0 - lo + v * t, 2.0 * span);
    lo + if p < span { p } else { 2.0 * span - p }
}

/// A creature with every field a world reads or writes, for tests.
#[derive(Debug)]
pub struct Bug {
    pub id: CreatureId,
    pub index: i64,
    pub pid: String,
    pub team: String,
    pub job: String,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub size: f64,
    pub level: f64,
    pub dead: bool,
    pub dragging: bool,
    pub airborne: bool,
    pub job_busy: bool,
    pub player: bool,
    pub webbed: bool,
    pub feeding: bool,
    pub hunting: bool,
    pub hunter: bool,
    pub prey: Option<dc_world::FlyId>,
    pub hidden: bool,
    pub speed: f64,
    pub aim: f64,
    pub homing: f64,
    pub hp: Option<f64>,
    pub rng: PyRandom,
    /// "neutral" vs everyone, else friend/foe by team (the job tests' rule).
    pub neutral_is_nobody: bool,
    /// Foe only of team "rivals" (`test_jobs.py`'s `DummySpider` rule).
    pub rivals_rule: bool,
    // What the worlds wrote.
    pub mode: JobMode,
    pub target: Option<(f64, f64)>,
    pub alert: Option<CreatureId>,
    pub base_id: Option<String>,
    pub facing: Option<f64>,
    pub pins: Vec<(i64, String, u64)>,
    pub frame: i64,
    pub webbed_until: i64,
}

impl Bug {
    pub fn new(index: i64, job: &str, team: &str, x: f64, y: f64) -> Bug {
        Bug {
            id: CreatureId(index as u64),
            index,
            pid: format!("test:{index}"),
            team: team.to_string(),
            job: job.to_string(),
            x,
            y,
            heading: 0.0,
            size: 20.0,
            level: 1.0,
            dead: false,
            dragging: false,
            airborne: false,
            job_busy: false,
            player: false,
            webbed: false,
            feeding: false,
            hunting: false,
            hunter: false,
            prey: None,
            hidden: false,
            speed: 0.0,
            aim: 0.0,
            homing: 0.0,
            hp: None,
            rng: PyRandom::from_u64(900 + index as u64),
            neutral_is_nobody: true,
            rivals_rule: false,
            mode: JobMode::Idle,
            target: None,
            alert: None,
            base_id: None,
            facing: None,
            pins: Vec::new(),
            frame: 0,
            webbed_until: -1,
        }
    }

    /// `test_jobs.py`'s `DummySpider`.
    pub fn dummy(job: &str, team: &str, x: f64, y: f64, ident: i64) -> Bug {
        let mut b = Bug::new(ident, job, team, x, y);
        b.rivals_rule = true;
        b
    }
}

impl WorldCreature for Bug {
    fn id(&self) -> CreatureId {
        self.id
    }
    fn index(&self) -> i64 {
        self.index
    }
    fn progression_id(&self) -> &str {
        &self.pid
    }
    fn team_id(&self) -> &str {
        if self.team.is_empty() { "neutral" } else { &self.team }
    }
    fn job_id(&self) -> &str {
        &self.job
    }
    fn x(&self) -> f64 {
        self.x
    }
    fn y(&self) -> f64 {
        self.y
    }
    fn heading(&self) -> f64 {
        self.heading
    }
    fn size(&self) -> f64 {
        self.size
    }
    fn level(&self) -> f64 {
        self.level
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn dragging(&self) -> bool {
        self.dragging
    }
    fn airborne(&self) -> bool {
        self.airborne
    }
    fn job_busy(&self) -> bool {
        self.job_busy
    }
    fn player_controlled(&self) -> bool {
        self.player
    }
    fn webbed(&self) -> bool {
        self.webbed
    }
    fn is_feeding(&self) -> bool {
        self.feeding
    }
    fn hunting_prey(&self) -> bool {
        self.hunting
    }
    fn acts_as_hunter(&self) -> bool {
        self.hunter
    }
    fn prey_fly(&self) -> Option<dc_world::FlyId> {
        self.prey
    }
    fn desktop_fully_hidden(&self) -> bool {
        self.hidden
    }
    fn current_speed(&self) -> f64 {
        self.speed
    }
    fn aim_intent(&self) -> f64 {
        self.aim
    }
    fn web_homing(&self) -> f64 {
        self.homing
    }
    fn relation_to(&self, other: &Self) -> Relation {
        let (mine, theirs) = (self.team_id(), other.team_id());
        if self.rivals_rule {
            return if theirs == "rivals" {
                Relation::Foe
            } else if mine == theirs {
                Relation::Friend
            } else {
                Relation::Neutral
            };
        }
        if self.neutral_is_nobody && (mine == "neutral" || theirs == "neutral") {
            return Relation::Neutral;
        }
        if mine == theirs { Relation::Friend } else { Relation::Foe }
    }
    fn set_job_intent(&mut self, intent: &JobIntent<'_>) {
        self.mode = intent.mode;
        self.target = intent.target;
        self.alert = intent.alert_target;
        self.base_id = intent.base_id.map(str::to_string);
        self.facing = intent.facing;
    }
    fn set_job_base_id(&mut self, base_id: Option<&str>) {
        self.base_id = base_id.map(str::to_string);
    }
    fn heal(&mut self, amount: f64) -> f64 {
        match self.hp {
            Some(hp) => {
                let after = 100.0f64.min(hp + amount.max(0.0));
                self.hp = Some(after);
                after - hp
            }
            None => 0.0,
        }
    }
    fn web_pinned(&mut self, kind: &str, by: CreatureId) {
        self.webbed = true;
        self.webbed_until = self.frame + 90;
        self.pins.push((self.frame, kind.to_string(), by.0));
    }
    fn rng(&mut self) -> &mut PyRandom {
        &mut self.rng
    }
}
