"""Entering a deliberate action: inspect, aim, jump, feed, play, roll.

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
from ..constants import (
    smootherstep,
)


class ActionMixin:
    """Entering a deliberate action: inspect, aim, jump, feed, play, roll."""

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
        if not self._acts_as_drifter() or self.airborne or self.dragging:
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
        if not self._acts_as_drifter():
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
            # DC-22: a knocked-out neighbour is not someone to play with.
            if (target.dragging or getattr(target, "knocked_out", False)
                    or target not in self.neighbors):
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

