"""Leg solver, gait timing, and stepping.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import List, Tuple

from ..math_utils import (
    angle_lerp,
    clamp,
    clamp_point,
)
from .constants import (
    smootherstep,
)

@dataclass
class LegState:
    definition: dict
    foot_x: float = 0.0
    foot_y: float = 0.0
    step_start_x: float = 0.0
    step_start_y: float = 0.0
    step_target_x: float = 0.0
    step_target_y: float = 0.0
    step_control_x: float = 0.0
    step_control_y: float = 0.0
    pending_target_x: float = 0.0
    pending_target_y: float = 0.0
    pending_delay: float = 0.0
    step_timer: float = 0.0
    step_duration: float = 0.24
    stepping: bool = False
    pending_step: bool = False
    lift: float = 0.0
    # Randomised per instance by Creature, after construction, from the
    # creature's own seeded generator -- a dataclass default factory has no
    # access to it.
    phase_seed: float = 0.0
    twitch_clock: float = 0.0
    gait_phase_offset: float = 0.0
    step_cooldown: float = 0.0
    # Grounded locomotion state.  ``foot_x/y`` is the world-space contact;
    # these values describe how that fixed contact is being stroked behind the
    # body while the body pose is solved from all supporting legs.
    contact_state: str = "stance"
    support_weight: float = 1.0
    contact_age: float = 0.0
    stroke_progress: float = 0.0
    stance_base_f: float = 0.0
    stance_base_s: float = 0.0
    stance_stroke_f: float = 0.0
    stance_stroke_s: float = 0.0
    stance_turn: float = 0.0
    last_step_phase: float = 0.0
    last_step_emergency: bool = False
    joint_phase: float = 0.0
    joint_bends: List[float] = field(default_factory=list)
    # Suspended/carry pose dynamics. These are screen-space spring offsets for
    # the visible foot and are deliberately separate from grounded contacts.
    held_spring_x: float = 0.0
    held_spring_y: float = 0.0
    held_spring_vx: float = 0.0
    held_spring_vy: float = 0.0


class KinematicsMixin:
    """Leg IK, gait scheduling, and grounded-locomotion body solving."""

    def _basis(self) -> Tuple[float, float, float, float]:
        """Forward and right unit vectors for the current heading.

        Measured at about 460 calls per spider per frame, which is four
        trigonometric functions each time for a heading that only changes once
        per update. Cached on the heading value itself, so a changed heading
        recomputes and there is no way to serve a stale basis.
        """
        heading = self.heading
        cached = self._basis_cache
        if cached is not None and cached[0] == heading:
            return cached[1]
        fx = math.cos(heading)
        fy = math.sin(heading)
        # Screen-space right vector. At heading 0, right points downward.
        basis = (fx, fy, -fy, fx)
        self._basis_cache = (heading, basis)
        return basis

    @classmethod
    def _side_sign(cls, side: str) -> float:
        try:
            return cls._SIDE_SIGNS[side]
        except (KeyError, TypeError):
            sign = -1.0 if str(side).lower().startswith("l") else 1.0
            try:
                cls._SIDE_SIGNS[side] = sign
            except TypeError:
                pass
            return sign

    def _world_to_body_local(self, x: float, y: float) -> Tuple[float, float]:
        fx, fy, rx, ry = self._basis()
        dx = x - self.x
        dy = y - self.y
        return dx * fx + dy * fy, dx * rx + dy * ry

    def _body_local_to_world(self, forward: float, side: float) -> Tuple[float, float]:
        fx, fy, rx, ry = self._basis()
        return self.x + fx * forward + rx * side, self.y + fy * forward + ry * side

    def _compute_leg_sectors(self) -> None:
        """Give each leg its own angular territory around the body.

        The lively gait confines every foot to a wedge between the midpoints to
        its angular neighbours, so legs can never cross into one another (the
        "pinwheel/swastika" tangle when turning).  Wedges are derived from where
        each leg naturally rests, so the real per-row stance is preserved: front
        legs point forward, rear legs trail back.  A per-leg distance range is
        stored too, so a foot can never sit absurdly far from the body.
        """
        self._leg_sectors = {}

        def rest_polar(leg: LegState):
            d = leg.definition
            if "rest_forward" in d or "rest_side" in d:
                rf = float(d.get("rest_forward", 0.0))
                rs = abs(float(d.get("rest_side", 1.0)))
                ang = math.degrees(math.atan2(rs, rf))  # 0 = straight forward, 180 = straight back
                dist = math.hypot(rf, rs)
            else:
                ang = abs(float(d.get("rest_angle", 90.0)))
                dist = float(d.get("reach", 1.8)) * 0.62
            return ang, max(0.6, dist)

        for side in ("left", "right"):
            idxs = [i for i, leg in enumerate(self.legs)
                    if str(leg.definition.get("side", "right")).lower().startswith(side[0])]
            idxs.sort(key=lambda i: rest_polar(self.legs[i])[0])
            n = len(idxs)
            for k, i in enumerate(idxs):
                ang, dist = rest_polar(self.legs[i])
                prev_ang = rest_polar(self.legs[idxs[k - 1]])[0] if k > 0 else None
                next_ang = rest_polar(self.legs[idxs[k + 1]])[0] if k + 1 < n else None
                lo = (ang + prev_ang) * 0.5 if prev_ang is not None else max(6.0, ang - 26.0)
                hi = (ang + next_ang) * 0.5 if next_ang is not None else min(174.0, ang + 26.0)
                lo += 2.5   # small buffer so neighbouring wedges never touch
                hi -= 2.5
                if hi <= lo:
                    lo = hi = (lo + hi) * 0.5
                self._leg_sectors[id(self.legs[i])] = (
                    math.radians(lo), math.radians(hi), dist * 0.42, dist * 1.30,
                )

    def _clamp_to_sector(self, leg: LegState, x: float, y: float) -> Tuple[float, float]:
        """Pull a foot into its leg's angular wedge and distance range.

        Operates in body-centred polar coordinates so a foot is always on the
        leg's own side, inside its wedge, and within a believable distance; this
        is what guarantees the lively gait never crosses or overstretches a leg.
        """
        sec = self._leg_sectors.get(id(leg))
        if not sec:
            return x, y
        lo, hi, rmin_m, rmax_m = sec
        sign = self._side_sign(leg.definition.get("side", "right"))
        f, s_signed = self._world_to_body_local(x, y)
        s = s_signed * sign                       # own-side component
        r = clamp(math.hypot(f, s), self.size * rmin_m, self.size * rmax_m)
        ang = math.atan2(max(s, 1e-4), f)         # forced onto the leg's own side
        ang = clamp(ang, lo, hi)
        f2 = r * math.cos(ang)
        s2 = r * math.sin(ang)
        return self._body_local_to_world(f2, s2 * sign)

    def _leg_max_reach(self, leg: LegState, visual: bool = False) -> float:
        """Maximum believable coxa-to-tarsus reach for one leg.

        This is a *safety* limit, not the normal gait envelope.  The previous
        version used this value inside foot target placement, which made the rear
        legs freeze because their valid rest lane was being clipped too tightly.
        Keep it generous for scheduling and a little tighter only for rendering.

        Pure in the leg definition and the body size, and asked about fifty
        times per spider per frame, so it is cached on exactly those. A spider
        that grows a level changes its size and the cache misses.
        """
        cache_key = (id(leg), visual, self.size)
        cached = self._reach_cache.get(cache_key)
        if cached is not None:
            return cached
        d = leg.definition
        reach = max(self.size * 0.85, float(d.get("reach", 1.8)) * self.size)
        upper = max(self.size * 0.22, float(d.get("upper_len", 0.85)) * self.size)
        lower = max(self.size * 0.22, float(d.get("lower_len", 1.05)) * self.size)
        rest_f = float(d.get("rest_forward", 0.0)) * self.size
        rest_s = abs(float(d.get("rest_side", 1.0)) * self.size)
        attach_f = float(d.get("attach_forward", 0.0)) * self.size
        attach_s = abs(float(d.get("attach_side", 0.30)) * self.size)
        rest_dist = math.hypot(rest_f - attach_f, rest_s - attach_s)

        # The art rig is stylised, so a strict upper+lower IK length is too short
        # for the rear pair.  Use it as a soft visual guide, not a hard gait brake.
        chain_limit = (upper + lower) * (1.08 if visual else 1.18)
        target_limit = reach * (1.04 if visual else 1.14)
        rest_allowance = rest_dist * (1.18 if visual else 1.30)
        reach_limit = max(min(chain_limit, target_limit), rest_allowance, self.size * 0.92)
        if len(self._reach_cache) > 64:
            # Bounded: a size that changes every frame would otherwise grow this
            # without limit, and only the current size is ever asked about.
            self._reach_cache.clear()
        self._reach_cache[cache_key] = reach_limit
        return reach_limit

    def _leg_reach_metrics(self, leg: LegState, x: float, y: float, visual: bool = False) -> Tuple[float, float, bool, bool]:
        ax, ay = self._leg_attach(leg)
        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(x, y)
        dist = math.hypot(f_f - a_f, f_s - a_s)
        max_reach = self._leg_max_reach(leg, visual=visual)
        too_far = dist > max_reach
        very_far = dist > max_reach * 1.14
        return dist, max_reach, too_far, very_far

    def _limit_world_point_to_leg_reach(self, leg: LegState, x: float, y: float, visual: bool = False) -> Tuple[float, float]:
        """Clamp a world point to the coxa-centered reach circle without changing lanes."""
        ax, ay = self._leg_attach(leg)
        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(x, y)
        dv_f = f_f - a_f
        dv_s = f_s - a_s
        dist = math.hypot(dv_f, dv_s)
        max_reach = self._leg_max_reach(leg, visual=visual)
        if dist > max_reach:
            scale = max_reach / max(1e-5, dist)
            f_f = a_f + dv_f * scale
            f_s = a_s + dv_s * scale
            return self._body_local_to_world(f_f, f_s)
        return x, y

    def _soft_limit_leg_state(self, leg: LegState, blend: float = 1.0) -> None:
        """Gently pull impossible stored footfalls back after dragging.

        Do not clamp active swing paths aggressively.  The renderer has its own
        safety clamp, while the gait scheduler must still see a misplaced rear
        foot as urgent so it actually steps instead of being dragged forever.
        """
        blend = clamp(blend, 0.0, 1.0)
        if blend <= 0.0:
            return
        pairs = [("foot_x", "foot_y")]
        if not leg.stepping:
            pairs.extend((("step_start_x", "step_start_y"), ("step_target_x", "step_target_y"), ("step_control_x", "step_control_y")))
        if not leg.pending_step:
            pairs.append(("pending_target_x", "pending_target_y"))

        for x_attr, y_attr in pairs:
            x = getattr(leg, x_attr)
            y = getattr(leg, y_attr)
            _, _, too_far, very_far = self._leg_reach_metrics(leg, x, y, visual=False)
            if not very_far:
                continue
            safe_x, safe_y = self._limit_world_point_to_leg_reach(leg, x, y, visual=False)
            safe_x, safe_y = self._constrain_leg_point(leg, safe_x, safe_y)
            use_blend = blend if x_attr == "foot_x" else min(blend, 0.55)
            setattr(leg, x_attr, x + (safe_x - x) * use_blend)
            setattr(leg, y_attr, y + (safe_y - y) * use_blend)

    def _leg_alignment_metrics(self, leg: LegState, x: float, y: float) -> Tuple[float, float, float, float, bool, bool]:
        """Return local forward/side plus whether the foot is on the wrong lateral or front/back zone."""
        d = leg.definition
        sign = self._side_sign(d.get("side", "right"))
        local_f, local_s = self._world_to_body_local(x, y)
        rest_f = float(d.get("rest_forward", 0.0)) * self.size
        rest_s = abs(float(d.get("rest_side", 1.0)) * self.size)
        attach_f = float(d.get("attach_forward", 0.0)) * self.size
        attach_s = abs(float(d.get("attach_side", 0.30)) * self.size)

        min_side = max(attach_s + self.size * 0.10, rest_s * 0.48)
        side_mag = sign * local_s
        wrong_side = side_mag < min_side
        severe_wrong_side = side_mag < max(self.size * 0.05, min_side * 0.36)

        if abs(rest_f) >= self.size * 0.22:
            if rest_f >= 0.0:
                forward_wrong = local_f < attach_f - self.size * 0.22
            else:
                forward_wrong = local_f > attach_f + self.size * 0.22
        else:
            forward_wrong = abs(local_f - rest_f) > self.size * 0.95
        return local_f, local_s, min_side, rest_f, wrong_side, (severe_wrong_side or forward_wrong)

    def _constrain_leg_point(self, leg: LegState, x: float, y: float) -> Tuple[float, float]:
        """Keep a foot target on the anatomically correct side of the body and within a heading-relative envelope."""
        d = leg.definition
        sign = self._side_sign(d.get("side", "right"))
        local_f, local_s = self._world_to_body_local(x, y)

        rest_f = float(d.get("rest_forward", 0.0)) * self.size
        rest_s = abs(float(d.get("rest_side", 1.0)) * self.size)
        attach_f = float(d.get("attach_forward", 0.0)) * self.size
        attach_s = abs(float(d.get("attach_side", 0.30)) * self.size)
        reach = max(self.size * 0.7, float(d.get("reach", 1.8)) * self.size)

        min_side = max(attach_s + self.size * 0.10, rest_s * 0.48)
        max_side = max(min_side + self.size * 0.28, rest_s * 1.38)
        clamped_side_mag = clamp(sign * local_s, min_side, max_side)
        local_s = sign * clamped_side_mag

        if abs(rest_f) >= self.size * 0.22:
            if rest_f >= 0.0:
                forward_min = min(attach_f, rest_f) - self.size * 0.55
                forward_max = max(attach_f, rest_f) + self.size * 0.95
            else:
                forward_min = min(attach_f, rest_f) - self.size * 0.95
                forward_max = max(attach_f, rest_f) + self.size * 0.55
        else:
            forward_min = min(rest_f, attach_f) - self.size * 0.70
            forward_max = max(rest_f, attach_f) + self.size * 0.70
        local_f = clamp(local_f, forward_min, forward_max)

        # Clamp distance from the leg attachment so planted feet never drift to impossible opposite-side poses.
        ax, ay = self._leg_attach(leg)
        a_f, a_s = self._world_to_body_local(ax, ay)
        dv_f = local_f - a_f
        dv_s = local_s - a_s
        dist = math.hypot(dv_f, dv_s)
        # Normal gait targets use the model reach.  The stricter visual safety
        # limit is applied only at render/drag repair time so rear legs do not freeze.
        max_reach = reach * 1.08
        min_reach = max(self.size * 0.20, reach * 0.24)
        if dist > max_reach:
            scale = max_reach / max(1e-5, dist)
            dv_f *= scale
            dv_s *= scale
        elif dist < min_reach:
            scale = min_reach / max(1e-5, dist)
            dv_f *= scale
            dv_s *= scale
        local_f = a_f + dv_f
        local_s = a_s + dv_s
        # Final safety to keep correct body side after reach clamp.
        final_side_mag = sign * local_s
        if final_side_mag < min_side:
            local_s = sign * min_side
        return self._body_local_to_world(local_f, local_s)

    def _uses_lively_gait(self) -> bool:
        return getattr(self, "gait_style", "classic") in ("lively", "skitter")

    def _uses_skitter_gait(self) -> bool:
        return getattr(self, "gait_style", "classic") == "skitter"

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
        strafe = self.state == "Observe" and self._is_observer_personality()
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
            if self.state == "Observe" and self._is_observer_personality() and self.current_speed > 2.0:
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

    def _finish_roll(self) -> None:
        """Undo everything a tumble was doing: the spin, the tuck, the feet."""
        self._rolling = False
        self.roll_spin = 0.0
        self.roll_tuck = 0.0
        self.roll_progress = 1.0
        self.motion_paused = False
        self.squash = 1.0
        self._reset_roll_contacts()
        self._spider_locomotion_active = False

    def _reconcile_roll(self) -> None:
        """Tidy up after a tumble that something cut short.

        Only the roll running to completion used to clean up after itself, so
        anything that interrupted one -- a startle, a grab, a job, the roll
        skill being switched off -- left the body rotated by whatever the spin
        had reached and the legs still tucked, permanently. Measured across
        seven seeds that was about 410 degrees of leftover rotation, feet past
        their own reach envelope and up to two legs on the wrong side of the
        body.

        This is the same reconciliation the weaving and web-walking states
        already do at the top of every frame, for the same reason: a state can
        be left in more ways than it can be finished.
        """
        if self._rolling and self.state != "Roll":
            self._finish_roll()

    def _reset_roll_contacts(self) -> None:
        """Restore every foot to a safe stance after a visual tumble.

        Roll rotates the rendered creature around its centre, but deliberately
        does not rotate the logical body heading. Leaving the old world-space
        contacts in place therefore makes the body finish in one orientation
        while most feet still belong to the pre-roll orientation. That is the
        source of the one-frame stretched legs and flattened-looking landings
        seen on non-tarantula sprite rigs.

        A roll is an explicit flourish, so a clean contact reset is preferable
        to replaying the normal walking scheduler here. The next grounded frame
        can then begin a new gait from eight bounded, stance contacts.
        """
        for leg in self.legs:
            foot_x, foot_y = self._leg_ideal_foot(leg)
            foot_x, foot_y = self._constrain_leg_point(leg, foot_x, foot_y)
            leg.foot_x, leg.foot_y = foot_x, foot_y
            leg.step_start_x, leg.step_start_y = foot_x, foot_y
            leg.step_target_x, leg.step_target_y = foot_x, foot_y
            leg.step_control_x, leg.step_control_y = foot_x, foot_y
            leg.pending_target_x, leg.pending_target_y = foot_x, foot_y
            leg.pending_delay = 0.0
            leg.step_timer = 0.0
            leg.stepping = False
            leg.pending_step = False
            leg.lift = 0.0
            leg.joint_bends = []
            leg.held_spring_x = 0.0
            leg.held_spring_y = 0.0
            leg.held_spring_vx = 0.0
            leg.held_spring_vy = 0.0
            self._spider_reanchor_stance(leg)
        self._spider_heading_filter = self.heading

    def _update_roll(self, dt: float, mx: float, my: float) -> None:
        # Spin through the planned turns with an ease-out so it whirls fast then
        # settles. The body stays put under the spin; we only nudge the centre
        # along the drift direction and carry the (tucked) feet with it.
        self.motion_paused = True
        self.speed = 0.0
        # The grounded solver runs after the FSM update in the normal frame
        # order. Keep walking momentum out of the roll and its completion frame,
        # otherwise the spider can resume in the pre-roll direction.
        self.current_speed = 0.0
        self.vel_x = 0.0
        self.vel_y = 0.0
        prev_eased = self.roll_eased
        self.roll_progress = min(1.0, self.roll_progress + dt / max(0.05, self.roll_duration))
        self.roll_eased = 1.0 - (1.0 - self.roll_progress) ** 2
        d_eased = self.roll_eased - prev_eased
        self.roll_spin = self.roll_dir * self.roll_total * self.roll_eased
        self.roll_tuck = math.sin(math.pi * clamp(self.roll_progress, 0.0, 1.0)) ** 0.7

        dist = self.roll_distance * d_eased
        ndx = math.cos(self.roll_drift_dir) * dist
        ndy = math.sin(self.roll_drift_dir) * dist
        nx, ny = clamp_point(self.x + ndx, self.y + ndy, self.margin, self.screen_w, self.screen_h)
        adx, ady = nx - self.x, ny - self.y
        self.x, self.y = nx, ny
        self._translate_leg_world_points(adx, ady, 1.0)
        # Freeze the base facing and target so the spin reads cleanly and the body
        # does not also try to turn or travel underneath it.
        self.target_x, self.target_y = self.x, self.y
        self.target_heading = self.heading

        if self.roll_progress >= 1.0:
            self._finish_roll()
            self.enter_idle()

    def _skitter_motion_factor(self, dt: float, desired_speed: float, target_dist: float) -> float:
        """Return a stop-go speed multiplier for the Skitter movement style.

        It leaves ordinary behaviour decisions alone, but changes how the body
        covers distance: very short rapid runs are interrupted by tiny still
        moments, matching the quick-successions-and-freeze feel of the reference
        spider gif.
        """
        if not self._uses_skitter_gait():
            return 1.0
        if desired_speed <= 5.0 or target_dist < max(10.0, self.size * 0.70):
            # Reset to a fresh burst when movement starts again, rather than
            # unexpectedly beginning inside a pause.
            self._skitter_pause_timer = 0.0
            self._skitter_burst_timer = min(self._skitter_burst_timer, self.rng.uniform(0.12, 0.24))
            return 1.0
        # Do not make low-grip drifting more nervous; that personality already
        # has its own momentum rhythm.
        if self.state == "DriftRun" and self._is_drift_sliding():
            return 1.0

        speed01 = clamp(desired_speed / 160.0, 0.0, 1.0)
        self._skitter_phase = (self._skitter_phase + dt * (18.0 + speed01 * 16.0)) % math.tau

        if self._skitter_pause_timer > 0.0:
            self._skitter_pause_timer = max(0.0, self._skitter_pause_timer - dt)
            return 0.0

        self._skitter_burst_timer -= dt
        if self._skitter_burst_timer <= 0.0:
            fast_state = self.state in ("Chase", "Retreat", "Startled")
            self._skitter_pause_timer = self.rng.uniform(0.055, 0.135) if fast_state else self.rng.uniform(0.085, 0.22)
            self._skitter_burst_timer = self.rng.uniform(0.10, 0.24) if fast_state else self.rng.uniform(0.16, 0.36)
            self._skitter_burst_jitter = self.rng.uniform(0.94, 1.20)
            return 0.0

        # The burst is not perfectly steady: a tiny internal pulse makes the run
        # read as several fast pushes in succession rather than one smooth glide.
        pulse = 0.90 + 0.16 * max(0.0, math.sin(self._skitter_phase))
        state_boost = 1.34 if self.state in ("Chase", "Retreat", "Startled") else 1.18
        return state_boost * self._skitter_burst_jitter * pulse

    def _move_body(self, dt: float) -> None:
        edge_push_x = 0.0
        edge_push_y = 0.0
        edge_zone = self.margin + 30.0
        if self.x < edge_zone:
            edge_push_x += (edge_zone - self.x) / edge_zone
        if self.x > self.screen_w - edge_zone:
            edge_push_x -= (self.x - (self.screen_w - edge_zone)) / edge_zone
        if self.y < edge_zone:
            edge_push_y += (edge_zone - self.y) / edge_zone
        if self.y > self.screen_h - edge_zone:
            edge_push_y -= (self.y - (self.screen_h - edge_zone)) / edge_zone
        if edge_push_x or edge_push_y:
            self.target_x = clamp(self.target_x + edge_push_x * 140.0 * dt, self.margin, self.screen_w - self.margin)
            self.target_y = clamp(self.target_y + edge_push_y * 140.0 * dt, self.margin, self.screen_h - self.margin)

        if self.inertia_timer > 0.0:
            self.inertia_timer = max(0.0, self.inertia_timer - dt)
            throw_speed = math.hypot(self.inertia_vx, self.inertia_vy)
            drift_amount = self._drift_amount(dt, throw_speed, inertia=True) if throw_speed > 2.0 else 0.0
            if throw_speed > 2.0:
                self.target_heading = math.atan2(self.inertia_vy, self.inertia_vx)
                if drift_amount:
                    self.target_heading -= drift_amount * float(self.personality.get("drift_countersteer", 0.34))
            old_heading = self.heading
            self.heading = angle_lerp(self.heading, self.target_heading, self.turn_rate * 1.9 * dt)
            turn_delta = ((self.heading - old_heading + math.pi) % math.tau) - math.pi
            old_x, old_y = self.x, self.y
            drift_vx = drift_vy = 0.0
            if drift_amount and throw_speed > 2.0:
                side_x = -self.inertia_vy / throw_speed
                side_y = self.inertia_vx / throw_speed
                drift_vx = side_x * throw_speed * drift_amount * 0.60
                drift_vy = side_y * throw_speed * drift_amount * 0.60
            self.x += (self.inertia_vx + drift_vx) * dt
            self.y += (self.inertia_vy + drift_vy) * dt
            clamped_x, clamped_y = clamp_point(self.x, self.y, self.margin * 0.25, self.screen_w, self.screen_h)
            if clamped_x != self.x:
                self.inertia_vx *= -0.28
            if clamped_y != self.y:
                self.inertia_vy *= -0.28
            self.x, self.y = clamped_x, clamped_y
            self.target_x = clamp(self.x + self.inertia_vx * 0.22, self.margin, self.screen_w - self.margin)
            self.target_y = clamp(self.y + self.inertia_vy * 0.22, self.margin, self.screen_h - self.margin)
            self._translate_leg_world_points(self.x - old_x, self.y - old_y, 0.22)

            decay = max(0.0, 1.0 - dt * (2.15 + clamp(throw_speed / 900.0, 0.0, 1.0) * 1.25))
            self.inertia_vx *= decay
            self.inertia_vy *= decay
            if throw_speed < 24.0:
                self.inertia_timer = 0.0
                self.inertia_vx = 0.0
                self.inertia_vy = 0.0

            self.vel_x = self.inertia_vx + drift_vx
            self.vel_y = self.inertia_vy + drift_vy
            self.current_speed = clamp(math.hypot(self.vel_x, self.vel_y), 0.0, 260.0)
            speed01 = clamp(self.current_speed / 220.0, 0.0, 1.0)
            startle = self._startle_amount()
            self.bob_phase += dt * (5.4 + self.current_speed * 0.055)
            self.breath_phase += dt * (2.2 + 1.2 * startle + 0.35 * speed01)
            self.body_bob = math.sin(self.bob_phase) * (0.6 + speed01 * 1.9)
            self.body_sway = (
                math.sin(self.bob_phase * 0.9) * self.size * (0.018 + startle * 0.020)
                + turn_delta * self.size * 0.55
                + drift_amount * self.size * 0.44
            )
            self.abdomen_pulse = math.sin(self.breath_phase * (1.0 + startle * 0.6)) * (0.045 + 0.035 * startle)
            self.ceph_pulse = -math.sin(self.breath_phase + 0.85) * (0.018 + 0.022 * startle)
            return

        dx = self.target_x - self.x
        dy = self.target_y - self.y
        target_dist = math.hypot(dx, dy)
        move_heading = math.atan2(dy, dx) if target_dist > 2.0 else self.heading
        strafe_observe = self.state == "Observe" and self._is_observer_personality()
        self.strafe_observe = strafe_observe

        if target_dist > 2.0 and not strafe_observe:
            if len(self.legs) >= 8:
                # Keep the older spider rigs from converting a zig-zagging
                # target bearing directly into a body twitch. Gait-enabled
                # models already use the same filter in their grounded path;
                # this is the equivalent for the legacy body path.
                gait_config = self._spider_gait_config()
                if gait_config is not None:
                    self.target_heading = self._spider_filtered_heading(
                        move_heading, dt, gait_config
                    )
                else:
                    filtered = float(self._spider_heading_filter)
                    bearing_error = ((move_heading - filtered + math.pi) % math.tau) - math.pi
                    if abs(bearing_error) > 0.05:
                        filtered += bearing_error * (1.0 - math.exp(-dt * 10.0))
                    self._spider_heading_filter = filtered
                    self.target_heading = filtered
            else:
                self.target_heading = move_heading

        if self.state == "DriftRun":
            # Drifters should not rotate like they have sticky feet.  In charge-up
            # they can still steer, but once sliding the body lags behind the path.
            if self._is_drift_sliding():
                turn_mult = float(self.personality.get("drift_slide_body_turn", 0.42))
            else:
                turn_mult = float(self.personality.get("drift_charge_body_turn", 0.62))
        else:
            turn_mult = 1.35 if self.state in ("Chase", "Retreat", "Dragged", "Startled") else 1.0
            if self.state in ("Alert", "Approach", "Chase", "Observe"):
                turn_mult *= float(self.personality.get("turn_rate_multiplier", 1.0))
        max_turn_rate = abs(self.turn_rate * turn_mult)
        if self._uses_lively_gait() or len(self.legs) >= 8:
            # A spider can pivot quickly, but an unrestricted personality
            # multiplier made the body jump 10+ degrees in one frame. Keep the
            # response fast while giving the angular velocity a smooth ceiling.
            max_turn_rate = min(max_turn_rate, 7.0)
        angle_error = ((self.target_heading - self.heading + math.pi) % math.tau) - math.pi
        max_turn = self._smooth_turn_step(
            angle_error, dt, max_turn_rate if abs(angle_error) > 1e-7 else 0.0
        )
        old_heading = self.heading
        self.heading = angle_lerp(self.heading, self.target_heading, max_turn)
        turn_delta = ((self.heading - old_heading + math.pi) % math.tau) - math.pi
        self.turn_rehome_pressure = clamp(self.turn_rehome_pressure * max(0.0, 1.0 - dt * 2.6) + abs(turn_delta) * 3.0, 0.0, 1.4)

        desired_speed = self.speed if not self.motion_paused else 0.0
        if target_dist < 15.0 and self.state not in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun"):
            desired_speed *= target_dist / 15.0
        if strafe_observe:
            # Observer keeps its head/body aimed at the watched target while the
            # body can move on the opposite vector instead of only forward-facing.
            alignment = float(self.personality.get("observe_strafe_alignment", 1.0))
        else:
            angle_error = abs((self.target_heading - self.heading + math.pi) % math.tau - math.pi)
            if self.state == "DriftRun":
                # Keep momentum through counter-steer; do not kill speed just because
                # the body is intentionally lagging behind the slide direction.
                alignment = clamp(1.0 - angle_error / math.pi, 0.72, 1.0)
            else:
                alignment = clamp(1.0 - angle_error / (math.pi * 0.75), 0.15, 1.0)
        desired_speed *= clamp(alignment, 0.15, 1.0)
        skitter_factor = self._skitter_motion_factor(dt, desired_speed, target_dist)
        desired_speed *= skitter_factor
        accel_mult = float(self.personality.get("acceleration_multiplier", 1.0))
        if self._uses_skitter_gait():
            # Snappy acceleration and braking make the little pauses visible.
            accel_mult *= 1.55 if desired_speed > self.current_speed else 2.35
        if self.state == "DriftRun" and self._is_drift_sliding():
            accel_mult *= float(self.personality.get("drift_slide_accel_grip", 0.46))
        accel = (420.0 if desired_speed > self.current_speed else 580.0) * accel_mult
        if self.current_speed < desired_speed:
            self.current_speed = min(desired_speed, self.current_speed + accel * dt)
        else:
            self.current_speed = max(desired_speed, self.current_speed - accel * dt)

        move = min(target_dist, self.current_speed * dt)
        drift_amount = self._drift_amount(dt, self.current_speed) if move > 0.0 else 0.0
        if move > 0.0:
            if strafe_observe and target_dist > 1e-4:
                move_x = dx / target_dist
                move_y = dy / target_dist
            else:
                move_x = math.cos(self.heading)
                move_y = math.sin(self.heading)
            if drift_amount:
                side_x = -move_y
                side_y = move_x
                blended_x = move_x + side_x * drift_amount
                blended_y = move_y + side_y * drift_amount
                mag = max(1e-5, math.hypot(blended_x, blended_y))
                move_x = blended_x / mag
                move_y = blended_y / mag
            if self.state == "DriftRun" and self._is_drift_sliding():
                # Low-grip slide: preserve the previous travel vector and only let
                # the feet nudge it toward the new target.  This reads as momentum
                # carrying the body sideways instead of legs instantly turning it.
                prev_speed = math.hypot(self.vel_x, self.vel_y)
                if prev_speed > 2.0:
                    old_x = self.vel_x / prev_speed
                    old_y = self.vel_y / prev_speed
                    keep = clamp(float(self.personality.get("drift_inertia_keep", 0.78)) + abs(drift_amount) * 0.14, 0.0, 0.92)
                    move_x = old_x * keep + move_x * (1.0 - keep)
                    move_y = old_y * keep + move_y * (1.0 - keep)
                    mag = max(1e-5, math.hypot(move_x, move_y))
                    move_x /= mag
                    move_y /= mag
            # A low-grip Drifter may slide sideways, but never let stale arc
            # momentum make the sprite visibly moonwalk. Keep the correction
            # limited to the pathological behind-the-body case.
            move_x, move_y = self._guard_drifter_travel_direction(move_x, move_y)
            self.vel_x = move_x * self.current_speed
            self.vel_y = move_y * self.current_speed
            self.x += move_x * move
            self.y += move_y * move
        else:
            self.vel_x *= max(0.0, 1.0 - dt * 8.0)
            self.vel_y *= max(0.0, 1.0 - dt * 8.0)
        self.x, self.y = clamp_point(self.x, self.y, self.margin * 0.4, self.screen_w, self.screen_h)

        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        skitter = self._uses_skitter_gait()
        self.bob_phase += dt * ((5.2 if skitter else 3.6) + self.current_speed * (0.135 if skitter else 0.095))
        self.breath_phase += dt * ((1.85 if skitter else 1.55) + 0.35 * speed01)
        self.body_bob = math.sin(self.bob_phase) * speed01 * (1.20 if skitter else 1.55)
        self.body_sway = (
            math.sin(self.bob_phase * (0.78 if skitter else 0.5)) * speed01 * self.size * (0.022 if skitter else 0.030)
            + turn_delta * self.size * 0.55
            + drift_amount * self.size * 0.26
            + float(getattr(self, "drift_lean", 0.0)) * self.size * 0.62
        )
        # The abdomen pulse is visible at rest and compresses slightly during fast motion.
        self.abdomen_pulse = math.sin(self.breath_phase) * 0.045 + math.sin(self.bob_phase * 0.5) * speed01 * 0.022
        self.ceph_pulse = -math.sin(self.breath_phase + 0.85) * 0.018

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
        if self.state == "Observe" and self._is_observer_personality():
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
        if self.state == "Observe" and self._is_observer_personality() and moving:
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

    def _update_legs_lively(self, dt: float) -> None:
        """Visible alternating-tetrapod gait with lifted legs and object probing.

        Unlike the classic reactive gait, this drives a steady stepping cadence
        tied to body speed (and to turning, so a pivot steps the feet around
        instead of pinning them).  Swinging legs are clearly lifted by the
        renderer.  When the spider is settled near something it is attending to,
        it reaches a front leg out to tap the object and waves its pedipalps.
        """
        spider_gait = self._spider_gait_config()
        if spider_gait is not None:
            if self._spider_gait_frame_updated:
                self._spider_gait_frame_updated = False
                return
            self._update_spider_gait(dt, spider_gait)
            return
        if self.airborne:
            return
        self._advance_active_steps(dt)

        # Turn signal: both the steering error and the rotation actually applied
        # this frame.  A pivot in place still advances the gait clock, so the legs
        # visibly walk the body around to its new facing instead of staying rooted.
        dheading = ((self.heading - self._prev_heading_gait + math.pi) % math.tau) - math.pi
        self._prev_heading_gait = self.heading
        turn_speed = abs(dheading) / max(dt, 1e-3)
        turn_err = abs(((self.target_heading - self.heading + math.pi) % math.tau) - math.pi)

        moving = self.current_speed > 4.0
        turning = turn_speed > 0.30 or turn_err > 0.12
        skitter = self._uses_skitter_gait()
        spider_gait = self._spider_gait_config()
        hold_still = (self.current_speed < (4.5 if skitter else 3.0)) and not turning and not self.dragging
        speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0)
        sliding = self._is_drift_sliding()
        fast_state = self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun")

        max_air = 3 if skitter else 2
        if moving and speed01 >= (0.34 if skitter else 0.5):
            max_air = 4 if skitter else 3
        if fast_state or self.dragging:
            max_air = 5 if skitter else 4
        if turning:
            # A turn needs several feet free to walk the body around quickly so
            # legs do not linger crossed over one another.
            max_air = max(max_air, 4 if skitter else 3)
        if spider_gait is not None:
            # Keep at least five of eight feet planted.  Four airborne legs can
            # be stable for a hexapod, but makes an eight-legged spider rock
            # sideways like a crab.
            max_air = min(max_air, spider_gait["max_airborne"])
        swinging_now = sum(1 for leg in self.legs if leg.stepping or leg.pending_step)

        # Emergency repair: never allow an impossible pose, but fix it with a
        # visible lifted step rather than snapping the foot into place.
        broken_candidates = []
        for i, leg in enumerate(self.legs):
            _, _, _, _, wrong_side, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, too_far, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            broken = severe_wrong or (very_far if hold_still else too_far)
            if broken and not leg.stepping and not leg.pending_step and leg.step_cooldown <= 0.0:
                ix, iy = self._leg_ideal_foot(leg)
                dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy) + (self.size * 0.6 if very_far else 0.0)
                broken_candidates.append((dist, i, leg, ix, iy))
        broken_candidates.sort(reverse=True, key=lambda it: it[0])
        for _, i, leg, ix, iy in broken_candidates[:max(0, max_air - swinging_now)]:
            self._schedule_step(leg, ix, iy, delay=0.0, force_fast=True)
            leg.step_cooldown = self.rng.uniform(0.010, 0.032) if skitter else self.rng.uniform(0.02, 0.05)
            swinging_now += 1

        # Feeler probing runs whether or not the body is moving.
        self._update_feelers(dt, moving=moving, turning=turning)

        if hold_still:
            # Settled: keep planted feet exactly where they are (besides repairs
            # and feeler probes), so a watching spider truly holds still.
            return

        # Cadence clock.  Faster body => quicker steps; a pivot keeps the clock
        # turning so the feet shuffle around the turn.
        cadence_hz = (2.85 + speed01 * 6.2) if skitter else (1.05 + speed01 * 3.0)
        if fast_state:
            cadence_hz += 2.4 if skitter else 1.2
        if turning:
            cadence_hz = max(cadence_hz, (1.65 if skitter else 0.9) + min(turn_speed, 5.5) * (0.86 if skitter else 0.55))
        self._lively_gait_phase = (self._lively_gait_phase + dt * cadence_hz) % 1.0

        # Alternating tetrapod: two leg groups half a cycle out of phase. Skitter
        # uses the same idea as lively, but with a shorter swing window, lower
        # replant threshold, and faster clock so feet tick in quick succession.
        swing_frac = spider_gait["swing_fraction"] if spider_gait is not None else (0.31 if skitter else 0.40)
        group_count = max(1, len(self.gait_groups))
        replant_thresh = self.size * ((0.095 if (moving or sliding) else 0.080) if skitter else (0.16 if (moving or sliding) else 0.12))
        if fast_state:
            replant_thresh *= 0.72 if skitter else 0.8
        if turning:
            replant_thresh = min(replant_thresh, self.size * (0.065 if skitter else 0.10))

        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step or leg.step_cooldown > 0.0:
                continue
            group = int(leg.definition.get("gait_group", 0))
            g = (group % 2) if group_count >= 2 else 0
            window_pos = (self._lively_gait_phase - 0.5 * g) % 1.0
            in_window = window_pos < swing_frac
            ix, iy = self._leg_ideal_foot(leg)
            dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy)
            _, _, _, _, wrong_side, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, too_far, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            # A foot that has slipped to the wrong side of the body, or out of
            # reach, is what produces the crossed / pinwheel tangle when turning.
            # Such feet must step back into their own sector right now, regardless
            # of the cadence window.
            urgent = wrong_side or too_far
            if not (urgent or dist > replant_thresh):
                continue
            score = dist / max(1.0, replant_thresh)
            if urgent:
                score += 3.0 + (1.5 if (severe_wrong or very_far) else 0.0)
            elif in_window:
                score += 1.8 if skitter else 1.4
            else:
                # Outside its swing window only a clearly out-of-place foot steps.
                if score < (1.08 if skitter else 1.25):
                    continue
                score *= 0.74 if skitter else 0.6
            candidates.append((score, i, leg, ix, iy))

        candidates.sort(reverse=True, key=lambda it: it[0])
        slots = max(0, max_air - swinging_now)
        used = set()
        for _, i, leg, ix, iy in candidates:
            if slots <= 0:
                break
            # Keep diagonally opposed / neighbouring legs from lifting together so
            # the four planted feet stay well spread for balance.
            if any(abs(i - j) in (1, 2) for j in used):
                continue
            if skitter:
                delay = self.rng.uniform(0.0, 0.010 if fast_state else 0.020)
                cooldown = self.rng.uniform(0.012, 0.044)
            else:
                delay = self.rng.uniform(0.0, 0.02 if fast_state else 0.05)
                cooldown = self.rng.uniform(0.04, 0.10)
            self._schedule_step(leg, ix, iy, delay=delay, force_fast=fast_state or skitter)
            leg.step_cooldown = cooldown
            used.add(i)
            slots -= 1

        if used:
            self.turn_rehome_pressure = max(0.0, self.turn_rehome_pressure - 0.18)

    def _update_feelers(self, dt: float, moving: bool, turning: bool) -> None:
        """Reach a front leg out to tap what the spider is attending to and wave
        the pedipalps, the way a wandering spider feels an object.

        Only runs in calm, attentive states so it never fights hunting, feeding,
        web work, or play.  It raises floors on the existing ``catch_blend`` and
        ``inspect_intent`` channels, which the renderer already turns into a
        front-leg reach and palp motion; pulsing them produces a tap-tap probe.
        """
        antenna_cfg = self._appearance("antennae", {})
        # Tarantula pedipalps should not inherit the generic cursor-probing
        # behavior used by insect-like feelers. Their behavior channels are
        # raised by explicit inspect/catch/aim actions instead, so a passive
        # mouse position cannot repeatedly pull the hands out of their rest pose.
        if (
            isinstance(antenna_cfg, dict)
            and str(antenna_cfg.get("style", "")).strip().lower() == "tarantula_hand_palps"
        ):
            self._feeler_pulse = 0.0
            self._feeler_pulse_t = 0.0
            return
        if self.state not in ("Idle", "Wander", "Observe", "Inspect", "Alert", "Approach"):
            self._feeler_pulse = 0.0
            self._feeler_pulse_t = 0.0
            return
        # Feeling is a slow, careful act; do not probe while scurrying quickly.
        if self.current_speed > 70.0:
            self._feeler_pulse = 0.0
            self._feeler_pulse_t = 0.0
            return
        focus = clamp(self.focus_strength, 0.0, 1.0)
        attentive = self.state in ("Observe", "Inspect")
        if focus < 0.25 and not attentive:
            self._feeler_pulse = 0.0
            self._feeler_pulse_t = 0.0
            return
        d = math.hypot(self.focus_x - self.x, self.focus_y - self.y)
        if d > self.size * 7.0 and not attentive:
            self._feeler_pulse = 0.0
            self._feeler_pulse_t = 0.0
            return

        if self._feeler_pulse_t > 0.0:
            # Mid-pulse: advance the tap envelope (a smooth reach-out / draw-back).
            self._feeler_pulse_t = max(0.0, self._feeler_pulse_t - dt)
            phase = 1.0 - (self._feeler_pulse_t / max(1e-3, self._feeler_pulse_dur))
            self._feeler_pulse = math.sin(math.pi * clamp(phase, 0.0, 1.0))
        else:
            self._feeler_pulse = 0.0
            self._feeler_clock -= dt
            if self._feeler_clock <= 0.0:
                self._feeler_pulse_dur = self.rng.uniform(0.45, 0.85)
                self._feeler_pulse_t = self._feeler_pulse_dur
                self._feeler_clock = self.rng.uniform(0.5, 1.5)

        if self._feeler_pulse > 0.001:
            # Reach the front legs toward the object and let the palps probe it.
            # Kept modest so the front legs tap rather than overstretch.
            self.catch_point = (self.focus_x, self.focus_y)
            self.catch_blend = max(self.catch_blend, self._feeler_pulse * 0.5)
            self.inspect_intent = max(self.inspect_intent, self._feeler_pulse * 0.9)

