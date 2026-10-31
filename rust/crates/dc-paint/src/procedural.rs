//! Port of the drawing half of `creature/render_procedural.py`:
//! `_render_procedural`, `_draw_leg_armour`, `_draw_abdomen_marking`,
//! `_draw_leg_connections`, `_draw_antennae` and `_draw_eyes`. The leg
//! geometry is `dc-kinematics` (see [`crate::resolve`]).

use crate::PaintSwitches;
use crate::antenna::{antenna_drive_from_mood, build_antenna_points};
use crate::armour_art;
use crate::batch::{
    LAYER_CORE, LAYER_FOOT, LAYER_FUZZ, LAYER_HAIR, LAYER_JOINT, LAYER_SEGMENT, LAYER_SHADOW, LegBatch,
};
use crate::color::mix;
use crate::draw::CreatureDraw;
use crate::look::{AntennaStyle, CreatureLook};
use dc_core::math::clamp;
use dc_core::{Color, Rect};
use dc_kinematics::detail::{MERGED_HAIR_TINT, hair_detail, hair_pen_width, joint_node_shows};
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen, PenStyle};
use std::sync::OnceLock;

/// `QPen(color, width, Qt.SolidLine, cap)` (join defaults to bevel).
pub(crate) fn pen_cap(color: Color, width: f64, cap: Cap) -> Pen {
    Pen::full(color, width, PenStyle::Solid, cap, Join::Bevel)
}

/// `QPen(color, width, Qt.SolidLine, cap, join)`.
pub(crate) fn pen_full(color: Color, width: f64, cap: Cap, join: Join) -> Pen {
    Pen::full(color, width, PenStyle::Solid, cap, join)
}

/// `QRectF(x, y, w, h)`.
#[inline]
pub(crate) fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h)
}

/// One armoured leg: its points and the width of each segment.
type ArmouredLeg = (Vec<(f64, f64)>, Vec<f64>);

/// `_enemy_shell_path(style)`: unit outlines, cached once.
fn enemy_shell_path(style: &str) -> &'static Path {
    static CRAB: OnceLock<Path> = OnceLock::new();
    static SPINY: OnceLock<Path> = OnceLock::new();
    let build = |points: &[(f64, f64)]| {
        let mut p = Path::new();
        p.move_to(points[0].0, points[0].1);
        for &(x, y) in &points[1..] {
            p.line_to(x, y);
        }
        p.close_subpath();
        p
    };
    if style == "crab" {
        CRAB.get_or_init(|| {
            build(&[
                (-0.46, -0.26),
                (-0.22, -0.50),
                (0.23, -0.47),
                (0.50, -0.25),
                (0.50, 0.25),
                (0.23, 0.47),
                (-0.22, 0.50),
                (-0.46, 0.26),
            ])
        })
    } else {
        SPINY.get_or_init(|| {
            build(&[
                (0.48, 0.0),
                (0.31, -0.30),
                (0.42, -0.67),
                (0.10, -0.44),
                (-0.15, -0.50),
                (-0.36, -0.76),
                (-0.38, -0.32),
                (-0.68, -0.20),
                (-0.48, 0.0),
                (-0.68, 0.20),
                (-0.38, 0.32),
                (-0.36, 0.76),
                (-0.15, 0.50),
                (0.10, 0.44),
                (0.42, 0.67),
                (0.31, 0.30),
            ])
        })
    }
}

/// The colour helpers every creature painter uses.
pub(crate) struct Paint<'a> {
    pub d: &'a CreatureDraw,
    pub look: &'a CreatureLook,
    pub sw: &'a PaintSwitches,
}

impl<'a> Paint<'a> {
    #[inline]
    pub fn qcolor(&self, key: &str, alpha: i64) -> Color {
        self.d.palette.qcolor(key, alpha)
    }
    #[inline]
    pub fn triplet(&self, rgb: [f64; 3], alpha: i64) -> Color {
        self.d.palette.qcolor_triplet(rgb, alpha)
    }
    /// `_appearance_color(key, fallback)` for the two accents the painters use.
    pub fn fluff_rgb(&self) -> [f64; 3] {
        self.d
            .palette
            .appearance_color("fluff_color", "highlight", self.look.fluff_color)
            .unwrap_or([120.0, 80.0, 60.0])
    }
}

