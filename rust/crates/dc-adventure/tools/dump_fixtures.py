"""Dump Python oracle output for dc-adventure.

Run from anywhere:
    <venv>/python.exe -I rust/crates/dc-adventure/tools/dump_fixtures.py
Writes rust/crates/dc-adventure/tests/fixtures/*.json:

- stat_text.json: every skill tooltip and every armour tooltip at each level;
- campaign.json: enemy_loadout / roll_drop draws from seeded random.Random;
- encounters.json: outpost plans and wave traces per layout, profile and tier;
- custom_maps.json: clean_map / problems / map_info for hand-made inputs;
- controls.json: ControlSettings.from_dict(...).to_dict() for odd inputs;
- profiles.json: what load_profile makes of odd files (as save_profile writes it).
"""
from __future__ import annotations

import json
import os
import random
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
WORKTREE = HERE.parents[3]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ["DESKTOP_BUG_STATE_DIR"] = tempfile.mkdtemp(prefix="dc-adventure-fixtures-")

from desktop_bug.app import custom_maps as cm  # noqa: E402
from desktop_bug.app.adventure_profile import load_profile, save_profile  # noqa: E402
from desktop_bug.app.campaign import MAPS, enemy_loadout, roll_drop  # noqa: E402
from desktop_bug.app.controls import ControlSettings  # noqa: E402
from desktop_bug.app.encounters import PROFILES, EncounterDirector  # noqa: E402
from desktop_bug.app.stat_text import item_tooltip, skill_tooltip  # noqa: E402
from desktop_bug.state.progression import ABILITY_TREE, ARMOR_CATALOG, MAX_ITEM_LEVEL  # noqa: E402
from desktop_bug.world.playfield import ScreenRect  # noqa: E402
from desktop_bug.world.screen_layout import ScreenLayout  # noqa: E402

OUT = HERE.parent / "tests" / "fixtures"


def stat_text():
    return {
        "skills": [{"id": n.id, "html": skill_tooltip(n, None)} for n in ABILITY_TREE],
        "items": [{"id": i.id, "level": lv, "html": item_tooltip(i, None, lv)}
                  for i in ARMOR_CATALOG for lv in range(1, MAX_ITEM_LEVEL + 1)],
    }


def campaign():
    cases = []
    for seed in range(12):
        rng = random.Random(seed)
        draws = []
        for info in MAPS:
            for role in ("guard", "hunter", "guardian"):
                worn, level = enemy_loadout(info, role, rng)
                draws.append({"map": info.id, "role": role, "worn": worn, "level": level,
                              "drop": roll_drop(worn, role == "guardian", rng),
                              "after": rng.random()})
        cases.append({"seed": seed, "draws": draws})
    return cases


LAYOUTS = [
    [[0, 0, 1600, 1000]],
    [[0, 0, 1600, 1000], [1600, 0, 1600, 1000]],
    [[0, 718, 2560, 1440], [2560, 0, 3840, 2160]],
    [[0, 0, 1600, 1000], [1900, 200, 1280, 800], [4000, 0, 1200, 900]],
    [[0, 0, 1920, 1080], [0, 1080, 1920, 1080], [1920, 500, 1280, 1024], [-2000, 0, 2000, 1200]],
]


