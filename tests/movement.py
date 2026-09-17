"""The spider gait rig, driven headlessly.

These helpers build a creature, step its controller and walk it for a
while, reporting what the legs did. Two test modules use them --
the gait checks and the tarantula checks -- so they live beside the tests
rather than inside one of them; importing one test module from another
made the gait file both a test and a library.
"""

from __future__ import annotations

import math

from desktop_bug.creature import Creature


def build_creature(model: dict, personality: dict, *, gait_style: str = "lively") -> Creature:
    creature = Creature(model, personality, 2400, 1400, gait_style=gait_style)
    creature.x = 900.0
    creature.y = 700.0
    creature.heading = 0.0
    creature.target_heading = 0.0
    creature.target_x = 2100.0
    creature.target_y = 700.0
    creature.current_speed = 0.0
    creature.speed = 80.0
    creature.vel_x = creature.vel_y = 0.0
    creature.state = "Wander"
    creature._initialize_legs()
    return creature


def advance_controller(creature: Creature, dt: float, config: dict) -> None:
    """Use the production grounded update order without a GUI frame."""
    creature._update_spider_grounded_frame(dt)
    # In the GUI path _update_legs consumes this flag after the body update.
    creature._spider_gait_frame_updated = False


def run_causality_checks(model: dict, personality: dict, config: dict) -> None:
    frozen = build_creature(model, personality)
    frozen.target_x = frozen.x
    frozen.target_y = frozen.y
    frozen.speed = frozen.current_speed = 0.0
    frozen_heading = frozen.heading
    frozen_pos = (frozen.x, frozen.y)
    for _ in range(30):
        frozen._update_spider_grounded_locomotion(1.0 / 60.0)
    assert math.hypot(frozen.x - frozen_pos[0], frozen.y - frozen_pos[1]) < 1e-6
    assert abs(((frozen.heading - frozen_heading + math.pi) % math.tau) - math.pi) < 1e-6

    swing_only = build_creature(model, personality)
    swing_only.target_x = swing_only.x
    swing_only.target_y = swing_only.y
    swing_only.speed = swing_only.current_speed = 0.0
    leg = swing_only.legs[0]
    swing_only._schedule_step(leg, leg.foot_x + 5.0, leg.foot_y)
    before = (swing_only.x, swing_only.y, swing_only.heading)
    for _ in range(8):
        swing_only._update_spider_grounded_locomotion(1.0 / 60.0)
        swing_only._advance_active_steps(1.0 / 60.0)
    after = (swing_only.x, swing_only.y, swing_only.heading)
    assert math.hypot(after[0] - before[0], after[1] - before[1]) < 1e-6
    assert abs(((after[2] - before[2] + math.pi) % math.tau) - math.pi) < 1e-6


def run_heading_filter_check(model: dict, personality: dict) -> dict:
    """Check that cursor reversals do not become body-heading twitch commands."""
    creature = build_creature(model, personality)
    creature.state = "Chase"
    creature.speed = creature.current_speed = 0.0
    creature._spider_heading_filter = creature.heading
    config = creature._spider_gait_config()
    assert config is not None

    previous = creature.target_heading
    max_target_step = 0.0
    zigzag_values = []
    dt = 1.0 / 60.0
    for frame in range(180):
        raw_angle = 0.78 if frame % 2 else -0.78
        creature.target_x = creature.x + math.cos(raw_angle) * 500.0
        creature.target_y = creature.y + math.sin(raw_angle) * 500.0
        creature._spider_locomotion_intent(dt)
        step = abs(((creature.target_heading - previous + math.pi) % math.tau) - math.pi)
        max_target_step = max(max_target_step, step)
        zigzag_values.append(creature.target_heading)
        previous = creature.target_heading

    settled_amplitude = max(abs(value) for value in zigzag_values[-60:])
    assert max_target_step < 0.20, max_target_step
    assert settled_amplitude < 0.13, settled_amplitude

    # A deliberate, sustained turn must still arrive promptly; the filter is
    # for cursor noise, not a multi-second turn-rate penalty.
    creature.target_x = creature.x + math.cos(0.78) * 500.0
    creature.target_y = creature.y + math.sin(0.78) * 500.0
    for _ in range(30):
        creature._spider_locomotion_intent(dt)
    sustained_error = abs(((creature.target_heading - 0.78 + math.pi) % math.tau) - math.pi)
    assert sustained_error < 0.14, sustained_error
    return {
        "max_target_step": max_target_step,
        "settled_amplitude": settled_amplitude,
        "sustained_error": sustained_error,
    }


