"""Where a leg attaches, where its foot wants to be, and held-leg springs.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import Tuple

from ...support.math_utils import (
    clamp,
)

from .legstate import LegState


class LegPlacementMixin:
    """Where a leg attaches, where its foot wants to be, and held-leg springs."""

    def _visual_foot_for_render(self, leg: LegState) -> Tuple[float, float]:
        """Return the visible foot location without making planted feet slide with the body.

        Normal planted feet stay in world space so the spider walks over them.  If a sharp
        turn, drag, or throw has made a foot visually impossible, only the rendered
        position is eased toward a safe body-local lane until the next corrective step
        replants it.  This prevents rubber-band legs without breaking gait urgency.

        The lively gait pulls misplaced feet fully into their sector (no crossed,
        pinwheel legs while turning) and reach-limits every foot (no front legs
        stretching off into the distance).  The classic gait is unchanged.
        """
        lively = self._uses_lively_gait()
        spider_gait = self._spider_gait_config()
        if spider_gait is not None:
            # A planted tarsus is a world-space contact point.  Never project it
            # back into a body-relative sector during rendering: the sector moves
            # with the body and made planted feet slide visibly while walking.
            # The scheduler now replants before this becomes a long limb.  Active
            # swings may still use a safety envelope because they are not contacts.
            if (
                not leg.stepping
                and not leg.pending_step
                and self.held_release_timer <= 0.0
            ):
                return leg.foot_x, leg.foot_y
            return self._limit_world_point_to_leg_reach(leg, leg.foot_x, leg.foot_y, visual=True)
        if lively:
            # Hard guarantee for the lively gait: pull the rendered foot into the
            # leg's own angular wedge (no crossing) and cap the leg length tightly
            # from the coxa (no front-leg overstretch).  For a well-placed foot
            # this is a no-op; it only corrects feet that drifted during a turn.
            vx, vy = self._clamp_to_sector(leg, leg.foot_x, leg.foot_y)
            ax, ay = self._leg_attach(leg)
            d = leg.definition
            cap = min(float(d.get("reach", 1.8)) * 1.02,
                      (float(d.get("upper_len", 0.85)) + float(d.get("lower_len", 1.05))) * 1.05) * self.size
            ddx, ddy = vx - ax, vy - ay
            dl = math.hypot(ddx, ddy)
            if dl > cap and dl > 1e-5:
                scale = cap / dl
                vx, vy = ax + ddx * scale, ay + ddy * scale
            return vx, vy
        safe_x, safe_y = self._constrain_leg_point(leg, leg.foot_x, leg.foot_y)
        safe_x, safe_y = self._limit_world_point_to_leg_reach(leg, safe_x, safe_y, visual=True)
        local_f, local_s, min_side, rest_f, wrong_side, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
        _, _, too_far, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=True)
        if severe_wrong or very_far:
            blend = 0.88
        elif too_far:
            blend = 0.58
        elif wrong_side:
            blend = 0.42
        else:
            return leg.foot_x, leg.foot_y
        vx = leg.foot_x + (safe_x - leg.foot_x) * blend
        vy = leg.foot_y + (safe_y - leg.foot_y) * blend
        return self._limit_world_point_to_leg_reach(leg, vx, vy, visual=True)

    def _leg_attach(self, leg: LegState) -> Tuple[float, float]:
        d = leg.definition
        fx, fy, rx, ry = self._basis()
        sign = self._side_sign(d.get("side", "right"))
        if "attach_forward" in d or "attach_side" in d:
            af = float(d.get("attach_forward", 0.0)) * self.size
            a_side = float(d.get("attach_side", 0.30)) * self.size * sign
            # A small body sway prevents perfectly rigid hinge points while keeping feet planted.
            sway = self.body_sway * sign * 0.35
            return self.x + fx * af + rx * (a_side + sway), self.y + fy * af + ry * (a_side + sway)
        ang = self.heading + math.radians(float(d.get("attach_angle", 0.0)))
        radius = self.size * 0.25
        return self.x + math.cos(ang) * radius, self.y + math.sin(ang) * radius

    def _leg_ideal_foot(self, leg: LegState) -> Tuple[float, float]:
        d = leg.definition
        fx, fy, rx, ry = self._basis()
        sign = self._side_sign(d.get("side", "right"))
        if "rest_forward" in d or "rest_side" in d:
            rf = float(d.get("rest_forward", 0.0)) * self.size
            rs = float(d.get("rest_side", 1.0)) * self.size * sign
            stride = float(d.get("stride_forward", 0.34)) * self.size
            jitter = float(d.get("rest_jitter", 0.0)) * self.size
            speed01 = clamp(self.current_speed / 145.0, 0.0, 1.0)
            spider_gait = self._spider_gait_config()
            if spider_gait is not None and self._uses_lively_gait():
                stride *= spider_gait["stride_gain"]
            # Feet anticipate the next body position, especially during a scurry.
            # Normal personalities plant ahead of the body-facing direction. Observer
            # is special: it keeps its head aimed at its focus while the body backs
            # or scurries away, so next footfalls need to follow the actual travel vector.
            if self.state == "Observe" and self._acts_as_observer() and self.current_speed > 2.0:
                inv_speed = 1.0 / max(1e-4, math.hypot(self.vel_x, self.vel_y))
                move_f = (self.vel_x * fx + self.vel_y * fy) * inv_speed
                move_s = (self.vel_x * rx + self.vel_y * ry) * inv_speed
                lateral_mult = float(self.personality.get("observe_lateral_stride_mult", 1.25))
                forward_mult = float(self.personality.get("observe_forward_stride_mult", 0.28))
                rf += stride * speed01 * move_f * forward_mult
                rs += stride * speed01 * move_s * lateral_mult
            else:
                rf += stride * speed01 * (1.05 if self.state not in ("Chase", "Retreat", "Dragged", "Startled") else 1.35)
            if spider_gait is not None and self._uses_lively_gait():
                # Leading legs reach a little farther ahead while rear legs
                # retain a shorter stroke.  The side lane stays fixed, so a
                # forward walk cannot turn into a lateral crab shuffle.
                front_factor = clamp(float(d.get("rest_forward", 0.0)), -1.0, 1.0)
                rf += stride * speed01 * front_factor * spider_gait["front_stride_bias"]
            # Subtle organic toe spread while idle; this does not slide planted feet.
            micro_x = math.sin(self.breath_phase * 0.9 + leg.phase_seed) * jitter
            micro_y = math.cos(self.breath_phase * 0.7 + leg.phase_seed * 1.13) * jitter
            wx = self.x + fx * rf + rx * rs + micro_x
            wy = self.y + fy * rf + ry * rs + micro_y
            if self._uses_lively_gait():
                wx, wy = self._clamp_to_sector(leg, wx, wy)
            return self._constrain_leg_point(leg, wx, wy)
        angle = self.heading + math.radians(float(d.get("rest_angle", 0.0)))
        reach = float(d.get("reach", 1.8)) * self.size
        wx = self.x + math.cos(angle) * reach
        wy = self.y + math.sin(angle) * reach
        if self._uses_lively_gait():
            wx, wy = self._clamp_to_sector(leg, wx, wy)
        return self._constrain_leg_point(leg, wx, wy)

    def _initialize_legs(self) -> None:
        for leg in self.legs:
            fx, fy = self._leg_ideal_foot(leg)
            leg.foot_x, leg.foot_y = self._constrain_leg_point(leg, fx + self.rng.uniform(-2.0, 2.0), fy + self.rng.uniform(-2.0, 2.0))
            leg.step_target_x = leg.foot_x
            leg.step_target_y = leg.foot_y
            leg.twitch_clock = self.rng.uniform(0.0, 1.0)
            try:
                leg.joint_phase = float(leg.definition.get("phase_offset", 0.0)) * math.tau + leg.phase_seed * 0.11
            except (TypeError, ValueError):
                leg.joint_phase = leg.phase_seed * 0.11
            leg.joint_bends = []
            self._spider_reanchor_stance(leg)

    def _translate_leg_world_points(self, dx: float, dy: float, factor: float = 1.0) -> None:
        """Move existing footfall arcs when the body is externally displaced.

        Planted spider feet should resist a little instead of being glued to the
        body.  Moving them by only part of the body displacement creates a
        believable scrabble/stretch while preventing impossible crossed legs.
        """
        if abs(dx) < 1e-5 and abs(dy) < 1e-5:
            return
        tx = dx * clamp(factor, 0.0, 1.0)
        ty = dy * clamp(factor, 0.0, 1.0)
        for leg in self.legs:
            leg.foot_x += tx
            leg.foot_y += ty
            leg.step_start_x += tx
            leg.step_start_y += ty
            leg.step_target_x += tx
            leg.step_target_y += ty
            leg.step_control_x += tx
            leg.step_control_y += ty
            leg.pending_target_x += tx
            leg.pending_target_y += ty
            # External displacement can leave planted feet far behind the body.
            # Keep some scrabble/stretch, but never let the visible leg become elastic.
            self._soft_limit_leg_state(leg, blend=0.55 if factor >= 0.50 else 0.78)

    def _update_held_leg_springs(self, dt: float) -> None:
        """Drive a damped, per-leg carry response from hand motion.

        A held spider has no ground contact, so its feet must not be advanced by
        the walking gait. They should still react to the acceleration of the
        hand: proximal mass lags, then the relaxed chain settles with a small
        phase difference from its neighbors. The spring offsets are applied only
        to the rendered carried pose; stored ground contacts remain untouched.
        """
        chain_config = self._sprite_leg_chain_config() or {}
        try:
            bounce = clamp(float(chain_config.get("carry_bounce", 0.18)), 0.04, 0.36)
            stiffness = clamp(float(chain_config.get("carry_spring", 1.0)), 0.65, 1.50)
        except (TypeError, ValueError):
            bounce, stiffness = 0.18, 1.0
        speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0)
        response = clamp(float(getattr(self, "held_drag_response", 0.0)), 0.0, 1.0)
        accel_scale = max(1.0, self.size * 18.0)
        accel_x = clamp(float(getattr(self, "held_drag_accel_x", 0.0)) / accel_scale, -1.0, 1.0)
        accel_y = clamp(float(getattr(self, "held_drag_accel_y", 0.0)) / accel_scale, -1.0, 1.0)
        acceleration_level = clamp(math.hypot(accel_x, accel_y), 0.0, 1.0)
        motion_level = clamp(max(response, acceleration_level * 0.62), 0.0, 1.0)
        held_clock = float(getattr(self, "held_pose_clock", 0.0))

        for index, leg in enumerate(self.legs):
            phase = float(getattr(leg, "phase_seed", 0.0))
            # Each foot has its own delayed wave. The acceleration term creates
            # a genuine hand-following lag; the smaller wave prevents eight
            # legs from moving as one rigid fan while the hand is in motion.
            wave_phase = held_clock * (3.0 + speed01 * 1.5) + phase * 0.42 + index * 0.57
            wave_x = math.sin(wave_phase) * bounce * (0.16 + response * 0.24)
            wave_y = math.cos(wave_phase * 0.86 + 0.7) * bounce * (0.20 + response * 0.32)
            target_x = self.size * (
                -accel_x * (0.10 + response * 0.20)
                + wave_x * motion_level
            )
            target_y = self.size * (
                -accel_y * (0.08 + response * 0.16)
                + wave_y * motion_level
            )
            target_x = clamp(target_x, -self.size * 0.30, self.size * 0.30)
            target_y = clamp(target_y, -self.size * 0.24, self.size * 0.24)

            spring_k = (38.0 + speed01 * 16.0) * stiffness
            damping = 5.8 + response * 1.8
            leg.held_spring_vx += (target_x - leg.held_spring_x) * spring_k * dt
            leg.held_spring_vy += (target_y - leg.held_spring_y) * spring_k * dt
            leg.held_spring_vx *= math.exp(-damping * dt)
            leg.held_spring_vy *= math.exp(-damping * dt)
            leg.held_spring_x += leg.held_spring_vx * dt
            leg.held_spring_y += leg.held_spring_vy * dt
            leg.held_spring_x = clamp(leg.held_spring_x, -self.size * 0.30, self.size * 0.30)
            leg.held_spring_y = clamp(leg.held_spring_y, -self.size * 0.25, self.size * 0.25)

    @staticmethod
    def _point_segment_distance(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
        abx = bx - ax
        aby = by - ay
        denom = abx * abx + aby * aby
        if denom <= 1e-6:
            return math.hypot(px - ax, py - ay)
        t = clamp(((px - ax) * abx + (py - ay) * aby) / denom, 0.0, 1.0)
        cx = ax + abx * t
        cy = ay + aby * t
        return math.hypot(px - cx, py - cy)

    def _panic_rehome_legs(self, intensity: float = 1.0) -> None:
        intensity = clamp(intensity, 0.0, 1.0)
        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step:
                continue
            ix, iy = self._leg_ideal_foot(leg)
            dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy)
            candidates.append((dist + self.rng.random() * self.size * 0.12, i, leg, ix, iy))
        candidates.sort(reverse=True, key=lambda item: item[0])
        # Start a few quick, staggered corrective steps instead of snapping every foot.
        for rank, (_, _, leg, ix, iy) in enumerate(candidates[: max(2, int(2 + intensity * 3))]):
            delay = rank * self.rng.uniform(0.012, 0.035)
            side = self._side_sign(leg.definition.get("side", "right"))
            ix += self.rng.uniform(-0.10, 0.10) * self.size
            iy += side * self.rng.uniform(0.02, 0.15) * self.size * intensity
            self._schedule_step(leg, ix, iy, delay=delay, force_fast=True)
            leg.step_cooldown = self.rng.uniform(0.012, 0.040)