/// `_render_procedural(painter)`.
pub(crate) fn render_procedural(p: &mut Canvas, cx: &Paint) {
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

    p.set_pen(Pen::NONE);
    p.set_brush(cx.qcolor("legs", 10.max((30.0 * shadow_shrink) as i64)));
    p.draw_ellipse_at(d.x, d.y + size * 0.14, size * 0.78 * shadow_shrink, size * 0.42 * shadow_shrink);

    let leg_width_scale = look.leg_thickness;
    let fluffiness = look.fluffiness;
    let segmented_legs = look.segmented_legs;
    let show_joint_nodes = look.show_joint_nodes;
    let joint_node_scale = look.joint_node_scale;
    let coxa_width_scale = look.coxa_thickness_scale;
    let chain_config = look.leg_chain.as_ref();
    let resting_leg_width = (size * 0.066 * leg_width_scale).max(1.4);
    let detail = hair_detail(resting_leg_width, chain_config, cx.sw.detail_enabled);
    let shadow = match (&look.leg_shadow, chain_config) {
        (Some(cfg), Some(_)) if d.jump_z <= 0.0 && !d.dragging => {
            let shadow_h = size * clamp(cfg.knee_height, 0.0, 0.8);
            Some((
                shadow_h * 0.55,
                shadow_h * 0.80,
                cfg.root_height,
                cx.triplet([0.0, 0.0, 0.0], clamp(cfg.alpha, 0.0, 255.0) as i64),
                resting_leg_width * clamp(cfg.width, 0.3, 2.5),
            ))
        }
        _ => None,
    };
    let hair_color = if detail.separate_hair || detail.merge_hair {
        let hair_scale = chain_config.map(|c| c.hair_scale).unwrap_or(0.0);
        Some(cx.triplet(cx.fluff_rgb(), (72.0 + hair_scale * 90.0) as i64))
    } else {
        None
    };

    let mut batch = LegBatch::new(cx.sw.leg_width_step, !cx.sw.batch_legs);
    let leg_armour = d.worn.legs.as_deref();
    let mut armoured_legs: Vec<ArmouredLeg> = Vec::new();
    for leg in &d.legs {
        let (ax, ay, kx, ky) = (leg.ax, leg.ay, leg.kx, leg.ky);
        let (coxa_x, coxa_y, tarsus_x, tarsus_y) = (leg.coxa_x, leg.coxa_y, leg.tarsus_x, leg.tarsus_y);
        let (foot_x, foot_y) = (leg.foot_x, leg.foot_y);
        let chain_points = if chain_config.is_some() && segmented_legs { leg.chain_points.as_deref() } else { None };
        if let (Some(cp), Some((sdx, sdy, root_k, shadow_color, shadow_w))) = (chain_points, shadow) {
            let (end_x, end_y) = *cp.last().unwrap();
            batch.line(
                p,
                LAYER_SHADOW,
                shadow_color,
                shadow_w,
                ax + sdx * root_k,
                ay + sdy * root_k,
                kx + sdx,
                ky + sdy,
            );
            batch.line(p, LAYER_SHADOW, shadow_color, shadow_w, kx + sdx, ky + sdy, end_x, end_y);
        }

        let base_width = (size * (0.066 + leg.lift * 0.016) * leg_width_scale).max(1.4) * (1.0 + startle * 0.10);
        let coxa_width = (base_width * 1.08 * coxa_width_scale).max(1.2);
        let femur_width = (base_width * 0.98).max(1.0);
        let tibia_width = (base_width * 0.78).max(1.0);
        let tarsus_width = (base_width * 0.42).max(1.0);
        if leg_armour.is_some() && chain_points.is_none() {
            armoured_legs.push((
                vec![(coxa_x, coxa_y), (kx, ky), (tarsus_x, tarsus_y), (foot_x, foot_y)],
                vec![femur_width, tibia_width, tarsus_width],
            ));
        }
        if fluffiness > 0.0 && chain_points.is_none() {
            let mut fuzzy = Path::new();
            fuzzy.move_to(ax, ay);
            fuzzy.cubic_to(coxa_x, coxa_y, coxa_x, coxa_y, kx, ky);
            fuzzy.line_to(tarsus_x, tarsus_y);
            fuzzy.line_to(foot_x, foot_y);
            batch.add_path(
                p,
                LAYER_FUZZ,
                cx.qcolor("highlight", (40.0 + fluffiness * 50.0) as i64),
                base_width * (1.4 + fluffiness * 0.6),
                &fuzzy,
            );
        }

        let color = if startle_highlight { cx.qcolor("highlight", 230) } else { cx.qcolor("legs", 245) };
        if let (Some(cp), Some(chain)) = (chain_points, chain_config) {
            let width_bases =
                [coxa_width, femur_width, tibia_width, tarsus_width.max(tibia_width * 0.72), tarsus_width];
            let scales = &chain.width_scales;
            let keys = &chain.segment_color_keys;
            let n = cp.len() - 1;
            let chain_widths: Vec<f64> = (0..n).map(|i| width_bases[i] * scales[i]).collect();
            if leg_armour.is_some() {
                armoured_legs.push((cp.to_vec(), chain_widths.clone()));
            }
            for (index, &width) in chain_widths.iter().enumerate() {
                let (start_x, start_y) = cp[index];
                let (end_x, end_y) = cp[index + 1];
                if detail.separate_hair {
                    batch.line(
                        p,
                        LAYER_HAIR,
                        hair_color.unwrap(),
                        hair_pen_width(width, chain.hair_scale),
                        start_x,
                        start_y,
                        end_x,
                        end_y,
                    );
                }
                let mut segment_color = if startle_highlight {
                    cx.qcolor("highlight", 230)
                } else {
                    let key = keys.get(index).map(String::as_str).unwrap_or("legs");
                    cx.qcolor(key, if index < chain_widths.len() - 1 { 245 } else { 220 })
                };
                let mut stroke_width = width;
                if detail.merge_hair {
                    stroke_width = hair_pen_width(width, chain.hair_scale);
                    segment_color = mix(segment_color, hair_color, MERGED_HAIR_TINT);
                }
                batch.line(p, LAYER_SEGMENT, segment_color, stroke_width, start_x, start_y, end_x, end_y);
            }
            if show_joint_nodes {
                let joint_color = cx.qcolor(&chain.joint_color_key, 225);
                let node_r = (base_width * 0.24 * joint_node_scale).max(1.0);
                let joints = &cp[1..cp.len() - 1];
                let mut shown: Vec<(usize, f64, f64)> = Vec::new();
                for (joint_index, &(jx, jy)) in joints.iter().enumerate() {
                    let scale = chain.joint_scales[joint_index];
                    let radius = node_r * scale * if joint_index == 1 { 1.12 } else { 0.96 };
                    let hi = (joint_index + 2).min(chain_widths.len());
                    if !joint_node_shows(radius, &chain_widths[joint_index.min(hi)..hi], cx.sw.detail_enabled) {
                        continue;
                    }
                    shown.push((joint_index, jx, jy));
                    batch.dot(p, LAYER_JOINT, joint_color, jx, jy, radius);
                }
                if !shown.is_empty() {
                    let core_color = cx.qcolor(&chain.joint_color_key, 230);
                    for (joint_index, jx, jy) in shown {
                        let core_scale = if joint_index == 1 || joint_index == joints.len() - 1 { 0.48 } else { 0.34 };
                        batch.dot(p, LAYER_CORE, core_color, jx, jy, node_r * core_scale);
                    }
                }
            }
        } else {
            if segmented_legs {
                batch.line(p, LAYER_SEGMENT, color, coxa_width, ax, ay, coxa_x, coxa_y);
                batch.line(p, LAYER_SEGMENT, color, femur_width, coxa_x, coxa_y, kx, ky);
                batch.line(p, LAYER_SEGMENT, color, tibia_width, kx, ky, tarsus_x, tarsus_y);
                batch.line(p, LAYER_SEGMENT, cx.qcolor("legs", 210), tarsus_width, tarsus_x, tarsus_y, foot_x, foot_y);
            } else {
                let mut path = Path::new();
                path.move_to(ax, ay);
                path.cubic_to(coxa_x, coxa_y, coxa_x, coxa_y, kx, ky);
                path.line_to(tarsus_x, tarsus_y);
                path.line_to(foot_x, foot_y);
                batch.add_path(p, LAYER_SEGMENT, color, base_width, &path);
                batch.line(
                    p,
                    LAYER_SEGMENT,
                    cx.qcolor("legs", 210),
                    (base_width * 0.48).max(1.0),
                    tarsus_x,
                    tarsus_y,
                    foot_x,
                    foot_y,
                );
            }
            if show_joint_nodes {
                let joint_key = chain_config.map(|c| c.joint_color_key.as_str()).unwrap_or("legs");
                let joint_color = cx.qcolor(joint_key, 225);
                let node_r = (base_width * 0.24 * joint_node_scale).max(1.0);
                batch.dot(p, LAYER_JOINT, joint_color, coxa_x, coxa_y, node_r * 1.05);
                batch.dot(p, LAYER_JOINT, joint_color, kx, ky, node_r * 1.30);
                batch.dot(p, LAYER_JOINT, joint_color, tarsus_x, tarsus_y, node_r * 0.98);
                batch.dot(p, LAYER_CORE, cx.qcolor("highlight", 210), kx, ky, node_r * 0.60);
            }
        }
        let foot_key = if startle_highlight {
            "highlight"
        } else {
            chain_config.map(|c| c.tip_color_key.as_str()).unwrap_or("legs")
        };
        batch.point(p, LAYER_FOOT, cx.qcolor(foot_key, 220), (tarsus_width * 0.8).max(1.0), foot_x, foot_y);
    }

    let crouch_drop = d.crouch * size * 0.06;
    let legs_over_body = look.legs_over_body;
    if !legs_over_body {
        batch.flush(p);
        draw_leg_armour(p, leg_armour, &armoured_legs);
    }
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
    let body = cx.qcolor("body", 255);
    let highlight = cx.qcolor("highlight", look.highlight_alpha);
    let leg_color = cx.qcolor("legs", 255);
    let fluff_color = cx.triplet(cx.fluff_rgb(), (38.0 + fluffiness * 70.0) as i64);

    let abdomen_scale = look.abdomen_scale.as_deref().unwrap_or(&[0.92, 0.84]);
    let ceph_scale = look.cephalothorax_scale.as_deref().unwrap_or(&[0.70, 0.66]);
    let mut abdomen_offset_x = look.abdomen_offset_x * size;
    let mut ceph_offset_x = look.cephalothorax_offset_x.unwrap_or(0.40) * size;
    let pedicel_cfg = &look.pedicel;
    let head_cfg = &look.head;
    let pedipalp_scale = look.pedipalp_scale;
    let eye_scale = look.eye_scale.unwrap_or(1.0);
    let eye_count = look.eye_count.unwrap_or(2).max(2);

    let abdomen_w = size * (abdomen_scale[0] * (1.0 - d.abdomen_pulse * 0.35)) * (1.0 + startle * 0.055);
    let abdomen_h = size * (abdomen_scale[1] * (1.0 + d.abdomen_pulse)) * (1.0 - startle * 0.060);
    let mut ceph_w = size * (ceph_scale[0] * (1.0 + d.ceph_pulse * 0.25)) * (1.0 + startle * 0.075);
    let mut ceph_h = size * (ceph_scale[1] * (1.0 + d.ceph_pulse)) * (1.0 + startle * 0.030);
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
    let abdo_wag = d.abdomen_wag * size * 0.45;

    let pedicel_enabled = pedicel_cfg.enabled;
    let pedicel_offset_x = if pedicel_enabled { pedicel_cfg.offset_x * size } else { 0.0 };
    let (pedicel_w, pedicel_h) = match (pedicel_enabled, pedicel_cfg.scale) {
        (true, Some([sx, sy])) => (size * sx, size * sy),
        (false, _) => (0.0, 0.0),
        _ => (0.0, 0.0),
    };
    let head_enabled = head_cfg.enabled;
    let head_offset_x = if head_enabled { head_cfg.offset_x * size } else { ceph_offset_x };
    let (head_w, head_h) = match (head_enabled, head_cfg.scale) {
        (true, Some([sx, sy])) => (size * sx, size * sy),
        _ => (0.0, 0.0),
    };

    if fluffiness > 0.0 {
        p.set_pen(Pen::NONE);
        p.set_brush(fluff_color);
        for (ox, oy, sx, sy) in [
            (abdomen_offset_x - size * 0.03, -size * 0.03, abdomen_w * 0.60, abdomen_h * 0.54),
            (abdomen_offset_x - size * 0.13, size * 0.12, abdomen_w * 0.58, abdomen_h * 0.46),
            (ceph_offset_x + size * 0.01, -size * 0.06, ceph_w * 0.62, ceph_h * 0.52),
            (ceph_offset_x + size * 0.02, size * 0.08, ceph_w * 0.54, ceph_h * 0.42),
            (head_offset_x, -size * 0.02, head_w * 0.66, head_h * 0.56),
        ] {
            p.draw_ellipse(rect(ox - sx * 0.5, oy - sy * 0.5, sx, sy));
        }
    }

    p.set_pen(pen_cap(leg_color, (size * 0.035).max(1.0), Cap::Round));
    p.set_brush(body);
    let shell = look.enemy_shell.as_str();
    if shell == "crab" || shell == "spiny" {
        p.save();
        p.translate(abdomen_offset_x, abdo_wag);
        p.scale(abdomen_w, abdomen_h);
        p.set_pen(Pen::new(leg_color, 0.025));
        p.draw_path(enemy_shell_path(shell));
        p.restore();
    } else {
        p.draw_ellipse(rect(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5 + abdo_wag, abdomen_w, abdomen_h));
    }
    if pedicel_enabled && pedicel_w > 0.0 && pedicel_h > 0.0 {
        let pedicel_color = cx.qcolor(&pedicel_cfg.color_key, 255);
        let pedicel_outline = cx.qcolor(&pedicel_cfg.outline_key, 210);
        let outline_width = (size * clamp(pedicel_cfg.outline_width, 0.01, 0.07)).max(1.0);
        p.set_pen(pen_cap(pedicel_outline, outline_width, Cap::Round));
        p.set_brush(pedicel_color);
        p.draw_ellipse(rect(pedicel_offset_x - pedicel_w * 0.5, -pedicel_h * 0.5, pedicel_w, pedicel_h));
    }
    p.draw_ellipse(rect(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h));
    if let Some(rim) = &look.carapace_rim {
        let rim_w = size * clamp(rim.width, 0.01, 0.25);
        p.set_pen(Pen::NONE);
        p.set_brush(cx.qcolor(&rim.color_key, rim.alpha));
        p.draw_ellipse(rect(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h));
        let patch_w = (ceph_w - 2.0 * rim_w).max(0.0);
        let patch_h = (ceph_h - 2.0 * rim_w).max(0.0);
        p.set_brush(body);
        p.draw_ellipse(rect(ceph_offset_x - patch_w * 0.5, -patch_h * 0.5, patch_w, patch_h));
    }
    if head_enabled && head_w > 0.0 && head_h > 0.0 {
        let head_color = cx.qcolor(&head_cfg.color_key, 255);
        let head_outline = cx.qcolor(&head_cfg.outline_key, 225);
        let outline_width = (size * clamp(head_cfg.outline_width, 0.01, 0.07)).max(1.0);
        p.set_pen(pen_cap(head_outline, outline_width, Cap::Round));
        p.set_brush(head_color);
        p.draw_ellipse(rect(head_offset_x - head_w * 0.5, -head_h * 0.5, head_w, head_h));
    }
    p.set_pen(Pen::NONE);
    p.set_brush(highlight);
    p.draw_ellipse(rect(
        abdomen_offset_x - abdomen_w * 0.18,
        -abdomen_h * 0.28 + abdo_wag,
        abdomen_w * 0.28,
        abdomen_h * 0.18,
    ));
    p.draw_ellipse(rect(ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.22, ceph_w * 0.20, ceph_h * 0.13));
    if head_enabled && head_w > 0.0 && head_h > 0.0 {
        let head_highlight = cx.qcolor(&head_cfg.highlight_key, head_cfg.highlight_alpha);
        p.set_brush(head_highlight);
        p.draw_ellipse(rect(head_offset_x - head_w * 0.18, -head_h * 0.26, head_w * 0.28, head_h * 0.16));
    }

    if look.stripe_color_set {
        let stripe = d.palette.appearance_color("stripe_color", "leg_band", look.stripe_color);
        if let Some(stripe) = stripe {
            p.set_brush(cx.triplet(stripe, look.stripe_alpha));
            let stripe_count = look.stripe_count.max(1);
            for i in 0..stripe_count {
                let t = i as f64 / (stripe_count - 1).max(1) as f64;
                let scx = abdomen_offset_x - abdomen_w * (0.12 + t * 0.18);
                p.draw_ellipse(rect(
                    scx - abdomen_w * 0.08,
                    -abdomen_h * 0.28 + t * abdomen_h * 0.16,
                    abdomen_w * 0.16,
                    abdomen_h * 0.10,
                ));
            }
        }
    }

    if look.enemy_crown {
        p.set_pen(Pen::new(cx.qcolor("leg_band", 255), (size * 0.035).max(1.0)));
        for side in [-1.0, 1.0] {
            p.draw_line(ceph_offset_x, side * ceph_h * 0.37, ceph_offset_x + ceph_w * 0.44, side * ceph_h * 0.68);
            p.draw_line(
                ceph_offset_x + ceph_w * 0.44,
                side * ceph_h * 0.68,
                ceph_offset_x + ceph_w * 0.38,
                side * ceph_h * 0.25,
            );
        }
    }
    p.save();
    if !look.enemy_marking.is_empty() {
        p.translate(0.0, abdo_wag);
    }
    draw_abdomen_marking(p, cx, abdomen_offset_x, abdomen_w, abdomen_h);
    p.restore();
    if let Some(id) = &d.worn.abdomen {
        armour_art::paint_abdomen(p, abdomen_offset_x, abdomen_w, abdomen_h, abdo_wag, &armour_art::look_for(id));
    }
    if let Some(id) = &d.worn.carapace {
        armour_art::paint_carapace(p, ceph_offset_x, ceph_w, ceph_h, &armour_art::look_for(id));
    }
    if let Some(id) = &d.worn.head
        && head_enabled
        && head_w > 0.0
    {
        armour_art::paint_head(p, head_offset_x, head_w, head_h, &armour_art::look_for(id));
    }

    p.set_pen(pen_cap(leg_color, (size * 0.045).max(1.2), Cap::Round));
    let custom_hand_palps = look.antennae_is_dict && look.antennae.style != AntennaStyle::Feelers;
    if !custom_hand_palps {
        let palps_y = size * 0.12 * pedipalp_scale * (1.0 + startle * 0.55);
        p.draw_line(ceph_offset_x + ceph_w * 0.25, -palps_y, ceph_offset_x + ceph_w * 0.55, -palps_y * 1.85);
        p.draw_line(ceph_offset_x + ceph_w * 0.25, palps_y, ceph_offset_x + ceph_w * 0.55, palps_y * 1.85);
    }

    let (head_face_x, head_face_y) = if head_enabled && head_w > 0.0 && head_h > 0.0 {
        (head_offset_x + head_w * head_cfg.face_forward, head_h * head_cfg.face_side)
    } else {
        (ceph_offset_x, 0.0)
    };
    let aiming = clamp(d.aim_intent, 0.0, 1.0);
    let eye_startle = d.eye_startle_amount(startle);
    let eye_r = (size * 0.035 * eye_scale).max(1.0) * (1.0 + eye_startle * 0.72);
    let mut eyes: Vec<(f64, f64, f64)> = Vec::new();
    if look.eye_layout.as_deref() == Some("enemy_jumper") {
        for side in [-1.0, 1.0] {
            eyes.push((head_face_x + head_w * 0.08, side * head_h * 0.19, eye_r * 1.35));
            eyes.push((head_face_x - head_w * 0.22, side * head_h * 0.39, eye_r * 0.55));
        }
    } else if eye_count <= 2 {
        eyes.push((head_face_x, head_face_y - head_h * 0.16, eye_r));
        eyes.push((head_face_x, head_face_y + head_h * 0.16, eye_r));
    } else {
        let rows = 2;
        let cols = (eye_count / 2).max(2);
        'rows: for r in 0..rows {
            for c in 0..cols {
                if r * cols + c >= eye_count {
                    continue 'rows;
                }
                let eyex = head_face_x - head_w * 0.20 + head_w * (0.08 + c as f64 * 0.13);
                let eyey = head_face_y + (-0.18 + r as f64 * 0.18 + (c % 2) as f64 * 0.02) * head_h;
                eyes.push((eyex, eyey, eye_r * if c % 2 == 1 { 0.85 } else { 1.0 }));
            }
        }
    }
    if head_enabled && head_w > 0.0 && head_h > 0.0 && head_cfg.fangs {
        let fang_color = cx.qcolor("legs", 235);
        p.set_pen(pen_cap(fang_color, (size * 0.018).max(1.0), Cap::Round));
        let fang_x = head_offset_x + head_w * 0.33;
        for side_sign in [-1.0, 1.0] {
            p.draw_line(fang_x, side_sign * head_h * 0.13, fang_x + head_w * 0.04, side_sign * head_h * 0.42);
        }
    }
    draw_eyes(p, cx, &eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming);
    draw_antennae(p, cx, ceph_offset_x, ceph_w, ceph_h, startle);
    p.restore();
    if legs_over_body {
        batch.flush(p);
        draw_leg_armour(p, leg_armour, &armoured_legs);
    }
    draw_leg_connections(p, cx);
}