def run_roll_recovery_check(model: dict, personality: dict) -> dict:
    """Ensure a playful roll cannot leave stale or overextended contacts."""
    creature = build_creature(model, personality)
    # Exercise the production update order, including scheduler sync. A direct
    # _update_roll call cannot catch a phase timer ending Roll before the
    # physical animation reaches progress 1.0.
    creature.squash = 0.66
    creature.current_speed = 130.0
    creature.vel_x = 130.0
    creature.enter_roll(direction=0.0)
    assert creature.state == "Roll"
    for _ in range(180):
        creature.update(1.0 / 60.0, 0.0, 0.0, 2400, 1400)
        if creature.state != "Roll" and creature.roll_progress >= 1.0:
            break
    assert creature.state == "Idle"
    assert creature.roll_spin == 0.0
    assert creature.roll_tuck == 0.0
    assert creature.squash == 1.0
    assert creature.current_speed == 0.0
    assert creature.vel_x == 0.0 and creature.vel_y == 0.0
    assert not any(leg.stepping or leg.pending_step for leg in creature.legs)
    for leg in creature.legs:
        _, _, _, very_far = creature._leg_reach_metrics(
            leg, leg.foot_x, leg.foot_y, visual=False
        )
        assert not very_far
    return {"contacts": len(creature.legs), "state": creature.state}


def run_quick_turn_check(model: dict, personality: dict) -> dict:
    """Check that a sharp turn is fast, monotonic, and frame-smooth."""
    creature = build_creature(model, personality)
    creature.speed = creature.current_speed = 0.0
    creature.target_x = creature.x + 500.0
    creature.target_y = creature.y
    creature.target_heading = 0.0
    dt = 1.0 / 60.0
    max_step = 0.0
    reversals = 0
    previous_step = 0.0
    for frame in range(90):
        angle = 0.0 if frame < 8 else math.pi * 0.5
        creature.target_x = creature.x + math.cos(angle) * 500.0
        creature.target_y = creature.y + math.sin(angle) * 500.0
        creature.target_heading = angle
        old_heading = creature.heading
        if creature._spider_gait_config() is not None:
            creature._update_spider_grounded_frame(dt)
        else:
            creature._move_body(dt)
            creature._update_legs_lively(dt)
        step = ((creature.heading - old_heading + math.pi) % math.tau) - math.pi
        max_step = max(max_step, abs(step))
        if abs(step) > 1e-5 and previous_step * step < -1e-5:
            reversals += 1
        previous_step = step

    final_error = abs(((creature.heading - math.pi * 0.5 + math.pi) % math.tau) - math.pi)
    assert max_step < 0.12, max_step
    assert reversals == 0, reversals
    assert final_error < 0.08, final_error
    return {
        "max_step": max_step,
        "reversals": reversals,
        "final_error": final_error,
    }


