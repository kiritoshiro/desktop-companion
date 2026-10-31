//! Painting the manager's frame (`CreatureManager.render`). Python painted
//! straight off the live objects; here each one is copied into dc-paint's
//! plain draw structs and `dc_paint::scene::render_scene` lays the layers
//! down in Python's order.

use crate::CreatureManager;
use dc_creature::Creature;
use dc_kinematics::{DrawCtx, PlayfieldClamp};
use dc_paint::PaintSwitches;
use dc_paint::color::Palette;
use dc_paint::creature::CreaturePainter;
use dc_paint::draw::{CreatureDraw, LabelDraw, Mood, NetSpoke, RegalExpression, Worn};
use dc_paint::look::CreatureLook;
use dc_paint::resolve::resolve_legs;
use dc_paint::scene::{CreatureEntry, SceneDraw, render_scene};
use dc_paint::world::{
    BaseDraw, CageDraw, CaptureDraw, CarcassDraw, Clip, FlyDraw, FlyRemainsDraw, FlySpawnerDraw, FlyWorldDraw,
    MouseShotDraw, MouseWebDraw, RemainsPart, SpentSplatDraw, StickyHoldDraw, StrandDraw, WebDraw, WebShotDraw,
};
use dc_render::Canvas;
use dc_world::flies::{PartKind, Projectile};
use dc_world::mouse_webs::{Capture, StickyHold};
use dc_world::webs::WebState;
use std::collections::HashMap;

/// Keeps what lasts between frames: the sprite cache and each model's look.
pub struct ScenePainter {
    painter: CreaturePainter,
    looks: HashMap<String, CreatureLook>,
}

impl ScenePainter {
    pub fn new(switches: PaintSwitches) -> Self {
        ScenePainter { painter: CreaturePainter::new(switches), looks: HashMap::new() }
    }

    /// `CreatureManager.render(painter)`, with `clip` the repaint rectangle
    /// grown by 2 px (`None` for the whole overlay).
    pub fn paint(&mut self, m: &mut CreatureManager, canvas: &mut Canvas, clip: Option<Clip>) {
        // Python's "render-props" and "render" sections in one: the props
        // and the spiders are laid down by the same `render_scene` call.
        let _span = dc_core::profiling::section("render");
        let order: Vec<u64> =
            if m.render_order.is_empty() { m.creatures.iter().map(|c| c.id).collect() } else { m.render_order.clone() };
        let mut draws = Vec::with_capacity(order.len());
        let mut occlusion = Vec::with_capacity(order.len());
        let mut model_ids = Vec::with_capacity(order.len());
        for id in order {
            let Some(i) = m.creatures.iter().position(|c| c.id == id) else { continue };
            // Python's render asked the same question, which can end a hide.
            occlusion.push(m.occlusion_at(i));
            let c = &m.creatures[i];
            let model_id = c.model.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let look = self
                .looks
                .entry(model_id.clone())
                .or_insert_with(|| CreatureLook::from_model_json(&c.model, dc_creature::creature::CURVED_LEGS));
            draws.push(creature_draw(c, look));
            model_ids.push(model_id);
        }
        let entries: Vec<CreatureEntry> = draws
            .iter()
            .zip(occlusion)
            .zip(&model_ids)
            .map(|((draw, (fully_covered, hidden_regions)), id)| CreatureEntry {
                draw,
                look: &self.looks[id],
                fully_covered,
                hidden_regions,
            })
            .collect();
        let world = world_draw(m);
        let scene = SceneDraw {
            screen_w: m.screen_w,
            screen_h: m.screen_h,
            always_show_names: m.always_show_names,
            clip,
            cages: &world.cages,
            bases: &world.bases,
            webs: &world.webs,
            mouse_webs: &world.mouse_webs,
            flies: &world.flies,
            carcasses: &world.carcasses,
            creatures: &entries,
        };
        render_scene(canvas, &mut self.painter, &scene);
    }
}

