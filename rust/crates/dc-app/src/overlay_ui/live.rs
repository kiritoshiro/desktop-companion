//! Live spiders in the windows: the inspector's view of one creature
//! (`CreatureInspectorDialog.refresh` in `app/inspector_ui.py`), its
//! portrait (`portrait(creature)`, painted by dc-paint on the CPU), and
//! armour pictures (dc-paint's `armour_icon`) as egui textures.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dc_creature::Creature;
use dc_manager::CreatureManager;
use dc_paint::{CreatureLook, CreaturePainter, PaintSwitches};
use dc_render::{CpuRenderer, Image, RenderHint, Scene};
use dc_ui::model::{ArmourIconFn, ThumbnailFn};
use dc_ui::windows::inspector::{InspectorView, OtherSpider};
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};

/// `inspector_ui.PORTRAIT`.
pub const PORTRAIT: u32 = 104;

/// `round(x, 2)`, as `progression_snapshot` rounded.
fn round2(x: f64) -> f32 {
    dc_content::util::py_round_n(x, 2) as f32
}

/// `refresh()`: the spider whose progression id is `key`, as it is now. A
/// spider no longer in the manager comes back with `alive: false`, which
/// closes the window.
pub fn inspector_view(manager: &CreatureManager, key: &str) -> InspectorView {
    let Some(c) = manager.creatures.iter().find(|c| c.progression_id == key) else {
        return InspectorView { key: key.to_owned(), alive: false, ..Default::default() };
    };
    let model_name = ["display_name", "name"]
        .iter()
        .filter_map(|k| c.model.get(*k).and_then(|v| v.as_str()))
        .find(|s| !s.is_empty())
        .unwrap_or("Spider")
        .to_owned();
    let hp = round2(c.hp);
    let max_hp = round2(c.max_hp);
    let fraction = (hp as f64 / (max_hp as f64).max(1e-6)).clamp(0.0, 1.0);
    let [r, g, b, a] = Creature::health_bar_color(fraction);
    let others = manager
        .creatures
        .iter()
        .filter(|o| o.id != c.id)
        .map(|o| OtherSpider {
            key: o.progression_id.clone(),
            name: o.display_name(),
            relation: c.relation_to(&o.view()).as_str().to_owned(),
        })
        .collect();
    InspectorView {
        key: key.to_owned(),
        display_name: c.display_name(),
        model_name,
        job_name: c.job_label().to_owned(),
        hp,
        max_hp,
        energy: round2(c.energy),
        max_energy: round2(c.max_energy),
        armor: round2(c.armor),
        damage: round2(c.damage),
        progression: (&c.progression).into(),
        level_pinned: c.level_label_pinned(),
        health_pinned: c.health_label_pinned(),
        health_colour: Some(Color32::from_rgba_unmultiplied(r, g, b, a)),
        others,
        team_profiles: dc_ui::bridge::team_profiles(&manager.team_profiles),
        alive: true,
    }
}

/// Paints portraits; keeps dc-paint's sprite cache and each model's look.
pub struct PortraitPainter {
    painter: CreaturePainter,
    looks: HashMap<String, CreatureLook>,
    renderer: CpuRenderer,
}

impl Default for PortraitPainter {
    fn default() -> Self {
        Self::new()
    }
}

impl PortraitPainter {
    pub fn new() -> Self {
        Self {
            painter: CreaturePainter::new(PaintSwitches::from_env()),
            looks: HashMap::new(),
            renderer: CpuRenderer::new(),
        }
    }

    /// `portrait(creature, size)`: the body and legs as it stands now,
    /// fitted to a `2 * size` square (Qt drew it at device pixel ratio 2),
    /// without its name, bars or damage numbers.
    pub fn portrait(&mut self, c: &Creature, size: u32) -> Image {
        let model_id = c.model.get("id").and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let look = self
            .looks
            .entry(model_id)
            .or_insert_with(|| CreatureLook::from_model_json(&c.model, dc_creature::creature::CURVED_LEGS));
        let draw = dc_manager::paint::creature_draw(c, look);
        let xs: Vec<f64> = std::iter::once(draw.x).chain(draw.legs.iter().map(|l| l.leg_foot_x)).collect();
        let ys: Vec<f64> = std::iter::once(draw.y).chain(draw.legs.iter().map(|l| l.leg_foot_y)).collect();
        let (min_x, max_x) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
        let (min_y, max_y) = ys.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
        let pad = draw.size * 0.9;
        let span = (max_x - min_x).max(max_y - min_y) + pad * 2.0;
        let side = size as f64 * 2.0;
        let scale = (side * 0.9) / span.max(1.0);
        let (cx, cy) = ((max_x + min_x) / 2.0, (max_y + min_y) / 2.0);
        let mut scene = Scene::new();
        {
            let mut canvas = scene.canvas();
            canvas.set_render_hint(RenderHint::Antialiasing, true);
            canvas.translate(size as f64, size as f64);
            canvas.scale(scale, scale);
            canvas.translate(-cx, -cy);
            self.painter.render_body(&mut canvas, &draw, look);
        }
        self.renderer.render(&scene, size * 2, size * 2, None)
    }
}

/// A dc-render image as an egui image.
pub fn color_image(image: &Image) -> ColorImage {
    let (w, h) = image.size();
    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &image.to_straight_rgba())
}

/// The newest portrait of one spider, uploaded to a window's texture on
/// first use.
#[derive(Default)]
pub struct PortraitSlot {
    image: Option<ColorImage>,
    dirty: bool,
    handle: Option<TextureHandle>,
}

impl PortraitSlot {
    pub fn set(&mut self, image: ColorImage) {
        self.image = Some(image);
        self.dirty = true;
    }

    pub fn has_image(&self) -> bool {
        self.image.is_some()
    }
}

/// The inspector's [`ThumbnailFn`]: the slot's image as a texture of `ctx`.
pub fn portrait_fn(ctx: &egui::Context, slot: Rc<RefCell<PortraitSlot>>) -> ThumbnailFn {
    let ctx = ctx.clone();
    Box::new(move |_key, _size| {
        let mut slot = slot.borrow_mut();
        if slot.dirty {
            slot.dirty = false;
            if let Some(image) = slot.image.clone() {
                match &mut slot.handle {
                    Some(handle) => handle.set(image, TextureOptions::LINEAR),
                    None => slot.handle = Some(ctx.load_texture("overlay/portrait", image, TextureOptions::LINEAR)),
                }
            }
        }
        slot.handle.clone()
    })
}

/// An [`ArmourIconFn`] painting `armour_icon(item, slot, size)` once per
/// piece and size into textures of `ctx`.
pub fn armour_icon_fn(ctx: &egui::Context) -> ArmourIconFn {
    let ctx = ctx.clone();
    let cache: RefCell<HashMap<(String, String, u32), TextureHandle>> = RefCell::new(HashMap::new());
    Box::new(move |item, slot, size| {
        let key = (item.to_owned(), slot.to_owned(), size);
        if let Some(found) = cache.borrow().get(&key) {
            return Some(found.clone());
        }
        let image = dc_paint::armour_art::armour_icon(item, slot, size.max(1));
        let handle =
            ctx.load_texture(format!("overlay/armour/{item}/{size}"), color_image(&image), TextureOptions::LINEAR);
        cache.borrow_mut().insert(key, handle.clone());
        Some(handle)
    })
}
