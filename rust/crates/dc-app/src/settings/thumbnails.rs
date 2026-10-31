//! `_model_icon`: the skin thumbnails in the settings window, painted with
//! the overlay's own creature renderer so every model shows its legs.
//! Without them the window drew its legless fallback icon.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::Path;

use dc_creature::Creature;
use dc_creature::creature::CreatureConfig;
use dc_ui::model::ThumbnailFn;
use egui::{TextureHandle, TextureOptions};
use serde_json::{Value, json};

use crate::overlay_ui::live::{PortraitPainter, color_image};

/// `MODEL_ICON_CANVAS`: the space the preview spider stands in.
const CANVAS: f64 = 96.0;
/// `Creature.BASE_SIZE`.
const BASE_SIZE: f64 = 28.0;

/// A [`ThumbnailFn`] painting each model once per size into textures of `ctx`.
pub fn model_thumbnail_fn(ctx: &egui::Context, root: Option<&Path>) -> ThumbnailFn {
    let (models, _) = dc_content::discovery::discover_models(root);
    let models: HashMap<String, Value> = models.iter().map(|(k, v)| (k.to_string(), v.to_value())).collect();
    let ctx = ctx.clone();
    let painter = RefCell::new(PortraitPainter::new());
    let cache: RefCell<HashMap<(String, u32), Option<TextureHandle>>> = RefCell::new(HashMap::new());
    Box::new(move |model_id, size| {
        let px = size.x.round().max(1.0) as u32;
        let key = (model_id.to_owned(), px);
        if let Some(found) = cache.borrow().get(&key) {
            return found.clone();
        }
        // A model that is gone or will not load keeps the fallback icon.
        let handle = models.get(model_id).map(|model| {
            let creature = preview_creature(model);
            // `portrait` paints a `2 * size` square, as Qt did at device pixel ratio 2.
            let image = painter.borrow_mut().portrait(&creature, px);
            ctx.load_texture(format!("settings/model/{model_id}/{px}"), color_image(&image), TextureOptions::LINEAR)
        });
        cache.borrow_mut().insert(key, handle.clone());
        handle
    })
}

/// The standing, still spider of `_model_icon`.
fn preview_creature(model: &Value) -> Creature {
    let personality = json!({
        "id": "preview",
        "display_name": "Preview",
        "speed_multiplier": 1.0,
        "reaction_radius": 120,
        "boldness": 0.5,
        "wander_frequency": 0.0,
        "idle_time": [1.0, 2.0],
        "move_time": [1.0, 2.0],
        "mood": model.get("default_personality").cloned().unwrap_or_else(|| json!("auto")),
    });
    let mut c = Creature::new(CreatureConfig::new(model.clone(), personality, CANVAS, CANVAS));
    c.kin.x = CANVAS * 0.47;
    c.kin.y = CANVAS * 0.50;
    let mut heading = 0.0;
    if c.is_regal() {
        // Present the raised face upright in the small skin swatch.
        heading = if c.regal_face_forward() { PI / 2.0 } else { -PI / 2.0 };
    }
    c.kin.heading = heading;
    c.kin.target_heading = heading;
    c.kin.current_speed = 0.0;
    c.kin.speed = 0.0;
    c.kin.body_bob = 0.0;
    c.kin.abdomen_pulse = 0.0;
    c.kin.ceph_pulse = 0.0;
    // Every model readable in the same icon space; not a size comparison.
    c.kin.size = BASE_SIZE * 0.75;
    c.initialize_legs();
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_model_paints_a_preview() {
        let (models, _) = dc_content::discovery::discover_models(None);
        assert!(!models.is_empty());
        let mut painter = PortraitPainter::new();
        for (id, def) in models.iter() {
            let c = preview_creature(&def.to_value());
            let image = painter.portrait(&c, 34);
            let rgba = image.to_straight_rgba();
            let opaque = rgba.chunks(4).filter(|p| p[3] > 0).count();
            // The legless fallback is never drawn here; a painted spider
            // covers a real share of its square.
            assert!(opaque > 68 * 68 / 20, "{id}: only {opaque} painted pixels");
        }
    }
}