/// One creature's draw struct, legs resolved for this frame.
pub fn creature_draw(c: &Creature, look: &CreatureLook) -> CreatureDraw {
    let s = c.render_snapshot();
    let regal = RegalExpression {
        yaw: s.regal.yaw,
        pitch: s.regal.pitch,
        tilt: s.regal.tilt,
        left: s.regal.left,
        right: s.regal.right,
        bounce: s.regal.bounce,
        palps: s.regal.palps,
    };
    let playfield = c.split_playfield();
    let ctx = c.kctx(playfield.as_ref().map(|p| p as &dyn PlayfieldClamp));
    let (strike_push, strike_lift) = c.strike_phase();
    let draw = DrawCtx {
        combat_body_offset: s.combat_body_offset,
        combat_stance: c.combat_stance,
        strike_push,
        strike_lift,
        strike_face: c.strike_face,
        combat_face_x: c.combat_face_x,
        combat_face_y: c.combat_face_y,
        feed_paw: c.feed_paw,
        jump_z: c.jump_z,
        jump_peak: c.jump_peak,
    };
    let legs = resolve_legs(&c.kin, &ctx, &draw, look, &regal);
    let l = &s.label;
    CreatureDraw {
        x: s.x,
        y: s.y,
        size: s.size,
        heading: s.heading,
        jump_z: s.jump_z,
        body_bob: s.body_bob,
        body_wiggle: s.body_wiggle,
        breath_phase: s.breath_phase,
        startle_phase: s.startle_phase,
        crouch: s.crouch,
        squash: s.squash,
        roll_spin: s.roll_spin,
        rear: s.rear,
        abdomen_pulse: s.abdomen_pulse,
        ceph_pulse: s.ceph_pulse,
        abdomen_wag: s.abdomen_wag,
        combat_stance: s.combat_stance,
        combat_body_offset: s.combat_body_offset,
        state: s.state.clone(),
        spider_gait: s.spider_gait,
        dragging: s.dragging,
        dead: s.dead,
        current_speed: s.current_speed,
        startled_timer: s.startled_timer,
        startle_highlight_suppression: s.startle_highlight_suppression,
        aim_intent: s.aim_intent,
        inspect_intent: s.inspect_intent,
        cuddle_intent: s.cuddle_intent,
        catch_blend: s.catch_blend,
        expression_blink: s.expression_blink,
        look_fwd: s.look_fwd,
        look_side: s.look_side,
        mood: Mood {
            valence: s.mood.valence,
            arousal: s.mood.arousal,
            affection: s.mood.affection,
            curiosity: s.mood.curiosity,
        },
        antenna_phase: s.antenna_phase,
        antenna_segment_angles: s.antenna_segment_angles.clone(),
        antenna_joint_lifts: s.antenna_joint_lifts.clone(),
        antenna_extension: s.antenna_extension.clone(),
        focus_x: s.focus_x,
        focus_y: s.focus_y,
        antenna_hand_targets: s.antenna_hand_targets.clone(),
        regal,
        palette: Palette {
            colors: s
                .palette
                .colors
                .iter()
                .map(|(k, [r, g, b])| (k.clone(), [*r as f64, *g as f64, *b as f64]))
                .collect(),
            color_overrides: s.palette.color_overrides.clone(),
            camouflage_color: s.palette.camouflage_color.map(|[r, g, b]| [r as f64, g as f64, b as f64]),
            camouflage_strength: s.palette.camouflage_strength,
            camouflage_color_blend: s.palette.camouflage_color_blend,
        },
        camouflage_opacity_drop: s.camouflage_opacity_drop,
        worn: Worn::from_pairs(s.worn.iter().map(|(slot, id)| (slot.as_str(), id.as_str()))),
        legs,
        webbed: s.webbed,
        web_net: s
            .web_net
            .iter()
            .map(|n| NetSpoke {
                angle: n.angle,
                rings: n.rings.to_vec(),
                anchor: n.anchor,
                tear: n.tear,
                ring_tears: n.ring_tears.to_vec(),
            })
            .collect(),
        web_anchors: s.web_anchors.clone(),
        struggle: s.struggle,
        struggle_clock: s.struggle_clock,
        webbed_timer: s.webbed_timer,
        label: LabelDraw {
            name: l.name.clone(),
            display_name: l.display_name.clone(),
            level: l.level,
            label_style: l.label_style.clone(),
            level_label_pinned: l.level_label_pinned,
            health_label_pinned: l.health_label_pinned,
            xp_label_pinned: l.xp_label_pinned,
            stamina_label_pinned: l.stamina_label_pinned,
            hovered: l.hovered,
            team_id: l.team_id.clone(),
            team_color: l.team_color,
            hp: l.hp,
            max_hp: l.max_hp,
            energy: l.energy,
            max_energy: l.max_energy,
            xp: l.xp,
        },
        damage_numbers: s.damage_numbers.clone(),
        damage_team: s.damage_team.clone(),
    }
}

/// Everything below and around the spiders, as draw structs.
pub struct WorldDraw {
    pub cages: Vec<CageDraw>,
    pub bases: Vec<BaseDraw>,
    pub webs: Vec<WebDraw>,
    pub mouse_webs: MouseWebDraw,
    pub flies: FlyWorldDraw,
    pub carcasses: Vec<CarcassDraw>,
}

fn unit(dx: f64, dy: f64) -> (f64, f64) {
    let m = dx.hypot(dy);
    if m <= 1e-9 { (1.0, 0.0) } else { (dx / m, dy / m) }
}

fn hold_draw(h: &StickyHold) -> StickyHoldDraw {
    StickyHoldDraw {
        anchor: h.anchor,
        leash: h.leash,
        escape: h.escape,
        struggle: h.struggle,
        strain: h.strain,
        wobble: h.wobble,
        pegs: h.pegs.clone(),
    }
}

