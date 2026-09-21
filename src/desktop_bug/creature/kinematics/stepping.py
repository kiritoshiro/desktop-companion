"""Scheduling, starting and advancing a footstep.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math

from ...support.math_utils import (
    clamp,
)
from ..constants import (
    smootherstep,
)

from .legstate import LegState


class SteppingMixin:
    """Scheduling, starting and advancing a footstep."""

    def _schedule_step(self, leg: LegState, target_x: float, target_y: float, delay: float = 0.0, force_fast: bool = False) -> None:
        if leg.stepping or leg.pending_step:
            return
        leg.pending_step = True
        leg.pending_delay = max(0.0, delay)
        leg.pending_target_x, leg.pending_target_y = self._constrain_leg_point(leg, target_x, target_y)
        if delay <= 0.001:
            self._begin_step(leg, force_fast=force_fast)

    def _begin_step(self, leg: LegState, force_fast: bool = False) -> None:
        leg.pending_step = False
        leg.stepping = True
        leg.contact_state = "swing"
        leg.support_weight = 0.0
        leg.contact_age = 0.0
        leg.stroke_progress = 0.0
        leg.step_timer = 0.0
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        spider_gait = self._spider_gait_config()
        if spider_gait is not None and self._uses_lively_gait():
            leg.step_duration = self._spider_step_duration(
                spider_gait,
                clamp(self.current_speed / 150.0, 0.0, 1.0),
                float(self._spider_turn_speed),
                force_fast=force_fast,
            )
        else:
            leg.step_duration = 0.30 - 0.13 * speed01
        sliding = self._is_drift_sliding()
        if spider_gait is None and (force_fast or (self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun") and not sliding) or (abs(getattr(self, "last_drift_amount", 0.0)) > 0.22 and not sliding)):
            leg.step_duration *= 0.72
        if spider_gait is None and self._uses_skitter_gait() and not sliding:
            # The third movement option is based on lively, but its tarsi flick
            # forward much faster so each burst reads as several quick steps.
            leg.step_duration *= 0.58 if force_fast else 0.66
        if spider_gait is None and sliding:
            # During the visible slide, feet should look light and skiddy rather
            # than gripping hard and machine-gunning new steps.
            leg.step_duration *= float(self.personality.get("drift_slide_step_slowdown", 1.42))
        if spider_gait is None:
            min_step = 0.052 if self._uses_skitter_gait() and not sliding else 0.095
            max_step = 0.26 if self._uses_skitter_gait() and not sliding else 0.40
            leg.step_duration = clamp(leg.step_duration, min_step, max_step)
        _, _, _, _, _, start_severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
        if start_severe_wrong:
            # If a planted foot crossed to the wrong side after a sharp turn, start
            # from the nearest safe lane.  Over-reach alone is repaired by a visible
            # swing, not by snapping the rear leg into place.
            leg.foot_x, leg.foot_y = self._constrain_leg_point(leg, leg.foot_x, leg.foot_y)
        leg.step_start_x = leg.foot_x
        leg.step_start_y = leg.foot_y
        # Do not stamp every foot onto an identical metronomic target.  Real spider
        # tarsi land with small leg-by-leg variation, even when the overall gait is
        # close to an alternating tetrapod.
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        target_f, target_s = self._world_to_body_local(leg.pending_target_x, leg.pending_target_y)
        side = self._side_sign(leg.definition.get("side", "right"))
        if spider_gait is not None and self._uses_lively_gait():
            # Predictable landing is part of the gait.  Keep only a tiny
            # deterministic per-leg offset so the eight feet do not stamp onto
            # a mathematically identical line.
            target_f += math.sin(leg.phase_seed) * 0.018 * self.size
            target_s += side * math.cos(leg.phase_seed * 1.37) * 0.012 * self.size
        else:
            target_f += self.rng.uniform(-0.060, 0.095) * self.size * (0.7 + speed01)
            target_s += side * self.rng.uniform(-0.040, 0.055) * self.size * (0.6 + speed01 * 0.5)
        tx, ty = self._body_local_to_world(target_f, target_s)
        leg.step_target_x, leg.step_target_y = self._constrain_leg_point(leg, tx, ty)

        # Quadratic Bezier control point: forward/sideways arcing swing, not straight interpolation.
        mid_x = (leg.step_start_x + leg.step_target_x) * 0.5
        mid_y = (leg.step_start_y + leg.step_target_y) * 0.5
        path_x = leg.step_target_x - leg.step_start_x
        path_y = leg.step_target_y - leg.step_start_y
        path_len = max(1.0, math.hypot(path_x, path_y))
        spider_gait = self._spider_gait_config()
        if spider_gait is not None and self._uses_lively_gait():
            # Spider tarsi lift mostly forward and upward, with only a small
            # outward clearance arc.  The previous normal-to-path arc made
            # every swing fan sideways, which read as a crab walk.
            fx, fy, rx, ry = self._basis()
            side = self._side_sign(leg.definition.get("side", "right"))
            front_factor = clamp(float(leg.definition.get("rest_forward", 0.0)), -1.0, 1.0)
            forward_arc = path_len * spider_gait["swing_arc_forward"] * (0.82 + max(0.0, front_factor) * 0.18)
            outward_arc = path_len * spider_gait["swing_arc_outward"]
            leg.step_control_x = mid_x + fx * forward_arc + rx * side * outward_arc
            leg.step_control_y = mid_y + fy * forward_arc + ry * side * outward_arc
            return
        nx = -path_y / path_len
        ny = path_x / path_len
        _, _, rx, ry = self._basis()
        side = self._side_sign(leg.definition.get("side", "right"))
        out_x, out_y = rx * side, ry * side
        # Choose the outward side for the swing arc.
        if nx * out_x + ny * out_y < 0.0:
            nx, ny = -nx, -ny
        arc = clamp(path_len * self.rng.uniform(0.22, 0.36), self.size * 0.12, self.size * 0.56)
        if self._uses_skitter_gait():
            arc *= 0.78
        leg.step_control_x = mid_x + nx * arc
        leg.step_control_y = mid_y + ny * arc

    def _advance_active_steps(self, dt: float) -> None:
        """Advance any in-flight foot swings and per-leg cooldowns.

        Shared by both gait styles: it integrates the quadratic-Bezier swing
        arc, drives ``leg.lift`` across the swing, and plants the foot at the end.
        """
        for leg in self.legs:
            leg.step_cooldown = max(0.0, leg.step_cooldown - dt)

            if leg.pending_step:
                leg.pending_delay -= dt
                if leg.pending_delay <= 0.0:
                    sliding = self._is_drift_sliding()
                    self._begin_step(leg, force_fast=(self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun") or abs(getattr(self, "last_drift_amount", 0.0)) > 0.22) and not sliding)

            if leg.stepping:
                leg.step_timer += dt
                t = clamp(leg.step_timer / max(0.001, leg.step_duration), 0.0, 1.0)
                s = smootherstep(t)
                # Quadratic Bezier foot placement with an ease-in/ease-out time base.
                a_x = leg.step_start_x + (leg.step_control_x - leg.step_start_x) * s
                a_y = leg.step_start_y + (leg.step_control_y - leg.step_start_y) * s
                b_x = leg.step_control_x + (leg.step_target_x - leg.step_control_x) * s
                b_y = leg.step_control_y + (leg.step_target_y - leg.step_control_y) * s
                # During swing, keep the lifted tarsus on its world-space arc.  Planted
                # feet remain planted afterwards, which prevents moonwalking.
                leg.foot_x = a_x + (b_x - a_x) * s
                leg.foot_y = a_y + (b_y - a_y) * s
                leg.lift = math.sin(math.pi * t) ** 0.85
                if t >= 1.0:
                    leg.stepping = False
                    leg.lift = 0.0
                    leg.foot_x = leg.step_target_x
                    leg.foot_y = leg.step_target_y
                    if self._spider_gait_config() is not None:
                        # Touchdown is a new world-space contact.  Give it a
                        # small transfer weight so the rigid support solve does
                        # not jump when the support set changes.
                        local_f, local_s = self._world_to_body_local(leg.foot_x, leg.foot_y)
                        leg.contact_state = "stance"
                        leg.support_weight = 0.12
                        leg.contact_age = 0.0
                        leg.stroke_progress = 0.0
                        leg.stance_base_f = local_f
                        leg.stance_base_s = local_s
                        leg.stance_stroke_f = 0.0
                        leg.stance_stroke_s = 0.0
                        leg.stance_turn = 0.0
                    speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
                    spider_gait = self._spider_gait_config()
                    if spider_gait is not None and self._uses_lively_gait():
                        # Refractory time is derived from the same shared gait
                        # clock as the swing itself; it cannot desynchronise the
                        # two tetrapod windows.
                        leg.step_cooldown = leg.step_duration * spider_gait["refractory_fraction"]
                    elif self._uses_skitter_gait():
                        leg.step_cooldown = self.rng.uniform(0.012, 0.045) * (1.08 - speed01 * 0.36)
                    else:
                        leg.step_cooldown = self.rng.uniform(0.035, 0.095) * (1.15 - speed01 * 0.45)

    def _update_legs(self, dt: float) -> None:
        """Update spider legs using leg-level coordination instead of locked groups.

        Real spiders often approximate an alternating tetrapod gait, but individual
        legs are not mechanically welded into two perfect four-leg teams.  Opposite
        and neighboring legs tend toward antiphase while each leg still has its own
        phase drift, urgency threshold, and footfall target.
        """
        if self.dragging:
            # Dragging is a suspended pose, not a gait state. Keep the leg chain
            # and its joints still until release_drag asks the ground controller
            # to rehome the feet.
            return
        if self.state == "Roll":
            # The roll owns the whole rendered pose. Do not advance a walking
            # swing underneath the tuck transform.
            return
        if self._uses_lively_gait():
            self._update_legs_lively(dt)
            return
        if self.airborne:
            # Feet are tucked in flight (handled by the renderer) and carried with
            # the body; the gait scheduler resumes after landing.
            return
        self._advance_active_steps(dt)

        moving = self.current_speed > 4.0
        # Once the spider has actually stopped, leave its feet planted exactly where
        # they landed instead of tidying them back toward a rest pose or fidgeting in
        # place. This keeps a hunter dead still while it watches its prey and stops
        # every idle spider from endlessly shuffling its legs. Genuinely impossible
        # poses are still repaired by the emergency pass below.
        hold_still = self.current_speed < 3.0 and not self.airborne
        threshold = self.size * (0.36 if moving else 0.21)
        drifting_fast = abs(getattr(self, "last_drift_amount", 0.0)) > 0.18
        sliding = self._is_drift_sliding()
        if sliding:
            # Sliding feet have less grip, so they tolerate more foot drift before
            # replanting.  This removes the sticky, over-turning look.
            threshold *= float(self.personality.get("drift_slide_rehome_slop", 1.58))
        elif self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun") or drifting_fast:
            threshold *= 0.72
        if self.state == "Observe" and self._acts_as_observer():
            # Side-stepping makes feet drift out of their comfortable lane sooner
            # than forward walking, so replant a little earlier and more often.
            threshold *= 0.78

        # Keep enough tarsi on the ground, but avoid the robotic look of four legs
        # lifting at exactly once.  Faster movement allows more simultaneous swings.
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        max_swinging = 1 if not moving else (2 if speed01 < 0.55 else 3)
        if sliding:
            max_swinging = min(max_swinging, int(self.personality.get("drift_slide_max_swinging", 2)))
        elif self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun") or drifting_fast:
            max_swinging = 4 if self.state in ("Dragged", "DriftRun") or drifting_fast else 3
        if self.state == "Observe" and self._acts_as_observer() and moving:
            max_swinging = max(max_swinging, 2)
        swinging_now = sum(1 for leg in self.legs if leg.stepping or leg.pending_step)

        # Correct impossible poses, but never by lifting every leg at once.  The
        # renderer already eases severely misplaced planted feet into a safe visual
        # lane; the scheduler then repairs the worst offenders over a few frames.
        emergency_rehome = False
        emergency_candidates = []
        for i, leg in enumerate(self.legs):
            local_f, local_s, min_side, rest_f, wrong_side, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, too_far, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            # While stopped, only a genuinely broken pose (a leg crossed to the wrong
            # side, or extreme overstretch) is worth disturbing the planted feet for.
            # Mild drift is left exactly as it is so the spider truly holds still.
            broken = severe_wrong or (very_far if hold_still else too_far)
            if broken:
                emergency_rehome = True
                if not leg.stepping and not leg.pending_step:
                    ix, iy = self._leg_ideal_foot(leg)
                    dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy)
                    if very_far:
                        dist += self.size * 0.65
                    emergency_candidates.append((dist, i, leg, ix, iy))
        emergency_candidates.sort(reverse=True, key=lambda item: item[0])
        emergency_slots = max(0, max_swinging - swinging_now)
        for _, i, leg, ix, iy in emergency_candidates[:emergency_slots]:
            self._schedule_step(leg, ix, iy, delay=0.0, force_fast=True)
            leg.step_cooldown = self.rng.uniform(0.020, 0.055)
            swinging_now += 1

        if hold_still:
            # Stopped: keep the planted feet exactly as they are. The fidget, the
            # rest-pose re-homing and the idle toe twitch below are all skipped.
            return

        if swinging_now >= max_swinging:
            return

        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step or leg.step_cooldown > 0.0:
                continue

            ix, iy = self._leg_ideal_foot(leg)
            dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy)
            local_f, local_s, min_side, rest_f, wrong_side, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, too_far, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            if wrong_side:
                dist = max(dist, threshold + self.size * (0.62 if severe_wrong else 0.34))
            if too_far:
                dist = max(dist, threshold + self.size * (0.74 if very_far else 0.42))
            if self.turn_rehome_pressure > 0.40:
                dist = max(dist, abs(local_f - rest_f) * 0.34 + threshold + self.size * 0.08)

            urgency = dist / max(1.0, threshold)
            if urgency <= 1.0 and not (not moving and self.rng.random() < 0.012):
                continue

            # Bias toward alternating tetrapod timing, but do not enforce it.  Each
            # leg has its own random phase offset, which creates the small gait
            # variation seen in real spiders.
            group = int(leg.definition.get("gait_group", 0))
            preferred_phase = group * math.pi + leg.gait_phase_offset
            phase = (self.bob_phase + preferred_phase) % math.tau
            phase_gate = 0.55 + 0.45 * ((math.cos(phase) + 1.0) * 0.5)
            urgency *= phase_gate

            # Opposite and adjacent legs tend to be antiphase.  Penalize scheduling
            # a leg while its neighbor/opposite is already in swing, but allow it
            # when urgently out of place.  This is what removes the fixed team look.
            for j, other in enumerate(self.legs):
                if not (other.stepping or other.pending_step):
                    continue
                same_segment_opposite = abs(i - j) == 1 and min(i, j) % 2 == 0
                same_side_adjacent = abs(i - j) == 2
                same_group = int(other.definition.get("gait_group", 0)) == group
                if same_segment_opposite:
                    urgency *= 0.58
                elif same_side_adjacent:
                    urgency *= 0.62
                elif same_group:
                    urgency *= 0.82

            # Slightly favor the most stale planted foot, so pairs do not remain
            # frozen just because another leg in the old group moved recently.
            urgency += self.rng.uniform(-0.10, 0.18)
            if urgency > 1.0:
                candidates.append((urgency, i, leg, ix, iy))

        candidates.sort(reverse=True, key=lambda item: item[0])
        slots = max(0, max_swinging - swinging_now)
        started = False
        used_indices = set()
        scheduled_count = 0
        for _, i, leg, ix, iy in candidates:
            if scheduled_count >= slots:
                break
            # Avoid choosing immediate opposite/adjacent legs in the same frame unless
            # the spider is correcting an impossible pose.
            if not emergency_rehome and any(abs(i - j) in (1, 2) for j in used_indices):
                continue
            side_sign = self._side_sign(leg.definition.get("side", "right"))
            turn_bias = ((self.target_heading - self.heading + math.pi) % math.tau) - math.pi
            turn_bias = clamp(turn_bias, -1.0, 1.0)
            # Tiny turn variation only.  Do not push legs across the body/facing
            # direction; that was the source of the wrong-way rotation look.
            local_ix, local_iy = self._world_to_body_local(ix, iy)
            local_ix += side_sign * turn_bias * self.size * 0.035
            ix, iy = self._body_local_to_world(local_ix, local_iy)
            fast_footwork = (self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun") or drifting_fast) and not sliding
            delay = self.rng.uniform(0.030, 0.120) if sliding else self.rng.uniform(0.000, 0.040 if fast_footwork else 0.070)
            self._schedule_step(leg, ix, iy, delay=delay, force_fast=fast_footwork)
            leg.step_cooldown = self.rng.uniform(0.070, 0.160) if sliding else self.rng.uniform(0.025, 0.070)
            used_indices.add(i)
            scheduled_count += 1
            started = True

        self.idle_twitch_timer -= dt
        if not started and self.current_speed < 2.0 and self.idle_twitch_timer <= 0.0:
            idle_candidates = [leg for leg in self.legs if not leg.stepping and not leg.pending_step]
            if idle_candidates:
                leg = self.rng.choice(idle_candidates)
                ix, iy = self._leg_ideal_foot(leg)
                # Tiny independent toe probe, not a whole-group twitch.
                self._schedule_step(leg, ix + self.rng.uniform(-3.2, 3.2), iy + self.rng.uniform(-3.2, 3.2), force_fast=False)
                started = True
            self.idle_twitch_timer = self.rng.uniform(0.75, 2.0)

        if started:
            self.turn_rehome_pressure = max(0.0, self.turn_rehome_pressure - 0.18)

