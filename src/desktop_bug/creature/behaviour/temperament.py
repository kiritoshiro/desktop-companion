"""Which behaviour modules a spider acts on, and the movement each
    temperament flavours: drifting, hopping, observing, fleeing.

Split out of the single ``behaviour.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import TYPE_CHECKING, Tuple

if TYPE_CHECKING:
    from ..core import Creature

from ...support.math_utils import (
    angle_to,
    clamp,
    clamp_point,
    distance,
    rand_range,
)
from ...content.personality_profiles import behaviour_modules_for
from ..constants import (
    smootherstep,
)


class TemperamentMixin:
    """Which behaviour modules a spider acts on, and the movement each"""

    def _behaviour_modules(self) -> frozenset[str]:
        """Behaviour modules this personality selects (DC-19, C5).

        Computed fresh from ``self.personality`` rather than cached: the
        settings window can rewrite a live creature's personality dict, and
        this is cheap (a handful of dict lookups). Job-linked modules
        (a Hunter job, a Scout job) and the runtime skill gate stay separate,
        dynamic checks at each call site below, exactly as the personality-id
        branches this replaces used to check ``job_id``/``has_skill``
        themselves alongside the personality flags this now replaces.
        """
        return behaviour_modules_for(self.personality)

    def _has_behaviour_module(self, module_id: str) -> bool:
        return module_id in self._behaviour_modules()

    def _acts_as_hunter(self) -> bool:
        return (self.job_id == "hunter" or self._has_behaviour_module("hunter")) and self.has_skill("chase")

    def _acts_as_jumper(self) -> bool:
        return self._has_behaviour_module("jumper") and self.has_skill("jump")

    def _acts_as_observer(self) -> bool:
        return (self.job_id == "scout" or self._has_behaviour_module("observer")) and self.has_skill("observe")

    def _acts_as_nope(self) -> bool:
        return self._has_behaviour_module("nope") and self.has_skill("run_away")

    def _acts_as_drifter(self) -> bool:
        return self._has_behaviour_module("drifter") and self.has_skill("drift")

    def _acts_as_webber(self) -> bool:
        return (self.job_id == "webber" or self._has_behaviour_module("webber")) and self.has_skill("weave_web")

    def _acts_as_web_shooter(self) -> bool:
        return self._has_behaviour_module("web_shooter") and (self.has_skill("shoot_web") or self.has_skill("wall_web"))

    def _drift_state_multiplier(self, *, inertia: bool = False) -> float:
        if not self._acts_as_drifter():
            return 0.0
        if inertia:
            return 1.18
        return {
            "Approach": 0.58,
            "Chase": 0.76,
            "Retreat": 0.82,
            "Dragged": 0.82,
            "Startled": 0.92,
            "Wander": 0.34,
            "Zoom": 0.82,
            "DriftRun": 1.14,
            "Play": 0.62,
        }.get(self.state, 0.0)

    def _is_drift_sliding(self) -> bool:
        if not self._acts_as_drifter():
            return False
        if self.state == "DriftRun" and getattr(self, "drift_run_phase", "charge") == "slide":
            return True
        return (
            abs(float(getattr(self, "last_drift_amount", 0.0))) > 0.16
            and float(getattr(self, "drift_momentum", 0.0)) > float(self.personality.get("drift_breakaway", 0.46))
        )

    def _prime_drift(self, boost: float = 0.5, direction: float | None = None) -> None:
        if not self._acts_as_drifter():
            return
        if direction is not None:
            self.drift_dir = 1.0 if direction >= 0.0 else -1.0
        self.drift_boost = max(float(getattr(self, "drift_boost", 0.0)), clamp(boost, 0.0, 1.6))
        self.drift_flip_timer = max(
            float(getattr(self, "drift_flip_timer", 0.0)),
            rand_range(self.personality.get("drift_switch_time"), 1.15, 2.4, rng=self.rng),
        )
        self.wiggle_burst = max(self.wiggle_burst, 0.16 + self.drift_boost * 0.10)

    def _drift_amount(self, dt: float, speed: float, *, inertia: bool = False) -> float:
        mult = self._drift_state_multiplier(inertia=inertia)
        if mult <= 0.0 or (self.motion_paused and self.state != "Dragged"):
            self.last_drift_amount = 0.0
            self.drift_momentum = max(0.0, float(getattr(self, "drift_momentum", 0.0)) - dt * 1.8)
            self.drift_lean += (0.0 - float(getattr(self, "drift_lean", 0.0))) * (1.0 - math.exp(-dt * 7.0))
            return 0.0

        min_speed = float(self.personality.get("drift_min_speed", 62.0))
        full_speed = max(min_speed + 1.0, float(self.personality.get("drift_full_speed", 185.0)))
        speed01 = clamp((float(speed) - min_speed) / (full_speed - min_speed), 0.0, 1.0)
        target_momentum = smootherstep(speed01)
        if inertia:
            target_momentum = max(target_momentum, clamp(float(speed) / max(1.0, full_speed), 0.0, 1.0))

        build = float(self.personality.get("drift_momentum_build", 1.65)) * (1.4 if inertia else 1.0)
        decay = float(self.personality.get("drift_momentum_decay", 1.15))
        momentum = float(getattr(self, "drift_momentum", 0.0))
        if target_momentum > momentum:
            momentum = min(target_momentum, momentum + build * dt)
        else:
            momentum = max(target_momentum, momentum - decay * dt)
        self.drift_momentum = clamp(momentum, 0.0, 1.0)

        breakaway = float(self.personality.get("drift_breakaway", 0.46))
        if self.drift_momentum < breakaway:
            self.drift_phase += dt * (1.2 + speed01 * 1.4)
            self.drift_boost = max(0.0, float(getattr(self, "drift_boost", 0.0)) - dt * float(self.personality.get("drift_boost_decay", 1.15)))
            self.last_drift_amount = 0.0
            self.drift_lean += (0.0 - float(getattr(self, "drift_lean", 0.0))) * (1.0 - math.exp(-dt * 5.0))
            return 0.0

        slide01 = smootherstep((self.drift_momentum - breakaway) / max(0.001, 1.0 - breakaway))
        self.drift_phase += dt * (float(self.personality.get("drift_wave_speed", 2.4)) + speed01 * 1.9)
        self.drift_flip_timer -= dt
        if self.drift_flip_timer <= 0.0:
            # Drifts should hold an arc.  Direction changes are rare and read as
            # deliberate counter-steer, not twitchy foot grip.
            if self.rng.random() < float(self.personality.get("drift_flip_chance", 0.18)):
                self.drift_dir *= -1.0
                self.wiggle_burst = max(self.wiggle_burst, 0.34)
            self.drift_flip_timer = rand_range(self.personality.get("drift_switch_time"), 1.15, 2.4, rng=self.rng)

        slip_key = "drift_throw_slip" if inertia else "drift_slip"
        base = float(self.personality.get(slip_key, self.personality.get("drift_slip", 0.48)))
        wobble = 0.86 + 0.14 * math.sin(self.drift_phase)
        boost = 1.0 + float(getattr(self, "drift_boost", 0.0)) * 0.24
        amount = self.drift_dir * base * mult * slide01 * wobble * boost
        self.drift_boost = max(0.0, float(getattr(self, "drift_boost", 0.0)) - dt * float(self.personality.get("drift_boost_decay", 1.15)))
        self.last_drift_amount = clamp(amount, -0.82, 0.82)
        lean_target = self.last_drift_amount * (0.55 + 0.45 * slide01)
        self.drift_lean += (lean_target - float(getattr(self, "drift_lean", 0.0))) * (1.0 - math.exp(-dt * 6.5))
        if abs(self.last_drift_amount) > 0.14:
            self.drift_slide_timer = 0.20
        else:
            self.drift_slide_timer = max(0.0, float(getattr(self, "drift_slide_timer", 0.0)) - dt)
        return self.last_drift_amount

    def _guard_drifter_travel_direction(self, move_x: float, move_y: float) -> Tuple[float, float]:
        """Keep an intentional slide from turning into visible reverse walking.

        DriftRun deliberately lets velocity lag the body during a low-grip arc,
        but an old carried velocity can briefly point behind the creature after
        the run changes corner or reverses its curve.  A spider may crab slightly
        while sliding; it should not travel more than a bounded angle behind its
        facing direction.  Clamp only that pathological case and leave ordinary
        sideways drift untouched.
        """
        if self.state != "DriftRun" or not self._acts_as_drifter():
            return move_x, move_y
        travel_length = math.hypot(move_x, move_y)
        if travel_length <= 1e-5:
            return move_x, move_y
        fx, fy, rx, ry = self._basis()
        forward = (move_x * fx + move_y * fy) / travel_length
        max_angle = math.radians(78.0)
        if forward >= math.cos(max_angle):
            return move_x, move_y
        side = (move_x * rx + move_y * ry) / travel_length
        side_sign = 1.0 if side >= 0.0 else -1.0
        corrected_angle = side_sign * max_angle
        return (
            fx * math.cos(corrected_angle) + rx * math.sin(corrected_angle),
            fy * math.cos(corrected_angle) + ry * math.sin(corrected_angle),
        )

    def _cursor_speed(self) -> float:
        return math.hypot(self.prev_cursor_vx, self.prev_cursor_vy)

    def _cursor_is_still_for_observe(self) -> bool:
        # When the manager substitutes a fly for the cursor, use the fly's
        # explicit stop/walk state instead of a smoothed pixel-speed estimate.
        # That makes Hunter stalking switch exactly with the prey's movement.
        if self._hunting_prey:
            prey = self._prey
            if prey is not None:
                # A loose fly's stop-and-go motion controls Hunter stalking: the
                # spider freezes whenever the fly pauses or turns. Once prey is
                # trapped, however, that rule must no longer hold the spider at
                # range. The webbed fly cannot flee, so Trappers/Hunters should
                # immediately close in and let the manager resolve the feeding
                # contact.
                if bool(getattr(prey, "trapped", False)):
                    return False
                return not bool(getattr(prey, "is_moving", False))
        return self._cursor_speed() <= float(self.personality.get("observe_cursor_speed", 24.0))

    def _hunter_approach_speed(self, dist_to_cursor: float, reaction: float) -> float:
        pair = self.personality.get("hunt_approach_speed", [70.0, 185.0])
        if isinstance(pair, (list, tuple)) and len(pair) >= 2:
            far_speed = float(pair[0])
            near_speed = float(pair[1])
        else:
            far_speed, near_speed = 70.0, 185.0
        closeness = 1.0 - clamp(dist_to_cursor / max(1.0, reaction), 0.0, 1.0)
        return (far_speed + (near_speed - far_speed) * smootherstep(closeness)) * self._speed_mult()

    def _observer_radius_bounds(self) -> Tuple[float, float]:
        pair = self.personality.get("observe_radius_mult", [4.2, 6.8])
        if isinstance(pair, (list, tuple)) and len(pair) >= 2:
            lo = float(pair[0]) * self.size
            hi = float(pair[1]) * self.size
        else:
            lo = self.size * 4.2
            hi = self.size * 6.8
        lo = max(self.size * 2.6, lo)
        hi = max(lo + self.size * 0.9, hi)
        return lo, hi

    def _observer_anchor(self, mx: float, my: float) -> Tuple[float, float, "Creature" | None] | None:
        if not self._acts_as_observer():
            return None
        reaction = float(self.personality.get("reaction_radius", 360))
        cursor_range = reaction * float(self.personality.get("observe_cursor_range_mult", 1.18))
        dist_to_cursor = distance(self.x, self.y, mx, my)
        social_range = max(self.size * 12.0, reaction * float(self.personality.get("observe_social_range_mult", 0.95)))
        social_pref = clamp(float(self.personality.get("observe_social_preference", 0.66)), 0.0, 1.0)
        mate = self._find_social_target(social_range)
        if mate is not None:
            dist_to_mate = distance(self.x, self.y, mate.x, mate.y)
            if dist_to_cursor > cursor_range or dist_to_mate <= dist_to_cursor * 1.12 or self.rng.random() < social_pref:
                return mate.x, mate.y, mate
        if dist_to_cursor < cursor_range:
            return mx, my, None
        return None

    def _maybe_jumper_hop(self, dt: float, after: str, toward: Tuple[float, float]) -> bool:
        if not self.has_skill("jump") or not self._acts_as_jumper() or self.airborne or self.motion_paused:
            return False
        if self.speed <= 1.0 and self.current_speed <= 8.0:
            return False
        self.hop_timer -= dt
        if self.hop_timer > 0.0:
            return False
        self.hop_timer = rand_range(self.personality.get("hop_interval"), 0.18, 0.42, rng=self.rng)
        power = rand_range(self.personality.get("hop_power"), 0.34, 0.58, rng=self.rng)
        self.enter_coil(after=after, power=power, toward=toward)
        return True

    def _cursor_triggers_nope_escape(self, mx: float, my: float, cursor_vx: float, cursor_vy: float) -> bool:
        if not self._acts_as_nope() or self.dragging or self.airborne or self.nope_cooldown > 0.0:
            return False
        if self.state in ("Jump", "Land", "Retreat", "Startled", "Dragged"):
            return False
        dist_to_cursor = distance(self.x, self.y, mx, my)
        trigger_radius = float(self.personality.get("nope_trigger_radius", self.personality.get("reaction_radius", 360)))
        if dist_to_cursor > trigger_radius:
            return False
        near_radius = float(self.personality.get("nope_near_radius", trigger_radius * 0.62))
        if dist_to_cursor <= near_radius:
            return True
        cursor_speed = math.hypot(cursor_vx, cursor_vy)
        speed_threshold = float(self.personality.get("nope_cursor_speed", 45.0))
        if cursor_speed < speed_threshold:
            return False
        # Positive score means the cursor velocity points toward the spider.  A low
        # threshold makes Nope react to a visible approach instead of waiting for a
        # high-speed jab like the regular threat reflex.
        toward_x = self.x - mx
        toward_y = self.y - my
        toward_len = max(1.0, math.hypot(toward_x, toward_y))
        approach_score = (cursor_vx * toward_x + cursor_vy * toward_y) / (max(1.0, cursor_speed) * toward_len)
        return approach_score > float(self.personality.get("nope_toward_score", 0.12))

    def enter_nope_escape(self, mx: float, my: float, *, continuing: bool = False) -> None:
        """Rapid backwards zigzag jump chain used by the Nope personality."""
        if not self._phase_allowed("run_away") or not self.has_skill("run_away"):
            self.enter_alert(mx, my)
            return
        if not self._phase_allowed("jump") or not self.has_skill("jump"):
            self.enter_retreat(mx, my)
            return

        if not continuing:
            count_pair = self.personality.get("nope_jump_count", [3, 5])
            if isinstance(count_pair, (list, tuple)) and len(count_pair) >= 2:
                self.nope_repeats = self.rng.randint(int(count_pair[0]), int(count_pair[1]))
            else:
                self.nope_repeats = int(count_pair) if count_pair is not None else 4
            self.nope_zigzag_dir = self.rng.choice((-1.0, 1.0))
            self.mood.bump(arousal=0.6, valence=-0.22, curiosity=-0.12)
        else:
            self.nope_repeats = max(0, self.nope_repeats - 1)

        away = math.atan2(self.y - my, self.x - mx)
        if distance(self.x, self.y, mx, my) < 1.0:
            away = self.heading + math.pi
        self.nope_zigzag_dir *= -1.0
        jump_dist = rand_range(self.personality.get("nope_jump_distance"), self.size * 4.5, self.size * 8.0, rng=self.rng)
        zigzag = rand_range(self.personality.get("nope_zigzag_distance"), self.size * 2.2, self.size * 4.6, rng=self.rng)
        perp = away + math.pi * 0.5
        target_x = self.x + math.cos(away) * jump_dist + math.cos(perp) * zigzag * self.nope_zigzag_dir
        target_y = self.y + math.sin(away) * jump_dist + math.sin(perp) * zigzag * self.nope_zigzag_dir
        target_x, target_y = clamp_point(target_x, target_y, self.margin, self.screen_w, self.screen_h)

        # Keep the body aimed at the scary cursor while the landing point is behind
        # it, so the motion reads as a panicked backwards hop rather than a chase.
        face_cursor = angle_to(self.x, self.y, mx, my)
        self.heading = face_cursor
        self.target_heading = face_cursor
        self.social_target = None
        self.wiggle_burst = max(self.wiggle_burst, 0.85)
        self._launch_jump(target_x, target_y, kind="escape", after="nope")

