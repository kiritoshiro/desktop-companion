"""Deterministic geometry, palp and locomotion checks for the tarantula model.

The three parts were one 430-line `main()`. They are split where the state is
actually shared: the model dictionary is only read, so the shape checks stand
alone, and the walks only need the solved gait configuration. The middle part
stays whole because each step mutates the same probe -- the feelers, then the
antennae, then the leg release -- and splitting it would quietly change what
each check starts from.
"""

from __future__ import annotations

import json
import math
import random

import pytest
from movement import (
    advance_controller,
    build_creature,
    run_causality_checks,
    run_heading_filter_check,
    run_walk,
)
from support import ROOT
from desktop_bug.content.body_plans import resolve_body_plan

SEED = 19


@pytest.fixture(scope="module")
def tarantula():
    model = resolve_body_plan(json.loads((ROOT / "models/tarantula/model.json").read_text()))
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    return model, personality


@pytest.fixture(scope="module")
def config(tarantula):
    random.seed(SEED)
    return build_creature(*tarantula)._spider_gait_config()


def test_the_model_is_shaped_as_the_gait_expects(tarantula):
    model, personality = tarantula
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
    # This used to require the patella to be at least as long as the
    # metatarsus, which is the wrong way round for a tarantula. The holotype
    # measurements in [[Tarantula Reference - Brachypelma hamorii]] give leg I
    # a patella of 13.9% and a metatarsus of 22.3% -- the patella is the
    # second-shortest segment on the leg and the metatarsus one of the
    # longest. Drawing it as the longest is what put the knee halfway out
    # along the leg and made each leg read as a spoke.
    lengths = chain["segment_lengths"]
    assert lengths[1] == min(lengths), (
        f"the patella must be the shortest segment, got {lengths}"
    )
    # DC-83: the femur is the longest segment of a real leg (27.1% of leg I
    # in the holotype). DC-75 foreshortened it to 17% for a top-down view and
    # made the metatarsus longest; in the owner's top-down photographs the
    # femur is a long black segment clearly seen before the orange knee, and
    # at 17% it all but vanished under the carapace rim.
    assert lengths[0] == max(lengths), (
        f"the femur must be the longest segment, got {lengths}"
    )
    # and the knee therefore sits nearer the body than the midpoint, which is
    # what a raised knee looks like from directly overhead.
    assert sum(lengths[:2]) / sum(lengths) < 0.42
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
    # DC-83: the leg roots sit *under* the carapace rim. They used to be
    # required out at the flank (>= 58% of the carapace width), which put
    # every socket 1.09-1.23 of the way out on the carapace outline -- just
    # outside it -- and left a sliver of background between each leg and the
    # body: the owner's "gaps between the legs and the body ... background
    # dots". In the photographs every leg comes out from under the rim.
    ceph_half_len = float(ceph_scale[0]) * 0.5
    ceph_half_wid = float(ceph_scale[1]) * 0.5
    for f, s in zip(root_forwards, root_sides):
        r = math.hypot((f - ceph_offset) / ceph_half_len, s / ceph_half_wid)
        assert 0.6 <= r <= 0.92, f"a leg root at ({f}, {s}) is {r:.2f} of the way to the rim"
    assert all(ceph_forward_min <= value <= ceph_forward_max for value in root_forwards)
    # These four bands used to be 35 / 45-60 / 120-135 / 145, drawn around the
    # shipped numbers -- and the shipped numbers were the fault. 29/52/128/151
    # puts the eight legs in four pairs 23 degrees apart with a 76 degree hole
    # where a leg should be pointing straight out sideways, which is what the
    # owner was seeing: *"the body and legs sometimes on spiders become too
    # close or just weird loking."*
    assert default_polar_angles["front_left"] < 40.0
    assert 55.0 < default_polar_angles["mid_front_left"] < 80.0
    assert 100.0 < default_polar_angles["mid_rear_left"] < 125.0
    assert default_polar_angles["rear_left"] > 140.0
    # The property those bands were standing in for, and never checked: the
    # legs are spread, not clumped. A band around each leg separately cannot
    # see a clump, which is how the fault sat here through four packages that
    # each ran this test.
    #
    #   shipped   gaps 23 76 23 58 (mirrored)   widest/narrowest 3.33
    #   here      gaps 35 44 35 66              widest/narrowest 1.89
    ring = sorted(
        (default_polar_angles[leg["name"]]
         * (-1.0 if str(leg.get("side")) == "left" else 1.0)) % 360.0
        for leg in model["legs"]
    )
    gaps = [b - a for a, b in zip(ring, ring[1:])] + [ring[0] + 360.0 - ring[-1]]
    assert min(gaps) > 30.0, f"two legs only {min(gaps):.1f} deg apart"
    assert max(gaps) / min(gaps) < 2.2, (
        f"legs are clumped: widest gap {max(gaps):.1f} deg against narrowest "
        f"{min(gaps):.1f}"
    )
    assert leg_by_name["front_left"]["rest_forward"] > leg_by_name["mid_front_left"]["rest_forward"] > 0.0
    assert leg_by_name["mid_rear_left"]["rest_forward"] > leg_by_name["rear_left"]["rest_forward"]
    pedicel = model["appearance"]["pedicel"]
    head = model["appearance"]["head"]
    assert pedicel["enabled"] is True and head["enabled"] is True
    assert pedicel["scale"][0] < ceph_scale[0]
    assert head["scale"][0] < ceph_scale[0]
    connections = model["appearance"]["leg_connections"]
    # DC-83: off. The painted sockets sat over the shell; the legs now come
    # out from under the carapace rim, as in the owner's photographs, so
    # there is nothing to paint. The geometry stays valid for re-enabling.
    assert connections["enabled"] is False
    assert model["appearance"]["legs_over_body"] is False
    assert connections["coxa_length"] > connections["trochanter_length"] > 0.0
    assert connections["socket_radius"] > connections["joint_radius"]
    antennae = model["appearance"]["antennae"]
    assert antennae["style"] == "tarantula_hand_palps"
    assert antennae["segments"] == 5
    assert len(antennae["segment_lengths"]) == 5
    assert antennae["control_mode"] == "sensory_hand"
    # < 0.50 until DC-83. In the owner's photographs the pedipalps are
    # leg-like and reach about 0.6 of a carapace length ahead of it; short
    # thin palps read as fangs ("look like those teeth more than pedipalps").
    assert antennae["length"] < 0.75
    assert 0.14 <= antennae["proximal_rise"] < 0.22
    # < 0.10 until DC-83: a real pedipalp is nearly as thick as a leg, and a
    # thin one is what made them read as fangs.
    assert antennae["thickness"] < 0.16
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
    # DC-77: these were floors (>= 0.85 rad, >= 4.0 rad/s) set with the
    # smooth-pursuit tuning, and the model sat at 1.2 rad and 8.0 rad/s -- a
    # body allowed to spin 69 degrees under planted feet at 458 deg/s. On a
    # guard's patrol-to-idle turn that is exactly what it did, and all eight
    # legs swept one way round the body: the pinwheel the owner reported.
    # They are now ceilings. The turn is not slower for it (see
    # test_turn_pinwheel.py); the feet replant instead.
    assert 0.25 <= gait["support_turn_limit"] <= 0.45
    assert 3.0 <= gait["max_body_turn_rate"] <= 5.0
    assert gait["turn_gain"] >= 1.5
    assert 0.45 <= gait["turn_step_pressure"] <= 0.88
    assert gait["turn_cycle_gain"] > 0.0
    assert gait["turn_error_drive"] >= 1.0

    average_reach = sum(float(leg["reach"]) for leg in model["legs"]) / 8.0
    average_chain = sum(float(leg["upper_len"]) + float(leg["lower_len"]) for leg in model["legs"]) / 8.0
    assert average_reach >= 2.15
    assert average_chain >= 2.20

