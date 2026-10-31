"""Dump Python oracle traces for dc-world's simulation worlds.

Webs (planners, site claims, a build / tear / mend trace, persistence), flies
(two scripted flocks with spiders, webs, shots, eating and dragging), cursor
silk, colony bases (a scripted colony of every job), mounds and dirt patches,
cages and carcasses.

Every creature input is either recorded in the fixture or produced by a tiny
script whose arithmetic the Rust side repeats exactly (`tri` below: a triangle
wave, additions and Python `%` only), so the Rust run sees the same inputs.

Run from anywhere:
    <venv>/python.exe -I rust/crates/dc-world/tools/dump_world_fixtures.py
Writes rust/crates/dc-world/tests/fixtures/{webs,flies,mouse_webs,jobs,cage,carcass}.json.
"""
from __future__ import annotations

import json
import math
import os
import random
import sys
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
WORKTREE = HERE.parents[3]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from desktop_bug.world import flies as F  # noqa: E402
from desktop_bug.world import jobs as J  # noqa: E402
from desktop_bug.world import mouse_webs as M  # noqa: E402
from desktop_bug.world import webs as W  # noqa: E402
from desktop_bug.world.aimed_silk import AimedSilk  # noqa: E402
from desktop_bug.world.cage import Cage  # noqa: E402
from desktop_bug.world.carcass import Carcass, eaters_near  # noqa: E402
from desktop_bug.world.playfield import Playfield, ScreenRect  # noqa: E402

OUT = HERE.parent / "tests" / "fixtures"
DT = 1.0 / 60.0


def tri(x0: float, v: float, lo: float, hi: float, t: float) -> float:
    """A point bouncing between lo and hi: lo + triangle wave. Mirrored in Rust."""
    span = hi - lo
    p = (x0 - lo + v * t) % (2.0 * span)
    return lo + (p if p < span else 2.0 * span - p)


def strands_json(strands):
    return [[s.kind, bool(s.draw), bool(s.temporary), [[x, y] for x, y in s.points]] for s in strands]


# ======================================================================
# Webs
# ======================================================================
class Weaver:
    def __init__(self, size, x=0.0, y=0.0):
        self.size = size
        self.x = x
        self.y = y


def web_plans():
    sw, sh = 1600.0, 900.0
    specs = W.WebWorld(sw, sh)._specs()
    cases = []
    for seed in range(10):
        spec = specs[seed % 4]
        span = 90.0 + seed * 23.0
        vertex, din = spec["vertex"], spec["din"]
        center = (200.0 + seed * 130.0, 150.0 + (seed % 5) * 160.0)
        for pattern in ("corner_orb", "orb", "funnel", "tangle"):
            rng = random.Random(seed * 10 + len(pattern))
            if pattern == "corner_orb":
                plan = W._plan_corner_orb(vertex, din, span, 38.0, sw, sh, rng=rng)
            elif pattern == "orb":
                plan = W._plan_orb(center, span, sw, sh, rng=rng)
            elif pattern == "funnel":
                plan = W._plan_funnel(vertex, din, span, sw, sh, rng=rng)
            else:
                plan = W._plan_tangle(vertex, din, span, sw, sh, rng=rng)
            strands, hub, wob = plan
            cases.append({
                "pattern": pattern, "seed": seed * 10 + len(pattern), "vertex": list(vertex),
                "din": list(din), "center": list(center), "span": span,
                "strands": strands_json(strands), "hub": list(hub), "wob": list(wob),
                "next": rng.random(),
            })
    return cases


def web_claims():
    cases = []
    for seed in range(3):
        world = W.WebWorld(1600, 900, rng=random.Random(f"{seed}:webs"))
        picker = random.Random(seed + 500)
        events = []
        for i in range(9):
            size = 14.0 + (i * 7 % 20)
            prefer = i % 2 == 0
            weights = None if i % 3 else {"funnel": 0.5, "tangle": 0.5 + seed * 0.1, "corner_orb": 0.1, "bogus": 3.0}
            web = world.claim_site(Weaver(size), prefer_corner=prefer, pattern_weights=weights, rng=picker)
            events.append({"size": size, "prefer": prefer,
                           "weights": None if weights is None else [[k, v] for k, v in weights.items()],
                           "claimed": web is not None})
        walk = [list(web.walkable_point(1600.0, 900.0)) for web in world.webs for _ in range(3)]
        cases.append({"seed": seed, "events": events, "webs": world.to_dict(), "walk": walk,
                      "next": world.rng.random(), "picker_next": picker.random()})
    return cases