/// `_draw_leg_armour(painter, item, legs)`.
fn draw_leg_armour(p: &mut Canvas, item: Option<&str>, legs: &[ArmouredLeg]) {
    let Some(item) = item else { return };
    if legs.is_empty() {
        return;
    }
    let l = armour_art::look_for(item);
    for (points, widths) in legs {
        armour_art::paint_leg_armour(p, points, widths, &l);
    }
}

/// `_draw_abdomen_marking(painter, abdomen_x, abdomen_w, abdomen_h)`.
pub(crate) fn draw_abdomen_marking(p: &mut Canvas, cx: &Paint, abdomen_x: f64, abdomen_w: f64, abdomen_h: f64) {
    let marking = cx.d.palette.colors.get("marking");
    // `if not self.colors.get("marking")`: absent or an empty list.
    if marking.is_none() {
        return;
    }
    p.save();
    p.set_pen(Pen::NONE);
    p.set_brush(cx.qcolor("marking", 235));
    let style = cx.look.enemy_marking.as_str();
    if !style.is_empty() {
        match style {
            "stripe" => {
                p.draw_rounded_rect(
                    rect(abdomen_x - abdomen_w * 0.36, -abdomen_h * 0.085, abdomen_w * 0.70, abdomen_h * 0.17),
                    2.0,
                    2.0,
                );
            }
            "spots" => {
                for offset in [-0.23, 0.03, 0.26] {
                    for side in [-1.0, 1.0] {
                        p.draw_ellipse(rect(
                            abdomen_x + abdomen_w * (offset - 0.055),
                            abdomen_h * (side * 0.23 - 0.065),
                            abdomen_w * 0.11,
                            abdomen_h * 0.13,
                        ));
                    }
                }
            }
            "plates" => {
                p.set_brush(Brush::None);
                p.set_pen(Pen::new(cx.qcolor("marking", 220), (abdomen_w * 0.055).max(1.0)));
                for offset in [-0.23, 0.0, 0.23] {
                    p.draw_line(
                        abdomen_x + abdomen_w * offset,
                        -abdomen_h * 0.32,
                        abdomen_x + abdomen_w * offset,
                        abdomen_h * 0.32,
                    );
                }
            }
            "chevrons" => {
                p.set_pen(Pen::new(cx.qcolor("marking", 230), (abdomen_w * 0.065).max(1.0)));
                for offset in [-0.25, 0.0, 0.25] {
                    let x = abdomen_x + abdomen_w * offset;
                    p.draw_line(x - abdomen_w * 0.11, -abdomen_h * 0.27, x, 0.0);
                    p.draw_line(x, 0.0, x - abdomen_w * 0.11, abdomen_h * 0.27);
                }
            }
            _ => {}
        }
        p.restore();
        return;
    }
    let front = abdomen_x + abdomen_w * 0.26;
    let back = abdomen_x - abdomen_w * 0.44;
    let width = abdomen_h * 0.15;
    p.draw_rounded_rect(rect(back, -width / 2.0, front - back, width), width / 2.0, width / 2.0);
    for (i, reach) in [0.34, 0.27, 0.19].into_iter().enumerate() {
        let x = abdomen_x + abdomen_w * (0.16 - i as f64 * 0.22);
        let half = abdomen_h * reach;
        let bar = abdomen_w * 0.075;
        p.draw_rounded_rect(rect(x - bar / 2.0, -half, bar, half * 2.0), bar / 2.0, bar / 2.0);
    }
    p.restore();
}