def test_the_palps_feelers_and_leg_release_hold_together(tarantula):
    model, personality = tarantula
    ceph_scale = model["appearance"]["cephalothorax_scale"]
    ceph_offset = float(model["appearance"]["cephalothorax_offset_x"])
    antennae = model["appearance"]["antennae"]
    random.seed(19)
    probe = build_creature(model, personality)
    probe.focus_strength = 1.0
    probe.focus_x = probe.x + 1.45 * probe.size
    probe.focus_y = probe.y - 0.80 * probe.size
    probe._update_feelers(1.0 / 60.0, moving=False, turning=False)
    assert probe._feeler_pulse == 0.0
    assert probe.catch_blend == 0.0
    probe._update_antennae(1.0 / 60.0)
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
    # 0.45 before DC-83; the palps are straighter now, as they are in the
    # owner's photographs -- leg-like, reaching forward. The point of the
    # check is that the proximal link stays visibly raised, and 0.30 still is.
    assert probe.antenna_segment_angles[1][0] > 0.30
    assert probe.antenna_segment_angles[1][1] > probe.antenna_segment_angles[1][2]
    # < -0.35 until DC-83; the photographs show the palp tip only slightly
    # turned in, not hooked. Still inward.
    assert probe.antenna_segment_angles[1][-1] < 0.0
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
        # 0.20 when legs I sat at 33 degrees off the body axis. They
        # now sit at 27, because the reference note has them reaching
        # forward close to parallel with the head, so a dangling front leg
        # has less lateral offset by design. The guard is against the legs
        # collapsing into one vertical line, and 0.15 still catches that:
        # measured, the narrowest is 0.195 and the widest 1.16.
        assert abs(held_x - probe.x) > probe.size * 0.15
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
    run_heading_filter_check(
        model,
        json.loads((ROOT / "personalities/hunter.json").read_text()),
    )

    preview_chain = probe._sprite_leg_chain_config()
    for leg in probe.legs:
        points = probe._sprite_leg_chain_points(
            leg, *probe._leg_attach(leg), *probe._visual_foot_for_render(leg), preview_chain
        )
        assert len(points) == 6
        # The proximal segment must lift away from the body; the remaining
        # chain then descends toward the planted foot.
        #
        # This used to be written as `points[1].y <= points[0].y`, i.e. the
        # joint must sit higher up the *screen*. That is the same thing as
        # "away from the body" only for the legs on one side: screen-up is
        # outward for one flank and straight across the shell for the other,
        # so the assertion held while four of the eight legs tucked their
        # first joint under the carapace and vanished (DC-67).
        #
        # Stated in the body's own frame it is the property that was always
        # meant, and it now holds for all eight legs at every heading rather
        # than only at the two where the screen happens to agree.
        root_f, root_s = probe._world_to_body_local(*points[0])
        joint_f, joint_s = probe._world_to_body_local(*points[1])
        side = probe._side_sign(leg.definition.get("side", "right"))
        assert joint_s * side >= root_s * side - 0.25, (
            f"{leg.definition['name']} folds its first joint inboard of its "
            f"own socket: {root_s:.1f} -> {joint_s:.1f}")

