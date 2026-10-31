//! Port of `tests/test_progression.py`: XP, growth, equipment, resources and
//! relations, headless.
//!
//! Python seeds the module `random` and builds unseeded creatures; here the
//! same seed goes to seeded creatures.

mod common;

use dc_creature::Creature;
use dc_creature::progression::Relation;
use dc_state::progression::{MAX_LEVEL, ProgressionState, xp_to_next_level};

/// `pair()`: two spiders from the same model, with distinct progression identities.
fn pair() -> (Creature, Creature) {
    let build = |index: i64| {
        common::spider_with(common::load_model("tarantula"), common::load_personality("mellow"), 7, |cfg| {
            cfg.screen_w = 1280.0;
            cfg.screen_h = 720.0;
            cfg.index = index;
            cfg.progression_id = Some(format!("smoke:{index}"));
            cfg.id = 100 + index as u64;
        })
    };
    (build(0), build(1))
}

fn spider() -> Creature {
    pair().0
}

/// `sum(xp_to_next_level(level) for level in range(1, 5)) + 1`
fn four_levels() -> f64 {
    ((1..5).map(xp_to_next_level).sum::<i64>() + 1) as f64
}

#[test]
fn test_a_new_spider_starts_at_the_bottom() {
    let s = spider();
    assert!(s.level() == 1 && s.xp() == 0);
}

#[test]
fn test_a_level_arrives_exactly_at_the_threshold() {
    let mut s = spider();
    assert!(s.gain_experience((xp_to_next_level(1) - 1) as f64).is_empty());
    assert_eq!(s.level(), 1);
    assert!(!s.gain_experience(1.0).is_empty());
    assert_eq!(s.level(), 2);
}

#[test]
fn test_growth_follows_levels() {
    let mut s = spider();
    let old_size = s.size();
    let old_speed = s.speed_mult();
    s.gain_experience(four_levels());
    assert!(s.size() > old_size && s.speed_mult() > old_speed);
}

#[test]
fn test_one_award_may_cross_several_thresholds_but_not_the_ceiling() {
    let mut s = spider();
    let events = s.gain_experience(four_levels());
    assert!(s.level() >= 4 && !events.is_empty());
    assert!(s.level() <= MAX_LEVEL);
}

#[test]
fn test_a_talent_costs_a_point_and_raises_a_stat() {
    let mut s = spider();
    let before_hp = s.max_hp;
    let before_points = s.progression.skill_points;
    s.gain_experience(four_levels());

    assert!(s.progression.unlocked_abilities.iter().any(|a| a == "vitality"));
    assert!(s.max_hp > before_hp);
    assert!(s.progression.skill_points < before_points + 4);

    let (ok, message) = s.unlock_progression_ability("vitality");
    assert!(!ok);
    assert!(message.to_lowercase().contains("already"), "{message}");
}

#[test]
fn test_worn_equipment_absorbs_damage() {
    let mut s = spider();
    assert!(s.add_inventory_item("fluffy_mantle"));
    let (ok, _) = s.equip_item("fluffy_mantle");
    assert!(ok && s.armor > 0.0);
    let hp_before = s.hp;
    assert!(s.take_damage(20.0, None) < 20.0 && s.hp < hp_before);
}

#[test]
fn test_energy_stays_within_its_bounds() {
    let mut s = spider();
    assert!(s.spend_energy(10.0));
    assert!(0.0 <= s.energy && s.energy <= s.max_energy);
}

#[test]
fn test_a_team_decides_the_default_relation() {
    for (their_team, expected) in [("pack", "friend"), ("pack_b", "neutral"), ("rivals", "foe")] {
        let (mut s, mut other) = pair();
        s.set_team("pack");
        other.set_team(their_team);
        assert_eq!(s.relation_to(&other.view()), Relation::parse(expected), "{their_team}");
        assert_eq!(s.relation_to(&other.view()).as_str(), expected);
    }
}

#[test]
fn test_a_pair_choice_outranks_the_team() {
    for over in ["friend", "foe"] {
        let (mut s, mut other) = pair();
        s.set_team("pack");
        other.set_team("rivals");
        s.progression.relation_overrides.insert(other.progression_id.clone(), over.to_string());
        assert_eq!(s.relation_to(&other.view()).as_str(), over);
    }
}

#[test]
fn test_progression_survives_a_round_trip() {
    let s = spider();
    let state = ProgressionState::from_value(&s.progression.to_value());
    assert_eq!(state.to_value(), s.progression.to_value());
}

#[test]
fn test_experience_stops_at_the_last_level() {
    let mut s = spider();
    s.progression.level = MAX_LEVEL;
    s.gain_experience(999999.0);
    assert!(s.level() == MAX_LEVEL && s.xp() == xp_to_next_level(MAX_LEVEL));
}
