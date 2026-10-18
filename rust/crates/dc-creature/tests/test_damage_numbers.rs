//! Port of `tests/test_damage_numbers.py`: floating damage numbers record
//! how much health a blow took. Drawing them is `dc-paint`'s.

mod common;

use dc_creature::damage_numbers::{
    ALLY_DAMAGE_COLOR, DAMAGE_MERGE_SECONDS, DAMAGE_NUMBER_SECONDS, DAMAGE_RISE_PX, ENEMY_DAMAGE_COLOR, damage_text,
};
use dc_creature::labels::STAMINA_BAR_COLOR;
use dc_creature::{ApproxLabelMetrics, Creature};
use dc_world::carcass::{Carcass, DamageNumber, carcass_for};

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// `Creature(model, personality, 900, 700, index=0, seed=7)` at (450, 350) with 1000 hp.
fn spider() -> Creature {
    let mut s = common::spider_with(common::load_model("tarantula"), common::load_personality("mellow"), 7, |cfg| {
        cfg.screen_w = 900.0;
        cfg.screen_h = 700.0;
        cfg.index = 0;
    });
    s.kin.x = 450.0;
    s.kin.y = 350.0;
    s.max_hp = 1000.0;
    s.hp = 1000.0;
    s.armor = 0.0;
    s
}

/// `carcass_for(spider)`: the remains carry the body colour and the numbers.
fn remains_of(s: &Creature) -> Carcass {
    let body = s.colors.get("body").or_else(|| s.colors.get("abdomen")).map(|c| (c[0], c[1], c[2]));
    let numbers: Vec<DamageNumber> =
        s.damage_numbers.iter().map(|n| DamageNumber { amount: n.amount, age: n.age, nudge: n.nudge }).collect();
    carcass_for(s, body, &numbers)
}

#[test]
fn test_a_hit_shows_what_it_took_and_quick_hits_add_up() {
    let mut s = spider();
    let dealt = s.take_damage(12.0, None);
    assert!(!s.damage_numbers.is_empty() && approx(s.damage_numbers[0].amount, dealt));
    s.update_damage_numbers(DAMAGE_MERGE_SECONDS / 2.0);
    s.take_damage(5.0, None);
    assert!(s.damage_numbers.len() == 1 && approx(s.damage_numbers[0].amount, 17.0));
    s.update_damage_numbers(DAMAGE_MERGE_SECONDS);
    s.take_damage(3.0, None);
    assert_eq!(s.damage_numbers.len(), 2, "a later blow gets its own number");
    s.update_damage_numbers(DAMAGE_NUMBER_SECONDS);
    assert!(s.damage_numbers.is_empty(), "numbers fade out");
    assert_eq!(damage_text(12.4), "-12");
    assert_eq!(damage_text(0.3), "-0.3");
}

#[test]
fn test_the_repaint_area_covers_the_numbers() {
    let mut s = spider();
    s.take_damage(9.0, None);
    let bx = s.bounding_rect(false, &ApproxLabelMetrics);
    let top = s.damage_top();
    assert!(bx.1 <= top - DAMAGE_RISE_PX - 20.0, "the risen number is above the box");
    assert!(bx.2 >= s.kin.x + s.size() * 0.8 + 60.0, "the number's width is outside the box");
}

#[test]
fn test_the_killing_blow_shows_on_the_remains() {
    let mut s = spider();
    s.hp = 5.0;
    s.take_damage(40.0, None);
    assert!(s.dead);
    let mut remains = remains_of(&s);
    assert!(!remains.damage_numbers.is_empty() && approx(remains.damage_numbers[0].amount, 40.0));
    let plain = remains_of(&spider());
    assert!(remains.footprint().3 > plain.footprint().3, "its repaint area covers the number");
    remains.update(DAMAGE_NUMBER_SECONDS, 0);
    assert!(remains.damage_numbers.is_empty());
}

// not ported: test_a_fight_puts_numbers_over_the_spiders (needs CreatureManager)
// not ported: test_the_number_is_drawn_in_red (pixel count of a render; dc-paint)

#[test]
fn test_damage_to_your_side_is_red_and_to_an_enemy_ice_blue() {
    let (mut ally, mut enemy) = (spider(), spider());
    ally.progression.team_id = "adventurers".into();
    enemy.progression.team_id = "rivals".into();
    assert_eq!(ally.damage_color(), ALLY_DAMAGE_COLOR);
    assert_eq!(enemy.damage_color(), ENEMY_DAMAGE_COLOR);
    assert_ne!(ENEMY_DAMAGE_COLOR, ALLY_DAMAGE_COLOR);
    let lethal = enemy.hp + 50.0;
    enemy.take_damage(lethal, None);
    let [r, g, b] = ENEMY_DAMAGE_COLOR;
    assert_eq!(remains_of(&enemy).damage_color(), (r, g, b), "the remains keep the enemy's colour");
}

/// `QColor.hue()` / `QColor.saturation()` (HSV, 0..359 and 0..255).
fn hsv_hue_sat(rgb: [u8; 3]) -> (f64, f64) {
    let [r, g, b] = rgb.map(f64::from);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let delta = max - min;
    let sat = if max == 0.0 { 0.0 } else { 255.0 * delta / max };
    let hue = if delta == 0.0 {
        -1.0
    } else if max == r {
        (60.0 * (g - b) / delta).rem_euclid(360.0)
    } else if max == g {
        60.0 * (b - r) / delta + 120.0
    } else {
        60.0 * (r - g) / delta + 240.0
    };
    (hue, sat)
}

#[test]
fn test_stamina_is_yellow_everywhere() {
    // Only the creature's colour: `wood_theme.STAMINA` is `dc-ui`'s.
    let (hue, sat) = hsv_hue_sat(STAMINA_BAR_COLOR);
    assert!((40.0..=60.0).contains(&hue) && sat > 180.0, "{hue} {sat}");
}

// not ported: test_numbers_are_drawn_last_above_every_spider_and_name (needs CreatureManager render order; monkeypatching)
