"""Unmodified Python manager oracle; all state lives in TemporaryDirectory.

Run with the main checkout's python.exe -I. Isolated Python ignores PYTHONPATH,
so insert the worktree src explicitly (also set PYTHONPATH for subprocesses).
No reference methods or random streams are monkeypatched, except that the
painted frames draw the squared-up front legs as Rust does (see
`rust_combat_leg_pose`); the simulation never calls that method.
"""
from pathlib import Path
import json
import math
import os
import random
import sys
import tempfile

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[2]
os.environ["PYTHONPATH"] = str(ROOT / "src")
sys.path.insert(0, str(ROOT / "src"))
os.environ["QT_QPA_PLATFORM"] = "offscreen"


def label_metrics(c):
    """The oracle's font metrics for this spider's label, as bounding_rect
    measures them (Qt's offscreen fonts differ from a desktop's). The Rust
    test feeds these back so it compares the simulation, not font shaping."""
    from PyQt5.QtGui import QFontMetrics
    font = c._label_font()
    fm = QFontMetrics(font)
    text = c._label_text()
    return [text, font.pointSizeF(), fm.horizontalAdvance(text), fm.height()]


def trace(m):
    return {
        "creatures": [{"id": c.progression_id, "name": c.name, "x": c.x, "y": c.y,
            "heading": c.heading, "state": c.state, "hp": c.hp, "xp": c.progression.total_xp,
            "foe": c._foe.progression_id if c._foe else None,
            "prey": m.fly_world.flies.index(c._prey) if c._prey in m.fly_world.flies else None,
            "cage": m.cages.index(c.cage) if c.cage in m.cages else None,
            "feed_cooldown": c._feed_cooldown, "prey_recheck": c._prey_recheck,
            "flee_timer": c.flee_timer, "attack_cooldown": c.attack_cooldown,
            "hovered": c._hovered,
            "hidden": bool(getattr(c, "_desktop_fully_hidden", False)),
            "hide_kind": getattr(c, "_desktop_hide_kind", None),
            "label": label_metrics(c)} for c in m.creatures],
        "cages": [c.to_dict() for c in m.cages],
        "bases": m.base_world.to_dict(),
        "flies": [{"x": f.x, "y": f.y, "state": f.state} for f in m.fly_world.flies],
        "carcasses": [{"x": c.x, "y": c.y, "t": c.t} for c in m.carcasses],
    }


# Scenarios whose last frame is also painted. None pins a label, so with the
# label switches off no text is drawn except damage numbers.
RENDERED = ("colony", "fight", "hunt", "stalk", "duel")


def rust_combat_leg_pose(original):
    """Rust changed the squared-up pose on purpose (dc-kinematics
    PORT_STATUS.md): the front legs reach towards the foe instead of being
    lifted up the screen, and the pose eases while the spider walks. The bite
    and everything else stay Python's."""
    def pose(self, leg, ax, ay, foot_x, foot_y, front):
        stance = getattr(self, "combat_stance", 0.0)
        _, lift = self.strike_phase()
        if lift > 0.01 or stance <= 0.01 or front <= 0.15:
            return original(self, leg, ax, ay, foot_x, foot_y, front)
        front_weight = min(max((front - 0.15) / 0.85, 0.0), 1.0)
        walking = min(max(self.current_speed / 120.0, 0.0), 1.0)
        weight = stance * front_weight * (1.0 - 0.65 * walking)
        face_x, face_y = self.combat_face_x, self.combat_face_y
        if math.hypot(face_x, face_y) < 1e-6:
            face_x, face_y = math.cos(self.heading), math.sin(self.heading)
        lateral_x, lateral_y = -face_y, face_x
        side = self._side_sign(leg.definition.get("side", "right"))
        foot_x += face_x * self.size * 0.46 * weight
        foot_y += face_y * self.size * 0.46 * weight
        foot_x += lateral_x * side * self.size * 0.34 * weight
        foot_y += lateral_y * side * self.size * 0.34 * weight
        return foot_x, foot_y
    return pose


