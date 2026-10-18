"""Dump parity fixtures for dc-kinematics from the Python reference.

Run from anywhere with the project venv:

    C:/Users/win/Documents/Workspace/projects/desktop-companion/.venv/Scripts/python.exe \
        rust/crates/dc-kinematics/tools/dump_fixtures.py

Each scenario builds a real `Creature`, then drives it frame by frame at
dt = 1/60 through a scripted motion. The kinematic calls under test draw from
their own seeded `random.Random(KIN_SEED)`; everything else (the scenario
script, and any FSM method such as `_update_drift_run` or `enter_roll`) runs
with a separate generator, so the Rust replay only has to reproduce the
kinematics' own draws. Any kinematic state the script changes between steps
is recorded as a `pre` patch the replay applies before the step: the Rust
test exercises only kinematics.

Output: `tests/fixtures/<scenario>.json` next to this crate, plus
`hypot.json` (CPython `math.hypot` oracle) and `stance_chains.json`
(hand-set stances for the curved-leg and leg-symmetry geometry).
"""

from __future__ import annotations

import json
import math
import os
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
WORKTREE = CRATE.parents[2]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from desktop_bug.content.body_plans import resolve_body_plan  # noqa: E402
from desktop_bug.creature import Creature  # noqa: E402
from desktop_bug.creature import render_procedural as RP  # noqa: E402

OUT = CRATE / "tests" / "fixtures"
DT = 1.0 / 60.0
KIN_SEED = 4242
FSM_SEED = 777
GAIT_SEED = "fixture:gait-spell"

BODY = [
    ("x", "x"), ("y", "y"), ("heading", "heading"), ("target_heading", "target_heading"),
    ("target_x", "target_x"), ("target_y", "target_y"), ("current_speed", "current_speed"),
    ("speed", "speed"), ("vel_x", "vel_x"), ("vel_y", "vel_y"), ("turn_rate", "turn_rate"),
    ("size", "size"), ("motion_paused", "motion_paused"),
    ("body_bob", "body_bob"), ("body_sway", "body_sway"), ("bob_phase", "bob_phase"),
    ("breath_phase", "breath_phase"), ("abdomen_pulse", "abdomen_pulse"), ("ceph_pulse", "ceph_pulse"),
    ("turn_rehome_pressure", "turn_rehome_pressure"), ("strafe_observe", "strafe_observe"),
    ("inertia_timer", "inertia_timer"), ("inertia_vx", "inertia_vx"), ("inertia_vy", "inertia_vy"),
    ("catch_point", "catch_point"), ("catch_blend", "catch_blend"), ("inspect_intent", "inspect_intent"),
    ("feeler_clock", "_feeler_clock"), ("feeler_pulse", "_feeler_pulse"),
    ("feeler_pulse_dur", "_feeler_pulse_dur"), ("feeler_pulse_t", "_feeler_pulse_t"),
    ("gait_style", "gait_style"), ("gait_spell", "gait_spell"), ("gait_spell_timer", "gait_spell_timer"),
    ("lively_gait_phase", "_lively_gait_phase"), ("skitter_burst_timer", "_skitter_burst_timer"),
    ("skitter_pause_timer", "_skitter_pause_timer"), ("skitter_phase", "_skitter_phase"),
    ("skitter_burst_jitter", "_skitter_burst_jitter"), ("prev_heading_gait", "_prev_heading_gait"),
    ("spider_heading_filter", "_spider_heading_filter"),
    ("spider_locomotion_active", "_spider_locomotion_active"),
    ("spider_gait_frame_updated", "_spider_gait_frame_updated"),
    ("turn_speed_smooth", "_turn_speed_smooth"), ("idle_twitch_timer", "idle_twitch_timer"),
    ("spider_solver_dt", "_spider_solver_dt"), ("spider_turn_speed", "_spider_turn_speed"),
    ("rolling", "_rolling"), ("roll_spin", "roll_spin"), ("roll_tuck", "roll_tuck"),
    ("roll_progress", "roll_progress"), ("roll_eased", "roll_eased"), ("roll_duration", "roll_duration"),
    ("roll_dir", "roll_dir"), ("roll_total", "roll_total"), ("roll_distance", "roll_distance"),
    ("roll_drift_dir", "roll_drift_dir"),
    ("last_drift_amount", "last_drift_amount"), ("drift_momentum", "drift_momentum"),
    ("drift_lean", "drift_lean"), ("drift_phase", "drift_phase"), ("drift_boost", "drift_boost"),
    ("drift_flip_timer", "drift_flip_timer"), ("drift_dir", "drift_dir"),
    ("drift_slide_timer", "drift_slide_timer"), ("wiggle_burst", "wiggle_burst"),
]
# Compared every frame; the rest of BODY is compared on full snapshots.
BODY_KEY = {
    "x", "y", "heading", "target_heading", "target_x", "target_y", "current_speed", "vel_x", "vel_y",
    "body_sway", "bob_phase", "breath_phase", "turn_rehome_pressure", "lively_gait_phase",
    "spider_heading_filter", "turn_speed_smooth", "inertia_vx", "inertia_vy", "last_drift_amount",
    "drift_momentum", "catch_blend", "feeler_pulse", "skitter_burst_timer", "roll_progress", "size",
}
LEG = [
    "foot_x", "foot_y", "step_start_x", "step_start_y", "step_target_x", "step_target_y",
    "step_control_x", "step_control_y", "pending_target_x", "pending_target_y", "pending_delay",
    "step_timer", "step_duration", "stepping", "pending_step", "lift", "phase_seed", "twitch_clock",
    "gait_phase_offset", "step_cooldown", "contact_state", "support_weight", "contact_age",
    "stroke_progress", "stance_base_f", "stance_base_s", "stance_stroke_f", "stance_stroke_s",
    "stance_turn", "last_step_phase", "last_step_emergency", "joint_phase", "joint_bends",
    "held_spring_x", "held_spring_y", "held_spring_vx", "held_spring_vy",
]
LEG_KEY = {
    "foot_x", "foot_y", "stepping", "pending_step", "lift", "step_timer", "step_cooldown",
    "support_weight", "stance_turn", "contact_state", "joint_bends", "step_target_x", "step_target_y",
}


