"""Headless checks for weighted personality behaviour phases."""

import random
from collections import Counter
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from desktop_bug.personality_profiles import BEHAVIOUR_PHASE_IDS, phase_scores_for
from desktop_bug.creature import Creature
from desktop_bug.discovery import validate_personality
from desktop_bug.phase_scheduler import (
    PHASE_DURATION_RANGES,
    BehaviourPhaseScheduler,
    normalize_focus,
)


def main() -> None:
    scores = {phase_id: 0 for phase_id in BEHAVIOUR_PHASE_IDS}
    scores.update({"roll": 10, "jump": 1})
    personality = {"id": "phase-test", "phase_scores": scores}
    enabled = list(BEHAVIOUR_PHASE_IDS)

    first = BehaviourPhaseScheduler(personality, enabled, random.Random(41))
    second = BehaviourPhaseScheduler(personality, enabled, random.Random(41))
    assert first.deck == second.deck
    assert Counter(first.deck) == Counter({"roll": 10, "jump": 1})
    assert "wander" not in first.deck
    assert first.is_allowed("roll")
    assert not first.is_allowed("wander")

    first_plan = first.choose("mouse")
    second_plan = second.choose("cursor")
    assert first_plan is not None and second_plan is not None
    assert first_plan.phase_id == second_plan.phase_id
    assert first_plan.focus == "cursor"
    assert first_plan.duration == second_plan.duration
    low, high = PHASE_DURATION_RANGES[first_plan.phase_id]
    duration_scale = first.duration_multiplier * (0.72 + first_plan.score / 10.0 * 0.56)
    assert low * duration_scale <= first_plan.duration <= high * duration_scale

    gated = BehaviourPhaseScheduler(
        personality,
        ("jump",),
        random.Random(2),
    )
    assert gated.choose("none").phase_id == "jump"
    assert gated.choose("none").phase_id == "jump"
    empty = BehaviourPhaseScheduler(personality, (), random.Random(2))
    assert empty.choose("none") is None
    target_only_scores = {phase_id: 0 for phase_id in BEHAVIOUR_PHASE_IDS}
    target_only_scores["approach"] = 10
    target_only = BehaviourPhaseScheduler(
        {"id": "target-only", "phase_scores": target_only_scores},
        enabled,
        random.Random(3),
    )
    assert target_only.choose("none") is None
    assert target_only.choose("cursor").phase_id == "approach"

    assert normalize_focus("fly") == "prey"
    assert normalize_focus("another spider") == "none"
    assert phase_scores_for(personality)["roll"] == 10.0
    assert phase_scores_for(personality)["wander"] == 0.0

    runtime_scores = {phase_id: 0 for phase_id in BEHAVIOUR_PHASE_IDS}
    runtime_scores["wander"] = 10
    runtime_personality = {
        "id": "phase-runtime",
        "speed_multiplier": 1.0,
        "reaction_radius": 80,
        "boldness": 0.5,
        "wander_frequency": 1.0,
        "idle_time": [0.0, 0.0],
        "phase_scores": runtime_scores,
    }
    runtime = Creature({"base_size": 25, "legs": []}, runtime_personality, 800, 600)
    runtime.x, runtime.y = 400.0, 300.0
    runtime.state_timer = 0.0
    runtime.decision_timer = 0.0
    runtime.update(1.0 / 60.0, 780.0, 580.0, 800, 600)
    assert runtime.state == "Wander"
    assert runtime.phase_scheduler.current_phase_id == "wander"

    schema_base = {
        "id": "schema-test",
        "display_name": "Schema test",
        "speed_multiplier": 1.0,
        "reaction_radius": 100,
        "boldness": 0.5,
        "wander_frequency": 0.5,
    }
    valid, _ = validate_personality({**schema_base, "phase_scores": {"wander": 0, "chase": 10}}, ROOT / "phase-test.json")
    assert valid
    invalid, _ = validate_personality({**schema_base, "phase_scores": {"wander": 11}}, ROOT / "phase-test.json")
    assert not invalid
    invalid, _ = validate_personality({**schema_base, "behaviours": ["weave_web"]}, ROOT / "phase-test.json")
    assert not invalid
    print(
        "phase-scheduler-smoke: "
        f"deck={len(first.deck) + 1} first={first_plan.phase_id} "
        f"duration={first_plan.duration:.3f} gated=jump runtime=wander deterministic=yes"
    )


if __name__ == "__main__":
    main()