def encounters():
    cases = []
    for rects in LAYOUTS:
        for primary in range(len(rects)):
            layout = ScreenLayout([ScreenRect(*map(float, r)) for r in rects], primary)
            for kind in PROFILES:
                for tier in (1, 2, 3, 4, 6):
                    for avoid in ([], [list(layout.primary.anchor(0.55, 0.45, 110.0))]):
                        director = EncounterDirector(PROFILES[kind], layout, tier, random.Random(1))
                        plans = director.outposts([tuple(a) for a in avoid])
                        sites = [SimpleNamespace(owned=False, reserves=2 + i, warning=0.0) for i in range(len(plans))]
                        trace = []
                        for step in range(900):
                            if step == 500 and sites:
                                sites[0].owned = True
                            got = director.update(0.1, sites)
                            for index, _ in got:
                                if step % 7 == 0:
                                    director.retry(index)
                                else:
                                    sites[index].reserves -= 1
                            if got:
                                trace.append([step, [[i, r] for i, r in got], [s.warning for s in sites]])
                        cases.append({
                            "rects": rects, "primary": primary, "kind": kind, "tier": tier, "avoid": avoid,
                            "plans": [{"screen": p.screen, "x": p.x, "y": p.y, "kind": p.kind, "name": p.name,
                                       "defenders": list(p.defenders), "reserves": p.reserves} for p in plans],
                            "trace": trace,
                        })
    return cases


MAP_INPUTS = [
    {},
    [],
    "nonsense",
    {"title": "  Spider   fort ", "tier": "3", "cap": 99.9, "wave_every": "nan", "tactics": 7,
     "enemy_tiers": "rare", "reward_companion": "hunter", "blurb": "x" * 300},
    {"title": "", "tier": 99, "cap": -3,
     "buildings": [{"kind": "castle"}, {"kind": "food", "fx": 5, "guards": [
         {"role": "wizard", "kind": "dragon", "armor": ["nope", "frost_aegis", "frost_aegis"]}]}]},
    {"buildings": [{"kind": "home", "owned": 0, "far": 1}, {"kind": "hatchery"}, {"kind": "hatchery", "name": "  "},
                   {"kind": "nest", "guards": "ab"}, {"kind": "silk", "supply": "9999", "reserves": "5"},
                   "junk", {"kind": "venom", "guards": [{"role": "spitter", "kind": "pale_cave", "level_bonus": 99,
                                                          "hp": 0.01, "item_level": "4", "armor": "auto"}] * 10}],
     "enemies": [{"role": "hunter", "fx": -1, "fy": "0.25", "far": "yes"}] * 25 + ["x"],
     "boss": {"name": "", "armor": ["sun_crown", "brood_crown", "brood_plates"], "kind": "thorn_crown"}},
    {"title": 12, "id": "custom-x", "enemy_tiers": ["epic", "legendary", "bogus"], "reward_companion": "dragon",
     "boss": {"name": "N" * 80, "armor": [], "hp": "2"}},
    {"boss": "boss", "buildings": [{"kind": "home"}, {"kind": "nest"}]},
]


def custom_maps():
    cases = []
    for raw in MAP_INPUTS:
        data = cm.clean_map(raw)
        info = cm.map_info(data)
        cases.append({"raw": raw, "clean": data, "problems": cm.problems(data),
                      "info": {"title": info.title, "blurb": info.blurb, "guardian": info.guardian,
                               "guardian_set": info.guardian_set, "tier": info.tier,
                               "enemy_tiers": list(info.enemy_tiers), "reward_companion": info.reward_companion}})
    new = cm.new_map("Fresh")
    cases.append({"raw": None, "clean": new, "problems": cm.problems(new), "info": None})
    return cases


CONTROL_INPUTS = [
    None, {}, [], {"bindings": "W"},
    {"bindings": {"jump": "W", "shoot": "???"}, "aim_cone": "x"},
    {"bindings": {"move_up": "Up", "move_down": "Up", "bite": "", "sprint": 5, "release": "F1"}, "aim_cone": 9.9,
     "face_mouse_when_still": "yes", "movement": "screen"},
    {"bindings": {"shoot": "Mouse Left", "bite": "Mouse Right"}},
    {"bindings": {"shoot": "Mouse Left", "bite": "Mouse Right", "jump": "F"}},
    {"version": 1, "bindings": {"shoot": "Mouse Left", "bite": "Mouse Right"}, "aim_cone": 720, "movement": 3},
    {"bindings": {a: f"F{i + 1}" for i, a in enumerate(("move_up", "move_down", "move_left", "move_right",
                                                       "sprint", "jump", "shoot", "bite"))}},
    {"bindings": {"skills": "Shift+W", "pause": "space", "map_view": "~"}, "aim_cone": True},
]