def plain(value):
    if isinstance(value, tuple):
        return [plain(v) for v in value]
    if isinstance(value, list):
        return [plain(v) for v in value]
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return value
    if isinstance(value, int):
        return float(value)
    return value


def rnd(value):
    """Post-step values are oracle outputs: 13 significant digits is ample
    for a 1e-9 comparison and keeps the fixtures small. Inputs (`pre`,
    `init`) stay exact."""
    if isinstance(value, float):
        return float("%.13g" % value)
    if isinstance(value, list):
        return [rnd(v) for v in value]
    if isinstance(value, dict):
        return {k: rnd(v) for k, v in value.items()}
    return value


def snap(c, full=True):
    body = {name: plain(getattr(c, attr)) for name, attr in BODY if full or name in BODY_KEY}
    body["legs"] = [
        {name: plain(getattr(leg, name)) for name in LEG if full or name in LEG_KEY} for leg in c.legs
    ]
    return body


def diff(before, after):
    out = {}
    for key, value in after.items():
        if key == "legs":
            legs = []
            changed = False
            for lb, la in zip(before["legs"], value):
                d = {k: v for k, v in la.items() if lb.get(k) != v}
                changed |= bool(d)
                legs.append(d)
            if changed:
                out["legs"] = legs
        elif before.get(key) != value:
            out[key] = value
    return out


def ctx(c):
    return {
        "state": c.state,
        "fleeing": bool(c.fleeing),
        "acts_as_observer": bool(c._acts_as_observer()),
        "acts_as_drifter": bool(c._acts_as_drifter()),
        "reverse_walk": bool(getattr(c, "reverse_walk", False)),
        "has_job_facing": getattr(c, "job_facing", None) is not None,
        "airborne": bool(c.airborne),
        "dragging": bool(c.dragging),
        "webbed_held": bool(c.webbed_held),
        "startled_timer": float(c.startled_timer),
        "drift_run_sliding": getattr(c, "drift_run_phase", "charge") == "slide",
        "held_release_timer": float(c.held_release_timer),
        "focus_strength": float(c.focus_strength),
        "focus_x": float(c.focus_x),
        "focus_y": float(c.focus_y),
        "held_drag_response": float(getattr(c, "held_drag_response", 0.0)),
        "held_drag_accel_x": float(getattr(c, "held_drag_accel_x", 0.0)),
        "held_drag_accel_y": float(getattr(c, "held_drag_accel_y", 0.0)),
        "held_pose_clock": float(getattr(c, "held_pose_clock", 0.0)),
        "held_drag_sway_x": float(getattr(c, "held_drag_sway_x", 0.0)),
        "held_drag_sway_y": float(getattr(c, "held_drag_sway_y", 0.0)),
        "held_leg_relax": float(getattr(c, "held_leg_relax", 1.0)),
        "drag_vel_x": float(c.drag_vel_x),
        "drag_vel_y": float(c.drag_vel_y),
        "screen_w": float(c.screen_w),
        "screen_h": float(c.screen_h),
        "margin": float(c.margin),
    }


