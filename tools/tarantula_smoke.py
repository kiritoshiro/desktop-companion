"""Deterministic geometry and locomotion checks for the Tarantula model."""

import json
import math
import random
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from spider_movement_smoke import build_creature, run_causality_checks, run_walk


def main() -> int:
    model = json.loads((ROOT / "models/tarantula/model.json").read_text())
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    chain = model["appearance"]["leg_chain"]
    gait = model["appearance"]["spider_gait"]

    assert model["id"] == "tarantula"
    assert model["render_mode"] == "procedural"
    assert len(model["legs"]) == 8
    assert chain["enabled"] and chain["segment_count"] == 5
    assert chain["elevated_arc"] >= 0.05
    assert chain["proximal_lift"] > 1.0
    assert chain["hairy"] is True
    assert chain["hair_scale"] > 0.0
    assert len(chain["segment_lengths"]) == 5
    assert chain["segment_lengths"][1] >= chain["segment_lengths"][3]
    antennae = model["appearance"]["antennae"]
    assert antennae["style"] == "tarantula_front_legs"
    assert antennae["segments"] == 5
    assert len(antennae["segment_lengths"]) == 5
    assert len(chain["bend_directions"]) == 4
    assert chain["bend_directions"][-1] < 0.0
    assert gait["profile"] == "tarantula"
    assert gait["max_airborne"] == 2

    average_reach = sum(float(leg["reach"]) for leg in model["legs"]) / 8.0
    average_chain = sum(float(leg["upper_len"]) + float(leg["lower_len"]) for leg in model["legs"]) / 8.0
    assert average_reach >= 2.15
    assert average_chain >= 2.20

    random.seed(19)
    probe = build_creature(model, personality)
    config = probe._spider_gait_config()
    run_causality_checks(model, personality, config)
    preview_chain = probe._sprite_leg_chain_config()
    for leg in probe.legs:
        points = probe._sprite_leg_chain_points(
            leg, *probe._leg_attach(leg), *probe._visual_foot_for_render(leg), preview_chain
        )
        assert len(points) == 6
        # At the neutral heading the proximal segment must lift from the body;
        # the remaining chain then descends toward the planted foot.
        assert points[1][1] <= points[0][1] + 0.25

    reports = []
    for speed, turn_rate in ((45.0, 0.0), (100.0, 0.0), (170.0, 0.0), (100.0, 1.1), (170.0, -1.1)):
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
        assert result["max_heading_jump"] < 0.12
        if turn_rate:
            final_turn = abs(((result["creature"].heading + math.pi) % math.tau) - math.pi)
            assert final_turn > 0.50
        reports.append((speed, turn_rate, result))

    print("tarantula-smoke: geometry=8-legs/5-segments/elevated profile=tarantula")
    for speed, turn_rate, result in reports:
        print(
            f"  speed={speed:.0f} turn={turn_rate:+.1f} starts={result['starts']} "
            f"air={result['max_airborne']} supports>={result['min_supports']} "
            f"stretch={result['max_chain_stretch']:.3f} "
            f"segment_limit={result['max_segment_ratio']:.3f} "
            f"joint_range={result['max_joint_bend_range']:.3f}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
