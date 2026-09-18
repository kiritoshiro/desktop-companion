"""The personality state machine: every `enter_*`/`_update_*` state method.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, Tuple

if TYPE_CHECKING:
    from .core import Creature

from ..math_utils import (
    angle_to,
    clamp,
    clamp_point,
    distance,
    rand_range,
    smoothstep,
)
from ..phase_scheduler import phase_id_for_state
from .constants import (
    JOB_PREEMPTING_STATES,
    JOB_STATES,
    smootherstep,
)

class BehaviourMixin:
    """State entry points and per-state updates."""

    def enter_idle(self) -> None:
        self.state = "Idle"
        self.speed = 0.0
        self.motion_paused = False
        self.state_timer = rand_range(self.personality.get("idle_time"), 1.0, 3.0, rng=self.rng)

    def enter_alert(self, mx: float, my: float) -> None:
        self.state = "Alert"
        self.speed = 0.0
        self.motion_paused = False
        self.target_x = mx
        self.target_y = my
        self.state_timer = rand_range(self.personality.get("alert_time"), 0.3, 0.9, rng=self.rng)

    def enter_approach(self, mx: float, my: float) -> None:
        if not self._phase_allowed("approach") or not self.has_skill("approach"):
            self.enter_alert(mx, my)
            return
        self.target_x = mx
        self.target_y = my
        self.state = "Approach"
        self.motion_paused = False
        reaction = float(self.personality.get("reaction_radius", 360))
        if self._is_hunter_personality():
            self.speed = self._hunter_approach_speed(distance(self.x, self.y, mx, my), reaction)
        else:
            self.speed = 52.0 * self._speed_mult()
        self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3, rng=self.rng)
        self._prime_drift(0.35)

    def enter_chase(self, mx: float, my: float) -> None:
        if not self._phase_allowed("chase") or not self.has_skill("chase"):
            if self._phase_allowed("approach") and self.has_skill("approach"):
                self.enter_approach(mx, my)
            else:
                self.enter_alert(mx, my)
            return
        self.state = "Chase"
        self.motion_paused = False
        self.target_x = mx + self.rng.uniform(-16.0, 16.0)
        self.target_y = my + self.rng.uniform(-16.0, 16.0)
        base_speed = float(self.personality.get("hunt_chase_speed", 150.0)) if self._is_hunter_personality() else 112.0
        self.speed = base_speed * self._speed_mult()
        self.chase_timer = float(self.personality.get("chase_persistence", 2.5))
        if self._is_hunter_personality():
            self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.06, 0.14, rng=self.rng)
        else:
            self.state_timer = self.rng.uniform(0.12, 0.28)
        self._prime_drift(0.85)

    def enter_retreat(self, mx: float, my: float) -> None:
        if not self._phase_allowed("run_away") or not self.has_skill("run_away"):
            self.enter_alert(mx, my)
            return
        angle_away = math.atan2(self.y - my, self.x - mx)
        distance_away = rand_range(self.personality.get("retreat_distance"), 220.0, 380.0, rng=self.rng)
        self.target_x = self.x + math.cos(angle_away) * distance_away
        self.target_y = self.y + math.sin(angle_away) * distance_away
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.state = "Retreat"
        self.motion_paused = False
        self.speed = 148.0 * self._speed_mult()
        self.state_timer = rand_range(self.personality.get("retreat_time"), 0.6, 1.2, rng=self.rng)
        self._prime_drift(0.95)

    def enter_startled(self, mx: float, my: float) -> None:
        """Brief startled mode used after being grabbed or thrown."""
        self.state = "Startled"
        self.motion_paused = False
        self.startled_timer = max(self.startled_timer, 1.15)
        away = math.atan2(self.y - my, self.x - mx)
        throw_speed = math.hypot(self.inertia_vx, self.inertia_vy)
        if throw_speed > 60.0:
            # A throw is carried by friction alone. Projecting a target further
            # along the throw heading made the spider *walk* the rest of the way
            # once the slide ended, for several seconds, which read as running
            # away in the thrown direction rather than being thrown.
            heading = math.atan2(self.inertia_vy, self.inertia_vx)
            distance_out = 0.0
            # A beat on the spot after the skid: plant, gather itself, then
            # react. Without it the startled scurry begins the instant friction
            # wins and the stop is never visible.
            self.throw_recovery = self.rng.uniform(0.34, 0.52)
        else:
            heading = away
            distance_out = self.rng.uniform(80.0, 160.0)
            self.throw_recovery = 0.0
        self.target_x = self.x + math.cos(heading) * distance_out
        self.target_y = self.y + math.sin(heading) * distance_out
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.target_heading = heading
        self.speed = 74.0 * self._speed_mult()
        self.state_timer = self.rng.uniform(0.65, 1.15)
        throw_boost = clamp(math.hypot(self.inertia_vx, self.inertia_vy) / 580.0, 0.45, 1.45)
        self._prime_drift(throw_boost)

    def enter_wander(self) -> None:
        if not self._phase_allowed("wander") or not self.has_skill("wander"):
            self.enter_idle()
            return
        self.state = "Wander"
        self.motion_paused = False
        angle = self.rng.uniform(-math.pi, math.pi)
        dist = self.rng.uniform(120.0, 320.0)
        self.target_x = self.x + math.cos(angle) * dist
        self.target_y = self.y + math.sin(angle) * dist
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.speed = 38.0 * self._speed_mult()
        self.state_timer = self.rng.uniform(1.4, 3.2)
        self._prime_drift(0.25)

    def _update_job_state(self, dt: float, mx: float, my: float) -> bool:
        """Apply a Builder/Guard work intent before ordinary personality FSM logic."""
        if self.job_id not in ("builder", "guard"):
            return False
        mode = str(getattr(self, "job_mode", "idle") or "idle")
        target = getattr(self, "job_target", None)
        self.job_busy = self._job_outranked_by_personality(mode)
        if self.job_busy or mode == "idle":
            # Either temperament is mid-something more urgent, or this spider is
            # off shift. Hand it back rather than overwriting the state it is in.
            self._release_job_state()
            return False
        if mode == "build_travel" and target is not None:
            self.state = "JobTravel"
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 52.0 * self._speed_mult()
            return True
        if mode == "build":
            self.state = "JobBuild"
            self.motion_paused = True
            self.speed = 0.0
            self.target_x, self.target_y = self.x, self.y
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 0.9)
            return True
        if mode == "patrol" and target is not None:
            self.state = "JobPatrol"
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 38.0 * self._speed_mult()
            return True
        if mode == "guard_alert" and target is not None:
            self.state = "JobGuardAlert"
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 72.0 * self._speed_mult()
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 2.0)
            return True
        self._release_job_state()
        return False

    def _job_outranked_by_personality(self, mode: str) -> bool:
        """Return whether temperament currently beats this spider's job."""
        if self.state in JOB_PREEMPTING_STATES:
            return True
        # A guard answering an intruder at its own base outranks an ordinary
        # hunt, but nothing outranks fleeing or a jump already in the air.
        if self._hunting_prey and mode != "guard_alert":
            return True
        return False

    def _release_job_state(self) -> bool:
        """Hand a spider back to the personality FSM when its job lets go.

        ``JOB_STATES`` have no branch in ``_update_state``, so a spider left in
        one would fall through the whole dispatch and hold its last speed and
        ``motion_paused`` flag indefinitely.
        """
        if self.state not in JOB_STATES:
            return False
        self.aim_intent = 0.0
        self.enter_idle()
        return True

    def _is_hunter_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self.job_id == "hunter" or self._personality_flag("mouse_hunter") or pid == "hunter") and self.has_skill("chase")

    def _is_jumper_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self._personality_flag("constant_small_hops") or pid == "jumper") and self.has_skill("jump")

    def _is_observer_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self.job_id == "scout" or self._personality_flag("horizontal_orbit_observer") or pid == "observer") and self.has_skill("observe")

    def _is_nope_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self._personality_flag("nope_escape") or pid == "nope") and self.has_skill("run_away")

    def _is_drifter_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (
            self._personality_flag("drifter")
            or self._personality_flag("drift_movement")
            or pid == "drifter"
        ) and self.has_skill("drift")

    def _is_webber_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (
            self.job_id == "webber"
            or
            self._personality_flag("web_weaver")
            or self._personality_flag("webber")
            or pid in ("webber", "weaver")
        ) and self.has_skill("weave_web")

    def _is_web_shooter_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (
            self._personality_flag("web_shooter")
            or pid in ("trapper", "webslinger")
        ) and (self.has_skill("shoot_web") or self.has_skill("wall_web"))

    def _drift_state_multiplier(self, *, inertia: bool = False) -> float:
        if not self._is_drifter_personality():
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
        if not self._is_drifter_personality():
            return False
        if self.state == "DriftRun" and getattr(self, "drift_run_phase", "charge") == "slide":
            return True
        return (
            abs(float(getattr(self, "last_drift_amount", 0.0))) > 0.16
            and float(getattr(self, "drift_momentum", 0.0)) > float(self.personality.get("drift_breakaway", 0.46))
        )

    def _prime_drift(self, boost: float = 0.5, direction: float | None = None) -> None:
        if not self._is_drifter_personality():
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
        if self.state != "DriftRun" or not self._is_drifter_personality():
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
        if not self._is_observer_personality():
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
        if not self.has_skill("jump") or not self._is_jumper_personality() or self.airborne or self.motion_paused:
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
        if not self._is_nope_personality() or self.dragging or self.airborne or self.nope_cooldown > 0.0:
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

    def _set_focus(self, x: float, y: float, strength: float = 1.0) -> None:
        self.focus_x = x
        self.focus_y = y
        self.focus_strength = clamp(strength, 0.0, 1.0)

    def enter_inspect(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        if not self._phase_allowed("inspect") or not self.has_skill("inspect"):
            self.enter_idle()
            return
        self.state = "Inspect"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.speed = 46.0 * self._speed_mult()
        self.state_timer = self.rng.uniform(2.2, 4.8)
        self.inspect_phase = "approach"
        self.inspect_clock = 0.0
        self.orbit_dir = self.rng.choice((-1.0, 1.0))
        self.mood.bump(curiosity=0.22, arousal=0.06)

    def enter_observe(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        if not self._phase_allowed("observe") or not self.has_skill("observe"):
            if self._phase_allowed("inspect") and self.has_skill("inspect"):
                self.enter_inspect(tx, ty, target)
            else:
                self.enter_idle()
            return
        self.state = "Observe"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.observe_dir = self.rng.choice((-1.0, 1.0))
        self.observe_clock = 0.0
        lo, hi = self._observer_radius_bounds()
        current = distance(self.x, self.y, tx, ty)
        # Pick a watch point farther along the current focus->spider line, so the
        # first observation move visibly backs away from the observed object instead
        # of choosing a left/right orbit side.
        backoff = self.size * float(self.personality.get("observe_backoff_mult", 1.6))
        self.observe_radius = clamp(max(current + backoff, lo), lo, hi)
        self.observe_vertical_scale = clamp(float(self.personality.get("observe_vertical_scale", 0.58)), 0.35, 1.0)
        self.observe_horizontal_scale = max(1.0, float(self.personality.get("observe_horizontal_scale", 1.18)))
        self.speed = float(self.personality.get("observe_speed", 44.0)) * self._speed_mult()
        self.state_timer = rand_range(self.personality.get("observe_orbit_time"), 2.8, 6.4, rng=self.rng)
        self.mood.bump(curiosity=0.18, arousal=0.04)

    def enter_cuddle(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        if not self._phase_allowed("cuddle") or not self.has_skill("cuddle"):
            self.enter_idle()
            return
        self.state = "Cuddle"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.speed = 40.0 * self._speed_mult()
        self.state_timer = self.rng.uniform(3.0, 6.5)
        self.cuddle_phase = "approach"
        self.cuddle_clock = 0.0
        self.boop_timer = self.rng.uniform(0.4, 0.9)
        self.mood.bump(affection=0.18, valence=0.06)

    def enter_aim(
        self,
        tx: float,
        ty: float,
        target: "Creature" | None = None,
        after: str = "outcome",
        ranging: Tuple[float, float] = (0.6, 1.25),
        abort_chance: float = 0.16,
    ) -> None:
        """Crouch and range a target before a pounce (jumping-spider style)."""
        if (
            not self._phase_allowed("prepare_jump_attack")
            or not self.has_skill("prepare_jump_attack")
            or not self.has_skill("jump")
        ):
            if after == "play" and target is not None and self._phase_allowed("social_play") and self.has_skill("social_play"):
                self.enter_play(target)
            elif self._phase_allowed("chase") and self.has_skill("chase"):
                self.enter_chase(tx, ty)
            elif self._phase_allowed("approach") and self.has_skill("approach"):
                self.enter_approach(tx, ty)
            else:
                self.enter_alert(tx, ty)
            return
        self.state = "Aim"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_heading = angle_to(self.x, self.y, tx, ty)
        self.state_timer = rand_range(ranging, ranging[0], ranging[1], rng=self.rng)
        self.aim_after = after
        self.aim_abort_chance = clamp(abort_chance, 0.0, 0.9)
        self.range_clock = 0.0
        self.range_mode = "waggle"
        self.range_switch = self.rng.uniform(0.18, 0.34)
        self.mood.bump(arousal=0.16, curiosity=0.05)

    def enter_coil(self, after: str = "idle", power: float = 1.0, toward: Tuple[float, float] | None = None) -> None:
        """Quick wind-up for a playful spring/hop."""
        if not self._phase_allowed("jump") or not self.has_skill("jump"):
            if after == "wander":
                self.enter_wander()
            elif after == "approach" and toward is not None and self._phase_allowed("approach"):
                self.enter_approach(toward[0], toward[1])
            elif after == "chase" and toward is not None and self._phase_allowed("chase"):
                self.enter_chase(toward[0], toward[1])
            else:
                self.enter_idle()
            return
        self.state = "Coil"
        self.motion_paused = True
        self.speed = 0.0
        self.state_timer = self.rng.uniform(0.10, 0.20)
        self.coil_after = after
        self.coil_power = clamp(power, 0.4, 1.6)
        self.coil_toward = toward
        self.mood.bump(arousal=0.12, valence=0.05)

    def _launch_jump(
        self,
        tx: float,
        ty: float,
        kind: str = "pounce",
        after: str = "outcome",
        peak: float | None = None,
        duration: float | None = None,
        reach: float = 1.0,
    ) -> None:
        if not self.has_skill("jump"):
            if after == "wander":
                self.enter_wander()
            elif after == "approach":
                self.enter_approach(tx, ty)
            elif after == "chase":
                self.enter_chase(tx, ty)
            else:
                self.enter_idle()
            return
        self.airborne = True
        self.state = "Jump"
        self.motion_paused = False
        self.crouch = 0.0
        self.jump_kind = kind
        self.jump_after = after
        self.jump_t = 0.0

        dx = tx - self.x
        dy = ty - self.y
        dist = math.hypot(dx, dy)
        if kind == "hop":
            # Mostly vertical bounce, small drift.
            land_dist = min(dist, self.size * 0.7) * reach
            ang = math.atan2(dy, dx) if dist > 1.0 else self.heading
            land_x = self.x + math.cos(ang) * land_dist
            land_y = self.y + math.sin(ang) * land_dist
            coil_power = getattr(self, "coil_power", 1.0)
            hop_peak_mult = float(self.personality.get("hop_peak_multiplier", 1.0))
            hop_duration_mult = float(self.personality.get("hop_duration_multiplier", 1.0))
            self.jump_peak = peak if peak is not None else self.size * self.rng.uniform(0.85, 1.25) * coil_power * hop_peak_mult
            self.jump_duration = duration if duration is not None else self.rng.uniform(0.36, 0.5) * hop_duration_mult
        elif kind == "escape":
            # Nope escape: a deliberately long, very fast backward hop.  Heading is
            # already aimed at the mouse in enter_nope_escape; do not rotate toward
            # the landing point here, otherwise it reads like a normal forward jump.
            travel = clamp(dist, self.size * 1.8, self.size * 11.0) * reach
            ang = math.atan2(dy, dx) if dist > 1.0 else self.heading + math.pi
            land_x = self.x + math.cos(ang) * travel
            land_y = self.y + math.sin(ang) * travel
            peak_pair = self.personality.get("nope_jump_peak_mult", [1.4, 2.4])
            if isinstance(peak_pair, (list, tuple)) and len(peak_pair) >= 2:
                peak_mult = self.rng.uniform(float(peak_pair[0]), float(peak_pair[1]))
            else:
                peak_mult = float(peak_pair) if peak_pair is not None else 1.8
            self.jump_peak = peak if peak is not None else self.size * peak_mult
            self.jump_duration = duration if duration is not None else rand_range(self.personality.get("nope_jump_duration"), 0.14, 0.22, rng=self.rng)
        else:
            # Pounce: land a touch beyond the target so it reads as committing onto it.
            overshoot = self.size * 0.4
            travel = clamp(dist + overshoot, self.size * 1.2, self.size * 9.0) * reach
            ang = math.atan2(dy, dx) if dist > 1.0 else self.heading
            land_x = self.x + math.cos(ang) * travel
            land_y = self.y + math.sin(ang) * travel
            self.target_heading = ang
            self.heading = ang
            self.jump_peak = peak if peak is not None else clamp(travel * 0.32, self.size * 0.9, self.size * 2.6)
            self.jump_duration = duration if duration is not None else clamp(travel / 520.0 + 0.30, 0.32, 0.6)

        land_x, land_y = clamp_point(land_x, land_y, self.margin * 0.5, self.screen_w, self.screen_h)
        self.jump_from = (self.x, self.y)
        self.jump_to = (land_x, land_y)
        self.catch_point = (tx, ty)
        # Push-off briefly extends the legs; suppress gait scheduling while airborne.
        for leg in self.legs:
            leg.stepping = False
            leg.pending_step = False

    def enter_land(self, after: str = "idle") -> None:
        self.airborne = False
        self.jump_z = 0.0
        self.state = "Land"
        self.motion_paused = False
        self.speed = 0.0
        # A tarantula's broad body should absorb a landing without becoming a
        # paper-thin pancake when it is facing sideways.  The generic squash
        # value was designed for small round creatures; on this elongated
        # spider it made a post-pounce rotation look like a broken flattened
        # body.  Keep a restrained compression while preserving the recovery.
        self.squash = 0.88 if self._spider_gait_config() is not None else 0.66
        self.land_recover = 0.22
        if after == "nope":
            self.state_timer = rand_range(self.personality.get("nope_land_pause"), 0.02, 0.05, rng=self.rng)
        else:
            self.state_timer = 0.16
        self.land_after = after
        self._panic_rehome_legs(0.85)
        self.mood.bump(arousal=0.05)

    def enter_catch(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        self.state = "Catch"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.catch_point = (tx, ty)
        self.state_timer = self.rng.uniform(0.45, 0.72)
        self.catch_resolved = False
        self.mood.bump(arousal=0.2)

    def enter_feed(self, tx: float, ty: float) -> None:
        """Pounce-and-pin a caught fly, then a brief satisfied feeding pause.

        Called by the manager the instant a spider reaches a fly.  It clears any
        prey-chasing transient, faces the kill, and plays a short crouched
        "munch" before the spider straightens up and moves on, content.
        """
        self.airborne = False
        self.jump_z = 0.0
        self.state = "Feed"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = None
        self.weaving_web = None
        self.repairing_web = None
        self.web_target = None
        self.target_x, self.target_y = self.x, self.y
        self.target_heading = angle_to(self.x, self.y, tx, ty)
        self._set_focus(tx, ty, 1.0)
        self.catch_point = (tx, ty)
        self.catch_blend = 1.0
        self.crouch = min(1.0, self.crouch + 0.55)
        self.wiggle_burst = max(self.wiggle_burst, 0.9)
        # A short, intense bout of working the prey with the front legs, then a
        # settling beat.  The body stays put; the legs do the visible work.
        self._feed_frenzy = self.rng.uniform(0.5, 0.75)
        self._feed_anchor = (self.x, self.y)
        self._feed_paw = 0.0
        self.state_timer = self._feed_frenzy + self.rng.uniform(0.4, 0.6)
        self.mood.bump(valence=0.45, arousal=0.3, affection=0.05, curiosity=0.05)

    def enter_play(self, target: "Creature", role: str | None = None) -> None:
        if not self._phase_allowed("social_play") or not self.has_skill("social_play"):
            if self._phase_allowed("inspect") and self.has_skill("inspect"):
                self.enter_inspect(target.x, target.y, target)
            else:
                self.enter_idle()
            return
        self.state = "Play"
        self.motion_paused = False
        self.social_target = target
        if role is None:
            role = "chase" if self.index <= getattr(target, "index", self.index) else "flee"
        self.social_role = role
        self.speed = (92.0 if role == "chase" else 104.0) * self._speed_mult()
        self.state_timer = self.rng.uniform(2.6, 5.5)
        self.play_clock = 0.0
        self.play_lookback = self.rng.uniform(0.5, 1.1)
        self._prime_drift(0.55)
        self.mood.bump(valence=0.16, arousal=0.2, curiosity=0.08)

    def enter_zoomies(self) -> None:
        if not self._phase_allowed("zoomies") or not self.has_skill("zoomies"):
            self.enter_idle()
            return
        self.state = "Zoom"
        self.motion_paused = False
        angle = self.rng.uniform(-math.pi, math.pi)
        dist = self.rng.uniform(120.0, 260.0)
        self.target_x = self.x + math.cos(angle) * dist
        self.target_y = self.y + math.sin(angle) * dist
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.speed = 138.0 * self._speed_mult()
        self.state_timer = self.rng.uniform(0.35, 0.7)
        self.zoom_repeats = self.rng.randint(1, 3)
        self._prime_drift(1.0)
        self.mood.bump(valence=0.14, arousal=0.24)

    def _reset_drift_run_cooldown(self) -> None:
        self.drift_run_cooldown = rand_range(self.personality.get("drift_run_interval"), 4.5, 10.5, rng=self.rng)

    def _should_start_drift_run(self, *, from_idle: bool) -> bool:
        if not self._is_drifter_personality() or self.airborne or self.dragging:
            return False
        if self.state in ("Dragged", "Startled", "Retreat", "Chase", "Jump", "Land", "Roll", "Coil", "Catch"):
            return False
        if float(getattr(self, "drift_run_cooldown", 0.0)) > 0.0:
            return False
        chance_key = "drift_run_start_chance" if from_idle else "drift_run_wander_chance"
        fallback = 0.86 if from_idle else 0.34
        return self.rng.random() < clamp(float(self.personality.get(chance_key, fallback)), 0.0, 1.0)

    def enter_drift_run(self, mode: str | None = None) -> None:
        """Autonomous Drifter burst: fast circular/corner screen drifting."""
        if not self._is_drifter_personality():
            self.enter_idle()
            return

        self.state = "DriftRun"
        self.motion_paused = False
        self.social_target = None
        self.drift_run_dir = self.rng.choice((-1.0, 1.0))
        self.drift_dir = self.drift_run_dir
        self.state_timer = rand_range(self.personality.get("drift_run_time"), 2.2, 4.6, rng=self.rng)
        self.drift_run_turn_rate = rand_range(self.personality.get("drift_circle_turn_rate"), 0.95, 1.85, rng=self.rng)
        self.drift_run_radius = rand_range(self.personality.get("drift_circle_radius"), max(92.0, self.size * 4.0), max(230.0, self.size * 8.0), rng=self.rng)
        self.drift_run_phase = "charge"
        self.drift_run_phase_timer = rand_range(self.personality.get("drift_charge_time"), 0.55, 1.05, rng=self.rng)
        self.drift_momentum = min(float(getattr(self, "drift_momentum", 0.0)), 0.25)
        self.drift_boost = 0.0
        self.last_drift_amount = 0.0

        if mode not in ("circle", "corner"):
            edge_band = self.margin + max(120.0, self.drift_run_radius * 0.85)
            near_edge = (
                self.x < edge_band
                or self.x > self.screen_w - edge_band
                or self.y < edge_band
                or self.y > self.screen_h - edge_band
            )
            corner_chance = clamp(float(self.personality.get("drift_corner_chance", 0.46)) + (0.20 if near_edge else 0.0), 0.0, 0.92)
            mode = "corner" if self.rng.random() < corner_chance else "circle"
        self.drift_run_mode = mode

        radius = max(35.0, self.drift_run_radius)
        if mode == "corner":
            inset = self.margin + radius * 0.65
            corners = [
                (inset, inset),
                (self.screen_w - inset, inset),
                (self.screen_w - inset, self.screen_h - inset),
                (inset, self.screen_h - inset),
            ]
            # Prefer the nearest corner with a little randomness, so it often looks
            # like the spider intentionally skids into the corner it is already near.
            ranked = sorted(enumerate(corners), key=lambda item: distance(self.x, self.y, item[1][0], item[1][1]))
            if len(ranked) > 1 and self.rng.random() < 0.32:
                idx, (cx, cy) = self.rng.choice(ranked[:2])
            else:
                idx, (cx, cy) = ranked[0]
            self.drift_run_corner_index = idx
            self.drift_run_center_x = clamp(cx, self.margin, self.screen_w - self.margin)
            self.drift_run_center_y = clamp(cy, self.margin, self.screen_h - self.margin)
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, self.x, self.y)
            self.drift_run_turn_rate *= 0.86
        else:
            radial = self.rng.uniform(-math.pi, math.pi)
            cx = self.x - math.cos(radial) * radius
            cy = self.y - math.sin(radial) * radius
            low_x = min(self.screen_w * 0.5, self.margin + radius)
            high_x = max(low_x, self.screen_w - self.margin - radius)
            low_y = min(self.screen_h * 0.5, self.margin + radius)
            high_y = max(low_y, self.screen_h - self.margin - radius)
            self.drift_run_center_x = clamp(cx, low_x, high_x)
            self.drift_run_center_y = clamp(cy, low_y, high_y)
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, self.x, self.y)

        self.drift_run_speed = rand_range(self.personality.get("drift_run_speed"), 145.0, 205.0, rng=self.rng)
        self.speed = self.drift_run_speed * self._speed_mult()
        self.wiggle_burst = max(self.wiggle_burst, 0.45)
        self.mood.bump(valence=0.10, arousal=0.32, curiosity=0.08)
        self._reset_drift_run_cooldown()

    def _update_drift_run(self, dt: float, mx: float, my: float) -> None:
        self.motion_paused = False
        self.speed = float(getattr(self, "drift_run_speed", 170.0)) * self._speed_mult()

        # A drift run now has two readable beats: first he digs in and builds
        # momentum, then the legs loosen and the body slides wide through the arc.
        self.drift_run_phase_timer = max(0.0, float(getattr(self, "drift_run_phase_timer", 0.0)) - dt)
        phase = getattr(self, "drift_run_phase", "charge")
        if phase == "charge" and (self.drift_run_phase_timer <= 0.0 or float(getattr(self, "drift_momentum", 0.0)) > 0.54):
            self.drift_run_phase = "slide"
            self.drift_run_phase_timer = rand_range(self.personality.get("drift_slide_time"), 0.95, 1.65, rng=self.rng)
            self._prime_drift(1.18, direction=self.drift_run_dir)
            self.wiggle_burst = max(self.wiggle_burst, 0.42)
            phase = "slide"
        elif phase == "slide" and self.drift_run_phase_timer <= 0.0 and self.state_timer > 0.45:
            self.drift_run_phase = "charge"
            self.drift_run_phase_timer = rand_range(self.personality.get("drift_charge_time"), 0.45, 0.95, rng=self.rng)
            phase = "charge"

        slide01 = smootherstep(float(getattr(self, "drift_momentum", 0.0))) if phase == "slide" else 0.0
        turn_grip = (
            float(self.personality.get("drift_slide_turn_grip", 0.34)) if phase == "slide"
            else float(self.personality.get("drift_charge_turn_grip", 0.62))
        )
        self.drift_run_angle += self.drift_run_dir * self.drift_run_turn_rate * turn_grip * dt

        # Keep aiming ahead along a wide arc.  For corner runs, part of the arc lives
        # outside the desktop bounds, and clamping turns that into a wall/corner skid.
        lead_base = float(self.personality.get("drift_run_lead", 0.52))
        lead = self.drift_run_dir * (lead_base + slide01 * float(self.personality.get("drift_slide_lead_extra", 0.82)))
        wobble = 1.0 + math.sin(self.drift_phase * 0.48) * float(self.personality.get("drift_run_radius_wobble", 0.055))
        radius = max(70.0, self.drift_run_radius * (1.0 + slide01 * 0.18) * wobble)
        tx = self.drift_run_center_x + math.cos(self.drift_run_angle + lead) * radius
        ty = self.drift_run_center_y + math.sin(self.drift_run_angle + lead) * radius

        if self.drift_run_mode == "corner":
            # Overshoot the corner while sliding so the path looks like a skid into
            # the edge, not a tight foot-powered pirouette.
            margin = self.margin * (0.18 if phase == "slide" else 0.45)
        else:
            margin = self.margin
        self.target_x, self.target_y = clamp_point(tx, ty, margin, self.screen_w, self.screen_h)

        tangent_heading = self.drift_run_angle + self.drift_run_dir * math.pi * 0.5
        # During the slide he leans/counter-steers.  The velocity can keep carving
        # the arc while the body turns much more slowly.
        self.target_heading = tangent_heading - self.drift_run_dir * slide01 * float(self.personality.get("drift_countersteer", 0.42))
        self._set_focus(self.target_x, self.target_y, 0.55)

        if self.decision_timer <= 0.0:
            self.decision_timer = self.rng.uniform(0.42, 0.80)
            if phase == "charge" and self.rng.random() < float(self.personality.get("drift_run_flip_chance", 0.07)):
                self.drift_run_dir *= -1.0
                self.drift_dir = self.drift_run_dir
                self.wiggle_burst = max(self.wiggle_burst, 0.36)

        if self.state_timer <= 0.0:
            repeat_chance = float(self.personality.get("drift_run_chain_chance", 0.18))
            if self.rng.random() < repeat_chance:
                next_mode = "corner" if self.drift_run_mode == "circle" and self.rng.random() < 0.48 else None
                self.enter_drift_run(next_mode)
            else:
                self.enter_idle()

    def enter_spring(self, after: str = "idle", power: float = 1.0) -> None:
        """Public helper: a happy spring/hop in place."""
        if not self.has_skill("jump"):
            self.enter_idle()
            return
        self.enter_coil(after=after, power=power, toward=None)

    def enter_roll(self, direction: float | None = None) -> None:
        """A playful tumble: the spider curls up and rolls across the desk.

        The whole body spins through one or two turns while the legs tuck in, and
        it drifts a short way in ``direction`` (random when not given). This is a
        pure happy flourish; it carries no target and resolves back to idle.
        """
        if not self._phase_allowed("roll") or not self.has_skill("roll"):
            self.enter_idle()
            return
        self.state = "Roll"
        self.motion_paused = True
        self.speed = 0.0
        # A roll is a self-contained flourish, not a continuation of the prior
        # walk. Clear translational momentum so the first grounded frame after
        # the roll cannot reuse the old travel direction.
        self.current_speed = 0.0
        self.vel_x = 0.0
        self.vel_y = 0.0
        self.inertia_vx = 0.0
        self.inertia_vy = 0.0
        self.inertia_timer = 0.0
        self.social_target = None
        self.roll_dir = self.rng.choice((-1.0, 1.0))
        # Whole turns. The spin is a draw rotation that is dropped to zero the
        # instant the roll finishes, while the legs re-plant against the
        # unchanged logical heading -- so a fractional turn made the body jump
        # by up to 173 degrees on the last frame and left the legs looking
        # wrong. One or two turns still reads as a tumble; a turn and a half
        # reads as a glitch.
        turns = float(self.rng.randint(1, 2))
        self.roll_total = math.tau * turns
        self.roll_duration = self.rng.uniform(0.7, 1.15)
        self.roll_progress = 0.0
        self.roll_eased = 0.0
        self.roll_spin = 0.0
        self.roll_tuck = 0.0
        # Set here and cleared by whatever ends the roll, so an interrupted
        # tumble is still known to need tidying up. Reading the spin instead is
        # not enough: some callers zero it and leave the feet where they were.
        self._rolling = True
        # A roll is a yaw-like flourish in the top-down view, not a landing.
        # Clear any leftover jump compression so the next frame cannot render
        # a flattened body, especially on the smaller sprite-rig spiders.
        self.squash = 1.0
        self.roll_distance = self.size * self.rng.uniform(1.6, 3.4)
        if direction is None:
            direction = self.rng.uniform(-math.pi, math.pi)
        self.roll_drift_dir = float(direction)
        self.state_timer = self.roll_duration + 0.1
        self.mood.bump(valence=0.2, arousal=0.16, curiosity=0.05)

    def _resolve_pounce_outcome(self) -> None:
        tx, ty = self.catch_point
        target = self.social_target
        target_gone = False
        if target is not None:
            if target.dragging or target not in self.neighbors:
                target_gone = True
            else:
                tx, ty = target.x, target.y
        d = distance(self.x, self.y, tx, ty)
        in_range = d < self.size * 3.2 and not target_gone

        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        m = self.mood
        catch_w = 0.35 + m.arousal * 0.4 + m.curiosity * 0.25 + boldness * 0.2
        cuddle_w = 0.18 + m.affection * 0.95 + m.happy * 0.3
        flee_w = 0.14 + m.sad * 0.6 + (1.0 - boldness) * 0.5 + m.sleepy * 0.2
        if not in_range:
            # Missed the leap: mostly a small frustrated catch-and-miss or a flee.
            catch_w *= 0.6
            cuddle_w *= 0.3
            flee_w += 0.25
        total = catch_w + cuddle_w + flee_w
        roll = self.rng.random() * total
        if roll < catch_w:
            self.enter_catch(tx, ty, target)
        elif roll < catch_w + cuddle_w and in_range:
            self.mood.bump(valence=0.2, affection=0.12)
            self.enter_cuddle(tx, ty, target)
        else:
            self.mood.bump(valence=-0.12, arousal=0.22)
            self.enter_retreat(tx, ty)

    def _find_social_target(self, max_range: float) -> "Creature" | None:
        if not self.allow_social or not self.neighbors:
            return None
        best = None
        best_d = max_range
        for other in self.neighbors:
            if other is self or other.dragging or other.airborne:
                continue
            if getattr(other, "_desktop_fully_hidden", False):
                continue
            if other.state in ("Dragged", "Startled"):
                continue
            d = distance(self.x, self.y, other.x, other.y)
            if d < best_d:
                best_d = d
                best = other
        return best

    def _startle_amount(self) -> float:
        if self.dragging:
            return 1.0
        return clamp(self.startled_timer / 1.15, 0.0, 1.0)

    def _startle_highlight_active(self, startle: float) -> bool:
        """Return whether the startle palette should alter leg colors.

        Dragging keeps a timid expression, but it must not recolor the spider.
        The grab should communicate through its suspended pose only.
        """
        return (
            startle > 0.35
            and not self.dragging
            and self._startle_highlight_suppression <= 0.0
        )

    def _eye_startle_amount(self, startle: float) -> float:
        """Keep a small timid eye reaction without ballooning the eyes."""
        # During a grab ``_startle_amount`` is pinned at 1.0. Cap the visual eye
        # reaction to a small, cute change rather than a frightened exaggeration.
        cap = 0.28 if self.dragging else 0.45
        return clamp(startle, 0.0, 1.0) * cap

    def register_camouflage_touch(self, visible_time: float | None = None) -> None:
        """Make a camouflage spider fully visible after touch/contact.

        This is intentionally tiny and safe for non-camouflage personalities: when
        a personality has no camouflage strength, the method is a no-op.
        """
        try:
            max_strength = float(self.personality.get("camouflage_strength", 0.0) or 0.0)
        except Exception:
            max_strength = 0.0
        if max_strength <= 0.0:
            return
        if visible_time is None:
            visible_time = rand_range(self.personality.get("camouflage_touch_visible_time"), 4.5, 8.0, rng=self.rng)
        try:
            visible_time = max(0.4, float(visible_time))
        except Exception:
            visible_time = 5.5
        self._camouflage_strength = 0.0
        self._camouflage_color = None
        self._camouflage_idle_timer = 0.0
        self._camouflage_visible_timer = max(float(self._camouflage_visible_timer), visible_time)

    def _phase_focus_context(self, mx: float, my: float) -> str:
        """Classify the object currently most relevant to this spider."""
        prey = self._prey
        if self._hunting_prey and prey is not None and getattr(prey, "alive", False):
            return "prey"
        if (
            self.state in ("WeaveApproach", "Weave", "WebApproach", "WebWalk", "WebAim", "WebShot", "RepairApproach", "Repair")
            or self.web_target is not None
            or self.weaving_web is not None
            or self.repairing_web is not None
        ):
            return "web"
        if self.social_target is not None and not getattr(self.social_target, "dragging", False):
            return "creature"
        reaction = float(self.personality.get("reaction_radius", 360.0))
        social_range = max(self.size * 12.0, reaction * 0.85)
        mate = self._find_social_target(social_range) if self.allow_social else None
        cursor_distance = distance(self.x, self.y, mx, my)
        if mate is not None and (cursor_distance > reaction * 0.85 or distance(self.x, self.y, mate.x, mate.y) < cursor_distance * 1.15):
            return "creature"
        if cursor_distance < reaction:
            return "cursor"
        return "none"

    def _phase_target(self, focus: str, mx: float, my: float) -> Tuple[float, float, "Creature" | None]:
        """Resolve a phase focus to coordinates and, when applicable, a mate."""
        if focus == "prey":
            prey = self._prey
            if prey is not None and getattr(prey, "alive", False) and not getattr(prey, "eaten", False):
                return prey.x, prey.y, None
        if focus == "creature":
            mate = self.social_target
            if mate is None or getattr(mate, "dragging", False) or getattr(mate, "airborne", False):
                mate = self._find_social_target(max(self.size * 12.0, float(self.personality.get("reaction_radius", 360.0))))
            if mate is not None:
                return mate.x, mate.y, mate
        return mx, my, None

    def _phase_allowed(self, phase_id: str) -> bool:
        scheduler = getattr(self, "phase_scheduler", None)
        return scheduler is None or scheduler.is_allowed(phase_id)

    def _dispatch_scheduled_phase(self, phase_id: str, focus: str, mx: float, my: float) -> bool:
        """Translate one scheduled phase into the existing FSM entry helpers."""
        target_phases = {
            "approach",
            "chase",
            "observe",
            "prepare_jump_attack",
            "inspect",
            "cuddle",
            "social_play",
            "run_away",
        }
        if phase_id in target_phases and focus == "none":
            return False
        tx, ty, target = self._phase_target(focus, mx, my)
        if phase_id == "wander":
            self.enter_wander()
        elif phase_id == "jump":
            self.enter_spring(after="idle", power=self.rng.uniform(0.72, 1.15))
        elif phase_id == "roll":
            self.enter_roll()
        elif phase_id == "zoomies":
            self.enter_zoomies()
        elif phase_id == "approach":
            self.enter_approach(tx, ty)
        elif phase_id == "chase":
            self.enter_chase(tx, ty)
        elif phase_id == "observe":
            self.enter_observe(tx, ty, target)
        elif phase_id == "prepare_jump_attack":
            self.enter_aim(tx, ty, target=target, after="outcome")
        elif phase_id == "inspect":
            self.enter_inspect(tx, ty, target)
        elif phase_id == "cuddle":
            self.enter_cuddle(tx, ty, target)
        elif phase_id == "social_play":
            if target is None:
                return False
            self.enter_play(target)
        elif phase_id == "run_away":
            self.enter_retreat(mx, my)
        else:
            return False
        return phase_id_for_state(self.state) == phase_id

    def _activate_scheduled_phase(self, mx: float, my: float) -> bool:
        """Draw and start a focus-aware phase when the FSM reaches a decision point."""
        if self.airborne or self.dragging or self.state not in ("Idle", "Alert"):
            return False
        focus = self._phase_focus_context(mx, my)
        # Failed context-specific actions (for example social play with no mate)
        # consume their deck slot and let the next weighted phase try immediately.
        for _ in range(len(self.phase_scheduler.phase_ids) + 1):
            plan = self.phase_scheduler.choose(focus)
            if plan is None:
                return False
            if self._dispatch_scheduled_phase(plan.phase_id, focus, mx, my):
                self.state_timer = plan.duration
                return True
            self.phase_scheduler.finish()
        return False

    def _sync_scheduled_phase(self, focus: str | None = None) -> None:
        """Adopt legacy FSM entries so their periods also use personality pacing."""
        phase_id = phase_id_for_state(self.state)
        if phase_id is None:
            if self.phase_scheduler.current is not None:
                self.phase_scheduler.cancel()
            return
        if self.phase_scheduler.current_phase_id == phase_id:
            return
        duration = self.phase_scheduler.adopt(phase_id, focus or "none")
        if duration is not None:
            self.state_timer = duration

    def _finish_expired_scheduled_phase(self, mx: float, my: float) -> bool:
        """End a high-level period cleanly when its scheduled time is exhausted."""
        scheduler = self.phase_scheduler
        if scheduler.current is None or scheduler.remaining > 0.0:
            return False
        # Jump preparation and playful rolls are allowed to complete their
        # physical hand-off; the next grounded state will start or cancel the
        # following period.  A roll has two clocks: the personality phase timer
        # and the actual curl/spin animation. Ending the phase first leaves
        # ``roll_tuck``/``roll_spin`` active while the state is already Idle.
        if self.state in ("Coil", "Aim", "Jump", "Land", "Roll"):
            return False
        scheduler.finish()
        if self.state in ("Wander", "Approach", "Chase", "Observe", "Inspect", "Cuddle", "Play", "Zoom", "Roll", "Retreat"):
            self.enter_idle()
            return True
        return False

    def _update_state(self, dt: float, mx: float, my: float) -> None:
        self.state_timer -= dt
        self.decision_timer -= dt
        if self._finish_expired_scheduled_phase(mx, my):
            return
        dist_to_cursor = distance(self.x, self.y, mx, my)
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        reaction = float(self.personality.get("reaction_radius", 360))
        hunter = self._is_hunter_personality()
        jumper = self._is_jumper_personality()
        observer = self._is_observer_personality()
        cursor_still = hunter and self._cursor_is_still_for_observe()
        hunt_catch_distance = self.size * float(self.personality.get("hunt_catch_distance_mult", 2.25))
        if self._update_job_state(dt, mx, my):
            # Jobs own their work target, while temperament still drives the
            # leg solver, posture, and animation style underneath it.
            return
        if self._is_drifter_personality() and self.state != "DriftRun":
            self.drift_run_cooldown = max(0.0, float(getattr(self, "drift_run_cooldown", 0.0)) - dt)

        if self.state == "Idle":
            self.speed = 0.0
            self.target_heading = angle_to(self.x, self.y, mx, my) if dist_to_cursor < reaction else self.target_heading
            if hunter and dist_to_cursor < reaction and self.decision_timer <= 0.0:
                self.decision_timer = self.rng.uniform(0.08, 0.18)
                if cursor_still:
                    self.enter_alert(mx, my)
                else:
                    self.enter_approach(mx, my)
                return
            if jumper and self.state_timer <= 0.0 and self.decision_timer <= 0.0:
                # A jumper rarely slides straight out of idle; it starts roaming with a small hop.
                if self.rng.random() < float(self.personality.get("idle_hop_chance", 0.55)):
                    tx = self.x + math.cos(self.heading + self.rng.uniform(-0.65, 0.65)) * self.size * self.rng.uniform(1.0, 2.0)
                    ty = self.y + math.sin(self.heading + self.rng.uniform(-0.65, 0.65)) * self.size * self.rng.uniform(1.0, 2.0)
                    self.enter_coil(after="wander", power=rand_range(self.personality.get("hop_power"), 0.34, 0.58, rng=self.rng), toward=(tx, ty))
                    return
            if observer and self.decision_timer <= 0.0:
                anchor = self._observer_anchor(mx, my)
                if anchor is not None and (dist_to_cursor < reaction * 1.18 or anchor[2] is not None):
                    self.decision_timer = self.rng.uniform(0.16, 0.32)
                    ax, ay, target = anchor
                    self.enter_observe(ax, ay, target)
                    return
            if self.state_timer <= 0.0 and self.decision_timer <= 0.0:
                self.decision_timer = self.rng.uniform(0.2, 0.5)
                if self._activate_scheduled_phase(mx, my):
                    return
                if self._consider_special_actions(dist_to_cursor, mx, my, from_idle=True):
                    return
                if dist_to_cursor < reaction:
                    self.enter_alert(mx, my)
                elif self.rng.random() < float(self.personality.get("wander_frequency", 0.35)):
                    self.enter_wander()
                else:
                    self.enter_idle()

        elif self.state == "Alert":
            self.speed = 0.0
            self.target_x = mx
            self.target_y = my
            self.target_heading = angle_to(self.x, self.y, mx, my)
            if hunter:
                self.aim_intent = min(1.0, self.aim_intent + dt * 2.2)
                if cursor_still:
                    # Freeze and watch a still cursor instead of creeping into it.
                    if self.state_timer <= 0.0:
                        # A motionless pointer is an easy mark for sticky silk.
                        if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                            return
                        self.state_timer = rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42, rng=self.rng)
                    return
                if dist_to_cursor <= hunt_catch_distance * 1.5:
                    # Within striking range: a web-shooter webs its prey; others pounce.
                    if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                        return
                    self.enter_chase(mx, my)
                    return
                if dist_to_cursor < reaction:
                    self.enter_approach(mx, my)
                    return
            if observer and self.state_timer <= 0.0:
                anchor = self._observer_anchor(mx, my)
                if anchor is not None:
                    ax, ay, target = anchor
                    self.enter_observe(ax, ay, target)
                    return
            if self.state_timer <= 0.0:
                if self._activate_scheduled_phase(mx, my):
                    return
                if self._consider_special_actions(dist_to_cursor, mx, my, from_idle=False):
                    return
                if self.has_skill("run_away") and dist_to_cursor < float(self.personality.get("threat_radius", 180)) * 0.55 and self.rng.random() > boldness:
                    self.enter_retreat(mx, my)
                elif self.has_skill("chase") and dist_to_cursor < 95.0 and self.rng.random() < 0.45 + boldness * 0.45:
                    self.enter_chase(mx, my)
                elif dist_to_cursor < reaction and self.rng.random() < 0.25 + boldness * 0.65:
                    self.enter_approach(mx, my)
                else:
                    self.enter_idle()

        elif self.state == "Approach":
            self.target_heading = angle_to(self.x, self.y, mx, my)
            if hunter:
                self.target_x = mx
                self.target_y = my
                self.aim_intent = min(1.0, self.aim_intent + dt * 1.8)
                if cursor_still:
                    self.motion_paused = True
                    self.speed = 0.0
                    if self.state_timer <= 0.0:
                        if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                            return
                        self.state_timer = rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42, rng=self.rng)
                    return
                if self.motion_paused:
                    # The instant prey moves again, break observation and burst forward.
                    self.motion_paused = False
                    self.state_timer = min(self.state_timer, 0.08)
                if dist_to_cursor <= hunt_catch_distance * 1.2:
                    # Close enough to strike: web the prey if able, otherwise pounce.
                    if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                        return
                    self.enter_chase(mx, my)
                    return
                if dist_to_cursor > reaction * 1.65:
                    self.enter_alert(mx, my) if dist_to_cursor < reaction * 2.0 else self.enter_idle()
                    return
                self.speed = self._hunter_approach_speed(dist_to_cursor, reaction)
                if self.state_timer <= 0.0:
                    # Stalk pause: a chance to fire sticky silk at the prey from range.
                    if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                        return
                    self.motion_paused = True
                    self.speed = 0.0
                    self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.08, 0.20, rng=self.rng)
                return
            if self.has_skill("chase") and dist_to_cursor < 78.0 and self.rng.random() < 0.025 + boldness * 0.035:
                self.enter_chase(mx, my)
                return
            if dist_to_cursor > reaction * 1.35:
                self.enter_alert(mx, my)
                return
            if self.motion_paused:
                self.speed = 0.0
                self.target_x = mx
                self.target_y = my
            else:
                self.speed = 52.0 * self._speed_mult()
                if self._maybe_jumper_hop(dt, "approach", (mx, my)):
                    return
            if self.state_timer <= 0.0:
                if self.motion_paused:
                    self.motion_paused = False
                    self.target_x = mx
                    self.target_y = my
                    self.speed = 52.0 * self._speed_mult()
                    self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3, rng=self.rng)
                else:
                    self.motion_paused = True
                    self.speed = 0.0
                    self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.25, 0.7, rng=self.rng)

        elif self.state == "Chase":
            self.chase_timer -= dt
            if hunter:
                self.target_heading = angle_to(self.x, self.y, mx, my)
                self.aim_intent = min(1.0, self.aim_intent + dt * 2.6)
                if self._hunting_prey:
                    # Stalk a fly using stop-motion camouflage: advance only while
                    # the prey itself is moving, then freeze the instant it stops.
                    # The manager normally keeps Hunters in Approach, but this
                    # safeguard preserves the rule if a pounce/chase transition
                    # leaves one in Chase.
                    self.target_x, self.target_y = mx, my
                    if cursor_still:
                        self.motion_paused = True
                        self.speed = 0.0
                        self.current_speed = min(self.current_speed, 8.0)
                    else:
                        self.motion_paused = False
                        self.speed = self._hunter_approach_speed(dist_to_cursor, reaction)
                    self.chase_timer = max(self.chase_timer, 1.0)
                    return
                if cursor_still:
                    self.motion_paused = True
                    self.speed = 0.0
                    self.target_x = mx
                    self.target_y = my
                    if self.chase_timer <= 0.0 and dist_to_cursor > reaction * 1.2:
                        self.enter_idle()
                    return
                self.motion_paused = False
                base_speed = float(self.personality.get("hunt_chase_speed", 150.0)) * self._speed_mult()
                if dist_to_cursor <= hunt_catch_distance and self.decision_timer <= 0.0:
                    # Tag the cursor, whip around it, then immediately recommit.
                    self.mood.bump(valence=0.08, arousal=0.18, curiosity=0.08)
                    self.wiggle_burst = max(self.wiggle_burst, 0.75)
                    self.catch_blend = 0.0
                    spin = angle_to(mx, my, self.x, self.y) + self.rng.choice((-1.0, 1.0)) * self.rng.uniform(1.35, 2.75)
                    orbit = self.rng.uniform(self.size * 1.2, self.size * 2.8)
                    self.target_x = mx + math.cos(spin) * orbit
                    self.target_y = my + math.sin(spin) * orbit
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.speed = base_speed * 1.18
                    self.chase_timer = max(self.chase_timer, float(self.personality.get("chase_persistence", 2.5)))
                    self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12, rng=self.rng)
                    self.decision_timer = self.rng.uniform(0.10, 0.22)
                    return
                if self.state_timer <= 0.0:
                    lead = clamp(self._cursor_speed() / 80.0, 0.0, 18.0)
                    self.target_x = mx + self.prev_cursor_vx * 0.035 + self.rng.uniform(-10.0 - lead, 10.0 + lead)
                    self.target_y = my + self.prev_cursor_vy * 0.035 + self.rng.uniform(-10.0 - lead, 10.0 + lead)
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12, rng=self.rng)
                self.speed = base_speed
                if self.chase_timer <= 0.0 or dist_to_cursor > reaction * 1.75:
                    self.enter_alert(mx, my) if dist_to_cursor < reaction else self.enter_idle()
                return
            if self.state_timer <= 0.0:
                self.target_x = mx + self.rng.uniform(-18.0, 18.0)
                self.target_y = my + self.rng.uniform(-18.0, 18.0)
                self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                self.state_timer = self.rng.uniform(0.10, 0.24)
            self.speed = 112.0 * self._speed_mult()
            if self._maybe_jumper_hop(dt, "chase", (mx, my)):
                return
            if self._hunting_prey:
                # Stay locked on the fly; the manager keeps the focus on it.
                self.target_x, self.target_y = mx, my
                self.chase_timer = max(self.chase_timer, 0.6)
            elif self.chase_timer <= 0.0 or dist_to_cursor > reaction * 1.45:
                self.enter_alert(mx, my) if dist_to_cursor < reaction else self.enter_idle()

        elif self.state == "Retreat":
            self.speed = 148.0 * self._speed_mult()
            if self.state_timer <= 0.0 or distance(self.x, self.y, self.target_x, self.target_y) < 25.0:
                if dist_to_cursor < reaction * 0.8:
                    self.enter_alert(mx, my)
                else:
                    self.enter_idle()

        elif self.state == "Startled":
            # After a drop/throw, let throw inertia dominate briefly. Once it fades,
            # the spider takes a short nervous scurry away from the pointer.
            if self.inertia_timer > 0.0:
                self.speed = 0.0
            elif getattr(self, "throw_recovery", 0.0) > 0.0:
                # Skid finished: hold still for a beat so the stop is visible,
                # then let the ordinary startled reaction take over.
                self.throw_recovery = max(0.0, self.throw_recovery - dt)
                self.speed = 0.0
                self.motion_paused = True
                self.target_x, self.target_y = self.x, self.y
            else:
                self.motion_paused = False
                self.speed = 74.0 * self._speed_mult()
                if dist_to_cursor < reaction * 0.85 and self.decision_timer <= 0.0:
                    away = math.atan2(self.y - my, self.x - mx)
                    self.target_x = self.x + math.cos(away) * self.rng.uniform(95.0, 180.0)
                    self.target_y = self.y + math.sin(away) * self.rng.uniform(95.0, 180.0)
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.target_heading = away
                    self.decision_timer = self.rng.uniform(0.25, 0.45)
            if self.state_timer <= 0.0 and self.inertia_timer <= 0.0 and self.throw_recovery <= 0.0:
                if self.has_skill("run_away") and dist_to_cursor < reaction * 0.65:
                    self.enter_retreat(mx, my)
                else:
                    self.enter_idle()

        elif self.state == "Wander":
            self.speed = 0.0 if self.motion_paused else 38.0 * self._speed_mult()
            if jumper and not self.motion_paused and self._maybe_jumper_hop(dt, "wander", (self.target_x, self.target_y)):
                return
            if observer:
                anchor = self._observer_anchor(mx, my)
                if anchor is not None and self.decision_timer <= 0.0:
                    ax, ay, target = anchor
                    self.enter_observe(ax, ay, target)
                    return
            if dist_to_cursor < reaction * 0.75:
                self.enter_alert(mx, my)
                return
            if self.state_timer <= 0.0 or distance(self.x, self.y, self.target_x, self.target_y) < 22.0:
                self.enter_idle()
                return
            if self.decision_timer <= 0.0:
                self.decision_timer = self.rng.uniform(0.35, 0.8)
                if self._should_start_drift_run(from_idle=False):
                    self.enter_drift_run()
                    return
                if self.rng.random() < 0.25:
                    self.motion_paused = not self.motion_paused

        elif self.state == "DriftRun":
            self._update_drift_run(dt, mx, my)
        elif self.state == "Observe":
            self._update_observe(dt, mx, my)
        elif self.state == "Inspect":
            self._update_inspect(dt, mx, my)
        elif self.state == "Cuddle":
            self._update_cuddle(dt, mx, my)
        elif self.state == "Aim":
            self._update_aim(dt, mx, my)
        elif self.state == "Coil":
            self.speed = 0.0
            if self.state_timer <= 0.0:
                tx, ty = self.coil_toward if self.coil_toward else (self.x + math.cos(self.heading) * self.size, self.y + math.sin(self.heading) * self.size)
                self._launch_jump(tx, ty, kind="hop", after=self.coil_after, reach=self.coil_power)
        elif self.state == "Land":
            self.speed = 0.0
            if self.state_timer <= 0.0:
                after = getattr(self, "land_after", "idle")
                if after == "outcome":
                    self._resolve_pounce_outcome()
                elif after == "nope":
                    if self.nope_repeats > 0 and self.has_skill("jump") and self.has_skill("run_away"):
                        self.enter_nope_escape(mx, my, continuing=True)
                    elif self.has_skill("run_away"):
                        self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28, rng=self.rng)
                        self.enter_retreat(mx, my)
                    else:
                        self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28, rng=self.rng)
                        self.enter_idle()
                elif after == "play" and self.social_target is not None and not self.social_target.dragging:
                    self.enter_play(self.social_target, role=self.social_role if self.social_role in ("chase", "flee") else None)
                elif after == "wander":
                    self.enter_wander()
                elif after == "approach":
                    self.enter_approach(mx, my)
                elif after == "chase":
                    self.enter_chase(mx, my)
                else:
                    self.enter_idle()
        elif self.state == "Catch":
            self._update_catch(dt, mx, my)
        elif self.state == "Feed":
            # Crouched over the kill, the spider holds the fly in its front legs
            # and works it: the body stays planted (no trembling) while the front
            # legs trample and turn the prey.  Then it straightens and moves on.
            self.motion_paused = True
            self.speed = 0.0
            self.crouch = min(1.0, self.crouch + dt * 1.5)
            self.catch_blend = 1.0          # keep the front legs reaching the prey
            self._feed_paw = self._feed_paw + dt
            self._feed_frenzy = self._feed_frenzy - dt
            anchor = self._feed_anchor
            # Hold the body exactly on the catch spot; only the legs move.
            self.x += (anchor[0] - self.x) * clamp(dt * 14.0, 0.0, 1.0)
            self.y += (anchor[1] - self.y) * clamp(dt * 14.0, 0.0, 1.0)
            self.target_x, self.target_y = self.x, self.y
            if self._feed_frenzy > 0.0:
                # A light body wiggle reads as effort without the body jittering.
                self.wiggle_burst = max(self.wiggle_burst, 0.4)
                self.aim_intent = min(1.0, self.aim_intent + dt * 4.0)
            if self.state_timer <= 0.0:
                self.crouch = max(0.0, self.crouch - 0.3)
                self.motion_paused = False
                self.enter_idle()
        elif self.state == "Play":
            self._update_play(dt, mx, my)
        elif self.state == "Zoom":
            if self.state_timer <= 0.0 or distance(self.x, self.y, self.target_x, self.target_y) < 26.0:
                if self.zoom_repeats > 0 and self.rng.random() < 0.7:
                    self.zoom_repeats -= 1
                    if self.rng.random() < 0.3:
                        self.enter_spring(after="idle", power=self.rng.uniform(0.7, 1.1))
                    else:
                        self.enter_zoomies()
                else:
                    self.enter_idle()
        elif self.state == "Roll":
            self._update_roll(dt, mx, my)
        elif self.state == "WeaveApproach":
            self._update_weave_approach(dt, mx, my)
        elif self.state == "Weave":
            self._update_weave(dt, mx, my)
        elif self.state == "WebApproach":
            self._update_web_approach(dt, mx, my)
        elif self.state == "WebWalk":
            self._update_web_walk(dt, mx, my)
        elif self.state == "WebAim":
            self._update_web_aim(dt, mx, my)
        elif self.state == "WebShot":
            self._update_web_shot(dt, mx, my)
        elif self.state == "RepairApproach":
            self._update_repair_approach(dt, mx, my)
        elif self.state == "Repair":
            self._update_repair(dt, mx, my)

    def _consider_special_actions(self, dist_to_cursor: float, mx: float, my: float, from_idle: bool) -> bool:
        self.social_cooldown = max(0.0, self.social_cooldown - 0.0)  # decremented in mood update
        m = self.mood
        reaction = float(self.personality.get("reaction_radius", 360))
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        hunter = self._is_hunter_personality()

        # --- Drifter self-directed flourish ---
        if self._should_start_drift_run(from_idle=from_idle):
            self.enter_drift_run()
            return True

        webber = self._is_webber_personality()

        # --- Web weaving / repairing / finishing (webbers do this constantly) ---
        if (self.has_skill("weave_web") and self.cage is None
                and self.perception.has_web_world and self.weave_cooldown <= 0.0):
            # First, prefer mending a torn finished web -- a webber dislikes a
            # broken net and will go fix it.  Webbers travel anywhere for it;
            # other spiders only mend one that is reasonably close.
            repairable = self.perception.repairable_web(max_dist=1e9 if webber else reaction * 1.5)
            if repairable is not None and self.rng.random() < (0.9 if webber else 0.25):
                if self._begin_repair(repairable):
                    return True
            # Next, prefer finishing an abandoned, unfinished web -- even one
            # another spider began.  Webbers will travel anywhere to finish it;
            # other spiders only adopt one that is reasonably close.
            adoptable = self.perception.adoptable_web(max_dist=1e9 if webber else reaction * 1.6)
            if adoptable is not None and self.rng.random() < (0.85 if webber else 0.22):
                if self._begin_adopt(adoptable):
                    return True
            # Otherwise start a brand-new web.  Each finished, intact web already
            # on screen reduces the urge to build another, so a webber stops once
            # it has spun a few; a torn web does not count, so damage keeps the
            # webber motivated (to repair, or to replace) until it is whole again.
            intact = self.perception.intact_web_count()
            satiation = clamp(1.0 - intact * float(self.personality.get("web_satiation_per_web", 0.22)),
                              0.12, 1.0)
            start_chance = ((0.62 + m.curiosity * 0.28) if webber else 0.03) * satiation
            if self.rng.random() < start_chance:
                if self._begin_weave():
                    return True

        # --- Walking onto a finished web to bounce-test it (any spider) ---
        if (self.has_skill("web_walk") and self.cage is None
                and self.perception.has_web_world and self.web_walk_cooldown <= 0.0):
            walkable = self.perception.walkable_web(max_dist=reaction * 1.8)
            if walkable is not None:
                walk_chance = 0.45 if webber else 0.16 + m.curiosity * 0.2
                if self.rng.random() < walk_chance:
                    if self._begin_web_walk(walkable):
                        return True

        # --- Shooting sticky silk at the pointer (trap it / shove it to a wall) ---
        # The hunting states drive this for stalkers; this covers every other
        # spider that has the skill, so a non-hunter web-shooter still fires.
        if dist_to_cursor < reaction:
            if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                return True

        # --- Social play with another creature ---
        if self.allow_social and not hunter and self.social_cooldown <= 0.0:
            social_range = max(reaction * 0.85, self.size * 12.0)
            mate = self._find_social_target(social_range)
            if mate is not None:
                d = distance(self.x, self.y, mate.x, mate.y)
                # Pick an interaction by current mood.
                play_w = 0.25 + m.arousal * 0.6 + max(0.0, m.valence) * 0.4
                inspect_w = 0.25 + m.curiosity * 0.8
                cuddle_w = 0.1 + m.affection * 0.8
                chance = clamp(0.35 + m.arousal * 0.4 + m.curiosity * 0.3, 0.2, 0.92)
                if self.rng.random() < chance:
                    self.social_cooldown = self.rng.uniform(4.0, 8.0)
                    roll = self.rng.random() * (play_w + inspect_w + cuddle_w)
                    if roll < play_w:
                        if d > self.size * 3.0 and m.arousal > 0.45 and self.rng.random() < 0.5:
                            self.enter_aim(mate.x, mate.y, target=mate, after="play", ranging=(0.4, 0.8), abort_chance=0.1)
                        else:
                            self.enter_play(mate)
                    elif roll < play_w + inspect_w:
                        self.enter_inspect(mate.x, mate.y, target=mate)
                    else:
                        self.enter_cuddle(mate.x, mate.y, target=mate)
                    return True

        # --- Cursor-directed expressive actions ---
        if dist_to_cursor < reaction:
            near = dist_to_cursor < self.size * 6.5
            mid = dist_to_cursor < reaction * 0.8

            # Hunter-ish aim+pounce at the cursor from mid range.
            if mid and (boldness > 0.6 or m.arousal > 0.6) and self.rng.random() < 0.10 + boldness * 0.30 + m.arousal * 0.2:
                self.enter_aim(mx, my, target=None, after="outcome",
                               ranging=(0.55, 1.2), abort_chance=0.22 - boldness * 0.15)
                return True
            # Curious lean-in inspection.
            if mid and m.curiosity > 0.55 and self.rng.random() < 0.18 + m.curiosity * 0.4:
                self.enter_inspect(mx, my, target=None)
                return True
            # Affectionate cuddle when close.
            if near and m.affection > 0.55 and self.rng.random() < 0.2 + m.affection * 0.5:
                self.enter_cuddle(mx, my, target=None)
                return True
            # Playful little pounce when close.
            if near and m.valence > 0.4 and m.arousal > 0.5 and self.rng.random() < 0.25:
                self.enter_aim(mx, my, target=None, after="outcome",
                               ranging=(0.35, 0.7), abort_chance=0.1)
                return True
            # Happy little tumble away from the cursor.
            if near and m.valence > 0.5 and m.arousal > 0.55 and self.rng.random() < 0.12:
                self.enter_roll(direction=angle_to(self.x, self.y, mx, my) + math.pi)
                return True

        # --- Self-directed play when no obvious target (playful temperament) ---
        # Hunters reserve idle time for scanning and stalking.  In particular,
        # do not let the generic happy-mood flourish chooser turn a hunter into
        # a roller/zoomies spider between prey sightings.
        if from_idle and not hunter and m.valence > 0.4 and m.arousal > 0.55:
            r = self.rng.random()
            if r < 0.12:
                self.enter_roll()
                return True
            if r < 0.26:
                self.enter_spring(after="idle", power=self.rng.uniform(0.7, 1.2))
                return True
            if r < 0.34:
                self.enter_zoomies()
                return True
        return False

    def _begin_weave(self) -> bool:
        """Claim a fresh site and start walking to it to build.

        Corners are still favoured, but a webber will often pick an open spot out
        in the room so its webs end up scattered around rather than only hugging
        the screen edges.
        """
        if self.web_world is None or self.cage is not None:
            return False
        prefer_corner = self.rng.random() < float(self.personality.get("web_corner_bias", 0.55))
        web = self.web_world.claim_site(self, prefer_corner=prefer_corner)
        if web is None:
            # Every site is taken; back off briefly before trying again.
            self.weave_cooldown = rand_range(self.personality.get("weave_retry_cooldown"), 5.0, 10.0, rng=self.rng)
            return False
        self.weaving_web = web
        self.enter_weave_approach()
        return True

    def _begin_adopt(self, web) -> bool:
        """Take over an abandoned, unfinished web to finish it off."""
        if self.web_world is None or web is None:
            return False
        if not self.web_world.adopt(web, self):
            return False
        self.weaving_web = web
        self.enter_weave_approach()
        return True

    def _begin_web_walk(self, web) -> bool:
        """Walk onto a finished web to bounce-test it."""
        if self.web_world is None or web is None or not web.is_complete():
            return False
        self.web_target = web
        self.enter_web_approach()
        return True

    def _abandon_weaving(self) -> None:
        """Release the in-progress web so any spider can finish it later."""
        if self.weaving_web is not None and self.web_world is not None:
            self.web_world.abandon(self.weaving_web)
        self.weaving_web = None
        self._weave_drawing = False
        # Short cooldown so an interrupted spider does not instantly re-weave.
        self.weave_cooldown = max(self.weave_cooldown,
                                  rand_range(self.personality.get("weave_retry_cooldown"), 3.5, 7.5, rng=self.rng))

    def _finish_weave(self) -> None:
        self.weaving_web = None
        self._weave_drawing = False
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, rng=self.rng)
        self.weave_cooldown = base / (4.0 if self._is_webber_personality() else 1.0)
        self.mood.bump(arousal=-0.05, valence=0.18, affection=0.04)
        self.enter_idle()

    def _begin_repair(self, web) -> bool:
        """Claim a torn finished web and walk over to re-knit it."""
        if self.web_world is None or web is None:
            return False
        if not self.web_world.claim_repair(web, self):
            return False
        self.repairing_web = web
        self.enter_repair_approach()
        return True

    def _abandon_repair(self) -> None:
        if self.repairing_web is not None and self.web_world is not None:
            self.web_world.release_repair(self.repairing_web)
        self.repairing_web = None
        self.weave_cooldown = max(self.weave_cooldown,
                                  rand_range(self.personality.get("weave_retry_cooldown"), 2.5, 5.0, rng=self.rng))

    def _finish_repair(self) -> None:
        if self.repairing_web is not None and self.web_world is not None:
            self.web_world.release_repair(self.repairing_web)
        self.repairing_web = None
        # Mending is satisfying but quick to come off cooldown for a webber, so it
        # stays attentive to further damage.
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, rng=self.rng)
        self.weave_cooldown = base / (6.0 if self._is_webber_personality() else 1.5)
        self.mood.bump(valence=0.14, affection=0.05, arousal=-0.04)
        self.enter_idle()

    def _repair_point(self):
        web = self.repairing_web
        if web is None:
            return None
        cen = web.damaged_centroid()
        if cen is None:
            return None
        # Stand a little inset so the spider can actually reach the torn area.
        ins = web.reach_inset
        return (clamp(cen[0], ins, self.screen_w - ins),
                clamp(cen[1], ins, self.screen_h - ins))

    def enter_repair_approach(self) -> None:
        if not self.has_skill("weave_web") or self.repairing_web is None:
            self.enter_idle()
            return
        self.state = "RepairApproach"
        self.motion_paused = False
        point = self._repair_point()
        if point is None:
            self._finish_repair()
            return
        self.target_x, self.target_y = point
        self.speed = max(48.0, self._weave_speed * 0.95) * self._speed_mult()
        self.state_timer = 7.0  # safety: do not approach forever
        self.mood.bump(arousal=0.06, valence=0.03)

    def _update_repair_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.repairing_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self._abandon_repair()
            self.enter_idle()
            return
        if not web.is_damaged():
            # Someone else mended it, or it got removed/rebuilt meanwhile.
            self._finish_repair()
            return
        point = self._repair_point()
        if point is None:
            self._finish_repair()
            return
        self.target_x, self.target_y = point
        self._set_focus(point[0], point[1], 0.6)
        if distance(self.x, self.y, point[0], point[1]) < 18.0 or self.state_timer <= 0.0:
            self.enter_repair()

    def enter_repair(self) -> None:
        if self.repairing_web is None:
            self.enter_idle()
            return
        self.state = "Repair"
        self.motion_paused = False
        self.speed = self._weave_speed * 0.8 * self._speed_mult()
        self.state_timer = 20.0  # safety cap on a single mend
        self.mood.bump(arousal=0.03, valence=0.05)

    def _update_repair(self, dt: float, mx: float, my: float) -> None:
        web = self.repairing_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self._abandon_repair()
            self.enter_idle()
            return
        if not web.is_damaged():
            self._finish_repair()
            return
        # Walk to the torn area and re-knit the broken segments nearest the
        # spider first, so the mend reads as the spider working across the damage.
        point = self._repair_point()
        if point is not None:
            self.target_x, self.target_y = point
            self._set_focus(point[0], point[1], 0.5)
            close = distance(self.x, self.y, point[0], point[1]) < self.size * 2.2
        else:
            close = True
        self._weave_drawing = True
        if close:
            self.speed = 0.0
        # Mend regardless of exact proximity once committed, mending nearest to
        # the spider first; the approach above is visual.
        web.repair_near(self.x, self.y, dt)
        if not web.is_damaged():
            self._finish_repair()
            return
        if self.state_timer <= 0.0:
            self._abandon_repair()
            self.enter_idle()

    def enter_weave_approach(self) -> None:
        if not self.has_skill("weave_web") or self.weaving_web is None:
            self.enter_idle()
            return
        self.state = "WeaveApproach"
        self.motion_paused = False
        point, _drawing = self.weaving_web.working_point(
            screen_w=self.screen_w, screen_h=self.screen_h)
        self.target_x, self.target_y = point
        self.speed = max(46.0, self._weave_speed * 0.95) * self._speed_mult()
        self.state_timer = 7.0  # safety: do not approach forever
        self.mood.bump(arousal=0.05, valence=0.05)

    def _update_weave_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.weaving_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.weaving_web = None
            self.enter_idle()
            return
        if web.is_complete():
            # Someone finished it while this spider was on its way over.
            self.weaving_web = None
            self.weave_cooldown = rand_range(self.personality.get("weave_cooldown"), 6.0, 14.0, rng=self.rng)
            self.enter_idle()
            return
        point, _drawing = web.working_point(screen_w=self.screen_w, screen_h=self.screen_h)
        self.target_x, self.target_y = point
        self._set_focus(point[0], point[1], 0.6)
        if distance(self.x, self.y, point[0], point[1]) < 16.0 or self.state_timer <= 0.0:
            self.enter_weave()

    def enter_weave(self) -> None:
        if not self.has_skill("weave_web") or self.weaving_web is None:
            self.enter_idle()
            return
        self.state = "Weave"
        self.motion_paused = False
        self.speed = self._weave_speed * self._speed_mult()
        self.state_timer = 34.0  # generous safety cap for a whole web
        self.mood.bump(arousal=0.04, valence=0.06)

    def _update_weave(self, dt: float, mx: float, my: float) -> None:
        web = self.weaving_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.weaving_web = None
            self.enter_idle()
            return
        if web.is_complete():
            self._finish_weave()
            return
        # Trace the silk: aim at the current working point and lay thread while
        # the body keeps up with it.  ``advance`` only extends silk while the
        # spider trails the reachable tip, so the strand stays pinned to it.
        point, drawing = web.working_point(screen_w=self.screen_w, screen_h=self.screen_h)
        self._weave_drawing = drawing
        self.target_x, self.target_y = point
        # Reposition runs (lifting silk to a new anchor) travel a lot quicker
        # so only the actual silk-laying reads as a careful, deliberate pace.
        self.speed = self._weave_speed * (1.0 if drawing else 2.4) * self._speed_mult()
        self._set_focus(point[0], point[1], 0.5)
        # Loosen the lead while weaving: the silk tip may run a little ahead of
        # the body (the spinnerets lead the body centre), so the build is not
        # throttled to a crawl every time the body has to turn between radii.
        web.advance(dt, self._weave_speed, (self.x, self.y), lead_max=58.0)
        if web.is_complete():
            self._finish_weave()
            return
        if self.state_timer <= 0.0:
            # Took too long (blocked path); leave it adoptable and move on.
            self._abandon_weaving()
            self.enter_idle()

    def enter_web_approach(self) -> None:
        if not self.has_skill("web_walk") or self.web_target is None:
            self.enter_idle()
            return
        self.state = "WebApproach"
        self.motion_paused = False
        px, py = self.web_target.walkable_point(self.screen_w, self.screen_h)
        self._web_walk_point = (px, py)
        self.target_x, self.target_y = px, py
        self.speed = max(48.0, self._weave_speed * 0.85) * self._speed_mult()
        self.state_timer = 7.0

    def _update_web_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.web_target
        if (web is None or self.web_world is None or web not in self.web_world.webs
                or not web.is_complete()):
            self.web_target = None
            self.enter_idle()
            return
        px, py = self._web_walk_point if self._web_walk_point is not None else (web.hub[0], web.hub[1])
        self.target_x, self.target_y = px, py
        self._set_focus(px, py, 0.5)
        if distance(self.x, self.y, px, py) < 16.0 or self.state_timer <= 0.0:
            self.enter_web_walk()

    def enter_web_walk(self) -> None:
        if self.web_target is None:
            self.enter_idle()
            return
        self.state = "WebWalk"
        self.motion_paused = True
        self.speed = 0.0
        self._web_pluck_count = self.rng.randint(2, 4)
        self._web_pluck_timer = rand_range(None, 0.25, 0.5, rng=self.rng)
        self.state_timer = 6.0
        self.mood.bump(arousal=0.06, valence=0.05, curiosity=0.05)

    def _update_web_walk(self, dt: float, mx: float, my: float) -> None:
        web = self.web_target
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.web_target = None
            self.motion_paused = False
            self.enter_idle()
            return
        # Stand on the web and pluck it; the web's own wobble is the bounce test.
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(web.hub[0], web.hub[1], 0.4)
        self._web_pluck_timer -= dt
        if self._web_pluck_timer <= 0.0:
            if self._web_pluck_count > 0:
                web.pluck((self.x, self.y), strength=self.rng.uniform(0.6, 1.3))
                self._web_pluck_count -= 1
                self._web_pluck_timer = rand_range(None, 0.5, 0.95, rng=self.rng)
                self.mood.bump(arousal=0.03, valence=0.04, curiosity=0.03)
            else:
                self._leave_web_walk()
                return
        if self.state_timer <= 0.0:
            self._leave_web_walk()

    def _leave_web_walk(self) -> None:
        self.web_target = None
        self.web_walk_cooldown = rand_range(self.personality.get("web_walk_cooldown"), 8.0, 20.0, rng=self.rng)
        self.motion_paused = False
        self.enter_idle()

    def _can_shoot_web(self, kind: str, prey=None) -> bool:
        skill = "wall_web" if kind == "wall" else "shoot_web"
        if not self.has_skill(skill) or self.airborne:
            return False
        if prey is not None:
            # Firing at a fly goes through the fly world and never touches the
            # real pointer, so neither the cursor-capture toggle nor being held
            # by the mouse gates it: a spider you are carrying can still web a fly.
            return self.perception.can_shoot_prey_web()
        if self.dragging:
            return False
        world = self.mouse_web_world
        return (
            world is not None
            and getattr(world, "enabled", False)
            and not world.busy()
        )

    def _maybe_shoot_web_at_cursor(self, dist_to_cursor: float, mx: float, my: float) -> bool:
        """Roll to fire sticky silk at the pointer. Returns True if it committed.

        Shared by the hunting states and the generic idle decision so any spider
        with the skill can use it, while a web-shooter does it eagerly.
        """
        # When hunting a fly the "cursor" coordinates are really the prey, so the
        # pointer-capture silk must never fire here (it would grab the real
        # mouse). The manager traps flies through the fly world instead.
        if self._hunting_prey:
            return False
        if self.web_shot_cooldown > 0.0:
            return False
        can_trap = self._can_shoot_web("trap")
        can_wall = self._can_shoot_web("wall")
        if not (can_trap or can_wall):
            return False
        web_shooter = self._is_web_shooter_personality()
        reaction = float(self.personality.get("reaction_radius", 360))
        range_mult = float(self.personality.get("web_shot_range_mult",
                                                0.85 if web_shooter else 0.5))
        shot_range = max(self.size * 4.0, reaction * range_mult)
        if dist_to_cursor > shot_range:
            return False
        m = self.mood
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        chance = float(self.personality.get("web_shot_chance", 0.6 if web_shooter else 0.05))
        # A still pointer is an easy mark; excitement and boldness help.
        if self._cursor_is_still_for_observe():
            chance *= 1.55
        chance = clamp(chance + m.arousal * 0.15 + boldness * 0.1, 0.0, 0.97)
        if self.rng.random() >= chance:
            return False
        # Pick the shot. The wall shove is a flashier finisher used a little less
        # often when the spider can also trap in place.
        kind = "trap"
        if can_wall and (not can_trap or
                         self.rng.random() < float(self.personality.get("wall_web_bias", 0.3))):
            kind = "wall"
        self.enter_web_aim(mx, my, kind)
        return True

    def _maybe_shoot_web_at_prey(self, prey) -> bool:
        """Roll to fling trapping silk at a fly. Returns True if it committed.

        The same shot the spider uses on the cursor, aimed at prey instead. A
        web-shooter does it eagerly; any spider with the skill does it sometimes.
        """
        if prey is None or self.web_shot_cooldown > 0.0:
            return False
        if prey.trapped or prey.dragging or not prey.alive:
            return False
        can_trap = self._can_shoot_web("trap", prey=prey)
        can_wall = self._can_shoot_web("wall", prey=prey)
        if not (can_trap or can_wall):
            return False
        web_shooter = self._is_web_shooter_personality()
        reaction = float(self.personality.get("reaction_radius", 360))
        range_mult = float(self.personality.get("web_shot_range_mult",
                                                0.85 if web_shooter else 0.5))
        shot_range = max(self.size * 4.0, reaction * range_mult)
        d = distance(self.x, self.y, prey.x, prey.y)
        if d > shot_range or d < self.size * 1.4:
            return False
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        chance = float(self.personality.get("web_shot_chance", 0.6 if web_shooter else 0.12))
        chance = clamp(chance + self.mood.arousal * 0.15 + boldness * 0.1, 0.0, 0.97)
        if self.rng.random() >= chance:
            return False
        kind = "trap"
        if can_wall and (not can_trap or
                         self.rng.random() < float(self.personality.get("wall_web_bias", 0.3))):
            kind = "wall"
        self.enter_web_aim(prey.x, prey.y, kind, prey=prey)
        return True

    def enter_web_aim(self, mx: float, my: float, kind: str = "trap", prey=None) -> None:
        """Crouch and range the target, then fire a glob of sticky silk."""
        if not self._can_shoot_web(kind, prey=prey):
            self._end_web_state()
            return
        self._web_shot_prey = prey
        self.state = "WebAim"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = None
        self._web_shot_kind = "wall" if kind == "wall" else "trap"
        self._set_focus(mx, my, 1.0)
        self.target_heading = angle_to(self.x, self.y, mx, my)
        self.state_timer = rand_range(self.personality.get("web_aim_time"), 0.3, 0.58, rng=self.rng)
        self.range_clock = 0.0
        self.range_mode = "waggle"
        self.range_switch = self.rng.uniform(0.14, 0.28)
        self.mood.bump(arousal=0.2, curiosity=0.05)

    def _update_web_aim(self, dt: float, mx: float, my: float) -> None:
        prey = self._web_shot_prey
        if prey is not None:
            # Aiming at a fly: bail if it got caught/eaten/grabbed first, and
            # keep tracking its live position as it tries to flee the aim.
            if not prey.alive or prey.eaten or prey.dragging or prey.trapped:
                self._finish_web_shot(fired=False)
                return
            mx, my = prey.x, prey.y
        else:
            # Bail out if pointer-silk got turned off or something else grabbed
            # the pointer while we were ranging.
            world = self.mouse_web_world
            if world is None or not getattr(world, "enabled", False) or world.busy():
                self._finish_web_shot(fired=False)
                return
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(mx, my, 1.0)
        self.target_heading = angle_to(self.x, self.y, mx, my)
        # Converge the feelers into a rangefinder and coil low, like the pounce aim.
        self.aim_intent = min(1.0, self.aim_intent + dt * 3.2)
        self.crouch = min(1.0, self.crouch + dt * 4.0)
        self.range_clock += dt
        if self.range_clock >= self.range_switch:
            self.range_clock = 0.0
            self.range_switch = self.rng.uniform(0.14, 0.28)
            self.range_mode = "nod" if self.range_mode == "waggle" else "waggle"
            if self.range_mode == "waggle":
                self.wiggle_burst = max(self.wiggle_burst, 0.6)
        if self.state_timer <= 0.0:
            self._fire_web_shot(mx, my)

    def _fire_web_shot(self, mx: float, my: float) -> None:
        kind = self._web_shot_kind
        prey = self._web_shot_prey
        # Launch from a little ahead of the body, where the spinnerets/front are.
        fx, fy, _rx, _ry = self._basis()
        origin = (self.x + fx * self.size * 0.6, self.y + fy * self.size * 0.6)
        launched = False
        if prey is not None:
            if self.fly_world is not None and self._can_shoot_web(kind, prey=prey):
                launched = self.fly_world.launch_web_shot(self, prey, kind=kind)
        else:
            world = self.mouse_web_world
            if world is not None and self._can_shoot_web(kind):
                # Silk is thrown, not guided: aim once, leading the pointer by
                # its current velocity. In-flight correction arrives with the
                # Silk tracking ability rather than being free from level one.
                launched = world.shoot(
                    origin,
                    (mx, my),
                    kind=kind,
                    homing=self._progression_effect("web_homing"),
                    lead=(self.prev_cursor_vx, self.prev_cursor_vy),
                )
        if launched:
            self.mood.bump(arousal=0.12, valence=0.12, curiosity=0.05)
            self.enter_web_shot()
        else:
            self._finish_web_shot(fired=False)

    def enter_web_shot(self) -> None:
        """Brief recoil hold right after letting the silk fly."""
        self.state = "WebShot"
        self.motion_paused = True
        self.speed = 0.0
        self.state_timer = rand_range(self.personality.get("web_recoil_time"), 0.18, 0.3, rng=self.rng)
        self.rear = min(1.0, self.rear + 0.4)
        self.wiggle_burst = max(self.wiggle_burst, 0.5)

    def _update_web_shot(self, dt: float, mx: float, my: float) -> None:
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(mx, my, 0.7)
        # Recover from the crouch as the body uncoils out of the shot.
        self.crouch = max(0.0, self.crouch - dt * 3.5)
        if self.state_timer <= 0.0:
            self._finish_web_shot(fired=True)

    def _finish_web_shot(self, fired: bool) -> None:
        web_shooter = self._is_web_shooter_personality()
        prey_shot = self._web_shot_prey is not None
        self._web_shot_prey = None
        if fired:
            base = rand_range(self.personality.get("web_shot_cooldown"), 8.0, 18.0, rng=self.rng)
        else:
            # Did not actually fire (blocked/cancelled): retry sooner.
            base = rand_range(self.personality.get("web_shot_retry_cooldown"), 2.0, 4.5, rng=self.rng)
        self.web_shot_cooldown = base / (3.5 if web_shooter else 1.0)
        if prey_shot:
            # After webbing a fly the spider should close in promptly to eat it,
            # so it is not blocked from firing again by a long cursor cooldown.
            self.web_shot_cooldown = min(self.web_shot_cooldown, 1.2)
        self._end_web_state()

    def _end_web_state(self) -> None:
        """Leave a web aim/shot cleanly: back to being held if still dragged."""
        self.motion_paused = False
        if self.dragging:
            self.state = "Dragged"
            self.motion_paused = True
            self.speed = 0.0
        else:
            self.enter_idle()

    def _update_observe(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is not None:
            if target.dragging or target not in self.neighbors:
                self.social_target = None
                target = None

        if target is not None:
            fx, fy = target.x, target.y
            self._set_focus(fx, fy, 1.0)
        else:
            fx, fy = mx, my
            self._set_focus(mx, my, 1.0)

        reaction = float(self.personality.get("reaction_radius", 360))
        cursor_range = reaction * float(self.personality.get("observe_cursor_range_mult", 1.18))
        if target is None and distance(self.x, self.y, fx, fy) > cursor_range * 1.4:
            self.enter_idle()
            return

        lo, hi = self._observer_radius_bounds()
        desired = clamp(getattr(self, "observe_radius", (lo + hi) * 0.5), lo, hi)
        current = distance(self.x, self.y, fx, fy)
        if current < lo * 0.72 or current > hi * 1.35:
            desired = clamp(current, lo, hi)
            self.observe_radius = desired

        # Move away from the thing being observed along the true opposite vector.
        # Older observer behaviour orbited on a horizontally-stretched ellipse, so it
        # often looked like the spider only chose a left or right side.  Here the
        # observation point can be above, below or diagonal: we preserve the current
        # direction from the focus to this creature and extend that direction to the
        # desired watching distance.
        away_x = self.x - fx
        away_y = self.y - fy
        away_len = math.hypot(away_x, away_y)
        if away_len <= 1e-4:
            fallback_ang = self.heading + math.pi
            away_x = math.cos(fallback_ang)
            away_y = math.sin(fallback_ang)
            away_len = 1.0
        away_x /= away_len
        away_y /= away_len

        self.observe_clock = getattr(self, "observe_clock", 0.0) + dt
        self.target_x = fx + away_x * desired
        self.target_y = fy + away_y * desired
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.target_heading = angle_to(self.x, self.y, fx, fy)
        self.speed = float(self.personality.get("observe_speed", 44.0)) * self._speed_mult()

        if self.decision_timer <= 0.0 and self.rng.random() < 0.18:
            self.decision_timer = self.rng.uniform(0.45, 0.9)
            self.observe_radius = clamp(desired + self.rng.uniform(-self.size * 0.8, self.size * 0.8), lo, hi)

        if self.state_timer <= 0.0:
            linger = clamp(float(self.personality.get("observe_linger_chance", 0.78)), 0.0, 1.0)
            next_anchor = self._observer_anchor(mx, my)
            if next_anchor is not None and self.rng.random() < linger:
                ax, ay, next_target = next_anchor
                self.enter_observe(ax, ay, next_target)
                if self.rng.random() < 0.45:
                    self.observe_dir *= -1.0
                return
            self.social_target = None
            if current < reaction * 0.8 and self.rng.random() < 0.35:
                self.enter_alert(fx, fy)
            else:
                self.enter_idle()

    def _update_inspect(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is not None:
            if target.dragging or target not in self.neighbors:
                self.social_target = None
                self.enter_idle()
                return
            self._set_focus(target.x, target.y, 1.0)
        else:
            self._set_focus(mx, my, 1.0)
        fx, fy = self.focus_x, self.focus_y
        d = distance(self.x, self.y, fx, fy)
        personal = self.size * 3.1
        self.inspect_clock += dt
        self.inspect_intent = min(1.0, self.inspect_intent + dt * 2.2)

        if self.inspect_phase == "approach":
            self.target_heading = angle_to(self.x, self.y, fx, fy)
            self.target_x, self.target_y = fx, fy
            self.speed = 46.0 * self._speed_mult()
            if d <= personal:
                self.inspect_phase = "study"
                self.inspect_clock = 0.0
                self.motion_paused = True
        elif self.inspect_phase == "study":
            self.speed = 0.0
            self.motion_paused = True
            self.target_heading = angle_to(self.x, self.y, fx, fy)
            self.wiggle_burst = max(self.wiggle_burst, 0.25)
            # Periodic head tilts and an occasional probing lunge.
            if self.inspect_clock > self.rng.uniform(0.7, 1.3):
                self.inspect_clock = 0.0
                roll = self.rng.random()
                if roll < 0.4:
                    self.inspect_phase = "orbit"
                    self.orbit_clock = 0.0
                    self.orbit_dir = self.rng.choice((-1.0, 1.0))
                elif roll < 0.62:
                    self.head_tilt = self.rng.uniform(-0.5, 0.5)
                elif roll < 0.78:
                    # tiny boop/tap toward the target
                    self.wiggle_burst = 0.6
            if d > personal * 2.4:
                self.inspect_phase = "approach"
                self.motion_paused = False
        elif self.inspect_phase == "orbit":
            self.motion_paused = False
            self.orbit_clock = getattr(self, "orbit_clock", 0.0) + dt
            ang = angle_to(fx, fy, self.x, self.y) + self.orbit_dir * dt * 1.3
            self.target_x = fx + math.cos(ang) * personal
            self.target_y = fy + math.sin(ang) * personal
            self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
            self.target_heading = angle_to(self.x, self.y, fx, fy)
            self.speed = 40.0 * self._speed_mult()
            if self.orbit_clock > self.rng.uniform(0.8, 1.6):
                self.inspect_phase = "study"
                self.inspect_clock = 0.0

        if self.state_timer <= 0.0:
            # Inspection concludes: escalate by mood, or lose interest.
            m = self.mood
            roll = self.rng.random()
            if m.affection > 0.6 and roll < 0.35:
                self.enter_cuddle(fx, fy, target)
            elif m.valence > 0.45 and m.arousal > 0.5 and roll < 0.6:
                if target is not None:
                    self.enter_play(target)
                else:
                    self.enter_aim(fx, fy, after="outcome", ranging=(0.4, 0.8), abort_chance=0.1)
            else:
                self.mood.bump(curiosity=-0.2)
                self.social_target = None
                self.enter_idle()

    def _update_cuddle(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is not None:
            if target.dragging or target not in self.neighbors:
                self.social_target = None
                self.enter_idle()
                return
            self._set_focus(target.x, target.y, 1.0)
        else:
            self._set_focus(mx, my, 1.0)
        fx, fy = self.focus_x, self.focus_y
        d = distance(self.x, self.y, fx, fy)
        snug = self.size * 1.7
        self.cuddle_clock += dt
        self.cuddle_intent = min(1.0, self.cuddle_intent + dt * 1.8)
        self.mood.bump(affection=dt * 0.25, valence=dt * 0.22)

        if self.cuddle_phase == "approach":
            self.target_heading = angle_to(self.x, self.y, fx, fy)
            self.target_x, self.target_y = fx, fy
            self.speed = 42.0 * self._speed_mult()
            if d <= snug:
                self.cuddle_phase = "snuggle"
                self.motion_paused = True
        else:
            self.speed = 0.0
            self.motion_paused = True
            self.target_heading = angle_to(self.x, self.y, fx, fy)
            # Slow happy wiggle and periodic nuzzle "boops".
            self.wiggle_burst = max(self.wiggle_burst, 0.2)
            self.boop_timer -= dt
            if self.boop_timer <= 0.0:
                self.boop_timer = self.rng.uniform(0.5, 1.0)
                self.wiggle_burst = 0.5
                self.rear = min(1.0, self.rear + 0.4)
            if d > snug * 2.6:
                self.cuddle_phase = "approach"
                self.motion_paused = False

        if self.state_timer <= 0.0:
            self.mood.bump(valence=0.25, affection=0.1)
            self.social_target = None
            self.enter_wander()

    def _update_aim(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is not None and not target.dragging and target in self.neighbors:
            self._set_focus(target.x, target.y, 1.0)
        else:
            if target is not None:
                # lost the playmate mid-aim
                self.social_target = None
            self._set_focus(mx, my, 1.0)
        fx, fy = self.focus_x, self.focus_y
        self.speed = 0.0
        self.motion_paused = True
        self.target_heading = angle_to(self.x, self.y, fx, fy)
        self.aim_intent = min(1.0, self.aim_intent + dt * 3.0)
        self.crouch = min(1.0, self.crouch + dt * 4.0)

        # Ranging behaviour: alternate a side-to-side waggle and a forward nod.
        self.range_clock += dt
        if self.range_clock >= self.range_switch:
            self.range_clock = 0.0
            self.range_switch = self.rng.uniform(0.16, 0.32)
            self.range_mode = "nod" if self.range_mode == "waggle" else "waggle"
            if self.range_mode == "waggle":
                self.wiggle_burst = max(self.wiggle_burst, 0.7)
            else:
                self.rear = min(1.0, self.rear + 0.5)

        if self.state_timer <= 0.0:
            if self.rng.random() < self.aim_abort_chance:
                # Stand down: did not commit to the leap.
                self.mood.bump(arousal=-0.1)
                if self.aim_after == "play" and target is not None:
                    self.enter_inspect(fx, fy, target)
                else:
                    self.enter_idle()
            else:
                self._launch_jump(fx, fy, kind="pounce", after=self.aim_after, reach=1.0)

    def _update_catch(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is not None and not target.dragging and target in self.neighbors:
            self.catch_point = (target.x, target.y)
        self.catch_blend = min(1.0, self.catch_blend + dt * 6.0)
        self.speed = 0.0
        self.motion_paused = True
        cx, cy = self.catch_point
        self.target_heading = angle_to(self.x, self.y, cx, cy)
        if self.state_timer <= 0.0 and not self.catch_resolved:
            self.catch_resolved = True
            d = distance(self.x, self.y, cx, cy)
            got_it = d < self.size * 2.6 and (target is None or (not target.dragging and target in self.neighbors))
            if got_it and self.rng.random() < 0.6:
                self.mood.bump(valence=0.28, affection=0.18, arousal=0.1)
                if target is not None:
                    self.enter_play(target, role="chase")
                else:
                    self.enter_cuddle(cx, cy, None)
            else:
                # Missed/escaped: a small frustrated shake, then maybe re-pounce.
                self.mood.bump(valence=-0.1, arousal=0.18)
                self.wiggle_burst = 0.8
                if self.rng.random() < 0.4 and self.mood.arousal > 0.5:
                    self.enter_aim(cx, cy, target, after="outcome", ranging=(0.3, 0.6), abort_chance=0.1)
                else:
                    self.enter_idle()

    def _update_play(self, dt: float, mx: float, my: float) -> None:
        target = self.social_target
        if target is None or target.dragging or target not in self.neighbors:
            self.social_target = None
            self.enter_idle()
            return
        self._set_focus(target.x, target.y, 0.8)
        self.play_clock += dt
        d = distance(self.x, self.y, target.x, target.y)
        contact = self.size * 1.9

        if self.social_role == "chase":
            self.motion_paused = False
            self.target_x = target.x + self.rng.uniform(-12.0, 12.0)
            self.target_y = target.y + self.rng.uniform(-12.0, 12.0)
            self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
            self.speed = 92.0 * self._speed_mult()
            self.target_heading = angle_to(self.x, self.y, target.x, target.y)
            if d <= contact:
                # Tag! A happy bounce, then resolve the bout.
                self.mood.bump(valence=0.22, arousal=0.18)
                target.mood.bump(valence=0.2, arousal=0.2)
                roll = self.rng.random()
                if roll < 0.45:
                    # Swap roles: the other becomes the chaser, we flee playfully.
                    self.social_role = "flee"
                    if target.state != "Play":
                        target.enter_play(self, role="chase")
                    self.enter_spring(after="idle", power=0.8)
                elif roll < 0.7 and self.mood.affection > 0.4:
                    self.enter_cuddle(target.x, target.y, target)
                else:
                    self.enter_spring(after="idle", power=1.0)
                return
        else:  # flee playfully
            self.motion_paused = False
            away = math.atan2(self.y - target.y, self.x - target.x)
            self.play_lookback -= dt
            if self.play_lookback <= 0.0 and d > contact * 2.5:
                # Pause and glance back, inviting another chase.
                self.play_lookback = self.rng.uniform(0.6, 1.2)
                self.speed = 0.0
                self.target_heading = angle_to(self.x, self.y, target.x, target.y)
                self.wiggle_burst = 0.4
            else:
                self.target_x = self.x + math.cos(away) * 90.0
                self.target_y = self.y + math.sin(away) * 90.0
                self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                self.speed = 104.0 * self._speed_mult()
                self.target_heading = away

        if self.state_timer <= 0.0:
            self.mood.bump(valence=0.12)
            self.social_target = None
            self.enter_idle()

    def _update_jump(self, dt: float) -> None:
        if self.jump_duration <= 0.0:
            self.enter_land(self.jump_after)
            return
        self.jump_t = min(1.0, self.jump_t + dt / self.jump_duration)
        ease = smoothstep(self.jump_t)
        fx, fy = self.jump_from
        tx, ty = self.jump_to
        new_x = fx + (tx - fx) * ease
        new_y = fy + (ty - fy) * ease
        dx = new_x - self.x
        dy = new_y - self.y
        self.x, self.y = new_x, new_y
        # Carry the (tucked) legs with the body so they do not stretch behind.
        self._translate_leg_world_points(dx, dy, 1.0)
        # Parabolic height, peak at the apex of the leap.
        self.jump_z = self.jump_peak * 4.0 * self.jump_t * (1.0 - self.jump_t)

        horiz_speed = math.hypot(dx, dy) / max(1e-4, dt)
        self.current_speed = clamp(horiz_speed, 0.0, 260.0)
        self.vel_x = dx / max(1e-4, dt)
        self.vel_y = dy / max(1e-4, dt)

        # Lively airborne body animation.
        self.bob_phase += dt * 6.0
        self.breath_phase += dt * 2.4
        self.body_bob = 0.0
        self.abdomen_pulse = math.sin(self.jump_t * math.pi) * 0.08
        self.ceph_pulse = math.sin(self.jump_t * math.pi) * 0.05

        if self.jump_t >= 1.0:
            self.enter_land(self.jump_after)

