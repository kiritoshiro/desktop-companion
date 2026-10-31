//! Port of `tests/test_regal_scout.py`: the scout's face axis. The painted
//! anatomy checks are `dc-paint`'s.

mod common;

use dc_creature::Creature;
use dc_creature::regal::{RegalExpression, RegalGesture};
use dc_creature::state::State;
use dc_creature::world::Population;

fn build(model: &str) -> Creature {
    common::spider_with(common::load_model(model), common::load_personality("curious"), 17, |cfg| {
        cfg.screen_w = 800.0;
        cfg.screen_h = 600.0;
    })
}

// not ported: test_painted_eyes_and_palps_face_forward (records QPainter ellipses; dc-paint owns drawing)
// not ported: test_both_models_remain_selectable (ConfigWindow dropdown; dc-ui)

#[test]
fn test_original_model_keeps_its_original_face_axis() {
    let original = build("regal_fluff");
    let mut scout = build("regal_scout");
    assert!(original.is_regal() && !original.regal_face_forward());
    assert!(scout.is_regal() && scout.regal_face_forward());
    scout.state = State::Idle;
    scout.kin.current_speed = 0.0;
    scout.startled_timer = 0.0;
    scout.regal = Some(RegalExpression { cooldown: 0.0, ..Default::default() });
    let (mx, my) = (scout.kin.x + 50.0, scout.kin.y + 50.0);
    scout.update_regal(0.04, mx, my, &Population::new());
    let gesture = scout.regal.unwrap().gesture;
    assert!(matches!(gesture, RegalGesture::Question | RegalGesture::Peek | RegalGesture::Reach), "{gesture:?}");
}
