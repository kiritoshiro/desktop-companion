"""Dump Python oracle output for dc-world: tactics plans, screen layouts, playfield clamps.

Run from anywhere:
    <venv>/python.exe -I rust/crates/dc-world/tools/dump_fixtures.py
Writes rust/crates/dc-world/tests/fixtures/*.json.
"""
from __future__ import annotations

import json
import os
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
WORKTREE = HERE.parents[3]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from desktop_bug.world import tactics  # noqa: E402
from desktop_bug.world.playfield import Playfield, ScreenRect  # noqa: E402
from desktop_bug.world.screen_layout import TUNNEL_REACH, ScreenLayout  # noqa: E402

OUT = HERE.parent / "tests" / "fixtures"
ROLES = ("tank", "melee", "fast", "ranged", "support")


def pt(p):
    return None if p is None else [p[0], p[1]]


def tactics_cases():
    rng = random.Random(20261018)
    cases = []
    for n in range(600):
        count = rng.randint(1, 6)
        spread = rng.choice((60.0, 200.0, 500.0, 900.0))
        members = []
        for i in range(count):
            role = rng.choice(ROLES)
            members.append({
                "key": f"m{i}", "x": rng.uniform(-spread, spread), "y": rng.uniform(-spread, spread),
                "health": rng.choice((1.0, rng.random())), "role": role,
                "reach": 180.0 if role in ("ranged", "support") else rng.choice((34.0, 50.0, 160.0)),
                "anchor": None if rng.random() < 0.2 else [rng.uniform(-300, 300), rng.uniform(-300, 300)],
            })
        foes = [{"key": f"f{i}", "x": rng.uniform(-900, 900), "y": rng.uniform(-900, 900),
                 "health": rng.random(), "threat": rng.choice((1.0, 1.6, 0.1, 3.0))}
                for i in range(rng.randint(0, 4))]
        level = rng.randint(0, 3)
        stance = rng.choice(("defend", "attack"))
        anchor = None if rng.random() < 0.3 else [rng.uniform(-200, 200), rng.uniform(-200, 200)]
        notice = rng.choice((360.0, 100.0, 800.0))
        ms = [tactics.Member(m["key"], m["x"], m["y"], m["health"], m["role"], m["reach"],
                             anchor=None if m["anchor"] is None else tuple(m["anchor"])) for m in members]
        fs = [tactics.Foe(f["key"], f["x"], f["y"], f["health"], f["threat"]) for f in foes]
        result = tactics.plan(ms, fs, level, stance, tuple(anchor) if anchor else None, notice)
        cases.append({
            "members": members, "foes": foes, "level": level, "stance": stance, "anchor": anchor,
            "notice": notice,
            "doctrine": result.doctrine, "focus": result.focus,
            "orders": [{"key": k, "mode": o.mode, "target": o.target, "point": pt(o.point), "note": o.note}
                       for k, o in result.orders.items()],
        })
    return cases


LAYOUTS = [
    [[0, 718, 2560, 1440], [2560, 0, 3840, 2160]],
    [[0, 0, 1600, 1000], [1600, 0, 1600, 1000]],
    [[0, 0, 1600, 1000], [1900, 200, 1280, 800]],
    [[0, 0, 1600, 1000], [1600, 0, 1600, 1000], [4000, 0, 1200, 900]],
    [[0, 0, 1920, 1080], [0, 1080, 1920, 1080], [1920, 500, 1280, 1024]],
    [[0, 0, 800, 600], [800, 0, 500, 360]],
    [[0, 0, 1920, 1080]],
    [[0, 0, 1000, 800], [1000, 790, 900, 700], [3000, 3000, 640, 480], [-50, 2000, 400, 400]],
    [[1920, 0, 2560, 1440], [0, 300, 1920, 1080], [4480, 100, 1080, 1920]],
]


def layout_cases():
    rng = random.Random(77)
    cases = []
    for rects in LAYOUTS:
        for primary in range(len(rects)):
            sr = [ScreenRect(*map(float, r)) for r in rects]
            layout = ScreenLayout(sr, primary)
            right = max(r.right for r in sr)
            bottom = max(r.bottom for r in sr)
            queries = []
            for _ in range(80):
                x, y = rng.uniform(-200, right + 200), rng.uniform(-200, bottom + 200)
                tx, ty = rng.uniform(-200, right + 200), rng.uniform(-200, bottom + 200)
                margin = rng.choice((0.0, 20.0, 40.0, 300.0))
                hit = layout.tunnel_at(x, y, TUNNEL_REACH * 20)
                queries.append({
                    "x": x, "y": y, "tx": tx, "ty": ty, "margin": margin,
                    "screen_at": layout.screen_at(x, y).index,
                    "route": list(layout.route(x, y, tx, ty)),
                    "clamp": list(layout.clamp(x, y, margin)),
                    "contains": layout.playfield.contains(x, y, margin),
                    "tunnel_at": None if hit is None else [layout.links.index(hit[0]), list(hit[1])],
                })
            # Door crossings: from each door's near side.
            for link in layout.links:
                for near, far_screen in ((link.a_point, link.b), (link.b_point, link.a)):
                    tx, ty = layout.screens[far_screen].centre
                    hit = layout.tunnel_at(near[0], near[1], TUNNEL_REACH * 20)
                    queries.append({
                        "x": near[0], "y": near[1], "tx": tx, "ty": ty, "margin": 0.0,
                        "screen_at": layout.screen_at(*near).index,
                        "route": list(layout.route(near[0], near[1], tx, ty)),
                        "clamp": list(layout.clamp(near[0], near[1], 0.0)),
                        "contains": layout.playfield.contains(near[0], near[1], 0.0),
                        "tunnel_at": None if hit is None else [layout.links.index(hit[0]), list(hit[1])],
                    })
            paths = {f"{a}-{b}": [layout.links.index(l) for l in layout.path(a, b)]
                     for a in range(layout.count) for b in range(layout.count)}
            cases.append({
                "rects": rects, "primary": primary, "simple": layout.playfield.simple,
                "links": [{"a": l.a, "b": l.b, "kind": l.kind, "a_point": list(l.a_point),
                           "b_point": list(l.b_point)} for l in layout.links],
                "names": [s.name for s in layout.screens],
                "paths": paths, "queries": queries,
            })
    return cases


def playfield_cases():
    rng = random.Random(3)
    cases = []
    for rects in LAYOUTS:
        sr = [ScreenRect(*map(float, r)) for r in rects]
        w, h = max(r.right for r in sr), max(r.bottom for r in sr)
        field = Playfield(w, h, sr)
        pts = []
        for _ in range(150):
            x, y = rng.uniform(-100, w + 100), rng.uniform(-100, h + 100)
            margin = rng.choice((0.0, 10.0, 40.0, 200.0))
            pts.append({"x": x, "y": y, "margin": margin, "contains": field.contains(x, y, margin),
                        "clamp": list(field.clamp(x, y, margin))})
        cases.append({"width": w, "height": h, "rects": rects, "simple": field.simple, "points": pts})
    return cases


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, data in (("tactics.json", tactics_cases()), ("screen_layout.json", layout_cases()),
                       ("playfield.json", playfield_cases())):
        (OUT / name).write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")
        print(name, len(data))


if __name__ == "__main__":
    main()
