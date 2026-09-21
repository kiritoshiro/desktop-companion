"""The eight-legged gait cycle: groups, phase windows, timing.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import Tuple

from ...support.math_utils import (
    clamp,
)

from .legstate import LegState


class SpiderGaitMixin:
    """The eight-legged gait cycle: groups, phase windows, timing."""

    def _spider_group_index(self, leg: LegState) -> int:
        """Map a model gait group to one of the two tetrapod phases."""
        groups = list(getattr(self, "gait_groups", (0, 1)))
        try:
            group = int(leg.definition.get("gait_group", 0))
        except (TypeError, ValueError):
            group = 0
        if len(groups) < 2:
            return 0
        try:
            return groups.index(group) % 2
        except ValueError:
            return group % 2

    def _spider_cycle_hz(self, config: dict, speed01: float, turn_speed: float = 0.0) -> float:
        """Return the one cadence used by both phase windows and foot swings."""
        hz = config["cycle_hz"] + speed01 * config["speed_cycle_gain"]
        if turn_speed > 0.30:
            hz += min(turn_speed, 5.5) * config.get("turn_cycle_gain", 0.10)
        return clamp(hz, 1.20, 4.50)

    def _spider_step_duration(self, config: dict, speed01: float,
                              turn_speed: float = 0.0, force_fast: bool = False) -> float:
        hz = self._spider_cycle_hz(config, speed01, turn_speed)
        duration = config["swing_fraction"] / max(0.1, hz)
        if force_fast:
            duration *= 0.86
        return clamp(duration, 0.075, 0.18)

    def _spider_phase_window(self, leg: LegState, phase: float, config: dict):
        """Return whether a leg is in its shared group launch window."""
        group = self._spider_group_index(leg)
        front = self._leg_front_factor(leg)
        # Front legs lead slightly; rear legs follow slightly.  All four legs
        # still use the same group clock instead of independent random phases.
        if config.get("profile") in ("chosen_one", "tarantula"):
            # These models use a metachronal wave around the body rather than
            # releasing four same-phase legs at once.  The model owns each
            # foot's phase so its front-to-rear wave remains stable while the
            # support solver changes heading.
            try:
                phase_offset = float(leg.definition.get("phase_offset", 0.5 * group))
            except (TypeError, ValueError):
                phase_offset = 0.5 * group
            start = phase_offset - front * config["front_phase_offset"]
        else:
            start = (0.5 * group) - front * config["front_phase_offset"]
        relative = (phase - start) % 1.0
        return relative < config["swing_fraction"], relative, group

    def _spider_predicted_target(self, leg: LegState, config: dict,
                                 turn_speed: float = 0.0) -> Tuple[float, float]:
        """Predict where the foot should land after its bounded swing."""
        speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0)
        duration = self._spider_step_duration(config, speed01, turn_speed)
        target_x, target_y = self._leg_ideal_foot(leg)
        vx, vy = self.vel_x, self.vel_y
        velocity = math.hypot(vx, vy)
        if velocity < 1.0 and self.current_speed > 1.0:
            fx, fy, _, _ = self._basis()
            vx, vy = fx * self.current_speed, fy * self.current_speed
            velocity = self.current_speed
        if velocity > 1.0:
            lookahead = velocity * duration * config["step_lookahead"]
            target_x += vx / velocity * lookahead
            target_y += vy / velocity * lookahead
        return self._constrain_leg_point(leg, target_x, target_y)

    def _update_spider_gait_step(self, dt: float, config: dict) -> None:
        """Run the grounded alternating-tetrapod scheduler for Snowpuff-2."""
        if self.airborne:
            return
        self._advance_active_steps(dt)

        dheading = ((self.heading - self._prev_heading_gait + math.pi) % math.tau) - math.pi
        self._prev_heading_gait = self.heading
        turn_speed = abs(dheading) / max(dt, 1e-3)
        self._spider_turn_speed = turn_speed
        turn_err = abs(((self.target_heading - self.heading + math.pi) % math.tau) - math.pi)
        moving = self.current_speed > 4.0
        turning = turn_speed > 0.30 or turn_err > 0.12
        # When the body is temporarily held by its support envelope, use the
        # outstanding turn request to keep foot handoffs moving.  Without this,
        # a stalled turn reports zero actual turn speed and falls back to the
        # slow walking cadence precisely when a quicker replant is needed.
        turn_drive = max(
            turn_speed,
            turn_err * config.get("turn_error_drive", 1.25),
        ) if turning else 0.0
        speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0)
        fast_state = self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun")
        max_air = config["max_airborne"]
        swinging_now = sum(1 for leg in self.legs if leg.stepping or leg.pending_step)

        # Advance the same clock that defines the swing duration before choosing
        # launches, so this frame's candidates are evaluated in the current window.
        self._lively_gait_phase = (
            self._lively_gait_phase + dt * self._spider_cycle_hz(config, speed01, turn_drive)
        ) % 1.0

        # Only severe wrong-side/overreach poses bypass the phase window.  Normal
        # foot-placement error is scheduled in its tetrapod's window.
        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step or leg.step_cooldown > 0.0:
                continue
            local_f0, local_s0, min_side0, _, _, severe_wrong = self._leg_alignment_metrics(
                leg, leg.foot_x, leg.foot_y
            )
            _, _, _, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            # A support stroke that is nearly full is a real mechanical limit,
            # even if the foot is still inside the loose reach envelope. Free
            # that leg before the solver stalls the whole body waiting for a
            # narrow phase window; this is the long retract/attract stroke that
            # makes spider locomotion efficient instead of tip-tapping in place.
            stroke_pressure = leg.stroke_progress >= 0.80
            turn_pressure = abs(leg.stance_turn) >= (
                config["support_turn_limit"] * config.get("turn_step_pressure", 0.62)
            )
            side_sign0 = self._side_sign(leg.definition.get("side", "right"))
            side_magnitude0 = side_sign0 * local_s0
            # Release a contact before a sharp turn drives it into the body's
            # centre. One deliberate replant is cheaper and more stable than
            # several failed support fits followed by rapid tap corrections.
            turn_side_pressure = (
                turning
                and side_magnitude0 < max(self.size * 0.10, min_side0 * 0.72)
                and abs(leg.stance_turn) > config["support_turn_limit"] * 0.28
            )
            emergency = severe_wrong or very_far or stroke_pressure or turn_pressure or turn_side_pressure
            target = self._spider_predicted_target(leg, config, turn_drive)
            distance_to_target = math.hypot(leg.foot_x - target[0], leg.foot_y - target[1])
            # During a turn, most of the apparent target error is the expected
            # body-relative sweep of a fixed tarsus.  Discount that component so
            # stance legs can use their contraction/extension range before a
            # true swing is requested.
            turn_allowance = self._spider_turn_rehome_allowance(leg, config) if turning else 0.0
            step_distance = max(0.0, distance_to_target - turn_allowance)
            threshold = self.size * max(config["step_trigger"], config["stance_deadband"])
            if not moving and not turning:
                threshold *= 0.78
            if fast_state:
                threshold *= 0.82
            if turning:
                threshold *= 1.08
            _, relative, group = self._spider_phase_window(leg, self._lively_gait_phase, config)
            in_window = relative < config["swing_fraction"]
            local_f, local_s, _, _, wrong_side, _ = self._leg_alignment_metrics(
                leg, leg.foot_x, leg.foot_y
            )
            _, _, too_far, _ = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            # The neutral side lane is intentionally relaxed while the body is
            # turning.  A non-severe inward sweep is the expected result of a
            # planted foot rotating with the body, not a crossed leg.  Severe
            # wrong-side and reach violations remain emergency step triggers.
            turn_side_safe = (
                turning
                and not too_far
                and self._side_sign(leg.definition.get("side", "right")) * local_s >= self.size * 0.07
                and abs(leg.stance_turn) < config["support_turn_limit"] * 0.98
            )
            if turn_side_safe:
                # A fixed contact can also appear forward/backward of its
                # neutral lane as the body rotates.  Keep it usable until it
                # is genuinely close to crossing or overstretching.
                wrong_side = False
                if abs(local_f - float(leg.definition.get("rest_forward", 0.0)) * self.size) < self.size * 1.40:
                    severe_wrong = False
            if (
                turning
                and wrong_side
                and not severe_wrong
                and not too_far
                and abs(leg.stance_turn) < config["support_turn_limit"] * 0.98
            ):
                wrong_side = False
            needs_step = step_distance > threshold or wrong_side or too_far
            if not emergency and (not needs_step or not in_window):
                continue
            score = step_distance / max(1.0, threshold)
            if emergency:
                score += 3.0
            score += max(0.0, config["swing_fraction"] - relative) * 0.35
            candidates.append((score, i, leg, target, emergency, group))

        candidates.sort(reverse=True, key=lambda item: item[0])
        slots = max(0, max_air - swinging_now)
        side_airborne = {
            -1.0: sum(1 for leg in self.legs if (leg.stepping or leg.pending_step) and self._side_sign(leg.definition.get("side", "right")) < 0),
            1.0: sum(1 for leg in self.legs if (leg.stepping or leg.pending_step) and self._side_sign(leg.definition.get("side", "right")) > 0),
        }
        used = set()
        for _, i, leg, target, emergency, group in candidates:
            if slots <= 0:
                break
            side = self._side_sign(leg.definition.get("side", "right"))
            # Keep support balanced by anatomical side, not array adjacency.
            if side_airborne[side] >= 2 and not emergency:
                continue
            leg.last_step_phase = self._lively_gait_phase
            leg.last_step_emergency = emergency
            self._schedule_step(leg, target[0], target[1], delay=0.0,
                                force_fast=emergency or fast_state)
            leg.step_cooldown = 0.0
            side_airborne[side] += 1
            used.add(i)
            slots -= 1

        self._update_spider_joint_articulation(dt, config)
        self._update_feelers(dt, moving=moving, turning=turning)
        if used:
            self.turn_rehome_pressure = max(0.0, self.turn_rehome_pressure - 0.18)

    def _update_spider_gait(self, dt: float, config: dict) -> None:
        """Advance the gait scheduler at the same fixed mechanics rate."""
        duration = max(0.0, float(dt))
        substeps = max(1, int(round(duration * 120.0)))
        step = min(1.0 / 120.0, duration)
        for _ in range(substeps):
            self._update_spider_gait_step(step, config)

