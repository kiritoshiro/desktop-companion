"""Dump Qt reference renders and the exact painter inputs for dc-paint.

For every shipped model (and a few stress variants, labels and world
objects) this writes, under ``--out`` (default
``rust/crates/dc-paint/target-review/reference``):

* ``<case>.json``: the draw inputs, with the field names of the Rust
  structs (``CreatureDraw``, ``LegDraw``, ``WebDraw``, ...). Leg geometry is
  computed with the same ``Creature`` methods the Python renderer calls, in
  the same order, so the Rust painter sees exactly the points Qt drew.
* ``<case>.png``: the same frame rendered by the Python painters with Qt.
* ``cases.json``: the index.

Run from the worktree root with the project venv:

    QT_QPA_PLATFORM=offscreen DESKTOP_BUG_GL=0 \\
        .venv/Scripts/python.exe -I rust/crates/dc-paint/tools/dump_reference.py

then ``cargo run --release -p dc-paint --example compare``.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_GL", "0")
# The offscreen platform has no font database of its own: without this the
# label cases render tofu boxes instead of text.
if os.name == "nt":
    os.environ.setdefault("QT_QPA_FONTDIR", os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts"))
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="dc-paint-ref-"))

from PyQt5.QtGui import QImage, QPainter  # noqa: E402
from PyQt5.QtWidgets import QApplication  # noqa: E402

APP = QApplication.instance() or QApplication([])

from desktop_bug.content.discovery import discover_models  # noqa: E402
from desktop_bug.creature import Creature  # noqa: E402
from desktop_bug.creature import render_procedural  # noqa: E402
from desktop_bug.creature.regal import RegalExpression  # noqa: E402
from desktop_bug.state.teams import team_color  # noqa: E402

CELL = 220
DT = 1.0 / 60.0
FRAMES = 45
SEED = 20261018


def personality():
    with (ROOT / "personalities" / "mellow.json").open(encoding="utf-8") as handle:
        return json.load(handle)


# --- armour -----------------------------------------------------------------
# `equipped_items(progression)` is replaced so a stress case can wear every
# slot without building a progression; everything else reads the real code.
# Keyed on id(progression) and holding the progression itself: without the
# reference a freed stress creature's id is reused by a later creature, which
# then wrongly "wears" the stress armour.
WORN: dict[int, tuple] = {}
_real_equipped = render_procedural.equipped_items


def _fake_equipped(progression):
    owner = WORN.get(id(progression))
    if owner is None or owner[0] is not progression:
        return _real_equipped(progression)
    return [SimpleNamespace(slot=slot, id=item) for slot, item in owner[1].items()]


render_procedural.equipped_items = _fake_equipped


def worn_of(c):
    owner = WORN.get(id(c.progression))
    if owner is not None and owner[0] is c.progression:
        return dict(owner[1])
    return {item.slot: item.id for item in _real_equipped(c.progression)}


# --- leg geometry (the leg loops of the three renderers) ---------------------
def pt(p):
    return [float(p[0]), float(p[1])]


def procedural_legs(c):
    chain_config = c._sprite_leg_chain_config()
    segmented = bool(c._appearance("segmented_legs", False))
    leg_y_off = -c.jump_z
    out = []
    for leg in c.legs:
        draw_points = c._leg_draw_points(leg)
        ax, ay, foot_x, foot_y = draw_points
        if chain_config and segmented:
            foot_x, foot_y = c._safe_sprite_leg_foot(leg, foot_x, foot_y, chain_config)
        kx, ky = c._solve_knee(ax, ay, foot_x, foot_y, leg)
        lift_px = c._leg_lift_px(leg)
        if lift_px > 0.0:
            flex = leg.lift * 0.08
            foot_x += (ax - foot_x) * flex
            foot_y += (ay - foot_y) * flex
            foot_y -= lift_px
            ky -= lift_px * 0.45
        _, _, rx, ry = c._basis()
        side = c._side_sign(leg.definition.get("side", "right"))
        coxa_len = float(leg.definition.get("coxa_len", 0.20)) * c.size
        coxa_x = ax + rx * side * coxa_len + (kx - ax) * 0.08
        coxa_y = ay + ry * side * coxa_len + (ky - ay) * 0.08
        tarsus_x = kx + (foot_x - kx) * 0.72
        tarsus_y = ky + (foot_y - ky) * 0.72
        if leg_y_off:
            ay += leg_y_off
            ky += leg_y_off
            coxa_y += leg_y_off
            tarsus_y += leg_y_off
            foot_y += leg_y_off
        chain_points = None
        if chain_config and segmented:
            chain_points = c._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
            coxa_x, coxa_y = chain_points[1]
            kx, ky = chain_points[2]
            tarsus_x, tarsus_y = chain_points[3]
        connection = None
        cfg = c._appearance("leg_connections", {})
        if isinstance(cfg, dict) and cfg.get("enabled", False) is True and chain_config:
            cax, cay = c._leg_attach_drawn(leg)
            cfx, cfy = c._leg_draw_points(leg)[2:]
            cfx, cfy = c._safe_sprite_leg_foot(leg, cfx, cfy, chain_config)
            cay -= c.jump_z
            cfy -= c.jump_z
            connection = [pt(p) for p in c._sprite_leg_chain_points(leg, cax, cay, cfx, cfy, chain_config)]
        out.append(leg_common(c, leg, draw_points) | {
            "ax": ax, "ay": ay, "kx": kx, "ky": ky,
            "coxa_x": coxa_x, "coxa_y": coxa_y, "tarsus_x": tarsus_x, "tarsus_y": tarsus_y,
            "foot_x": foot_x, "foot_y": foot_y,
            "chain_points": [pt(p) for p in chain_points] if chain_points is not None else None,
            "connection_points": connection,
        })
    return out


def sprite_legs(c):
    chain_config = c._sprite_leg_chain_config()
    leg_y_off = -c.jump_z
    out = []
    for leg in c.legs:
        draw_points = c._leg_draw_points(leg)
        ax, ay, foot_x, foot_y = draw_points
        foot_x, foot_y = c._safe_sprite_leg_foot(leg, foot_x, foot_y, chain_config)
        kx, ky = c._solve_knee(ax, ay, foot_x, foot_y, leg)
        lift_px = c._leg_lift_px(leg)
        if lift_px > 0.0:
            flex = leg.lift * 0.08
            foot_x += (ax - foot_x) * flex
            foot_y += (ay - foot_y) * flex
            foot_y -= lift_px
            ky -= lift_px * 0.45
        if leg_y_off:
            ay += leg_y_off
            ky += leg_y_off
            foot_y += leg_y_off
        chain_points = None
        if chain_config:
            chain_points = c._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
        out.append(leg_common(c, leg, draw_points) | {
            "ax": ax, "ay": ay, "kx": kx, "ky": ky, "coxa_x": ax, "coxa_y": ay,
            "tarsus_x": kx + (foot_x - kx) * 0.72, "tarsus_y": ky + (foot_y - ky) * 0.72,
            "foot_x": foot_x, "foot_y": foot_y,
            "chain_points": [pt(p) for p in chain_points] if chain_points is not None else None,
        })
    return out


def regal_legs(c, r):
    size = c.size
    out = []
    for leg in c.legs:
        draw_points = c._leg_draw_points(leg)
        ax, ay, fx, fy = draw_points
        kx, ky = c._solve_knee(ax, ay, fx, fy, leg)
        name = leg.definition.get("name", "")
        raised = (r.left if leg.definition.get("side") == "left" else r.right) if name.startswith("front_") else 0
        if raised:
            side = -1 if leg.definition.get("side") == "left" else 1
            px, py = c._body_local_to_world(size * (1.15 + raised * .3), side * size * .85)
            fx += (px - fx) * raised
            fy += (py - fy) * raised - size * raised * .52
            kx += (ax - kx) * raised * .15
            ky -= size * raised * .35
        lift = c._leg_lift_px(leg)
        points = [(ax, ay - c.jump_z), (kx, ky - c.jump_z - lift * .45),
                  (kx + (fx - kx) * .66, ky + (fy - ky) * .66 - c.jump_z - lift * .8),
                  (fx, fy - c.jump_z - lift)]
        out.append(leg_common(c, leg, draw_points) | {"regal_points": [pt(p) for p in points]})
    return out


def leg_common(c, leg, draw_points):
    ax, ay = c._leg_attach(leg)
    return {
        "leg_foot_x": leg.foot_x, "leg_foot_y": leg.foot_y,
        "attach_x": ax, "attach_y": ay, "lift": leg.lift,
        "draw_points": [float(v) for v in draw_points],
    }


# --- the creature snapshot ---------------------------------------------------
def regal_of(c):
    r = getattr(c, "regal", RegalExpression())
    if c._regal_blocked():
        r = RegalExpression()
    return r


def creature_draw(c):
    render_mode = str(c.model.get("render_mode", "procedural")).lower()
    r = regal_of(c)
    if c._is_regal():
        legs = regal_legs(c, r)
    elif render_mode == "sprite_rig" and c._load_sprite_assets():
        legs = sprite_legs(c)
    else:
        legs = procedural_legs(c)
    m = c.mood
    team_id = str(getattr(c.progression, "team_id", "neutral") or "neutral")
    overrides = getattr(c, "color_overrides", None) or {}
    camo = c._camouflage_color
    return {
        "x": c.x, "y": c.y, "size": c.size, "heading": c.heading, "jump_z": c.jump_z,
        "body_bob": c.body_bob, "body_wiggle": c.body_wiggle, "breath_phase": c.breath_phase,
        "startle_phase": c.startle_phase, "crouch": c.crouch, "squash": c.squash,
        "roll_spin": c.roll_spin, "rear": c.rear, "abdomen_pulse": c.abdomen_pulse,
        "ceph_pulse": c.ceph_pulse, "abdomen_wag": c.abdomen_wag,
        "combat_stance": float(getattr(c, "combat_stance", 0.0)),
        "combat_body_offset": list(c.combat_body_offset()),
        "state": str(c.state), "spider_gait": c._spider_gait_config() is not None,
        "dragging": bool(c.dragging), "dead": bool(c.dead), "current_speed": c.current_speed,
        "startled_timer": c.startled_timer,
        "startle_highlight_suppression": c._startle_highlight_suppression,
        "aim_intent": c.aim_intent, "inspect_intent": c.inspect_intent,
        "cuddle_intent": c.cuddle_intent, "catch_blend": c.catch_blend,
        "expression_blink": c.expression_blink, "look_fwd": c.look_fwd, "look_side": c.look_side,
        "mood": {"valence": m.valence, "arousal": m.arousal, "affection": m.affection,
                 "curiosity": m.curiosity},
        "antenna_phase": list(c.antenna_phase),
        "antenna_segment_angles": [list(v) for v in getattr(c, "antenna_segment_angles", [[], []])],
        "antenna_joint_lifts": [list(v) for v in getattr(c, "antenna_joint_lifts", [[], []])],
        "antenna_extension": list(getattr(c, "antenna_extension", [0.0, 0.0])),
        "focus_x": c.focus_x, "focus_y": c.focus_y,
        "antenna_hand_targets": [pt(p) for p in c.antenna_hand_targets],
        "regal": {k: getattr(r, k) for k in ("yaw", "pitch", "tilt", "left", "right", "bounce", "palps")},
        "colors": {k: [float(v) for v in val[:3]] for k, val in c.colors.items()
                   if isinstance(val, (list, tuple)) and len(val) >= 3},
        "color_overrides": [str(k) for k in overrides],
        "camouflage_color": [float(v) for v in camo] if camo is not None else None,
        "camouflage_strength": float(c._camouflage_strength),
        "camouflage_color_blend": float(c.personality.get("camouflage_color_blend", 0.0) or 0.0),
        "camouflage_opacity_drop": float(c.personality.get("camouflage_opacity_drop", 0.72)),
        "worn": worn_of(c),
        "legs": legs,
        "webbed": bool(c.webbed), "web_net": [
            {"angle": s["angle"], "rings": list(s["rings"]), "anchor": pt(s["anchor"]),
             "tear": s["tear"], "ring_tears": list(s["ring_tears"])} for s in (c.web_net or [])],
        "web_anchors": [pt(a) for a in (c.web_anchors or [])],
        "struggle": c.struggle, "struggle_clock": c.struggle_clock, "webbed_timer": c.webbed_timer,
        "label": {
            "name": c.name, "display_name": c.display_name, "level": int(c.level),
            "label_style": c.label_style,
            "level_label_pinned": bool(c.level_label_pinned),
            "health_label_pinned": bool(c.health_label_pinned),
            "xp_label_pinned": bool(c.xp_label_pinned),
            "stamina_label_pinned": bool(c.stamina_label_pinned),
            "hovered": bool(c._hovered), "team_id": team_id,
            "team_color": list(team_color(team_id, getattr(c, "team_profiles", None))),
            "hp": float(c.hp), "max_hp": float(c.max_hp), "energy": float(c.energy),
            "max_energy": float(c.max_energy), "xp": float(c.xp),
        },
        "damage_numbers": [list(map(float, n)) for n in c.damage_numbers],
        "damage_team": getattr(c, "damage_team", None),
    }


def model_json(c):
    keep = ("id", "render_mode", "appearance", "_folder", "display_name")
    return {k: c.model[k] for k in keep if k in c.model}


def render_creature(c, w, h, tx, ty, always_show_names=False, numbers=False):
    image = QImage(w, h, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    painter.translate(tx, ty)
    c.render(painter, always_show_names=always_show_names, numbers=numbers)
    painter.end()
    return image


def make_creature(model, index, overrides=None):
    rng_seed = SEED + index * 7919
    random.seed(rng_seed)
    c = Creature(model, personality(), 960, 640, index=index, seed=rng_seed,
                 color_overrides=overrides)
    c.x, c.y = 480.0, 320.0
    for _ in range(FRAMES):
        c.update(DT, -100000.0, -100000.0, 960, 640)
    return c


def write_case(out, name, kind, w, h, payload, image):
    (out / f"{name}.json").write_text(json.dumps(payload | {"name": name, "kind": kind, "w": w, "h": h}),
                                      encoding="utf-8")
    image.save(str(out / f"{name}.png"))
    return {"name": name, "kind": kind, "w": w, "h": h}


def creature_case(out, name, c, family, always_show_names=False, numbers=False, w=CELL, h=CELL):
    tx, ty = w * 0.5 - c.x, h * 0.55 - c.y
    payload = {
        "family": family,
        "translate": [tx, ty],
        "always_show_names": always_show_names,
        "numbers": numbers,
        "model": model_json(c),
        "draw": creature_draw(c),
    }
    image = render_creature(c, w, h, tx, ty, always_show_names, numbers)
    return write_case(out, name, "creature", w, h, payload, image)


def family_of(model):
    mid = model["id"]
    app = model.get("appearance", {})
    if app.get("species_profile") in ("regal_fluff", "regal_scout"):
        return "regal"
    if mid.startswith("enemy_"):
        return "enemy"
    if str(model.get("render_mode", "procedural")).lower() == "sprite_rig":
        return "sprite"
    if "tarantula" in mid or (isinstance(app.get("leg_chain"), dict) and app["leg_chain"].get("enabled")):
        return "tarantula/chained"
    return "procedural"


def stress(c):
    """Startle, armour on every slot, a net, a hop, camouflage, blush."""
    c.startled_timer = 0.8
    c._startle_highlight_suppression = 0.0
    WORN[id(c.progression)] = (c.progression, {"abdomen": "warden_abdomen", "carapace": "sun_carapace",
                                               "head": "brood_helm", "legs": "frost_greaves",
                                               "pedipalps": "pedipalp_cuffs"})
    c.webbed_timer = 3.0
    c._pin_net()
    c.struggle = 0.6
    c.struggle_clock = 0.37
    c.jump_z = 4.0
    c.personality = dict(c.personality, camouflage_color_blend=0.55)
    c._camouflage_color = (96, 140, 92)
    c._camouflage_strength = 0.35
    c.mood.affection = 0.9
    c.combat_stance = 0.5
    c.rear = 0.4


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(ROOT / "rust/crates/dc-paint/target-review/reference"))
    parser.add_argument("--only", default="")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    models, warnings = discover_models(ROOT)
    for warning in warnings:
        print("warning:", warning)
    cases = []
    ids = sorted(models)
    if args.only:
        ids = [i for i in ids if args.only in i]
    stress_ids = [i for i in ids if i in (
        "tarantula", "black_tarantula_hybrid", "chosen_one", "spider", "fluffy_spider",
        "plush_curly_hybrid", "jumping_spider_hybrid", "regal_fluff", "regal_scout",
        "enemy_crab_reaver", "enemy_thorn_crown", "silk_peacock_jumper", "cute_spider")]
    for index, mid in enumerate(ids):
        model = models[mid]
        c = make_creature(model, index)
        cases.append(creature_case(out, f"rest__{mid}", c, family_of(model)) | {"family": family_of(model)})
        if mid in stress_ids:
            c2 = make_creature(model, index + 500)
            stress(c2)
            cases.append(creature_case(out, f"stress__{mid}", c2, family_of(model)) | {"family": family_of(model)})
        print(mid, family_of(model), flush=True)
    # A tinted sprite rig (DC-48) and the enemy shade.
    for mid, overrides in (("plush_curly_hybrid", {"body": [70, 140, 220], "legs": [200, 90, 60]}),
                           ("cocoa_plush", {"shade_to": [60, 12, 14]})):
        if mid in models:
            c = make_creature(models[mid], 900, overrides)
            cases.append(creature_case(out, f"tint__{mid}", c, "sprite") | {"family": "sprite"})
    # Labels and damage numbers (text: visual check only).
    if "tarantula" in models and not args.only:
        for style in ("full", "quiet", "hero"):
            c = make_creature(models["tarantula"], 950)
            c.name = "Rosie"
            c.label_style = style
            c.force_show_level = True
            c.force_show_health = True
            c.force_show_xp = True
            c.force_show_stamina = True
            c.hp = 37.0
            c.energy = 62.0
            c.damage_numbers = [[12.4, 0.2, 0.0], [0.45, 0.6, 9.0]]
            cases.append(creature_case(out, f"label__{style}", c, "label", True, True, 260, 260)
                         | {"family": "label"})
    if not args.only:
        cases.extend(world_cases(out))
    (out / "cases.json").write_text(json.dumps(cases, indent=1), encoding="utf-8")
    print(f"{len(cases)} cases -> {out}")


# --- world objects -----------------------------------------------------------
def render_world(w, h, draw):
    image = QImage(w, h, QImage.Format_ARGB32_Premultiplied)
    image.fill(0)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    draw(painter)
    painter.end()
    return image


def strand_json(s):
    return {"points": [pt(p) for p in s.points], "draw": s.draw, "kind": s.kind, "temporary": s.temporary}


def web_json(web):
    return {"strands": [strand_json(s) for s in web.strands], "hub": pt(web.hub),
            "wobble_dir": pt(web.wobble_dir), "built": web.built, "active_t": web.active_t,
            "state": web.state, "cut": [list(c) for c in sorted(web.cut)],
            "bounce_amp": web.bounce_amp, "bounce_phase": web.bounce_phase,
            "bounce_origin": pt(web.bounce_origin)}


def world_cases(out):
    from desktop_bug.world import webs as webs_mod
    from desktop_bug.world.webs import Web
    from desktop_bug.world.flies import Fly, FlyRemains, FlySpawner, WebShotProjectile
    from desktop_bug.world import mouse_webs as mw
    from desktop_bug.world.jobs import BaseSite, BaseWorld
    from desktop_bug.world.cage import Cage
    from desktop_bug.world.carcass import Carcass

    cases = []
    rng = random.Random(77)
    W, H = 640, 420
    # Webs: an orb being built, a finished torn wobbling orb, a funnel, a tangle.
    webs = []
    strands, hub, wob = webs_mod._plan_orb((150.0, 140.0), 95.0, W, H, rng=rng)
    web = Web("orb", strands, hub, wob, "a", 30.0, W, H, rng=rng)
    web.built = len(strands) * 2 // 3
    web.active_t = 0.4
    webs.append(web)
    strands, hub, wob = webs_mod._plan_orb((420.0, 150.0), 100.0, W, H, rng=rng)
    web = Web("orb", strands, hub, wob, "b", 30.0, W, H, rng=rng)
    web.built = len(strands)
    web.state = "complete"
    web.damage_near(hub[0] + 40.0, hub[1] + 10.0, 22.0)
    web.pluck((hub[0] - 20.0, hub[1]), 1.0)
    for _ in range(3):
        web.update(DT)
    webs.append(web)
    strands, hub, wob = webs_mod._plan_funnel((0.0, H), (0.7071, -0.7071), 140.0, W, H, rng=rng)
    web = Web("funnel", strands, hub, wob, "c", 30.0, W, H, rng=rng)
    web.built = len(strands)
    web.state = "complete"
    webs.append(web)
    strands, hub, wob = webs_mod._plan_tangle((W, H), (-0.7071, -0.7071), 140.0, W, H, rng=rng)
    web = Web("tangle", strands, hub, wob, "d", 30.0, W, H, rng=rng)
    web.built = len(strands) // 2
    web.active_t = 0.7
    webs.append(web)
    payload = {"family": "world", "webs": [web_json(w) for w in webs]}
    image = render_world(W, H, lambda p: [w.draw(p) for w in webs])
    cases.append(write_case(out, "world__webs", "webs", W, H, payload, image) | {"family": "world"})

    # Flies: nest, remains, flying, trapped, eaten, a silk glob.
    frng = random.Random(5)
    spawner = FlySpawner(90.0, 300.0, rng=frng)
    remains = FlyRemains(220.0, 320.0, scale=1.3, rng=frng)
    remains.t = 1.1
    flying = Fly(330.0, 120.0, W, H, heading=0.7, rng=frng)
    flying.wing_phase = 1.1
    trapped = Fly(460.0, 250.0, W, H, heading=-0.4, scale=1.4, rng=frng)
    trapped.pin_with_tether((400.0, 300.0))
    trapped._make_web_pegs()
    trapped.struggle_phase = 0.8
    trapped._draw_jitter = 1.2
    eaten = Fly(560.0, 120.0, W, H, rng=frng)
    eaten.begin_eaten()
    eaten.eat_t = 0.15
    target = Fly(600.0, 360.0, W, H, heading=2.0, rng=frng)
    shooter = SimpleNamespace(x=380.0, y=380.0, heading=0.2, size=24.0)
    shot = WebShotProjectile(shooter, target)
    for _ in range(6):
        shot.update(DT * 0.5)
    flies = [flying, trapped, eaten]

    def draw_flies(p):
        spawner.render(p)
        remains.render(p)
        for f in flies:
            f.render(p)
        shot.render(p)

    def fly_json(f):
        return {"x": f.x, "y": f.y, "length": f.length, "heading": f.heading, "state": f.state,
                "struggle_phase": f.struggle_phase, "wing_phase": f.wing_phase, "eat_t": f.eat_t,
                "draw_jitter": getattr(f, "_draw_jitter", 0.0),
                "web_pegs": [pt(p) for p in (getattr(f, "_web_pegs", None) or [])],
                "tether_from": pt(f.tether_from) if f.tether_from is not None else None}

    payload = {"family": "world", "flies": {
        "spawners": [{"x": spawner.x, "y": spawner.y, "radius": spawner.radius, "pulse": spawner.pulse,
                      "specks": [list(s) for s in spawner.specks]}],
        "remains": [{"x": remains.x, "y": remains.y, "scale": remains.scale, "t": remains.t,
                     "parts": remains.parts}],
        "flies": [fly_json(f) for f in flies],
        "projectiles": [{"trail": [pt(p) for p in shot.trail], "pos": pt(shot.pos)}],
    }}
    cases.append(write_case(out, "world__flies", "flies", W, H, payload, render_world(W, H, draw_flies))
                 | {"family": "world"})

    # Cursor silk: a spent splat, a trap mid-struggle, a wall shove, a glob.
    mrng = random.Random(9)
    spent = mw._SpentSplat((80.0, 80.0), rng=mrng)
    spent.t = 0.12
    trap = mw.TrapCapture((220.0, 200.0), rng=mrng)
    trap.hold.struggle = 300.0
    trap.hold.strain = (236.0, 214.0)
    trap.hold.wobble = 2.5
    shove = mw.ShoveCapture((420.0, 300.0), (1.0, 0.3), W, H, rng=mrng)
    for _ in range(5):
        shove.update(DT, 420.0, 300.0, True)
    proj = mw._Projectile((300.0, 380.0), (520.0, 120.0), "trap")
    for _ in range(5):
        proj.update(DT * 0.5, 520.0, 120.0)

    def hold_json(h):
        return {"anchor": pt(h.anchor), "leash": h.leash, "escape": h.escape, "struggle": h.struggle,
                "strain": pt(h.strain), "wobble": h.wobble,
                "pegs": [[pt(off), base] for off, base in h.pegs]}

    def draw_mouse(p):
        spent.draw(p)
        trap.draw(p)
        shove.draw(p)
        proj.draw(p)

    payload = {"family": "world", "mouse_webs": [
        {"spent": [{"at": pt(spent.at), "t": spent.t, "spokes": [pt(s) for s in spent.spokes]}],
         "capture": {"kind": "trap", "hold": hold_json(trap.hold)}, "projectile": None},
        {"spent": [], "capture": {"kind": "wall", "phase": shove.phase, "trail": [pt(p) for p in shove.trail],
                                  "cur": pt(shove.cur), "wall": pt(shove.wall),
                                  "hold": hold_json(shove.hold) if shove.hold else None},
         "projectile": {"trail": [pt(p) for p in proj.trail], "pos": pt(proj.pos)}},
    ]}
    cases.append(write_case(out, "world__mouse_webs", "mouse_webs", W, H, payload,
                            render_world(W, H, draw_mouse)) | {"family": "world"})

    # Bases: a part-dug neutral base, a finished team base, one on alert.
    bw = BaseWorld(W, H)
    for site in (BaseSite("base-a", "o1", "neutral", 140.0, 220.0, build_progress=40.0, level=1),
                 BaseSite("base-b", "o2", "rivals", 330.0, 200.0, build_progress=120.0, level=3),
                 BaseSite("base-c", "o3", "colony", 520.0, 230.0, build_progress=75.0, level=2, alert=0.6)):
        bw.bases[site.id] = site
    payload = {"family": "world", "bases": [
        {"id": s.id, "team_id": s.team_id, "x": s.x, "y": s.y, "build_progress": s.build_progress,
         "level": s.level, "alert": s.alert, "team_color": list(team_color(s.team_id, bw.team_profiles))}
        for s in bw.bases.values()]}
    cases.append(write_case(out, "world__bases", "bases", W, H, payload, render_world(W, H, bw.render))
                 | {"family": "world"})

    # Cages and carcasses.
    cage_a = Cage(30.0, 30.0, 260.0, 180.0)
    cage_b = Cage(330.0, 40.0, 220.0, 160.0)
    carcass_a = Carcass(160.0, 320.0, 30.0, "neutral", {"body": (90, 60, 40)})
    carcass_a.t = 3.0
    carcass_b = Carcass(420.0, 330.0, 24.0, "rivals")
    carcass_b.t = 9.0

    def draw_props(p):
        cage_a.draw(p, member_count=0, active=False)
        cage_b.draw(p, member_count=0, active=True)
        carcass_a.draw(p, numbers=False)
        carcass_b.draw(p, numbers=False)

    payload = {"family": "world",
               "cages": [{"x": c.x, "y": c.y, "w": c.w, "h": c.h, "member_count": 0, "active": a}
                         for c, a in ((cage_a, False), (cage_b, True))],
               "carcasses": [{"x": k.x, "y": k.y, "size": k.size, "t": k.t,
                              "body": list(k.colors["body"]) if k.colors.get("body") else None,
                              "legs": [list(l) for l in k.legs], "team_id": k.team_id}
                             for k in (carcass_a, carcass_b)]}
    cases.append(write_case(out, "world__props", "props", W, H, payload, render_world(W, H, draw_props))
                 | {"family": "world"})
    return cases


if __name__ == "__main__":
    main()
