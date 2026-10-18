//! Port of `Creature.render` (`creature/core.py`): camouflage fade, roll
//! spin, the body painter for the model's render mode, the web net, the
//! name label and the damage numbers.

use crate::PaintSwitches;
use crate::draw::CreatureDraw;
use crate::look::{CreatureLook, RenderMode};
use crate::procedural::{Paint, render_procedural};
use crate::sprite::{SpriteCache, render_sprite_rig};
use dc_render::Canvas;

/// Paints creatures. Owns the sprite cache (`Creature.SPRITE_CACHE`, a
/// class attribute in Python), so keep one per overlay.
#[derive(Default)]
pub struct CreaturePainter {
    pub switches: PaintSwitches,
    pub sprites: SpriteCache,
}

impl CreaturePainter {
    pub fn new(switches: PaintSwitches) -> Self {
        CreaturePainter { switches, sprites: SpriteCache::new() }
    }

    /// The body alone (`_render_regal` / `_render_sprite_rig` /
    /// `_render_procedural`), with no camouflage, roll, net or label.
    pub fn render_body(&mut self, p: &mut Canvas, d: &CreatureDraw, look: &CreatureLook) {
        let cx = Paint { d, look, sw: &self.switches };
        match look.render_mode {
            RenderMode::Regal => crate::regal::render_regal(p, &cx),
            RenderMode::SpriteRig => {
                let assets = match look.assets_dir() {
                    Some(dir) => {
                        let overrides: Vec<(String, [f64; 3])> = d
                            .palette
                            .color_overrides
                            .iter()
                            .filter_map(|k| d.palette.colors.get(k).map(|v| (k.clone(), *v)))
                            .collect();
                        Some(self.sprites.load(&dir, &overrides))
                    }
                    None => None,
                };
                let drawn = assets.as_deref().is_some_and(|a| render_sprite_rig(p, &cx, a));
                if !drawn {
                    render_procedural(p, &cx);
                }
            }
            RenderMode::Procedural => render_procedural(p, &cx),
        }
    }

    /// `render(painter, always_show_names, numbers)`.
    pub fn render(
        &mut self,
        p: &mut Canvas,
        d: &CreatureDraw,
        look: &CreatureLook,
        always_show_names: bool,
        numbers: bool,
    ) {
        let camouflage_opacity = d.camouflage_opacity();
        let camouflage_saved = camouflage_opacity < 0.999;
        if camouflage_saved {
            p.save();
            let o = p.opacity();
            p.set_opacity(o * camouflage_opacity);
        }
        let rolling = d.roll_spin.abs() > 1e-4;
        if rolling {
            p.save();
            p.translate(d.x, d.y);
            p.rotate(d.roll_spin.to_degrees());
            p.translate(-d.x, -d.y);
        }
        self.render_body(p, d, look);
        if rolling {
            p.restore();
        }
        crate::web_net::draw_web_net(p, d);
        crate::label::draw_name_label(p, d, always_show_names);
        if numbers {
            d.draw_damage_numbers(p);
        }
        if camouflage_saved {
            p.restore();
        }
    }
}
