//! Port of `tests/test_situational_temperament.py`: one temperament, many
//! situations (DC-60 to DC-63).
//!
//! Python built unseeded creatures from the discovered content; here each is
//! seeded (distinct seeds give distinct ids) from the same discovered
//! personality and the resolved model.

mod common;

use dc_content::OrderedMap;
use dc_content::discovery::discover_personalities;
use dc_content::personality::Personality;
use dc_content::personality_profiles::{PersonalityRef, phase_score_profile, phase_scores_for};
use dc_creature::constants::{CURSOR_PRESSURE_PER_GRAB, CURSOR_WARY_THRESHOLD, gait_by_state};
use dc_creature::phase_scheduler::{Focus, Phase, normalize_focus};
use dc_creature::world::{CreatureView, Population};
use dc_creature::{Creature, State};
use dc_kinematics::GaitStyle;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;
const FAR: f64 = -9000.0;

fn personalities() -> OrderedMap<Personality> {
    discover_personalities(Some(&common::root())).0
}

/// `_spider(content, personality_id, gait_style=...)` on an 800x600 screen.
fn spider_as(personality_id: &str, seed: i64, gait_style: Option<&str>) -> Creature {
    let personality = personalities().get(personality_id).unwrap_or_else(|| panic!("{personality_id}")).to_value();
    let gait = gait_style.map(str::to_string);
    common::spider_with(common::load_model("tarantula"), personality, seed, move |cfg| {
        cfg.screen_w = 800.0;
        cfg.screen_h = 600.0;
        if let Some(g) = gait {
            cfg.gait_style = g;
        }
    })
}

fn spider(seed: i64) -> Creature {
    spider_as("balanced", seed, None)
}

fn pop_of(views: &[CreatureView]) -> Population {
    Population::from_views(views.iter().cloned())
}

/// One `update(1/60, mx, my, 800, 600)` with the creature alone.
fn tick(c: &mut Creature, mx: f64, my: f64) {
    common::step(c, DT, mx, my, &[]);
}

fn effective_gait(c: &Creature) -> GaitStyle {
    c.kin.effective_gait_style(&c.kctx(None))
}

// ------------------------------------------------------ DC-61: friend and foe

#[test]
fn test_a_foe_is_its_own_situation() {
    // `"foe" in FOCUS_PHASE_BIAS` / `"friend" in ...`: both are rows of the table.
    assert_eq!(Focus::Foe.as_str(), "foe");
    assert_eq!(Focus::Friend.as_str(), "friend");
    assert_eq!(Focus::Foe.bias_row().len(), 12);
    assert_eq!(Focus::Friend.bias_row().len(), 12);
    assert_eq!(normalize_focus(Some("enemy")), Focus::Foe);
    assert_eq!(normalize_focus(Some("ally")), Focus::Friend);
}

#[test]
fn test_nobody_cuddles_a_foe() {
    assert_eq!(Focus::Foe.bias(Phase::Cuddle), 0.0);
    assert_eq!(Focus::Foe.bias(Phase::SocialPlay), 0.0);
    assert!(Focus::Friend.bias(Phase::Cuddle) > 1.0);
}

#[test]
fn test_a_foe_pulls_towards_fighting_and_a_friend_away_from_it() {
    let (foe, friend) = (Focus::Foe, Focus::Friend);
    assert!(foe.bias(Phase::Chase) > friend.bias(Phase::Chase));
    assert!(foe.bias(Phase::PrepareJumpAttack) > friend.bias(Phase::PrepareJumpAttack));
    assert!(foe.bias(Phase::RunAway) > friend.bias(Phase::RunAway));
    assert!(friend.bias(Phase::SocialPlay) > foe.bias(Phase::SocialPlay));
}

