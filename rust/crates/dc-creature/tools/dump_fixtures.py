"""Dump parity fixtures for dc-creature from the Python reference.

Run from anywhere with the project venv:

    C:/Users/win/Documents/Workspace/projects/desktop-companion/.venv/Scripts/python.exe -I \
        rust/crates/dc-creature/tools/dump_fixtures.py

Creatures are made exactly as `CreatureManager._create_creature` makes them
(seeded with the manager's seed and their own progression id). The
constructor arguments are recorded through a recording `Creature` subclass so
the Rust replay builds the same spider from the same model and personality
dicts. Each scenario then drives the creatures through the real
`Creature.update` with a recorded cursor path, with no web, fly, mouse-web or
base world attached (the world-less path every Python unit test uses), and
records a per-frame trace. Drag events (start / drag_to / release) are
recorded and replayed the same way.

Output: `tests/fixtures/<scenario>.json` next to this crate.
"""

from __future__ import annotations

import copy
import json
import math
import os
import random
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
WORKTREE = CRATE.parents[2]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ["DESKTOP_BUG_STATE_DIR"] = tempfile.mkdtemp(prefix="dc-creature-fixtures-")

import desktop_bug.manager.core as mcore  # noqa: E402
from desktop_bug.manager import CreatureManager  # noqa: E402
from desktop_bug.world.webs import WebWorld  # noqa: E402
from desktop_bug.world.cage import Cage  # noqa: E402

# A creature's `_begin_weave` calls `claim_site(self, prefer_corner=...)` with
# no `rng`, which in Python falls back to the unseeded module-level `random`.
# dc-world draws the site from the web world's own stream instead (its port
# decision); the fixtures follow dc-world so the run is replayable at all.
_orig_claim_site = WebWorld.claim_site


def _claim_site(self, creature, prefer_corner=True, pattern_weights=None, rng=None):
    return _orig_claim_site(self, creature, prefer_corner, pattern_weights, rng if rng is not None else self.rng)


WebWorld.claim_site = _claim_site

OUT = CRATE / "tests" / "fixtures"
DT = 1.0 / 60.0
SW, SH = 1600, 900
SEED = 11

RECORDS: list = []


class RecordingCreature(mcore.Creature):
    def __init__(self, model, personality, screen_w, screen_h, **kw):
        RECORDS.append({
            "model": json.loads(json.dumps(model, default=str)),
            "personality": json.loads(json.dumps(personality, default=str)),
            "screen_w": screen_w,
            "screen_h": screen_h,
            "kw": json.loads(json.dumps(kw, default=str)),
        })
        super().__init__(model, personality, screen_w, screen_h, **kw)


mcore.Creature = RecordingCreature


def rnd(value):
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return value
    if isinstance(value, (int, float)):
        return float("%.13g" % float(value))
    if isinstance(value, (list, tuple)):
        return [rnd(v) for v in value]
    if isinstance(value, dict):
        return {k: rnd(v) for k, v in value.items()}
    return value


def phase_of(c):
    cur = c.phase_scheduler.current
    return None if cur is None else cur.phase_id


def frame_row(c):
    m = c.mood
    return [
        c.state,
        rnd(c.x), rnd(c.y), rnd(c.heading),
        rnd(c.state_timer), rnd(c.decision_timer),
        rnd(c.speed), rnd(c.current_speed),
        rnd(c.target_x), rnd(c.target_y),
        rnd(m.valence), rnd(m.arousal), rnd(m.affection), rnd(m.curiosity),
        rnd(c.energy), rnd(c.jump_z), rnd(c.crouch), rnd(c.focus_x), rnd(c.focus_y),
        phase_of(c),
        rnd(c.hp), rnd(c.level), rnd(c.flee_timer), c.webbed,
    ]


FRAME_COLUMNS = [
    "state", "x", "y", "heading", "state_timer", "decision_timer", "speed", "current_speed",
    "target_x", "target_y", "valence", "arousal", "affection", "curiosity", "energy", "jump_z",
    "crouch", "focus_x", "focus_y", "phase", "hp", "level", "flee_timer", "webbed",
]