def draw_ctx(c):
    push, lift = c.strike_phase()
    return {
        "combat_body_offset": plain(c.combat_body_offset()),
        "combat_stance": float(getattr(c, "combat_stance", 0.0)),
        "strike_push": float(push),
        "strike_lift": float(lift),
        "strike_face": plain(c.strike_face),
        "combat_face_x": float(c.combat_face_x),
        "combat_face_y": float(c.combat_face_y),
        "feed_paw": float(c._feed_paw),
        "jump_z": float(c.jump_z),
        "jump_peak": float(c.jump_peak),
    }


def probe(c):
    """Rendered leg geometry: what the painter would ask for."""
    chain = c._sprite_leg_chain_config()
    c._chain_points_cache.clear()
    legs = []
    for leg in c.legs:
        ax, ay = c._leg_attach(leg)
        vx, vy = c._visual_foot_for_render(leg)
        dax, day, dfx, dfy = c._leg_draw_points(leg)
        item = {
            "attach": [ax, ay],
            "visual_foot": [vx, vy],
            "draw_points": [dax, day, dfx, dfy],
            "lift_px": c._leg_lift_px(leg),
            "ideal": list(c._leg_ideal_foot(leg)),
            "knee": list(c._solve_knee(ax, ay, vx, vy, leg)),
        }
        if chain is not None:
            item["chain"] = [list(p) for p in c._sprite_leg_chain_points(leg, ax, ay, vx, vy, chain)]
            item["safe_foot"] = list(c._safe_sprite_leg_foot(leg, dfx, dfy, chain))
        legs.append(item)
    return {"draw": draw_ctx(c), "legs": legs}


class Recorder:
    def __init__(self, name, model_id, personality_id, gait_style, seed, build="movement", model_patch=None):
        self.name = name
        root = WORKTREE
        model = resolve_body_plan(json.loads((root / "models" / model_id / "model.json").read_text(encoding="utf-8")))
        if model_patch:
            model_patch(model)
        personality = json.loads((root / "personalities" / f"{personality_id}.json").read_text(encoding="utf-8"))
        random.seed(seed)
        c = Creature(model, personality, 2400, 1400, gait_style=gait_style)
        if build == "movement":
            # tests/movement.py::build_creature
            c.x, c.y = 900.0, 700.0
            c.heading = c.target_heading = 0.0
            c.target_x, c.target_y = 2100.0, 700.0
            c.current_speed = 0.0
            c.speed = 80.0
            c.vel_x = c.vel_y = 0.0
            c.state = "Wander"
            c._initialize_legs()
        assert c._split_playfield() is None
        self.c = c
        self.kin_rng = random.Random(KIN_SEED)
        self.fsm_rng = random.Random(FSM_SEED)
        c.rng = self.kin_rng
        c._gait_rng = random.Random(GAIT_SEED)
        self.mid_patches = []
        original_enter_idle = c.enter_idle

        def enter_idle_hook():
            before = snap(c)
            c.rng = self.fsm_rng
            try:
                original_enter_idle()
            finally:
                c.rng = self.kin_rng
            self.mid_patches.append(diff(before, snap(c)))

        c.enter_idle = enter_idle_hook
        self.fixture = {
            "name": name,
            "model": {"legs": model.get("legs", []), "appearance": {
                k: model.get("appearance", {}).get(k)
                for k in ("spider_gait", "leg_chain", "antennae", "front_leg_feeler_mode", "front_leg_idle_lift",
                          "front_leg_idle_forward", "front_leg_feeler_sway", "roll_tuck_strength")
                if k in model.get("appearance", {})
            }},
            "personality": personality,
            "gait_style": c.gait_style,
            "kin_seed": KIN_SEED,
            "gait_seed": GAIT_SEED,
            "dt": DT,
            "gait_groups": list(c.gait_groups),
            "init": snap(c),
            "frames": [],
        }
        self.prev_ctx = {}
        self.prev = snap(c)

    def script(self, fn):
        """Run non-kinematic setup with the FSM generator."""
        c = self.c
        c.rng = self.fsm_rng
        try:
            fn(c)
        finally:
            c.rng = self.kin_rng

    def frame(self, ops, script=None, probe_every=10, extra=None):
        c = self.c
        before = snap(c)
        if script is not None:
            self.script(script)
        after_script = snap(c)
        pre = diff(before, after_script)
        frame_ctx = ctx(c)
        ctx_delta = {k: v for k, v in frame_ctx.items() if self.prev_ctx.get(k) != v}
        self.prev_ctx = frame_ctx
        self.mid_patches = []
        results = []
        for op in ops:
            results.append(run_op(c, op, extra))
        index = len(self.fixture["frames"])
        # Exhaustive: every snapshot field that changed during the ops. The
        # replay accumulates pre/post onto an expected state and compares
        # every field, every frame.
        record = {"pre": pre, "ctx": ctx_delta, "ops": ops, "post": rnd(diff(after_script, snap(c)))}
        if self.mid_patches:
            record["mid"] = self.mid_patches
        if any(r is not None for r in results):
            record["results"] = results
        if probe_every and index % probe_every == 0:
            record["probe"] = rnd(probe(c))
        self.fixture["frames"].append(record)

    def save(self):
        OUT.mkdir(parents=True, exist_ok=True)
        path = OUT / f"{self.name}.json"
        path.write_text(json.dumps(self.fixture, separators=(",", ":")), encoding="utf-8")
        print(f"{path.name}: {len(self.fixture['frames'])} frames, {path.stat().st_size // 1024} KiB")


