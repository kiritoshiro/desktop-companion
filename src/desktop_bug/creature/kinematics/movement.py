"""Moving the body itself, and the skitter that flavours it.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math

from ...support.math_utils import (
    angle_lerp,
    clamp,
    clamp_point,
)



class BodyMovementMixin:
    """Moving the body itself, and the skitter that flavours it."""

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

    def _split_playfield(self):
        """The Playfield when monitors leave space none of them shows, else None."""
        playfield = getattr(self, "playfield", None)
        if playfield is None or playfield.simple:
            return None
        return playfield

    def _aim_at_a_real_screen(self) -> None:
        """Pull this frame's destination onto a monitor before any step (DC-88).

        A destination in space no monitor shows -- a patrol point, a hunt, a
        wander picked from the window's bounding box -- had the spider walk
        into it and the manager shove it back out every frame afterwards, the
        body jumping and the legs thrashing. Aiming for the nearest point on a
        real screen instead, it walks to the edge and stops there.
        """
        playfield = self._split_playfield()
        if playfield is not None:
            self.target_x, self.target_y = playfield.clamp(self.target_x, self.target_y, self.margin)

    def _move_body(self, dt: float) -> None:
        playfield = self._split_playfield()
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
            if playfield is not None:
                # Thrown towards space no monitor shows: bounce off the real edge.
                clamped_x, clamped_y = playfield.clamp(clamped_x, clamped_y, self.margin * 0.25)
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

        if self.webbed_held and not self.dragging:
            # Held by silk: no walking and no turning (web_net.py).
            self.current_speed = 0.0
            self.vel_x = self.vel_y = 0.0
            return
        dx = self.target_x - self.x
        dy = self.target_y - self.y
        target_dist = math.hypot(dx, dy)
        move_heading = math.atan2(dy, dx) if target_dist > 2.0 else self.heading
        strafe_observe = self._walks_while_facing_elsewhere()
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
        if playfield is not None:
            # The same margin the manager's backstop uses, so it never has to act.
            self.x, self.y = playfield.clamp(self.x, self.y, max(8.0, self.size * 0.5))

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

