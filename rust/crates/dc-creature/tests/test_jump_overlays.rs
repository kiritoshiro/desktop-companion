//! Port of `tests/test_jump_overlays.py` (DC-82): nothing is left on the
//! ground when a spider jumps.
//!
//! Every Python test here renders with QPainter; the simulation half they all
//! stand on is the `_airborne` fixture -- a hand-placed hop that reaches its
//! apex -- and that precondition is what is ported.
//!
//! Python seeds the module `random` and builds an unseeded creature; here the
//! same seed goes to a seeded creature. `spider.has_skill = lambda s: True`
//! becomes enabling the one skill the hop asks for.
//!
//! not ported: test_the_leg_roots_rise_with_the_legs (render plus a
//! monkeypatched `_sprite_leg_chain_points`; dc-paint).
//! not ported: test_nothing_solid_is_left_under_a_jumping_tarantula (QPainter
//! pixels; dc-paint).
//! not ported: test_armour_rises_with_the_body (render plus a monkeypatched
//! `equipped_items`; dc-paint).

mod common;

use dc_creature::creature::{After, JumpKind};
use dc_creature::{Creature, State};

/// `discover_personalities(ROOT)[0]["balanced"]`: a built-in temperament
/// with no file of its own.
fn balanced() -> serde_json::Value {
    let (personalities, _warnings) = dc_content::discovery::discover_personalities(Some(&common::root()));
    let balanced = personalities.get("balanced").expect("the balanced temperament was not discovered");
    serde_json::to_value(balanced).unwrap()
}

/// `_airborne(content)`: a spider hand-placed at the top of a hop.
fn airborne() -> Creature {
    let mut spider = common::spider_with(common::load_model("tarantula"), balanced(), 3, |cfg| {
        cfg.screen_w = 800.0;
        cfg.screen_h = 600.0;
    });
    spider.kin.x = 400.0;
    spider.kin.y = 300.0;
    spider.skills.set_enabled("jump", true);
    let size = spider.kin.size;
    spider.launch_jump(405.0, 300.0, JumpKind::Hop, After::Outcome, Some(size * 1.2), Some(0.5), 1.0);
    assert!(spider.airborne, "the hop never left the ground");
    while spider.jump_t < 0.5 {
        spider.update_jump(1.0 / 60.0);
    }
    spider
}

#[test]
fn test_a_hop_reaches_its_apex() {
    let spider = airborne();
    assert!(
        spider.jump_z > spider.kin.size * 0.9,
        "the apex was {:.1}px high for a {:.1}px spider",
        spider.jump_z,
        spider.kin.size
    );
    assert!(spider.airborne);
    assert_eq!(spider.state, State::Jump);
}
