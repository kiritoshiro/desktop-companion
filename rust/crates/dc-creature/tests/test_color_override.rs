//! Port of `tests/test_color_override.py`: a preset may recolour a model,
//! without reaching into the model itself.

mod common;

use dc_content::preset_io::validate_preset;
use serde_json::{Value, json};

fn palette() -> Value {
    json!({"body": [220, 40, 80], "leg_band": [40, 200, 160]})
}

fn personality() -> Value {
    json!({
        "id": "curious",
        "speed_multiplier": 1.0,
        "reaction_radius": 120,
        "boldness": 0.5,
        "wander_frequency": 0.0,
    })
}

fn preset(colors: Option<Value>) -> Value {
    let mut slot = json!({"model": "tarantula", "personality": "curious", "count": 1});
    if let Some(colors) = colors {
        slot["colors"] = colors;
    }
    json!({"name": "Color smoke", "slots": [slot]})
}

#[test]
fn test_a_slot_may_omit_colors_entirely() {
    validate_preset(&preset(None)).unwrap();
}

#[test]
fn test_a_well_formed_palette_is_accepted() {
    validate_preset(&preset(Some(palette()))).unwrap();
}

#[test]
fn test_a_malformed_palette_is_rejected() {
    for (id, colors) in [
        ("too-few-channels", json!({"body": [1, 2]})),
        ("a-bool-is-not-a-channel", json!({"body": [1, true, 3]})),
        ("out-of-range", json!({"body": [1, 2, 300]})),
    ] {
        assert!(validate_preset(&preset(Some(colors))).is_err(), "{id}");
    }
}

#[test]
fn test_an_override_reaches_the_creature_without_being_shared() {
    let model = common::load_model("tarantula");
    let original = model["colors"].clone();
    let build = |seed| {
        common::spider_with(model.clone(), personality(), seed, |cfg| {
            cfg.screen_w = 800.0;
            cfg.screen_h = 600.0;
            cfg.color_overrides = Some(palette());
        })
    };
    let mut first = build(1);
    let second = build(2);

    assert_eq!(first.colors.get("body"), Some(&[220, 40, 80]), "override did not reach Creature.colors");
    assert_eq!(first.colors.get("leg_band"), Some(&[40, 200, 160]));
    assert_eq!(first.model["colors"], original, "color override mutated shared model metadata");

    // Two creatures from the same palette must not share the list behind it.
    first.colors.get_mut("body").unwrap()[0] = 0;
    assert_eq!(second.colors.get("body").unwrap()[0], 220, "creatures share a mutable palette");
}
