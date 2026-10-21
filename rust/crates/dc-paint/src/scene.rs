//! Port of `CreatureManager.render` (`manager/core.py`): the layer order of
//! one overlay frame.
//!
//! cages → bases → webs → mouse webs → flies → carcasses → creatures (in
//! render order, skipped outside the clip, skipped when fully covered,
//! clipped against occluding windows by region subtraction) → damage
//! numbers (carcasses', then every creature's).

use crate::creature::CreaturePainter;
use crate::draw::CreatureDraw;
use crate::look::CreatureLook;
use crate::world::{
    BaseDraw, CageDraw, CarcassDraw, Clip, FlyWorldDraw, MouseWebDraw, WebDraw, draw_cage, draw_carcass, render_bases,
    render_flies, render_mouse_webs, render_webs,
};
use dc_core::Rect;
use dc_render::{Canvas, ClipOp, Region};

/// One creature in render order, with its window occlusion resolved by the
/// manager (`_occluding_surfaces_for_creature`,
/// `_creature_fully_covered_by_any_surface`, `_surface_visible_qregion`).
pub struct CreatureEntry<'a> {
    pub draw: &'a CreatureDraw,
    pub look: &'a CreatureLook,
    /// `_creature_fully_covered_by_any_surface(...)`: not drawn at all.
    pub fully_covered: bool,
    /// The visible region of each occluding surface (its rect minus the
    /// surfaces above it), already integer-snapped as in Python. Empty
    /// means no occluders.
    pub hidden_regions: Vec<Region>,
}

/// Everything `CreatureManager.render` draws in one frame.
pub struct SceneDraw<'a> {
    pub screen_w: f64,
    pub screen_h: f64,
    pub always_show_names: bool,
    /// `painter.clipBoundingRect()` grown by 2 px, as `(x0, y0, x1, y1)`;
    /// `None` for a full repaint.
    pub clip: Option<Clip>,
    pub cages: &'a [CageDraw],
    pub bases: &'a [BaseDraw],
    pub webs: &'a [WebDraw],
    pub mouse_webs: &'a MouseWebDraw,
    pub flies: &'a FlyWorldDraw,
    pub carcasses: &'a [CarcassDraw],
    /// Already in `_render_order` (sorted by `y`).
    pub creatures: &'a [CreatureEntry<'a>],
}

/// `_refresh_render_order()`: creature indices sorted by `y` (stable).
pub fn render_order(creatures: &[CreatureDraw]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..creatures.len()).collect();
    order.sort_by(|&a, &b| creatures[a].y.partial_cmp(&creatures[b].y).unwrap_or(std::cmp::Ordering::Equal));
    order
}

/// `CreatureManager.render(painter)`.
pub fn render_scene(p: &mut Canvas, painter: &mut CreaturePainter, s: &SceneDraw) {
    for cage in s.cages {
        draw_cage(p, cage);
    }
    let clip = s.clip;
    render_bases(p, s.bases, clip);
    render_webs(p, s.webs, clip);
    render_mouse_webs(p, s.mouse_webs, clip);
    render_flies(p, s.flies, clip);
    for carcass in s.carcasses {
        draw_carcass(p, carcass, false);
    }
    for entry in s.creatures {
        if let Some(c) = clip {
            let (x0, y0, x1, y1) = entry.draw.bounding_rect(s.always_show_names);
            if x1 < c.0 || x0 > c.2 || y1 < c.1 || y0 > c.3 {
                continue;
            }
        }
        if entry.fully_covered {
            continue;
        }
        if entry.hidden_regions.is_empty() {
            painter.render(p, entry.draw, entry.look, s.always_show_names, false);
            continue;
        }
        let mut visible = Region::from_rect(Rect::new(0.0, 0.0, s.screen_w.trunc(), s.screen_h.trunc()));
        for hidden in &entry.hidden_regions {
            if !hidden.is_empty() {
                visible = visible.subtracted(hidden);
            }
        }
        p.save();
        p.set_clip_region_op(&visible, ClipOp::Intersect);
        painter.render(p, entry.draw, entry.look, s.always_show_names, false);
        p.restore();
    }
    for carcass in s.carcasses {
        carcass.draw_damage_numbers(p);
    }
    for entry in s.creatures {
        entry.draw.render_damage_numbers(p);
    }
}

/// `_surface_visible_qregion(surface)`: the integer-snapped rect of a
/// surface minus every surface above it. Rects are `(x, y, w, h)`.
pub fn surface_visible_region(surface: (f64, f64, f64, f64), higher: &[(f64, f64, f64, f64)]) -> Region {
    let snap = |(x, y, w, h): (f64, f64, f64, f64)| {
        Rect::new(x.floor(), y.floor(), (w.ceil() as i64).max(1) as f64, (h.ceil() as i64).max(1) as f64)
    };
    let mut region = Region::from_rect(snap(surface));
    for &h in higher {
        region.subtract_rect(&snap(h));
    }
    region
}
