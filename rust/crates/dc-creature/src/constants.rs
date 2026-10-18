//! Port of `creature/constants.py`.
//!
//! The gait half (`smootherstep`, `GAIT_BY_STATE`, `GAIT_SPELL_*`,
//! `VALID_GAIT_STYLES`, `GAIT_LABELS`, `normalize_gait_style`) lives in
//! `dc-kinematics` and is re-exported here; everything else (combat, webbing,
//! nerve, jobs) is defined below with the Python values.

pub use dc_kinematics::constants::*;

use crate::state::State;

// DC-22: how a hit resolves.
pub const MIN_DAMAGE_FRACTION: f64 = 0.15;

// DC-45: silk landed on a spider by another spider.
pub const WEBBED_SECONDS: f64 = 4.6;
pub const WEBBED_SPEED_MULT: f64 = 0.35;
/// DC-50 / 2026-09-25: the whole pin holds the spider still.
pub const WEBBED_HOLD_SECONDS: f64 = WEBBED_SECONDS;
pub const STRUGGLE_PERIOD: f64 = 0.55;
pub const STRUGGLE_DUTY: f64 = 0.62;
pub const STRUGGLE_DRAIN: f64 = 0.75;

// DC-50: nerve.
pub const FLEE_HEALTH_FRACTION: f64 = 0.35;
pub const RALLY_HEALTH_FRACTION: f64 = 0.72;
pub const THREAT_SCAN_RADIUS: f64 = 260.0;
pub const OUTNUMBERED_RATIO: f64 = 1.5;
pub const FLEE_SPEED_MULT: f64 = 1.45;

// DC-64: a retreat has to end.
pub const ESCAPED_RADIUS: f64 = THREAT_SCAN_RADIUS * 1.6;
pub const ESCAPED_SECONDS: f64 = 1.1;
pub const RECOVER_ARRIVE_RADIUS: f64 = 40.0;

/// `JOB_STATES`
pub const JOB_STATES: [State; 11] = [
    State::JobTravel,
    State::JobBuild,
    State::JobPatrol,
    State::JobGuardAlert,
    State::JobScoutTravel,
    State::JobScoutReport,
    State::JobWebTravel,
    State::JobWebRepair,
    State::JobWebWeave,
    State::JobHuntPatrol,
    State::JobHuntReturn,
];

/// `JOB_MODE_STATES`: job mode -> state.
pub const JOB_MODE_STATES: [(&str, State); 11] = [
    ("build_travel", State::JobTravel),
    ("build", State::JobBuild),
    ("patrol", State::JobPatrol),
    ("guard_alert", State::JobGuardAlert),
    ("scout_travel", State::JobScoutTravel),
    ("scout_report", State::JobScoutReport),
    ("web_travel", State::JobWebTravel),
    ("web_repair", State::JobWebRepair),
    ("web_weave", State::JobWebWeave),
    ("hunt_patrol", State::JobHuntPatrol),
    ("hunt_return", State::JobHuntReturn),
];

/// `JOB_MODE_STATES.get(mode)`
pub fn job_mode_state(mode: &str) -> Option<State> {
    JOB_MODE_STATES.iter().find(|(m, _)| *m == mode).map(|(_, s)| *s)
}

/// `HUNT_COMMITTED_STATES`
pub const HUNT_COMMITTED_STATES: [State; 17] = [
    State::Aim,
    State::Coil,
    State::Jump,
    State::Land,
    State::Catch,
    State::Feed,
    State::Roll,
    State::DriftRun,
    State::Play,
    State::Cuddle,
    State::Inspect,
    State::WebAim,
    State::WebShot,
    State::Weave,
    State::WeaveApproach,
    State::Repair,
    State::RepairApproach,
];

/// `HUNT_BUSY_STATES`
pub const HUNT_BUSY_STATES: [State; 6] =
    [State::WebAim, State::WebShot, State::Weave, State::WeaveApproach, State::Repair, State::RepairApproach];

/// `JOB_PREEMPTING_STATES`
pub const JOB_PREEMPTING_STATES: [State; 13] = [
    State::Retreat,
    State::Startled,
    State::Coil,
    State::Jump,
    State::Land,
    State::Catch,
    State::Feed,
    State::WebAim,
    State::WebShot,
    State::WeaveApproach,
    State::Weave,
    State::RepairApproach,
    State::Repair,
];

// DC-59: what a fight looks like.
pub const COMBAT_SPACING: f64 = 1.35;
pub const COMBAT_SPACING_SLACK: f64 = 0.22;
pub const STANCE_RISE_PER_SECOND: f64 = 4.5;
pub const STANCE_FALL_PER_SECOND: f64 = 1.6;
pub const LUNGE_REACH: f64 = 0.42;
pub const RECOIL_REACH: f64 = 0.26;
pub const LUNGE_DECAY_PER_SECOND: f64 = 4.2;
pub const STRIKE_WINDUP: f64 = 0.09;
pub const STRIKE_SNAP: f64 = 0.07;
pub const STRIKE_DURATION: f64 = 0.38;
pub const STRIKE_DRAW_BACK: f64 = 0.45;
pub const STRIKE_OVERSHOOT: f64 = 1.6;

// DC-62: how much a spider minds being picked up.
pub const CURSOR_PRESSURE_PER_GRAB: f64 = 0.34;
pub const CURSOR_PRESSURE_DECAY_PER_SECOND: f64 = 1.0 / 120.0;
pub const CURSOR_WARY_THRESHOLD: f64 = 0.5;