def init_snapshot(c):
    """Construction-time state the replay checks before the first frame."""
    legs = [[rnd(leg.foot_x), rnd(leg.foot_y), rnd(leg.phase_seed), rnd(leg.gait_phase_offset)] for leg in c.legs]
    return {
        "x": rnd(c.x), "y": rnd(c.y), "heading": rnd(c.heading), "size": rnd(c.size),
        "turn_rate": rnd(c.turn_rate), "state_timer": rnd(c.state_timer),
        "decision_timer": rnd(c.decision_timer), "hop_timer": rnd(c.hop_timer),
        "blink_timer": rnd(c.blink_timer), "social_cooldown": rnd(c.social_cooldown),
        "weave_cooldown": rnd(c.weave_cooldown), "web_walk_cooldown": rnd(c.web_walk_cooldown),
        "web_shot_cooldown": rnd(c.web_shot_cooldown), "pounce_cooldown": rnd(c._pounce_cooldown),
        "trap_shot_cooldown": rnd(c._trap_shot_cooldown), "weave_speed": rnd(c._weave_speed),
        "max_hp": rnd(c.max_hp), "damage": rnd(c.damage), "armor": rnd(c.armor),
        "mood": rnd([c.mood.valence, c.mood.arousal, c.mood.affection, c.mood.curiosity]),
        "active_gait_index": c.active_gait_index,
        "deck": list(c.phase_scheduler.deck),
        "legs": legs,
    }


def make(manager, model_id, personality_id, index, pid, team="neutral", job="none", pos=None, skills=None):
    model = manager.models[model_id]
    personality = manager.personalities[personality_id]
    if skills is None:
        skills = mcore.skills_with_default_abilities(personality, mcore.job_ability_ids(job))
    before = len(RECORDS)
    c = manager._create_creature(
        model, personality, index, pos=pos, skills=skills, progression_id=pid, team_id=team, job_id=job
    )
    assert len(RECORDS) == before + 1
    rec = RECORDS[-1]
    c.web_world = None
    c.mouse_web_world = None
    c.fly_world = None
    c.base_world = None
    c.playfield = None
    spec = {
        "ctor": rec,
        "team": team,
        "pos": list(pos) if pos is not None else None,
        "allow_social": bool(manager.social_play),
        "mood_mode": manager.mood_mode,
        "team_stances": json.loads(json.dumps(manager.team_stances or {})),
    }
    return c, spec


def cursor_path(frames, rng, anchors, kinds=None):
    """Off screen, slow approaches, holds, fast swipes and orbits."""
    kinds = kinds or ["off", "walk", "hold", "swipe", "orbit", "walk", "hold"]
    path = []
    f = 0
    mx, my = -5000.0, -5000.0
    while f < frames:
        kind = rng.choice(kinds)
        n = rng.randint(30, 90)
        ax, ay = rng.choice(anchors)
        if kind == "off":
            for _ in range(n):
                path.append((-5000.0, -5000.0))
            mx, my = -5000.0, -5000.0
        elif kind == "walk":
            sx, sy = (mx, my) if mx > -1000 else (ax + rng.uniform(-400, 400), ay + rng.uniform(-300, 300))
            tx, ty = ax + rng.uniform(-60, 60), ay + rng.uniform(-60, 60)
            for i in range(n):
                t = (i + 1) / n
                path.append((sx + (tx - sx) * t, sy + (ty - sy) * t))
            mx, my = tx, ty
        elif kind == "hold":
            if mx < -1000:
                mx, my = ax + rng.uniform(-150, 150), ay + rng.uniform(-150, 150)
            for _ in range(n):
                path.append((mx, my))
        elif kind == "swipe":
            ang = rng.uniform(-math.pi, math.pi)
            sx, sy = ax - math.cos(ang) * 260, ay - math.sin(ang) * 260
            tx, ty = ax + math.cos(ang) * 260, ay + math.sin(ang) * 260
            n = rng.randint(8, 16)
            for i in range(n):
                t = (i + 1) / n
                path.append((sx + (tx - sx) * t, sy + (ty - sy) * t))
            mx, my = tx, ty
        else:
            r = rng.uniform(60, 200)
            a0 = rng.uniform(-math.pi, math.pi)
            for i in range(n):
                a = a0 + i * 0.06
                path.append((ax + math.cos(a) * r, ay + math.sin(a) * r))
            mx, my = path[-1]
        f = len(path)
    return [(float("%.17g" % x), float("%.17g" % y)) for x, y in path[:frames]]