def web_trace():
    world = W.WebWorld(1600, 900, rng=random.Random("7:webs"))
    picker = random.Random(77)
    for size, prefer in ((18.0, True), (24.0, False), (30.0, True)):
        world.claim_site(Weaver(size), prefer_corner=prefer, rng=picker)
    speeds = (300.0, 420.0, 550.0)
    frames = []
    checkpoints = {}
    swipe = None
    mend_from = 1500
    for f in range(2200):
        cursor = None
        if f >= 1 and swipe is None and all(w.is_complete() for w in world.webs):
            hub = world.webs[0].hub
            swipe = (f, hub)
        if swipe is not None and f < swipe[0] + 40:
            k = f - swipe[0]
            cursor = (swipe[1][0] - 300.0 + k * 15.0, swipe[1][1] + 3.0)
        for i, web in enumerate(world.webs):
            if web.state == "building":
                tip, _drawing = web.working_point()
                lag = 30.0 if (f // 40) % 5 == i else 0.0
                web.advance(DT, speeds[i], (tip[0] + lag, tip[1]))
        if f >= mend_from:
            for web in world.webs:
                if web.is_damaged():
                    web.repair_near(web.hub[0] + 7.0, web.hub[1] - 5.0, DT)
        world.update(DT, cursor)
        if f % 10 == 0:
            rows = [[w.built, w.active_t, w.state, w.bounce_amp, w.bounce_phase, len(w.cut),
                     w.intact_fraction(), w.is_animating(), w.progress(), w.temp_alpha()]
                    for w in world.webs]
        else:
            rows = [[w.built, w.active_t, w.bounce_amp, len(w.cut)] for w in world.webs]
        frames.append({
            "cursor": None if cursor is None else list(cursor),
            "webs": rows,
            "dirty": len(world.dirty_rects()),
        })
        if f in (0, 600, 1200) or (swipe is not None and f == swipe[0] + 45):
            checkpoints[str(f)] = {
                "webs": world.to_dict(),
                "centroids": [None if w.damaged_centroid() is None else list(w.damaged_centroid())
                              for w in world.webs],
            }
    checkpoints["end"] = {"webs": world.to_dict(), "centroids": []}
    walk = [list(world.webs[i % len(world.webs)].walkable_point(1600.0, 900.0)) for i in range(9)]
    return {"speeds": list(speeds), "mend_from": mend_from, "frames": frames,
            "checkpoints": checkpoints, "walk": walk, "next": world.rng.random()}


def web_persistence():
    world = W.WebWorld(1600, 900, rng=random.Random("11:webs"))
    picker = random.Random(11)
    for size in (16.0, 28.0):
        world.claim_site(Weaver(size), prefer_corner=True, rng=picker)
    saved = world.to_dict()
    good = dict(saved[0])
    entries = [
        good,
        "not a dict",
        {"strands": "abc", "hub": [1, 2], "wobble_dir": [0, 1]},
        dict(good, hub="12", wobble_dir=[0, 0], reach_inset="abc", pattern=5, spec_id=None,
             built="3", active_t="0.25", state="complete",
             cut=[[0, 0], ["1", "0"], [99, 0], [0, -1], "xy", [0], [2, 1.0]]),
        dict(good, built=2.7, active_t=7, state="building", cut=None),
        dict(good, built="2.7"),
        dict(good, hub=[1]),
        dict(good, strands=[{"points": [[1, 2], ["3", "4"], [5]], "kind": None, "draw": 0},
                            {"points": [[0, 0]]}, 7,
                            {"points": [[10, 10], [20, 20]], "temporary": "yes"}]),
        dict(saved[1], hub=[-500.0, -500.0],
             strands=[{"points": [[-900.0, -900.0], [-800.0, -850.0]], "kind": "frame"}]),
        saved[1],
    ]
    other = W.WebWorld(1600, 900, rng=random.Random(0))
    count = other.restore(entries)
    small = W.WebWorld(1600, 900, rng=random.Random(0))
    small.restore(saved)
    small.set_screen(300, 200)
    return {"entries": entries, "count": count, "restored": other.to_dict(),
            "after_shrink": small.to_dict()}


# ======================================================================
# Flies
# ======================================================================
class FlySpider:
    """What a fly (and a web shot) reads off a spider."""

    def __init__(self, index, team, size, x0, y0, vx, vy):
        self.index = index
        self.team = team
        self.size = size
        self.x0, self.y0, self.svx, self.svy = x0, y0, vx, vy
        self.x = x0
        self.y = y0
        self.heading = math.atan2(vy, vx)
        self.current_speed = math.hypot(vx, vy)
        self.aim_intent = 0.0
        self.airborne = False
        self.dragging = False
        self._desktop_fully_hidden = False
        self.hunter = False
        self._hunting_prey = False
        self._prey = None
        self.dead = False
        self.webbed = False
        self.webbed_until = -1
        self.frame = 0
        self.homing = 0.5 if index % 3 == 1 else 0.0
        self.pins = []

    def _acts_as_hunter(self):
        return self.hunter

    def _progression_effect(self, name):
        return self.homing if name == "web_homing" else 0.0

    def relation_to(self, other):
        return "friend" if other.team == self.team else "foe"

    def web_pinned(self, kind, by):
        self.webbed = True
        self.webbed_until = self.frame + 90
        self.pins.append([self.frame, kind, by.index])

    def place(self, f, w, h):
        t = f * DT
        self.x = tri(self.x0, self.svx, 20.0, w - 20.0, t)
        self.y = tri(self.y0, self.svy, 20.0, h - 20.0, t)
        self.frame = f
        if self.webbed and f >= self.webbed_until:
            self.webbed = False


def spider_flags(sp, f):
    """Scripted per-frame flags; mirrored in Rust."""
    i = sp.index
    sp.airborne = i % 4 == 1 and (f // 50) % 7 == 3
    sp.dragging = i % 5 == 2 and 200 <= f < 260
    sp._desktop_fully_hidden = i % 6 == 3 and 400 <= f < 450
    sp.aim_intent = 0.5 if (i % 3 == 1 and (f // 60) % 2 == 1) else 0.0


def fly_record(fly, fid):
    return [fid, fly.x, fly.y, fly.heading, fly.state, fly.panic_level, fly.speed]


def fly_full(fly, fid):
    return {
        "fid": fid, "x": fly.x, "y": fly.y, "heading": fly.heading, "target_heading": fly.target_heading,
        "speed": fly.speed, "vx": fly.vx, "vy": fly.vy, "state": fly.state, "motion_mode": fly.motion_mode,
        "motion_timer": fly.motion_timer, "wander_timer": fly.wander_timer, "panic_level": fly.panic_level,
        "wing_phase": fly.wing_phase, "buzz_phase": fly.buzz_phase, "struggle_phase": fly.struggle_phase,
        "pluck_timer": fly.pluck_timer, "trapped_for": fly.trapped_for, "free_progress": fly.free_progress,
        "web_check_timer": fly.web_check_timer, "draw_jitter": fly._draw_jitter, "length": fly.length,
        "web_splat": fly.web_splat, "captured_by_web": fly.captured_by_web,
        "tether": None if fly.tether_from is None else list(fly.tether_from),
        "pegs": [list(p) for p in fly._web_pegs], "eat_t": fly.eat_t,
        "trapper": None if fly._trapper is None else fly._trapper.index,
    }


def fly_scenario(name, seed, n_spiders, frames, split, max_flies):
    sw, sh = 1600.0, 900.0
    world = F.FlyWorld(sw, sh, enabled=True, min_interval=0.4, max_interval=1.2, max_flies=max_flies,
                       rng=random.Random(f"{seed}:flies"))
    rects = None
    if split:
        rects = [[0.0, 0.0, 1000.0, 900.0], [1000.0, 0.0, 600.0, 500.0]]
        world.playfield = Playfield(sw, sh, [ScreenRect(*r) for r in rects])
    web_world = W.WebWorld(sw, sh, rng=random.Random(f"{seed}:webs"))
    for pattern, plan, spec_id in (
        ("corner_orb", W._plan_corner_orb((0.0, 0.0), (0.7071067811865475, 0.7071067811865475), 260.0,
                                          38.0, sw, sh, rng=random.Random(5)), "corner_nw"),
        ("orb", W._plan_orb((700.0, 420.0), 170.0, sw, sh, rng=random.Random(6)), "open"),
    ):
        strands, hub, wob = plan
        web = W.Web(pattern, strands, hub, wob, spec_id, 38.0, sw, sh, rng=web_world.rng)
        web.state = "complete"
        web.built = len(web.strands)
        web_world.webs.append(web)

    script = random.Random(seed * 1000 + 1)
    spiders = []
    for i in range(n_spiders):
        spiders.append(FlySpider(i, "a" if i % 2 == 0 else "b", script.uniform(14.0, 30.0),
                                 script.uniform(40.0, sw - 40.0), script.uniform(40.0, sh - 40.0),
                                 script.uniform(-90.0, 90.0), script.uniform(-70.0, 70.0)))
    spiders[0].hunter = True

    fids = {}
    next_fid = [1]

    def fid_of(fly):
        if id(fly) not in fids:
            fids[id(fly)] = (fly, next_fid[0])
            next_fid[0] += 1
        return fids[id(fly)][1]

    def by_fid(fid):
        for fly in world.flies:
            if fid_of(fly) == fid:
                return fly
        return None

    events = []
    trace = []
    full = {}
    drag = None
    for f in range(frames):
        for sp in spiders:
            sp.place(f, sw, sh)
            spider_flags(sp, f)
        for fly in world.flies:
            fid_of(fly)
        # The hunter stalks the oldest live fly between frames 300 and 500.
        hunter = spiders[0]
        if 300 <= f < 500:
            live = [fly for fly in world.flies if fly.alive]
            hunter._hunting_prey = True
            hunter._prey = live[0] if live else None
        else:
            hunter._hunting_prey = False
            hunter._prey = None
        prey = None if hunter._prey is None else fid_of(hunter._prey)

        ev = []
        if f == 60:
            world.add_spawner((300.0, 700.0))
            ev.append({"kind": "spawner", "at": [300.0, 700.0]})
        if f >= 100 and f % 90 == 10:
            shooter = spiders[(f // 90) % len(spiders)]
            targets = [fly for fly in world.flies if fly.alive and not fly.trapped and not fly.dragging]
            if targets:
                fly = targets[(f // 90) % len(targets)]
                kind = "trap" if f % 180 == 10 else "tether"
                ok = world.launch_web_shot(shooter, fly, kind)
                ev.append({"kind": "shot", "shooter": shooter.index, "fid": fid_of(fly), "shot": kind, "ok": ok})
        if f == 333 and len(spiders) > 2:
            ok = world.launch_web_shot_at_creature(spiders[0], spiders[1], "trap")
            ev.append({"kind": "shoot_spider", "shooter": 0, "target": 1, "ok": ok})
        if f == 420 and len(spiders) > 2:
            world.projectiles.append(AimedSilk(spiders[1], 0.3, 600.0, lambda: spiders, lambda: world.flies))
            ev.append({"kind": "aimed", "shooter": 1, "angle": 0.3, "reach": 600.0})
        if f >= 200 and f % 150 == 50:
            trapped = [fly for fly in world.flies if fly.trapped]
            if trapped:
                fly = trapped[0]
                fly.begin_eaten()
                world.add_remains((fly.x, fly.y), fly.scale)
                ev.append({"kind": "eat", "fid": fid_of(fly)})
        if f == 500:
            flying = [fly for fly in world.flies if fly.state == "flying"]
            if flying:
                drag = flying[-1]
                drag.start_drag(drag.x + 5.0, drag.y + 5.0)
                ev.append({"kind": "drag_start", "fid": fid_of(drag), "at": [drag.x + 5.0, drag.y + 5.0]})
        if drag is not None and 500 < f < 530:
            mx, my = 600.0 + (f - 500) * 9.0, 300.0 + (f - 500) * 4.0
            drag.drag_to(mx, my)
            ev.append({"kind": "drag_to", "fid": fid_of(drag), "at": [mx, my]})
        if drag is not None and f == 530:
            drag.release_drag(870.0, 420.0)
            ev.append({"kind": "drag_release", "fid": fid_of(drag), "at": [870.0, 420.0]})
            drag = None
        if f == 700:
            world.configure(max_flies=4, min_interval=0.2, max_interval=0.3)
            ev.append({"kind": "configure"})
        if f == 820:
            world.clear_hunters()
            ev.append({"kind": "clear_hunters"})
        if ev:
            events.append({"frame": f, "events": ev})

        world.update(DT, spiders, web_world)
        web_world.update(DT, None)
        for fly in world.flies:
            fid_of(fly)
        if f % 3 == 0 or f == frames - 1:
            trace.append({
                "frame": f, "prey": prey,
                "flies": [fly_record(fly, fid_of(fly)) for fly in world.flies],
                "proj": [[p.pos[0], p.pos[1], p.travelled] for p in world.projectiles],
                "remains": len(world.remains),
                "spawners": [[sp.x, sp.y, sp.pulse] for sp in world.spawners],
                "webs": [w.bounce_amp for w in web_world.webs],
                "webbed": [sp.webbed for sp in spiders],
                "dirty": len(world.dirty_rects()),
                "timer": world._spawn_timer,
            })
        if f % 150 == 149 or f == frames - 1:
            full[str(f)] = [fly_full(fly, fid_of(fly)) for fly in world.flies]
    return {
        "name": name, "seed": seed, "frames": frames, "split": rects, "max_flies": max_flies,
        "spiders": [{"team": sp.team, "size": sp.size, "x0": sp.x0, "y0": sp.y0, "vx": sp.svx, "vy": sp.svy,
                     "homing": sp.homing, "hunter": sp.hunter} for sp in spiders],
        "events": events, "trace": trace, "full": full,
        "pins": [sp.pins for sp in spiders],
        "nests": world.spawners_to_dict(),
        "next": world.rng.random(),
    }


def fly_parts():
    rng = random.Random(42)
    world = F.FlyWorld(1200, 700, rng=random.Random("5:flies"))
    world.ensure_spawner()
    world.add_spawner()
    world.add_spawner((5000.0, -40.0))
    nests = world.spawners_to_dict()
    restored = F.FlyWorld(1200, 700, rng=random.Random(9))
    count = restored.restore_spawners([{"x": 10, "y": "20"}, {"x": 400.0, "y": 300.0, "scale": 1.7},
                                       {"y": 3}, "nope", {"x": "a", "y": 1}, {"x": 9e9, "y": -9e9, "scale": 0.1}])
    remains = []
    for i in range(5):
        r = F.FlyRemains(100.0 * i, 50.0, scale=0.4 + i * 0.4, rng=rng)
        remains.append({"x": r.x, "y": r.y, "scale": r.scale, "footprint": list(r.footprint()),
                        "parts": [[p["x"], p["y"], p["rot"], p["len"], p["kind"]] for p in r.parts]})
    flies = []
    for i in range(8):
        fly = F.Fly(300.0, 200.0, 1200, 700, heading=None if i % 2 else 0.5, scale=0.3 + i * 0.3, rng=rng)
        flies.append(fly_full(fly, i))
    return {"nests": nests, "restored_count": count, "restored": restored.spawners_to_dict(),
            "remains": remains, "flies": flies, "next": rng.random()}


# ======================================================================
# Cursor silk
# ======================================================================
def mouse_case(seed, can_control):
    world = M.MouseWebWorld(1200, 800, can_control=can_control, rng=random.Random(f"{seed}:mouse-webs"))
    rows = []
    desired = None
    mx, my = 600.0, 400.0
    events = []
    for f in range(1500):
        ev = None
        if f == 0:
            ok = world.shoot((100.0, 100.0), (600.0, 400.0), "trap")
            ev = ["trap", [100.0, 100.0], [600.0, 400.0], 0.0, [0.0, 0.0], ok]
        elif f == 420:
            ok = world.shoot((1100.0, 700.0), (mx, my), "Wall", homing=0.5, lead=(30.0, -20.0))
            ev = ["Wall", [1100.0, 700.0], [mx, my], 0.5, [30.0, -20.0], ok]
        elif f == 800:
            ok = world.shoot((100.0, 700.0), (50.0, 50.0), "trap")
            ev = ["trap", [100.0, 700.0], [50.0, 50.0], 0.0, [0.0, 0.0], ok]
        elif f == 1000:
            ok = world.shoot((100.0, 100.0), (900.0, 300.0), "trap", homing=1.0)
            ev = ["trap", [100.0, 100.0], [900.0, 300.0], 1.0, [0.0, 0.0], ok]
        if ev is not None:
            events.append([f] + ev)
        if f == 1060:
            world.enabled = False
        if f == 1100:
            world.enabled = True
        # The pointer: pulled where the silk wants it, plus the hand's own wiggle.
        if 800 <= f < 1000:
            base = (1100.0, 100.0)
        elif desired is not None:
            base = desired
        else:
            base = (600.0 + (f % 17), 400.0 + (f % 13))
            if f >= 1000:
                base = (900.0 + (f % 7), 300.0 + (f % 5))
        wiggle = (0.0, 0.0)
        if (f // 45) % 3 != 0:
            wiggle = (float((f * 37) % 23 - 11), float((f * 53) % 19 - 9))
        mx, my = base[0] + wiggle[0], base[1] + wiggle[1]
        desired = world.update(DT, mx, my)
        cap = world.capture
        hold = None if cap is None else getattr(cap, "hold", None)
        rows.append([
            mx, my, None if desired is None else list(desired), world.active_kind(),
            None if world.projectile is None else list(world.projectile.pos),
            None if cap is None else list(cap.bbox()),
            None if hold is None else [hold.struggle, hold.hold_t, hold.wobble, list(hold.strain)],
            len(world.spent), [list(r) for r in world.dirty_rects()],
        ])
    return {"seed": seed, "can_control": can_control, "events": events, "rows": rows,
            "next": world.rng.random()}


# ======================================================================
# Colony bases
# ======================================================================
class JobSpider:
    def __init__(self, index, job, team, x, y, size=20.0, level=1, hp=None):
        self.index = index
        self.job_id = job
        self.x = float(x)
        self.y = float(y)
        self.size = size
        self.level = level
        self.progression_id = f"test:{index}"
        self.progression = SimpleNamespace(team_id=team)
        self.dragging = False
        self.airborne = False
        self.job_busy = False
        self.dead = False
        self.state = "Idle"
        self._hunting_prey = False
        self.rng = random.Random(900 + index)
        self.job_mode = "idle"
        self.job_target = None
        self.job_alert_target = None
        self.job_base_id = None
        self.job_facing = None
        if hp is not None:
            self.hp = hp
            self.heal = self._heal

    def _heal(self, amount):
        before = self.hp
        self.hp = min(100.0, self.hp + max(0.0, float(amount)))
        return self.hp - before

    def relation_to(self, other):
        mine = self.progression.team_id
        theirs = other.progression.team_id
        if mine == "neutral" or theirs == "neutral":
            return "neutral"
        return "friend" if mine == theirs else "foe"


COLONY = [
    # job, team, x, y, level, hp
    ("builder", "pack_a", 300.0, 300.0, 1, 40.0),
    ("guard", "pack_a", 330.0, 300.0, 2, None),
    ("guard", "pack_a", 300.0, 340.0, 1, None),
    ("scout", "pack_a", 280.0, 320.0, 1, 50.0),
    ("webber", "pack_a", 320.0, 330.0, 3, None),
    ("hunter", "pack_a", 350.0, 360.0, 1, None),
    ("none", "rivals", 1200.0, 700.0, 1, None),
    ("hunter", "rivals", 1300.0, 650.0, 4, 30.0),
    ("builder", "neutral", 900.0, 150.0, 1, None),
    ("hunter", "neutral", 1000.0, 200.0, 1, None),
    ("guard", "rivals", 1250.0, 720.0, 1, None),
]


def colony_flags(c, f):
    """Scripted inputs; mirrored in Rust."""
    i = c.index
    c.job_busy = i == 3 and 700 <= f < 760
    c.dragging = i == 1 and 1000 <= f < 1030
    c.airborne = i == 2 and 1100 <= f < 1120
    c._hunting_prey = (i == 5 and 400 <= f < 500) or (i == 7 and 1300 <= f < 1400)
    c.state = "Feed" if ((i == 5 and (200 <= f < 230 or 900 <= f < 930)) or (i == 7 and 1500 <= f < 1520)) else "Idle"


def colony_move(c, f):
    """Walk toward the job target at 150 px/s; the foe roams. Mirrored in Rust."""
    if c.index == 6:
        t = f * DT
        c.x = tri(1200.0, -170.0, 60.0, 1540.0, t)
        c.y = tri(700.0, -90.0, 60.0, 840.0, t)
        return
    if c.job_target is None or c.dragging:
        return
    tx, ty = c.job_target
    dx, dy = tx - c.x, ty - c.y
    d = math.hypot(dx, dy)
    step = 150.0 * DT
    if d <= step:
        c.x, c.y = tx, ty
    else:
        c.x += dx / d * step
        c.y += dy / d * step


def colony_trace():
    rng = random.Random(4)
    world = J.BaseWorld(1600, 900, rng=rng)
    web_world = W.WebWorld(1600, 900, rng=random.Random("9:webs"))
    creatures = [JobSpider(i, job, team, x, y, level=level, hp=hp)
                 for i, (job, team, x, y, level, hp) in enumerate(COLONY)]
    site = world.ensure_site(creatures[0])
    site.resources = 30.0
    # A finished web beside the base, torn before the first frame, so the web
    # tender's first job is a mend.
    strands, hub, wob = W._plan_orb((420.0, 430.0), 80.0, 1600.0, 900.0, rng=random.Random(31))
    near = W.Web("orb", strands, hub, wob, "test_near", 20.0, 1600.0, 900.0, rng=web_world.rng)
    near.state = "complete"
    near.built = len(near.strands)
    web_world.webs.append(near)
    for k in range(-5, 6):
        near.damage_near(hub[0] + k * 8.0, hub[1] + 2.0)
    rows = []
    bases = {}
    events = []
    for f in range(2000):
        for c in creatures:
            colony_flags(c, f)
        ev = []
        if f % 120 == 60:
            world.credit_team_food("pack_a", J.FLY_CATCH_RESOURCE_AMOUNT)
            world.credit_team_food("rivals", 2.5)
            ev.append("food")
        if f == 600:
            s = world.bases["team:pack_a"]
            s.integrity = 40.0
            ev.append("damage")
        if f == 1500:
            for web in web_world.webs:
                if web.is_complete():
                    hx, hy = web.hub
                    for k in range(-6, 7):
                        web.damage_near(hx + k * 8.0, hy + 2.0)
                    ev.append("tear")
                    break
        if f == 1800:
            world.move_base("team:rivals", 1400.0, 820.0)
            ev.append("move")
        if ev:
            events.append([f, ev])
        world.update(DT, creatures, web_world)
        web_world.update(DT, None)
        row = []
        for c in creatures:
            alert = None
            if c.job_alert_target is not None:
                alert = creatures.index(c.job_alert_target)
            row.append([c.job_mode, None if c.job_target is None else list(c.job_target), alert,
                        c.job_base_id, c.job_facing])
        rows.append(row)
        if f % 60 == 59:
            bases[str(f)] = {
                "bases": world.to_dict(),
                "hp": [getattr(c, "hp", None) for c in creatures],
                "pos": [[c.x, c.y] for c in creatures],
                "webs": [[w.built, w.active_t, w.state, len(w.cut), list(w.hub)] for w in web_world.webs],
                "duty": [None if world.duty_state(c) is None else [world.duty_state(c).on_duty,
                                                                     world.duty_state(c).remaining]
                         for c in creatures],
            }
        for c in creatures:
            colony_move(c, f)
    return {"colony": [list(row) for row in COLONY], "events": events, "rows": rows, "checkpoints": bases,
            "next": rng.random(), "webs_next": web_world.rng.random(),
            "creature_next": [c.rng.random() for c in creatures]}


def base_parts():
    mounds = []
    for site_id in ("team:pack_a", "owner:test:1", "demo|1", "team:hunters|base-7"):
        for level in (0, 2, 5):
            radius = 30.0 + level * 10.0
            mounds.append({"id": site_id, "radius": radius, "plan": [list(m) for m in J._plan_mounds(site_id, radius)]})
    sites = []
    for completion in (0.0, 0.04, 0.5, 0.54, 0.99, 1.0):
        s = J.BaseSite(id="demo|1", owner_id="o", team_id="hunters", x=400.0, y=300.0,
                       build_progress=J.MAX_BUILD_PROGRESS * completion, level=2)
        sites.append({"completion": completion, "mounds": [list(m) for m in s.mounds()]})
    recipes = {}
    for key in ("demo|1:field", "demo|1:grit", "demo|1:patch:10.00:20.00", "colony|7:patch:100.00:200.00"):
        r = J.patch_recipe(key)
        recipes[key] = {"body": [list(p) for p in r["body"]], "crown": [list(p) for p in r["crown"]],
                        "crown_offset": list(r["crown_offset"]), "clods": [list(c) for c in r["clods"]]}
    blends = []
    for base in ((46, 34, 25), (86, 62, 42), (118, 91, 63)):
        for towards in ((255, 0, 0), (0, 255, 0), (12, 200, 99), (255, 255, 255)):
            for amount in (0.05, 0.08, 0.13, 0.5):
                blends.append([list(base), list(towards), amount, list(J._blend(base, towards, amount))])
    raw = [
        {"id": "team:a", "owner_id": "o", "team_id": "a", "x": 10, "y": "20", "build_progress": 500,
         "level": 9, "integrity": -4, "max_integrity": 0.5, "alert": 3, "last_alert": -1,
         "patrol_angle": "1.5", "resources": 5000, "larder": -3,
         "points_of_interest": [{"k": i} for i in range(11)] + [5, "x"]},
        {"id": "", "owner_id": "o"},
        {"id": "team:b", "owner_id": ""},
        {"id": "team:c", "owner_id": "p", "team_id": "", "x": "bad"},
        {"id": "team:d", "owner_id": "p", "team_id": "x" * 40, "level": "2"},
        {"id": "team:e", "owner_id": "p", "level": 2.9, "points_of_interest": None},
        {"id": "team:f", "owner_id": "p", "points_of_interest": "abc"},
        {"id": "team:g", "owner_id": "p", "points_of_interest": 7},
        {"id": 5, "owner_id": None, "x": True},
        "nope",
    ]
    parsed = []
    for value in raw:
        site = J.BaseSite.from_dict(value)
        parsed.append(None if site is None else site.to_dict())
    return {"mounds": mounds, "sites": sites, "recipes": recipes, "blends": blends,
            "from_dict": [[r, p] for r, p in zip(raw, parsed)]}


# ======================================================================
# Cages and carcasses
# ======================================================================
def cage_cases():
    rng = random.Random(8)
    cases = []
    for _ in range(150):
        cage = Cage(rng.uniform(-200, 1800), rng.uniform(-200, 1000), rng.uniform(20, 900), rng.uniform(20, 700))
        start = list(cage.rect())
        inset = rng.choice((0.0, 12.0, 40.0, 500.0))
        px, py = rng.uniform(cage.x - 40, cage.right() + 40), rng.uniform(cage.y - 40, cage.bottom() + 40)
        corner_probe = (cage.right() + rng.uniform(-14, 14), cage.y + rng.uniform(-14, 14))
        corner = rng.choice(("nw", "ne", "sw", "se"))
        rx, ry = rng.uniform(-300, 2000), rng.uniform(-300, 1200)
        sw, sh = rng.choice(((1920.0, 1080.0), (300.0, 200.0), (100.0, 80.0)))
        out = {
            "start": start, "inset": inset, "p": [px, py], "corner_probe": list(corner_probe),
            "interior": list(cage.interior(inset)), "clamp": list(cage.clamp_center(px, py, inset)),
            "contains": cage.contains_point(px, py), "on_border": cage.on_border(px, py),
            "corner_at": cage.corner_at(*corner_probe), "outer": list(cage.outer_bbox()),
            "footprint": list(cage.footprint_rect()), "bands": [list(r) for r in cage.border_rects()],
            "corner": corner, "resize_to": [rx, ry], "screen": [sw, sh],
        }
        cage.resize_corner(corner, rx, ry)
        out["resized"] = list(cage.rect())
        cage.clamp_to_screen(sw, sh)
        out["clamped"] = list(cage.rect())
        cases.append(out)
    raw = [{"x": 1, "y": 2, "w": 3, "h": 4}, {"x": "5", "y": 2.5, "w": 300, "h": "200"},
           {"x": 1, "y": 2, "w": 3}, {"x": None, "y": 2, "w": 3, "h": 4}, "cage", [1, 2, 3, 4]]
    parsed = []
    for value in raw:
        cage = Cage.from_dict(value)
        parsed.append(None if cage is None else cage.to_dict())
    return {"cases": cases, "from_dict": [[r, p] for r, p in zip(raw, parsed)]}


def carcass_trace():
    out = []
    script = random.Random(12)
    for k in range(4):
        c = Carcass(100.0 + k * 50.0, 200.0, 6.0 + k * 9.0, "" if k == 2 else "pack_a",
                    {"body": (120, 80, 60)})
        c.damage_numbers = [[3.0 + k, 0.0, 0.0]]
        c.damage_team = "rivals" if k % 2 else "pack_a"
        bodies = [SimpleNamespace(x=c.x + 5.0, y=c.y, dead=False, dragging=False),
                  SimpleNamespace(x=c.x + 500.0, y=c.y, dead=False, dragging=False)]
        rows = []
        done_at = None
        for f in range(1800):
            bodies[0].x = c.x + (5.0 if (f // (60 + k * 30)) % 2 else 400.0)
            bodies[1].dragging = f % 2 == 0
            if f % 97 == 3:
                c._note_damage(script.choice((0.0, 0.5, 2.0, 7.0)))
            eaters = eaters_near(c, bodies)
            done = c.update(DT, eaters)
            if f % 10 == 0 or done:
                rows.append([f, eaters, c.t, c.settle_t, c.spent, c.being_eaten, done, list(c.footprint()),
                             [list(n) for n in c.damage_numbers]])
            if done:
                done_at = f
                break
        out.append({"k": k, "size": c.size, "team": c.team_id, "legs": [list(l) for l in c.legs],
                    "damage_top": c._damage_top(), "rows": rows, "done_at": done_at,
                    "color": list(c.damage_color())})
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    data = {
        "webs.json": {"plans": web_plans(), "claims": web_claims(), "trace": web_trace(),
                      "persistence": web_persistence()},
        "flies.json": {"scenarios": [fly_scenario("few", 3, 4, 900, False, 10),
                                     fly_scenario("crowd", 8, 40, 600, True, 14)],
                       "parts": fly_parts()},
        "mouse_webs.json": [mouse_case(1, True), mouse_case(2, False), mouse_case(3, True)],
        "jobs.json": {"colony": colony_trace(), "parts": base_parts()},
        "cage.json": cage_cases(),
        "carcass.json": carcass_trace(),
    }
    for name, value in data.items():
        text = json.dumps(value, separators=(",", ":"))
        (OUT / name).write_text(text + "\n", encoding="utf-8")
        print(name, len(text))


if __name__ == "__main__":
    main()
