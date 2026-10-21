"""Qt reference renders and word-finder fixtures for the dc-art port.

    <venv>/python.exe -I rust/crates/dc-art/tools/qt_reference.py [out_dir]

Renders, through the Python app and Qt, every picture the Rust `dc-art`
crate ports, at the fixed inputs `examples/compare_qt.rs` uses too, into
``out_dir`` (default ``rust/crates/dc-art/target-review/qt``):

- ``backdrop_<look>.png``      mission_backdrop.paint_backdrop(look, 640, 360, sites, 0)
- ``map_<id>[_locked].png``    map_art.map_picture(id, kind, locked)
- ``skill_<id>_<look>.png``, ``crest_<branch>.png``, ``stat_<kind>.png``
- ``building_<kind>[_owned].png``  mission_art.building_art
- ``hud_*.png``                adventure_ui.draw_hud / mission_ui overlays / the
                               engine's released banner, on 1600 x 1000
- ``reclaim_mid.png`` / ``reclaim_after.png``  a seeded synthetic Reclaim desktop
- ``menu_<name>.png``, ``card_frame.png``      tools/generate_ui_art

and writes the word finder's boxes on synthetic desktops to
``rust/crates/dc-art/tests/fixtures/word_boxes.json`` (small; committed).
Nothing in the Python tree is changed.
"""
from __future__ import annotations

