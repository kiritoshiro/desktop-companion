"""Dump Python oracle values for the dc-content tests.

Run from anywhere with the project venv:

    C:\\Users\\win\\Documents\\Workspace\\projects\\desktop-companion\\.venv\\Scripts\\python.exe -I \\
        rust/crates/dc-content/tools/dump_fixtures.py

Writes rust/crates/dc-content/tests/fixtures/content_oracle.json.
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

from desktop_bug.content import body_plans, enemy_kinds, enemy_traits, palettes  # noqa: E402
from desktop_bug.content import personality_profiles as pp  # noqa: E402
from desktop_bug.content import skills  # noqa: E402
from desktop_bug.content.discovery import (  # noqa: E402
    discover_models,
    discover_personalities,
    validate_model,
    validate_personality,
)
from desktop_bug.content.preset_io import safe_preset_filename, validate_preset  # noqa: E402


def strip_private(data: dict) -> dict:
    return {k: v for k, v in data.items() if not k.startswith("_") or k == "_canonical"}


def validation(fn, data, path="X"):
    try:
        ok, message = fn(data, Path(path))
        return "" if ok else message
    except Exception as exc:  # pragma: no cover - recorded as the oracle
        return f"EXC {exc}"


def preset_validation(data):
    try:
        validate_preset(data)
        return ""
    except ValueError as exc:
        return str(exc)


out: dict = {}

out["body_plans"] = {pid: {"legs": plan["legs"], "gait": plan["spider_gait"]} for pid, plan in body_plans.BODY_PLANS.items()}

out["palettes"] = {
    "named": [{"id": p.id, "name": p.name, "colors": p.colors} for p in palettes.NAMED_PALETTES],
    "from_hue": [
        {"args": [h, s, v], "colors": palettes.palette_from_hue(h, s, v)}
        for h, s, v in [(0.0, 0.55, 0.78), (0.999, 1.2, 1.4), (-0.3, -0.1, 0.5), (0.5, 0.0, 0.5), (1.25, 0.8, 0.9)]
    ],
    "random": [{"seed": seed, "colors": palettes.random_palette(random.Random(seed))} for seed in range(8)],
    "enemy": palettes.enemy_palette(),
}

out["enemy_kinds"] = [
    {
        "id": k.id, "name": k.name, "model_id": k.model_id, "skins": list(k.skins),
        "size_scale": k.size_scale, "boss": k.boss, "tier": k.tier, "tags": list(k.tags),
        "picks": [enemy_kinds.pick_skin(k, seed) for seed in (0, 1, 2, 3, 17, 12345)],
    }
    for k in enemy_kinds.ENEMY_KINDS.values()
]
out["kinds_for_tier"] = {
    f"{tier}:{boss}": [k.id for k in enemy_kinds.kinds_for_tier(tier, boss)] for tier in (1, 2, 3) for boss in (False, True)
}


def traits_dict(t):
    return {
        "temperament": t.temperament, "abilities": list(t.abilities), "speed": t.speed, "health": t.health,
        "bite": t.bite, "shell": t.shell, "squad_role": t.squad_role, "style": t.style, "look": t.look,
        "leash": t.temper.leash, "notice": t.temper.notice, "chase_forever": t.temper.chase_forever,
        "give_up": t.temper.give_up,
    }


out["enemy_traits"] = {
    "kinds": {kid: traits_dict(t) for kid, t in enemy_traits.TRAITS.items()},
    "roles": {role: traits_dict(enemy_traits.traits_for(None, role)) for role in ("guard", "hunter", "weaver", "spitter", "guardian", "nonsense")},
}

personalities, warnings = discover_personalities(WORKTREE)
assert not warnings, warnings
out["personalities"] = {pid: strip_private(p) for pid, p in personalities.items()}
derived = {}
for pid, p in personalities.items():
    derived[pid] = {
        "modules": sorted(pp.behaviour_modules_for(p)),
        "gait": pp.personality_gait_style(p),
        "default_skills": skills.default_skills_for_personality(p),
        "default_abilities": skills.default_ability_ids(p),
        "no_abilities": skills.skills_with_selected_abilities(p, []),
        "with_job_abilities": skills.skills_with_default_abilities(p, ["shoot_web"]),
        "bundles": pp.ability_bundle_ids_for(p),
    }
out["personality_derived"] = derived
ids = sorted(set(list(personalities) + ["bashful", "weaver", "webslinger", "nonsense", "  HUNTER "]))
out["by_id"] = {
    pid: {
        "temperament": pp.temperament_for(pid),
        "movement": pp.movement_profile_for(pid),
        "phase_scores": pp.phase_scores_for(pid),
        "duration": pp.phase_duration_multiplier_for(pid),
        "abilities": pp.ability_ids_for(pid),
        "modules": sorted(pp.behaviour_modules_for(pid)),
        "default_skills": skills.default_skills_for_personality(pid),
    }
    for pid in ids
}
out["selectable"] = {
    "none": list(pp.selectable_personality_ids()),
    "hunter": list(pp.selectable_personality_ids(personalities, "hunter")),
    "missing": list(pp.selectable_personality_ids(personalities, "nonsense")),
    "no_catalog": list(pp.selectable_personality_ids(None, "Mystery")),
}

models, warnings = discover_models(WORKTREE)
assert not warnings, warnings
out["models"] = {mid: strip_private(m) for mid, m in models.items()}

sample_lists = [None, [], ["jump"], ["JUMP ", "roll", "chase"], ["jump", "roll", "chase", "drift", "nonsense"],
                list(skills.DEFAULT_SKILL_IDS), list(skills.ABILITY_SKILL_IDS), ["web_walk", "shoot_web"]]
out["skill_summaries"] = [
    {"input": value, "skills": skills.compact_skill_summary(value), "abilities": skills.compact_ability_summary(value),
     "normalized": skills.normalize_skill_ids(value), "normalized_abilities": skills.normalize_ability_ids(value),
     "unknown": skills.unknown_skill_ids(value)}
    for value in sample_lists
]

leg = {"name": "a", "side": "left", "gait_group": 0, "attach_angle": 1, "rest_angle": 1, "reach": 1, "upper_len": 1, "lower_len": 1}
base_model = {"id": "m", "display_name": "M", "base_size": 20, "default_personality": "mellow", "colors": {}, "legs": [leg]}
model_cases = [
    base_model,
    {**base_model, "body_plan": "nonsense"},
    {**base_model, "body_plan": 3},
    {k: v for k, v in base_model.items() if k not in ("id", "colors")},
    {**base_model, "legs": []},
    {**base_model, "legs": [{"name": "a"}]},
    {**base_model, "appearance": []},
    {**base_model, "appearance": {"leg_chain": []}},
    {**base_model, "appearance": {"leg_chain": {"enabled": True, "segment_count": 6}}},
    {**base_model, "appearance": {"leg_chain": {"enabled": True, "segment_count": 5.0, "extra_segment_asset": " "}}},
    {**base_model, "appearance": {"leg_chain": {"enabled": True, "segment_scales": [1, 2]}}},
    {**base_model, "appearance": {"leg_chain": {"enabled": True, "joint_scales": [1, 2, 3]}}},
    {**base_model, "appearance": {"leg_chain": {"enabled": True, "leg_curve": True}}},
    {**base_model, "appearance": {"leg_chain": {"enabled": False, "segment_count": 9}}},
    {k: v for k, v in base_model.items() if k != "legs"} | {"body_plan": "Tarantula"},
]
out["model_validation"] = [{"data": c, "message": validation(validate_model, c)} for c in model_cases]

base_p = {"id": "p", "display_name": "P", "speed_multiplier": 1, "reaction_radius": 1, "boldness": 1, "wander_frequency": 1}
personality_cases = [
    base_p,
    {"id": "p"},
    {**base_p, "skills": None},
    {**base_p, "skills": ["jump", "fly"]},
    {**base_p, "behaviours": ["drift"]},
    {**base_p, "abilities": ["jump"]},
    {**base_p, "ability_bundles": ["web_weaver", "nope"]},
    {**base_p, "abilities": "drift"},
    {**base_p, "include_common_abilities": 1},
    {**base_p, "behaviour_modules": ["hunter", "x"]},
    {**base_p, "behaviour_modules": "hunter"},
    {**base_p, "movement_profile": "Bold"},
    {**base_p, "movement_profile": "zzz"},
    {**base_p, "temperament": []},
    {**base_p, "temperament": {"energy": 1}},
    {**base_p, "temperament": dict.fromkeys(pp.TEMPERAMENT_TRAIT_IDS, True)},
    {**base_p, "temperament": dict.fromkeys(pp.TEMPERAMENT_TRAIT_IDS, 11)},
    {**base_p, "temperament": dict.fromkeys(pp.TEMPERAMENT_TRAIT_IDS, "4")},
    {**base_p, "phase_scores": {"fly": 1}},
    {**base_p, "phase_scores": {"jump": "x"}},
    {**base_p, "phase_scores": {"Jump": 12}},
    {**base_p, "phase_duration_multiplier": 3},
    {**base_p, "phase_duration_multiplier": False},
]
out["personality_validation"] = [{"data": c, "message": validation(validate_personality, c)} for c in personality_cases]

slot = {"model": "tarantula", "personality": "mellow", "count": 1}
preset_cases = [
    {"name": "A", "slots": [slot]},
    [],
    {"name": " ", "slots": []},
    {"name": "A"},
    {"name": "A", "slots": [], "settings": []},
    {"name": "A", "slots": [], "settings": {"size_scale": "x"}},
    {"name": "A", "slots": [], "settings": {"size_scale": 3}},
    {"name": "A", "slots": [], "settings": {"interferable": 1}},
    {"name": "A", "slots": [], "settings": {"always_show_xp": "yes"}},
    {"name": "A", "slots": [], "settings": {"mood_mode": "Angry"}},
    {"name": "A", "slots": [], "settings": {"teams": {"a": 3}}},
    {"name": "A", "slots": [], "settings": {"teams": {"a": {"name": 3}}}},
    {"name": "A", "slots": [], "settings": {"teams": {"a": {"color": "#12"}}}},
    {"name": "A", "slots": [], "settings": {"teams": {"a": {"color": [1, 2, 3]}, "b": "Bees"}}},
    {"name": "A", "slots": [], "settings": {"team_relations": {"a": {"b": "enemy"}}}},
    {"name": "A", "slots": [], "settings": {"team_relations": {"a": []}}},
    {"name": "A", "slots": [], "settings": {"flies": {"min_interval": 0}}},
    {"name": "A", "slots": [], "settings": {"flies": {"max_flies": "3.5"}}},
    {"name": "A", "slots": [], "settings": {"flies": {"max_flies": 41}}},
    {"name": "A", "slots": [], "settings": {"flies": {"spawner": None}}},
    {"name": "A", "slots": [3]},
    {"name": "A", "slots": [{"model": "x"}]},
    {"name": "A", "slots": [{**slot, "model": " "}]},
    {"name": "A", "slots": [{**slot, "slot_id": ""}]},
    {"name": "A", "slots": [{**slot, "team_id": 4}]},
    {"name": "A", "slots": [{**slot, "job": "baker"}]},
    {"name": "A", "slots": [{**slot, "job": " Weaver"}]},
    {"name": "A", "slots": [{**slot, "count_random": "no"}]},
    {"name": "A", "slots": [{**slot, "skills": ["jump", "fly"]}]},
    {"name": "A", "slots": [{**slot, "abilities": ["jump"]}]},
    {"name": "A", "slots": [{**slot, "abilities": ["fly"]}]},
    {"name": "A", "slots": [{**slot, "colors": {"body": [1, 2]}}]},
    {"name": "A", "slots": [{**slot, "colors": {"body": [1, 2, 300]}}]},
    {"name": "A", "slots": [{**slot, "count": "two"}]},
    {"name": "A", "slots": [{**slot, "count": 51}]},
    {"name": "A", "slots": [{**slot, "count": 2.9}]},
]
out["preset_validation"] = [{"data": c, "message": preset_validation(c)} for c in preset_cases]
out["safe_filenames"] = {name: safe_preset_filename(name) for name in ["Default", " my/preset?? ", "...", "Colony 2.0", "ünïcode", "a__b"]}

target = HERE.parents[1] / "tests" / "fixtures" / "content_oracle.json"
target.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
print(f"wrote {target}")
