//! Port of `tests/test_phase_scheduler.py`: weighted personality behaviour
//! phases.

mod common;

use dc_content::personality::Personality;
use dc_content::personality_profiles::{BEHAVIOUR_PHASE_IDS, PersonalityRef, phase_scores_for};
use dc_core::PyRandom;
use dc_creature::creature::typed_personality;
use dc_creature::phase_scheduler::{BehaviourPhaseScheduler, Focus, Phase, normalize_focus};
use dc_creature::{CreatureConfig, State};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

const ENABLED: [&str; 12] = BEHAVIOUR_PHASE_IDS;

fn schema_base() -> Map<String, Value> {
    json!({
        "id": "schema-test",
        "display_name": "Schema test",
        "speed_multiplier": 1.0,
        "reaction_radius": 100,
        "boldness": 0.5,
        "wander_frequency": 0.5,
    })
    .as_object()
    .unwrap()
    .clone()
}

fn scores_with(weights: &[(&str, f64)]) -> Value {
    let mut scores = Map::new();
    for id in BEHAVIOUR_PHASE_IDS {
        scores.insert(id.to_string(), json!(0));
    }
    for (k, v) in weights {
        scores.insert(k.to_string(), json!(v));
    }
    Value::Object(scores)
}

fn personality_json() -> Value {
    json!({"id": "phase-test", "phase_scores": scores_with(&[("roll", 10.0), ("jump", 1.0)])})
}

fn personality() -> Personality {
    typed_personality(&personality_json())
}

fn scheduler(p: &Personality, enabled: &[&str], seed: i128) -> (BehaviourPhaseScheduler, PyRandom) {
    let mut rng = PyRandom::from_int(seed);
    let s = BehaviourPhaseScheduler::new(p, enabled, &mut rng);
    (s, rng)
}

fn counter(deck: &[Phase]) -> HashMap<Phase, usize> {
    let mut out = HashMap::new();
    for p in deck {
        *out.entry(*p).or_insert(0) += 1;
    }
    out
}

#[test]
fn test_the_deck_holds_only_what_is_scored() {
    let (s, _) = scheduler(&personality(), &ENABLED, 41);
    assert_eq!(counter(&s.deck), HashMap::from([(Phase::Roll, 10), (Phase::Jump, 1)]));
    assert!(!s.deck.contains(&Phase::Wander));
}

#[test]
fn test_the_same_seed_deals_the_same_deck() {
    let p = personality();
    let (first, _) = scheduler(&p, &ENABLED, 41);
    let (second, _) = scheduler(&p, &ENABLED, 41);
    assert_eq!(first.deck, second.deck);
}

#[test]
fn test_only_scored_phases_are_allowed() {
    let (s, _) = scheduler(&personality(), &ENABLED, 41);
    assert!(s.is_allowed_id("roll"));
    assert!(!s.is_allowed_id("wander"));
}

#[test]
fn test_two_names_for_the_same_focus_plan_alike() {
    let p = personality();
    let (mut first, mut r1) = scheduler(&p, &ENABLED, 41);
    let (mut second, mut r2) = scheduler(&p, &ENABLED, 41);
    let first_plan = first.choose(normalize_focus(Some("mouse")), &mut r1).expect("a plan");
    let second_plan = second.choose(normalize_focus(Some("cursor")), &mut r2).expect("a plan");
    assert_eq!(first_plan.phase_id, second_plan.phase_id);
    assert_eq!(first_plan.focus, Focus::Cursor);
    assert_eq!(first_plan.focus.as_str(), "cursor");
    assert_eq!(first_plan.duration, second_plan.duration);
}

#[test]
fn test_a_duration_stays_inside_its_scored_range() {
    let (mut s, mut rng) = scheduler(&personality(), &ENABLED, 41);
    let plan = s.choose(normalize_focus(Some("mouse")), &mut rng).expect("a plan");
    let (low, high) = plan.phase_id.duration_range();
    let scale = s.duration_multiplier * (0.72 + plan.score / 10.0 * 0.56);
    assert!(low * scale <= plan.duration && plan.duration <= high * scale, "{plan:?}");
}