/// `_draw_leg_connections(painter, chain_config)`.
fn draw_leg_connections(p: &mut Canvas, cx: &Paint) {
    let (Some(cfg), Some(chain)) = (&cx.look.leg_connections, &cx.look.leg_chain) else { return };
    let size = cx.d.size;
    let socket_radius = cfg.socket_radius * size;
    let outline_width = cfg.socket_outline_width * size;
    let socket_core_scale = cfg.socket_core_scale;
    let coxa_length = cfg.coxa_length * size;
    let trochanter_length = cfg.trochanter_length * size;
    let coxa_width = cfg.coxa_width * size;
    let trochanter_width = cfg.trochanter_width * size;
    let joint_radius = cfg.joint_radius * size;

    let socket_color = cx.qcolor(&cfg.socket_color_key, 245);
    let core_color = cx.qcolor(&cfg.core_color_key, 250);
    let accent_color = cx.qcolor(&cfg.accent_color_key, 150);
    let proximal_key = chain.segment_color_keys.first().map(String::as_str).unwrap_or(cfg.core_color_key.as_str());
    let proximal_color = cx.qcolor(proximal_key, 245);

    for leg in &cx.d.legs {
        let Some(cp) = leg.connection_points.as_deref() else { continue };
        if cp.len() < 2 {
            continue;
        }
        let (root_x, root_y) = cp[0];
        let (first_x, first_y) = cp[1];
        let mut dx = first_x - root_x;
        let mut dy = first_y - root_y;
        let mut len = dx.hypot(dy);
        if len < 1e-4 && cp.len() > 2 {
            let (sx, sy) = cp[2];
            dx = sx - root_x;
            dy = sy - root_y;
            len = dx.hypot(dy);
        }
        if len < 1e-4 {
            continue;
        }
        dx /= len;
        dy /= len;
        let exposed = (coxa_length + trochanter_length).min((size * 0.12).max(len));
        let end_x = root_x + dx * exposed;
        let end_y = root_y + dy * exposed;
        let split = coxa_length / (coxa_length + trochanter_length).max(1e-4);
        let joint_x = root_x + (end_x - root_x) * split;
        let joint_y = root_y + (end_y - root_y) * split;

        p.set_pen(pen_cap(socket_color, outline_width.max(1.0), Cap::Round));
        p.draw_line(root_x, root_y, end_x, end_y);
        p.set_pen(pen_cap(proximal_color, coxa_width.max(1.2), Cap::Round));
        p.draw_line(root_x, root_y, joint_x, joint_y);
        p.set_pen(pen_cap(core_color, trochanter_width.max(1.0), Cap::Round));
        p.draw_line(joint_x, joint_y, end_x, end_y);

        p.set_pen(Pen::NONE);
        p.set_brush(socket_color);
        p.draw_ellipse_at(root_x, root_y, socket_radius, socket_radius * 0.76);
        p.set_brush(proximal_color);
        p.draw_ellipse_at(root_x, root_y, socket_radius * socket_core_scale, socket_radius * socket_core_scale * 0.74);
        p.set_brush(accent_color);
        p.draw_ellipse_at(joint_x, joint_y, joint_radius, joint_radius * 0.72);
    }
}