def render_frame(m, path):
    """The manager's own `render(painter)` on a transparent 1280x720 image,
    as the overlay paints it, after the run (so it changes nothing above)."""
    from PyQt5.QtCore import Qt
    from PyQt5.QtGui import QImage, QPainter
    from desktop_bug.creature.render_procedural import RenderProceduralMixin
    original = RenderProceduralMixin._combat_leg_pose
    RenderProceduralMixin._combat_leg_pose = rust_combat_leg_pose(original)
    for name in ("names", "levels", "health", "xp", "stamina"):
        getattr(m, f"set_always_show_{name}")(False)
    # Offscreen Qt has no fonts and draws text as empty boxes, so the floating
    # damage numbers are cleared on both sides before painting.
    for thing in list(m.creatures) + list(m.carcasses):
        thing.damage_numbers.clear()
    image = QImage(1280, 720, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    try:
        m.render(painter)
    finally:
        painter.end()
        RenderProceduralMixin._combat_leg_pose = original
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(str(path))


def main():
    with tempfile.TemporaryDirectory(prefix="dc-manager-oracle-") as scratch:
        os.environ["DESKTOP_BUG_STATE_DIR"] = str(Path(scratch) / "initial")
        from PyQt5.QtWidgets import QApplication
        from desktop_bug.manager import CreatureManager
        app = QApplication.instance() or QApplication([])
        out = CRATE / "tests" / "fixtures"
        out.mkdir(parents=True, exist_ok=True)
        # scenario -> (frames simulated, record every n-th frame)
        plan = {"colony": (180, 1), "fight": (180, 1), "hunt": (180, 1), "cages": (180, 1),
                "stalk": (600, 2), "duel": (3600, 10), "surfaces": (3600, 10)}
        surfaces = [
            {"x": 100.0, "y": 100.0, "w": 300.0, "h": 220.0, "kind": "folder", "class_name": "CabinetWClass"},
            {"x": 800.0, "y": 380.0, "w": 320.0, "h": 240.0, "kind": "folder", "class_name": "CabinetWClass"},
            {"x": 450.0, "y": 120.0, "w": 300.0, "h": 200.0, "kind": "window", "class_name": "Notepad"},
        ]
        for scenario, (total, every) in plan.items():
            for seed in (3, 7, 11):
                random.seed(seed)  # reproducible oracle's legacy module RNG
                os.environ["DESKTOP_BUG_STATE_DIR"] = str(Path(scratch) / f"{scenario}-{seed}")
                fighting = scenario in ("fight", "duel")
                settings = {"flies": {"enabled": scenario in ("hunt", "stalk"), "spawner": False},
                            "conflict": fighting, "social_play": True}
                if fighting:
                    settings["team_relations"] = {"a": {"b": "foe"}}
                kinds = ("explorer", "explorer", "explorer") if scenario == "surfaces" else ("bold", "cautious", "social")
                preset = {"name": "fixture", "slots": [
                    {"model": "tarantula", "personality": kinds[0], "count": 1, "slot_id": "a", "team": "a"},
                    {"model": "tarantula", "personality": kinds[1], "count": 1, "slot_id": "b", "team": "b"},
                    {"model": "tarantula", "personality": kinds[2], "count": 1, "slot_id": "c", "team": "a"},
                ], "settings": settings}
                path = Path(scratch) / "fixture.json"
                path.write_text(json.dumps(preset), encoding="utf-8")
                m = CreatureManager(path, 1280, 720, seed=seed)
                m.mouse_web_world.can_control = False
                models = {k: v for k, v in m.models.items() if k == "tarantula"}
                personalities = {k: v for k, v in m.personalities.items() if k in kinds}
                # Portable content dicts; sprite folder paths do not affect simulation.
                for values in (models, personalities):
                    for v in values.values():
                        v.pop("_path", None)
                        v.pop("_folder", None)
                actions = []
                if scenario in ("fight", "duel", "hunt", "stalk", "cages", "surfaces"):
                    for i, c in enumerate(m.creatures):
                        x, y = (500.0 + i * 35.0, 350.0) if fighting else (300.0 + i * 250.0, 350.0)
                        c.x, c.y = x, y
                        c.target_x, c.target_y = x, y
                        c._initialize_legs()
                    actions.append({"kind": "positions", "positions": [[c.x, c.y] for c in m.creatures]})
                if scenario == "hunt":
                    f = m.fly_world.spawn(force=True)
                    f.x, f.y = m.creatures[0].x, m.creatures[0].y
                    actions.append({"kind": "fly", "x": f.x, "y": f.y})
                if scenario == "stalk":
                    f = m.fly_world.spawn(force=True)
                    f.x, f.y = m.creatures[0].x + 220.0, m.creatures[0].y - 60.0
                    actions.append({"kind": "fly", "x": f.x, "y": f.y})
                if scenario == "surfaces":
                    from desktop_bug.world.desktop_environment import DesktopSurface
                    m.set_desktop_surfaces([DesktopSurface(**s) for s in surfaces])
                    actions.append({"kind": "surfaces", "surfaces": surfaces})
                if scenario == "cages":
                    m.add_cage(300.0, 350.0, 360.0, 260.0)
                    m.name_creature(m.creatures[0], "  Charlotte  ")
                    m.set_creature_level_pin(m.creatures[0], True)
                    actions += [{"kind": "cage"}, {"kind": "name"}]
                frames = [{"state": trace(m)}]
                for frame in range(total):
                    x, y, down, pressed, released = -5000.0, -5000.0, False, False, False
                    if scenario == "cages" and 5 <= frame <= 15:
                        x, y = 120.0 + (frame - 5) * 2.0, 350.0
                        down, pressed, released = frame < 15, frame == 5, frame == 15
                    m.update(1 / 60, x, y, down, pressed, released)
                    if scenario == "cages":
                        m.update_hover(x, y)
                    if (frame + 1) % every == 0:
                        frames.append({"input": [x, y, down, pressed, released], "steps": every, "state": trace(m)})
                # Probe a cloned manager stream without advancing it.
                rr = random.Random()
                rr.setstate(m._rng.getstate())
                rng_next = [rr.random() for _ in range(4)]
                # The file both apps must be able to read: creatures.json after a save.
                m.save_runtime_state()
                saved = json.loads(m._progression_state_path.read_text(encoding="utf-8"))
                payload = {"scenario": scenario, "seed": seed, "preset": preset,
                           "models": models, "personalities": personalities,
                           "actions": actions, "frames": frames,
                           "rng_next": rng_next, "saved": saved}
                (out / f"{scenario}-{seed}.json").write_text(json.dumps(payload, separators=(",", ":"), default=str) + "\n", encoding="utf-8")
                if scenario in RENDERED:
                    render_frame(m, out / "frames" / f"{scenario}-{seed}.png")
                print(f"{scenario}-{seed}: {len(frames)} frames", flush=True)
        del app

if __name__ == "__main__":
    main()
