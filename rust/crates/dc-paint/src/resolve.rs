//! The geometry half of the Python leg loops, producing [`LegDraw`]s from a
//! live `dc-kinematics` body: `_render_procedural`'s and
//! `_render_sprite_rig`'s per-leg solve (`_leg_draw_points`,
//! `_safe_sprite_leg_foot`, `_solve_knee`, `_leg_lift_px`, the coxa and
//! tarsus points, the jump offset, `_sprite_leg_chain_points`),
//! `_draw_leg_connections`' own chain solve, and `_render_regal`'s raised
//! greeting legs.
//!
//! Every call here is a `dc-kinematics` function; this module only strings
//! them together in the order the Python leg loops did.

use crate::draw::{LegDraw, RegalExpression};
use crate::look::{CreatureLook, RenderMode};
use dc_kinematics::{DrawCtx, FrameCtx, Kinematics};

/// Resolve every leg of one creature for this frame.
pub fn resolve_legs(
    kin: &Kinematics,
    ctx: &FrameCtx,
    draw: &DrawCtx,
    look: &CreatureLook,
    regal: &RegalExpression,
) -> Vec<LegDraw> {
    (0..kin.legs.len()).map(|i| resolve_leg(kin, i, ctx, draw, look, regal)).collect()
}

fn resolve_leg(
    kin: &Kinematics,
    i: usize,
    ctx: &FrameCtx,
    draw: &DrawCtx,
    look: &CreatureLook,
    regal: &RegalExpression,
) -> LegDraw {
    let state = &kin.legs[i];
    let def = &kin.rig.legs[i];
    let size = kin.size;
    let jump_z = draw.jump_z;
    let (attach_x, attach_y) = kin.leg_attach(i);
    let (dax, day, dfx, dfy) = kin.leg_draw_points(i, ctx, draw);
    let mut out = LegDraw {
        leg_foot_x: state.foot_x,
        leg_foot_y: state.foot_y,
        attach_x,
        attach_y,
        lift: state.lift,
        draw_points: [dax, day, dfx, dfy],
        ..LegDraw::default()
    };
    let chain = look.leg_chain.as_ref();

    match look.render_mode {
        RenderMode::Regal => {
            let (ax, ay, mut fx, mut fy) = (dax, day, dfx, dfy);
            let (mut kx, mut ky) = kin.solve_knee(i, ax, ay, fx, fy, ctx);
            let left = def.side == "left";
            let raised = if def.name.starts_with("front_") { if left { regal.left } else { regal.right } } else { 0.0 };
            if raised != 0.0 {
                let side = if left { -1.0 } else { 1.0 };
                let (px, py) = kin.body_local_to_world(size * (1.15 + raised * 0.3), side * size * 0.85);
                fx += (px - fx) * raised;
                fy += (py - fy) * raised - size * raised * 0.52;
                kx += (ax - kx) * raised * 0.15;
                ky -= size * raised * 0.35;
            }
            let lift = kin.leg_lift_px(i, ctx);
            out.regal_points = Some([
                (ax, ay - jump_z),
                (kx, ky - jump_z - lift * 0.45),
                (kx + (fx - kx) * 0.66, ky + (fy - ky) * 0.66 - jump_z - lift * 0.8),
                (fx, fy - jump_z - lift),
            ]);
        }
        RenderMode::SpriteRig => {
            let (ax, mut ay, mut foot_x, mut foot_y) = (dax, day, dfx, dfy);
            (foot_x, foot_y) = kin.safe_sprite_leg_foot(i, foot_x, foot_y, chain.is_some(), ctx, draw);
            let (kx, mut ky) = kin.solve_knee(i, ax, ay, foot_x, foot_y, ctx);
            let lift_px = kin.leg_lift_px(i, ctx);
            if lift_px > 0.0 {
                let flex = state.lift * 0.08;
                foot_x += (ax - foot_x) * flex;
                foot_y += (ay - foot_y) * flex;
                foot_y -= lift_px;
                ky -= lift_px * 0.45;
            }
            if jump_z != 0.0 {
                ay -= jump_z;
                ky -= jump_z;
                foot_y -= jump_z;
            }
            out.ax = ax;
            out.ay = ay;
            out.kx = kx;
            out.ky = ky;
            out.foot_x = foot_x;
            out.foot_y = foot_y;
            out.tarsus_x = kx + (foot_x - kx) * 0.72;
            out.tarsus_y = ky + (foot_y - ky) * 0.72;
            out.coxa_x = ax;
            out.coxa_y = ay;
            if chain.is_some() {
                out.chain_points =
                    kin.sprite_leg_chain_points(i, ax, ay, foot_x, foot_y, ctx).map(|c| c.as_slice().to_vec());
            }
        }
        RenderMode::Procedural => {
            let (ax, mut ay, mut foot_x, mut foot_y) = (dax, day, dfx, dfy);
            let chained = chain.is_some() && look.segmented_legs;
            if chained {
                (foot_x, foot_y) = kin.safe_sprite_leg_foot(i, foot_x, foot_y, true, ctx, draw);
            }
            let (mut kx, mut ky) = kin.solve_knee(i, ax, ay, foot_x, foot_y, ctx);
            let lift_px = kin.leg_lift_px(i, ctx);
            if lift_px > 0.0 {
                let flex = state.lift * 0.08;
                foot_x += (ax - foot_x) * flex;
                foot_y += (ay - foot_y) * flex;
                foot_y -= lift_px;
                ky -= lift_px * 0.45;
            }
            let [_, _, rx, ry] = kin.basis();
            let side = def.side_sign;
            let coxa_len = def.coxa_len.unwrap_or(0.20) * size;
            let mut coxa_x = ax + rx * side * coxa_len + (kx - ax) * 0.08;
            let mut coxa_y = ay + ry * side * coxa_len + (ky - ay) * 0.08;
            let mut tarsus_x = kx + (foot_x - kx) * 0.72;
            let mut tarsus_y = ky + (foot_y - ky) * 0.72;
            if jump_z != 0.0 {
                ay -= jump_z;
                ky -= jump_z;
                coxa_y -= jump_z;
                tarsus_y -= jump_z;
                foot_y -= jump_z;
            }
            if chained && let Some(points) = kin.sprite_leg_chain_points(i, ax, ay, foot_x, foot_y, ctx) {
                let pts = points.as_slice();
                (coxa_x, coxa_y) = pts[1];
                (kx, ky) = pts[2];
                (tarsus_x, tarsus_y) = pts[3];
                out.chain_points = Some(pts.to_vec());
            }
            out.ax = ax;
            out.ay = ay;
            out.kx = kx;
            out.ky = ky;
            out.coxa_x = coxa_x;
            out.coxa_y = coxa_y;
            out.tarsus_x = tarsus_x;
            out.tarsus_y = tarsus_y;
            out.foot_x = foot_x;
            out.foot_y = foot_y;
            if look.leg_connections.is_some() && chain.is_some() {
                let (cax, cay) = kin.leg_attach_drawn(i, draw);
                let (fx, fy) = kin.safe_sprite_leg_foot(i, dfx, dfy, true, ctx, draw);
                out.connection_points = kin
                    .sprite_leg_chain_points(i, cax, cay - jump_z, fx, fy - jump_z, ctx)
                    .map(|c| c.as_slice().to_vec());
            }
        }
    }
    out
}
