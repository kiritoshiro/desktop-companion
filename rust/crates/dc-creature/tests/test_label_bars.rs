//! Port of `tests/test_label_bars.py` (DC-87): the bars under a spider's
//! label, and the palps' colour after a drop. Only the logic halves: which
//! bars show, how full they are and how much room they take. The pixel
//! widths and rows are `dc-paint`'s.
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature.

mod common;

use dc_creature::labels::LABEL_BAR_WIDTH;
use dc_creature::{ApproxLabelMetrics, Creature};
use dc_state::progression::xp_to_next_level;
use serde_json::json;

const DT: f64 = 1.0 / 60.0;
const AWAY: (f64, f64) = (-100000.0, -100000.0);

/// `build(seed)`: the mellow tarantula, `progression_id=f"bars:{seed}"`, settled for 30 frames.
fn build(seed: i64) -> Creature {
    let mut c = common::spider_with(common::load_model("tarantula"), common::load_personality("mellow"), seed, |cfg| {
        cfg.index = 0;
        cfg.progression_id = Some(format!("bars:{seed}"));
    });
    c.kin.x = 800.0;
    c.kin.y = 450.0;
    c.initialize_legs();
    common::run(&mut c, 30, AWAY.0, AWAY.1);
    c
}

#[test]
fn test_the_health_bar_is_the_same_width_whatever_the_name_or_health() {
    // Logic half: the bar is a fixed width and its layout does not depend on
    // the name or the health pool (the pixel width check is dc-paint's).
    const { assert!(LABEL_BAR_WIDTH >= 20.0) };
    let mut heights = Vec::new();
    for (name, max_hp) in [("Bo", None), ("Maximilian the Magnificent", None), ("Bo", Some(900.0))] {
        let mut c = build(9);
        c.set_name(name);
        if let Some(hp) = max_hp {
            c.max_hp = hp;
            c.hp = hp;
        }
        c.set_health_label_pinned(true);
        let spec = c.label_spec(true).expect("a pinned label shows");
        assert!(spec.health_pinned);
        assert_eq!(spec.health_fraction, 1.0);
        heights.push((spec.health_bar_height, spec.bars_height));
    }
    assert!(heights.windows(2).all(|w| w[0] == w[1]), "{heights:?}");
}

#[test]
fn test_xp_is_a_line_under_the_level_not_a_bar_under_the_name() {
    let mut c = build(9);
    c.set_name("Bo");
    c.set_health_label_pinned(true);
    let without_xp = c.label_bars_height();
    c.force_show_xp = true;
    let threshold = xp_to_next_level(c.level());

    c.progression.xp = (threshold as f64 * 0.9) as i64;
    let spec = c.label_spec(true).unwrap();
    assert!(spec.xp_pinned);
    assert!(spec.level_suffix.contains(&format!("Lv {}", c.level())), "{:?}", spec.level_suffix);
    assert_eq!(spec.bars_height, without_xp, "XP is still a bar under the name");
    let most = spec.xp_fraction;
    assert!(most >= 0.85, "{most}");

    c.progression.xp = (threshold as f64 * 0.2) as i64;
    let little = c.label_spec(true).unwrap().xp_fraction;
    assert!(little < most * 0.5, "the XP line did not follow the XP: {little} vs {most}");

    c.force_show_xp = false;
    assert!(!c.label_spec(true).unwrap().xp_pinned, "XP drawn while off");
}

#[test]
fn test_only_health_and_stamina_hang_under_the_label() {
    let mut c = build(9);
    c.set_name("Bo");
    c.set_level_label_pinned(true);
    c.set_health_label_pinned(true);
    let plain = c.bounding_rect(true, &ApproxLabelMetrics);
    c.force_show_xp = true;
    assert_eq!(c.bounding_rect(true, &ApproxLabelMetrics).1, plain.1, "XP made the label taller");
    c.force_show_stamina = true;
    assert!(c.bounding_rect(true, &ApproxLabelMetrics).1 < plain.1 - 2.0, "no room was made for stamina");
    c.energy = c.max_energy;
    let full = c.label_spec(true).unwrap();
    assert!(full.stamina_pinned && full.stamina_fraction == 1.0);
    c.energy = c.max_energy * 0.25;
    let low = c.label_spec(true).unwrap().stamina_fraction;
    assert!(low < full.stamina_fraction * 0.5, "{low}");
}

#[test]
fn test_the_switch_reaches_every_spider_and_is_saved() {
    // not ported: the CreatureManager half (needs dc-manager). The preset
    // validation half:
    let bad = json!({"name": "x", "slots": [{"model": "tarantula", "personality": "mellow", "count": 1}],
                     "settings": {"always_show_xp": "yes"}});
    assert!(dc_content::preset_io::validate_preset(&bad).is_err());
}

#[test]
fn test_the_palps_keep_their_colour_when_a_spider_is_dropped() {
    // The startle gate the legs and palps share; counting `_qcolor` calls in
    // a render is dc-paint's.
    let mut c = build(9);
    let (x, y) = (c.kin.x, c.kin.y);
    c.start_drag(x, y);
    for _ in 0..10 {
        let (x, y) = (c.kin.x + 2.0, c.kin.y);
        c.drag_to(DT, x, y);
    }
    let (x, y) = (c.kin.x, c.kin.y);
    c.release_drag(x, y);
    let startle = c.startle_amount();
    assert!(startle > 0.35, "the drop did not leave the spider startled");
    assert!(
        !c.startle_highlight_active(startle),
        "the legs would change colour on the drop too; this test is meant for the palps"
    );
}
