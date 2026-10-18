//! Port of `tests/test_arbiter.py`: the single utility arbiter (DC-18, C1/C6).
//!
//! not ported: test_consider_special_actions_is_deleted (greps Python source text).
//! not ported: test_activate_scheduled_phase_is_deleted (greps Python source text).
//! not ported: test_drive_hunt_is_gone_from_the_manager (Python source text / CreatureManager).
//! not ported: test_behaviour_now_defers_decisions_to_the_arbiter (greps Python source text).
//! not ported: test_pursue_prey_replaces_the_manual_drive_hunt_and_still_catches_flies (needs CreatureManager).
//! not ported: test_perception_publishes_the_prey_candidate (needs CreatureManager).
//! not ported: test_arbiter_runs_at_a_throttled_rate_not_every_frame (monkeypatches arbiter.decide).

mod common;

use dc_creature::constants::{JOB_MODE_STATES, JOB_STATES};
use dc_creature::{Creature, State};
use std::collections::HashSet;

const DT: f64 = 1.0 / 60.0;
const AWAY: (f64, f64) = (-100000.0, -100000.0);

/// `Creature(model, curious, 1600, 900, index=0, job_id=job_id)`; seeded
/// instead of unseeded.
fn build_guard(job_id: &str) -> Creature {
    let model = common::load_model("plush_snow_hybrid_2");
    let personality = common::load_personality("curious");
    let job_id = job_id.to_string();
    common::spider_with(model, personality, 1, move |cfg| {
        cfg.index = 0;
        cfg.job_id = job_id;
    })
}

/// `random.seed(seed)` + an unseeded playful tarantula: here a seeded one.
fn build_playful(seed: i64) -> Creature {
    let model = common::load_model("tarantula");
    let traits = common::load_personality("playful");
    let mut creature = common::spider_with(model, traits, seed, |cfg| {
        cfg.index = 0;
        cfg.progression_id = Some(format!("arbiter:{seed}"));
    });
    creature.kin.x = 800.0;
    creature.kin.y = 450.0;
    creature.initialize_legs();
    common::run(&mut creature, 60, AWAY.0, AWAY.1);
    creature
}

#[test]
fn test_job_duty_outranks_the_arbiter() {
    let mut guard = build_guard("guard");
    guard.job_mode = "patrol".into();
    guard.job_target = Some((600.0, 400.0));
    guard.state = State::Idle;
    guard.decision_timer = 0.0;
    guard.state_timer = 0.0;
    for _ in 0..30 {
        common::step(&mut guard, DT, AWAY.0, AWAY.1, &[]);
        assert_eq!(guard.state, State::JobPatrol, "the arbiter must never run while a job is actively claiming the tick");
    }
}

#[test]
fn test_job_mode_states_table_covers_every_job_state() {
    let table: HashSet<State> = JOB_MODE_STATES.iter().map(|(_, s)| *s).collect();
    let states: HashSet<State> = JOB_STATES.iter().copied().collect();
    assert_eq!(table, states);
}

#[test]
fn test_unrecognized_job_id_still_hands_back_a_leftover_job_state() {
    let mut guard = build_guard("some-future-job-id");
    guard.state = State::JobPatrol;
    guard.kin.motion_paused = false;
    guard.kin.speed = 40.0;
    guard.job_mode = "idle".into();
    guard.job_target = None;
    assert!(!guard.update_job_state(DT));
    assert_eq!(guard.state, State::Idle);
    assert!(!guard.kin.motion_paused);
}

#[test]
fn test_idle_does_not_re_roll_its_timer_before_it_expires() {
    let mut creature = build_playful(5);
    let mut seen_states = HashSet::new();
    for _ in 0..600 {
        common::step(&mut creature, DT, AWAY.0, AWAY.1, &[]);
        seen_states.insert(creature.state);
    }
    assert_ne!(seen_states, HashSet::from([State::Idle]), "creature never left Idle in 10 simulated seconds");
}

#[test]
fn test_unrecognized_job_id_can_still_do_its_job_when_mode_matches_the_table() {
    let mut guard = build_guard("some-future-job-id");
    guard.state = State::Idle;
    guard.job_mode = "patrol".into();
    guard.job_target = Some((500.0, 500.0));
    assert!(guard.update_job_state(DT));
    assert_eq!(guard.state, State::JobPatrol);
}