/// `_draw_antennae(painter, ceph_offset_x, ceph_w, ceph_h, startle)`.
pub(crate) fn draw_antennae(p: &mut Canvas, cx: &Paint, ceph_offset_x: f64, ceph_w: f64, _ceph_h: f64, startle: f64) {
    let d = cx.d;
    let cfg = &cx.look.antennae;
    if !cfg.enabled {
        return;
    }
    let ceph_h = _ceph_h;
    let size = d.size;
    let hand_palp_style = cfg.style == AntennaStyle::HandPalps;
    let segments = cfg.segments;
    let startle_highlight = d.startle_highlight_active(startle);
    let aiming = if d.dragging { 0.0 } else { clamp(d.aim_intent, 0.0, 1.0) };
    let inspecting = if d.dragging { 0.0 } else { clamp(d.inspect_intent, 0.0, 1.0) };
    let cuddling = if d.dragging { 0.0 } else { clamp(d.cuddle_intent.max(d.catch_blend), 0.0, 1.0) };
    let aim_angle = d.antenna_aim_angle();
    let drive = antenna_drive_from_mood(&d.mood, aiming, inspecting, cuddling, aim_angle);

    let base_x = ceph_offset_x + ceph_w * cfg.base_forward;
    let mut span = cfg.length * size * (1.0 + if d.dragging { 0.0 } else { startle } * 0.08);
    let line_col = cx.qcolor(&cfg.color_key, 240);
    let tip_col = cx.qcolor(&cfg.tip_color_key, 240);

    if cfg.style != AntennaStyle::Feelers {
        let lengths = &cfg.lengths;
        let length_total = lengths.iter().sum::<f64>().max(1e-4);
        let angle_profile = &cfg.angle_profile;
        let width_profile = &cfg.width_profile;
        let proximal_rise = cfg.proximal_rise * size;
        let max_extension;
        if hand_palp_style {
            max_extension = cfg.max_extension;
            span *= clamp(drive.length, cfg.min_drive_length, cfg.max_drive_length);
        } else {
            max_extension = 0.24;
            span *= clamp(drive.length, 0.65, 1.50);
        }
        let limb_scale = size * cfg.thickness * drive.thickness * cfg.leg_thickness;
        for (idx, side_sign) in [-1.0f64, 1.0].into_iter().enumerate() {
            let root_y = side_sign * ceph_h * cfg.base_side;
            let empty: Vec<f64> = Vec::new();
            let side_angles = d.antenna_segment_angles.get(idx).unwrap_or(&empty);
            let side_lifts = d.antenna_joint_lifts.get(idx).unwrap_or(&empty);
            let extension = d.antenna_extension.get(idx).copied().unwrap_or(0.0);
            let (mut fwd, mut lateral) = (0.0, 0.0);
            let mut screen: Vec<(f64, f64)> = vec![(base_x, root_y)];
            for segment_index in 0..segments {
                let mut angle = if side_angles.len() == segments {
                    side_angles[segment_index]
                } else {
                    side_sign * angle_profile[segment_index]
                };
                if drive.aim_blend > 0.01 && !hand_palp_style {
                    angle = angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend;
                }
                let segment =
                    span * (1.0 + clamp(extension, 0.0, max_extension)) * lengths[segment_index] / length_total;
                fwd += angle.cos() * segment;
                lateral += angle.sin() * segment;
                let progress = (segment_index + 1) as f64 / segments.max(1) as f64;
                let rise = if side_lifts.len() == segments {
                    size * clamp(side_lifts[segment_index], 0.0, 0.34)
                } else {
                    proximal_rise * (1.0 - progress).max(0.0)
                };
                screen.push((base_x + fwd, root_y + lateral - side_sign * rise * cfg.screen_lift_scale));
            }

            for segment_index in 0..segments {
                let width = (limb_scale * width_profile[segment_index]).max(0.85);
                let (x1, y1) = screen[segment_index];
                let (x2, y2) = screen[segment_index + 1];
                if cfg.hairy && cfg.hair_scale > 0.0 {
                    let hair_color = cx.triplet(cx.fluff_rgb(), (70.0 + cfg.hair_scale * 95.0) as i64);
                    p.set_pen(pen_full(hair_color, width * (1.16 + cfg.hair_scale * 0.60), Cap::Round, Join::Round));
                    p.draw_line(x1, y1, x2, y2);
                }
                let key = if startle_highlight { "highlight" } else { cfg.segment_color_keys[segment_index].as_str() };
                let segment_color = cx.qcolor(key, 240);
                let cap = if segment_index == segments - 1 { Cap::Flat } else { Cap::Round };
                p.set_pen(pen_full(segment_color, width, cap, Join::Round));
                p.draw_line(x1, y1, x2, y2);
            }

            p.set_pen(Pen::NONE);
            p.set_brush(cx.qcolor(&cfg.joint_color_key, 235));
            for (joint_index, &(jx, jy)) in screen[1..screen.len() - 1].iter().enumerate() {
                let adjacent_width = width_profile[joint_index.min(segments - 1)];
                let mut joint_radius = (limb_scale * adjacent_width * 0.20 * cfg.joint_scale).max(0.85);
                joint_radius *= if joint_index == 0 { 1.08 } else { 0.92 - (joint_index as f64 * 0.05).min(0.25) };
                p.draw_ellipse_at(jx, jy, joint_radius, joint_radius * 0.82);
            }
            if let Some(id) = &d.worn.pedipalps
                && hand_palp_style
                && segments >= 4
            {
                let bracer_w = (limb_scale * width_profile[2]).max(0.85);
                let (a, b) = (screen[2], screen[3]);
                armour_art::paint_bracer(p, a.0, a.1, b.0, b.1, bracer_w, &armour_art::look_for(id));
            }
            let (tx, ty) = *screen.last().unwrap();
            if segments >= 2 {
                let nail_length = cfg.tip_nail_length * span;
                let nail_curl = cfg.tip_curl;
                let n = screen.len();
                let last_dx = screen[n - 1].0 - screen[n - 2].0;
                let last_dy = screen[n - 1].1 - screen[n - 2].1;
                let last_norm = last_dx.hypot(last_dy).max(1e-4);
                let last_angle = last_dy.atan2(last_dx);
                let first_nail =
                    (tx + last_dx / last_norm * nail_length * 0.42, ty + last_dy / last_norm * nail_length * 0.42);
                let curl_angle = last_angle - side_sign * nail_curl;
                let nail_end = (
                    first_nail.0 + curl_angle.cos() * nail_length * 0.58,
                    first_nail.1 + curl_angle.sin() * nail_length * 0.58,
                );
                let claw_color = cx.qcolor(if startle_highlight { "highlight" } else { &cfg.claw_color_key }, 245);
                let tip_radius = (limb_scale * width_profile[width_profile.len() - 1] * 0.24).max(0.75);
                let (ux, uy) = (last_dx / last_norm, last_dy / last_norm);
                let claw_base = (tx + ux * nail_length * 0.12, ty + uy * nail_length * 0.12);
                let (perp_x, perp_y) = (-uy, ux);
                p.set_pen(Pen::NONE);
                p.set_brush(claw_color);
                p.draw_polygon(&[
                    (claw_base.0 + perp_x * tip_radius, claw_base.1 + perp_y * tip_radius),
                    (claw_base.0 - perp_x * tip_radius, claw_base.1 - perp_y * tip_radius),
                    nail_end,
                ]);
            }
        }
        return;
    }

    for (idx, side_sign) in [-1.0f64, 1.0].into_iter().enumerate() {
        let pts = build_antenna_points(side_sign, segments, &drive, d.antenna_phase[idx]);
        let root_y = side_sign * ceph_h * cfg.base_side;
        let screen: Vec<(f64, f64)> = pts.iter().map(|&(f, s)| (base_x + f * span, root_y + s * span)).collect();
        let n = screen.len();
        for j in 0..n - 1 {
            let t = j as f64 / (n as f64 - 2.0).max(1.0);
            let w = (size * cfg.thickness * drive.thickness * (1.0 - 0.55 * t)).max(1.0);
            p.set_pen(pen_full(line_col, w, Cap::Round, Join::Round));
            let (x1, y1) = screen[j];
            let (x2, y2) = screen[j + 1];
            p.draw_line(x1, y1, x2, y2);
        }
        let (tx, ty) = screen[n - 1];
        let r = (size * cfg.thickness * drive.tip_bulb * 0.9).max(1.2);
        p.set_pen(Pen::NONE);
        p.set_brush(tip_col);
        p.draw_ellipse_at(tx, ty, r, r);
    }
}

