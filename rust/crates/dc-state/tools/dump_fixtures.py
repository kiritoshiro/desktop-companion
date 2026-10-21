"""Dump Python oracle values for the dc-state tests.

    C:\\Users\\win\\Documents\\Workspace\\projects\\desktop-companion\\.venv\\Scripts\\python.exe -I \\
        rust/crates/dc-state/tools/dump_fixtures.py

Writes rust/crates/dc-state/tests/fixtures/state_oracle.json.
"""

from __future__ import annotations

import json
import random
import sys
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve()
WORKTREE = HERE.parents[4]
sys.path.insert(0, str(WORKTREE / "src"))

from desktop_bug.state import progression as pr  # noqa: E402
from desktop_bug.state import runtime_state as rs  # noqa: E402
from desktop_bug.state import teams  # noqa: E402

out: dict = {}

out["xp"] = {str(level): pr.xp_to_next_level(level) for level in range(-1, 33)}
out["growth"] = {str(level): list(pr.growth_multipliers(level)) for level in range(-1, 33)}
out["level_multiplier"] = {str(level): pr.level_multiplier(level) for level in range(-1, 8)}


def item_dict(item):
    return {k: getattr(item, k) for k in ("id", "name", "slot", "description", "armor", "max_hp", "max_energy",
                                           "damage", "speed", "size_min", "size_max", "set_id", "tier")}


out["items"] = {item.id: {str(level): item_dict(pr.item_at_level(item, level)) for level in (1, 2, 3, 5, 9)}
                for item in pr.ARMOR_CATALOG}
out["abilities"] = [{"id": n.id, "name": n.name, "level_required": n.level_required, "cost": n.cost,
                     "prerequisites": list(n.prerequisites), "effects": n.effects} for n in pr.ABILITY_TREE]
out["sets"] = {s.id: {"pieces": list(s.pieces), "effects": s.effects} for s in pr.ARMOR_SETS.values()}

progression_inputs = [
    None,
    {},
    {"level": 99, "xp": -5, "total_xp": 3, "skill_points": -1},
    {"level": "7", "xp": "12", "total_xp": "nope"},
    {"level": 3.9, "xp": 4.5, "skill_points": 2},
    {"unlocked_abilities": ["vitality", "fly", 3], "inventory": ["sun_crown", "nope", "frost_aegis"],
     "equipped": {"head": "sun_crown", "carapace": "silk_carapace", "legs": "frost_aegis"}},
    {"inventory": None, "equipped": {"carapace": "silk_carapace"}},
    {"team_id": "  Porch   Guard ", "relation_overrides": {"a": "FOE", "b": "enemy", "c": 3}},
    {"team_id": "", "pin_level": 1, "pin_health": "yes"},
    {"team_id": "x" * 40, "item_levels": {"sun_crown": 9, "frost_aegis": "2", "nope": 3, "brood_crown": "x"}},
]
out["progression"] = [{"input": value, "output": pr.ProgressionState.from_dict(value).to_dict()} for value in progression_inputs]

full_sets = []
for worn in (["forager_hood", "forager_shell", "forager_mantle", "forager_leggings", "forager_wraps"],
             ["sun_crown", "sun_aegis", "sun_scales", "sun_legs", "sun_gauntlets", "warden_crest"],
             ["warden_crest", "warden_carapace", "warden_tergites", "warden_greaves", "warden_bracers"]):
    state = pr.ProgressionState(inventory=list(worn))
    for item_id in worn:
        state.equip(item_id)
    state.item_levels = {worn[0]: 3}
    full_sets.append({
        "worn": worn,
        "equipped": dict(state.equipped),
        "bonus": pr.set_bonus_effects(state),
        "items": [item_dict(i) for i in pr.equipped_items(state)],
    })
out["set_bonus"] = full_sets

names = ["neutral", "", "Rivals", "pack_a", "porch_guard", "shed_crew", "Porch Guard", "attic_watch", "garden_patrol",
         "roof", "hearth", "wanderers", "night_shift", "x-ray_team", "émigrés", "a" * 40]
out["team_defaults"] = {
    name: {"id": pr.normalize_team_id(name), "color": list(teams.default_color(name)), "name": teams.default_name(name)}
    for name in names
}
taken_sets = [[], [(79, 163, 209)], [teams.default_color("roof"), (0, 0, 0)]]
out["distinct"] = [{"name": name, "taken": [list(t) for t in taken], "color": list(teams.distinct_color(name, taken))}
                   for name in names for taken in taken_sets]


def profiles_dict(profiles):
    return [[k, p.name, list(p.color)] for k, p in profiles.items()]