def controls():
    return [{"raw": raw, "out": ControlSettings.from_dict(raw).to_dict()} for raw in CONTROL_INPUTS]


PROFILE_INPUTS = [
    {},
    {"progression": {"level": 3, "xp": "7", "inventory": ["silk_carapace", "warden_crest", 4],
                     "equipped": {"head": "warden_crest", "legs": "leg_guard_set"},
                     "item_levels": {"warden_crest": 3, "silk_carapace": 1, "bogus": 2}}},
    {"version": 2, "name": "  Big \n spider  with a very very long name ", "progression": {"level": 2},
     "missions": {"territory": {"victories": 2, "defeats": "x", "best_seconds": 81.25}, "ember": 3}},
    {"name": 7, "selected_map": "custom-mine", "all_screens": 0, "disabled_screens": ["\\\\.\\DISPLAY2", 3],
     "admin": "yes", "two_player": 1, "player_two": {"name": "", "progression": {"level": 4, "team_id": "Pack A"}},
     "armoury": {"owned": ["sun_crown", "sun_crown", "nope", "warden_crest"],
                 "levels": {"sun_crown": 9, "nope": 3, "warden_crest": "x"},
                 "spares": {"sun_crown": "2", "x": 1, "sun_aegis": 0, "frost_aegis": 3}, "amber": "41"},
     "companions": {"hunter": {"name": "  Hunty ", "progression": {"level": 5}}, "dragon": {},
                    "scout": {"name": ""}}},
    {"selected_map": "atlantis", "armoury": "none", "progression": {"inventory": "silk"}},
    {"armoury": {"owned": "ab"}, "companions": [], "missions": [], "player_two": "x"},
]


def profiles():
    cases = []
    folder = Path(tempfile.mkdtemp(prefix="dc-profiles-"))
    for n, raw in enumerate(PROFILE_INPUTS):
        src = folder / f"in-{n}.json"
        src.write_text(json.dumps(raw), encoding="utf-8")
        out = folder / f"out-{n}.json"
        save_profile(load_profile(src), out)
        cases.append({"raw": raw, "saved": json.loads(out.read_text(encoding="utf-8"))})
    return cases



