"""The per-frame update for each action a spider is already committed to.

Split out of the single ``behaviour.py`` by DC-43; a pure move.
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
    smoothstep,
)


class StateUpdateMixin:
    """The per-frame update for each action a spider is already committed to."""

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

