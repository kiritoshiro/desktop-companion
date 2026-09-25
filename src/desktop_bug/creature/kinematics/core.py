"""Leg solver, gait timing, and stepping.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

import math

from ...support.math_utils import (
    clamp,
)


from .geometry import LegGeometryMixin
from .gait import GaitConfigMixin
from .legs import LegPlacementMixin
from .roll import RollMixin
from .movement import BodyMovementMixin
from .stepping import SteppingMixin
from .spider_gait import SpiderGaitMixin


class KinematicsMixin(
    LegGeometryMixin,
    GaitConfigMixin,
    LegPlacementMixin,
    RollMixin,
    BodyMovementMixin,
    SteppingMixin,
    SpiderGaitMixin,
):
    """Leg IK, gait scheduling, and grounded-locomotion body solving."""

    def _walks_while_facing_elsewhere(self) -> bool:
        """Whether this spider is walking one way while looking another.

        Both locomotion paths ordinarily overwrite ``target_heading`` with
        the direction of travel and step along the body's facing.  Two states
        need them not to: an Observer strafes to keep what it is watching in
        view, and a guard on station (DC-42) holds its post facing outwards
        while tracking a little way along its patrol line.  Sharing one
        predicate keeps the legacy body path and the spider gait path from
        disagreeing about which those are.
        """
        if self.state == "Observe" and self._acts_as_observer():
            return True
        if getattr(self, "reverse_walk", False):
            # The player backing up (turn-and-walk controls).
            return True
        return self.state == "JobPatrol" and getattr(self, "job_facing", None) is not None

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

