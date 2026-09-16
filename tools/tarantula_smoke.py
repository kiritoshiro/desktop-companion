"""Deterministic geometry and locomotion checks for the Tarantula model."""

import json
import math
import random
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from spider_movement_smoke import (
    advance_controller,
    build_creature,
    run_causality_checks,
    run_heading_filter_check,
    run_walk,
)


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
    assert len(chain["width_scales"]) == 5
    assert chain["width_scales"][1] > chain["width_scales"][3] > chain["width_scales"][-1]
    assert chain["segment_color_keys"][1] == "leg_band"
    assert chain["segment_color_keys"][-1] == "leg_tip"
    ceph_scale = model["appearance"]["cephalothorax_scale"]
    ceph_offset = float(model["appearance"]["cephalothorax_offset_x"])
    ceph_forward_min = ceph_offset - float(ceph_scale[0]) * 0.52
    ceph_forward_max = ceph_offset + float(ceph_scale[0]) * 0.52
    root_sides = [float(leg["attach_side"]) for leg in model["legs"]]
    root_forwards = [float(leg["attach_forward"]) for leg in model["legs"]]
    leg_by_name = {leg["name"]: leg for leg in model["legs"]}
    default_polar_angles = {
        name: math.degrees(math.atan2(float(leg["rest_side"]), float(leg["rest_forward"])))
        for name, leg in leg_by_name.items()
    }
    # Leg roots should sit near the carapace rim, not far outside it on exposed
    # tubes and not so deep that the shell hides the proximal joints.
    assert min(root_sides) >= float(ceph_scale[1]) * 0.58
    assert max(root_sides) <= float(ceph_scale[1]) * 0.75
    assert all(ceph_forward_min <= value <= ceph_forward_max for value in root_forwards)
    assert default_polar_angles["front_left"] < 35.0
    assert 45.0 < default_polar_angles["mid_front_left"] < 60.0
    assert 120.0 < default_polar_angles["mid_rear_left"] < 135.0
    assert default_polar_angles["rear_left"] > 145.0
    assert leg_by_name["front_left"]["rest_forward"] > leg_by_name["mid_front_left"]["rest_forward"] > 0.0
    assert leg_by_name["mid_rear_left"]["rest_forward"] > leg_by_name["rear_left"]["rest_forward"]
    pedicel = model["appearance"]["pedicel"]
    head = model["appearance"]["head"]
    assert pedicel["enabled"] is True and head["enabled"] is True
    assert pedicel["scale"][0] < ceph_scale[0]
    assert head["scale"][0] < ceph_scale[0]
    connections = model["appearance"]["leg_connections"]
    assert connections["enabled"] is True
    assert connections["coxa_length"] > connections["trochanter_length"] > 0.0
    assert connections["socket_radius"] > connections["joint_radius"]
    antennae = model["appearance"]["antennae"]
    assert antennae["style"] == "tarantula_hand_palps"
    assert antennae["segments"] == 5
    assert len(antennae["segment_lengths"]) == 5
    assert antennae["control_mode"] == "sensory_hand"
    assert antennae["length"] < 0.50
    assert 0.14 <= antennae["proximal_rise"] < 0.22
    assert antennae["thickness"] < 0.10
    assert antennae["leg_thickness"] < 1.0
    assert antennae["joint_scale"] < 1.15
    assert antennae["segment_widths"][0] > antennae["segment_widths"][-1]
    assert antennae["segment_widths"][-1] < 0.60
    assert antennae["segment_color_keys"][1] == "leg_band"
    assert antennae["claw_color_key"] == "leg_tip"
    assert 0.45 <= antennae["rub_frequency"] <= 1.80
    assert antennae["rub_lateral_amount"] > 0.0
    assert antennae["rub_joint_angle"] >= 0.40
    assert antennae["rest_angles"][0] > 0.50
    assert antennae["rest_angles"][2] > 0.0
    assert antennae["rest_angles"][-1] < antennae["rest_angles"][2]
    rest_turns = [
        abs(b - a)
        for a, b in zip(antennae["rest_angles"], antennae["rest_angles"][1:])
    ]
    assert max(rest_turns) - min(rest_turns) < 0.20
    assert len(antennae["joint_angle_limits"]) == antennae["segments"]
    assert antennae["joint_angle_limits"][-1] < antennae["joint_angle_limits"][1]
    assert len(antennae["free_range_of_motion"]) == antennae["segments"]
    assert antennae["free_range_of_motion"][1] > 0.60
    assert antennae["free_range_of_motion"][1] > antennae["free_range_of_motion"][-1]
    assert antennae["hand_max_forward"] > antennae["hand_rest_forward"]
    assert antennae["hand_max_lateral"] > antennae["hand_rest_lateral"]
    assert antennae["hand_max_forward"] < 0.72
    assert antennae["hand_max_lateral"] < 0.54
    assert antennae["max_drive_length"] <= 1.10
    assert antennae["max_extension"] <= 0.06
    assert antennae["symmetric_rest"] is True
    assert 0.0 < antennae["screen_lift_scale"] < 0.45
    assert len(antennae["joint_steering"]) == antennae["segments"]
    assert len(antennae["joint_steering_limits"]) == antennae["segments"]
    assert antennae["joint_steering"][1] > antennae["joint_steering"][0]
    assert antennae["joint_steering"][3] < antennae["joint_steering"][1]
    assert antennae["joint_steering_limits"][3] < antennae["joint_steering_limits"][1]
    assert antennae["serial_follow"] >= 0.65
    assert len(chain["bend_directions"]) == 4
    assert chain["bend_directions"][-1] < 0.0
    assert gait["profile"] == "tarantula"
    assert gait["max_airborne"] == 3
    assert gait["stride_gain"] > 1.0
    assert gait["step_trigger"] >= 0.25
    assert gait["stance_deadband"] > gait["step_trigger"]
    assert gait["stance_deadband"] >= 0.50
    assert gait["support_stroke_limit"] >= 0.90
    assert gait["support_turn_limit"] >= 0.85
    assert gait["max_body_turn_rate"] >= 4.0
    assert gait["turn_gain"] >= 1.5
    assert 0.45 <= gait["turn_step_pressure"] <= 0.88
    assert gait["turn_cycle_gain"] > 0.0
    assert gait["turn_error_drive"] >= 1.0

    average_reach = sum(float(leg["reach"]) for leg in model["legs"]) / 8.0
    average_chain = sum(float(leg["upper_len"]) + float(leg["lower_len"]) for leg in model["legs"]) / 8.0
    assert average_reach >= 2.15
    assert average_chain >= 2.20

    random.seed(19)
    probe = build_creature(model, personality)
    probe.focus_strength = 1.0
    probe.focus_x = probe.x + 1.45 * probe.size
    probe.focus_y = probe.y - 0.80 * probe.size
    probe._update_feelers(1.0 / 60.0, moving=False, turning=False)
    assert probe._feeler_pulse == 0.0
    assert probe.catch_blend == 0.0
    probe._update_antennae(1.0 / 60.0)
    rest_forward = float(antennae["hand_rest_forward"]) * probe.size
    rest_lateral = float(antennae["hand_rest_lateral"]) * probe.size
    # Idle hands may already be inside a broad grooming stroke on the first
    # frame. They must stay inside the configured envelope and remain mirrored,
    # but they no longer need to be pinned to one exact rest coordinate.
    assert all(
        float(antennae["hand_min_forward"]) * probe.size
        <= target_f
        <= float(antennae["hand_max_forward"]) * probe.size
        and float(antennae["hand_min_lateral"]) * probe.size
        <= abs(target_s)
        <= float(antennae["hand_max_lateral"]) * probe.size
        for target_f, target_s in probe.antenna_hand_targets
    )
    assert abs(
        probe.antenna_hand_targets[0][0]
        - probe.antenna_hand_targets[1][0]
    ) < probe.size * 0.04
    assert abs(
        probe.antenna_hand_targets[0][1]
        + probe.antenna_hand_targets[1][1]
    ) < probe.size * 0.04
    first_angles = [list(row) for row in probe.antenna_segment_angles]
    assert max(
        abs(left + right)
        for left, right in zip(
            probe.antenna_segment_angles[0], probe.antenna_segment_angles[1]
        )
    ) < 0.08
    # The draw-time lift is part of the visible palp pose. Verify the complete
    # 2-D chain is mirrored too; checking angles alone would miss the old bug
    # where both sides subtracted the same screen-Y lift.
    palp_cfg = antennae
    palp_lengths = [float(value) for value in palp_cfg["segment_lengths"]]
    palp_total = sum(palp_lengths)
    palp_ceph_h = float(ceph_scale[1])
    palp_base_x = (
        float(ceph_offset) + float(ceph_scale[0]) * float(palp_cfg["base_forward"])
    ) * probe.size
    palp_root_side = palp_ceph_h * float(palp_cfg["base_side"]) * probe.size

    def palp_points(side_index: int):
        side_sign = -1.0 if side_index == 0 else 1.0
        fwd = 0.0
        lateral = 0.0
        points = []
        for index, angle in enumerate(probe.antenna_segment_angles[side_index]):
            length = probe.size * float(palp_cfg["length"]) * palp_lengths[index] / palp_total
            fwd += math.cos(angle) * length
            lateral += math.sin(angle) * length
            points.append(
                (
                    palp_base_x + fwd,
                    side_sign * palp_root_side + lateral
                    - side_sign
                    * probe.size
                    * probe.antenna_joint_lifts[side_index][index]
                    * float(palp_cfg["screen_lift_scale"]),
                )
            )
        return points

    left_palp = palp_points(0)
    right_palp = palp_points(1)
    assert max(abs(left[0] - right[0]) for left, right in zip(left_palp, right_palp)) < 0.08
    assert max(abs(left[1] + right[1]) for left, right in zip(left_palp, right_palp)) < 0.08
    idle_lateral = []
    idle_middle_angles = []
    idle_joint_ranges = [[] for _ in range(antennae["segments"])]
    idle_side_difference = []
    for _ in range(240):
        probe._update_antennae(1.0 / 60.0)
        idle_lateral.append(probe.antenna_hand_targets[0][1])
        idle_middle_angles.append(probe.antenna_segment_angles[0][2])
        for joint_index in range(antennae["segments"]):
            idle_joint_ranges[joint_index].append(
                probe.antenna_segment_angles[0][joint_index]
            )
        idle_side_difference.append(
            max(
                abs(left + right)
                for left, right in zip(
                    probe.antenna_segment_angles[0],
                    probe.antenna_segment_angles[1],
                )
            )
        )
    # Passive palps should occasionally work through a small hand-rub stroke;
    # otherwise the articulated chain is technically correct but visually dead.
    assert max(idle_lateral) - min(idle_lateral) > probe.size * 0.025
    assert max(idle_middle_angles) - min(idle_middle_angles) > 0.22
    assert max(max(values) - min(values) for values in idle_joint_ranges) > 0.22
    assert max(idle_side_difference) > 0.08
    probe._update_antennae(1.0 / 60.0)
    assert all(len(row) == antennae["segments"] for row in probe.antenna_segment_angles)
    assert max(
        abs(after - before)
        for before_row, after_row in zip(first_angles, probe.antenna_segment_angles)
        for before, after in zip(before_row, after_row)
    ) > 1e-5
    assert all(row[0] > row[-1] for row in probe.antenna_joint_lifts)
    lower_palp = probe.antenna_segment_angles[1]
    assert lower_palp[1] > lower_palp[2] > lower_palp[3]
    assert lower_palp[3] > lower_palp[4]
    probe.inspect_intent = 1.0
    probe._update_antennae(1.0 / 60.0)
    assert all(
        float(antennae["hand_min_forward"]) * probe.size <= target_f <= float(antennae["hand_max_forward"]) * probe.size
        and float(antennae["hand_min_lateral"]) * probe.size <= abs(target_s) <= float(antennae["hand_max_lateral"]) * probe.size
        for target_f, target_s in probe.antenna_hand_targets
    )
    probe.catch_blend = 1.0
    probe.catch_point = (probe.x + 0.90 * probe.size, probe.y + 0.35 * probe.size)
    probe._update_antennae(1.0 / 60.0)
    assert max(probe.antenna_hand_grips) > 0.01
    # A target may move the hand, but it must do so through the knuckles.  The
    # raised proximal link and the curled distal link remain distinct after the
    # command has had time to settle.
    for _ in range(120):
        probe._update_antennae(1.0 / 60.0)
    assert probe.antenna_segment_angles[1][0] > 0.45
    assert probe.antenna_segment_angles[1][1] > probe.antenna_segment_angles[1][2]
    assert probe.antenna_segment_angles[1][-1] < -0.35
    probe.start_drag(probe.x, probe.y)
    assert probe._startle_amount() == 1.0
    assert probe._startle_highlight_active(probe._startle_amount()) is False
    probe.update(1.0 / 60.0, probe.x, probe.y, 2400, 1400)
    assert probe.body_bob == 0.0
    assert probe.body_sway == 0.0
    drag_leg = probe.legs[0]
    probe.drag_to(1.0 / 60.0, probe.x + 120.0, probe.y)
    probe.update(1.0 / 60.0, probe.x, probe.y, 2400, 1400)
    moving_held_x = probe._leg_draw_points(drag_leg)[2] - probe.x
    held_spring_samples = []
    for _ in range(40):
        probe.drag_to(1.0 / 60.0, probe.x, probe.y)
        probe.update(1.0 / 60.0, probe.x, probe.y, 2400, 1400)
        held_spring_samples.append(
            max(
                math.hypot(leg.held_spring_x, leg.held_spring_y)
                for leg in probe.legs
            )
        )
    settled_held_x = probe._leg_draw_points(drag_leg)[2] - probe.x
    assert probe.held_leg_relax == 1.0
    # The endpoint should settle back toward its own body-relative lane, but
    # the carried pose intentionally keeps it there instead of re-spreading.
    assert settled_held_x > moving_held_x + probe.size * 0.25
    assert max(held_spring_samples) > probe.size * 0.025
    drag_ax, drag_ay = probe._leg_attach(drag_leg)
    normal_fx, normal_fy = probe._visual_foot_for_render(drag_leg)
    _, _, picked_fx, picked_fy = probe._leg_draw_points(drag_leg)
    picked_distance = math.hypot(picked_fx - drag_ax, picked_fy - drag_ay)
    # With lateral coxa roots, a carried foot may hang farther from the root
    # than its grounded contact. It must remain bounded, not be shorter than
    # the normal stance by an arbitrary ratio.
    assert picked_distance < probe.size * 1.55
    _, lifted_test_y = probe._picked_up_leg_pose(drag_leg, drag_ax, drag_ay, drag_ax, drag_ay)
    assert lifted_test_y > probe.y + probe.size * 0.50
    probe.breath_phase = 0.0
    held_pose_a = probe._picked_up_leg_pose(drag_leg, drag_ax, drag_ay, normal_fx, normal_fy)
    probe.breath_phase = 9.0
    held_pose_b = probe._picked_up_leg_pose(drag_leg, drag_ax, drag_ay, normal_fx, normal_fy)
    assert held_pose_a == held_pose_b
    for leg in probe.legs:
        _, _, held_x, held_y = probe._leg_draw_points(leg)
        assert held_y > probe.y + probe.size * 0.50
        assert abs(held_x - probe.x) > probe.size * 0.20
    held_xs = [probe._leg_draw_points(leg)[2] for leg in probe.legs]
    assert max(held_xs) - min(held_xs) > probe.size * 1.60
    slow_held = probe._picked_up_leg_pose(drag_leg, drag_ax, drag_ay, normal_fx, normal_fy)
    probe.drag_vel_x = 520.0
    probe.drag_vel_y = 0.0
    probe.current_speed = 260.0
    fast_held = probe._picked_up_leg_pose(drag_leg, drag_ax, drag_ay, normal_fx, normal_fy)
    assert fast_held[0] < slow_held[0]
    assert fast_held[1] > slow_held[1] + probe.size * 0.02
    probe.drag_vel_x = 0.0
    probe.current_speed = 0.0
    probe.legs[0].stepping = True
    probe.legs[0].lift = 1.0
    probe._update_legs(1.0 / 60.0)
    assert probe.legs[0].stepping is True
    assert probe.legs[0].lift == 1.0
    probe.legs[0].stepping = False
    probe.legs[0].lift = 0.0
    probe.release_drag(probe.x, probe.y)
    assert probe._startle_highlight_active(probe._startle_amount()) is False
    assert probe.held_release_timer > 0.0
    release_chain = probe._sprite_leg_chain_config()
    for leg in probe.legs:
        ax, ay = probe._leg_attach(leg)
        _, _, fx, fy = probe._leg_draw_points(leg)
        fx, fy = probe._safe_sprite_leg_foot(leg, fx, fy, release_chain)
        released_span = math.hypot(fx - ax, fy - ay)
        chain_span = (
            float(leg.definition.get("upper_len", 0.85))
            + float(leg.definition.get("lower_len", 1.05))
        ) * probe.size * release_chain["max_stretch"]
        assert released_span <= chain_span + 1e-4
    config = probe._spider_gait_config()
    run_causality_checks(model, personality, config)

    # A stationary 90-degree pivot should not spend most of its time waiting
    # for the next foot handoff. Reassert the target point as the body moves so
    # this measures the mechanical turn response, not target pursuit.
    random.seed(19)
    pivot = build_creature(model, personality)
    pivot.speed = 0.0
    pivot.turn_rate = 5.0
    pivot.target_heading = math.pi * 0.5
    pivot_config = pivot._spider_gait_config()
    early_pivot_heading = 0.0
    pivot_launches = 0
    pivot_45_time = None
    for _ in range(96):
        pivot.target_x = pivot.x
        pivot.target_y = pivot.y
        pivot.target_heading = math.pi * 0.5
        before_swinging = sum(leg.stepping or leg.pending_step for leg in pivot.legs)
        advance_controller(pivot, 1.0 / 60.0, pivot_config)
        after_swinging = sum(leg.stepping or leg.pending_step for leg in pivot.legs)
        pivot_launches += max(0, after_swinging - before_swinging)
        if pivot_45_time is None and pivot.heading >= math.pi * 0.25 - 0.01:
            pivot_45_time = (_ + 1) / 60.0
        if early_pivot_heading == 0.0 and pivot.heading >= 1.25:
            early_pivot_heading = pivot.heading
    pivot_heading = abs(((pivot.heading + math.pi) % math.tau) - math.pi)
    # A turn should free a stressed stance early enough to keep rotating. This
    # catches regressions where the final angle passes but the first second is
    # spent waiting on the generic walking cadence.
    assert early_pivot_heading >= 1.25
    assert pivot_heading > 1.40
    assert pivot_45_time is not None and pivot_45_time <= 0.75
    assert pivot_launches <= 14

    # Positive screen-space turns contract the positive-side/inside leg and
    # extend the opposite-side/outside leg. This guards the stance actuation
    # sign independently of the final visual heading.
    left_change = pivot._spider_turn_radial_adjustment(
        pivot.legs[0], pivot.size, pivot.size, 0.10, pivot_config
    )
    right_change = pivot._spider_turn_radial_adjustment(
        pivot.legs[1], pivot.size, pivot.size, 0.10, pivot_config
    )
    assert left_change > 0.0 and right_change < 0.0

    random.seed(19)
    hunter_heading_filter = run_heading_filter_check(
        model,
        json.loads((ROOT / "personalities/hunter.json").read_text()),
    )

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
        assert result["min_supports"] >= 5
        assert result["planted_displacements"] == 0
        assert result["max_chain_stretch"] <= 1.30
        assert result["max_segment_ratio"] <= 1.001
        assert result["max_joint_bend_range"] > 0.08
        assert result["max_joint_motion_spread"] > 1e-5
        assert result["max_heading_jump"] < 0.12
        if not turn_rate and speed >= 100.0:
            travelled = math.hypot(result["creature"].x - 900.0, result["creature"].y - 700.0)
            assert travelled > (300.0 if speed < 150.0 else 650.0)
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
    print(
        f"  hunter zigzag target_step={hunter_heading_filter['max_target_step']:.3f} "
        f"settled={hunter_heading_filter['settled_amplitude']:.3f} "
        f"sustained_error={hunter_heading_filter['sustained_error']:.3f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
