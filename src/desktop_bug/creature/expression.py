"""Mood coupling, antennae, and posture -- how a creature reads as alive.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

import math

from ..math_utils import (
    clamp,
)
from ..mood import antenna_drive_from_mood

class ExpressionMixin:
    """Mood-driven posture and antenna animation."""

    def _update_mood(self, dt: float) -> None:
        self.social_cooldown = max(0.0, self.social_cooldown - dt)
        # Movement is mildly energising; quiet idling settles the spider.
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        if speed01 > 0.25:
            self.mood.bump(arousal=dt * 0.12 * speed01)
        relax_rate = 0.35
        if self.state in ("Idle", "Wander"):
            relax_rate = 0.6
        self.mood.relax(dt, rate=relax_rate)

        # Smoothly relax the situational intents unless their state keeps them up.
        decay = math.exp(-dt * 3.2)
        if self.state not in ("Aim", "WebAim"):
            self.aim_intent *= decay
        if self.state != "Inspect":
            self.inspect_intent *= decay
        if self.state not in ("Cuddle",):
            self.cuddle_intent *= decay
        if self.state != "Catch":
            self.catch_blend *= math.exp(-dt * 5.0)

        # Blink.
        self.blink_timer -= dt
        if self.expression_blink > 0.0:
            self.expression_blink = max(0.0, self.expression_blink - dt * 7.0)
        elif self.blink_timer <= 0.0:
            self.expression_blink = 1.0
            self.blink_timer = self.rng.uniform(2.0, 6.0)

    def _update_posture(self, dt: float) -> None:
        m = self.mood
        # Decay transient channels.
        self.wiggle_burst = max(0.0, self.wiggle_burst - dt * 1.6)
        self.head_tilt *= math.exp(-dt * 2.5)
        if self.state not in ("Aim", "Coil", "WebAim"):
            self.crouch = max(0.0, self.crouch - dt * 3.0)
        if self.state not in ("Cuddle", "Aim"):
            self.rear = max(0.0, self.rear - dt * 3.5)
        # Landing squash recovers smoothly back to neutral.
        self.squash += (1.0 - self.squash) * (1.0 - math.exp(-dt * 10.0))
        if self.land_recover > 0.0:
            self.land_recover = max(0.0, self.land_recover - dt)

        # Idle/excitement wiggle amplitude target (whole-body wag, legs stay planted).
        base_wiggle = 0.02 + m.arousal * 0.05 + max(0.0, m.valence) * 0.04
        if self.current_speed > 30.0:
            base_wiggle *= 0.5
        self.wiggle_amp = base_wiggle + self.wiggle_burst * 0.22

        # Advance the wiggle clock; faster and wider when excited.
        self.wiggle_phase += dt * (2.2 + m.arousal * 3.5 + self.wiggle_burst * 6.0)
        self.body_wiggle = math.sin(self.wiggle_phase) * self.wiggle_amp
        # Abdomen wags a touch more than the front, like a happy tail.
        wag_drive = 0.03 + m.happy * 0.07 + m.affection * 0.05 + self.wiggle_burst * 0.10
        self.abdomen_wag = math.sin(self.wiggle_phase * 1.35 + 0.5) * wag_drive

        # Gaze tracks the focus target when it matters.
        gaze = clamp(self.focus_strength, 0.0, 1.0)
        if gaze > 0.05:
            local_f, local_s = self._world_to_body_local(self.focus_x, self.focus_y)
            mag = max(1e-3, math.hypot(local_f, local_s))
            tgt_f = local_f / mag
            tgt_s = local_s / mag
        else:
            tgt_f, tgt_s = 1.0, 0.0
        self.look_fwd += (tgt_f - self.look_fwd) * (1.0 - math.exp(-dt * 8.0))
        self.look_side += (tgt_s - self.look_side) * (1.0 - math.exp(-dt * 8.0))

    def _update_antennae(self, dt: float) -> None:
        m = self.mood
        speed01 = clamp(self.current_speed / 160.0, 0.0, 1.0)
        cfg = self._appearance("antennae", {})
        style = str(cfg.get("style", "")).strip().lower() if isinstance(cfg, dict) else ""
        custom_hand_palps = style in (
            "front_leg", "tarantula_front_legs", "tarantula_hand_palps"
        )

        # Tarantula pedipalps are short, load-bearing-looking hand appendages,
        # not insect feelers. Keep their idle clock deliberately slow so the
        # pose settles between small probing strokes instead of visibly chasing
        # every mouse update.
        if self.dragging:
            phase_rate = 0.0
        elif custom_hand_palps:
            # Keep a slow, visible palp-work cycle alive even when the legs are
            # planted.  A palp is a free sensory hand, not a stance leg: it can
            # make a larger exploratory stroke without waiting for the gait.
            phase_rate = 1.08 + m.arousal * 0.86 + m.curiosity * 0.58 + speed01 * 0.30
        else:
            phase_rate = 1.8 + m.arousal * 3.2 + m.curiosity * 1.6 + speed01 * 2.0
        for i in range(2):
            self.antenna_phase[i] += dt * phase_rate

        if not custom_hand_palps:
            return
        try:
            segments = max(3, int(cfg.get("segments", 5)))
        except (TypeError, ValueError):
            segments = 5

        drive = antenna_drive_from_mood(
            self.mood,
            aiming=0.0 if self.dragging else clamp(self.aim_intent, 0.0, 1.0),
            inspecting=0.0 if self.dragging else clamp(self.inspect_intent, 0.0, 1.0),
            cuddling=0.0 if self.dragging else clamp(max(self.cuddle_intent, self.catch_blend), 0.0, 1.0),
            aim_angle=self._antenna_aim_angle(),
        )
        # The first link must visibly lift like a spider-leg femur.  The
        # following links reverse through the knee and descend toward the
        # sensory hand; keeping the chain short prevents an antler silhouette.
        # Models may provide their own anatomical rest pose, but malformed
        # values fall back to this deliberately folded tarantula profile.
        # Absolute link bearings form a smooth serial curl. The old profile
        # changed most of its heading in the first two links, which made the
        # palp read like a row of rigid insect teeth instead of a hand bending
        # through its knuckles.
        default_angle_profile = [0.90, 0.56, 0.22, -0.13, -0.46]
        raw_angle_profile = cfg.get("rest_angles", default_angle_profile)
        if not isinstance(raw_angle_profile, list):
            raw_angle_profile = default_angle_profile
        try:
            angle_profile = [float(value) for value in raw_angle_profile[:segments]]
        except (TypeError, ValueError):
            angle_profile = []
        if len(angle_profile) != segments or any(
            not math.isfinite(value) for value in angle_profile
        ):
            angle_profile = list(default_angle_profile[:segments])
        lift_profile = [1.00, 0.55, 0.20, 0.06, 0.0]
        phase_offsets = [0.00, 0.12, 0.24, 0.34, 0.44]
        angle_profile = angle_profile[:segments]
        lift_profile = lift_profile[:segments]
        phase_offsets = phase_offsets[:segments]
        while len(angle_profile) < segments:
            angle_profile.append(angle_profile[-1] * 0.72)
        while len(lift_profile) < segments:
            lift_profile.append(max(0.0, lift_profile[-1] * 0.55))
        while len(phase_offsets) < segments:
            phase_offsets.append(phase_offsets[-1] + 0.14)

        # Pedipalps are controlled as a short serial chain, not as one pointer
        # aimed at the cursor.  The proximal link can make the largest useful
        # adjustment; the distal links have progressively smaller authority so
        # the knee and terminal claw keep their folded, hand-like silhouette.
        def _joint_profile(name: str, fallback: list[float]) -> list[float]:
            raw = cfg.get(name, fallback)
            if not isinstance(raw, list):
                raw = fallback
            try:
                values = [float(value) for value in raw[:segments]]
            except (TypeError, ValueError):
                values = []
            if len(values) != segments or any(
                not math.isfinite(value) or value < 0.0 for value in values
            ):
                values = list(fallback[:segments])
            while len(values) < segments:
                values.append(values[-1] if values else 0.0)
            return values

        joint_steering = _joint_profile(
            "joint_steering", [0.24, 0.40, 0.34, 0.24, 0.14]
        )
        joint_steering_limits = _joint_profile(
            "joint_steering_limits", [0.42, 0.66, 0.58, 0.44, 0.30]
        )
        serial_follow = clamp(float(cfg.get("serial_follow", 0.68)), 0.0, 0.94)
        joint_angle_limits = _joint_profile(
            "joint_angle_limits", [0.38, 0.60, 0.54, 0.40, 0.26]
        )
        # Walking legs are constrained by stance support.  Pedipalps are free
        # sensory/manipulation appendages, so give their knuckles a separate
        # larger ROM envelope.  This is a joint envelope, not extra segment
        # length: every rendered link still uses its fixed model length.
        free_rom = _joint_profile(
            "free_range_of_motion", [0.48, 0.78, 0.68, 0.54, 0.38]
        )
        joint_angle_limits = [
            max(limit, free_rom[index])
            for index, limit in enumerate(joint_angle_limits)
        ]
        joint_steering_limits = [
            max(limit, free_rom[index] * 1.12)
            for index, limit in enumerate(joint_steering_limits)
        ]

        if len(self.antenna_segment_angles) != 2:
            self.antenna_segment_angles = [[], []]
        if len(self.antenna_joint_lifts) != 2:
            self.antenna_joint_lifts = [[], []]
        if len(self.antenna_extension) != 2:
            self.antenna_extension = [0.0, 0.0]
        if len(self.antenna_hand_targets) != 2:
            self.antenna_hand_targets = [(0.0, 0.0), (0.0, 0.0)]
        if len(self.antenna_hand_grips) != 2:
            self.antenna_hand_grips = [0.0, 0.0]

        probing = 0.0 if self.dragging else clamp(max(self._feeler_pulse, self.inspect_intent * 0.55), 0.0, 1.0)
        # These appendages are sensory hands, not locomotion legs: their target
        # and stroke are driven by attention/contact intent, never by a leg's
        # stepping state or by the body gait clock.
        explicit_hand_target = (
            not self.dragging
            and (
                self.catch_blend > 0.08
                or self.inspect_intent > 0.40
                or self.aim_intent > 0.55
            )
        )
        target_world = None
        if explicit_hand_target:
            target_world = self.catch_point if self.catch_blend > 0.08 else (self.focus_x, self.focus_y)
        hand_activity = 0.0 if self.dragging else clamp(
            max(
                probing * 0.55,
                self.inspect_intent * 0.35,
                self.aim_intent * 0.25,
                self.catch_blend * 0.70,
            ),
            0.0, 1.0,
        )
        if explicit_hand_target:
            hand_activity = max(hand_activity, self.focus_strength * 0.25)
        proximal_rise = clamp(float(cfg.get("proximal_rise", 0.12)), 0.0, 0.28)
        rest_forward = clamp(float(cfg.get("hand_rest_forward", 1.02)), 0.28, 1.45)
        rest_lateral = clamp(float(cfg.get("hand_rest_lateral", 0.52)), 0.14, 1.10)
        min_forward = clamp(float(cfg.get("hand_min_forward", 0.34)), 0.16, rest_forward)
        max_forward = clamp(float(cfg.get("hand_max_forward", 1.52)), rest_forward, 2.20)
        min_lateral = clamp(float(cfg.get("hand_min_lateral", 0.20)), 0.10, rest_lateral)
        max_lateral = clamp(float(cfg.get("hand_max_lateral", 0.86)), rest_lateral, 1.30)
        rub_frequency = clamp(float(cfg.get("rub_frequency", 0.92)), 0.45, 1.80)
        rub_lateral_amount = clamp(float(cfg.get("rub_lateral_amount", 0.18)), 0.0, 0.34)
        rub_forward_amount = clamp(float(cfg.get("rub_forward_amount", 0.09)), 0.0, 0.20)
        rub_joint_angle = clamp(float(cfg.get("rub_joint_angle", 0.42)), 0.02, 0.72)
        rub_lift_amount = clamp(float(cfg.get("rub_lift_amount", 0.24)), 0.0, 0.42)
        knuckle_wave_delay = clamp(float(cfg.get("knuckle_wave_delay", 0.18)), 0.08, 0.36)
        knuckle_lift_delay = clamp(float(cfg.get("knuckle_lift_delay", 0.24)), 0.10, 0.44)
        try:
            ceph_scale = self._appearance("cephalothorax_scale", [0.70, 0.66])
            ceph_forward = float(self._appearance("cephalothorax_offset_x", 0.40))
            base_forward = ceph_forward + float(ceph_scale[0]) * float(cfg.get("base_forward", 0.34))
            base_lateral = float(ceph_scale[1]) * float(cfg.get("base_side", 0.26))
        except (TypeError, ValueError, IndexError):
            base_forward, base_lateral = 0.64, 0.18

        shared_phase = (self.antenna_phase[0] + self.antenna_phase[1]) * 0.5
        rub_phase = shared_phase * rub_frequency
        observing = self.state in ("Observe", "Inspect", "Aim", "Catch", "Alert")
        attention_level = clamp(
            max(
                self.inspect_intent,
                self.aim_intent,
                self.catch_blend,
                0.82 if observing and self.focus_strength > 0.15 else 0.0,
            ),
            0.0, 1.0,
        )
        # In a quiet state the rubbing is occasional and subtle. Attention or
        # a touch command opens the same stroke into a deliberate probe.
        idle_rub_gate = 0.5 + 0.5 * math.sin(rub_phase * 0.52 - 1.15)
        gesture_level = clamp(
            max(attention_level, idle_rub_gate * (0.52 if observing else 0.48)),
            0.0, 1.0,
        )
        if explicit_hand_target:
            gesture_level = max(gesture_level, 0.72 * hand_activity)
        # The flex wave peaks in the middle knuckles, then tapers toward the
        # small terminal claw. Every link follows the same curl with a short
        # delay; independently alternating waves made the old palp zigzag like
        # a row of teeth instead of bending as a small hand.
        joint_rub_profile = [0.32, 0.70, 1.00, 0.82, 0.56]

        for side_index, side_sign in enumerate((-1.0, 1.0)):
            if len(self.antenna_segment_angles[side_index]) != segments:
                self.antenna_segment_angles[side_index] = [side_sign * angle for angle in angle_profile]
            if len(self.antenna_joint_lifts[side_index]) != segments:
                self.antenna_joint_lifts[side_index] = [proximal_rise * lift for lift in lift_profile]

            side_phase = self.antenna_phase[side_index]
            phase = side_phase
            if custom_hand_palps and bool(cfg.get("symmetric_rest", False)):
                # The two pedipalps are independent when commanded, but their
                # neutral pose should be a mirrored pair.  A random phase per
                # side made one hand rise while the other sagged even at rest.
                phase = (self.antenna_phase[0] + self.antenna_phase[1]) * 0.5
            desired_f = rest_forward
            desired_s = side_sign * rest_lateral
            if target_world is not None:
                target_f, target_s = self._world_to_body_local(*target_world)
                target_f = target_f / max(1.0, self.size) - base_forward
                target_s = target_s / max(1.0, self.size) - side_sign * base_lateral
                target_f = clamp(target_f, min_forward, max_forward)
                target_s = side_sign * clamp(side_sign * target_s, min_lateral, max_lateral)
                desired_f += (target_f - desired_f) * hand_activity
                desired_s += (target_s - desired_s) * hand_activity
            elif not self.dragging:
                # Relaxed searching is a small shared hand stroke, not a
                # synchronized leg cadence.  Side mirroring is supplied by
                # the signed lateral target, so the rest silhouette stays
                # symmetrical until an explicit hand target arrives.
                desired_f += 0.006 * math.sin(phase)
                desired_s += side_sign * 0.004 * math.cos(phase * 0.91)
            if custom_hand_palps and not self.dragging:
                # A palp stroke is a small inward/outward hand-rub, not a
                # mouse-following reach. Both hands keep their mirrored lane;
                # the shared phase makes them close and open together.
                rub_wave = 0.5 + 0.5 * math.sin(rub_phase)
                desired_f += math.cos(rub_phase) * rub_forward_amount * gesture_level
                desired_s = side_sign * (
                    abs(desired_s)
                    - rub_lateral_amount * gesture_level * rub_wave
                )
            desired_f = clamp(desired_f, min_forward, max_forward)
            desired_s = side_sign * clamp(side_sign * desired_s, min_lateral, max_lateral)
            self.antenna_hand_targets[side_index] = (
                desired_f * self.size,
                desired_s * self.size,
            )
            hand_reach = math.hypot(desired_f, desired_s)
            extension_target = clamp(
                max(0.0, hand_reach - math.hypot(rest_forward, rest_lateral)) * 0.10
                + hand_activity * 0.025
                + max(0.0, math.sin(phase)) * 0.006,
                0.0, 0.10,
            )
            self.antenna_extension[side_index] += (
                extension_target - self.antenna_extension[side_index]
            ) * (1.0 - math.exp(-dt * 4.5))

            grip_target = 0.0 if self.dragging else clamp(
                self.catch_blend * 0.72 + self.inspect_intent * 0.24
                + hand_activity * 0.12
                + max(0.0, math.sin(phase)) * 0.03,
                0.0, 1.0,
            )
            self.antenna_hand_grips[side_index] += (
                grip_target - self.antenna_hand_grips[side_index]
            ) * (1.0 - math.exp(-dt * 9.0))

            inherited_delta = 0.0
            for segment_index in range(segments):
                delayed_phase = phase - phase_offsets[segment_index]
                joint_wave = math.sin(delayed_phase) * (0.018 + m.curiosity * 0.022)
                profile_angle = side_sign * angle_profile[segment_index]
                target_angle = profile_angle
                target_angle += side_sign * (drive.curl * 0.045 + (drive.base_angle - 0.62) * 0.18)
                target_angle += side_sign * joint_wave
                if target_world is not None:
                    desired_angle = math.atan2(desired_s, max(0.05, desired_f))
                    # Apply a bounded command to this knuckle only.  Keeping
                    # the delta relative to the anatomical rest angle means a
                    # target cannot flatten the chain into a straight pointer.
                    aim_delta = math.atan2(
                        math.sin(desired_angle - profile_angle),
                        math.cos(desired_angle - profile_angle),
                    )
                    target_angle += clamp(
                        aim_delta * joint_steering[segment_index],
                        -joint_steering_limits[segment_index],
                        joint_steering_limits[segment_index],
                    )
                # A sensory stroke starts at the proximal link and propagates
                # toward the toe; it is not a single rigid antenna sway.
                stroke = 0.5 + 0.5 * math.sin(delayed_phase)
                if probing > 0.01:
                    target_angle += side_sign * probing * (0.025 if segment_index < 2 else -0.018) * stroke
                if hand_activity > 0.01:
                    target_angle += side_sign * hand_activity * 0.010 * math.sin(delayed_phase * 0.85)
                if custom_hand_palps and not self.dragging:
                    # Let one coherent curl travel through the knuckles with a
                    # short proximal-to-distal delay. Keeping adjacent phases
                    # close prevents the alternating bends that read as scary
                    # insect teeth or antlers.
                    joint_wave_scale = joint_rub_profile[min(segment_index, len(joint_rub_profile) - 1)]
                    knuckle_phase = (
                        rub_phase
                        - segment_index * knuckle_wave_delay
                        + side_index * 0.08
                    )
                    target_angle += (
                        side_sign
                        * math.sin(knuckle_phase)
                        * rub_joint_angle
                        * gesture_level
                        * joint_wave_scale
                    )
                    # Preserve a little independent left/right hand motion,
                    # but keep it smooth along the chain instead of giving
                    # every knuckle a different direction.
                    side_wave = math.sin(
                        side_phase * 0.82 + 0.65 + segment_index * 0.04
                    )
                    target_angle += (
                        side_wave
                        * rub_joint_angle
                        * gesture_level
                        * 0.24
                    )
                    # Attention opens the hand in a second, gentle plane. The
                    # proximal links lift first and the distal links fold back,
                    # like a tiny hand feeling its way around an object.
                    if attention_level > 0.01:
                        probe_fold = math.sin(
                            rub_phase * 0.62
                            - segment_index * knuckle_wave_delay * 1.25
                            + side_index * 0.11
                        )
                        target_angle += (
                            side_sign
                            * attention_level
                            * rub_joint_angle
                            * (0.18 if segment_index < 2 else -0.11)
                            * probe_fold
                        )

                # A hand target can steer a knuckle, but it cannot erase the
                # folded anatomy of the palp.  Clamp every joint around its
                # signed rest angle so the appendage remains a chain instead
                # of becoming a straight cursor pointer.
                angle_delta = math.atan2(
                    math.sin(target_angle - profile_angle),
                    math.cos(target_angle - profile_angle),
                )
                if segment_index > 0:
                    # A real appendage joint rotates the links distal to it.
                    # Carry part of the previous knuckle's bend forward before
                    # clamping this joint. This creates a continuous curled
                    # palp rather than five independently angled "teeth".
                    angle_delta += inherited_delta * serial_follow * 0.56
                    angle_delta = math.atan2(math.sin(angle_delta), math.cos(angle_delta))
                target_angle = profile_angle + clamp(
                    angle_delta,
                    -joint_angle_limits[segment_index],
                    joint_angle_limits[segment_index],
                )
                inherited_delta = inherited_delta * 0.34 + (
                    target_angle - profile_angle
                ) * 0.66

                lift_target = proximal_rise * lift_profile[segment_index]
                lift_target *= 0.90 + hand_activity * 0.34 + self.antenna_hand_grips[side_index] * 0.16
                lift_target += proximal_rise * 0.08 * max(0.0, math.sin(delayed_phase))
                if custom_hand_palps and not self.dragging:
                    # Unlike the old one-way lift, this is signed: the
                    # knuckles gently rise and settle downward around their
                    # neutral pose. The proximal link has the largest motion;
                    # the claw remains small and controlled.
                    lift_phase = (
                        rub_phase
                        - segment_index * knuckle_lift_delay
                        + side_index * 0.09
                    )
                    lift_scale = 1.0 if segment_index < 2 else (0.68 if segment_index < 4 else 0.42)
                    lift_target += (
                        proximal_rise
                        * rub_lift_amount
                        * gesture_level
                        * math.sin(lift_phase)
                        * lift_scale
                    )
                    if attention_level > 0.01:
                        lift_target += (
                            proximal_rise
                            * attention_level
                            * (0.12 if segment_index < 2 else 0.06)
                            * math.sin(
                                rub_phase * 0.62
                                - segment_index * knuckle_lift_delay
                                + side_index * 0.13
                            )
                        )
                lift_target = clamp(lift_target, 0.0, 0.48)
                # The movement wave travels from the base toward the hand:
                # proximal joints lead and the distal claw settles last.
                angle_rate = max(4.6, 8.2 - segment_index * 0.50 + hand_activity * 1.4)
                lift_rate = max(4.8, 8.0 - segment_index * 0.56 + hand_activity * 1.0)
                angle_alpha = 1.0 - math.exp(-dt * angle_rate)
                lift_alpha = 1.0 - math.exp(-dt * lift_rate)
                self.antenna_segment_angles[side_index][segment_index] += (
                    target_angle - self.antenna_segment_angles[side_index][segment_index]
                ) * angle_alpha
                self.antenna_joint_lifts[side_index][segment_index] += (
                    lift_target - self.antenna_joint_lifts[side_index][segment_index]
                ) * lift_alpha

    def _antenna_aim_angle(self) -> float:
        local_f, local_s = self._world_to_body_local(self.focus_x, self.focus_y)
        return math.atan2(local_s, local_f)

