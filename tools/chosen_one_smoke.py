"""Deterministic geometry and locomotion checks for the Chosen One model."""

import json
import math
import random
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from spider_movement_smoke import build_creature, run_causality_checks, run_walk


def main() -> int:
    model = json.loads((ROOT / "models/chosen_one/model.json").read_text())
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    legs = model["legs"]
    appearance = model["appearance"]
    gait = appearance["spider_gait"]
    chain = appearance["leg_chain"]

    assert model["id"] == "chosen_one"
    assert model["render_mode"] == "procedural"
    assert len(legs) == 8
    assert len({round(float(leg["phase_offset"]), 4) for leg in legs}) == 8
    assert chain["enabled"] and chain["segment_count"] == 5
    assert len(chain["joint_phase_offsets"]) == 4
    assert len(chain["bend_directions"]) == 4
    assert chain["bend_directions"][-1] < 0.0
    assert gait["profile"] == "chosen_one"
    assert gait["max_airborne"] == 2

    random.seed(19)
    probe = build_creature(model, personality)
    config = probe._spider_gait_config()
    run_causality_checks(model, personality, config)
    preview_chain = probe._sprite_leg_chain_config()
    preview_points = probe._sprite_leg_chain_points(
        probe.legs[0], *probe._leg_attach(probe.legs[0]),
        *probe._visual_foot_for_render(probe.legs[0]), preview_chain
    )
    assert len(preview_points) == 6, len(preview_points)

    reports = []
    for speed, turn_rate in ((40.0, 0.0), (90.0, 0.0), (180.0, 0.0), (90.0, 1.2), (180.0, -1.2)):
        random.seed(19)
        result = run_walk(model, personality, config, 5.0, speed, turn_rate, 1.0 / 60.0)
        assert result["starts"] > 0
        assert result["outside"] == 0
        assert result["max_airborne"] <= config["max_airborne"]
        assert result["min_supports"] >= 6
        assert result["planted_displacements"] == 0
        assert result["max_chain_stretch"] <= 1.30
        assert result["max_segment_ratio"] <= 1.001
        assert result["max_joint_bend_range"] > 0.08
        assert result["max_joint_motion_spread"] > 1e-5
        assert result["max_pose_jump"] < max(14.0, speed * 0.05)
        assert result["max_heading_jump"] < 0.11
        if turn_rate:
            final_turn = abs(((result["creature"].heading + math.pi) % math.tau) - math.pi)
            assert final_turn > 0.50
        reports.append((speed, turn_rate, result))

    by_dt = []
    for dt in (1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0):
        random.seed(19)
        by_dt.append(run_walk(model, personality, config, 5.0, 90.0, 0.0, dt)["creature"])
    for creature in (by_dt[0], by_dt[2]):
        assert math.hypot(creature.x - by_dt[1].x, creature.y - by_dt[1].y) < 8.0
        assert abs(((creature.heading - by_dt[1].heading + math.pi) % math.tau) - math.pi) < 0.12

    print("chosen-one-smoke: geometry=8-legs/5-segments profile=chosen_one")
    for speed, turn_rate, result in reports:
        print(
            f"  speed={speed:.0f} turn={turn_rate:+.1f} starts={result['starts']} "
            f"air={result['max_airborne']} supports>={result['min_supports']} "
            f"stretch={result['max_chain_stretch']:.3f} "
            f"pose_step={result['max_pose_jump']:.2f}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