#[test]
fn test_a_spider_with_a_foe_reports_that_situation() {
    let mut me = spider(1);
    let mut them = spider(2);
    me.kin.x = 300.0;
    me.kin.y = 300.0;
    them.kin.x = 340.0;
    them.kin.y = 300.0;
    let pop = pop_of(&[me.view(), them.view()]);
    assert_eq!(me.phase_focus_context(FAR, FAR, &pop), Focus::None);
    me.foe = Some(them.id);
    assert_eq!(me.phase_focus_context(FAR, FAR, &pop), Focus::Foe);
}

#[test]
fn test_a_dead_foe_is_not_a_situation() {
    let mut me = spider(1);
    let mut them = spider(2);
    me.foe = Some(them.id);
    them.dead = true;
    let pop = pop_of(&[me.view(), them.view()]);
    assert_ne!(me.phase_focus_context(FAR, FAR, &pop), Focus::Foe);
}

#[test]
fn test_the_foe_situation_hands_back_no_mate() {
    let mut me = spider(1);
    let mut them = spider(2);
    me.kin.x = 300.0;
    me.kin.y = 300.0;
    them.kin.x = 380.0;
    them.kin.y = 300.0;
    me.foe = Some(them.id);
    let pop = pop_of(&[me.view(), them.view()]);
    let (tx, ty, mate) = me.phase_target(normalize_focus(Some("foe")), FAR, FAR, &pop);
    assert_eq!((tx, ty), (them.kin.x, them.kin.y));
    assert_eq!(mate, None);
}

#[test]
fn test_the_stance_between_teams_decides_friend_or_foe() {
    let mut me = spider(1);
    let mut them = spider(2);
    me.set_team("hunters");
    them.set_team("hunters");
    assert_eq!(me.social_focus_for(&them.view()), Focus::Friend);
    them.set_team("rivals");
    let stances = dc_state::progression::normalize_team_stances(Some(&json!({"hunters": {"rivals": "foe"}})));
    me.team_stances = Some(Arc::new(stances));
    them.team_stances = me.team_stances.clone();
    assert_eq!(me.social_focus_for(&them.view()), Focus::Foe);
}

// --------------------------------------------------- DC-62: the mouse remembers

#[test]
fn test_being_grabbed_leaves_a_mark() {
    let mut s = spider(1);
    assert_eq!(s.cursor_pressure, 0.0);
    assert!(!s.wary_of_cursor());
    s.start_drag(0.0, 0.0);
    assert!((s.cursor_pressure - CURSOR_PRESSURE_PER_GRAB).abs() < 1e-9, "{}", s.cursor_pressure);
}

#[test]
fn test_a_few_grabs_make_a_spider_wary() {
    let mut s = spider(1);
    let mut grabs = 0;
    while !s.wary_of_cursor() && grabs < 10 {
        s.start_drag(0.0, 0.0);
        s.dragging = false;
        grabs += 1;
    }
    assert!(1 < grabs && grabs <= 3, "{grabs}");
    assert!(s.cursor_pressure >= CURSOR_WARY_THRESHOLD);
}

#[test]
fn test_a_wary_spider_sees_the_pointer_differently() {
    let mut s = spider(1);
    s.kin.x = 400.0;
    s.kin.y = 300.0;
    let near = (s.kin.x + 40.0, s.kin.y);
    let pop = pop_of(&[s.view()]);
    assert_eq!(s.phase_focus_context(near.0, near.1, &pop), Focus::Cursor);
    s.cursor_pressure = 1.0;
    assert_eq!(s.phase_focus_context(near.0, near.1, &pop), Focus::CursorWary);
}

#[test]
fn test_wary_means_run_more_and_approach_less() {
    let (calm, wary) = (Focus::Cursor, Focus::CursorWary);
    assert!(wary.bias(Phase::RunAway) > calm.bias(Phase::RunAway) * 3.0);
    assert!(wary.bias(Phase::Approach) < calm.bias(Phase::Approach));
    assert!(wary.bias(Phase::Chase) < calm.bias(Phase::Chase));
    assert!(wary.bias(Phase::Observe) > calm.bias(Phase::Observe));
}

