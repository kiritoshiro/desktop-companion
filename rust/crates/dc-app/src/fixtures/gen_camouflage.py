"""Dump Python's camouflage strength trace (`_refresh_camouflage_samples`).

The oracle for dc-app's `camouflage::tests::strength_trace_matches_python`.
Three scripted spiders, each a plain namespace carrying exactly the fields the
engine reads, with the real `register_camouflage_touch` bound to it, go
through 140 samples (16.8 s at 120 ms). The engine method is called unbound
on a stand-in window, so no Qt window is made.

* `hider`: the shipped Camouflage personality. Still until it has faded in,
  then a lifted leg, a drag, a walk, a walk while wholly behind a window,
  a leg about to step, a crawl below the threshold.
* `plain`: the shipped Curious personality (no camouflage) left at 0.5
  strength, which decays to nothing.
* `tuned`: Camouflage with a short delay and fade, a reveal speed given as a
  string, a fade speed that is not a number, a long motion reveal, a throw
  (inertia), a stepping leg and a leg lifted exactly to the threshold.

Each sample lists what differs from a spider at rest (`in`) and the
strength and timers after it, so the Rust test replays the same inputs onto
real creatures. `leg` is `[index, stepping, pending_step, lift]`.

Run from the repository root:
    .venv/Scripts/python.exe -I rust/crates/dc-app/src/fixtures/gen_camouflage.py
Writes rust/crates/dc-app/src/fixtures/camouflage.json.
"""
from __future__ import annotations

import json
import os
import sys
import types
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from desktop_bug.app.engine import OverlayWindow  # noqa: E402
from desktop_bug.creature.behaviour.actions import ActionMixin  # noqa: E402

SAMPLES = 140
LEGS = 8
NUMBERS = ("vel_x", "vel_y", "current_speed", "inertia_vx", "inertia_vy")
TUNED = {
    "camouflage_idle_delay": 0.5,
    "camouflage_hide_fade_time": 1.0,
    "camouflage_reveal_speed": "0.5",
    "camouflage_fade_speed": "fast",
    "camouflage_motion_visible_time": 9.0,
}
# name: (personality file, overrides, size, strength, visible timer)
SPIDERS = {
    "hider": ("camouflage", {}, 30.0, 0.0, 1.0),
    "plain": ("curious", {}, 24.0, 0.5, 0.0),
    "tuned": ("camouflage", TUNED, 60.0, 0.0, 0.0),
}


def personality(name: str, overrides: dict) -> dict:
    data = json.loads((ROOT / "personalities" / f"{name}.json").read_text(encoding="utf-8"))
    data.update(overrides)
    return data


def spider(personality_data: dict, size: float, strength: float, visible: float) -> SimpleNamespace:
    c = SimpleNamespace(
        personality=personality_data,
        size=size,
        dragging=False,
        _desktop_fully_hidden=False,
        legs=[SimpleNamespace(stepping=False, pending_step=False, lift=0.0) for _ in range(LEGS)],
        _camouflage_strength=strength,
        _camouflage_color=(1, 2, 3),
        _camouflage_idle_timer=0.0,
        _camouflage_visible_timer=visible,
    )
    for key in NUMBERS:
        setattr(c, key, 0.0)
    c.register_camouflage_touch = types.MethodType(ActionMixin.register_camouflage_touch, c)
    return c


def inputs(name: str, i: int) -> dict:
    """What spider `name` does at sample `i`, beyond resting."""
    if name == "hider":
        if i == 80:
            return {"leg": [3, False, False, 0.02]}
        if 90 <= i < 93:
            return {"dragging": True}
        if 105 <= i < 110:
            return {"vel_x": 2.0}
        if 110 <= i < 115:
            return {"vel_y": -3.0, "hidden": True}
        if i == 120:
            return {"leg": [6, False, True, 0.0]}
        if i == 121:
            return {"current_speed": 1.1}
    elif name == "tuned":
        if i == 40:
            return {"inertia_vx": 50.0}
        if i == 70:
            return {"leg": [0, True, False, 0.0]}
        if i == 71:
            return {"leg": [1, False, False, 0.012]}
    return {}


def apply(c: SimpleNamespace, d: dict) -> None:
    c.dragging = d.get("dragging", False)
    c._desktop_fully_hidden = d.get("hidden", False)
    for key in NUMBERS:
        setattr(c, key, d.get(key, 0.0))
    for leg in c.legs:
        leg.stepping, leg.pending_step, leg.lift = False, False, 0.0
    if "leg" in d:
        index, stepping, pending, lift = d["leg"]
        leg = c.legs[index]
        leg.stepping, leg.pending_step, leg.lift = stepping, pending, lift


def main() -> None:
    out = {"samples": SAMPLES, "spiders": {}}
    live = {}
    for name, (file, overrides, size, strength, visible) in SPIDERS.items():
        live[name] = spider(personality(file, overrides), size, strength, visible)
        out["spiders"][name] = {"personality": file, "overrides": overrides, "size": size,
                                "strength": strength, "visible_timer": visible, "trace": []}
    window = SimpleNamespace(manager=SimpleNamespace(creatures=list(live.values())))
    for i in range(SAMPLES):
        step = {name: inputs(name, i) for name in live}
        for name, c in live.items():
            apply(c, step[name])
        OverlayWindow._refresh_camouflage_samples(window)
        for name, c in live.items():
            assert c._camouflage_color is None
            out["spiders"][name]["trace"].append({
                "in": step[name],
                "strength": c._camouflage_strength,
                "visible_timer": c._camouflage_visible_timer,
                "idle_timer": c._camouflage_idle_timer,
            })
    path = Path(__file__).with_name("camouflage.json")
    path.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