def web_row(ww):
    return [[w.state, rnd(w.progress()), rnd(w.intact_fraction()) if hasattr(w, "intact_fraction") else None]
            for w in ww.webs]


def run(name, creatures, specs, frames, path_seed, group, events=None, note="", web_seed=None, kinds=None):
    events = events or {}
    ww = None
    if web_seed is not None:
        ww = WebWorld(SW, SH, rng=random.Random(web_seed))
        for c in creatures:
            c.web_world = ww
    webs_trace = []
    rng = random.Random(path_seed)
    anchors = [(c.x, c.y) for c in creatures] + [(SW * 0.5, SH * 0.5)]
    path = cursor_path(frames, rng, anchors, kinds)
    init = [init_snapshot(c) for c in creatures]
    trace = []
    dragging = set()
    for f in range(frames):
        mx, my = path[f]
        for ev in events.get(f, []):
            kind, i = ev[0], ev[1]
            if kind == "start":
                creatures[i].start_drag(mx, my)
                dragging.add(i)
            elif kind == "release":
                creatures[i].release_drag(mx, my)
                dragging.discard(i)
            elif kind == "foe":
                creatures[i]._foe = None if ev[2] < 0 else creatures[ev[2]]
            elif kind == "pin":
                creatures[i].web_pinned("trap", None)
            elif kind == "cage":
                creatures[i].cage = Cage(*ev[2:6])
            elif kind == "damage":
                creatures[i].take_damage(ev[2], None)
            elif kind == "xp":
                creatures[i].gain_experience(ev[2])
            elif kind == "flee":
                creatures[i].flee_timer = ev[2]
                creatures[i].flee_from = creatures[ev[3]]
            else:
                raise ValueError(kind)
        for i in sorted(dragging):
            creatures[i].drag_to(DT, mx, my)
        for c in creatures:
            if group:
                c.set_neighbors(list(creatures))
            else:
                c.set_neighbors([c])
            c.update(DT, mx, my, SW, SH)
        if ww is not None:
            ww.update(DT, (mx, my))
            webs_trace.append(web_row(ww))
        trace.append([frame_row(c) for c in creatures])
    data = {
        "name": name,
        "note": note,
        "dt": DT,
        "screen": [SW, SH],
        "group": group,
        "columns": FRAME_COLUMNS,
        "creatures": specs,
        "init": init,
        "path": path,
        "events": {str(k): v for k, v in events.items()},
        "trace": trace,
        "web_seed": web_seed,
        "webs": webs_trace,
    }
    OUT.mkdir(parents=True, exist_ok=True)
    with open(OUT / f"{name}.json", "w", encoding="utf-8") as fh:
        json.dump(data, fh, separators=(",", ":"))
    states = sorted({row[0] for frame in trace for row in frame})
    print(f"{name}: {len(creatures)} creature(s), {frames} frames, states {states}")