/// `_draw_eyes(painter, eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_eyes(
    p: &mut Canvas,
    cx: &Paint,
    eyes: &[(f64, f64, f64)],
    ceph_offset_x: f64,
    ceph_w: f64,
    ceph_h: f64,
    startle: f64,
    aiming: f64,
) {
    let d = cx.d;
    let m = &d.mood;
    let blush_amt = clamp(m.affection * 0.75 + cx.look.cute_blush, 0.0, 1.0);
    if blush_amt > 0.05 {
        let col = cx.triplet([255.0, 168.0, 176.0], (30.0 + blush_amt * 80.0) as i64);
        p.set_pen(Pen::NONE);
        p.set_brush(col);
        p.draw_ellipse(rect(ceph_offset_x - ceph_w * 0.05, -ceph_h * 0.32, ceph_w * 0.17, ceph_h * 0.12));
        p.draw_ellipse(rect(ceph_offset_x - ceph_w * 0.05, ceph_h * 0.20, ceph_w * 0.17, ceph_h * 0.12));
    }
    let eye_startle = d.eye_startle_amount(startle);
    let openness =
        clamp(0.62 + m.arousal * 0.5 + aiming * 0.32 - m.sleepy() * 0.55 - m.valence.max(0.0) * 0.16, 0.12, 1.3)
            * (1.0 + eye_startle * 0.4);
    let eff_open = clamp(openness * (1.0 - d.expression_blink * 0.92), 0.05, 1.4);
    let squint_happy = m.valence > 0.45 && m.arousal < 0.72 && aiming < 0.2 && d.expression_blink < 0.4;

    let base_col = cx.qcolor("eyes", 240);
    let pupil_col = cx.triplet([20.0, 18.0, 26.0], 255);
    let shine_col = cx.triplet([255.0, 255.0, 255.0], 235);

    for &(ex, ey, r) in eyes {
        if squint_happy {
            p.set_pen(pen_full(pupil_col, (r * 0.5).max(1.2), Cap::Round, Join::Round));
            p.set_brush(Brush::None);
            let mut arc = Path::new();
            arc.move_to(ex - r, ey + r * 0.28);
            arc.quad_to(ex, ey - r * 0.82, ex + r, ey + r * 0.28);
            p.draw_path(&arc);
            continue;
        }
        p.set_pen(Pen::NONE);
        p.set_brush(base_col);
        p.draw_ellipse(rect(ex - r, ey - r * eff_open, r * 2.0, r * 2.0 * eff_open));
        if eff_open > 0.22 {
            let gx = ex + d.look_fwd * r * 0.40;
            let gy = ey + d.look_side * r * 0.40 * eff_open;
            let pr = r * 0.52;
            p.set_brush(pupil_col);
            p.draw_ellipse_at(gx, gy, pr, pr * eff_open);
            p.set_brush(shine_col);
            p.draw_ellipse_at(gx - pr * 0.32, gy - pr * 0.38 * eff_open, pr * 0.34, pr * 0.34 * eff_open);
        }
    }
}