import json
import math
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[2]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "tools"))
# The native platform, as the app runs: under "offscreen" Qt finds no fonts
# on Windows and every text renders as empty boxes. Override with
# QT_QPA_PLATFORM=offscreen on a machine without a display.
os.environ.setdefault("QT_QPA_PLATFORM", "windows" if sys.platform.startswith("win") else "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="dc-art-ref-"))
os.environ.setdefault("DESKTOP_BUG_GL", "0")

from PyQt5.QtCore import QRectF, Qt  # noqa: E402
from PyQt5.QtGui import QColor, QFont, QImage, QPainter  # noqa: E402
from PyQt5.QtWidgets import QApplication  # noqa: E402

APP = QApplication.instance() or QApplication(sys.argv[:1])

from desktop_bug.app import mission_ui  # noqa: E402
from desktop_bug.app.adventure_ui import draw_hud  # noqa: E402
from desktop_bug.app.desktop_capture import synthetic_snapshot  # noqa: E402
from desktop_bug.app.desktop_surface import DesktopSurface  # noqa: E402
from desktop_bug.app.map_art import map_picture  # noqa: E402
from desktop_bug.app.mission_art import building_art  # noqa: E402
from desktop_bug.app.mission_backdrop import paint_backdrop  # noqa: E402
from desktop_bug.app.screen_text import find_text_boxes  # noqa: E402
from desktop_bug.app.skill_art import branch_crest, skill_icon, stat_icon  # noqa: E402
from desktop_bug.world.playfield import ScreenRect  # noqa: E402

LOOKS = ["territory", "swarm", "ember", "reclaim", "storm", "obsidian", "queen"]
MAPS = [("territory", "raid"), ("swarm", "swarm"), ("ember", "raid"), ("reclaim", "reclaim"),
        ("storm", "raid"), ("obsidian", "raid"), ("queen", "raid"), ("burrow", "raid"),
        ("custom-x", "swarm")]
SKILLS = ["vitality", "carapace_harden", "power_strike", "apex_predator", "quick_step",
          "long_stride", "silk_sense", "web_crafter", "silk_tracking"]
BRANCHES = ["body", "fangs", "legs", "silk"]
STATS = ["armor", "damage", "points", "worn", "speed", "silk"]
BUILDINGS = ["home", "food", "silk", "hatchery", "nest", "outpost", "infestation", "venom",
             "lookout", "amber", "nursery", "flynest"]
W, H = 1600, 1000


def save(image: QImage, out: Path, name: str) -> None:
    image.save(str(out / f"{name}.png"))


def canvas() -> tuple[QImage, QPainter]:
    image = QImage(W, H, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing, True)
    p.setFont(QFont("Segoe UI", 9))
    return image, p


# -- fakes the overlays read ----------------------------------------------------------

class Controls:
    def __init__(self, table):
        self.table = table

    def binding(self, action):
        return self.table.get(action, "?")


class Window:
    """What draw_hud and the mission overlays read of the overlay window."""

    def __init__(self, players=0, player=None, mission=None):
        self.geometry_rect = None
        self._adventure_hud_position = None
        self.mission = mission if mission is not None else SimpleNamespace(players=[None] * players)
        self.player = player
        self.controls = Controls({"release": "F8"})

    def width(self):
        return W

    def height(self):
        return H


class Spider(SimpleNamespace):
    def _label_font(self):
        font = QFont("Segoe UI")
        font.setPointSizeF(10.0 + (self.level - 1) * 0.1)
        font.setWeight(QFont.DemiBold)
        return font

    def relation_to(self, other):
        return "foe" if getattr(other, "foe", False) else "friend"


def controller(dead=False, name="Webby", level=3):
    spider = Spider(display_name=name, level=level, dead=dead, hp=46.0, max_hp=100.0, energy=80.0,
                    max_energy=100.0, attack_cooldown=0.4)
    return SimpleNamespace(creature=spider, silk=5.0, silk_capacity=12.0, jump_cooldown=0.0,
                           web_cooldown=1.25, JUMP_ENERGY=10.0, WEB_ENERGY=5.0,
                           feedback="Web ready", feedback_time=1.0,
                           controls=Controls({"jump": "space", "shoot": "Mouse Left", "bite": "Mouse Right",
                                              "skills": "k", "companion_follow": "1",
                                              "companion_defend": "2", "companion_attack": "3"}))


class Link(SimpleNamespace):
    def point_on(self, screen):
        return self.a_point if screen == self.a else self.b_point

    def other(self, screen):
        return self.b if screen == self.a else self.a


def layout():
    screens = [SimpleNamespace(index=0, name="Main", rect=ScreenRect(0, 0, 800, 1000)),
               SimpleNamespace(index=1, name="Right", rect=ScreenRect(800, 0, 800, 1000))]
    links = [Link(a=0, b=1, kind="door", a_point=(780.0, 500.0), b_point=(820.0, 500.0)),
             Link(a=0, b=1, kind="tunnel", a_point=(300.0, 820.0), b_point=(1300.0, 240.0))]
    return SimpleNamespace(screens=screens, links=links, multi=True, count=2,
                           tunnels=tuple(link for link in links if link.kind == "tunnel"),
                           screen_at=lambda x, y: screens[0] if x < 800 else screens[1])


def mission(**extra):
    hero = Spider(display_name="Webby", level=4, x=420.0, y=560.0, dead=False)
    foe = SimpleNamespace(x=1200.0, y=400.0, dead=False, foe=True)
    friend = SimpleNamespace(x=460.0, y=600.0, dead=False)
    sites = [
        SimpleNamespace(x=300.0, y=420.0, kind="home", name="Home", owned=True, contested=False, warning=0.0,
                        supply=0.0, reserves=0, progress=0.0),
        SimpleNamespace(x=1150.0, y=380.0, kind="hatchery", name="Hatchery", owned=False, contested=False,
                        warning=0.0, supply=0.0, reserves=3, progress=0.4),
        SimpleNamespace(x=1180.0, y=760.0, kind="outpost", name="Outpost", owned=False, contested=False,
                        warning=2.25, supply=0.0, reserves=2, progress=0.0),
    ]
    base = dict(
        area=SimpleNamespace(x=0.0, y=0.0, w=1600.0, h=1000.0), layout=layout(), sites=sites,
        actors=[SimpleNamespace(windup=0.3, creature=SimpleNamespace(x=900.0, y=600.0, size=20.0))],
        loot=[], hazards=[SimpleNamespace(position=(700.0, 640.0, 30.0))],
        splashes=[SimpleNamespace(x=760.0, y=700.0, age=0.3)],
        threads=[SimpleNamespace(x0=600.0, y0=300.0, spider=SimpleNamespace(x=650.0, y=360.0), age=0.2)],
        objective="Seal the hatchery and take the outpost before the swarm grows",
        notice="", notice_time=0.0, state="active", save_error="", ended=False, end_clock=0.0,
        END_SCREEN_SECONDS=6.0, command="defend",
        allies=[SimpleNamespace(display_name="Silky", hp=30.0, max_hp=60.0, dead=False)],
        hero=hero, manager=SimpleNamespace(creatures=[hero, foe, friend]), players=[None],
        amber_mined=0, found=[], caught=0, reward_text="", words_eaten=12,
    )
    base.update(extra)
    return SimpleNamespace(**base)


def hud_scenes(out: Path) -> None:
    image, p = canvas()
    draw_hud(p, Window(), controller())
    p.end()
    save(image, out, "hud_player")

    image, p = canvas()
    win = Window(players=2)
    draw_hud(p, win, controller(dead=True, name="Second"), player_index=1)
    p.end()
    save(image, out, "hud_player_coop")

    image, p = canvas()
    m = mission(surface=SimpleNamespace(desktop_hp=0.42, hit_flash=0.3, intact_panes=lambda: [1]),
                intro_time=0.5)
    win = Window(player=controller(), mission=m)
    mission_ui.draw_buildings(p, m)
    mission_ui.draw_mission_hud(p, win, m)
    p.end()
    save(image, out, "hud_mission")

    image, p = canvas()
    m = mission(state="crashed", blue_screen=2.0, CRASH_SCREEN_SECONDS=7.0, ended=True)
    mission_ui.draw_blue_screen(p, m)
    p.end()
    save(image, out, "hud_bluescreen")

    image, p = canvas()
    m = mission(state="victory", ended=True, end_clock=1.0, found=["a", "b"], amber_mined=5)
    mission_ui.draw_end_title(p, m)
    p.end()
    save(image, out, "hud_end")

    from desktop_bug.app.engine import OverlayWindow

    image, p = canvas()
    OverlayWindow._draw_released_banner(Window(mission=mission()), p)
    p.end()
    save(image, out, "hud_released")


WINDOW = QRectF(200, 150, 700, 500)
BEHIND = QRectF(500, 300, 700, 500)


def reclaim_scene(out: Path) -> dict:
    snap = synthetic_snapshot([ScreenRect(0, 0, W, H)], [WINDOW, BEHIND], text_rows=8)
    s = DesktopSurface(snap, seed=2, find_text=True)
    words = [[w.rect.x(), w.rect.y(), w.rect.width(), w.rect.height(),
              w.colour.red(), w.colour.green(), w.colour.blue()] for w in s.words]
    s.melt(1300, 800, 34)
    s.melt(1000, 500, 26)
    s.crack(1400, 250, 0.7)
    for i, amount in ((0, 1.0), (3, 0.5), (5, 0.5)):
        s.eat(s.words[i], amount)
    while s.crack(400, 400, 1.0) != "window":
        pass
    for _ in range(15):
        s.update(0.02)
    for name in ("reclaim_mid", "reclaim_after"):
        image = QImage(W, H, QImage.Format_ARGB32_Premultiplied)
        image.fill(Qt.transparent)
        p = QPainter(image)
        s.paint(p)
        p.end()
        save(image, out, name)
        for _ in range(20):
            s.update(0.02)
    return {"words": words, "hp": s.desktop_hp, "integrity": s.integrity}


def word_cases() -> list:
    cases = [
        {"screen": [0, 0, 1600, 1000], "windows": [[200, 150, 700, 500]], "rows": 8, "crop": None},
        {"screen": [1600, 200, 1280, 800], "windows": [[1700, 260, 600, 400], [2100, 500, 700, 450]],
         "rows": 14, "crop": None},
        {"screen": [0, 0, 1600, 1000], "windows": [[200, 150, 700, 500]], "rows": 8,
         "crop": [180, 140, 400, 300]},
        {"screen": [0, 0, 1600, 1000], "windows": [[40, 40, 1500, 300], [300, 420, 913, 480]], "rows": 16,
         "crop": None},
    ]
    for case in cases:
        sx, sy, sw, sh = case["screen"]
        snap = synthetic_snapshot([ScreenRect(sx, sy, sw, sh)], [QRectF(*w) for w in case["windows"]],
                                  text_rows=case["rows"])
        image = snap.screens[0].image
        offset = (float(sx), float(sy))
        if case["crop"]:
            cx, cy, cw, ch = case["crop"]
            image = image.copy(cx, cy, cw, ch)
            offset = (float(sx + cx), float(sy + cy))
        boxes = find_text_boxes(image, offset)
        case["boxes"] = [[b.x(), b.y(), b.width(), b.height()] for b in boxes]
    return cases


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else CRATE / "target-review" / "qt"
    out.mkdir(parents=True, exist_ok=True)
    sites = [(120, 200, "home"), (480, 160, "nest")]
    for look in LOOKS:
        save(paint_backdrop(look, 640, 360, sites, 0), out, f"backdrop_{look}")
    for map_id, kind in MAPS:
        for locked in (False, True):
            name = f"map_{map_id}" + ("_locked" if locked else "")
            save(map_picture(map_id, kind, locked).toImage(), out, name)
    for skill in SKILLS:
        for look in ("learned", "available", "locked"):
            save(skill_icon(skill, look, 46).toImage(), out, f"skill_{skill}_{look}")
    for branch in BRANCHES:
        save(branch_crest(branch, 34).toImage(), out, f"crest_{branch}")
    for kind in STATS:
        save(stat_icon(kind, 30).toImage(), out, f"stat_{kind}")
    for kind in BUILDINGS:
        for owned in (False, True):
            save(building_art(kind, owned), out, f"building_{kind}" + ("_owned" if owned else ""))
    hud_scenes(out)
    reclaim = reclaim_scene(out)

    import generate_ui_art as ui

    ui.OUT = out
    ui.card_frame()
    for name in ("companion", "adventure", "strategy", "inspect", "pin", "name", "rename", "skills", "preset",
                 "creatures", "teams", "behavior", "flies", "launch", "base", "move", "remove", "cage",
                 "armor", "labels", "health", "stamina", "xp", "appearance", "performance", "randomize",
                 "interaction", "help", "quit"):
        ui.menu_emblem(name)

    fixture = {"cases": word_cases(), "reclaim": reclaim}
    path = CRATE / "tests" / "fixtures" / "word_boxes.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(fixture, separators=(",", ":")) + "\n", encoding="utf-8")
    print("wrote", out, "and", path)


if __name__ == "__main__":
    main()