def run_op(c, op, extra):
    if op == "production":
        if not c.airborne:
            c._aim_at_a_real_screen()
            if c._spider_grounded_mode_allowed():
                c._update_spider_grounded_frame(DT)
            else:
                if c._spider_locomotion_active:
                    c._spider_locomotion_active = False
                c._move_body(DT)
        c._reconcile_roll()
        c._update_legs(DT)
    elif op == "controller":
        c._update_spider_grounded_frame(DT)
        c._spider_gait_frame_updated = False
    elif op == "locomotion":
        c._update_spider_grounded_locomotion(DT)
    elif op == "advance_steps":
        c._advance_active_steps(DT)
    elif op == "move_body":
        c._move_body(DT)
    elif op == "update_legs":
        c._update_legs(DT)
    elif op == "roll":
        c._update_roll(DT, 0.0, 0.0)
    elif op == "reconcile_roll":
        c._reconcile_roll()
    elif op == "held":
        c._update_held_leg_springs(DT)
    elif op == "initialize_legs":
        c._initialize_legs()
    elif op == "panic_rehome":
        c._panic_rehome_legs(0.8)
    elif op == "gait_spell":
        c._update_gait_spell(DT)
    elif op == "intent":
        return list(c._spider_locomotion_intent(DT))
    else:
        raise ValueError(op)
    return None


def aim(angle, dist=500.0):
    def fn(c):
        c.target_x = c.x + math.cos(angle) * dist
        c.target_y = c.y + math.sin(angle) * dist
    return fn


def tarantula_walk():
    r = Recorder("tarantula_walk", "tarantula", "curious", "lively", 19)
    r.frame(["initialize_legs"])
    for i in range(270):
        script = None
        if i == 0:
            def script(c):
                c.speed = 100.0
        elif 120 <= i < 180:
            script = aim(math.pi * 0.5)
        elif i == 180:
            def script(c):
                c.speed = 0.0
                c.target_x, c.target_y = c.x, c.y
        elif i == 220:
            def script(c):
                c.state = "Chase"
                c.speed = 200.0
                c.target_x, c.target_y = 300.0, 1100.0
        elif i == 260:
            def script(c):
                c.size *= 1.25
        r.frame(["production", "gait_spell"], script=script)
    r.save()