pub fn world_draw(m: &CreatureManager) -> WorldDraw {
    let mut counts = vec![0usize; m.cages.len()];
    for c in &m.creatures {
        if let Some(count) = c.cage.and_then(|r| counts.get_mut(r.id as usize)) {
            *count += 1;
        }
    }
    let active = m.cage_drag.map(|d| d.index);
    let cages = m
        .cages
        .iter()
        .enumerate()
        .map(|(i, c)| CageDraw { x: c.x, y: c.y, w: c.w, h: c.h, member_count: counts[i], active: active == Some(i) })
        .collect();
    let bases = m
        .base_world
        .bases
        .iter()
        .map(|b| {
            let color = dc_state::teams::team_color(&b.team_id, Some(&m.team_profiles));
            BaseDraw {
                id: b.id.clone(),
                team_id: b.team_id.clone(),
                x: b.x,
                y: b.y,
                build_progress: b.build_progress,
                level: b.level,
                alert: b.alert,
                team_color: (color.r, color.g, color.b),
            }
        })
        .collect();
    let webs = m
        .web_world
        .webs
        .iter()
        .map(|w| WebDraw {
            strands: w
                .strands
                .iter()
                .map(|s| StrandDraw {
                    points: s.points.clone(),
                    draw: s.draw,
                    kind: s.kind.as_str().to_string(),
                    temporary: s.temporary,
                })
                .collect(),
            hub: w.hub,
            wobble_dir: unit(w.wobble_dir.0, w.wobble_dir.1),
            built: w.built,
            active_t: w.active_t,
            state: match w.state {
                WebState::Building => "building",
                WebState::Complete => "complete",
            }
            .to_string(),
            cut: w.cut.iter().copied().collect(),
            bounce_amp: w.bounce_amp,
            bounce_phase: w.bounce_phase,
            bounce_origin: w.bounce_origin,
        })
        .collect();
    let mw = &m.mouse_web_world;
    let mouse_webs = MouseWebDraw {
        spent: mw.spent.iter().map(|s| SpentSplatDraw { at: s.at, t: s.t, spokes: s.spokes.clone() }).collect(),
        capture: mw.capture.as_ref().map(|c| match c {
            Capture::Trap(hold) => CaptureDraw::Trap { hold: hold_draw(hold) },
            Capture::Wall(w) => CaptureDraw::Wall {
                phase: if w.shoving { "shove" } else { "pin" }.to_string(),
                trail: w.trail.clone(),
                cur: w.cur,
                wall: w.wall,
                hold: w.hold.as_ref().map(hold_draw),
            },
        }),
        projectile: mw.projectile.as_ref().map(|p| MouseShotDraw { trail: p.trail.clone(), pos: p.pos }),
    };
    let fw = &m.fly_world;
    let flies = FlyWorldDraw {
        spawners: fw
            .spawners
            .iter()
            .map(|s| FlySpawnerDraw { x: s.x, y: s.y, radius: s.radius, pulse: s.pulse, specks: s.specks.clone() })
            .collect(),
        remains: fw
            .remains
            .iter()
            .map(|r| FlyRemainsDraw {
                x: r.x,
                y: r.y,
                scale: r.scale,
                t: r.t,
                parts: r
                    .parts
                    .iter()
                    .map(|p| RemainsPart {
                        x: p.x,
                        y: p.y,
                        rot: p.rot,
                        len: p.len,
                        kind: match p.kind {
                            PartKind::Leg => "leg",
                            PartKind::Wing => "wing",
                            PartKind::Crumb => "crumb",
                        }
                        .to_string(),
                    })
                    .collect(),
            })
            .collect(),
        flies: fw
            .flies
            .iter()
            .map(|f| FlyDraw {
                x: f.x,
                y: f.y,
                length: f.length,
                heading: f.heading,
                state: f.state.as_str().to_string(),
                struggle_phase: f.struggle_phase,
                wing_phase: f.wing_phase,
                eat_t: f.eat_t,
                draw_jitter: f.draw_jitter,
                web_pegs: f.web_pegs.clone(),
                tether_from: f.tether_from,
            })
            .collect(),
        projectiles: fw
            .projectiles
            .iter()
            .map(|p| match p {
                Projectile::Shot(s) => WebShotDraw { trail: s.trail.clone(), pos: s.pos },
                Projectile::Aimed(a) => WebShotDraw { trail: a.trail.clone(), pos: a.pos },
            })
            .collect(),
    };
    let carcasses = m
        .carcasses
        .iter()
        .map(|c| CarcassDraw {
            x: c.x,
            y: c.y,
            size: c.size,
            t: c.t,
            body: c.body_color,
            legs: c.legs.to_vec(),
            damage_numbers: c.damage_numbers.iter().map(|n| (n.amount, n.age, n.nudge)).collect(),
            damage_team: c.damage_team.clone(),
            team_id: c.team_id.clone(),
        })
        .collect();
    WorldDraw { cages, bases, webs, mouse_webs, flies, carcasses }
}