#[test]
fn test_a_spider_left_alone_forgives() {
    let mut s = spider(1);
    s.cursor_pressure = 1.0;
    for _ in 0..(200 * 60) {
        tick(&mut s, FAR, FAR);
    }
    assert_eq!(s.cursor_pressure, 0.0);
    assert!(!s.wary_of_cursor());
}

#[test]
fn test_it_does_not_forgive_while_still_being_held() {
    let mut s = spider(1);
    s.start_drag(0.0, 0.0);
    let before = s.cursor_pressure;
    for _ in 0..(30 * 60) {
        tick(&mut s, 0.0, 0.0);
    }
    assert!((s.cursor_pressure - before).abs() < 1e-9, "{} vs {before}", s.cursor_pressure);
}

// -------------------------------------------------- DC-63: the movement pipeline

#[test]
fn test_the_pipeline_runs_most_specific_last() {
    let mut s = spider_as("balanced", 1, Some("lively"));
    s.state = State::Wander;
    s.kin.gait_spell = None;
    assert_eq!(effective_gait(&s), GaitStyle::Lively);
    s.kin.gait_spell = Some(GaitStyle::Skitter);
    assert_eq!(effective_gait(&s), GaitStyle::Skitter);
    s.state = State::Chase;
    assert_eq!(Some(effective_gait(&s)), gait_by_state(State::Chase.motion()));
}

#[test]
fn test_a_spell_actually_happens() {
    let mut s = spider_as("balanced", 1, Some("lively"));
    s.state = State::Idle;
    let mut seen = BTreeSet::new();
    for _ in 0..(120 * 60) {
        tick(&mut s, FAR, FAR);
        if gait_by_state(s.state.motion()).is_none() {
            seen.insert(effective_gait(&s).as_str());
        }
    }
    assert_eq!(seen, BTreeSet::from(["lively", "skitter"]));
}

/// Python checked `spider._gait_rng is not spider.rng` (and is a
/// `random.Random`). The port keeps them as separate fields, so this checks
/// the property that identity protected: drawing from one leaves the other's
/// stream where it was.
#[test]
fn test_the_spell_has_its_own_random_stream() {
    let mut s = spider(1);
    let mut gait_before = s.kin.gait_rng.clone();
    s.rng.random();
    s.rng.random();
    assert_eq!(s.kin.gait_rng.clone().random(), gait_before.random());
    let mut rng_before = s.rng.clone();
    s.kin.gait_rng.random();
    assert_eq!(s.rng.clone().random(), rng_before.random());
}

#[test]
fn test_classic_opts_out_of_the_whole_pipeline() {
    let mut s = spider_as("balanced", 1, Some("classic"));
    for _ in 0..(120 * 60) {
        tick(&mut s, FAR, FAR);
        assert_eq!(effective_gait(&s), GaitStyle::Classic);
        assert_eq!(s.kin.gait_spell, None);
    }
}

// ------------------------------------------- the point of keeping temperaments

#[test]
fn test_the_same_situation_moves_two_temperaments_in_opposite_directions() {
    let foe = Focus::Foe;
    let bold = phase_score_profile("bold").expect("bold profile");
    let timid = phase_score_profile("escape").expect("escape profile");
    let weight = |profile: &[(&str, f64)], phase: Phase| {
        profile.iter().find(|(k, _)| *k == phase.as_str()).map(|(_, v)| *v).unwrap_or(3.0) * foe.bias(phase)
    };
    assert!(weight(bold, Phase::Chase) > weight(bold, Phase::RunAway));
    assert!(weight(timid, Phase::RunAway) > weight(timid, Phase::Chase));
}

#[test]
fn test_balanced_is_the_default_and_does_a_bit_of_everything() {
    let found = personalities();
    let balanced = found.get("balanced").expect("balanced");
    let scores = phase_scores_for(PersonalityRef::Def(balanced));
    assert!(scores.values().all(|s| *s > 0.0), "{scores:?}");
    assert!(scores.values().copied().fold(f64::MIN, f64::max) < 9.0, "{scores:?}");
}