def missions():
    """Mission-level oracles, with real Python creatures but stationary capture traces."""
    from unittest.mock import patch
    from PyQt5.QtWidgets import QApplication
    from desktop_bug.app.adventure_profile import fresh_profile, profile_path
    from desktop_bug.app.mission_factory import create_mission
    from desktop_bug.app.mission import TerritoryMission
    from desktop_bug.manager import CreatureManager

    app = QApplication.instance() or QApplication([])
    original_random = random.Random
    rects = [ScreenRect(0, 0, 1600, 1000), ScreenRect(1600, 0, 1600, 1000)]

    def make(map_id, seed, count=1, two=False):
        profile = fresh_profile()
        profile.update(selected_map=map_id, admin=True, two_player=two)
        save_profile(profile)
        manager = CreatureManager(WORKTREE / "presets" / "colony.json", 1600 * count, 1000, seed=4)
        with patch.object(random, "Random", side_effect=lambda *args: original_random(*args) if args else original_random(seed)):
            return create_mission(manager, ControlSettings(), rects[:count])

    def snapshot(m):
        return dict(
            sites=[dict(kind=s.kind, name=s.name, x=s.x, y=s.y, owned=s.owned,
                        reserves=s.reserves, supply=s.supply, screen=s.screen) for s in m.sites],
            spiders=[dict(serial=c.index, x=c.x, y=c.y, name=c.display_name,
                          kind=c.enemy_kind, worn=c.mission_loot,
                          progression=c.progression.to_dict()) for c in m.manager.creatures],
            cap=m.CAP, wave_every=m.WAVE_EVERY,
            actors=[dict(role=a.role, style=a.style, home=a.home, silk=a.silk,
                         capacity=a.silk_capacity, from_outpost=a.from_outpost,
                         aggro_range=a.aggro_range) for a in m.actors],
            flies=[dict(x=f.x, y=f.y, heading=f.heading, length=f.length, size=f.size,
                        cruise=f.cruise, panic=f.panic) for f in m.manager.fly_world.flies],
        )

    openings = []
    for seed in (0, 6, 41):
        for info in MAPS:
            for count in (1, 2):
                m = make(info.id, seed, count, two=(seed == 41))
                openings.append(dict(map=info.id, seed=seed, screens=count, two=seed == 41,
                                     snapshot=snapshot(m), next=m.rng.random()))

    m = make("territory", 6)
    for c in list(m.manager.creatures):
        if c.progression.team_id == "rivals":
            c.dead = True
    m.manager._bury_the_dead()
    m.actors = [a for a in m.actors if not a.creature.dead]
    for c in m.manager.creatures:
        c.update = lambda *args: None
    food = m._site("food")
    m.hero.x, m.hero.y = food.x, food.y
    capture = []
    for tick in range(1, 101):
        m.update(0.05)
        capture.append(dict(tick=tick, progress=food.progress, owned=food.owned,
                            warning=m.nest.warning, pending=list(m.pending),
                            counter=m.counter_started, elapsed=m.elapsed,
                            xp=m.hero.progression.total_xp))

    m = make("territory", 6)
    m.wave_clock = 0
    waves = []
    for tick in range(1, 1501):
        m._spawning(0.05)
        if tick == 1 or tick % 10 == 0:
            waves.append(dict(tick=tick, warning=m.hatchery.warning, reserves=m.hatchery.reserves,
                              pending=m.pending.copy(), count=len(m.manager.creatures), clock=m.wave_clock))

    loot = []
    for seed in range(12):
        m = make("queen", seed)
        m.rng = original_random(seed)
        m.guardian = m._spawn("guardian", (800, 500))
        m.guardian.dead = True
        m._drop_loot()
        loot.append(dict(seed=seed, loot=[vars(l) for l in m.loot], next=m.rng.random()))
    # Live raids: the real creature frames, controllers, combat and AI, with
    # the hero left idle, for 30 seconds on one map of every kind.
    battles = []
    for map_id, seed in (("territory", 6), ("ember", 3), ("queen", 2), ("swarm", 5), ("reclaim", 7)):
        m = make(map_id, seed)
        trace = []
        for tick in range(1, 601):
            m.update(0.05)
            if tick % 10 == 0:
                trace.append(dict(
                    tick=tick, state=m.state, elapsed=m.elapsed, pending=list(m.pending),
                    spiders=[dict(serial=c.index, x=c.x, y=c.y, heading=c.heading, hp=c.hp,
                                  energy=c.energy, airborne=c.airborne) for c in m.manager.creatures],
                    sites=[dict(progress=s.progress, owned=s.owned, warning=s.warning,
                                reserves=s.reserves) for s in m.sites],
                    flies=[dict(x=f.x, y=f.y) for f in m.manager.fly_world.flies],
                    carcasses=len(m.manager.carcasses)))
        battles.append(dict(map=map_id, seed=seed, trace=trace, next=m.rng.random()))
    return dict(openings=openings, capture=capture, waves=waves, loot=loot, battles=battles)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, make in (("stat_text.json", stat_text), ("campaign.json", campaign), ("encounters.json", encounters),
                       ("custom_maps.json", custom_maps), ("controls.json", controls), ("profiles.json", profiles), ("missions.json", missions)):
        data = make()
        (OUT / name).write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")
        print(name, len(data))


if __name__ == "__main__":
    main()