def snowpuff_walk():
    r = Recorder("snowpuff_walk", "plush_snow_hybrid_2", "cuddly", "lively", 19)
    for i in range(240):
        script = None
        if i >= 150:
            def script(c):
                # tests/movement.py::run_walk pure pivot
                c.target_x, c.target_y = c.x, c.y
                c.turn_rate = 0.6
                c.target_heading = math.pi * 0.5
        r.frame(["controller"], script=script)
    r.save()


def jumper_skitter():
    r = Recorder("jumper_skitter", "chosen_one", "curious", "skitter", 23)
    for i in range(240):
        script = None
        if i == 60:
            def script(c):
                c.state = "Inspect"
                c.focus_strength = 1.0
                c.focus_x, c.focus_y = c.x + 60.0, c.y - 20.0
                c.speed = 40.0
        elif i == 150:
            def script(c):
                c.state = "Wander"
                c.speed = 90.0
                c.target_x, c.target_y = 600.0, 300.0
        r.frame(["production"], script=script)
    r.save()


def bug_classic():
    r = Recorder("bug_classic", "spider", "mellow", "classic", 5)
    for i in range(200):
        script = None
        if i == 100:
            script = aim(-2.0)
        elif i == 160:
            def script(c):
                c.speed = 0.0
        r.frame(["production"], script=script)
    r.save()


def strip_gait(model):
    model.setdefault("appearance", {})["spider_gait"] = {"enabled": False}


def nogait(style, name, personality):
    r = Recorder(name, "spider", personality, style, 8, model_patch=strip_gait)
    for i in range(220):
        script = None
        if i == 90:
            script = aim(2.4)
        elif i == 150:
            def script(c):
                c.state = "Chase"
                c.speed = 160.0
        elif i == 190:
            def script(c):
                c.state = "Idle"
                c.speed = 0.0
        r.frame(["production"], script=script)
    r.save()


def drifter():
    r = Recorder("drifter", "plush_snow_hybrid_2", "drifter", "skitter", 7, build="none")

    def setup(c):
        c.x, c.y = 1200.0, 700.0
        c.heading = c.target_heading = 0.0
        c.enter_drift_run("circle")

    r.frame(["initialize_legs"], script=setup)
    for _ in range(240):
        r.frame(["move_body", "update_legs"], script=lambda c: c._update_drift_run(DT, -1000.0, -1000.0))
    r.save()


def throw():
    r = Recorder("throw", "tarantula", "mellow", "lively", 11, build="none")

    def setup(c):
        c.x, c.y = 400.0, 700.0
        c.dragging = True
        c.drag_vel_x, c.drag_vel_y = 900.0, 0.0
        c.release_drag(-100000.0, -100000.0)

    r.frame(["initialize_legs"])
    r.frame(["panic_rehome"], script=setup)
    for _ in range(150):
        r.frame(["production"])
    r.save()


def roll():
    r = Recorder("roll", "plush_snow_hybrid_2", "playful", "lively", 19)

    def setup(c):
        c.squash = 0.66
        c.current_speed = 130.0
        c.vel_x = 130.0
        c.enter_roll(direction=0.0)

    for _ in range(10):
        r.frame(["controller"])
    r.frame(["roll", "reconcile_roll", "update_legs"], script=setup)
    for _ in range(120):
        if r.c.state == "Roll":
            r.frame(["roll", "reconcile_roll", "update_legs"])
        else:
            r.frame(["production"])
    r.save()


def held():
    r = Recorder("held", "tarantula", "curious", "lively", 3)

    def grab(c):
        c.dragging = True
        c.held_leg_relax = 0.0

    r.frame(["held"], script=grab)
    for i in range(90):
        def script(c, i=i):
            vx = 400.0 * math.sin(i * 0.11)
            c.drag_vel_x, c.drag_vel_y = vx, 120.0 * math.cos(i * 0.07)
            c.held_drag_response += (min(1.0, abs(vx) / 420.0) - c.held_drag_response) * 0.11
            c.held_pose_clock += DT * (1.15 + c.held_drag_response * 2.35)
            c.held_drag_accel_x = 400.0 * 0.11 * 60.0 * math.cos(i * 0.11)
            c.held_drag_accel_y = -120.0 * 0.07 * 60.0 * math.sin(i * 0.07)
            c.held_drag_sway_x = vx / 520.0 * c.size * 0.15
            c.held_drag_sway_y = 0.02 * c.size
            c.held_leg_relax = min(1.0, c.held_leg_relax + DT * 4.5)
            c.current_speed = abs(vx)
        r.frame(["held", "update_legs"], script=script, probe_every=5)
    r.save()


