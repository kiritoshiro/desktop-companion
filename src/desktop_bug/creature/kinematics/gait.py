"""Gait configuration and grounded-locomotion body solving.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import Tuple

from ...support.math_utils import (
    clamp,
    clamp_point,
)

from ..constants import GAIT_BY_STATE, GAIT_SPELL_GAP, GAIT_SPELL_LENGTH
from .legstate import LegState


class GaitConfigMixin:
    """Gait configuration and grounded-locomotion body solving."""

    def effective_gait_style(self) -> str:
        """The gait this spider is walking with *right now*.

        A pipeline of three stages, most specific last (DC-56, DC-63):

        1. **Temperament** sets the baseline, once, for life (DC-52).
        2. **A spell** may swap it for a few seconds, at random, so walking
           around is not monotonous.
        3. **The activity** overrides both: a curious spider darts, a running
           one runs. What a spider is doing always beats what it fancies.

        `classic` opts out of every stage. It is the original pre-DC-16 gait,
        kept so a preset that names it behaves exactly as its author left it,
        and phasing it would quietly break that promise.
        """
        baseline = getattr(self, "gait_style", "classic")
        if baseline == "classic":
            return baseline
        if getattr(self, "fleeing", False):
            # DC-50's retreat can run under several states; whichever one it
            # is, a frightened spider is running.
            return "lively"
        activity = GAIT_BY_STATE.get(getattr(self, "state", ""))
        if activity is not None:
            return activity
        spell = getattr(self, "gait_spell", None)
        return spell or baseline

    def _update_gait_spell(self, dt: float) -> None:
        """Roll the occasional change of step (DC-63).

        Driven from a dedicated stream (`_gait_rng`), so a replayed run
        walks the same way *and* drawing from it cannot shift the shared
        simulation stream -- see the comment where it is created.
        """
        if getattr(self, "gait_style", "classic") == "classic":
            return
        self.gait_spell_timer -= max(0.0, dt)
        if self.gait_spell_timer > 0.0:
            return
        if self.gait_spell is None:
            other = "skitter" if self.gait_style == "lively" else "lively"
            self.gait_spell = other
            self.gait_spell_timer = self._gait_rng.uniform(*GAIT_SPELL_LENGTH)
        else:
            self.gait_spell = None
            self.gait_spell_timer = self._gait_rng.uniform(*GAIT_SPELL_GAP)

    def _uses_lively_gait(self) -> bool:
        return self.effective_gait_style() in ("lively", "skitter")

    def _uses_skitter_gait(self) -> bool:
        return self.effective_gait_style() == "skitter"

    def _spider_gait_config(self):
        """Return bounded tuning for models that opt into an insect-like gait.

        The existing lively/skitter scheduler is shared by many creatures.  A
        sprite model can opt in to a more grounded spider stride without
        changing the legacy gait of unrelated models.

        Cached, because this is a pure function of the model's appearance block
        and was the single most expensive thing in a frame: measured at 72 calls
        per spider per frame, each rebuilding a thirty-key dictionary through
        thirty `clamp(float(...))` calls. The cache is keyed on the identity of
        the model dict, so swapping a creature's model still rebuilds it.
        """
        cached = self._gait_config_cache
        if cached is not None and cached[0] is self.model:
            return cached[1]
        config = self._build_spider_gait_config()
        self._gait_config_cache = (self.model, config)
        return config

    def _build_spider_gait_config(self):
        raw = self._appearance("spider_gait", None)
        if not isinstance(raw, dict) or not bool(raw.get("enabled", False)):
            return None
        try:
            return {
                "profile": str(raw.get("profile", "tetrapod")).strip().lower(),
                "max_airborne": int(clamp(float(raw.get("max_airborne", 3)), 2, 3)),
                "swing_arc_outward": clamp(float(raw.get("swing_arc_outward", 0.12)), 0.0, 0.30),
                "swing_arc_forward": clamp(float(raw.get("swing_arc_forward", 0.24)), 0.0, 0.50),
                "swing_height": clamp(float(raw.get("swing_height", 0.56)), 0.35, 0.85),
                "front_stride_bias": clamp(float(raw.get("front_stride_bias", 0.16)), 0.0, 0.35),
                "stride_gain": clamp(float(raw.get("stride_gain", 1.0)), 0.75, 1.80),
                "swing_fraction": clamp(float(raw.get("swing_fraction", 0.29)), 0.20, 0.40),
                # One clock drives both the group launch window and the swing
                # duration.  This prevents the old 47 ms window / 122 ms swing
                # mismatch that caused most steps to start outside their phase.
                "cycle_hz": clamp(float(raw.get("cycle_hz", 2.10)), 1.20, 4.50),
                "speed_cycle_gain": clamp(float(raw.get("speed_cycle_gain", 1.45)), 0.0, 3.0),
                "step_lookahead": clamp(float(raw.get("step_lookahead", 0.72)), 0.25, 1.20),
                "step_trigger": clamp(float(raw.get("step_trigger", 0.18)), 0.12, 0.34),
                # Feet may remain planted inside this comfort band even when
                # the default rest pose would be slightly different. Replant
                # only when the stance is leaving that band or a hard support
                # limit is approaching.
                "stance_deadband": clamp(float(raw.get("stance_deadband", 0.36)), 0.18, 0.80),
                "front_phase_offset": clamp(float(raw.get("front_phase_offset", 0.035)), 0.0, 0.10),
                "refractory_fraction": clamp(float(raw.get("refractory_fraction", 0.16)), 0.04, 0.35),
                # Limits for the support-driven body solver.  They are expressed
                # in body-size units so the same controller behaves consistently
                # for differently scaled Snowpuff-2 instances.
                "support_stroke_limit": clamp(float(raw.get("support_stroke_limit", 0.62)), 0.35, 1.10),
                "support_turn_limit": clamp(float(raw.get("support_turn_limit", 0.58)), 0.25, 1.20),
                "support_blend_time": clamp(float(raw.get("support_blend_time", 0.10)), 0.04, 0.30),
                "max_body_turn_rate": clamp(float(raw.get("max_body_turn_rate", 2.60)), 0.80, 8.00),
                "turn_gain": clamp(float(raw.get("turn_gain", 1.0)), 0.80, 3.00),
                # A turn is primarily a stance action: the inside legs shorten
                # while the outside legs lengthen.  This lets the body rotate
                # through its existing foot contacts instead of demanding a
                # fresh swing for every few degrees of heading change.
                "turn_radial_gain": clamp(float(raw.get("turn_radial_gain", 0.34)), 0.0, 0.80),
                "turn_radial_limit": clamp(float(raw.get("turn_radial_limit", 0.28)), 0.08, 0.55),
                # Turning has a separate handoff threshold and cadence boost.
                # A stance should be released before the rigid support fit
                # stalls, rather than waiting for the generic stride trigger.
                "turn_step_pressure": clamp(float(raw.get("turn_step_pressure", 0.62)), 0.45, 0.88),
                "turn_cycle_gain": clamp(float(raw.get("turn_cycle_gain", 0.26)), 0.08, 0.80),
                "turn_error_drive": clamp(float(raw.get("turn_error_drive", 1.25)), 0.40, 2.40),
                "turn_speed_floor": clamp(float(raw.get("turn_speed_floor", 0.15)), 0.10, 0.60),
                # Apply most of a requested pivot per solver step, but leave a
                # small mechanical buffer for support handoffs. This prevents a
                # stalled frame from being followed by a visible angular snap.
                "turn_pose_fraction": clamp(float(raw.get("turn_pose_fraction", 0.78)), 0.55, 1.0),
                # Mouse/target bearing is intentionally filtered separately
                # from the mechanical body turn.  This removes high-frequency
                # target reversals while preserving a quick response to a
                # sustained turn request.
                "heading_smoothing": clamp(float(raw.get("heading_smoothing", 7.0)), 3.0, 18.0),
                "heading_deadband": clamp(float(raw.get("heading_deadband", 0.045)), 0.015, 0.16),
            }
        except (TypeError, ValueError):
            return None

    def _spider_reanchor_stance(self, leg: LegState) -> None:
        """Start a fresh world-space stance for a non-swinging leg."""
        local_f, local_s = self._world_to_body_local(leg.foot_x, leg.foot_y)
        leg.contact_state = "stance"
        leg.support_weight = 1.0
        leg.contact_age = 0.0
        leg.stroke_progress = 0.0
        leg.stance_base_f = local_f
        leg.stance_base_s = local_s
        leg.stance_stroke_f = 0.0
        leg.stance_stroke_s = 0.0
        leg.stance_turn = 0.0

    def _spider_reanchor_contacts(self) -> None:
        """Re-establish contacts after an externally controlled movement mode."""
        # External movement (dragging, throwing, jumping) may have changed the
        # body heading without going through the grounded controller.  Start
        # the next pursuit from that real pose instead of replaying a stale
        # filtered cursor bearing.
        self._spider_heading_filter = self.heading
        for leg in self.legs:
            if leg.stepping:
                leg.contact_state = "swing"
                leg.support_weight = 0.0
                continue
            leg.pending_step = False
            leg.pending_delay = 0.0
            self._spider_reanchor_stance(leg)

    def _spider_leg_attach_at_pose(self, leg: LegState, x: float, y: float, heading: float) -> Tuple[float, float]:
        d = leg.definition
        sign = self._side_sign(d.get("side", "right"))
        af = float(d.get("attach_forward", 0.0)) * self.size
        a_side = float(d.get("attach_side", 0.30)) * self.size * sign
        # Body sway is deliberately omitted from the mechanical contact solve:
        # the artwork and the leg roots must share one rigid body transform.
        fx, fy = math.cos(heading), math.sin(heading)
        rx, ry = -math.sin(heading), math.cos(heading)
        return x + fx * af + rx * a_side, y + fy * af + ry * a_side

    @staticmethod
    def _spider_rotate_local(forward: float, side: float, angle: float) -> Tuple[float, float]:
        ca, sa = math.cos(angle), math.sin(angle)
        return forward * ca - side * sa, forward * sa + side * ca

    def _spider_turn_radial_adjustment(
        self, leg: LegState, forward: float, side: float,
        turn_delta: float, config: dict,
    ) -> float:
        """Return a bounded stance-length change for one incremental turn.

        ``side`` is in the controller's screen-relative body frame, where a
        positive heading change turns toward the positive-side legs.  Those
        inside legs contract and the opposite-side legs extend.  The change is
        deliberately an instantaneous actuation cue, not a stored foot move:
        the tarsus remains fixed in world space while the support fit uses the
        cue to find the next body pose.
        """
        if abs(turn_delta) <= 1e-7:
            return 0.0
        radius = math.hypot(forward, side)
        if radius <= 1e-5:
            return 0.0
        side_sign = self._side_sign(leg.definition.get("side", "right"))
        lateral_leverage = clamp(abs(side) / radius, 0.35, 1.0)
        # Positive turn -> positive-side/inside legs contract.  The sign is
        # reversed for the opposite side, producing the outside push.
        signed_change = -turn_delta * side_sign
        gain = config.get("turn_radial_gain", 0.34)
        limit = config.get("turn_radial_limit", 0.28) * self.size
        change = signed_change * self.size * gain * (0.72 + 0.28 * lateral_leverage)
        return clamp(change, -limit, limit)

    def _spider_turn_rehome_allowance(self, leg: LegState, config: dict) -> float:
        """Allow a planted foot to follow the body's turn without replanting.

        A fixed contact naturally sweeps through a larger body-relative arc as
        the body rotates.  That displacement is not a bad foot placement.  Only
        translation, a reach violation, or a near-limit stance should trigger a
        swing, which removes the tiny corrective taps during quick turns.
        """
        if leg.contact_state != "stance" or leg.stepping or leg.pending_step:
            return 0.0
        local_f, local_s = self._world_to_body_local(leg.foot_x, leg.foot_y)
        radius = math.hypot(local_f, local_s)
        turn_limit = max(0.05, float(config.get("support_turn_limit", 0.58)))
        accumulated = clamp(abs(float(getattr(leg, "stance_turn", 0.0))), 0.0, turn_limit)
        # Arc length is a conservative allowance for the rotation-only part of
        # the target error; the reach/side checks below still remain hard gates.
        return radius * accumulated * 0.92

    def _spider_support_pose(self, scale: float, move_f: float, move_s: float,
                             turn_delta: float, config: dict):
        """Fit a rigid body pose to the fixed feet after one proposed stroke."""
        supports = [
            leg for leg in self.legs
            if leg.contact_state == "stance" and not leg.stepping and leg.support_weight > 0.01
        ]
        if len(supports) < 3:
            return None

        records = []
        turn_fraction = clamp(float(config.get("turn_pose_fraction", 0.78)), 0.55, 1.0)
        effective_turn_delta = turn_delta * turn_fraction
        total_weight = 0.0
        q_f_sum = q_s_sum = p_x_sum = p_y_sum = 0.0
        for leg in supports:
            stroke_f = leg.stance_stroke_f - move_f * scale
            stroke_s = leg.stance_stroke_s - move_s * scale
            stance_turn = leg.stance_turn - effective_turn_delta * scale
            base_f, base_s = self._spider_rotate_local(
                leg.stance_base_f, leg.stance_base_s, stance_turn
            )
            q_f = base_f + stroke_f
            q_s = base_s + stroke_s
            radial_change = self._spider_turn_radial_adjustment(
                leg, base_f, base_s, effective_turn_delta * scale, config
            )
            if radial_change:
                base_radius = max(1e-5, math.hypot(base_f, base_s))
                q_f += base_f / base_radius * radial_change
                q_s += base_s / base_radius * radial_change
            weight = max(0.01, float(leg.support_weight))
            records.append((leg, q_f, q_s, stroke_f, stroke_s, stance_turn, weight))
            total_weight += weight
            q_f_sum += q_f * weight
            q_s_sum += q_s * weight
            p_x_sum += leg.foot_x * weight
            p_y_sum += leg.foot_y * weight

        q_f_c = q_f_sum / total_weight
        q_s_c = q_s_sum / total_weight
        p_x_c = p_x_sum / total_weight
        p_y_c = p_y_sum / total_weight
        dot = cross = 0.0
        for leg, q_f, q_s, _, _, _, weight in records:
            px = leg.foot_x - p_x_c
            py = leg.foot_y - p_y_c
            qx = q_f - q_f_c
            qy = q_s - q_s_c
            dot += weight * (qx * px + qy * py)
            cross += weight * (qx * py - qy * px)
        solved_heading = math.atan2(cross, dot) if abs(dot) + abs(cross) > 1e-7 else self.heading
        solver_dt = max(0.001, self._spider_solver_dt)
        max_heading_step = config["max_body_turn_rate"] * solver_dt
        if abs(effective_turn_delta) > 1e-7:
            # Do not let the geometric fit reintroduce a larger angular jump
            # than the eased turn request that produced this proposal.
            max_heading_step = min(max_heading_step, abs(effective_turn_delta))
        heading_step = ((solved_heading - self.heading + math.pi) % math.tau) - math.pi
        heading_step = clamp(heading_step, -max_heading_step, max_heading_step)
        # The support fit can retain rotational residual after the requested
        # turn has already arrived. Never let that residual carry the body past
        # its requested heading; feet may continue cycling, but the body must
        # not slowly spin beyond the target.
        target_error = ((self.target_heading - self.heading + math.pi) % math.tau) - math.pi
        if abs(target_error) <= 1e-4:
            heading_step = 0.0
        elif heading_step * target_error <= 0.0:
            heading_step = 0.0
        else:
            heading_step = math.copysign(min(abs(heading_step), abs(target_error)), target_error)
        solved_heading = self.heading + heading_step
        fx, fy = math.cos(solved_heading), math.sin(solved_heading)
        rx, ry = -math.sin(solved_heading), math.cos(solved_heading)
        solved_x = p_x_c - (fx * q_f_c + rx * q_s_c)
        solved_y = p_y_c - (fy * q_f_c + ry * q_s_c)

        # Validate the proposed mechanical pose before committing the stroke.
        # Reaching the envelope causes the gait scheduler to lift a foot; it
        # never permits the body to stretch the planted chain indefinitely.
        for leg, _, _, stroke_f, stroke_s, stance_turn, _ in records:
            if math.hypot(stroke_f, stroke_s) > config["support_stroke_limit"] * self.size + 1e-4:
                return None
            if abs(stance_turn) > config["support_turn_limit"] + 1e-4:
                return None
            attach_x, attach_y = self._spider_leg_attach_at_pose(leg, solved_x, solved_y, solved_heading)
            reach = math.hypot(leg.foot_x - attach_x, leg.foot_y - attach_y)
            if reach > self._leg_max_reach(leg, visual=False) * 0.98:
                return None
            lf_dx = leg.foot_x - solved_x
            lf_dy = leg.foot_y - solved_y
            local_f = lf_dx * fx + lf_dy * fy
            local_s = lf_dx * rx + lf_dy * ry
            side = self._side_sign(leg.definition.get("side", "right"))
            min_side = max(
                float(leg.definition.get("attach_side", 0.30)) * self.size * 0.72,
                float(leg.definition.get("rest_side", 1.0)) * self.size * 0.30,
            )
            # A fixed tarsus can sweep inward during a turn.  Keep a hard
            # anti-crossing floor, but do not reject the support pose merely
            # because the foot has temporarily moved inside its neutral side
            # lane; that rejection was what stalled rotation and triggered a
            # burst of corrective taps.
            side_floor = min_side
            if abs(stance_turn) > 1e-4 or abs(effective_turn_delta) > 1e-4:
                side_floor = max(self.size * 0.05, min_side * 0.20)
            if side * local_s < side_floor:
                return None
            if abs(local_f) > self.size * 2.45:
                return None
        return solved_x, solved_y, solved_heading, records

    def _spider_grounded_mode_allowed(self) -> bool:
        """Return whether Snowpuff-2 may use stance-driven locomotion now."""
        if self._spider_gait_config() is None or self.airborne or self.dragging:
            return False
        if self.inertia_timer > 0.0:
            return False
        # These states deliberately own body displacement.  Walking resumes by
        # re-anchoring the contacts on the next controller frame.
        return self.state not in ("Jump", "Land", "Roll", "DriftRun", "Dragged")

    def _spider_filtered_heading(self, raw_heading: float, dt: float, config: dict) -> float:
        """Filter target bearing without slowing the actual body solver.

        A cursor can change direction several times between two meaningful
        locomotion decisions.  A first-order angular filter makes those changes
        readable as an intentional arc.  The deadband prevents tiny bearing
        noise around the cursor from constantly waking the turn/gait scheduler.
        The wrapped error keeps the filter stable across +/- pi.
        """
        current = float(self._spider_heading_filter)
        error = ((raw_heading - current + math.pi) % math.tau) - math.pi
        if abs(error) <= config["heading_deadband"]:
            return current
        alpha = 1.0 - math.exp(-config["heading_smoothing"] * max(0.0, min(dt, 0.10)))
        current += error * alpha
        self._spider_heading_filter = current
        return current

    def _smooth_turn_step(self, target_error: float, dt: float, requested_rate: float) -> float:
        """Return a quick but acceleration-limited angular step.

        The old turn path could apply its full angular rate immediately. During
        a fast pivot that made the body jump several degrees per frame, while a
        support handoff could then hold it still for a few frames. Ramping the
        *magnitude* of the turn keeps the support-driven causality intact while
        making the start, pauses, and finish read as one deliberate motion.
        """
        requested_rate = clamp(abs(float(requested_rate)), 0.0, 12.0)
        response = 10.0
        alpha = 1.0 - math.exp(-max(0.0, float(dt)) * response)
        self._turn_speed_smooth += (
            requested_rate - self._turn_speed_smooth
        ) * alpha
        self._turn_speed_smooth = clamp(self._turn_speed_smooth, 0.0, 12.0)
        return min(abs(float(target_error)), self._turn_speed_smooth * max(0.0, float(dt)))

    def _spider_locomotion_intent(self, dt: float) -> Tuple[float, float, float]:
        """Convert behavior state into a local stroke and turn request."""
        dx = self.target_x - self.x
        dy = self.target_y - self.y
        target_dist = math.hypot(dx, dy)
        strafe = self._walks_while_facing_elsewhere()
        self.strafe_observe = strafe
        if target_dist > 2.0 and not strafe:
            raw_heading = math.atan2(dy, dx)
            spider_gait = self._spider_gait_config()
            if spider_gait is not None:
                self.target_heading = self._spider_filtered_heading(raw_heading, dt, spider_gait)
            else:
                self.target_heading = raw_heading

        desired_speed = self.speed if not self.motion_paused else 0.0
        if target_dist < 15.0 and self.state not in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun"):
            desired_speed *= target_dist / 15.0
        angle_error = ((self.target_heading - self.heading + math.pi) % math.tau) - math.pi
        spider_gait = self._spider_gait_config()
        alignment_floor = spider_gait["turn_speed_floor"] if spider_gait is not None else 0.15
        alignment = clamp(1.0 - abs(angle_error) / (math.pi * 0.75), alignment_floor, 1.0)
        desired_speed *= alignment
        desired_speed *= self._skitter_motion_factor(dt, desired_speed, target_dist)
        accel_mult = float(self.personality.get("acceleration_multiplier", 1.0))
        accel = (420.0 if desired_speed > self.current_speed else 580.0) * accel_mult
        if self.current_speed < desired_speed:
            self.current_speed = min(desired_speed, self.current_speed + accel * dt)
        else:
            self.current_speed = max(desired_speed, self.current_speed - accel * dt)

        if strafe and target_dist > 1e-4:
            move_x, move_y = dx / target_dist, dy / target_dist
        else:
            move_x, move_y = math.cos(self.heading), math.sin(self.heading)
        req_vx, req_vy = move_x * self.current_speed, move_y * self.current_speed
        fx, fy, rx, ry = self._basis()
        request_f = (req_vx * fx + req_vy * fy) * dt
        request_s = (req_vx * rx + req_vy * ry) * dt
        turn_mult = 1.35 if self.state in ("Chase", "Retreat", "Startled") else 1.0
        if self.state in ("Alert", "Approach", "Chase", "Observe", "Retreat", "Startled"):
            # The legacy body path already honors this personality control. Keep
            # the stance-driven path equally responsive for hunters and skittish
            # spiders instead of silently falling back to the base turn rate.
            turn_mult *= float(self.personality.get("turn_rate_multiplier", 1.0))
        if spider_gait is not None:
            turn_mult *= spider_gait["turn_gain"]
            requested_rate = min(
                abs(self.turn_rate * turn_mult),
                float(spider_gait["max_body_turn_rate"]),
            )
        else:
            requested_rate = abs(self.turn_rate * turn_mult)
        if abs(angle_error) > 1e-7:
            requested_turn = math.copysign(
                self._smooth_turn_step(angle_error, dt, requested_rate),
                angle_error,
            )
        else:
            # Let the angular actuator settle at the requested heading. Keeping
            # the old rate cached here would make the next abrupt target change
            # skip the acceleration ramp and reintroduce a twitch.
            self._smooth_turn_step(0.0, dt, 0.0)
            requested_turn = 0.0
        return request_f, request_s, requested_turn

    def _update_spider_grounded_locomotion_step(self, dt: float) -> None:
        """Drive body pose from planted feet, then let the gait update swings."""
        config = self._spider_gait_config()
        if config is None:
            return
        if not self._spider_locomotion_active:
            self._spider_reanchor_contacts()
            self._spider_locomotion_active = True

        self._spider_solver_dt = dt
        move_f, move_s, turn_delta = self._spider_locomotion_intent(dt)
        blend_time = config["support_blend_time"]
        for leg in self.legs:
            if leg.contact_state == "stance" and not leg.stepping:
                leg.contact_age += dt
                leg.support_weight = min(1.0, max(leg.support_weight, leg.contact_age / blend_time))
                leg.stroke_progress = clamp(
                    math.hypot(leg.stance_stroke_f, leg.stance_stroke_s)
                    / max(1.0, config["support_stroke_limit"] * self.size),
                    0.0, 1.0,
                )

        old_x, old_y, old_heading = self.x, self.y, self.heading
        chosen = None
        # Reduce a stroke before reducing the body pose.  This makes a sharp
        # turn slow at the leg envelope instead of crossing or overextending.
        for scale in (1.0, 0.78, 0.56, 0.34, 0.16, 0.0):
            proposal = self._spider_support_pose(scale, move_f, move_s, turn_delta, config)
            if proposal is not None:
                chosen = (scale, proposal)
                break
        if chosen is not None:
            scale, (new_x, new_y, new_heading, records) = chosen
            for leg, _, _, stroke_f, stroke_s, stance_turn, _ in records:
                leg.stance_stroke_f = stroke_f
                leg.stance_stroke_s = stroke_s
                leg.stance_turn = stance_turn
                leg.stroke_progress = clamp(
                    math.hypot(stroke_f, stroke_s)
                    / max(1.0, config["support_stroke_limit"] * self.size),
                    0.0, 1.0,
                )
            self.x, self.y, self.heading = new_x, new_y, new_heading
        self.x, self.y = clamp_point(self.x, self.y, self.margin * 0.4, self.screen_w, self.screen_h)
        playfield = self._split_playfield()
        if playfield is not None:
            # The same margin the manager's backstop uses, so it never has to act (DC-88).
            self.x, self.y = playfield.clamp(self.x, self.y, max(8.0, self.size * 0.5))
        self.vel_x = (self.x - old_x) / max(dt, 1e-4)
        self.vel_y = (self.y - old_y) / max(dt, 1e-4)
        actual_turn = ((self.heading - old_heading + math.pi) % math.tau) - math.pi
        self.turn_rehome_pressure = clamp(
            self.turn_rehome_pressure * max(0.0, 1.0 - dt * 2.6) + abs(actual_turn) * 3.0,
            0.0, 1.4,
        )

        # The body shell and the leg roots share the mechanical transform.  Keep
        # breathing in the shell, but do not add a second whole-body sway/bob
        # transform that the contacts do not experience.
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        self.body_bob = 0.0
        self.body_sway = 0.0
        self.bob_phase += dt * (3.6 + self.current_speed * 0.095)
        self.breath_phase += dt * (1.55 + 0.35 * speed01)
        self.abdomen_pulse = math.sin(self.breath_phase) * 0.045 + math.sin(self.bob_phase * 0.5) * speed01 * 0.022
        self.ceph_pulse = -math.sin(self.breath_phase + 0.85) * 0.018

    def _update_spider_grounded_locomotion(self, dt: float) -> None:
        """Advance grounded mechanics at a fixed substep for frame-rate stability."""
        duration = max(0.0, float(dt))
        substeps = max(1, int(round(duration * 120.0)))
        step = min(1.0 / 120.0, duration)
        for _ in range(substeps):
            self._update_spider_grounded_locomotion_step(step)

    def _update_spider_grounded_frame(self, dt: float) -> None:
        """Run the production grounded order: contacts/body solve, then gait."""
        config = self._spider_gait_config()
        if config is None:
            return
        duration = max(0.0, float(dt))
        substeps = max(1, int(round(duration * 120.0)))
        step = min(1.0 / 120.0, duration)
        for _ in range(substeps):
            self._update_spider_grounded_locomotion_step(step)
            self._update_spider_gait_step(step, config)
        self._spider_gait_frame_updated = True