team_cases = [
    [{"porch_guard": {"name": "Porch guard", "color": "#3fa9d9"}, "shed": "Shed crew", "neutral": {"name": "Nope"}}, []],
    [None, ["pack_a", "neutral", "attic_watch"]],
    [{"Porch Guard": {"name": "Porch guard"}}, ["PORCH_guard", "porch guard"]],
    [None, ["attic_watch", "porch_guard", "shed_crew", "garden_patrol", "roof", "hearth", "wanderers", "night_shift"]],
    [None, ["night_shift", "wanderers", "hearth", "roof", "garden_patrol", "shed_crew", "porch_guard", "attic_watch"]],
    [{"a_team": {"color": "#3fa9d9"}, "b_team": {"color": "#3fa9d9"}}, []],
    [{"x": {"name": "  ", "color": [300, -4, 12.5]}, "y": {"name": "z" * 50, "color": "abc"}}, ["q"]],
]
out["normalize_teams"] = [{"raw": raw, "used": used, "profiles": profiles_dict(teams.normalize_teams(raw, used))}
                          for raw, used in team_cases]
out["teams_payload"] = teams.teams_payload(teams.normalize_teams(*team_cases[0]))
color_inputs = ["#3fa9d9", "3fa9d9", "#abc", [10, 20, 30], [1.5, 2.5, 254.6], "not a colour", None, [1, 2], " #ABCDEF ", 123456]
out["parse_color"] = [{"input": c, "output": list(teams.parse_color(c)) if teams.parse_color(c) else None} for c in color_inputs]

stance_inputs = [
    {"pack_a": {"rivals": "foe"}},
    {"pack_a": {"rivals": "neutral"}},
    {"Pack A": {"Rivals": "FRIEND", "hunters": "enemy"}, "b": [], "hunters": {"b": "foe"}},
    {"neutral": {"x": "foe"}, "x": {"x": "friend"}},
]
out["stances"] = []
for raw in stance_inputs:
    stances = pr.normalize_team_stances(raw)
    pairs = teams.stance_pairs(teams.normalize_teams(None, ["pack_a", "rivals", "hunters", "b", "x"]), stances)
    out["stances"].append({"raw": raw, "normalized": stances, "minimal": teams.minimal_stances(stances),
                           "pairs": [list(p) for p in pairs]})
stance_query = [("pack_a", "rivals"), ("pack_a", "pack_b"), ("pack_a", "Pack A"), ("neutral", "rivals"), ("rivals", "rivals")]
out["team_stance"] = [{"left": l, "right": r, "none": pr.team_stance(l, r), "declared": pr.team_stance(l, r, pr.normalize_team_stances(stance_inputs[1]))}
                      for l, r in stance_query]

rng = random.Random(1234)
out["new_ids"] = [rs.new_creature_id("Default", rng) for _ in range(5)]
out["namespaces"] = {v: rs.normalize_namespace(v) for v in ["Default", "  COLONY  ", "", "Straße"]}
out["keys"] = {k: [rs.normalize_state_key(k), rs.slot_id_from_key(k), rs.member_index_from_key(k)]
               for k in ["Default|slot-0:0", "a|b:c:12", "runtime:0", "", "default|", "x|y", "x|:3", "noprefix:4"]}

migration_input = {
    "Default|slot-0:0": {"name": "Webster", "progression": {"level": 5, "total_xp": 500}},
    "default|slot-0:0": {"progression": {"level": 2}},
    "runtime:0": {"progression": {"level": 9}},
    "colony|slot-1:3": {"level": 4, "total_xp": 400, "last_seen": 2.0, "born": True},
    "colony|abc": {"slot_id": None, "last_seen": 3, "born": "x"},
    "colony|def": "nonsense",
}
out["migration"] = {"input": migration_input, "output": rs.migrate_creatures(migration_input, 7)}
states = out["migration"]["output"]
out["slot_members"] = {
    "slot-0": rs.saved_slot_members(states, "DEFAULT", "slot-0"),
    "slot-1": rs.saved_slot_members(states, "colony", "slot-1"),
    "next_birth": rs.next_birth_ordinal(states),
}
out["evict"] = rs.evict({"a|1": {"last_seen": 100}, "a|2": {"last_seen": 50}, "a|3": {"last_seen": 49},
                         "a|4": {}, "a|5": {"last_seen": "x"}, "a|6": 3}, 100)
payload = rs.build_payload(states, [{"id": "b", "x": 1.5}], 7, {"cages": [{"x": 1e-05, "name": "ünï\n"}], "webs": None})
out["payload"] = payload
out["payload_text"] = json.dumps(payload, indent=2) + "\n"

target = HERE.parents[1] / "tests" / "fixtures" / "state_oracle.json"
target.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
print(f"wrote {target}")