#[test]
fn test_a_gated_scheduler_offers_only_what_is_enabled() {
    let (mut gated, mut rng) = scheduler(&personality(), &["jump"], 2);
    assert_eq!(gated.choose(normalize_focus(Some("none")), &mut rng).unwrap().phase_id, Phase::Jump);
    assert_eq!(gated.choose(normalize_focus(Some("none")), &mut rng).unwrap().phase_id, Phase::Jump);
}

#[test]
fn test_a_scheduler_with_nothing_enabled_offers_nothing() {
    let (mut empty, mut rng) = scheduler(&personality(), &[], 2);
    assert!(empty.choose(normalize_focus(Some("none")), &mut rng).is_none());
}

#[test]
fn test_a_phase_that_needs_a_target_waits_for_one() {
    let p = typed_personality(&json!({"id": "target-only", "phase_scores": scores_with(&[("approach", 10.0)])}));
    let (mut target_only, mut rng) = scheduler(&p, &ENABLED, 3);
    assert!(target_only.choose(normalize_focus(Some("none")), &mut rng).is_none());
    assert_eq!(target_only.choose(normalize_focus(Some("cursor")), &mut rng).unwrap().phase_id, Phase::Approach);
}

#[test]
fn test_focus_names_are_normalised() {
    assert_eq!(normalize_focus(Some("fly")), Focus::Prey);
    assert_eq!(normalize_focus(Some("fly")).as_str(), "prey");
    assert_eq!(normalize_focus(Some("another spider")), Focus::None);
    assert_eq!(normalize_focus(Some("another spider")).as_str(), "none");
}

#[test]
fn test_unscored_phases_read_as_zero() {
    let p = personality();
    let scores = phase_scores_for(PersonalityRef::Def(&p));
    assert_eq!(scores.get("roll").copied(), Some(10.0));
    assert_eq!(scores.get("wander").copied(), Some(0.0));
}

#[test]
fn test_a_creature_actually_follows_its_scheduler() {
    let runtime_personality = json!({
        "id": "phase-runtime",
        "speed_multiplier": 1.0,
        "reaction_radius": 80,
        "boldness": 0.5,
        "wander_frequency": 1.0,
        "idle_time": [0.0, 0.0],
        "phase_scores": scores_with(&[("wander", 10.0)]),
    });
    let mut cfg = CreatureConfig::new(json!({"base_size": 25, "legs": []}), runtime_personality, 800.0, 600.0);
    cfg.seed = Some(1);
    cfg.id = 1;
    let mut runtime = dc_creature::Creature::new(cfg);
    runtime.kin.x = 400.0;
    runtime.kin.y = 300.0;
    runtime.state_timer = 0.0;
    runtime.decision_timer = 0.0;
    common::step(&mut runtime, 1.0 / 60.0, 780.0, 580.0, &[]);
    assert_eq!(runtime.state, State::Wander);
    assert_eq!(runtime.phase_scheduler.current_phase_id(), Some(Phase::Wander));
}

fn schema_case(extra: Value, accepted: bool) {
    let mut data = schema_base();
    for (k, v) in extra.as_object().unwrap() {
        data.insert(k.clone(), v.clone());
    }
    let path = common::root().join("phase-test.json");
    let result = dc_content::discovery::validate_personality(&Value::Object(data), &path);
    assert_eq!(result.is_ok(), accepted, "{result:?}");
}

#[test]
fn test_the_personality_schema_holds_the_line_in_range() {
    schema_case(json!({"phase_scores": {"wander": 0, "chase": 10}}), true);
}

#[test]
fn test_the_personality_schema_holds_the_line_score_above_ten() {
    schema_case(json!({"phase_scores": {"wander": 11}}), false);
}

#[test]
fn test_the_personality_schema_holds_the_line_behaviours_are_gone() {
    schema_case(json!({"behaviours": ["weave_web"]}), false);
}