def run_walk(model: dict, personality: dict, config: dict, seconds: float,
             speed: float, turn_rate: float, dt: float):
    creature = build_creature(model, personality)
    creature.speed = speed
    if abs(turn_rate) > 1e-6:
        # A pure heading request makes this a support-driven in-place/slow arc
        # turn instead of letting target pursuit overwrite the turn target.
        creature.target_x = creature.x
        creature.target_y = creature.y
        creature.turn_rate = abs(turn_rate)
        creature.target_heading = math.copysign(math.pi * 0.5, turn_rate)

    outside_starts = 0
    normal_starts = 0
    max_airborne = 0
    planted_displacements = 0
    max_chain_stretch = 0.0
    max_segment_ratio = 0.0
    max_pose_jump = 0.0
    max_heading_jump = 0.0
    max_joint_bend_range = 0.0
    max_joint_motion_spread = 0.0
    previous_joint_bends = None
    min_supports = len(creature.legs)
    frames = max(1, round(seconds / dt))
    for _ in range(frames):
        if abs(turn_rate) > 1e-6:
            # Keep this benchmark a pure mechanical pivot.  The production
            # controller normally derives heading from a moving target, but
            # allowing that target to drift as the support solver translates
            # the body turns this check into a pursuit test.
            creature.target_x = creature.x
            creature.target_y = creature.y
            creature.target_heading = math.copysign(math.pi * 0.5, turn_rate)
        old_pose = (creature.x, creature.y, creature.heading)
        old_contacts = {
            id(leg): (leg.foot_x, leg.foot_y)
            for leg in creature.legs
            if leg.contact_state == "stance" and not leg.stepping and not leg.pending_step
        }
        was_stepping = [leg.stepping or leg.pending_step for leg in creature.legs]
        was_emergency = []
        for leg in creature.legs:
            _, _, _, _, _, severe_wrong = creature._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, _, very_far = creature._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            was_emergency.append(severe_wrong or very_far)

        advance_controller(creature, dt, config)
        pose_jump = math.hypot(creature.x - old_pose[0], creature.y - old_pose[1])
        heading_jump = abs(((creature.heading - old_pose[2] + math.pi) % math.tau) - math.pi)
        max_pose_jump = max(max_pose_jump, pose_jump)
        max_heading_jump = max(max_heading_jump, heading_jump)
        min_supports = min(min_supports, sum(
            leg.contact_state == "stance" and not leg.stepping for leg in creature.legs
        ))

        max_airborne = max(max_airborne, sum(leg.stepping or leg.pending_step for leg in creature.legs))
        for index, leg in enumerate(creature.legs):
            if id(leg) in old_contacts and not was_stepping[index] and leg.contact_state == "stance" and not leg.stepping:
                old_x, old_y = old_contacts[id(leg)]
                planted_displacements += int(math.hypot(leg.foot_x - old_x, leg.foot_y - old_y) > 1e-6)
                visible_x, visible_y = creature._visual_foot_for_render(leg)
                planted_displacements += int(math.hypot(visible_x - leg.foot_x, visible_y - leg.foot_y) > 1e-6)
            if not was_stepping[index] and leg.stepping:
                normal_starts += 1
                in_window, _, _ = creature._spider_phase_window(leg, leg.last_step_phase, config)
                if not in_window and not leg.last_step_emergency:
                    outside_starts += 1
            chain_config = creature._sprite_leg_chain_config()
            if chain_config:
                ax, ay = creature._leg_attach(leg)
                fx, fy = creature._visual_foot_for_render(leg)
                points = creature._sprite_leg_chain_points(leg, ax, ay, fx, fy, chain_config)
                direct = math.hypot(points[-1][0] - points[0][0], points[-1][1] - points[0][1])
                path = sum(math.hypot(points[i + 1][0] - points[i][0], points[i + 1][1] - points[i][1]) for i in range(len(points) - 1))
                if direct > 1e-6:
                    max_chain_stretch = max(max_chain_stretch, path / direct)
                upper = max(creature.size * 0.22, float(leg.definition.get("upper_len", 0.85)) * creature.size)
                lower = max(creature.size * 0.22, float(leg.definition.get("lower_len", 1.05)) * creature.size)
                length_weights = chain_config["segment_lengths"]
                weight_total = max(1e-4, sum(length_weights))
                segment_limits = [
                    (upper + lower) * chain_config["max_stretch"] * weight / weight_total
                    for weight in length_weights
                ]
                for segment_index, limit in enumerate(segment_limits):
                    length = math.hypot(
                        points[segment_index + 1][0] - points[segment_index][0],
                        points[segment_index + 1][1] - points[segment_index][1],
                    )
                    max_segment_ratio = max(max_segment_ratio, length / max(1e-4, limit))

        current_joint_bends = [tuple(leg.joint_bends) for leg in creature.legs]
        for bends in current_joint_bends:
            if bends:
                max_joint_bend_range = max(max_joint_bend_range, max(bends) - min(bends))
        if previous_joint_bends is not None:
            for before, after in zip(previous_joint_bends, current_joint_bends):
                deltas = [abs(current - prior) for prior, current in zip(before, after)]
                if deltas:
                    max_joint_motion_spread = max(max_joint_motion_spread, max(deltas) - min(deltas))
        previous_joint_bends = current_joint_bends

    return {
        "creature": creature,
        "starts": normal_starts,
        "outside": outside_starts,
        "max_airborne": max_airborne,
        "planted_displacements": planted_displacements,
        "max_chain_stretch": max_chain_stretch,
        "max_segment_ratio": max_segment_ratio,
        "max_pose_jump": max_pose_jump,
        "max_heading_jump": max_heading_jump,
        "min_supports": min_supports,
        "max_joint_bend_range": max_joint_bend_range,
        "max_joint_motion_spread": max_joint_motion_spread,
    }