@pytest.mark.parametrize(
    "speed,turn_rate",
    [(45.0, 0.0), (100.0, 0.0), (170.0, 0.0), (100.0, 1.1), (170.0, -1.1)],
)
def test_a_walk_stays_inside_its_limits(tarantula, config, speed, turn_rate):
    model, personality = tarantula
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


def test_the_middle_legs_split_forward_and_back():
    """DC-80. The owner: *"the side legs look a bit weird ... they should be
    position one pair more to up other pair more to down side"*. At 72 and
    108 degrees legs II and III sat 18 degrees either side of straight out
    and read as one flat row. Seen from above, a B. hamorii's leg II angles
    forward and leg III back, with a clear gap between them at the flank."""
    import ast
    import math
    from support import ROOT

    source = (ROOT / "src" / "desktop_bug" / "content" / "body_plans.py").read_text(encoding="utf-8")
    node = next(n for n in ast.parse(source).body if isinstance(n, ast.Assign)
                and getattr(n.targets[0], "id", None) == "_TARANTULA_LEGS")
    bearing = {}
    for leg in ast.literal_eval(node.value):
        bearing[leg["name"]] = math.degrees(math.atan2(abs(leg["rest_side"]), leg["rest_forward"]))
    for side in ("left", "right"):
        assert bearing[f"mid_front_{side}"] <= 65.0, bearing
        assert bearing[f"mid_rear_{side}"] >= 115.0, bearing
        assert bearing[f"mid_rear_{side}"] - bearing[f"mid_front_{side}"] >= 50.0, bearing
