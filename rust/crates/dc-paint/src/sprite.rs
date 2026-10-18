//! Port of `creature/render_sprite.py` (`_render_sprite_rig`) and the sprite
//! helpers of `render_procedural.py`: `_load_sprite_assets` (with the DC-48
//! palette-keyed tint cache), `_draw_sprite_segment` and `_draw_equipment`.

use crate::procedural::{Paint, draw_abdomen_marking, draw_antennae, draw_eyes, pen_cap, pen_full, rect};
use crate::sprite_tint::{palette_signature, tint_assets};
use dc_core::math::clamp;
use dc_core::Rect;
use dc_render::{Brush, Canvas, Cap, Image, Join, Pen, RenderHint};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

/// The parts `_load_sprite_assets` looks for.
pub const SPRITE_PARTS: [&str; 7] =
    ["abdomen", "cephalothorax", "leg_upper", "leg_lower", "leg_tip", "leg_knuckle", "shadow"];

/// One model's loaded (and, for an overridden palette, tinted) parts.
#[derive(Clone, Debug, Default)]
pub struct SpriteAssets {
    pub parts: HashMap<String, Image>,
}

impl SpriteAssets {
    pub fn get(&self, name: &str) -> Option<&Image> {
        self.parts.get(name)
    }
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// `Creature.SPRITE_CACHE`: keyed on the assets folder and the palette
/// signature, so differently coloured spiders of one model each get their
/// own tinted set and an untinted spider shares the files as loaded.
#[derive(Default)]
pub struct SpriteCache {
    map: HashMap<(PathBuf, Vec<(String, [i64; 3])>), Arc<SpriteAssets>>,
}

impl SpriteCache {
    pub fn new() -> Self {
        SpriteCache::default()
    }