def heading_filter():
    """tests/movement.py::run_heading_filter_check, recorded."""
    r = Recorder("heading_filter", "tarantula", "hunter", "lively", 19)

    def setup(c):
        c.state = "Chase"
        c.speed = c.current_speed = 0.0
        c._spider_heading_filter = c.heading

    r.frame(["intent"], script=setup, probe_every=0)
    for frame in range(180):
        r.frame(["intent"], script=aim(0.78 if frame % 2 else -0.78), probe_every=0)
    for _ in range(30):
        r.frame(["intent"], script=aim(0.78), probe_every=0)
    r.save()


def hypot_table():
    rng = random.Random(1)
    rows = []
    for _ in range(5000):
        a = rng.uniform(-500, 500) * 10 ** rng.randint(-6, 3)
        b = rng.uniform(-500, 500) * 10 ** rng.randint(-6, 3)
        if rng.random() < 0.05:
            b = 0.0
        rows.append([a, b, math.hypot(a, b)])
    sums = []
    for _ in range(2000):
        values = [rng.uniform(-10, 10) * 10 ** rng.randint(-3, 3) for _ in range(rng.randint(1, 7))]
        sums.append([values, sum(values)])
    (OUT / "hypot.json").write_text(json.dumps({"hypot": rows, "sum": sums}), encoding="utf-8")
    print("hypot.json")


def stance_chains():
    """tests/test_leg_symmetry.py::_standing and test_curved_legs, both CURVED_LEGS settings."""
    from desktop_bug.content.discovery import discover_models, discover_personalities

    models, _ = discover_models(WORKTREE)
    personalities, _ = discover_personalities(WORKTREE)
    out = {"model": None, "personality": personalities["balanced"], "cases": []}
    model = models["tarantula"]
    out["model"] = {"legs": model["legs"], "appearance": {
        k: model["appearance"].get(k) for k in ("spider_gait", "leg_chain", "antennae") if k in model["appearance"]}}
    for curved in (True, False):
        RP.CURVED_LEGS = curved
        for scale in (3.0, 1.0):
            for heading_deg in (0.0, 30.0, 45.0, 90.0, 135.0, 180.0, 200.0, 270.0):
                random.seed(1)
                spider = Creature(model, personalities["balanced"], 1600, 900, size_scale=scale)
                spider.x, spider.y = 800.0, 450.0
                spider.heading = spider.target_heading = math.radians(heading_deg)
                spider._basis_cache = None
                fx, fy, rx, ry = spider._basis()
                for leg in spider.legs:
                    d = leg.definition
                    sign = spider._side_sign(d.get("side", "right"))
                    rf = float(d["rest_forward"]) * spider.size
                    rs = float(d["rest_side"]) * spider.size * sign
                    leg.foot_x = spider.x + fx * rf + rx * rs
                    leg.foot_y = spider.y + fy * rf + ry * rs
                    leg.lift = 0.0
                chain = spider._sprite_leg_chain_config()
                legs = []
                for leg in spider.legs:
                    ax, ay = spider._leg_attach(leg)
                    pts = spider._sprite_leg_chain_points(leg, ax, ay, leg.foot_x, leg.foot_y, chain)
                    legs.append({
                        "foot": [leg.foot_x, leg.foot_y], "joint_bends": list(leg.joint_bends),
                        "phase_seed": leg.phase_seed, "chain": [list(p) for p in pts],
                    })
                out["cases"].append({
                    "curved": curved, "scale": scale, "heading_deg": heading_deg, "size": spider.size,
                    "x": spider.x, "y": spider.y, "heading": spider.heading, "body_sway": spider.body_sway,
                    "legs": legs,
                })
    RP.CURVED_LEGS = True
    (OUT / "stance_chains.json").write_text(json.dumps(out), encoding="utf-8")
    print("stance_chains.json")


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    hypot_table()
    stance_chains()
    tarantula_walk()
    snowpuff_walk()
    jumper_skitter()
    bug_classic()
    nogait("lively", "nogait_lively", "zoomy")
    nogait("classic", "nogait_classic", "bold")
    drifter()
    throw()
    roll()
    held()
    heading_filter()
