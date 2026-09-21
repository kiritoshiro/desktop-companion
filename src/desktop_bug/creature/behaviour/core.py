"""The personality state machine: every `enter_*`/`_update_*` state method.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass

from ...support.math_utils import (
    angle_to,
    clamp,
    clamp_point,
    distance,
    rand_range,
)

from .jobs import JobBehaviourMixin
from .temperament import TemperamentMixin
from .actions import ActionMixin
from .phases import PhaseMixin
from .webs import WebBehaviourMixin
from .updates import StateUpdateMixin


class BehaviourMixin(
    JobBehaviourMixin,
    TemperamentMixin,
    ActionMixin,
    PhaseMixin,
    WebBehaviourMixin,
    StateUpdateMixin,
):
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
        if self._acts_as_hunter():
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
        base_speed = float(self.personality.get("hunt_chase_speed", 150.0)) if self._acts_as_hunter() else 112.0
        self.speed = base_speed * self._speed_mult()
        self.chase_timer = float(self.personality.get("chase_persistence", 2.5))
        if self._acts_as_hunter():
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

    def _update_state(self, dt: float, mx: float, my: float) -> None:
        self.state_timer -= dt
        self.decision_timer -= dt
        if self._finish_expired_scheduled_phase(mx, my):
            return
        dist_to_cursor = distance(self.x, self.y, mx, my)
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        reaction = float(self.personality.get("reaction_radius", 360))
        hunter = self._acts_as_hunter()
        jumper = self._acts_as_jumper()
        observer = self._acts_as_observer()
        cursor_still = hunter and self._cursor_is_still_for_observe()
        hunt_catch_distance = self.size * float(self.personality.get("hunt_catch_distance_mult", 2.25))
        if self._update_job_state(dt, mx, my):
            # Jobs own their work target, while temperament still drives the
            # leg solver, posture, and animation style underneath it.
            return
        if self._acts_as_drifter() and self.state != "DriftRun":
            self.drift_run_cooldown = max(0.0, float(getattr(self, "drift_run_cooldown", 0.0)) - dt)

        if self.state == "Idle":
            self.speed = 0.0
            self.target_heading = angle_to(self.x, self.y, mx, my) if dist_to_cursor < reaction else self.target_heading
            if self.decision_timer <= 0.0:
                self._run_arbiter(mx, my, dist_to_cursor, from_idle=True)
                return

        elif self.state == "Alert":
            self.speed = 0.0
            self.target_x = mx
            self.target_y = my
            self.target_heading = angle_to(self.x, self.y, mx, my)
            if hunter:
                # A Hunter's stalk-and-strike cadence while already watching the
                # cursor is execution of a commitment the arbiter already made
                # (entering Alert in the first place), not a fresh decision --
                # see arbiter.py's module docstring, "deliberately out of scope".
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
            if self.state_timer <= 0.0:
                self._run_arbiter(mx, my, dist_to_cursor, from_idle=False)
                return

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