    /// `_load_sprite_assets()`.
    pub fn load(&mut self, assets_dir: &FsPath, overrides: &[(String, [f64; 3])]) -> Arc<SpriteAssets> {
        let signature = palette_signature(overrides);
        let dir = assets_dir.canonicalize().unwrap_or_else(|_| assets_dir.to_path_buf());
        let key = (dir, signature);
        if let Some(found) = self.map.get(&key) {
            return found.clone();
        }
        let mut parts = HashMap::new();
        for name in SPRITE_PARTS {
            let candidate = assets_dir.join(format!("{name}.png"));
            if candidate.exists()
                && let Ok(img) = Image::load_png(&candidate)
                && !img.is_null()
            {
                parts.insert(name.to_string(), img);
            }
        }
        tint_assets(&mut parts, &key.1);
        let assets = Arc::new(SpriteAssets { parts });
        self.map.insert(key, assets.clone());
        assets
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

fn full_rect(img: &Image) -> Rect {
    Rect::new(0.0, 0.0, img.width() as f64, img.height() as f64)
}

/// `_draw_sprite_segment(painter, pixmap, x1, y1, x2, y2, thickness, opacity)`.
#[allow(clippy::too_many_arguments)]
pub fn draw_sprite_segment(
    p: &mut Canvas,
    pixmap: Option<&Image>,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    thickness: f64,
    opacity: f64,
) {
    let Some(pixmap) = pixmap else { return };
    if pixmap.is_null() {
        return;
    }
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = dx.hypot(dy);
    if length < 0.5 {
        return;
    }
    let angle = dy.atan2(dx).to_degrees();
    p.save();
    let parent = p.opacity();
    p.set_opacity(parent * opacity.clamp(0.0, 1.0));
    p.translate((x1 + x2) * 0.5, (y1 + y2) * 0.5);
    p.rotate(angle);
    p.draw_pixmap(Rect::new(-length * 0.5, -thickness * 0.5, length, thickness), pixmap, full_rect(pixmap));
    p.restore();
}

/// `_render_sprite_rig(painter)`. Returns `false` when the model has no
/// assets, in which case the caller draws it procedurally as Python did.
pub(crate) fn render_sprite_rig(p: &mut Canvas, cx: &Paint, assets: &SpriteAssets) -> bool {
    if assets.is_empty() {
        return false;
    }
    let d = cx.d;
    let look = cx.look;
    let size = d.size;
    let startle = d.startle_amount();
    let startle_highlight = d.startle_highlight_active(startle);
    let motion_startle = if d.dragging { 0.0 } else { startle };
    let tremble_x = (d.breath_phase * 17.0 + d.startle_phase).sin() * size * 0.018 * motion_startle;
    let tremble_y = (d.breath_phase * 19.0 + d.startle_phase * 0.7).cos() * size * 0.018 * motion_startle;
    let jz_shadow = clamp(d.jump_z / size.max(1.0), 0.0, 3.0);
    let shadow_shrink = 1.0 / (1.0 + jz_shadow * 0.55);

    p.save();
    if !cx.sw.fast_pixmaps {
        p.set_render_hint(RenderHint::SmoothPixmapTransform, true);
    }
    let shadow_scale = look.shadow_scale;
    if let Some(shadow) = assets.get("shadow") {
        p.save();
        let parent = p.opacity();
        p.set_opacity(parent * 0.34 * shadow_shrink);
        let shadow_w =
            size * shadow_scale * (1.0 + clamp(d.current_speed / 180.0, 0.0, 1.0) * 0.10) * shadow_shrink;
        let shadow_h = shadow_w * 0.52;
        p.draw_pixmap(
            Rect::new(d.x - shadow_w * 0.5, d.y + size * 0.05, shadow_w, shadow_h),
            shadow,
            full_rect(shadow),
        );
        p.restore();
    } else {
        p.set_pen(Pen::NONE);
        p.set_brush(cx.qcolor("legs", 8.max((28.0 * shadow_shrink) as i64)));
        p.draw_ellipse_at(d.x, d.y + size * 0.14, size * 0.88 * shadow_shrink, size * 0.46 * shadow_shrink);
    }

    let leg_upper = assets.get("leg_upper");
    let leg_lower = assets.get("leg_lower").or(leg_upper);
    let leg_tip = assets.get("leg_tip").or(leg_lower);
    let leg_thick = look.leg_segment_thickness.unwrap_or(size * 0.34) * (1.0 + startle * 0.08);
    let leg_tip_thick = look.leg_tip_thickness.unwrap_or(size * 0.18) * (1.0 + startle * 0.08);
    let foot_bulb = look.foot_bulb.unwrap_or(size * 0.055) * (1.0 + startle * 0.20);
    let chain_config = look.leg_chain.as_ref();
    let leg_knuckle = chain_config.map(|c| assets.get(&c.asset_name).or(leg_lower));

    for leg in &d.legs {
        let (ax, ay, kx, ky, foot_x, foot_y) = (leg.ax, leg.ay, leg.kx, leg.ky, leg.foot_x, leg.foot_y);
        let step_gain = leg.lift * 0.12;
        let seg_scale = 1.0 + step_gain;
        let mut tarsus = (0.0, 0.0);
        let chain_points = match (chain_config, leg.chain_points.as_deref()) {
            (Some(chain), Some(points)) => {
                let scales = &chain.segment_scales;
                let width_bases = [leg_thick, leg_thick, leg_thick * 0.84, leg_thick * 0.68, leg_tip_thick];
                let sprites = [leg_upper, leg_lower, leg_knuckle.flatten(), leg_knuckle.flatten(), leg_tip];
                for index in 0..points.len() - 1 {
                    let width = width_bases[index] * scales[index] * seg_scale;
                    let (start_x, start_y) = points[index];
                    let (end_x, end_y) = points[index + 1];
                    draw_sprite_segment(p, sprites[index], start_x, start_y, end_x, end_y, width, 1.0);
                }
                Some(points)
            }
            _ => {
                let tarsus_x = kx + (foot_x - kx) * 0.72;
                let tarsus_y = ky + (foot_y - ky) * 0.72;
                tarsus = (tarsus_x, tarsus_y);
                draw_sprite_segment(p, leg_upper, ax, ay, kx, ky, leg_thick * seg_scale, 1.0);
                draw_sprite_segment(p, leg_lower, kx, ky, tarsus_x, tarsus_y, leg_thick * 0.84 * seg_scale, 1.0);
                draw_sprite_segment(p, leg_tip, tarsus_x, tarsus_y, foot_x, foot_y, leg_tip_thick * seg_scale, 1.0);
                None
            }
        };
        p.set_pen(Pen::NONE);
        p.set_brush(cx.qcolor("legs", 215));
        if let (Some(chain), Some(points)) = (chain_config, chain_points) {
            let joint_scales = &chain.joint_scales;
            let joints = &points[1..points.len() - 1];
            for (joint_index, &(jx, jy)) in joints.iter().enumerate() {
                let radius = leg_thick * 0.14 * joint_scales[joint_index];
                p.draw_ellipse_at(jx, jy, radius, radius);
            }
            p.set_brush(cx.qcolor("highlight", 210));
            for (joint_index, &(jx, jy)) in joints.iter().enumerate() {
                let core_scale = if joint_index == 1 || joint_index == joints.len() - 1 { 0.075 } else { 0.052 };
                let radius = leg_thick * core_scale * joint_scales[joint_index];
                p.draw_ellipse_at(jx, jy, radius, radius);
            }
        } else {
            let knee_scale = 1.0;
            p.draw_ellipse_at(kx, ky, leg_thick * 0.14 * knee_scale, leg_thick * 0.14 * knee_scale);
            p.draw_ellipse_at(tarsus.0, tarsus.1, leg_tip_thick * 0.18, leg_tip_thick * 0.18);
        }
        p.set_brush(if startle_highlight { cx.qcolor("highlight", 215) } else { cx.qcolor("legs", 180) });
        p.draw_ellipse_at(foot_x, foot_y, foot_bulb * (1.0 + step_gain), foot_bulb * 0.70 * (1.0 + step_gain));
    }

    let crouch_drop = d.crouch * size * 0.06;
    p.save();
    let (lunge_x, lunge_y) = d.combat_body_offset;
    p.translate(d.x + tremble_x + lunge_x, d.y + d.body_bob + tremble_y - d.jump_z + crouch_drop + lunge_y);
    p.rotate(d.heading.to_degrees());
    if !d.spider_gait {
        p.rotate(d.body_wiggle.to_degrees());
    }
    let jz_body = clamp(d.jump_z / size.max(1.0), 0.0, 3.0);
    let jump_scale = 1.0 + jz_body * 0.12;
    let visual_squash = if d.state == "Roll" || d.roll_spin.abs() > 1e-4 { 1.0 } else { d.squash };
    let vfac = clamp(visual_squash * (1.0 - d.crouch * 0.14), 0.55, 1.2);
    let vroot = vfac.sqrt();
    p.scale(jump_scale / vroot, jump_scale * vroot);

    let abdomen_scale = look.abdomen_scale.as_deref().unwrap_or(&[1.08, 0.92]);
    let ceph_scale = look.cephalothorax_scale.as_deref().unwrap_or(&[0.84, 0.78]);
    let mut abdomen_offset_x = look.abdomen_offset_x * size;
    let mut ceph_offset_x = look.cephalothorax_offset_x.unwrap_or(0.36) * size;
    let abdomen_w = size * abdomen_scale[0] * (1.0 - d.abdomen_pulse * 0.18) * (1.0 + startle * 0.050);
    let abdomen_h = size * abdomen_scale[1] * (1.0 + d.abdomen_pulse * 0.55) * (1.0 - startle * 0.055);
    let mut ceph_w = size * ceph_scale[0] * (1.0 + d.ceph_pulse * 0.08) * (1.0 + startle * 0.080);
    let mut ceph_h = size * ceph_scale[1] * (1.0 + d.ceph_pulse * 0.18) * (1.0 + startle * 0.030);
    ceph_w *= 1.0 + d.rear * 0.12;
    ceph_h *= 1.0 + d.rear * 0.12;
    ceph_offset_x += d.rear * size * 0.05;
    let stance = d.combat_stance;
    if stance > 0.01 {
        ceph_offset_x += stance * size * 0.16;
        ceph_w *= 1.0 + stance * 0.10;
        ceph_h *= 1.0 + stance * 0.10;
        abdomen_offset_x -= stance * size * 0.10;
    }

    if let Some(abdomen) = assets.get("abdomen") {
        p.save();
        p.rotate(d.abdomen_wag.to_degrees());
        p.draw_pixmap(
            rect(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5, abdomen_w, abdomen_h),
            abdomen,
            full_rect(abdomen),
        );
        draw_abdomen_marking(p, cx, abdomen_offset_x, abdomen_w, abdomen_h);
        p.restore();
    }
    if let Some(ceph) = assets.get("cephalothorax") {
        p.draw_pixmap(rect(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h), ceph, full_rect(ceph));
    }

    p.set_pen(pen_cap(cx.qcolor("legs", 215), (size * 0.036).max(1.0), Cap::Round));
    let palp_span = size * look.pedipalp_scale;
    p.draw_line(
        ceph_offset_x + ceph_w * 0.24,
        -size * 0.10,
        ceph_offset_x + ceph_w * 0.54,
        -size * 0.21 * palp_span / size,
    );
    p.draw_line(ceph_offset_x + ceph_w * 0.24, size * 0.10, ceph_offset_x + ceph_w * 0.54, size * 0.21 * palp_span / size);

    let eye_scale = look.eye_scale.unwrap_or(0.95);
    let eye_count = look.eye_count.unwrap_or(4).max(2);
    let eye_layout = look.eye_layout.as_deref().unwrap_or("grid").to_lowercase();
    let aiming = clamp(d.aim_intent, 0.0, 1.0);
    let eye_startle = d.eye_startle_amount(startle);
    let eye_r = (size * 0.030 * eye_scale).max(1.0) * (1.0 + eye_startle * 0.78);
    let mut eyes: Vec<(f64, f64, f64)> = Vec::new();
    if eye_layout == "jumping_spider" {
        let big_r = eye_r * 1.9;
        let small_r = eye_r * 0.75;
        eyes = vec![
            (ceph_offset_x + ceph_w * 0.05, -ceph_h * 0.03, big_r),
            (ceph_offset_x + ceph_w * 0.28, -ceph_h * 0.03, big_r),
            (ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.14, small_r),
            (ceph_offset_x + ceph_w * 0.44, -ceph_h * 0.14, small_r),
            (ceph_offset_x - ceph_w * 0.04, ceph_h * 0.16, small_r * 0.9),
            (ceph_offset_x + ceph_w * 0.36, ceph_h * 0.16, small_r * 0.9),
        ];
    } else {
        let rows = 2;
        let cols = (eye_count / 2).max(2);
        for r in 0..rows {
            for c in 0..cols {
                if r * cols + c >= eye_count {
                    break;
                }
                let eyex = ceph_offset_x + ceph_w * (0.02 + c as f64 * 0.11);
                let eyey = (-0.17 + r as f64 * 0.17 + (c % 2) as f64 * 0.015) * ceph_h;
                eyes.push((eyex, eyey, eye_r * if c % 2 == 1 { 0.92 } else { 1.0 }));
            }
        }
    }
    draw_eyes(p, cx, &eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming);
    draw_antennae(p, cx, ceph_offset_x, ceph_w, ceph_h, startle);
    p.restore();
    p.restore();
    draw_equipment(p, cx);
    true
}

/// `_draw_equipment(painter)`: outline accents for armour on sprite-rig and
/// regal models.
pub(crate) fn draw_equipment(p: &mut Canvas, cx: &Paint) {
    let d = cx.d;
    if d.worn.is_empty() {
        return;
    }
    let size = d.size;
    let armor_color = cx.qcolor("highlight", 175);
    let dark_color = cx.qcolor("legs", 175);
    p.save();
    p.translate(0.0, -d.jump_z);
    p.set_pen(pen_full(armor_color, (size * 0.035).max(1.0), Cap::Round, Join::Round));
    p.set_brush(Brush::None);
    if d.worn.abdomen.is_some() {
        p.draw_ellipse(rect(d.x - size * 0.52, d.y - size * 0.38, size * 0.78, size * 0.70));
    }
    if d.worn.carapace.is_some() {
        let (ccx, ccy) = d.body_local_to_world(size * 0.34, 0.0);
        p.draw_ellipse(rect(ccx - size * 0.34, ccy - size * 0.27, size * 0.68, size * 0.54));
    }
    if d.worn.head.is_some() {
        let (hx, hy) = d.body_local_to_world(size * 0.55, 0.0);
        p.draw_ellipse(rect(hx - size * 0.16, hy - size * 0.14, size * 0.32, size * 0.28));
    }
    if d.worn.legs.is_some() {
        p.set_pen(pen_cap(armor_color, (size * 0.055).max(1.0), Cap::Round));
        for leg in &d.legs {
            let [ax, ay, fx, fy] = leg.draw_points;
            p.draw_point(ax + (fx - ax) * 0.34, ay + (fy - ay) * 0.34);
        }
    }
    if d.worn.pedipalps.is_some() {
        p.set_pen(pen_cap(dark_color, (size * 0.060).max(1.0), Cap::Round));
        for &(tx, ty) in &d.antenna_hand_targets {
            if (tx, ty) != (0.0, 0.0) {
                p.draw_point(tx, ty);
            }
        }
    }
    p.restore();
}