def main():
    manager = CreatureManager(WORKTREE / "presets" / "colony.json", SW, SH, seed=SEED)
    manager.base_world.clear()

    # 1. The colony's five spiders, each on its own.
    colony = [
        ("plush_snow_hybrid", "balanced", "pack_a", "builder"),
        ("tarantula", "cautious", "pack_a", "guard"),
        ("blue_jewel", "curious", "pack_a", "scout"),
        ("moss_velvet_orbweaver", "webber", "pack_a", "webber"),
        ("spider", "bold", "rivals", "hunter"),
    ]
    for i, (model, pers, team, job) in enumerate(colony):
        c, spec = make(manager, model, pers, i, f"fixture:colony:{i}", team=team, job=job)
        run(f"colony_solo_{i}", [c], [spec], 600, 100 + i, group=False)

    # 2. Every personality file on the plain spider model, on its own.
    pids = sorted(p for p in manager.personalities if not p.startswith("_"))
    for i, pid in enumerate(pids):
        if pid not in {
            "bashful", "bold", "camouflage", "clingy", "cuddly", "curious", "drifter", "explorer",
            "grumpy", "hunter", "jumper", "mellow", "nope", "observer", "playful", "shy",
            "skittish", "sleepy", "trapper", "webber", "zoomy", "balanced", "cautious",
        }:
            continue
        c, spec = make(manager, "spider", pid, i, f"fixture:personality:{pid}")
        run(f"personality_{pid}", [c], [spec], 420, 300 + i, group=False)

    # 3. The colony together, neighbours visible to each other.
    group, specs = [], []
    for i, (model, pers, team, job) in enumerate(colony):
        c, spec = make(manager, model, pers, i, f"fixture:group:{i}", team=team, job=job)
        group.append(c)
        specs.append(spec)
    run("colony_group", group, specs, 600, 900, group=True)

    # 4. A sociable cluster placed close together (play, cuddle, inspect).
    social, specs = [], []
    for i, pid in enumerate(["playful", "cuddly", "curious", "zoomy"]):
        pos = (700.0 + 70.0 * (i % 2), 420.0 + 70.0 * (i // 2))
        c, spec = make(manager, "spider", pid, i, f"fixture:social:{i}", pos=pos)
        social.append(c)
        specs.append(spec)
    run("social_group", social, specs, 900, 901, group=True)

    # 5. A pick-up, a carry and a throw, twice.
    c, spec = make(manager, "spider", "skittish", 0, "fixture:drag:0")
    events = {60: [["start", 0]], 140: [["release", 0]], 260: [["start", 0]], 300: [["release", 0]]}
    run("drag_throw", [c], [spec], 480, 902, group=False, events=events)

    # 7. Two rivals published as each other's foe, hurt, pinned and routed.
    fighters, specs = [], []
    for i, (pid, team, pos) in enumerate([("bold", "pack_a", (700.0, 450.0)), ("grumpy", "rivals", (860.0, 470.0))]):
        c, spec = make(manager, "spider", pid, i, f"fixture:fight:{i}", team=team, pos=pos)
        fighters.append(c)
        specs.append(spec)
    events = {
        20: [["foe", 0, 1], ["foe", 1, 0]],
        90: [["damage", 1, 14.0]],
        120: [["xp", 0, 60.0]],
        160: [["damage", 0, 9.0], ["damage", 1, 22.0]],
        200: [["pin", 1]],
        330: [["damage", 1, 30.0]],
        360: [["flee", 1, 3.0, 0], ["foe", 1, -1]],
        600: [["foe", 0, -1]],
    }
    run("fight", fighters, specs, 900, 905, group=True, events=events, kinds=["off"])

    # 8. A caged zoomy spider teased by the cursor.
    c, spec = make(manager, "spider", "zoomy", 0, "fixture:cage:0")
    run("caged", [c], [spec], 900, 906, group=False, events={0: [["cage", 0, 600.0, 300.0, 260.0, 220.0]]})

    # 6. A web world: a webber builds, others walk and mend what the cursor tears.
    c, spec = make(manager, "moss_velvet_orbweaver", "webber", 0, "fixture:webs:solo")
    run("webs_webber", [c], [spec], 2400, 903, group=False, web_seed=f"{SEED}:webs",
        kinds=["off", "off", "off", "swipe", "swipe"])
    webbers, specs = [], []
    for i, (model, pid, pos) in enumerate([("moss_velvet_orbweaver", "webber", (300.0, 250.0)),
                                           ("spider", "explorer", (1300.0, 650.0)),
                                           ("spider", "webber", (800.0, 700.0))]):
        c, spec = make(manager, model, pid, i, f"fixture:webs:{i}", pos=pos)
        webbers.append(c)
        specs.append(spec)
    run("webs_group", webbers, specs, 3000, 904, group=True, web_seed=f"{SEED}:webs",
        kinds=["off", "off", "off", "swipe", "swipe", "orbit"])


if __name__ == "__main__":
    main()
