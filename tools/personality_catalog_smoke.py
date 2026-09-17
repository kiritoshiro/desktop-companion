"""Headless checks for the compact personality/ability catalog."""

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.discovery import discover_personalities
from desktop_bug.personality_profiles import (
    BEHAVIOUR_PHASE_IDS,
    COMPACT_TEMPERAMENT_IDS,
    MOVEMENT_PROFILES,
    TEMPERAMENT_TRAIT_IDS,
)
from desktop_bug.skills import (
    ABILITY_SKILL_IDS,
    COMMON_ABILITY_IDS,
    COMMON_BEHAVIOUR_IDS,
    default_skills_for_personality,
    skills_with_selected_abilities,
)


def main() -> int:
    personalities, warnings = discover_personalities(ROOT)
    assert not warnings, warnings
    # Legacy JSONs remain loadable, while the new launch menu is the compact
    # six-choice temperament catalog.
    assert len(personalities) >= 21, len(personalities)
    assert set(COMPACT_TEMPERAMENT_IDS).issubset(personalities)
    assert len(COMPACT_TEMPERAMENT_IDS) == 6
    assert set(ABILITY_SKILL_IDS) == {"drift", "weave_web", "web_walk", "shoot_web", "wall_web"}
    assert COMMON_ABILITY_IDS == ("web_walk",)
    assert "web_walk" not in COMMON_BEHAVIOUR_IDS

    for path in sorted((ROOT / "personalities").glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        assert "skills" not in data, path
        composed = personalities[data["id"]]
        assert composed["movement_profile"] in MOVEMENT_PROFILES
        assert tuple(composed["phase_scores"]) == BEHAVIOUR_PHASE_IDS
        assert all(0.0 <= float(value) <= 10.0 for value in composed["phase_scores"].values())

    for personality_id in COMPACT_TEMPERAMENT_IDS:
        temperament = personalities[personality_id]["temperament"]
        assert tuple(temperament) == TEMPERAMENT_TRAIT_IDS
        assert all(0.0 <= float(value) <= 10.0 for value in temperament.values())

    normal = default_skills_for_personality(personalities["hunter"])
    assert set(COMMON_BEHAVIOUR_IDS).issubset(normal)
    assert "web_walk" in normal
    hunter_scores = personalities["hunter"]["phase_scores"]
    assert hunter_scores["chase"] == 10.0
    assert hunter_scores["prepare_jump_attack"] >= 7.0
    assert all(hunter_scores[phase] == 0.0 for phase in ("jump", "roll", "zoomies", "cuddle", "social_play"))
    hunter_without_abilities = skills_with_selected_abilities(personalities["hunter"], [])
    assert set(COMMON_BEHAVIOUR_IDS).issubset(hunter_without_abilities)
    assert not set(ABILITY_SKILL_IDS).intersection(hunter_without_abilities)

    drifter = default_skills_for_personality(personalities["drifter"])
    assert "drift" in drifter and "web_walk" in drifter

    trapper = default_skills_for_personality(personalities["trapper"])
    assert {"shoot_web", "wall_web", "web_walk"}.issubset(trapper)

    webber = default_skills_for_personality(personalities["webber"])
    assert {"weave_web", "web_walk"}.issubset(webber)

    nope = default_skills_for_personality(personalities["nope"])
    assert set(nope) == {"wander", "jump", "run_away"}

    print(
        f"personality-catalog-smoke: personalities={len(personalities)} "
        f"movement_profiles={len(MOVEMENT_PROFILES)} "
        f"abilities={len(ABILITY_SKILL_IDS)} common_behaviours={len(COMMON_BEHAVIOUR_IDS)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
