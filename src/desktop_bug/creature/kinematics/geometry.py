"""Body/world frames, leg sectors, reach limits and constraints.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import Tuple

from ...support.math_utils import (
    clamp,
)

from .legstate import LegState


class LegGeometryMixin:
    """Body/world frames, leg sectors, reach limits and constraints."""

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

