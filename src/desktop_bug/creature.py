from __future__ import annotations

import math
import os
import random
from pathlib import Path
from dataclasses import dataclass, field
from typing import List, Tuple

from .math_utils import (
    angle_lerp,
    angle_to,
    clamp,
    clamp_point,
    cursor_is_threatening,
    distance,
    rand_range,
    smoothstep,
)
from .mood import Mood, antenna_drive_from_mood, build_antenna_points
from .skills import SkillSet


def smootherstep(t: float) -> float:
    t = clamp(t, 0.0, 1.0)
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)


VALID_GAIT_STYLES = ("classic", "lively", "skitter")
GAIT_LABELS = {
    "classic": "Classic",
    "lively": "Lively (lifted legs)",
    "skitter": "Skitter (rapid bursts)",
}


def normalize_gait_style(style: str | None) -> str:
    style = str(style or "classic").strip().lower()
    return style if style in VALID_GAIT_STYLES else "classic"


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
    phase_seed: float = field(default_factory=lambda: random.random() * math.tau)
    twitch_clock: float = 0.0
    gait_phase_offset: float = field(default_factory=lambda: random.uniform(-0.42, 0.42))
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


class Creature:
    """One data-driven desktop creature with procedural or hybrid sprite-rig rendering."""

    SPRITE_CACHE = {}


    def __init__(
        self,
        model: dict,
        personality: dict,
        screen_w: int,
        screen_h: int,
        index: int = 0,
        size_scale: float = 1.0,
        skills: list[str] | None = None,
        gait_style: str = "classic",
    ):
        self.model = model
        self.personality = personality
        self.skills = SkillSet(skills if skills is not None else personality.get("skills"))
        self.screen_w = max(200, int(screen_w))
        self.screen_h = max(200, int(screen_h))
        self.margin = 50.0
        self.index = index

        self.size_scale = clamp(float(size_scale), 0.45, 2.25)
        self.size_jitter = random.uniform(0.90, 1.12)
        self.size = float(model.get("base_size", 25)) * self.size_jitter * self.size_scale
        self.x = random.uniform(self.margin, self.screen_w - self.margin)
        self.y = random.uniform(self.margin, self.screen_h - self.margin)
        self.heading = random.uniform(-math.pi, math.pi)
        self.target_heading = self.heading
        self.target_x = self.x
        self.target_y = self.y
        self.speed = 0.0
        self.current_speed = 0.0
        self.vel_x = 0.0
        self.vel_y = 0.0
        self.strafe_observe = False
        self.turn_rate = random.uniform(4.0, 6.0)
        self.state = "Idle"
        self.state_timer = rand_range(personality.get("idle_time"), 1.0, 3.0)
        self.decision_timer = random.uniform(0.2, 0.5)
        self.motion_paused = False
        self.chase_timer = 0.0
        self.hop_timer = rand_range(personality.get("hop_interval"), 0.25, 0.65)
        self.nope_repeats = 0
        self.nope_zigzag_dir = random.choice((-1.0, 1.0))
        self.nope_cooldown = 0.0

        # Direct manipulation / throw physics. The overlay stays click-through,
        # so the manager polls the global mouse button and calls these methods.
        self.dragging = False
        self.grab_offset_x = 0.0
        self.grab_offset_y = 0.0
        self.drag_vel_x = 0.0
        self.drag_vel_y = 0.0
        self.inertia_vx = 0.0
        self.inertia_vy = 0.0
        self.inertia_timer = 0.0
        self.startled_timer = 0.0
        self.startle_phase = random.random() * math.tau

        # Drift movement: a deliberate sideways slip layered on top of normal
        # running/retreat/throw motion.  It only becomes active for personalities
        # that opt in (for example the Drifter personality) and when the Drift
        # skill is enabled.
        self.drift_phase = random.random() * math.tau
        self.drift_dir = random.choice((-1.0, 1.0))
        self.drift_boost = 0.0
        self.drift_flip_timer = rand_range(personality.get("drift_switch_time"), 0.55, 1.35)
        self.last_drift_amount = 0.0
        self.drift_momentum = 0.0
        self.drift_lean = 0.0
        self.drift_slide_timer = 0.0

        # Autonomous Drifter flourishes.  These are separate from normal wander so
        # the Drifter personality can occasionally tear around the desktop in a
        # committed drift-run: circling in place, carving arcs along screen edges,
        # or skidding around corners.
        self.drift_run_cooldown = rand_range(personality.get("drift_run_interval"), 4.5, 10.5)
        self.drift_run_mode = "circle"
        self.drift_run_center_x = self.x
        self.drift_run_center_y = self.y
        self.drift_run_radius = max(self.size * 2.8, 70.0)
        self.drift_run_angle = self.heading
        self.drift_run_dir = random.choice((-1.0, 1.0))
        self.drift_run_turn_rate = 2.8
        self.drift_run_speed = 150.0
        self.drift_run_corner_index = 0
        self.drift_run_phase = "charge"
        self.drift_run_phase_timer = 0.0

        # Separate animation clocks. Leg gait is tied to speed, but body breathing is alive even at rest.
        self.bob_phase = random.random() * math.tau
        self.breath_phase = random.random() * math.tau
        self.body_bob = 0.0
        self.body_sway = 0.0
        self.abdomen_pulse = 0.0
        self.ceph_pulse = 0.0

        self.prev_mx = None
        self.prev_my = None
        self.prev_cursor_vx = 0.0
        self.prev_cursor_vy = 0.0

        self.colors = model.get("colors", {})
        self.legs: List[LegState] = [LegState(definition=dict(item)) for item in model.get("legs", [])]
        self.gait_groups = sorted({int(leg.definition.get("gait_group", 0)) for leg in self.legs}) or [0]
        self.active_gait_index = random.randrange(len(self.gait_groups))
        self.stepping_group = None
        # Movement style. "classic" keeps the original reactive gait. "lively"
        # drives a clearer alternating-tetrapod cadence, lifts the swinging legs
        # visibly off the ground, and probes with front feelers. "skitter" is
        # based on lively but runs in quick burst-burst-stop successions, like
        # a small jumping spider in a macro video.
        self.gait_style = normalize_gait_style(gait_style)
        self._lively_gait_phase = random.random()
        self._skitter_burst_timer = random.uniform(0.14, 0.34)
        self._skitter_pause_timer = 0.0
        self._skitter_phase = random.random() * math.tau
        self._skitter_burst_jitter = random.uniform(0.94, 1.16)
        self._prev_heading_gait = self.heading
        self._spider_locomotion_active = False
        self._spider_gait_frame_updated = False
        # Feeler-probe pacing (lively style only): a gap timer between probes and
        # the current probe pulse envelope.
        self._feeler_clock = random.uniform(0.4, 1.4)
        self._feeler_pulse = 0.0
        self._feeler_pulse_dur = 0.0
        self._feeler_pulse_t = 0.0
        # Per-leg angular territories for the lively gait (built below once the
        # leg list and size are known).  Keeps legs from crossing into a pinwheel.
        self._leg_sectors = {}
        self.idle_twitch_timer = random.uniform(0.6, 1.8)
        self.turn_rehome_pressure = 0.0

        # ------------------------------------------------------------------
        # Emotion, expression and body language
        # ------------------------------------------------------------------
        self.mood_mode = str(personality.get("mood", "auto")).lower()
        self.mood = Mood()
        self.mood.set_baseline_named(self.mood_mode if self.mood_mode != "auto" else personality.get("id", "auto"))
        # Slight per-individual emotional temperament so a group is not uniform.
        self.mood.bump(
            valence=random.uniform(-0.12, 0.12),
            arousal=random.uniform(-0.10, 0.10),
            affection=random.uniform(-0.10, 0.10),
            curiosity=random.uniform(-0.10, 0.10),
        )
        self.expression_blink = 0.0
        self.blink_timer = random.uniform(1.5, 5.0)
        self.head_tilt = 0.0            # body-local cosmetic head roll
        self.look_fwd = 1.0             # pupil gaze direction in body-local forward
        self.look_side = 0.0

        # Posture channels that move the body shell without sliding planted feet.
        self.body_wiggle = 0.0          # whole-body wag (radians), legs stay put
        self.abdomen_wag = 0.0          # extra abdomen sway (radians)
        self.crouch = 0.0               # 0 upright .. 1 coiled/low (jump prep, play bow)
        self.rear = 0.0                 # 0 flat .. 1 reared front (alert/excited)
        self.squash = 1.0               # landing squash-and-stretch (1 = neutral)
        self.wiggle_phase = random.random() * math.tau
        self.wiggle_amp = 0.0           # target amplitude driven by mood/intent
        self.wiggle_burst = 0.0         # transient extra wiggle (aim waggle, excitement)

        # Vertical hop fakery for a top-down view.
        self.jump_z = 0.0               # >0 == airborne height in pixels
        self.airborne = False
        self.jump_t = 0.0
        self.jump_duration = 0.0
        self.jump_peak = 0.0
        self.jump_from = (self.x, self.y)

        # Playful tumble/roll. roll_spin is an extra whole-body draw rotation and
        # roll_tuck pulls the rendered feet toward the body so it curls into a ball.
        self.roll_spin = 0.0
        self.roll_tuck = 0.0
        self.roll_dir = 1.0
        self.roll_total = 0.0
        self.roll_duration = 0.0
        self.roll_progress = 0.0
        self.roll_eased = 0.0
        self.roll_distance = 0.0
        self.roll_drift_dir = 0.0
        self.jump_to = (self.x, self.y)
        self.jump_after = "land"        # what to roll into on landing
        self.jump_kind = "hop"          # "hop" (in place) or "pounce" (toward target)
        self.land_recover = 0.0

        # Front-leg / feeler "catch" reach blend.
        self.catch_blend = 0.0
        self.catch_point = (self.x, self.y)

        # Focus target the eyes/feelers track (cursor by default).
        self.focus_x = self.x
        self.focus_y = self.y
        self.focus_strength = 0.0

        # Antenna animation clocks.
        self.antenna_phase = [random.random() * math.tau, random.random() * math.tau]
        self.aim_intent = 0.0           # smoothed 0..1 hunting-aim weight
        self.inspect_intent = 0.0       # smoothed 0..1 inspecting weight
        self.cuddle_intent = 0.0        # smoothed 0..1 cuddling weight

        # Social play with other creatures.
        self.neighbors: List["Creature"] = []
        self.social_target: "Creature" | None = None
        self.social_role = "none"       # "chase" / "flee" / "play"
        self.social_cooldown = random.uniform(1.5, 5.0)
        self.allow_social = True

        # User-assigned identity. Empty means the spider is unnamed and shows no
        # hover label. ``_hovered`` is refreshed each frame by the overlay.
        self.name = ""
        self._hovered = False

        # Optional containment cage this spider has been placed inside. When set
        # (and the spider is not being carried) its motion is clamped to the
        # cage interior so it cannot wander out on its own.
        self.cage = None

        # Desktop-window awareness channels.  The manager owns the actual Win32
        # window snapshot; each creature only keeps tiny animation state so it can
        # fade behind windows and use Explorer folders as portal entrances.
        self._desktop_visibility_alpha = 1.0  # kept for backwards-safe drawing/portal transitions
        self._desktop_hidden_timer = 0.0
        self._desktop_fully_hidden = False
        self._desktop_occluding_keys = set()
        self._desktop_seek_timer = rand_range(personality.get("desktop_hide_seek_interval"), 28.0, 64.0)
        # Window/folder hiding is now intentional instead of automatic.  The
        # manager sets this to the specific top-visible surface the spider decided
        # to enter; ordinary movement, mouse chasing, and lower covered windows do
        # not make it disappear.
        self._desktop_hide_surface_key = None
        self._desktop_hide_timer = 0.0
        self._desktop_hide_kind = None
        # Newly-created spiders should always be visible first.  Window hiding
        # only becomes active after this grace period and only for surfaces the
        # spider has actually approached from outside.  This prevents a maximized
        # app or an already-overlapping window from making every spider vanish at
        # startup.
        self._desktop_spawn_grace = 2.4
        self._desktop_surface_keys_seen_outside = set()
        self._folder_portal_cooldown = rand_range(personality.get("folder_portal_cooldown"), 24.0, 55.0)
        self._folder_portal_dwell = 0.0
        self._folder_portal_threshold = rand_range(personality.get("folder_portal_dwell"), 0.85, 1.39)

        # Camouflage personalities are drawn with lower opacity only. Earlier
        # builds sampled/recoloured against the desktop; that was expensive and
        # looked muddy, so colour blending now remains off unless explicitly set
        # in a custom personality.
        self._camouflage_color = None
        self._camouflage_strength = 0.0
        # Camouflage is now social/touch-aware: the spider stays fully visible
        # after spawning or after any touch/grab, then slowly hides only after it
        # has been left alone for a while.
        self._camouflage_idle_timer = 0.0
        self._camouflage_visible_timer = rand_range(personality.get("camouflage_initial_visible_time"), 3.5, 6.0)

        # Cached screen-space bounding box (x0, y0, x1, y1) for partial repaints.
        # Recomputed on demand; the previous frame's box is unioned so a moving
        # spider always erases its old footprint cleanly.
        self._bbox: Tuple[float, float, float, float] | None = None
        self._bbox_prev: Tuple[float, float, float, float] | None = None

        # Web weaving / walking.  ``web_world`` is the shared world object set by
        # the manager after construction; webs are shared so one spider can build
        # a web and another can walk it or finish it when abandoned.
        self.web_world = None
        self.weaving_web = None          # the Web this spider is currently building
        self.repairing_web = None        # the Web this spider is currently mending
        self.web_target = None           # the Web it is walking onto to bounce-test
        self._weave_drawing = False      # is the silk tip drawing right now
        self._web_pluck_count = 0        # plucks remaining in the current web walk
        self._web_pluck_timer = 0.0
        self._web_wiggle = 0.0
        weave_bias = 4.0 if (self._personality_flag("web_weaver")
                             or self._personality_flag("webber")) else 1.0
        # Webbers come off cooldown quickly and often; everyone else rarely.
        self.weave_cooldown = rand_range(personality.get("weave_cooldown"),
                                         6.0, 14.0) / weave_bias
        self.web_walk_cooldown = rand_range(personality.get("web_walk_cooldown"), 8.0, 20.0)
        self._weave_speed = rand_range(personality.get("weave_speed"), 250.0, 330.0)

        # Cursor-trapping silk. ``mouse_web_world`` is the shared world object set
        # by the manager after construction. The spider shoots a glob of sticky
        # silk at the real pointer that pins it (trap) or shoves it to a wall.
        self.mouse_web_world = None
        self._web_shot_kind = "trap"      # which shot the current aim will fire
        shot_bias = 5.0 if self._personality_flag("web_shooter") else 1.0
        self.web_shot_cooldown = rand_range(personality.get("web_shot_cooldown"),
                                            8.0, 18.0) / shot_bias

        # Fly hunting.  When the manager points this spider at a fly it feeds the
        # fly's position in place of the cursor so the normal hunting/approach/
        # pounce behaviour applies to the prey.  ``_hunting_prey`` flips on for
        # those frames; while it is set the spider does not treat its fast-moving
        # prey as a threat to flee from, and it never yanks the real mouse pointer
        # with capture silk (the manager traps flies through the fly world
        # instead).  The remaining fields are predator bookkeeping the manager
        # owns; they live here so every spider carries them through rebuilds.
        self._hunting_prey = False
        self._prey = None
        self._prey_recheck = 0.0
        self._feed_cooldown = 0.0
        self._pounce_cooldown = random.uniform(0.0, 0.7)
        self._trap_shot_cooldown = random.uniform(0.4, 1.6)
        # The fly world (set by the manager) plus the fly a web shot is aimed at,
        # so the spider can fling its trapping silk at prey, not just the cursor.
        self.fly_world = None
        self._web_shot_prey = None

        self._compute_leg_sectors()
        self._initialize_legs()

    # ------------------------------------------------------------------
    # Geometry helpers
    # ------------------------------------------------------------------
    def _basis(self) -> Tuple[float, float, float, float]:
        fx = math.cos(self.heading)
        fy = math.sin(self.heading)
        # Screen-space right vector. At heading 0, right points downward.
        rx = -math.sin(self.heading)
        ry = math.cos(self.heading)
        return fx, fy, rx, ry

    @staticmethod
    def _side_sign(side: str) -> float:
        return -1.0 if str(side).lower().startswith("l") else 1.0

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
        sec = getattr(self, "_leg_sectors", {}).get(id(leg))
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
        """
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
        return max(min(chain_limit, target_limit), rest_allowance, self.size * 0.92)

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
        """
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
                "swing_fraction": clamp(float(raw.get("swing_fraction", 0.29)), 0.20, 0.40),
                # One clock drives both the group launch window and the swing
                # duration.  This prevents the old 47 ms window / 122 ms swing
                # mismatch that caused most steps to start outside their phase.
                "cycle_hz": clamp(float(raw.get("cycle_hz", 2.10)), 1.20, 4.50),
                "speed_cycle_gain": clamp(float(raw.get("speed_cycle_gain", 1.45)), 0.0, 3.0),
                "step_lookahead": clamp(float(raw.get("step_lookahead", 0.72)), 0.25, 1.20),
                "front_phase_offset": clamp(float(raw.get("front_phase_offset", 0.035)), 0.0, 0.10),
                "refractory_fraction": clamp(float(raw.get("refractory_fraction", 0.16)), 0.04, 0.35),
                # Limits for the support-driven body solver.  They are expressed
                # in body-size units so the same controller behaves consistently
                # for differently scaled Snowpuff-2 instances.
                "support_stroke_limit": clamp(float(raw.get("support_stroke_limit", 0.62)), 0.35, 1.10),
                "support_turn_limit": clamp(float(raw.get("support_turn_limit", 0.58)), 0.25, 1.20),
                "support_blend_time": clamp(float(raw.get("support_blend_time", 0.10)), 0.04, 0.30),
                "max_body_turn_rate": clamp(float(raw.get("max_body_turn_rate", 2.60)), 0.80, 4.50),
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
        total_weight = 0.0
        q_f_sum = q_s_sum = p_x_sum = p_y_sum = 0.0
        for leg in supports:
            stroke_f = leg.stance_stroke_f - move_f * scale
            stroke_s = leg.stance_stroke_s - move_s * scale
            stance_turn = leg.stance_turn - turn_delta * scale
            base_f, base_s = self._spider_rotate_local(
                leg.stance_base_f, leg.stance_base_s, stance_turn
            )
            q_f = base_f + stroke_f
            q_s = base_s + stroke_s
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
        max_heading_step = config["max_body_turn_rate"] * max(0.001, getattr(self, "_spider_solver_dt", 0.016))
        heading_step = ((solved_heading - self.heading + math.pi) % math.tau) - math.pi
        heading_step = clamp(heading_step, -max_heading_step, max_heading_step)
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
            if side * local_s < min_side:
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

    def _spider_locomotion_intent(self, dt: float) -> Tuple[float, float, float]:
        """Convert behavior state into a local stroke and turn request."""
        dx = self.target_x - self.x
        dy = self.target_y - self.y
        target_dist = math.hypot(dx, dy)
        strafe = self.state == "Observe" and self._is_observer_personality()
        self.strafe_observe = strafe
        if target_dist > 2.0 and not strafe:
            self.target_heading = math.atan2(dy, dx)

        desired_speed = self.speed if not self.motion_paused else 0.0
        if target_dist < 15.0 and self.state not in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun"):
            desired_speed *= target_dist / 15.0
        angle_error = ((self.target_heading - self.heading + math.pi) % math.tau) - math.pi
        alignment = clamp(1.0 - abs(angle_error) / (math.pi * 0.75), 0.15, 1.0)
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
        requested_turn = clamp(angle_error, -self.turn_rate * turn_mult * dt, self.turn_rate * turn_mult * dt)
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
            if not leg.stepping and not leg.pending_step:
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
            spider_gait = self._spider_gait_config()
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
            leg.foot_x, leg.foot_y = self._constrain_leg_point(leg, fx + random.uniform(-2.0, 2.0), fy + random.uniform(-2.0, 2.0))
            leg.step_target_x = leg.foot_x
            leg.step_target_y = leg.foot_y
            leg.twitch_clock = random.uniform(0.0, 1.0)
            try:
                leg.joint_phase = float(leg.definition.get("phase_offset", 0.0)) * math.tau + leg.phase_seed * 0.11
            except (TypeError, ValueError):
                leg.joint_phase = leg.phase_seed * 0.11
            leg.joint_bends = []
            self._spider_reanchor_stance(leg)

    # ------------------------------------------------------------------
    # FSM entry helpers. These intentionally set targets immediately.
    # ------------------------------------------------------------------
    def enter_idle(self) -> None:
        self.state = "Idle"
        self.speed = 0.0
        self.motion_paused = False
        self.state_timer = rand_range(self.personality.get("idle_time"), 1.0, 3.0)

    def enter_alert(self, mx: float, my: float) -> None:
        self.state = "Alert"
        self.speed = 0.0
        self.motion_paused = False
        self.target_x = mx
        self.target_y = my
        self.state_timer = rand_range(self.personality.get("alert_time"), 0.3, 0.9)

    def enter_approach(self, mx: float, my: float) -> None:
        if not self.has_skill("approach"):
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
            self.speed = 52.0 * float(self.personality.get("speed_multiplier", 1.0))
        self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3)
        self._prime_drift(0.35)

    def enter_chase(self, mx: float, my: float) -> None:
        if not self.has_skill("chase"):
            if self.has_skill("approach"):
                self.enter_approach(mx, my)
            else:
                self.enter_alert(mx, my)
            return
        self.state = "Chase"
        self.motion_paused = False
        self.target_x = mx + random.uniform(-16.0, 16.0)
        self.target_y = my + random.uniform(-16.0, 16.0)
        base_speed = float(self.personality.get("hunt_chase_speed", 150.0)) if self._is_hunter_personality() else 112.0
        self.speed = base_speed * float(self.personality.get("speed_multiplier", 1.0))
        self.chase_timer = float(self.personality.get("chase_persistence", 2.5))
        if self._is_hunter_personality():
            self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.06, 0.14)
        else:
            self.state_timer = random.uniform(0.12, 0.28)
        self._prime_drift(0.85)

    def enter_retreat(self, mx: float, my: float) -> None:
        if not self.has_skill("run_away"):
            self.enter_alert(mx, my)
            return
        angle_away = math.atan2(self.y - my, self.x - mx)
        distance_away = rand_range(self.personality.get("retreat_distance"), 220.0, 380.0)
        self.target_x = self.x + math.cos(angle_away) * distance_away
        self.target_y = self.y + math.sin(angle_away) * distance_away
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.state = "Retreat"
        self.motion_paused = False
        self.speed = 148.0 * float(self.personality.get("speed_multiplier", 1.0))
        self.state_timer = rand_range(self.personality.get("retreat_time"), 0.6, 1.2)
        self._prime_drift(0.95)

    def enter_startled(self, mx: float, my: float) -> None:
        """Brief startled mode used after being grabbed or thrown."""
        self.state = "Startled"
        self.motion_paused = False
        self.startled_timer = max(self.startled_timer, 1.15)
        away = math.atan2(self.y - my, self.x - mx)
        throw_speed = math.hypot(self.inertia_vx, self.inertia_vy)
        if throw_speed > 60.0:
            # Continue with the throw first, then self-correct once friction wins.
            heading = math.atan2(self.inertia_vy, self.inertia_vx)
            distance_out = clamp(throw_speed * 0.28, 80.0, 300.0)
        else:
            heading = away
            distance_out = random.uniform(80.0, 160.0)
        self.target_x = self.x + math.cos(heading) * distance_out
        self.target_y = self.y + math.sin(heading) * distance_out
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.target_heading = heading
        self.speed = 74.0 * float(self.personality.get("speed_multiplier", 1.0))
        self.state_timer = random.uniform(0.65, 1.15)
        throw_boost = clamp(math.hypot(self.inertia_vx, self.inertia_vy) / 580.0, 0.45, 1.45)
        self._prime_drift(throw_boost)

    def enter_wander(self) -> None:
        if not self.has_skill("wander"):
            self.enter_idle()
            return
        self.state = "Wander"
        self.motion_paused = False
        angle = random.uniform(-math.pi, math.pi)
        dist = random.uniform(120.0, 320.0)
        self.target_x = self.x + math.cos(angle) * dist
        self.target_y = self.y + math.sin(angle) * dist
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.speed = 38.0 * float(self.personality.get("speed_multiplier", 1.0))
        self.state_timer = random.uniform(1.4, 3.2)
        self._prime_drift(0.25)

    # ------------------------------------------------------------------
    # Expressive behaviours: inspect, cuddle, jump/pounce, play
    # ------------------------------------------------------------------
    def _speed_mult(self) -> float:
        return float(self.personality.get("speed_multiplier", 1.0))

    def _personality_flag(self, key: str, default: bool = False) -> bool:
        value = self.personality.get(key, default)
        if isinstance(value, str):
            return value.strip().lower() in ("1", "true", "yes", "on")
        return bool(value)

    def has_skill(self, skill_id: str) -> bool:
        """Return whether this live creature may use one selectable ability."""
        return self.skills.has(skill_id)

    def skill_ids(self) -> list[str]:
        return self.skills.ids()

    def set_skill_enabled(self, skill_id: str, enabled: bool) -> None:
        self.skills.set_enabled(skill_id, enabled)
        sid = str(skill_id).strip().lower()
        # Aiming/firing sticky silk depends on which shot is in progress; cancel
        # cleanly if the matching skill is switched off mid-aim.
        if self.state in ("WebAim", "WebShot") and not enabled:
            needed = "wall_web" if self._web_shot_kind == "wall" else "shoot_web"
            if sid == needed:
                self._finish_web_shot(fired=False)
                return
        # If the user disables a skill while this spider is actively using it,
        # leave that state immediately instead of letting a stale action continue.
        active_requirements = {
            "Approach": ("approach",),
            "Wander": ("wander",),
            "Chase": ("chase",),
            "Retreat": ("run_away",),
            "Observe": ("observe",),
            "Inspect": ("inspect",),
            "Cuddle": ("cuddle",),
            "Aim": ("prepare_jump_attack", "jump"),
            "Coil": ("jump",),
            "Jump": ("jump",),
            "Land": ("jump",),
            "Catch": ("prepare_jump_attack",),
            "Play": ("social_play",),
            "Zoom": ("zoomies",),
            "DriftRun": ("drift",),
            "Roll": ("roll",),
            "WeaveApproach": ("weave_web",),
            "Weave": ("weave_web",),
            "RepairApproach": ("weave_web",),
            "Repair": ("weave_web",),
            "WebApproach": ("web_walk",),
            "WebWalk": ("web_walk",),
        }.get(self.state, ())
        if sid in active_requirements and not enabled:
            self.social_target = None
            self.airborne = False
            self.jump_z = 0.0
            self.roll_spin = 0.0
            self.roll_tuck = 0.0
            if self.state in ("WeaveApproach", "Weave"):
                self._abandon_weaving()
            if self.state in ("RepairApproach", "Repair"):
                self._abandon_repair()
            self.web_target = None
            self.enter_idle()

    def _is_hunter_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self._personality_flag("mouse_hunter") or pid == "hunter") and self.has_skill("chase")

    def _is_jumper_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self._personality_flag("constant_small_hops") or pid == "jumper") and self.has_skill("jump")

    def _is_observer_personality(self) -> bool:
        pid = str(self.personality.get("id", "")).lower()
        return (self._personality_flag("horizontal_orbit_observer") or pid == "observer") and self.has_skill("observe")

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
            rand_range(self.personality.get("drift_switch_time"), 1.15, 2.4),
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
            if random.random() < float(self.personality.get("drift_flip_chance", 0.18)):
                self.drift_dir *= -1.0
                self.wiggle_burst = max(self.wiggle_burst, 0.34)
            self.drift_flip_timer = rand_range(self.personality.get("drift_switch_time"), 1.15, 2.4)

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

    def _cursor_speed(self) -> float:
        return math.hypot(self.prev_cursor_vx, self.prev_cursor_vy)

    def _cursor_is_still_for_observe(self) -> bool:
        # When the manager substitutes a fly for the cursor, use the fly's
        # explicit stop/walk state instead of a smoothed pixel-speed estimate.
        # That makes Hunter stalking switch exactly with the prey's movement.
        if self._hunting_prey:
            prey = getattr(self, "_prey", None)
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
            if dist_to_cursor > cursor_range or dist_to_mate <= dist_to_cursor * 1.12 or random.random() < social_pref:
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
        self.hop_timer = rand_range(self.personality.get("hop_interval"), 0.18, 0.42)
        power = rand_range(self.personality.get("hop_power"), 0.34, 0.58)
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
        if not self.has_skill("run_away"):
            self.enter_alert(mx, my)
            return
        if not self.has_skill("jump"):
            self.enter_retreat(mx, my)
            return

        if not continuing:
            count_pair = self.personality.get("nope_jump_count", [3, 5])
            if isinstance(count_pair, (list, tuple)) and len(count_pair) >= 2:
                self.nope_repeats = random.randint(int(count_pair[0]), int(count_pair[1]))
            else:
                self.nope_repeats = int(count_pair) if count_pair is not None else 4
            self.nope_zigzag_dir = random.choice((-1.0, 1.0))
            self.mood.bump(arousal=0.6, valence=-0.22, curiosity=-0.12)
        else:
            self.nope_repeats = max(0, self.nope_repeats - 1)

        away = math.atan2(self.y - my, self.x - mx)
        if distance(self.x, self.y, mx, my) < 1.0:
            away = self.heading + math.pi
        self.nope_zigzag_dir *= -1.0
        jump_dist = rand_range(self.personality.get("nope_jump_distance"), self.size * 4.5, self.size * 8.0)
        zigzag = rand_range(self.personality.get("nope_zigzag_distance"), self.size * 2.2, self.size * 4.6)
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
        if not self.has_skill("inspect"):
            self.enter_idle()
            return
        self.state = "Inspect"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.speed = 46.0 * self._speed_mult()
        self.state_timer = random.uniform(2.2, 4.8)
        self.inspect_phase = "approach"
        self.inspect_clock = 0.0
        self.orbit_dir = random.choice((-1.0, 1.0))
        self.mood.bump(curiosity=0.22, arousal=0.06)

    def enter_observe(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        if not self.has_skill("observe"):
            if self.has_skill("inspect"):
                self.enter_inspect(tx, ty, target)
            else:
                self.enter_idle()
            return
        self.state = "Observe"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.observe_dir = random.choice((-1.0, 1.0))
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
        self.state_timer = rand_range(self.personality.get("observe_orbit_time"), 2.8, 6.4)
        self.mood.bump(curiosity=0.18, arousal=0.04)

    def enter_cuddle(self, tx: float, ty: float, target: "Creature" | None = None) -> None:
        if not self.has_skill("cuddle"):
            self.enter_idle()
            return
        self.state = "Cuddle"
        self.motion_paused = False
        self.social_target = target
        self._set_focus(tx, ty, 1.0)
        self.target_x, self.target_y = tx, ty
        self.speed = 40.0 * self._speed_mult()
        self.state_timer = random.uniform(3.0, 6.5)
        self.cuddle_phase = "approach"
        self.cuddle_clock = 0.0
        self.boop_timer = random.uniform(0.4, 0.9)
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
        if not self.has_skill("prepare_jump_attack") or not self.has_skill("jump"):
            if after == "play" and target is not None and self.has_skill("social_play"):
                self.enter_play(target)
            elif self.has_skill("chase"):
                self.enter_chase(tx, ty)
            elif self.has_skill("approach"):
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
        self.state_timer = rand_range(ranging, ranging[0], ranging[1])
        self.aim_after = after
        self.aim_abort_chance = clamp(abort_chance, 0.0, 0.9)
        self.range_clock = 0.0
        self.range_mode = "waggle"
        self.range_switch = random.uniform(0.18, 0.34)
        self.mood.bump(arousal=0.16, curiosity=0.05)

    def enter_coil(self, after: str = "idle", power: float = 1.0, toward: Tuple[float, float] | None = None) -> None:
        """Quick wind-up for a playful spring/hop."""
        if not self.has_skill("jump"):
            if after == "wander":
                self.enter_wander()
            elif after == "approach" and toward is not None:
                self.enter_approach(toward[0], toward[1])
            elif after == "chase" and toward is not None:
                self.enter_chase(toward[0], toward[1])
            else:
                self.enter_idle()
            return
        self.state = "Coil"
        self.motion_paused = True
        self.speed = 0.0
        self.state_timer = random.uniform(0.10, 0.20)
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
            self.jump_peak = peak if peak is not None else self.size * random.uniform(0.85, 1.25) * coil_power * hop_peak_mult
            self.jump_duration = duration if duration is not None else random.uniform(0.36, 0.5) * hop_duration_mult
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
                peak_mult = random.uniform(float(peak_pair[0]), float(peak_pair[1]))
            else:
                peak_mult = float(peak_pair) if peak_pair is not None else 1.8
            self.jump_peak = peak if peak is not None else self.size * peak_mult
            self.jump_duration = duration if duration is not None else rand_range(self.personality.get("nope_jump_duration"), 0.14, 0.22)
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
        self.squash = 0.66
        self.land_recover = 0.22
        if after == "nope":
            self.state_timer = rand_range(self.personality.get("nope_land_pause"), 0.02, 0.05)
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
        self.state_timer = random.uniform(0.45, 0.72)
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
        self._feed_frenzy = random.uniform(0.5, 0.75)
        self._feed_anchor = (self.x, self.y)
        self._feed_paw = 0.0
        self.state_timer = self._feed_frenzy + random.uniform(0.4, 0.6)
        self.mood.bump(valence=0.45, arousal=0.3, affection=0.05, curiosity=0.05)

    def enter_play(self, target: "Creature", role: str | None = None) -> None:
        if not self.has_skill("social_play"):
            if self.has_skill("inspect"):
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
        self.state_timer = random.uniform(2.6, 5.5)
        self.play_clock = 0.0
        self.play_lookback = random.uniform(0.5, 1.1)
        self._prime_drift(0.55)
        self.mood.bump(valence=0.16, arousal=0.2, curiosity=0.08)

    def enter_zoomies(self) -> None:
        if not self.has_skill("zoomies"):
            self.enter_idle()
            return
        self.state = "Zoom"
        self.motion_paused = False
        angle = random.uniform(-math.pi, math.pi)
        dist = random.uniform(120.0, 260.0)
        self.target_x = self.x + math.cos(angle) * dist
        self.target_y = self.y + math.sin(angle) * dist
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
        self.speed = 138.0 * self._speed_mult()
        self.state_timer = random.uniform(0.35, 0.7)
        self.zoom_repeats = random.randint(1, 3)
        self._prime_drift(1.0)
        self.mood.bump(valence=0.14, arousal=0.24)

    def _reset_drift_run_cooldown(self) -> None:
        self.drift_run_cooldown = rand_range(self.personality.get("drift_run_interval"), 4.5, 10.5)

    def _should_start_drift_run(self, *, from_idle: bool) -> bool:
        if not self._is_drifter_personality() or self.airborne or self.dragging:
            return False
        if self.state in ("Dragged", "Startled", "Retreat", "Chase", "Jump", "Land", "Roll", "Coil", "Catch"):
            return False
        if float(getattr(self, "drift_run_cooldown", 0.0)) > 0.0:
            return False
        chance_key = "drift_run_start_chance" if from_idle else "drift_run_wander_chance"
        fallback = 0.86 if from_idle else 0.34
        return random.random() < clamp(float(self.personality.get(chance_key, fallback)), 0.0, 1.0)

    def enter_drift_run(self, mode: str | None = None) -> None:
        """Autonomous Drifter burst: fast circular/corner screen drifting."""
        if not self._is_drifter_personality():
            self.enter_idle()
            return

        self.state = "DriftRun"
        self.motion_paused = False
        self.social_target = None
        self.drift_run_dir = random.choice((-1.0, 1.0))
        self.drift_dir = self.drift_run_dir
        self.state_timer = rand_range(self.personality.get("drift_run_time"), 2.2, 4.6)
        self.drift_run_turn_rate = rand_range(self.personality.get("drift_circle_turn_rate"), 0.95, 1.85)
        self.drift_run_radius = rand_range(self.personality.get("drift_circle_radius"), max(92.0, self.size * 4.0), max(230.0, self.size * 8.0))
        self.drift_run_phase = "charge"
        self.drift_run_phase_timer = rand_range(self.personality.get("drift_charge_time"), 0.55, 1.05)
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
            mode = "corner" if random.random() < corner_chance else "circle"
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
            if len(ranked) > 1 and random.random() < 0.32:
                idx, (cx, cy) = random.choice(ranked[:2])
            else:
                idx, (cx, cy) = ranked[0]
            self.drift_run_corner_index = idx
            self.drift_run_center_x = clamp(cx, self.margin, self.screen_w - self.margin)
            self.drift_run_center_y = clamp(cy, self.margin, self.screen_h - self.margin)
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, self.x, self.y)
            self.drift_run_turn_rate *= 0.86
        else:
            radial = random.uniform(-math.pi, math.pi)
            cx = self.x - math.cos(radial) * radius
            cy = self.y - math.sin(radial) * radius
            low_x = min(self.screen_w * 0.5, self.margin + radius)
            high_x = max(low_x, self.screen_w - self.margin - radius)
            low_y = min(self.screen_h * 0.5, self.margin + radius)
            high_y = max(low_y, self.screen_h - self.margin - radius)
            self.drift_run_center_x = clamp(cx, low_x, high_x)
            self.drift_run_center_y = clamp(cy, low_y, high_y)
            self.drift_run_angle = angle_to(self.drift_run_center_x, self.drift_run_center_y, self.x, self.y)

        self.drift_run_speed = rand_range(self.personality.get("drift_run_speed"), 145.0, 205.0)
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
            self.drift_run_phase_timer = rand_range(self.personality.get("drift_slide_time"), 0.95, 1.65)
            self._prime_drift(1.18, direction=self.drift_run_dir)
            self.wiggle_burst = max(self.wiggle_burst, 0.42)
            phase = "slide"
        elif phase == "slide" and self.drift_run_phase_timer <= 0.0 and self.state_timer > 0.45:
            self.drift_run_phase = "charge"
            self.drift_run_phase_timer = rand_range(self.personality.get("drift_charge_time"), 0.45, 0.95)
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
            self.decision_timer = random.uniform(0.42, 0.80)
            if phase == "charge" and random.random() < float(self.personality.get("drift_run_flip_chance", 0.07)):
                self.drift_run_dir *= -1.0
                self.drift_dir = self.drift_run_dir
                self.wiggle_burst = max(self.wiggle_burst, 0.36)

        if self.state_timer <= 0.0:
            repeat_chance = float(self.personality.get("drift_run_chain_chance", 0.18))
            if random.random() < repeat_chance:
                next_mode = "corner" if self.drift_run_mode == "circle" and random.random() < 0.48 else None
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
        if not self.has_skill("roll"):
            self.enter_idle()
            return
        self.state = "Roll"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = None
        self.roll_dir = random.choice((-1.0, 1.0))
        turns = random.uniform(1.0, 2.0)
        self.roll_total = math.tau * turns
        self.roll_duration = random.uniform(0.7, 1.15)
        self.roll_progress = 0.0
        self.roll_eased = 0.0
        self.roll_spin = 0.0
        self.roll_tuck = 0.0
        self.roll_distance = self.size * random.uniform(1.6, 3.4)
        if direction is None:
            direction = random.uniform(-math.pi, math.pi)
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
        roll = random.random() * total
        if roll < catch_w:
            self.enter_catch(tx, ty, target)
        elif roll < catch_w + cuddle_w and in_range:
            self.mood.bump(valence=0.2, affection=0.12)
            self.enter_cuddle(tx, ty, target)
        else:
            self.mood.bump(valence=-0.12, arousal=0.22)
            self.enter_retreat(tx, ty)

    # ------------------------------------------------------------------
    # Social target selection
    # ------------------------------------------------------------------
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

    # ------------------------------------------------------------------
    # Dragging / hit testing
    # ------------------------------------------------------------------
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

    def _startle_amount(self) -> float:
        if self.dragging:
            return 1.0
        return clamp(self.startled_timer / 1.15, 0.0, 1.0)

    def _eye_startle_amount(self, startle: float) -> float:
        """Reduced startle intensity for eyes so grabbing does not balloon them."""
        # Keep the cute startled expression, but make the size change subtle.
        # During a grab ``_startle_amount`` is pinned at 1.0, which previously
        # made eyes grow by ~70-80%.  This caps the visual eye reaction to a
        # much smaller amount while leaving body/leg startle motion unchanged.
        cap = 0.28 if self.dragging else 0.45
        return clamp(startle, 0.0, 1.0) * cap

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

    def hit_test(self, mx: float, my: float) -> bool:
        """Return True when a mouse press is close enough to grab this spider."""
        local_f, local_s = self._world_to_body_local(mx, my)
        abdomen_scale = self._appearance("abdomen_scale", [0.96, 0.86])
        ceph_scale = self._appearance("cephalothorax_scale", [0.76, 0.70])
        abdomen_offset_x = float(self._appearance("abdomen_offset_x", -0.18)) * self.size
        ceph_offset_x = float(self._appearance("cephalothorax_offset_x", 0.38)) * self.size

        for cx, rx, ry in (
            (abdomen_offset_x, self.size * float(abdomen_scale[0]) * 0.62, self.size * float(abdomen_scale[1]) * 0.58),
            (ceph_offset_x, self.size * float(ceph_scale[0]) * 0.62, self.size * float(ceph_scale[1]) * 0.58),
        ):
            if ((local_f - cx) / max(1.0, rx)) ** 2 + (local_s / max(1.0, ry)) ** 2 <= 1.25:
                return True

        # Give legs a thin pick radius too, so grabbing feels natural on small models.
        leg_pick_radius = max(5.0, self.size * 0.16)
        for leg in self.legs:
            ax, ay = self._leg_attach(leg)
            fx, fy = self._visual_foot_for_render(leg)
            kx, ky = self._solve_knee(ax, ay, fx, fy, leg)
            if self._point_segment_distance(mx, my, ax, ay, kx, ky) <= leg_pick_radius:
                return True
            if self._point_segment_distance(mx, my, kx, ky, fx, fy) <= leg_pick_radius:
                return True
        return False

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
            visible_time = rand_range(self.personality.get("camouflage_touch_visible_time"), 4.5, 8.0)
        try:
            visible_time = max(0.4, float(visible_time))
        except Exception:
            visible_time = 5.5
        self._camouflage_strength = 0.0
        self._camouflage_color = None
        self._camouflage_idle_timer = 0.0
        self._camouflage_visible_timer = max(float(getattr(self, "_camouflage_visible_timer", 0.0)), visible_time)

    def _panic_rehome_legs(self, intensity: float = 1.0) -> None:
        intensity = clamp(intensity, 0.0, 1.0)
        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step:
                continue
            ix, iy = self._leg_ideal_foot(leg)
            dist = math.hypot(leg.foot_x - ix, leg.foot_y - iy)
            candidates.append((dist + random.random() * self.size * 0.12, i, leg, ix, iy))
        candidates.sort(reverse=True, key=lambda item: item[0])
        # Start a few quick, staggered corrective steps instead of snapping every foot.
        for rank, (_, _, leg, ix, iy) in enumerate(candidates[: max(2, int(2 + intensity * 3))]):
            delay = rank * random.uniform(0.012, 0.035)
            side = self._side_sign(leg.definition.get("side", "right"))
            ix += random.uniform(-0.10, 0.10) * self.size
            iy += side * random.uniform(0.02, 0.15) * self.size * intensity
            self._schedule_step(leg, ix, iy, delay=delay, force_fast=True)
            leg.step_cooldown = random.uniform(0.012, 0.040)


    def set_size_scale(self, size_scale: float) -> None:
        """Scale the creature and rehome its legs for clean live size changes."""
        new_scale = clamp(float(size_scale), 0.45, 2.25)
        if abs(new_scale - self.size_scale) < 1e-4:
            return
        self.size_scale = new_scale
        self.size = float(self.model.get("base_size", 25)) * self.size_jitter * self.size_scale
        self.x, self.y = clamp_point(self.x, self.y, self.margin * 0.4, self.screen_w, self.screen_h)
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin * 0.4, self.screen_w, self.screen_h)
        self._initialize_legs()
        self.startled_timer = max(self.startled_timer, 0.45)

    def start_drag(self, mx: float, my: float) -> None:
        self.dragging = True
        self.state = "Dragged"
        self.motion_paused = True
        self.speed = 0.0
        self.current_speed = 0.0
        self.inertia_vx = 0.0
        self.inertia_vy = 0.0
        self.inertia_timer = 0.0
        self.startled_timer = 1.2
        self.grab_offset_x = self.x - mx
        self.grab_offset_y = self.y - my
        self.drag_vel_x = 0.0
        self.drag_vel_y = 0.0
        self.state_timer = 0.25
        self.register_camouflage_touch()
        self._panic_rehome_legs(0.75)

    def drag_to(self, dt: float, mx: float, my: float) -> None:
        dt = clamp(dt, 0.001, 0.05)
        if not self.dragging:
            return
        target_x = mx + self.grab_offset_x
        target_y = my + self.grab_offset_y
        target_x, target_y = clamp_point(target_x, target_y, self.margin * 0.25, self.screen_w, self.screen_h)

        dx = target_x - self.x
        dy = target_y - self.y
        raw_vx = dx / dt
        raw_vy = dy / dt
        # Smooth the measured throw velocity so release inertia follows the hand
        # motion, not a single noisy frame.
        self.drag_vel_x = self.drag_vel_x * 0.58 + raw_vx * 0.42
        self.drag_vel_y = self.drag_vel_y * 0.58 + raw_vy * 0.42

        self.x = target_x
        self.y = target_y
        self.target_x = target_x
        self.target_y = target_y
        drag_speed = math.hypot(self.drag_vel_x, self.drag_vel_y)
        if drag_speed > 18.0:
            self.target_heading = math.atan2(self.drag_vel_y, self.drag_vel_x)
            if self._is_drifter_personality():
                self._prime_drift(clamp(drag_speed / 700.0, 0.25, 1.15))
                self.target_heading += self.drift_dir * clamp(drag_speed / 900.0, 0.0, 0.56)
        self.heading = angle_lerp(self.heading, self.target_heading, self.turn_rate * 2.2 * dt)
        # If it has locked onto a fly while being carried, it turns to face the
        # prey (and, if it can, will web it from the hand via the hunt driver).
        prey = getattr(self, "_prey", None)
        if prey is not None and getattr(prey, "alive", False) and not getattr(prey, "eaten", False):
            self.target_heading = angle_to(self.x, self.y, prey.x, prey.y)
            self.heading = angle_lerp(self.heading, self.target_heading, self.turn_rate * 2.6 * dt)
            self._set_focus(prey.x, prey.y, 1.0)
        self.vel_x = self.drag_vel_x
        self.vel_y = self.drag_vel_y
        self.current_speed = clamp(drag_speed, 0.0, 260.0)
        self.startled_timer = 1.2
        self.register_camouflage_touch(visible_time=5.0)

        # Legs are partly carried by the body and partly dragged behind, making the
        # spider look startled without breaking the anatomical constraints.
        carry = 0.68 if drag_speed < 520.0 else 0.56
        self._translate_leg_world_points(dx, dy, carry)
        # Do not over-clamp planted rear feet here; that made them look frozen.
        # Instead, let the scheduler see the overreach and lift them into quick
        # corrective steps while the renderer hides any temporary rubber-band length.
        if drag_speed > 45.0 and random.random() < 0.82:
            self._panic_rehome_legs(clamp(drag_speed / 800.0, 0.35, 1.0))

    def release_drag(self, mx: float, my: float) -> None:
        if not self.dragging:
            return
        self.dragging = False
        self.motion_paused = False
        speed = math.hypot(self.drag_vel_x, self.drag_vel_y)
        if speed > 1.0:
            max_throw = 980.0
            scale = min(max_throw, speed) / speed
            self.inertia_vx = self.drag_vel_x * scale * 0.78
            self.inertia_vy = self.drag_vel_y * scale * 0.78
            self.inertia_timer = clamp(speed / 760.0, 0.25, 1.25)
            self.target_heading = math.atan2(self.inertia_vy, self.inertia_vx)
        else:
            self.inertia_vx = 0.0
            self.inertia_vy = 0.0
            self.inertia_timer = 0.0
        self.register_camouflage_touch()
        self.enter_startled(mx, my)
        for leg in self.legs:
            self._soft_limit_leg_state(leg, blend=0.45)
        self._panic_rehome_legs(clamp(speed / 850.0, 0.55, 1.0))

    # ------------------------------------------------------------------
    # Update
    # ------------------------------------------------------------------
    def resize_screen(self, w: int, h: int) -> None:
        self.screen_w = max(200, int(w))
        self.screen_h = max(200, int(h))
        self.x, self.y = clamp_point(self.x, self.y, self.margin, self.screen_w, self.screen_h)
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)

    def set_neighbors(self, neighbors: List["Creature"]) -> None:
        """Provided by the manager each frame so creatures can play together."""
        self.neighbors = neighbors

    def set_mood_mode(self, mode: str) -> None:
        """Switch the emotional baseline at runtime (auto/playful/cuddly/curious/calm/...).

        ``auto`` restores the baseline implied by the personality.  Any other value
        names a baseline directly.  The live mood eases toward the new baseline.
        """
        mode = str(mode or "auto").lower()
        self.mood_mode = mode
        baseline = mode if mode != "auto" else self.personality.get("id", "auto")
        self.mood.set_baseline_named(baseline)

    def set_allow_social(self, enabled: bool) -> None:
        """Enable or disable seeking out other spiders to play with."""
        self.allow_social = bool(enabled)
        if not self.allow_social and self.state in ("Play",):
            self.social_target = None
            self.enter_idle()

    def set_gait_style(self, style: str) -> None:
        """Switch the movement/leg-animation style at runtime.

        ``classic`` is the original reactive gait; ``lively`` lifts the legs
        visibly while stepping, replants the feet through turns, and probes
        nearby objects with the front legs and pedipalps. ``skitter`` keeps
        the lively leg logic but makes it faster and broken into short bursts
        with momentary stillness.
        """
        self.gait_style = normalize_gait_style(style)

    def update(self, dt: float, mx: float, my: float, sw: int, sh: int) -> None:
        dt = clamp(dt, 0.001, 0.05)
        self.resize_screen(sw, sh)

        if self.prev_mx is None:
            cursor_vx = 0.0
            cursor_vy = 0.0
        else:
            cursor_vx = (mx - self.prev_mx) / dt
            cursor_vy = (my - self.prev_my) / dt
        cursor_vx = cursor_vx * 0.7 + self.prev_cursor_vx * 0.3
        cursor_vy = cursor_vy * 0.7 + self.prev_cursor_vy * 0.3
        self.prev_mx = mx
        self.prev_my = my
        self.prev_cursor_vx = cursor_vx
        self.prev_cursor_vy = cursor_vy
        self.nope_cooldown = max(0.0, self.nope_cooldown - dt)
        self.weave_cooldown = max(0.0, self.weave_cooldown - dt)
        self.web_walk_cooldown = max(0.0, self.web_walk_cooldown - dt)
        self.web_shot_cooldown = max(0.0, self.web_shot_cooldown - dt)
        self._feed_cooldown = max(0.0, getattr(self, "_feed_cooldown", 0.0) - dt)
        self._pounce_cooldown = max(0.0, getattr(self, "_pounce_cooldown", 0.0) - dt)
        self._trap_shot_cooldown = max(0.0, getattr(self, "_trap_shot_cooldown", 0.0) - dt)

        # Reconcile web behaviour with the live state.  If anything pulled this
        # spider out of a weave/web-walk state -- a threat reflex, a grab, a
        # disabled skill, a manager reset -- release the web so an unfinished one
        # becomes adoptable and another spider can finish it.
        if self.weaving_web is not None and self.state not in ("WeaveApproach", "Weave"):
            self._abandon_weaving()
        if self.repairing_web is not None and self.state not in ("RepairApproach", "Repair"):
            self._abandon_repair()
        if self.web_target is not None and self.state not in ("WebApproach", "WebWalk"):
            self.web_target = None

        # Hidden spiders are behind a real desktop window, so they are no longer
        # in the same visible interaction layer as the cursor or other spiders.
        if getattr(self, "_desktop_fully_hidden", False):
            self.social_target = None
        elif self.social_target is not None and getattr(self.social_target, "_desktop_fully_hidden", False):
            self.social_target = None

        # Default focus is the cursor; behaviours tracking a playmate override
        # focus_x/focus_y themselves and set social_target.
        if self.social_target is None:
            self._set_focus(mx, my, max(self.focus_strength, 0.25))

        if self.dragging:
            self.startled_timer = 1.2
            self._set_focus(mx, my, 1.0)
            speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0)
            self.bob_phase += dt * (7.5 + self.current_speed * 0.035)
            self.breath_phase += dt * (3.4 + speed01 * 1.3)
            self.body_bob = math.sin(self.bob_phase * 1.35) * (0.75 + speed01 * 1.45)
            drag_drift = self._drift_amount(dt, self.current_speed) if self._is_drifter_personality() else 0.0
            self.body_sway = (
                math.sin(self.bob_phase * 1.8) * self.size * (0.025 + speed01 * 0.035)
                + drag_drift * self.size * 0.42
            )
            self.abdomen_pulse = math.sin(self.breath_phase * 1.6) * 0.065
            self.ceph_pulse = -math.sin(self.breath_phase * 1.45 + 0.85) * 0.040
            self.mood.bump(arousal=dt * 0.7, valence=-dt * 0.25)
            # A spider being carried can still aim and fire trapping silk at a
            # fly it has locked onto, so advance an in-progress web shot here
            # (the normal FSM below is skipped while dragging).
            if self.state in ("WebAim", "WebShot"):
                self.state_timer -= dt
                if self.state == "WebAim":
                    self._update_web_aim(dt, mx, my)
                else:
                    self._update_web_shot(dt, mx, my)
            self._update_legs(dt)
            self._update_mood(dt)
            self._update_posture(dt)
            self._update_antennae(dt)
            return

        self.startled_timer = max(0.0, self.startled_timer - dt)

        # Threat reflex, softened by mood: a content/cuddly/playful spider tolerates
        # the cursor far more than a skittish one, and committed social/jump states
        # ignore everything but a very sudden jab.
        bravery = clamp(0.5 + self.mood.happy * 0.4 + self.mood.affection * 0.5
                        + max(0.0, self.mood.valence) * 0.2 - self.mood.sad * 0.5, 0.0, 1.6)
        calm_states = ("Cuddle", "Play", "Inspect", "Aim", "Coil", "Catch", "Land", "Roll", "Feed",
                       "Weave", "WeaveApproach", "WebWalk", "WebApproach", "WebAim", "WebShot",
                       "RepairApproach", "Repair")
        p = self.personality
        base_radius = float(p.get("threat_radius", 180))
        base_speed = float(p.get("threat_cursor_speed", 900))
        eff_radius = base_radius * clamp(1.0 - bravery * 0.32, 0.45, 1.0)
        eff_speed = base_speed * clamp(1.0 + bravery * 0.55, 1.0, 2.2)
        if self.state in calm_states:
            eff_speed *= 1.9
            eff_radius *= 0.6
        if self._cursor_triggers_nope_escape(mx, my, cursor_vx, cursor_vy) and not self._hunting_prey:
            self.social_target = None
            self.enter_nope_escape(mx, my)

        if (self.has_skill("run_away") and not self._hunting_prey and not self.airborne
                and self.state not in ("Retreat", "Startled", "Jump", "Land")
                and cursor_is_threatening(self.x, self.y, mx, my, cursor_vx, cursor_vy, eff_radius, eff_speed)):
            self.social_target = None
            self.mood.bump(arousal=0.25, valence=-0.1)
            self.enter_retreat(mx, my)

        if self.airborne:
            self._update_jump(dt)
        else:
            self._update_state(dt, mx, my)
            if self._spider_grounded_mode_allowed():
                self._update_spider_grounded_frame(dt)
            else:
                if self._spider_locomotion_active:
                    self._spider_locomotion_active = False
                self._move_body(dt)
        if self.cage is not None and not self.dragging:
            self._apply_cage_bounds()
        self._update_legs(dt)
        self._update_mood(dt)
        self._update_posture(dt)
        self._update_antennae(dt)

    def _update_state(self, dt: float, mx: float, my: float) -> None:
        self.state_timer -= dt
        self.decision_timer -= dt
        dist_to_cursor = distance(self.x, self.y, mx, my)
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        reaction = float(self.personality.get("reaction_radius", 360))
        hunter = self._is_hunter_personality()
        jumper = self._is_jumper_personality()
        observer = self._is_observer_personality()
        cursor_still = hunter and self._cursor_is_still_for_observe()
        hunt_catch_distance = self.size * float(self.personality.get("hunt_catch_distance_mult", 2.25))
        if self._is_drifter_personality() and self.state != "DriftRun":
            self.drift_run_cooldown = max(0.0, float(getattr(self, "drift_run_cooldown", 0.0)) - dt)

        if self.state == "Idle":
            self.speed = 0.0
            self.target_heading = angle_to(self.x, self.y, mx, my) if dist_to_cursor < reaction else self.target_heading
            if hunter and dist_to_cursor < reaction and self.decision_timer <= 0.0:
                self.decision_timer = random.uniform(0.08, 0.18)
                if cursor_still:
                    self.enter_alert(mx, my)
                else:
                    self.enter_approach(mx, my)
                return
            if jumper and self.state_timer <= 0.0 and self.decision_timer <= 0.0:
                # A jumper rarely slides straight out of idle; it starts roaming with a small hop.
                if random.random() < float(self.personality.get("idle_hop_chance", 0.55)):
                    tx = self.x + math.cos(self.heading + random.uniform(-0.65, 0.65)) * self.size * random.uniform(1.0, 2.0)
                    ty = self.y + math.sin(self.heading + random.uniform(-0.65, 0.65)) * self.size * random.uniform(1.0, 2.0)
                    self.enter_coil(after="wander", power=rand_range(self.personality.get("hop_power"), 0.34, 0.58), toward=(tx, ty))
                    return
            if observer and self.decision_timer <= 0.0:
                anchor = self._observer_anchor(mx, my)
                if anchor is not None and (dist_to_cursor < reaction * 1.18 or anchor[2] is not None):
                    self.decision_timer = random.uniform(0.16, 0.32)
                    ax, ay, target = anchor
                    self.enter_observe(ax, ay, target)
                    return
            if self.state_timer <= 0.0 and self.decision_timer <= 0.0:
                self.decision_timer = random.uniform(0.2, 0.5)
                if self._consider_special_actions(dist_to_cursor, mx, my, from_idle=True):
                    return
                if dist_to_cursor < reaction:
                    self.enter_alert(mx, my)
                elif random.random() < float(self.personality.get("wander_frequency", 0.35)):
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
                        self.state_timer = rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42)
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
                if self._consider_special_actions(dist_to_cursor, mx, my, from_idle=False):
                    return
                if self.has_skill("run_away") and dist_to_cursor < float(self.personality.get("threat_radius", 180)) * 0.55 and random.random() > boldness:
                    self.enter_retreat(mx, my)
                elif self.has_skill("chase") and dist_to_cursor < 95.0 and random.random() < 0.45 + boldness * 0.45:
                    self.enter_chase(mx, my)
                elif dist_to_cursor < reaction and random.random() < 0.25 + boldness * 0.65:
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
                        self.state_timer = rand_range(self.personality.get("observe_pause_time"), 0.18, 0.42)
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
                    self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.08, 0.20)
                return
            if self.has_skill("chase") and dist_to_cursor < 78.0 and random.random() < 0.025 + boldness * 0.035:
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
                self.speed = 52.0 * float(self.personality.get("speed_multiplier", 1.0))
                if self._maybe_jumper_hop(dt, "approach", (mx, my)):
                    return
            if self.state_timer <= 0.0:
                if self.motion_paused:
                    self.motion_paused = False
                    self.target_x = mx
                    self.target_y = my
                    self.speed = 52.0 * float(self.personality.get("speed_multiplier", 1.0))
                    self.state_timer = rand_range(self.personality.get("approach_move_time"), 0.5, 1.3)
                else:
                    self.motion_paused = True
                    self.speed = 0.0
                    self.state_timer = rand_range(self.personality.get("approach_pause_time"), 0.25, 0.7)

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
                    spin = angle_to(mx, my, self.x, self.y) + random.choice((-1.0, 1.0)) * random.uniform(1.35, 2.75)
                    orbit = random.uniform(self.size * 1.2, self.size * 2.8)
                    self.target_x = mx + math.cos(spin) * orbit
                    self.target_y = my + math.sin(spin) * orbit
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.speed = base_speed * 1.18
                    self.chase_timer = max(self.chase_timer, float(self.personality.get("chase_persistence", 2.5)))
                    self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12)
                    self.decision_timer = random.uniform(0.10, 0.22)
                    return
                if self.state_timer <= 0.0:
                    lead = clamp(self._cursor_speed() / 80.0, 0.0, 18.0)
                    self.target_x = mx + self.prev_cursor_vx * 0.035 + random.uniform(-10.0 - lead, 10.0 + lead)
                    self.target_y = my + self.prev_cursor_vy * 0.035 + random.uniform(-10.0 - lead, 10.0 + lead)
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.state_timer = rand_range(self.personality.get("chase_retarget_time"), 0.05, 0.12)
                self.speed = base_speed
                if self.chase_timer <= 0.0 or dist_to_cursor > reaction * 1.75:
                    self.enter_alert(mx, my) if dist_to_cursor < reaction else self.enter_idle()
                return
            if self.state_timer <= 0.0:
                self.target_x = mx + random.uniform(-18.0, 18.0)
                self.target_y = my + random.uniform(-18.0, 18.0)
                self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                self.state_timer = random.uniform(0.10, 0.24)
            self.speed = 112.0 * float(self.personality.get("speed_multiplier", 1.0))
            if self._maybe_jumper_hop(dt, "chase", (mx, my)):
                return
            if self._hunting_prey:
                # Stay locked on the fly; the manager keeps the focus on it.
                self.target_x, self.target_y = mx, my
                self.chase_timer = max(self.chase_timer, 0.6)
            elif self.chase_timer <= 0.0 or dist_to_cursor > reaction * 1.45:
                self.enter_alert(mx, my) if dist_to_cursor < reaction else self.enter_idle()

        elif self.state == "Retreat":
            self.speed = 148.0 * float(self.personality.get("speed_multiplier", 1.0))
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
            else:
                self.speed = 74.0 * float(self.personality.get("speed_multiplier", 1.0))
                if dist_to_cursor < reaction * 0.85 and self.decision_timer <= 0.0:
                    away = math.atan2(self.y - my, self.x - mx)
                    self.target_x = self.x + math.cos(away) * random.uniform(95.0, 180.0)
                    self.target_y = self.y + math.sin(away) * random.uniform(95.0, 180.0)
                    self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
                    self.target_heading = away
                    self.decision_timer = random.uniform(0.25, 0.45)
            if self.state_timer <= 0.0 and self.inertia_timer <= 0.0:
                if self.has_skill("run_away") and dist_to_cursor < reaction * 0.65:
                    self.enter_retreat(mx, my)
                else:
                    self.enter_idle()

        elif self.state == "Wander":
            self.speed = 0.0 if self.motion_paused else 38.0 * float(self.personality.get("speed_multiplier", 1.0))
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
                self.decision_timer = random.uniform(0.35, 0.8)
                if self._should_start_drift_run(from_idle=False):
                    self.enter_drift_run()
                    return
                if random.random() < 0.25:
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
                        self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28)
                        self.enter_retreat(mx, my)
                    else:
                        self.nope_cooldown = rand_range(self.personality.get("nope_cooldown"), 0.12, 0.28)
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
            self._feed_paw = getattr(self, "_feed_paw", 0.0) + dt
            self._feed_frenzy = getattr(self, "_feed_frenzy", 0.0) - dt
            anchor = getattr(self, "_feed_anchor", (self.x, self.y))
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
                if self.zoom_repeats > 0 and random.random() < 0.7:
                    self.zoom_repeats -= 1
                    if random.random() < 0.3:
                        self.enter_spring(after="idle", power=random.uniform(0.7, 1.1))
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

    # ------------------------------------------------------------------
    # Decision helper: choose an expressive action by mood / situation
    # ------------------------------------------------------------------
    def _consider_special_actions(self, dist_to_cursor: float, mx: float, my: float, from_idle: bool) -> bool:
        self.social_cooldown = max(0.0, self.social_cooldown - 0.0)  # decremented in mood update
        m = self.mood
        reaction = float(self.personality.get("reaction_radius", 360))
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)

        # --- Drifter self-directed flourish ---
        if self._should_start_drift_run(from_idle=from_idle):
            self.enter_drift_run()
            return True

        webber = self._is_webber_personality()

        # --- Web weaving / repairing / finishing (webbers do this constantly) ---
        if (self.has_skill("weave_web") and self.cage is None
                and self.web_world is not None and self.weave_cooldown <= 0.0):
            # First, prefer mending a torn finished web -- a webber dislikes a
            # broken net and will go fix it.  Webbers travel anywhere for it;
            # other spiders only mend one that is reasonably close.
            repairable = self.web_world.find_repairable_web(
                self, max_dist=1e9 if webber else reaction * 1.5)
            if repairable is not None and random.random() < (0.9 if webber else 0.25):
                if self._begin_repair(repairable):
                    return True
            # Next, prefer finishing an abandoned, unfinished web -- even one
            # another spider began.  Webbers will travel anywhere to finish it;
            # other spiders only adopt one that is reasonably close.
            adoptable = self.web_world.find_adoptable_web(
                self, max_dist=1e9 if webber else reaction * 1.6)
            if adoptable is not None and random.random() < (0.85 if webber else 0.22):
                if self._begin_adopt(adoptable):
                    return True
            # Otherwise start a brand-new web.  Each finished, intact web already
            # on screen reduces the urge to build another, so a webber stops once
            # it has spun a few; a torn web does not count, so damage keeps the
            # webber motivated (to repair, or to replace) until it is whole again.
            intact = self.web_world.intact_complete_count()
            satiation = clamp(1.0 - intact * float(self.personality.get("web_satiation_per_web", 0.22)),
                              0.12, 1.0)
            start_chance = ((0.62 + m.curiosity * 0.28) if webber else 0.03) * satiation
            if random.random() < start_chance:
                if self._begin_weave():
                    return True

        # --- Walking onto a finished web to bounce-test it (any spider) ---
        if (self.has_skill("web_walk") and self.cage is None
                and self.web_world is not None and self.web_walk_cooldown <= 0.0):
            walkable = self.web_world.find_walkable_web(self, max_dist=reaction * 1.8)
            if walkable is not None:
                walk_chance = 0.45 if webber else 0.16 + m.curiosity * 0.2
                if random.random() < walk_chance:
                    if self._begin_web_walk(walkable):
                        return True

        # --- Shooting sticky silk at the pointer (trap it / shove it to a wall) ---
        # The hunting states drive this for stalkers; this covers every other
        # spider that has the skill, so a non-hunter web-shooter still fires.
        if dist_to_cursor < reaction:
            if self._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my):
                return True

        # --- Social play with another creature ---
        if self.allow_social and self.social_cooldown <= 0.0:
            social_range = max(reaction * 0.85, self.size * 12.0)
            mate = self._find_social_target(social_range)
            if mate is not None:
                d = distance(self.x, self.y, mate.x, mate.y)
                # Pick an interaction by current mood.
                play_w = 0.25 + m.arousal * 0.6 + max(0.0, m.valence) * 0.4
                inspect_w = 0.25 + m.curiosity * 0.8
                cuddle_w = 0.1 + m.affection * 0.8
                chance = clamp(0.35 + m.arousal * 0.4 + m.curiosity * 0.3, 0.2, 0.92)
                if random.random() < chance:
                    self.social_cooldown = random.uniform(4.0, 8.0)
                    roll = random.random() * (play_w + inspect_w + cuddle_w)
                    if roll < play_w:
                        if d > self.size * 3.0 and m.arousal > 0.45 and random.random() < 0.5:
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
            if mid and (boldness > 0.6 or m.arousal > 0.6) and random.random() < 0.10 + boldness * 0.30 + m.arousal * 0.2:
                self.enter_aim(mx, my, target=None, after="outcome",
                               ranging=(0.55, 1.2), abort_chance=0.22 - boldness * 0.15)
                return True
            # Curious lean-in inspection.
            if mid and m.curiosity > 0.55 and random.random() < 0.18 + m.curiosity * 0.4:
                self.enter_inspect(mx, my, target=None)
                return True
            # Affectionate cuddle when close.
            if near and m.affection > 0.55 and random.random() < 0.2 + m.affection * 0.5:
                self.enter_cuddle(mx, my, target=None)
                return True
            # Playful little pounce when close.
            if near and m.valence > 0.4 and m.arousal > 0.5 and random.random() < 0.25:
                self.enter_aim(mx, my, target=None, after="outcome",
                               ranging=(0.35, 0.7), abort_chance=0.1)
                return True
            # Happy little tumble away from the cursor.
            if near and m.valence > 0.5 and m.arousal > 0.55 and random.random() < 0.12:
                self.enter_roll(direction=angle_to(self.x, self.y, mx, my) + math.pi)
                return True

        # --- Self-directed play when no obvious target (playful temperament) ---
        if from_idle and m.valence > 0.4 and m.arousal > 0.55:
            r = random.random()
            if r < 0.12:
                self.enter_roll()
                return True
            if r < 0.26:
                self.enter_spring(after="idle", power=random.uniform(0.7, 1.2))
                return True
            if r < 0.34:
                self.enter_zoomies()
                return True
        return False

    def _update_roll(self, dt: float, mx: float, my: float) -> None:
        # Spin through the planned turns with an ease-out so it whirls fast then
        # settles. The body stays put under the spin; we only nudge the centre
        # along the drift direction and carry the (tucked) feet with it.
        self.motion_paused = True
        self.speed = 0.0
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
            self.roll_spin = 0.0
            self.roll_tuck = 0.0
            self.motion_paused = False
            self._panic_rehome_legs(0.5)
            self.enter_idle()

    # ------------------------------------------------------------------
    # Web weaving and walking
    # ------------------------------------------------------------------
    def _begin_weave(self) -> bool:
        """Claim a fresh site and start walking to it to build.

        Corners are still favoured, but a webber will often pick an open spot out
        in the room so its webs end up scattered around rather than only hugging
        the screen edges.
        """
        if self.web_world is None or self.cage is not None:
            return False
        prefer_corner = random.random() < float(self.personality.get("web_corner_bias", 0.55))
        web = self.web_world.claim_site(self, prefer_corner=prefer_corner)
        if web is None:
            # Every site is taken; back off briefly before trying again.
            self.weave_cooldown = rand_range(self.personality.get("weave_retry_cooldown"), 5.0, 10.0)
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
                                  rand_range(self.personality.get("weave_retry_cooldown"), 3.5, 7.5))

    def _finish_weave(self) -> None:
        self.weaving_web = None
        self._weave_drawing = False
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0)
        self.weave_cooldown = base / (4.0 if self._is_webber_personality() else 1.0)
        self.mood.bump(arousal=-0.05, valence=0.18, affection=0.04)
        self.enter_idle()

    # ------------------------------------------------------------------
    # Web repair: a webber notices a torn net and goes to mend it
    # ------------------------------------------------------------------
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
                                  rand_range(self.personality.get("weave_retry_cooldown"), 2.5, 5.0))

    def _finish_repair(self) -> None:
        if self.repairing_web is not None and self.web_world is not None:
            self.web_world.release_repair(self.repairing_web)
        self.repairing_web = None
        # Mending is satisfying but quick to come off cooldown for a webber, so it
        # stays attentive to further damage.
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0)
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
            self.weave_cooldown = rand_range(self.personality.get("weave_cooldown"), 6.0, 14.0)
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
        px, py = getattr(self, "_web_walk_point", (web.hub[0], web.hub[1]))
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
        self._web_pluck_count = random.randint(2, 4)
        self._web_pluck_timer = rand_range(None, 0.25, 0.5)
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
                web.pluck((self.x, self.y), strength=random.uniform(0.6, 1.3))
                self._web_pluck_count -= 1
                self._web_pluck_timer = rand_range(None, 0.5, 0.95)
                self.mood.bump(arousal=0.03, valence=0.04, curiosity=0.03)
            else:
                self._leave_web_walk()
                return
        if self.state_timer <= 0.0:
            self._leave_web_walk()

    def _leave_web_walk(self) -> None:
        self.web_target = None
        self.web_walk_cooldown = rand_range(self.personality.get("web_walk_cooldown"), 8.0, 20.0)
        self.motion_paused = False
        self.enter_idle()

    # ------------------------------------------------------------------
    # Shooting sticky silk at the real pointer (trap it / shove it to a wall)
    # ------------------------------------------------------------------
    def _can_shoot_web(self, kind: str, prey=None) -> bool:
        skill = "wall_web" if kind == "wall" else "shoot_web"
        if not self.has_skill(skill) or self.airborne:
            return False
        if prey is not None:
            # Firing at a fly goes through the fly world and never touches the
            # real pointer, so neither the cursor-capture toggle nor being held
            # by the mouse gates it: a spider you are carrying can still web a fly.
            return self.fly_world is not None
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
        if random.random() >= chance:
            return False
        # Pick the shot. The wall shove is a flashier finisher used a little less
        # often when the spider can also trap in place.
        kind = "trap"
        if can_wall and (not can_trap or
                         random.random() < float(self.personality.get("wall_web_bias", 0.3))):
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
        if random.random() >= chance:
            return False
        kind = "trap"
        if can_wall and (not can_trap or
                         random.random() < float(self.personality.get("wall_web_bias", 0.3))):
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
        self.state_timer = rand_range(self.personality.get("web_aim_time"), 0.3, 0.58)
        self.range_clock = 0.0
        self.range_mode = "waggle"
        self.range_switch = random.uniform(0.14, 0.28)
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
            self.range_switch = random.uniform(0.14, 0.28)
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
                launched = world.shoot(origin, (mx, my), kind=kind)
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
        self.state_timer = rand_range(self.personality.get("web_recoil_time"), 0.18, 0.3)
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
            base = rand_range(self.personality.get("web_shot_cooldown"), 8.0, 18.0)
        else:
            # Did not actually fire (blocked/cancelled): retry sooner.
            base = rand_range(self.personality.get("web_shot_retry_cooldown"), 2.0, 4.5)
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

        if self.decision_timer <= 0.0 and random.random() < 0.18:
            self.decision_timer = random.uniform(0.45, 0.9)
            self.observe_radius = clamp(desired + random.uniform(-self.size * 0.8, self.size * 0.8), lo, hi)

        if self.state_timer <= 0.0:
            linger = clamp(float(self.personality.get("observe_linger_chance", 0.78)), 0.0, 1.0)
            next_anchor = self._observer_anchor(mx, my)
            if next_anchor is not None and random.random() < linger:
                ax, ay, next_target = next_anchor
                self.enter_observe(ax, ay, next_target)
                if random.random() < 0.45:
                    self.observe_dir *= -1.0
                return
            self.social_target = None
            if current < reaction * 0.8 and random.random() < 0.35:
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
            if self.inspect_clock > random.uniform(0.7, 1.3):
                self.inspect_clock = 0.0
                roll = random.random()
                if roll < 0.4:
                    self.inspect_phase = "orbit"
                    self.orbit_clock = 0.0
                    self.orbit_dir = random.choice((-1.0, 1.0))
                elif roll < 0.62:
                    self.head_tilt = random.uniform(-0.5, 0.5)
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
            if self.orbit_clock > random.uniform(0.8, 1.6):
                self.inspect_phase = "study"
                self.inspect_clock = 0.0

        if self.state_timer <= 0.0:
            # Inspection concludes: escalate by mood, or lose interest.
            m = self.mood
            roll = random.random()
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
                self.boop_timer = random.uniform(0.5, 1.0)
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
            self.range_switch = random.uniform(0.16, 0.32)
            self.range_mode = "nod" if self.range_mode == "waggle" else "waggle"
            if self.range_mode == "waggle":
                self.wiggle_burst = max(self.wiggle_burst, 0.7)
            else:
                self.rear = min(1.0, self.rear + 0.5)

        if self.state_timer <= 0.0:
            if random.random() < self.aim_abort_chance:
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
            if got_it and random.random() < 0.6:
                self.mood.bump(valence=0.28, affection=0.18, arousal=0.1)
                if target is not None:
                    self.enter_play(target, role="chase")
                else:
                    self.enter_cuddle(cx, cy, None)
            else:
                # Missed/escaped: a small frustrated shake, then maybe re-pounce.
                self.mood.bump(valence=-0.1, arousal=0.18)
                self.wiggle_burst = 0.8
                if random.random() < 0.4 and self.mood.arousal > 0.5:
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
            self.target_x = target.x + random.uniform(-12.0, 12.0)
            self.target_y = target.y + random.uniform(-12.0, 12.0)
            self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin, self.screen_w, self.screen_h)
            self.speed = 92.0 * self._speed_mult()
            self.target_heading = angle_to(self.x, self.y, target.x, target.y)
            if d <= contact:
                # Tag! A happy bounce, then resolve the bout.
                self.mood.bump(valence=0.22, arousal=0.18)
                target.mood.bump(valence=0.2, arousal=0.2)
                roll = random.random()
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
                self.play_lookback = random.uniform(0.6, 1.2)
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
            self._skitter_burst_timer = min(self._skitter_burst_timer, random.uniform(0.12, 0.24))
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
            self._skitter_pause_timer = random.uniform(0.055, 0.135) if fast_state else random.uniform(0.085, 0.22)
            self._skitter_burst_timer = random.uniform(0.10, 0.24) if fast_state else random.uniform(0.16, 0.36)
            self._skitter_burst_jitter = random.uniform(0.94, 1.20)
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
        max_turn = self.turn_rate * turn_mult * dt
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

    # ------------------------------------------------------------------
    # Ballistic hop (faked vertical for a top-down view)
    # ------------------------------------------------------------------
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

    # ------------------------------------------------------------------
    # Emotion and body-language integration
    # ------------------------------------------------------------------
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
            self.blink_timer = random.uniform(2.0, 6.0)

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
        for i in range(2):
            self.antenna_phase[i] += dt * (1.8 + m.arousal * 3.2 + m.curiosity * 1.6 + speed01 * 2.0)

    def _antenna_aim_angle(self) -> float:
        local_f, local_s = self._world_to_body_local(self.focus_x, self.focus_y)
        return math.atan2(local_s, local_f)

    # ------------------------------------------------------------------
    # Elegant planted-foot stepping
    # ------------------------------------------------------------------
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
                float(getattr(self, "_spider_turn_speed", 0.0)),
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
            target_f += random.uniform(-0.060, 0.095) * self.size * (0.7 + speed01)
            target_s += side * random.uniform(-0.040, 0.055) * self.size * (0.6 + speed01 * 0.5)
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
        arc = clamp(path_len * random.uniform(0.22, 0.36), self.size * 0.12, self.size * 0.56)
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
                        leg.step_cooldown = random.uniform(0.012, 0.045) * (1.08 - speed01 * 0.36)
                    else:
                        leg.step_cooldown = random.uniform(0.035, 0.095) * (1.15 - speed01 * 0.45)

    def _update_legs(self, dt: float) -> None:
        """Update spider legs using leg-level coordination instead of locked groups.

        Real spiders often approximate an alternating tetrapod gait, but individual
        legs are not mechanically welded into two perfect four-leg teams.  Opposite
        and neighboring legs tend toward antiphase while each leg still has its own
        phase drift, urgency threshold, and footfall target.
        """
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
            leg.step_cooldown = random.uniform(0.020, 0.055)
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
            if urgency <= 1.0 and not (not moving and random.random() < 0.012):
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
            urgency += random.uniform(-0.10, 0.18)
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
            delay = random.uniform(0.030, 0.120) if sliding else random.uniform(0.000, 0.040 if fast_footwork else 0.070)
            self._schedule_step(leg, ix, iy, delay=delay, force_fast=fast_footwork)
            leg.step_cooldown = random.uniform(0.070, 0.160) if sliding else random.uniform(0.025, 0.070)
            used_indices.add(i)
            scheduled_count += 1
            started = True

        self.idle_twitch_timer -= dt
        if not started and self.current_speed < 2.0 and self.idle_twitch_timer <= 0.0:
            idle_candidates = [leg for leg in self.legs if not leg.stepping and not leg.pending_step]
            if idle_candidates:
                leg = random.choice(idle_candidates)
                ix, iy = self._leg_ideal_foot(leg)
                # Tiny independent toe probe, not a whole-group twitch.
                self._schedule_step(leg, ix + random.uniform(-3.2, 3.2), iy + random.uniform(-3.2, 3.2), force_fast=False)
                started = True
            self.idle_twitch_timer = random.uniform(0.75, 2.0)

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
            hz += min(turn_speed, 4.5) * 0.10
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
        speed01 = clamp(self.current_speed / 150.0, 0.0, 1.0)
        fast_state = self.state in ("Chase", "Retreat", "Dragged", "Startled", "DriftRun")
        max_air = config["max_airborne"]
        swinging_now = sum(1 for leg in self.legs if leg.stepping or leg.pending_step)

        # Advance the same clock that defines the swing duration before choosing
        # launches, so this frame's candidates are evaluated in the current window.
        self._lively_gait_phase = (
            self._lively_gait_phase + dt * self._spider_cycle_hz(config, speed01, turn_speed)
        ) % 1.0

        # Only severe wrong-side/overreach poses bypass the phase window.  Normal
        # foot-placement error is scheduled in its tetrapod's window.
        candidates = []
        for i, leg in enumerate(self.legs):
            if leg.stepping or leg.pending_step or leg.step_cooldown > 0.0:
                continue
            _, _, _, _, _, severe_wrong = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, _, very_far = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            emergency = severe_wrong or very_far
            target = self._spider_predicted_target(leg, config, turn_speed)
            distance_to_target = math.hypot(leg.foot_x - target[0], leg.foot_y - target[1])
            threshold = self.size * (0.14 if moving or turning else 0.10)
            if fast_state:
                threshold *= 0.82
            _, relative, group = self._spider_phase_window(leg, self._lively_gait_phase, config)
            in_window = relative < config["swing_fraction"]
            _, _, _, _, wrong_side, _ = self._leg_alignment_metrics(leg, leg.foot_x, leg.foot_y)
            _, _, too_far, _ = self._leg_reach_metrics(leg, leg.foot_x, leg.foot_y, visual=False)
            needs_step = distance_to_target > threshold or wrong_side or too_far
            if not emergency and (not needs_step or not in_window):
                continue
            score = distance_to_target / max(1.0, threshold)
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
            leg.step_cooldown = random.uniform(0.010, 0.032) if skitter else random.uniform(0.02, 0.05)
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
                delay = random.uniform(0.0, 0.010 if fast_state else 0.020)
                cooldown = random.uniform(0.012, 0.044)
            else:
                delay = random.uniform(0.0, 0.02 if fast_state else 0.05)
                cooldown = random.uniform(0.04, 0.10)
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
                self._feeler_pulse_dur = random.uniform(0.45, 0.85)
                self._feeler_pulse_t = self._feeler_pulse_dur
                self._feeler_clock = random.uniform(0.5, 1.5)

        if self._feeler_pulse > 0.001:
            # Reach the front legs toward the object and let the palps probe it.
            # Kept modest so the front legs tap rather than overstretch.
            self.catch_point = (self.focus_x, self.focus_y)
            self.catch_blend = max(self.catch_blend, self._feeler_pulse * 0.5)
            self.inspect_intent = max(self.inspect_intent, self._feeler_pulse * 0.9)

    # ------------------------------------------------------------------
    # Rendering
    # ------------------------------------------------------------------
    def _qcolor(self, key: str, alpha: int = 255):
        from PyQt5.QtGui import QColor

        raw = self.colors.get(key, [35, 30, 25])
        rgb = [float(raw[0]), float(raw[1]), float(raw[2])]
        camo = getattr(self, "_camouflage_color", None)
        strength = clamp(float(getattr(self, "_camouflage_strength", 0.0)), 0.0, 1.0)
        color_blend = clamp(float(self.personality.get("camouflage_color_blend", 0.0) or 0.0), 0.0, 1.0)
        if camo is not None and strength > 0.001 and color_blend > 0.001:
            blend = strength * color_blend * (0.72 if key == "eyes" else 0.96 if key == "highlight" else 1.0)
            rgb = [rgb[i] * (1.0 - blend) + float(camo[i]) * blend for i in range(3)]
        return QColor(int(clamp(rgb[0], 0, 255)), int(clamp(rgb[1], 0, 255)), int(clamp(rgb[2], 0, 255)), alpha)

    def _appearance(self, key: str, default):
        return self.model.get("appearance", {}).get(key, default)

    def _qcolor_triplet(self, rgb, alpha: int = 255):
        from PyQt5.QtGui import QColor

        out = [float(rgb[0]), float(rgb[1]), float(rgb[2])]
        camo = getattr(self, "_camouflage_color", None)
        strength = clamp(float(getattr(self, "_camouflage_strength", 0.0)), 0.0, 1.0)
        color_blend = clamp(float(self.personality.get("camouflage_color_blend", 0.0) or 0.0), 0.0, 1.0)
        if camo is not None and strength > 0.001 and color_blend > 0.001:
            blend = strength * color_blend * 0.92
            out = [out[i] * (1.0 - blend) + float(camo[i]) * blend for i in range(3)]
        return QColor(int(clamp(out[0], 0, 255)), int(clamp(out[1], 0, 255)), int(clamp(out[2], 0, 255)), alpha)

    def _load_sprite_assets(self):
        from PyQt5.QtGui import QPixmap

        folder = self.model.get("_folder")
        if not folder:
            return {}
        assets_dir = Path(folder) / "assets"
        cache_key = str(assets_dir.resolve())
        if cache_key in self.SPRITE_CACHE:
            return self.SPRITE_CACHE[cache_key]

        names = ["abdomen", "cephalothorax", "leg_upper", "leg_lower", "leg_tip", "leg_knuckle", "shadow"]
        loaded = {}
        for name in names:
            candidate = assets_dir / f"{name}.png"
            if candidate.exists():
                pixmap = QPixmap(str(candidate))
                if not pixmap.isNull():
                    loaded[name] = pixmap
        self.SPRITE_CACHE[cache_key] = loaded
        return loaded

    def _sprite_leg_chain_config(self):
        """Return a sanitized optional multi-knuckle leg-chain configuration.

        The legacy three-piece sprite rig remains the default.  A model opts in
        with ``appearance.leg_chain`` so existing creatures keep their exact
        renderer path and asset requirements.
        """
        raw = self._appearance("leg_chain", None)
        if not isinstance(raw, dict) or not bool(raw.get("enabled", False)):
            return None
        try:
            segment_count = int(raw.get("segment_count", 4))
            if segment_count not in (4, 5):
                return None
            asset_name = str(raw.get("extra_segment_asset", "leg_knuckle")).strip()
            if not asset_name:
                return None
            split = clamp(float(raw.get("knuckle_split", 0.46)), 0.28, 0.66)
            bend = clamp(float(raw.get("knuckle_bend", 0.12)), 0.0, 0.24)
            max_stretch = clamp(float(raw.get("max_stretch", 1.05)), 1.0, 1.12)
            elevated_arc = clamp(float(raw.get("elevated_arc", 0.0)), 0.0, 0.45)
            proximal_lift = clamp(float(raw.get("proximal_lift", 1.0)), 1.0, 2.20)
            hairy = bool(raw.get("hairy", False))
            hair_scale = clamp(float(raw.get("hair_scale", 0.18)), 0.0, 0.50)
            default_scales = [1.0, 0.84, 0.64, 0.52, 0.42][:segment_count]
            # Width scales are deliberately separate from anatomical lengths.
            # Older configs only had segment_scales, which made the renderer
            # reuse presentation values as geometry and allowed a proximal
            # segment to become much longer than its knuckle could control.
            default_lengths = [1.0, 1.05, 0.78, 0.56, 0.44][:segment_count]
            default_joints = [0.95, 0.72, 0.60, 0.48][:segment_count - 1]
            default_profile = [1.0, 0.64, 0.28, 0.18][:segment_count - 1]
            default_phase_offsets = [0.0, 0.32, -0.22, 0.44][:segment_count - 1]
            default_bend_directions = [1.0, 1.0, 0.72, 0.42][:segment_count - 1]
            segment_scales = [float(value) for value in raw.get("segment_scales", default_scales)]
            segment_lengths = [float(value) for value in raw.get("segment_lengths", default_lengths)]
            joint_scales = [float(value) for value in raw.get("joint_scales", default_joints)]
            bend_profile = [float(value) for value in raw.get("bend_profile", default_profile)]
            joint_phase_offsets = [clamp(float(value), -math.tau, math.tau) for value in raw.get("joint_phase_offsets", default_phase_offsets)]
            bend_directions = [clamp(float(value), -1.4, 1.4) for value in raw.get("bend_directions", default_bend_directions)]
            joint_count = segment_count - 1
            if (len(segment_scales) != segment_count or len(segment_lengths) != segment_count
                    or len(joint_scales) != joint_count
                    or len(bend_profile) != joint_count or len(joint_phase_offsets) != joint_count
                    or len(bend_directions) != joint_count):
                return None
            if any(value <= 0.0 or not math.isfinite(value)
                   for value in segment_scales + segment_lengths + joint_scales + bend_profile):
                return None
            if any(not math.isfinite(value) for value in joint_phase_offsets + bend_directions):
                return None
            return {
                "segment_count": segment_count,
                "asset_name": asset_name,
                "split": split,
                "bend": bend,
                "max_stretch": max_stretch,
                "elevated_arc": elevated_arc,
                "proximal_lift": proximal_lift,
                "hairy": hairy,
                "hair_scale": hair_scale,
                "segment_scales": segment_scales,
                "segment_lengths": segment_lengths,
                "joint_scales": joint_scales,
                "bend_profile": bend_profile,
                "joint_phase_offsets": joint_phase_offsets,
                "bend_directions": bend_directions,
            }
        except (TypeError, ValueError):
            return None

    def _update_spider_joint_articulation(self, dt: float, config: dict) -> None:
        """Give each interior joint its own smooth flex rhythm.

        Foot contacts and body pose are mechanical state. Joint bend is a
        separate degree of freedom: it follows the leg's phase and position in
        the chain, then folds more strongly during swing. This prevents an
        added knuckle from merely duplicating one elbow angle everywhere.
        """
        chain = self._sprite_leg_chain_config()
        if chain is None:
            return
        joint_count = chain["segment_count"] - 1
        profile = chain["bend_profile"]
        phase_offsets = chain["joint_phase_offsets"]
        for leg in self.legs:
            if len(leg.joint_bends) != joint_count:
                leg.joint_bends = [0.88 + 0.08 * index for index in range(joint_count)]
            if leg.stepping:
                step_t = clamp(leg.step_timer / max(0.001, leg.step_duration), 0.0, 1.0)
                swing = math.sin(math.pi * step_t)
            else:
                swing = 0.0
            gait_angle = self._lively_gait_phase * math.tau + leg.joint_phase
            for index in range(joint_count):
                # Joints lead/lag within one leg. The middle joint folds most
                # during a lifted swing; distal joints remain restrained.
                joint_angle = gait_angle + phase_offsets[index]
                joint_wave = (
                    math.sin(joint_angle * (1.0 + index * 0.13)) * 0.72
                    + math.sin(joint_angle * 1.37 + index * 0.81) * 0.28
                )
                target = 0.86 + 0.11 * joint_wave + 0.08 * profile[index]
                if leg.stepping:
                    target += swing * (0.16 + 0.07 * (index == joint_count // 2))
                target = clamp(target, 0.55, 1.28)
                leg.joint_bends[index] += (target - leg.joint_bends[index]) * (1.0 - math.exp(-dt * 12.0))

    def _safe_sprite_leg_foot(self, leg: LegState, x: float, y: float, chain_config=None) -> Tuple[float, float]:
        """Keep rendered sprite feet inside the same reach envelope as the IK knee."""
        spider_gait = self._spider_gait_config()
        if spider_gait is not None and not leg.stepping and not leg.pending_step:
            # Do not move a planted contact point just because the attachment
            # moved with the body.  Corrective steps are scheduled by the gait
            # controller before the foot becomes impossible.
            return x, y
        safe_x, safe_y = self._limit_world_point_to_leg_reach(leg, x, y, visual=True)
        if chain_config is None:
            return safe_x, safe_y
        d = leg.definition
        segment_reach = (float(d.get("upper_len", 0.85)) + float(d.get("lower_len", 1.05))) * self.size
        max_reach = min(self._leg_max_reach(leg, visual=True), segment_reach * chain_config["max_stretch"])
        ax, ay = self._leg_attach(leg)
        dx, dy = safe_x - ax, safe_y - ay
        distance = math.hypot(dx, dy)
        if distance > max_reach and distance > 1e-5:
            scale = max_reach / distance
            safe_x, safe_y = ax + dx * scale, ay + dy * scale
        return safe_x, safe_y

    def _sprite_leg_chain_points(self, leg: LegState, ax: float, ay: float,
                                 fx: float, fy: float, chain_config: dict):
        """Solve a bounded four- or five-segment leg as one coordinated chain.

        The old renderer solved one large outward knee and then subdivided the
        remaining line.  That made the extra knuckle decorative: all joints
        inherited the same forced bow, and a long foot catch could stretch the
        first segment far past its own joint. These points now start from the
        independently phased joint pose, then pass through a 2D maximum-length
        chain solve. The root and foot stay fixed while every knuckle receives
        its own anatomical length limit.
        """
        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(fx, fy)
        direct = max(1e-4, math.hypot(f_f - a_f, f_s - a_s))
        seg_scales = chain_config["segment_scales"]
        total = max(1e-4, sum(seg_scales))
        fractions = []
        running = 0.0
        for scale in seg_scales[:-1]:
            running += scale / total
            fractions.append(running)

        _, _, rx, ry = self._basis()
        side = self._side_sign(leg.definition.get("side", "right"))
        front = self._leg_front_factor(leg)
        # The foot has already been reach-limited on the ground plane.  Keep
        # the joint fold modest at rest and let a lifted leg tuck a little more.
        bend_base = self.size * chain_config["bend"] * (0.78 + 0.34 * leg.lift)
        forward_bias = self.size * 0.075 * front * (0.72 + 0.28 * leg.lift)
        profiles = chain_config["bend_profile"]
        directions = chain_config["bend_directions"]
        elevated_arc = chain_config["elevated_arc"]
        proximal_lift = chain_config["proximal_lift"]
        joint_bends = leg.joint_bends
        if len(joint_bends) != len(profiles):
            joint_bends = [0.90 + 0.06 * index for index in range(len(profiles))]

        upper = max(self.size * 0.22, float(leg.definition.get("upper_len", 0.85)) * self.size)
        lower = max(self.size * 0.22, float(leg.definition.get("lower_len", 1.05)) * self.size)
        # Width scales are deliberately separate from anatomical lengths.
        # Older configs only had segment_scales, which made presentation values
        # double as geometry and allowed a proximal segment to grow too long.
        length_weights = chain_config["segment_lengths"]
        length_total = max(1e-4, sum(length_weights))
        segment_limits = [
            (upper + lower) * chain_config["max_stretch"] * weight / length_total
            for weight in length_weights
        ]

        def constrain_to_segment_limits(seed_points):
            """Project only overlong links while preserving both contacts."""
            points = list(seed_points)
            target = (fx, fy)

            def project(point, other, limit):
                dx = point[0] - other[0]
                dy = point[1] - other[1]
                distance = math.hypot(dx, dy)
                if distance <= limit or distance < 1e-5:
                    return point
                return (other[0] + dx * limit / distance,
                        other[1] + dy * limit / distance)

            # This is a max-length chain, so short links retain the curved
            # seed pose instead of being forced into a straight IK line.
            # Alternating projections keep the root and foot contacts stable.
            for _ in range(10):
                points[0] = (ax, ay)
                for index, limit in enumerate(segment_limits):
                    points[index + 1] = project(points[index + 1], points[index], limit)
                points[-1] = target
                for index in range(len(segment_limits) - 1, -1, -1):
                    points[index] = project(points[index], points[index + 1], segment_limits[index])
            points[0] = (ax, ay)
            points[-1] = target
            return points

        def build(bend_scale: float):
            seed_points = [(ax, ay)]
            for index, fraction in enumerate(fractions):
                bend = bend_base * profiles[index] * directions[index] * joint_bends[index] * bend_scale
                local_f = a_f + (f_f - a_f) * fraction + forward_bias * profiles[index]
                local_s = a_s + (f_s - a_s) * fraction + side * bend
                point_x, point_y = self._body_local_to_world(local_f, local_s)
                # A tarantula carries its joints high and lets the distal leg
                # descend to the floor. This is visual elevation only: the
                # foot contact and support solver remain on the ground plane.
                # The proximal limb rises first. Every later joint is lower
                # than the previous one, producing a tarantula knee arc rather
                # than a symmetric wing-like bow.
                rise = (self.size * elevated_arc * max(0.0, 1.0 - fraction)
                        * (1.0 + 0.18 * leg.lift) * bend_scale)
                if index == 0:
                    rise *= proximal_lift
                seed_points.append((point_x, point_y - rise))
            seed_points.append((fx, fy))
            points = constrain_to_segment_limits(seed_points)
            path_len = sum(math.hypot(points[i + 1][0] - points[i][0],
                                      points[i + 1][1] - points[i][1])
                           for i in range(len(points) - 1))
            return points, path_len

        points, path_len = build(1.0)
        # A bent chain may be a little longer than the direct anchor-to-foot
        # line, but never enough to look like a stretched rubber limb.
        path_budget = min((upper + lower) * chain_config["max_stretch"], direct * 1.28)
        if path_len > path_budget:
            extra = max(1e-4, path_len - direct)
            bend_scale = clamp((path_budget - direct) / extra, 0.0, 1.0)
            points, _ = build(bend_scale)
        return points

    def _draw_sprite_segment(self, painter, pixmap, x1: float, y1: float, x2: float, y2: float, thickness: float, opacity: float = 1.0):
        from PyQt5.QtCore import QPointF, QRectF

        if pixmap is None or pixmap.isNull():
            return
        dx = x2 - x1
        dy = y2 - y1
        length = math.hypot(dx, dy)
        if length < 0.5:
            return
        angle = math.degrees(math.atan2(dy, dx))
        painter.save()
        # Preserve any parent opacity, including Camouflage's whole-spider fade.
        # The previous code set segment opacity absolutely, so sprite-rig legs
        # stayed visible while the body disappeared.
        try:
            parent_opacity = float(painter.opacity())
        except Exception:
            parent_opacity = 1.0
        painter.setOpacity(parent_opacity * max(0.0, min(1.0, opacity)))
        painter.translate((x1 + x2) * 0.5, (y1 + y2) * 0.5)
        painter.rotate(angle)
        rect = QRectF(-length * 0.5, -thickness * 0.5, length, thickness)
        painter.drawPixmap(rect, pixmap, QRectF(pixmap.rect()))
        painter.restore()

    def _solve_knee(self, ax: float, ay: float, fx: float, fy: float, leg: LegState) -> Tuple[float, float]:
        """Stable anatomical knee solver.

        This solves in spider body-local space so the knee always bows outward
        from the body.  It is intentionally not a strict two-bone IK clamp: the
        sprite/procedural rig is stylised, and a strict IK branch was causing rear
        legs to lock and some legs to fold to the wrong side during turns.
        """
        d = leg.definition
        sign = self._side_sign(d.get("side", "right"))
        spider_gait = self._spider_gait_config()
        if spider_gait is None or leg.stepping or leg.pending_step:
            fx, fy = self._limit_world_point_to_leg_reach(leg, fx, fy, visual=True)
        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(fx, fy)

        rest_f = float(d.get("rest_forward", 0.0)) * self.size
        rest_s = abs(float(d.get("rest_side", 1.0)) * self.size)
        attach_s = abs(float(d.get("attach_side", 0.30)) * self.size)
        l1 = float(d.get("upper_len", 0.8)) * self.size

        # Strong outward bow. Never let the knee fold across the body axis.
        outward_side = max(abs(a_s), abs(f_s), attach_s + self.size * 0.12, rest_s * 0.60)
        outward_side += l1 * (0.10 + 0.08 * leg.lift)
        knee_s = sign * outward_side

        # Front legs bend forward, rear legs bend backward, middle legs stay near their lane.
        lane_pull = clamp(rest_f - a_f, -self.size * 0.92, self.size * 0.92)
        knee_f = (a_f + f_f) * 0.50 + lane_pull * 0.38
        if rest_f > self.size * 0.45:
            knee_f = max(knee_f, a_f + self.size * 0.20)
        elif rest_f < -self.size * 0.45:
            knee_f = min(knee_f, a_f - self.size * 0.20)

        # Keep knee and foot in the same broad anatomical lane.
        forward_min = min(a_f, rest_f, f_f) - self.size * 0.56
        forward_max = max(a_f, rest_f, f_f) + self.size * 0.56
        knee_f = clamp(knee_f, forward_min, forward_max)
        return self._body_local_to_world(knee_f, knee_s)

    def _render_procedural(self, painter) -> None:
        from PyQt5.QtCore import QPointF, QRectF, Qt
        from PyQt5.QtGui import QBrush, QPainterPath, QPen

        startle = self._startle_amount()
        tremble_x = math.sin(self.breath_phase * 17.0 + self.startle_phase) * self.size * 0.018 * startle
        tremble_y = math.cos(self.breath_phase * 19.0 + self.startle_phase * 0.7) * self.size * 0.018 * startle
        leg_y_off = -self.jump_z
        jz_shadow = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        shadow_shrink = 1.0 / (1.0 + jz_shadow * 0.55)

        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(self._qcolor("legs", max(10, int(30 * shadow_shrink)))))
        painter.drawEllipse(QPointF(self.x, self.y + self.size * 0.14), self.size * 0.78 * shadow_shrink, self.size * 0.42 * shadow_shrink)

        leg_width_scale = float(self._appearance("leg_thickness", 1.0))
        fluffiness = clamp(float(self._appearance("fluffiness", 0.0)), 0.0, 1.0)
        segmented_legs = bool(self._appearance("segmented_legs", False))
        show_joint_nodes = bool(self._appearance("show_joint_nodes", False))
        joint_node_scale = float(self._appearance("joint_node_scale", 1.0))
        coxa_width_scale = float(self._appearance("coxa_thickness_scale", 1.0))
        chain_config = self._sprite_leg_chain_config()

        # Legs first, underneath body. Segment thickness tapers from coxa to tarsus.
        for leg in self.legs:
            ax, ay, foot_x, foot_y = self._leg_draw_points(leg)
            kx, ky = self._solve_knee(ax, ay, foot_x, foot_y, leg)
            # Lift the swinging / reaching leg up off the ground (lively gait).
            # Pure draw-time screen offset applied after the knee solve so the
            # ground-plane IK stays correct; the foot also tucks slightly toward
            # the body as it rises, like a real leg flexing through its arc.
            lift_px = self._leg_lift_px(leg)
            if lift_px > 0.0:
                flex = leg.lift * 0.08
                foot_x += (ax - foot_x) * flex
                foot_y += (ay - foot_y) * flex
                foot_y -= lift_px
                ky -= lift_px * 0.45
            _, _, rx, ry = self._basis()
            side = self._side_sign(leg.definition.get("side", "right"))
            coxa_len = float(leg.definition.get("coxa_len", 0.20)) * self.size
            coxa_x = ax + rx * side * coxa_len + (kx - ax) * 0.08
            coxa_y = ay + ry * side * coxa_len + (ky - ay) * 0.08
            tarsus_x = kx + (foot_x - kx) * 0.72
            tarsus_y = ky + (foot_y - ky) * 0.72
            if leg_y_off:
                ay += leg_y_off
                ky += leg_y_off
                coxa_y += leg_y_off
                tarsus_y += leg_y_off
                foot_y += leg_y_off

            chain_points = None
            if chain_config and segmented_legs:
                # Procedural models use the same multi-joint solver as sprite
                # models, but draw the segments as tapered lines. This keeps
                # the model's joint definition and its visible articulation
                # identical without requiring copied bitmap assets.
                chain_points = self._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
                coxa_x, coxa_y = chain_points[1]
                kx, ky = chain_points[2]
                tarsus_x, tarsus_y = chain_points[3]

            base_width = max(1.4, self.size * (0.066 + leg.lift * 0.016) * leg_width_scale) * (1.0 + startle * 0.10)
            coxa_width = max(1.2, base_width * 1.08 * coxa_width_scale)
            femur_width = max(1.0, base_width * 0.98)
            tibia_width = max(1.0, base_width * 0.78)
            tarsus_width = max(1.0, base_width * 0.42)
            # A segmented spider must remain a set of separated rods. The old
            # fuzzy spline filled the gaps between joints and made long legs
            # look like bat wings, so reserve it for legacy unsegmented legs.
            if fluffiness > 0.0 and chain_points is None:
                painter.setPen(QPen(self._qcolor("highlight", int(40 + fluffiness * 50)), base_width * (1.4 + fluffiness * 0.6), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                if chain_points is not None:
                    fuzzy_path = QPainterPath(QPointF(*chain_points[0]))
                    for point_x, point_y in chain_points[1:]:
                        fuzzy_path.lineTo(QPointF(point_x, point_y))
                else:
                    fuzzy_path = QPainterPath(QPointF(ax, ay))
                    fuzzy_path.cubicTo(QPointF(coxa_x, coxa_y), QPointF(coxa_x, coxa_y), QPointF(kx, ky))
                    fuzzy_path.lineTo(QPointF(tarsus_x, tarsus_y))
                    fuzzy_path.lineTo(QPointF(foot_x, foot_y))
                painter.drawPath(fuzzy_path)

            # Leg motion is communicated by the articulated pose. Do not make
            # the swinging leg glow; highlight remains reserved for a genuine
            # startled state shared by the whole creature.
            color = self._qcolor("highlight" if startle > 0.35 else "legs", 230 if startle > 0.35 else 245)
            if chain_points is not None:
                # Keep a distinct narrow metatarsus between the tibia and toe.
                # The previous four-entry list silently dropped the fifth
                # point when Chosen One switched to a five-segment chain.
                width_bases = [
                    coxa_width,
                    femur_width,
                    tibia_width,
                    max(tarsus_width, tibia_width * 0.72),
                    tarsus_width,
                ]
                scales = chain_config["segment_scales"]
                chain_widths = [width_bases[index] * scales[index] for index in range(len(chain_points) - 1)]
                for index, width in enumerate(chain_widths):
                    start_x, start_y = chain_points[index]
                    end_x, end_y = chain_points[index + 1]
                    if chain_config["hairy"] and chain_config["hair_scale"] > 0.0:
                        hair_color = self._qcolor_triplet(
                            self._appearance("fluff_color", self.colors.get("highlight", [120, 80, 60])),
                            int(72 + chain_config["hair_scale"] * 90),
                        )
                        painter.setPen(QPen(hair_color, width * (1.12 + chain_config["hair_scale"] * 0.55), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                        painter.drawLine(QPointF(start_x, start_y), QPointF(end_x, end_y))
                    painter.setPen(QPen(color if index < len(chain_widths) - 1 else self._qcolor("legs", 210), width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                    painter.drawLine(QPointF(start_x, start_y), QPointF(end_x, end_y))
            elif segmented_legs:
                painter.setPen(QPen(color, coxa_width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.drawLine(QPointF(ax, ay), QPointF(coxa_x, coxa_y))
                painter.setPen(QPen(color, femur_width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.drawLine(QPointF(coxa_x, coxa_y), QPointF(kx, ky))
                painter.setPen(QPen(color, tibia_width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.drawLine(QPointF(kx, ky), QPointF(tarsus_x, tarsus_y))
                painter.setPen(QPen(self._qcolor("legs", 210), tarsus_width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.drawLine(QPointF(tarsus_x, tarsus_y), QPointF(foot_x, foot_y))
            else:
                painter.setPen(QPen(color, base_width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                path = QPainterPath(QPointF(ax, ay))
                path.cubicTo(QPointF(coxa_x, coxa_y), QPointF(coxa_x, coxa_y), QPointF(kx, ky))
                path.lineTo(QPointF(tarsus_x, tarsus_y))
                path.lineTo(QPointF(foot_x, foot_y))
                painter.drawPath(path)
                painter.setPen(QPen(self._qcolor("legs", 210), max(1.0, base_width * 0.48), Qt.SolidLine, Qt.RoundCap))
                painter.drawLine(QPointF(tarsus_x, tarsus_y), QPointF(foot_x, foot_y))
            if show_joint_nodes:
                painter.setPen(Qt.NoPen)
                painter.setBrush(QBrush(self._qcolor("legs", 220)))
                node_r = max(1.0, base_width * 0.24 * joint_node_scale)
                if chain_points is not None:
                    joint_scales = chain_config["joint_scales"]
                    joints = chain_points[1:-1]
                    for joint_index, (joint_x, joint_y) in enumerate(joints):
                        scale = joint_scales[joint_index]
                        radius = node_r * scale * (1.12 if joint_index == 1 else 0.96)
                        painter.drawEllipse(QPointF(joint_x, joint_y), radius, radius)
                    # A small core on every joint makes all four independently
                    # animated pivots readable at desktop scale. The patella
                    # and distal hinge receive a slightly stronger core.
                    painter.setBrush(QBrush(self._qcolor("highlight", 210)))
                    for joint_index, (joint_x, joint_y) in enumerate(joints):
                        core_scale = 0.48 if joint_index in (1, len(joints) - 1) else 0.34
                        painter.drawEllipse(QPointF(joint_x, joint_y), node_r * core_scale, node_r * core_scale)
                else:
                    painter.drawEllipse(QPointF(coxa_x, coxa_y), node_r * 1.05, node_r * 1.05)
                    painter.drawEllipse(QPointF(kx, ky), node_r * 1.30, node_r * 1.30)
                    painter.drawEllipse(QPointF(tarsus_x, tarsus_y), node_r * 0.98, node_r * 0.98)
                    painter.setBrush(QBrush(self._qcolor("highlight", 210)))
                    painter.drawEllipse(QPointF(kx, ky), node_r * 0.60, node_r * 0.60)
            painter.setPen(QPen(self._qcolor("highlight" if startle > 0.35 else "legs", 220), max(1.0, tarsus_width * 0.8), Qt.SolidLine, Qt.RoundCap))
            painter.drawPoint(QPointF(foot_x, foot_y))

        crouch_drop = self.crouch * self.size * 0.06
        painter.save()
        painter.translate(self.x + tremble_x, self.y + self.body_bob + tremble_y - self.jump_z + crouch_drop)
        painter.rotate(math.degrees(self.heading))
        # Snowpuff-2's stance solver owns the body pose; a second cosmetic
        # rotation here would rotate the shell away from the leg roots.
        if self._spider_gait_config() is None:
            painter.rotate(math.degrees(self.body_wiggle))
        jz_body = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        jump_scale = 1.0 + jz_body * 0.12
        vfac = clamp(self.squash * (1.0 - self.crouch * 0.14), 0.55, 1.2)
        vroot = math.sqrt(vfac)
        painter.scale(jump_scale / vroot, jump_scale * vroot)
        body = self._qcolor("body", 255)
        highlight = self._qcolor("highlight", int(self._appearance("highlight_alpha", 145)))
        leg_color = self._qcolor("legs", 255)
        fluff_color = self._qcolor_triplet(self._appearance("fluff_color", self.colors.get("highlight", [78, 66, 52])), int(38 + fluffiness * 70))

        abdomen_scale = self._appearance("abdomen_scale", [0.92, 0.84])
        ceph_scale = self._appearance("cephalothorax_scale", [0.70, 0.66])
        abdomen_offset_x = float(self._appearance("abdomen_offset_x", -0.18)) * self.size
        ceph_offset_x = float(self._appearance("cephalothorax_offset_x", 0.40)) * self.size
        pedipalp_scale = float(self._appearance("pedipalp_scale", 1.0))
        eye_scale = float(self._appearance("eye_scale", 1.0))
        eye_count = max(2, int(self._appearance("eye_count", 2)))

        abdomen_w = self.size * (float(abdomen_scale[0]) * (1.0 - self.abdomen_pulse * 0.35)) * (1.0 + startle * 0.055)
        abdomen_h = self.size * (float(abdomen_scale[1]) * (1.0 + self.abdomen_pulse)) * (1.0 - startle * 0.060)
        ceph_w = self.size * (float(ceph_scale[0]) * (1.0 + self.ceph_pulse * 0.25)) * (1.0 + startle * 0.075)
        ceph_h = self.size * (float(ceph_scale[1]) * (1.0 + self.ceph_pulse)) * (1.0 + startle * 0.030)
        ceph_w *= (1.0 + self.rear * 0.12)
        ceph_h *= (1.0 + self.rear * 0.12)
        ceph_offset_x += self.rear * self.size * 0.05
        abdo_wag = self.abdomen_wag * self.size * 0.45

        if fluffiness > 0.0:
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(fluff_color))
            for ox, oy, sx, sy in [
                (abdomen_offset_x - self.size * 0.03, -self.size * 0.03, abdomen_w * 0.60, abdomen_h * 0.54),
                (abdomen_offset_x - self.size * 0.13, self.size * 0.12, abdomen_w * 0.58, abdomen_h * 0.46),
                (ceph_offset_x + self.size * 0.01, -self.size * 0.06, ceph_w * 0.62, ceph_h * 0.52),
                (ceph_offset_x + self.size * 0.02, self.size * 0.08, ceph_w * 0.54, ceph_h * 0.42),
            ]:
                painter.drawEllipse(QRectF(ox - sx * 0.5, oy - sy * 0.5, sx, sy))

        painter.setPen(QPen(leg_color, max(1.0, self.size * 0.035), Qt.SolidLine, Qt.RoundCap))
        painter.setBrush(QBrush(body))
        painter.drawEllipse(QRectF(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5 + abdo_wag, abdomen_w, abdomen_h))
        painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h))

        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(highlight))
        painter.drawEllipse(QRectF(abdomen_offset_x - abdomen_w * 0.18, -abdomen_h * 0.28 + abdo_wag, abdomen_w * 0.28, abdomen_h * 0.18))
        painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.22, ceph_w * 0.20, ceph_h * 0.13))

        stripe_color = self._appearance("stripe_color", None)
        if stripe_color:
            painter.setBrush(QBrush(self._qcolor_triplet(stripe_color, int(self._appearance("stripe_alpha", 90)))))
            stripe_count = max(1, int(self._appearance("stripe_count", 2)))
            for i in range(stripe_count):
                t = i / max(1, stripe_count - 1)
                cx = abdomen_offset_x - abdomen_w * (0.12 + t * 0.18)
                painter.drawEllipse(QRectF(cx - abdomen_w * 0.08, -abdomen_h * 0.28 + t * abdomen_h * 0.16, abdomen_w * 0.16, abdomen_h * 0.10))

        painter.setPen(QPen(leg_color, max(1.2, self.size * 0.045), Qt.SolidLine, Qt.RoundCap))
        palps_y = self.size * 0.12 * pedipalp_scale * (1.0 + startle * 0.55)
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.25, -palps_y), QPointF(ceph_offset_x + ceph_w * 0.55, -palps_y * 1.85))
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.25, palps_y), QPointF(ceph_offset_x + ceph_w * 0.55, palps_y * 1.85))

        aiming = clamp(self.aim_intent, 0.0, 1.0)
        eye_startle = self._eye_startle_amount(startle)
        eye_r = max(1.0, self.size * 0.035 * eye_scale) * (1.0 + eye_startle * 0.72)
        eyes = []
        if eye_count <= 2:
            eyes.append((ceph_offset_x + ceph_w * 0.20, -ceph_h * 0.16, eye_r))
            eyes.append((ceph_offset_x + ceph_w * 0.20, ceph_h * 0.16, eye_r))
        else:
            rows = 2
            cols = max(2, eye_count // 2)
            for r in range(rows):
                for c in range(cols):
                    if r * cols + c >= eye_count:
                        break
                    eyex = ceph_offset_x + ceph_w * (0.10 + c * 0.08)
                    eyey = (-0.18 + r * 0.18 + (c % 2) * 0.02) * ceph_h
                    eyes.append((eyex, eyey, eye_r * (0.85 if c % 2 else 1.0)))
        self._draw_eyes(painter, eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming)
        self._draw_antennae(painter, ceph_offset_x, ceph_w, ceph_h, startle)
        painter.restore()

    def _render_sprite_rig(self, painter) -> None:
        from PyQt5.QtCore import QPointF, QRectF, Qt
        from PyQt5.QtGui import QBrush, QPen, QPainter

        startle = self._startle_amount()
        tremble_x = math.sin(self.breath_phase * 17.0 + self.startle_phase) * self.size * 0.018 * startle
        tremble_y = math.cos(self.breath_phase * 19.0 + self.startle_phase * 0.7) * self.size * 0.018 * startle
        leg_y_off = -self.jump_z
        jz_shadow = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        shadow_shrink = 1.0 / (1.0 + jz_shadow * 0.55)

        assets = self._load_sprite_assets()
        if not assets:
            self._render_procedural(painter)
            return

        painter.save()
        # Keep smooth pixmap transforms enabled by default. Disabling this caused
        # some PNG sprite-rig parts to render clipped/partly invisible on Windows
        # transparent overlays. Set DESKTOP_BUG_FAST_PIXMAPS=1 only if you prefer
        # speed over visual correctness.
        if os.environ.get("DESKTOP_BUG_FAST_PIXMAPS", "0").strip().lower() not in {"1", "true", "yes", "on"}:
            painter.setRenderHint(QPainter.SmoothPixmapTransform, True)

        shadow = assets.get("shadow")
        shadow_scale = float(self._appearance("shadow_scale", 1.6))
        if shadow is not None:
            painter.save()
            # Keep shadow tied to parent opacity too, so Camouflage fades as one
            # complete creature instead of leaving solid parts behind.
            try:
                parent_opacity = float(painter.opacity())
            except Exception:
                parent_opacity = 1.0
            painter.setOpacity(parent_opacity * 0.34 * shadow_shrink)
            shadow_w = self.size * shadow_scale * (1.0 + clamp(self.current_speed / 180.0, 0.0, 1.0) * 0.10) * shadow_shrink
            shadow_h = shadow_w * 0.52
            painter.drawPixmap(QRectF(self.x - shadow_w * 0.5, self.y + self.size * 0.05, shadow_w, shadow_h), shadow, QRectF(shadow.rect()))
            painter.restore()
        else:
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(self._qcolor("legs", max(8, int(28 * shadow_shrink)))))
            painter.drawEllipse(QPointF(self.x, self.y + self.size * 0.14), self.size * 0.88 * shadow_shrink, self.size * 0.46 * shadow_shrink)

        leg_upper = assets.get("leg_upper")
        leg_lower = assets.get("leg_lower", leg_upper)
        leg_tip = assets.get("leg_tip", leg_lower)
        leg_thick = float(self._appearance("leg_segment_thickness", self.size * 0.34)) * (1.0 + startle * 0.08)
        leg_tip_thick = float(self._appearance("leg_tip_thickness", self.size * 0.18)) * (1.0 + startle * 0.08)
        foot_bulb = float(self._appearance("foot_bulb", self.size * 0.055)) * (1.0 + startle * 0.20)
        chain_config = self._sprite_leg_chain_config()
        leg_knuckle = assets.get(chain_config["asset_name"], leg_lower) if chain_config else None

        for leg in self.legs:
            ax, ay, foot_x, foot_y = self._leg_draw_points(leg)
            # Use the same safe endpoint for the knee solve and the final draw.
            # Previously _solve_knee clamped only its private copy of the foot,
            # while the sprite segments still drew to the unclamped endpoint.
            foot_x, foot_y = self._safe_sprite_leg_foot(leg, foot_x, foot_y, chain_config)
            kx, ky = self._solve_knee(ax, ay, foot_x, foot_y, leg)
            # Lift the swinging / reaching leg off the ground (lively gait).
            lift_px = self._leg_lift_px(leg)
            if lift_px > 0.0:
                flex = leg.lift * 0.08
                foot_x += (ax - foot_x) * flex
                foot_y += (ay - foot_y) * flex
                foot_y -= lift_px
                ky -= lift_px * 0.45
            if leg_y_off:
                ay += leg_y_off
                ky += leg_y_off
                foot_y += leg_y_off
            step_gain = leg.lift * 0.12
            seg_scale = 1.0 + step_gain
            if chain_config:
                points = self._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
                scales = chain_config["segment_scales"]
                width_bases = [
                    leg_thick,
                    leg_thick,
                    leg_thick * 0.84,
                    leg_thick * 0.68,
                    leg_tip_thick,
                ]
                widths = [width_bases[index] * scales[index] * seg_scale for index in range(len(points) - 1)]
                sprites = [leg_upper, leg_lower, leg_knuckle, leg_knuckle, leg_tip][:len(points) - 1]
                for index, sprite in enumerate(sprites):
                    start_x, start_y = points[index]
                    end_x, end_y = points[index + 1]
                    self._draw_sprite_segment(painter, sprite, start_x, start_y, end_x, end_y, widths[index], opacity=1.0)
            else:
                tarsus_x = kx + (foot_x - kx) * 0.72
                tarsus_y = ky + (foot_y - ky) * 0.72
                self._draw_sprite_segment(painter, leg_upper, ax, ay, kx, ky, leg_thick * seg_scale, opacity=1.0)
                self._draw_sprite_segment(painter, leg_lower, kx, ky, tarsus_x, tarsus_y, leg_thick * 0.84 * seg_scale, opacity=1.0)
                self._draw_sprite_segment(painter, leg_tip, tarsus_x, tarsus_y, foot_x, foot_y, leg_tip_thick * seg_scale, opacity=1.0)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(self._qcolor("legs", 215)))
            if chain_config:
                joint_scales = chain_config["joint_scales"]
                for joint_index, (joint_x, joint_y) in enumerate(points[1:-1]):
                    radius = leg_thick * 0.14 * joint_scales[joint_index]
                    painter.drawEllipse(QPointF(joint_x, joint_y), radius, radius)
                painter.setBrush(QBrush(self._qcolor("highlight", 210)))
                joints = points[1:-1]
                for joint_index, (joint_x, joint_y) in enumerate(joints):
                    core_scale = 0.075 if joint_index in (1, len(joints) - 1) else 0.052
                    radius = leg_thick * core_scale * joint_scales[joint_index]
                    painter.drawEllipse(QPointF(joint_x, joint_y), radius, radius)
            else:
                knee_scale = 1.0
                painter.drawEllipse(QPointF(kx, ky), leg_thick * 0.14 * knee_scale, leg_thick * 0.14 * knee_scale)
                painter.drawEllipse(QPointF(tarsus_x, tarsus_y), leg_tip_thick * 0.18, leg_tip_thick * 0.18)
            painter.setBrush(QBrush(self._qcolor("highlight" if startle > 0.35 else "legs", 215 if startle > 0.35 else 180)))
            painter.drawEllipse(QPointF(foot_x, foot_y), foot_bulb * (1.0 + step_gain), foot_bulb * 0.70 * (1.0 + step_gain))

        crouch_drop = self.crouch * self.size * 0.06
        painter.save()
        painter.translate(self.x + tremble_x, self.y + self.body_bob + tremble_y - self.jump_z + crouch_drop)
        painter.rotate(math.degrees(self.heading))
        if self._spider_gait_config() is None:
            painter.rotate(math.degrees(self.body_wiggle))
        jz_body = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        jump_scale = 1.0 + jz_body * 0.12
        vfac = clamp(self.squash * (1.0 - self.crouch * 0.14), 0.55, 1.2)
        vroot = math.sqrt(vfac)
        painter.scale(jump_scale / vroot, jump_scale * vroot)

        abdomen = assets.get("abdomen")
        cephalothorax = assets.get("cephalothorax")
        abdomen_scale = self._appearance("abdomen_scale", [1.08, 0.92])
        ceph_scale = self._appearance("cephalothorax_scale", [0.84, 0.78])
        abdomen_offset_x = float(self._appearance("abdomen_offset_x", -0.18)) * self.size
        ceph_offset_x = float(self._appearance("cephalothorax_offset_x", 0.36)) * self.size

        abdomen_w = self.size * float(abdomen_scale[0]) * (1.0 - self.abdomen_pulse * 0.18) * (1.0 + startle * 0.050)
        abdomen_h = self.size * float(abdomen_scale[1]) * (1.0 + self.abdomen_pulse * 0.55) * (1.0 - startle * 0.055)
        ceph_w = self.size * float(ceph_scale[0]) * (1.0 + self.ceph_pulse * 0.08) * (1.0 + startle * 0.080)
        ceph_h = self.size * float(ceph_scale[1]) * (1.0 + self.ceph_pulse * 0.18) * (1.0 + startle * 0.030)
        ceph_w *= (1.0 + self.rear * 0.12)
        ceph_h *= (1.0 + self.rear * 0.12)
        ceph_offset_x += self.rear * self.size * 0.05

        if abdomen is not None:
            painter.save()
            painter.rotate(math.degrees(self.abdomen_wag))
            painter.drawPixmap(QRectF(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5, abdomen_w, abdomen_h), abdomen, QRectF(abdomen.rect()))
            painter.restore()
        if cephalothorax is not None:
            painter.drawPixmap(QRectF(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h), cephalothorax, QRectF(cephalothorax.rect()))

        painter.setPen(QPen(self._qcolor("legs", 215), max(1.0, self.size * 0.036), Qt.SolidLine, Qt.RoundCap))
        palp_span = self.size * float(self._appearance("pedipalp_scale", 1.0))
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.24, -self.size * 0.10), QPointF(ceph_offset_x + ceph_w * 0.54, -self.size * 0.21 * palp_span / self.size))
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.24, self.size * 0.10), QPointF(ceph_offset_x + ceph_w * 0.54, self.size * 0.21 * palp_span / self.size))

        eye_scale = float(self._appearance("eye_scale", 0.95))
        eye_count = max(2, int(self._appearance("eye_count", 4)))
        eye_layout = str(self._appearance("eye_layout", "grid")).lower()
        aiming = clamp(self.aim_intent, 0.0, 1.0)
        eye_startle = self._eye_startle_amount(startle)
        eye_r = max(1.0, self.size * 0.030 * eye_scale) * (1.0 + eye_startle * 0.78)
        eyes = []
        if eye_layout == "jumping_spider":
            big_r = eye_r * 1.9
            small_r = eye_r * 0.75
            eyes = [
                (ceph_offset_x + ceph_w * 0.05, -ceph_h * 0.03, big_r),
                (ceph_offset_x + ceph_w * 0.28, -ceph_h * 0.03, big_r),
                (ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.14, small_r),
                (ceph_offset_x + ceph_w * 0.44, -ceph_h * 0.14, small_r),
                (ceph_offset_x - ceph_w * 0.04, ceph_h * 0.16, small_r * 0.9),
                (ceph_offset_x + ceph_w * 0.36, ceph_h * 0.16, small_r * 0.9),
            ]
        else:
            rows = 2
            cols = max(2, eye_count // 2)
            for r in range(rows):
                for c in range(cols):
                    if r * cols + c >= eye_count:
                        break
                    eyex = ceph_offset_x + ceph_w * (0.02 + c * 0.11)
                    eyey = (-0.17 + r * 0.17 + (c % 2) * 0.015) * ceph_h
                    eyes.append((eyex, eyey, eye_r * (0.92 if c % 2 else 1.0)))
        self._draw_eyes(painter, eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming)
        self._draw_antennae(painter, ceph_offset_x, ceph_w, ceph_h, startle)
        painter.restore()
        painter.restore()

    # ------------------------------------------------------------------
    # Expressive rendering helpers (antennae, eyes, jump/catch leg posing)
    # ------------------------------------------------------------------
    def _leg_front_factor(self, leg: LegState) -> float:
        """How forward-facing a leg is: ~1 front pair, ~0 mid, ~-1 rear."""
        d = leg.definition
        if "rest_forward" in d:
            return clamp(float(d.get("rest_forward", 0.0)), -1.0, 1.0)
        return math.cos(math.radians(float(d.get("attach_angle", 0.0))))

    def _leg_lift_px(self, leg: LegState) -> float:
        """Screen-space height a swinging (or reaching) leg is raised, for the
        lively gait.  Returns 0 for the classic gait so its look is unchanged.

        The swing lift peels a stepping foot off the ground in a visible arc;
        front legs that are reaching out to feel an object rise a little extra.
        """
        if not self._uses_lively_gait():
            return 0.0
        spider_gait = self._spider_gait_config()
        lift_scale = spider_gait["swing_height"] if spider_gait is not None else (0.42 if self._uses_skitter_gait() else 0.55)
        px = leg.lift * self.size * lift_scale
        front = self._leg_front_factor(leg)
        if self.catch_blend > 0.01 and front > 0.15:
            px += self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0) * self.size * 0.28
        return px

    def _front_leg_feeler_pose(self, leg: LegState, ax: float, ay: float, foot_x: float, foot_y: float, front: float) -> Tuple[float, float]:
        """Optional model-specific pose bias that lets the front legs replace the
        cartoon antennae.  Only the foremost legs are affected, and only for
        models that opt into it through appearance settings.
        """
        if not bool(self._appearance("front_leg_feeler_mode", False)) or front <= 0.18:
            return foot_x, foot_y

        strength = clamp((front - 0.18) / 0.82, 0.0, 1.0)
        idle_lift = float(self._appearance("front_leg_idle_lift", 0.0)) * self.size * strength
        idle_forward = float(self._appearance("front_leg_idle_forward", 0.0)) * self.size * strength
        sway = float(self._appearance("front_leg_feeler_sway", 0.0)) * self.size * strength

        forward_dx = math.cos(self.heading) * idle_forward
        forward_dy = math.sin(self.heading) * idle_forward
        side_sign = self._side_sign(leg.definition.get("side", "right"))
        side_dx = -math.sin(self.heading) * sway * side_sign
        side_dy = math.cos(self.heading) * sway * side_sign

        # Small organic probing motion: quick little feeler sweeps with pauses.
        # The lively/skitter gait already handles the planted stepping; this is
        # only a presentational bias layered on top.
        feel_clock = self.breath_phase * (2.9 if self._uses_skitter_gait() else 2.1) + leg.phase_seed * 0.9
        pulse = 0.5 + 0.5 * math.sin(feel_clock)
        foot_x += forward_dx * (0.60 + pulse * 0.40) + side_dx * math.sin(feel_clock * 1.8)
        foot_y += forward_dy * (0.60 + pulse * 0.40) + side_dy * math.sin(feel_clock * 1.8)
        foot_y -= idle_lift * (0.65 + pulse * 0.35)

        # Keep the reach believable.
        max_span = self.size * (float(leg.definition.get("reach", 1.8)) + 0.45)
        ddx, ddy = foot_x - ax, foot_y - ay
        dlen = math.hypot(ddx, ddy)
        if dlen > max_span and dlen > 1e-4:
            scale = max_span / dlen
            foot_x = ax + ddx * scale
            foot_y = ay + ddy * scale
        return foot_x, foot_y

    def _leg_draw_points(self, leg: LegState) -> Tuple[float, float, float, float]:
        """World-space (attach, foot) for rendering with catch-reach + airborne tuck.

        The tuned gait keeps ``leg.foot_x/foot_y`` planted; this only adjusts the
        *rendered* foot so the front legs can reach out during a catch and all legs
        tuck up while the body is in the air.  Vertical jump lift is applied later
        as a pure draw-time screen offset so the solved knee geometry stays correct.
        """
        ax, ay = self._leg_attach(leg)
        foot_x, foot_y = self._visual_foot_for_render(leg)
        front = self._leg_front_factor(leg)
        if self.catch_blend > 0.001 and front > 0.15:
            cx, cy = self.catch_point
            reach = self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0)
            rx = foot_x + (cx - foot_x) * 0.55 * reach
            ry = foot_y + (cy - foot_y) * 0.55 * reach
            # Never let a distant target stretch the leg past a believable span.
            max_span = self.size * (float(leg.definition.get("reach", 1.8)) + 0.4)
            ddx, ddy = rx - ax, ry - ay
            dlen = math.hypot(ddx, ddy)
            if dlen > max_span and dlen > 1e-4:
                scale = max_span / dlen
                rx = ax + ddx * scale
                ry = ay + ddy * scale
            foot_x, foot_y = rx, ry
        if self.state == "Feed" and front > 0.2:
            # Work the prey held at the front: rapid in/out tugging along the
            # line to the catch point, plus a small sideways knead, each front
            # leg slightly out of phase so they look like busy little hands.
            paw = getattr(self, "_feed_paw", 0.0)
            cx, cy = self.catch_point
            dx, dy = cx - foot_x, cy - foot_y
            dl = math.hypot(dx, dy) or 1.0
            ux, uy = dx / dl, dy / dl
            amp = self.size * 0.11 * clamp((front - 0.2) / 0.8, 0.0, 1.0)
            osc = math.sin(paw * 22.0 + leg.phase_seed * 2.0)
            foot_x += ux * amp * osc
            foot_y += uy * amp * osc
            knead = math.sin(paw * 17.0 + leg.phase_seed)
            foot_x += -uy * amp * 0.5 * knead
            foot_y += ux * amp * 0.5 * knead
        foot_x, foot_y = self._front_leg_feeler_pose(leg, ax, ay, foot_x, foot_y, front)
        if self.airborne and self.jump_peak > 1e-3:
            tuck = clamp(self.jump_z / self.jump_peak, 0.0, 1.0) * 0.8
            foot_x += (ax - foot_x) * tuck
            foot_y += (ay - foot_y) * tuck
        if self.roll_tuck > 1e-3:
            # Curl the legs in toward the body so the spinning spider reads as a
            # tucked ball rather than a splayed star.
            t = clamp(self.roll_tuck, 0.0, 1.0) * 0.62
            foot_x += (ax - foot_x) * t
            foot_y += (ay - foot_y) * t
        return ax, ay, foot_x, foot_y

    def _draw_antennae(self, painter, ceph_offset_x: float, ceph_w: float, ceph_h: float, startle: float) -> None:
        """Two expressive feelers on the head front; shape carries the emotion."""
        from PyQt5.QtCore import QPointF, Qt
        from PyQt5.QtGui import QBrush, QPen

        cfg = self._appearance("antennae", {})
        if not isinstance(cfg, dict):
            cfg = {}
        if cfg.get("enabled", True) is False:
            return

        segments = max(3, int(cfg.get("segments", 5)))
        length_units = float(cfg.get("length", 1.15))
        base_forward = float(cfg.get("base_forward", 0.34))
        base_side = float(cfg.get("base_side", 0.26))
        thickness = float(cfg.get("thickness", 0.06))
        color_key = str(cfg.get("color_key", "legs"))
        tip_color_key = str(cfg.get("tip_color_key", "highlight"))

        aiming = clamp(self.aim_intent, 0.0, 1.0)
        inspecting = clamp(self.inspect_intent, 0.0, 1.0)
        cuddling = clamp(max(self.cuddle_intent, self.catch_blend), 0.0, 1.0)
        aim_angle = self._antenna_aim_angle()
        drive = antenna_drive_from_mood(
            self.mood,
            aiming=aiming,
            inspecting=inspecting,
            cuddling=cuddling,
            aim_angle=aim_angle,
        )

        base_x = ceph_offset_x + ceph_w * base_forward
        span = length_units * self.size * (1.0 + startle * 0.08)
        line_col = self._qcolor(color_key, 240)
        tip_col = self._qcolor(tip_color_key, 240)

        # Tarantulas do not have insect-like antennae.  Their front appendages
        # are broad, jointed, and rise out of the cephalothorax before the
        # remaining links descend toward the ground.  Keep this as an explicit
        # model style so the expressive feeler rig remains available to the
        # other creatures.
        if str(cfg.get("style", "")).strip().lower() in ("front_leg", "tarantula_front_legs"):
            raw_lengths = cfg.get("segment_lengths", [0.24, 0.28, 0.22, 0.15, 0.11])
            if not isinstance(raw_lengths, list):
                raw_lengths = [0.24, 0.28, 0.22, 0.15, 0.11]
            try:
                lengths = [float(value) for value in raw_lengths[:segments]]
            except (TypeError, ValueError):
                lengths = []
            if len(lengths) != segments or any(value <= 0.0 or not math.isfinite(value) for value in lengths):
                lengths = [1.0 for _ in range(segments)]
            length_total = max(1e-4, sum(lengths))
            # The proximal link points outward/upward; every following link
            # turns more forward and then slightly inward, like a tarantula's
            # femur -> tibia -> metatarsus -> tarsus sequence.
            angle_profile = [0.86, 0.56, 0.27, 0.08, -0.06]
            while len(angle_profile) < segments:
                angle_profile.append(angle_profile[-1] * 0.72)
            joint_scale = clamp(float(cfg.get("joint_scale", 0.95)), 0.45, 1.60)
            proximal_rise = clamp(float(cfg.get("proximal_rise", 0.12)), 0.0, 0.28) * self.size
            leg_thickness = clamp(float(cfg.get("leg_thickness", 1.0)), 0.65, 1.60)
            hairy = bool(cfg.get("hairy", False))
            hair_scale = clamp(float(cfg.get("hair_scale", 0.16)), 0.0, 0.50)
            span *= clamp(drive.length, 0.65, 1.50)

            for idx, side_sign in enumerate((-1.0, 1.0)):
                root_y = side_sign * ceph_h * base_side
                fwd = 0.0
                lateral = 0.0
                screen = [(base_x, root_y)]
                for segment_index in range(segments):
                    angle = side_sign * angle_profile[segment_index]
                    # Mood motion stays subtle; the silhouette remains a
                    # controlled leg chain rather than a waving thread.
                    angle += math.sin(self.antenna_phase[idx] + segment_index * 0.72) * drive.wave_amp * 0.045
                    if drive.aim_blend > 0.01:
                        angle = angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend
                    segment = span * lengths[segment_index] / length_total
                    fwd += math.cos(angle) * segment
                    lateral += math.sin(angle) * segment
                    progress = (segment_index + 1) / max(1, segments)
                    rise = proximal_rise * max(0.0, 1.0 - progress)
                    screen.append((base_x + fwd, root_y + lateral - rise))

                for segment_index in range(segments):
                    width = max(
                        1.5,
                        self.size * thickness * drive.thickness * leg_thickness
                        * (1.18 - 0.13 * segment_index),
                    )
                    x1, y1 = screen[segment_index]
                    x2, y2 = screen[segment_index + 1]
                    if hairy and hair_scale > 0.0:
                        hair_color = self._qcolor_triplet(
                            self._appearance("fluff_color", self.colors.get("highlight", [120, 80, 60])),
                            int(70 + hair_scale * 95),
                        )
                        painter.setPen(QPen(hair_color, width * (1.16 + hair_scale * 0.60), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                        painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
                    painter.setPen(QPen(line_col, width, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                    painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

                painter.setPen(Qt.NoPen)
                painter.setBrush(QBrush(line_col))
                for joint_index, (joint_x, joint_y) in enumerate(screen[1:-1]):
                    joint_radius = max(1.5, self.size * thickness * drive.thickness * 0.19 * joint_scale)
                    joint_radius *= 1.08 if joint_index == 0 else (0.92 - min(0.25, joint_index * 0.05))
                    painter.drawEllipse(QPointF(joint_x, joint_y), joint_radius, joint_radius)
                tx, ty = screen[-1]
                tip_radius = max(1.6, self.size * thickness * drive.tip_bulb * 0.25)
                painter.setBrush(QBrush(tip_col))
                painter.drawEllipse(QPointF(tx, ty), tip_radius, tip_radius)
            return

        for idx, side_sign in enumerate((-1.0, 1.0)):
            pts = build_antenna_points(side_sign, segments, drive, self.antenna_phase[idx])
            root_y = side_sign * ceph_h * base_side
            screen = [(base_x + fwd * span, root_y + sd * span) for (fwd, sd) in pts]
            n = len(screen)
            for j in range(n - 1):
                t = j / max(1, n - 2)
                w = max(1.0, self.size * thickness * drive.thickness * (1.0 - 0.55 * t))
                painter.setPen(QPen(line_col, w, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                x1, y1 = screen[j]
                x2, y2 = screen[j + 1]
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
            tx, ty = screen[-1]
            r = max(1.2, self.size * thickness * drive.tip_bulb * 0.9)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(tip_col))
            painter.drawEllipse(QPointF(tx, ty), r, r)

    def _draw_eyes(self, painter, eyes, ceph_offset_x: float, ceph_w: float, ceph_h: float, startle: float, aiming: float) -> None:
        """Draw a set of eyes whose openness, gaze and shape follow the mood."""
        from PyQt5.QtCore import QPointF, QRectF, Qt
        from PyQt5.QtGui import QBrush, QPainterPath, QPen

        m = self.mood
        # Affectionate / cute blush sits under the eyes.
        blush_amt = clamp(m.affection * 0.75 + float(self._appearance("cute_blush", 0.0)), 0.0, 1.0)
        if blush_amt > 0.05:
            col = self._qcolor_triplet([255, 168, 176], int(30 + blush_amt * 80))
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(col))
            painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.05, -ceph_h * 0.32, ceph_w * 0.17, ceph_h * 0.12))
            painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.05, ceph_h * 0.20, ceph_w * 0.17, ceph_h * 0.12))

        eye_startle = self._eye_startle_amount(startle)
        openness = clamp(
            0.62 + m.arousal * 0.5 + aiming * 0.32 - m.sleepy * 0.55 - max(0.0, m.valence) * 0.16,
            0.12,
            1.3,
        ) * (1.0 + eye_startle * 0.4)
        eff_open = clamp(openness * (1.0 - self.expression_blink * 0.92), 0.05, 1.4)
        squint_happy = (m.valence > 0.45 and m.arousal < 0.72 and aiming < 0.2 and self.expression_blink < 0.4)

        base_col = self._qcolor("eyes", 240)
        pupil_col = self._qcolor_triplet([20, 18, 26], 255)
        shine_col = self._qcolor_triplet([255, 255, 255], 235)

        for (ex, ey, r) in eyes:
            if squint_happy:
                painter.setPen(QPen(pupil_col, max(1.2, r * 0.5), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.setBrush(Qt.NoBrush)
                arc = QPainterPath(QPointF(ex - r, ey + r * 0.28))
                arc.quadTo(QPointF(ex, ey - r * 0.82), QPointF(ex + r, ey + r * 0.28))
                painter.drawPath(arc)
                continue
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(base_col))
            painter.drawEllipse(QRectF(ex - r, ey - r * eff_open, r * 2.0, r * 2.0 * eff_open))
            if eff_open > 0.22:
                gx = ex + self.look_fwd * r * 0.40
                gy = ey + self.look_side * r * 0.40 * eff_open
                pr = r * 0.52
                painter.setBrush(QBrush(pupil_col))
                painter.drawEllipse(QPointF(gx, gy), pr, pr * eff_open)
                painter.setBrush(QBrush(shine_col))
                painter.drawEllipse(QPointF(gx - pr * 0.32, gy - pr * 0.38 * eff_open), pr * 0.34, pr * 0.34 * eff_open)

    def _apply_cage_bounds(self) -> None:
        """Keep a caged spider inside its enclosure.

        The body centre is clamped to the cage interior, the steering target is
        pulled back inside so the AI stops trying to leave, any outward inertia
        is cancelled, and the planted feet are carried along by the correction so
        legs do not rubber-band across the fence.
        """
        cage = self.cage
        if cage is None:
            return
        inset = self.size * 0.85
        new_x, new_y = cage.clamp_center(self.x, self.y, inset)
        dx = new_x - self.x
        dy = new_y - self.y
        if dx or dy:
            self.x = new_x
            self.y = new_y
            # Bleed off velocity/inertia that points into the wall we just hit.
            if dx > 0:
                self.inertia_vx = max(0.0, self.inertia_vx)
                self.vel_x = max(0.0, self.vel_x)
            elif dx < 0:
                self.inertia_vx = min(0.0, self.inertia_vx)
                self.vel_x = min(0.0, self.vel_x)
            if dy > 0:
                self.inertia_vy = max(0.0, self.inertia_vy)
                self.vel_y = max(0.0, self.vel_y)
            elif dy < 0:
                self.inertia_vy = min(0.0, self.inertia_vy)
                self.vel_y = min(0.0, self.vel_y)
            self._translate_leg_world_points(dx, dy, 0.6)
        # Always keep the steering target inside so wandering re-aims indoors.
        self.target_x, self.target_y = cage.clamp_center(self.target_x, self.target_y, inset)

    # ------------------------------------------------------------------
    # Identity / hover label
    # ------------------------------------------------------------------
    def set_name(self, name: str) -> None:
        self.name = (name or "").strip()

    @property
    def display_name(self) -> str:
        return self.name

    def _label_font(self):
        from PyQt5.QtGui import QFont

        font = QFont()
        font.setPointSizeF(max(8.0, min(13.0, self.size * 0.42)))
        font.setBold(True)
        return font

    def label_visible(self, always_show: bool) -> bool:
        return bool(self.name) and (self._hovered or always_show)

    # ------------------------------------------------------------------
    # Screen-space bounding box (for partial repaints)
    # ------------------------------------------------------------------
    def bounding_rect(self, always_show_names: bool = False) -> Tuple[float, float, float, float]:
        """Return a padded (x0, y0, x1, y1) covering everything this spider draws.

        Includes the body, every leg's planted/visual foot, the soft shadow, the
        airborne lift, and the hover name label when it is showing.  The result
        is generous on purpose: clipping a leg tip during partial repaints would
        leave visible litter on the desktop.
        """
        min_x = max_x = self.x
        min_y = max_y = self.y
        for leg in self.legs:
            fx, fy = leg.foot_x, leg.foot_y
            ax, ay = self._leg_attach(leg)
            if fx < min_x:
                min_x = fx
            elif fx > max_x:
                max_x = fx
            if fy < min_y:
                min_y = fy
            elif fy > max_y:
                max_y = fy
            if ax < min_x:
                min_x = ax
            elif ax > max_x:
                max_x = ax
            if ay < min_y:
                min_y = ay
            elif ay > max_y:
                max_y = ay

        # Pad for leg thickness, antennae/pedipalps, shadow spread and tremble.
        pad = self.size * 0.85 + 12.0
        min_x -= pad
        max_x += pad
        min_y -= pad
        max_y += pad
        # The body is lifted upward by jump_z during a hop; the shadow stays low.
        min_y -= self.jump_z

        if self.state == "Roll" or abs(self.roll_spin) > 1e-4:
            # The whole creature spins about its centre, so guarantee a circular
            # footprint large enough to cover the body and legs at any angle.
            radius = self.size * 2.2 + pad
            min_x = min(min_x, self.x - radius)
            max_x = max(max_x, self.x + radius)
            min_y = min(min_y, self.y - radius)
            max_y = max(max_y, self.y + radius)

        if self.label_visible(always_show_names):
            from PyQt5.QtGui import QFontMetrics

            fm = QFontMetrics(self._label_font())
            text = self.display_name
            half_w = fm.horizontalAdvance(text) * 0.5 + 12.0
            label_h = fm.height() + 12.0
            min_x = min(min_x, self.x - half_w)
            max_x = max(max_x, self.x + half_w)
            # Label floats above the highest drawn point.
            min_y -= label_h + 6.0

        self._bbox = (min_x, min_y, max_x, max_y)
        return self._bbox

    def _draw_name_label(self, painter, always_show_names: bool) -> None:
        if not self.label_visible(always_show_names):
            return
        from PyQt5.QtCore import QRectF, Qt
        from PyQt5.QtGui import QColor, QFontMetrics, QPen, QBrush

        font = self._label_font()
        painter.setFont(font)
        fm = QFontMetrics(font)
        text = self.display_name
        tw = fm.horizontalAdvance(text)
        th = fm.height()
        pad_x = 8.0
        pad_y = 4.0
        box_w = tw + pad_x * 2.0
        box_h = th + pad_y * 2.0

        # Sit just above the spider's body/leg cluster.
        top_extent = self.y
        for leg in self.legs:
            if leg.foot_y < top_extent:
                top_extent = leg.foot_y
        top_extent -= self.size * 0.6 + self.jump_z
        box_x = self.x - box_w * 0.5
        box_y = top_extent - box_h - 4.0

        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(QColor(18, 18, 22, 205)))
        painter.drawRoundedRect(QRectF(box_x, box_y, box_w, box_h), 6.0, 6.0)
        painter.setBrush(Qt.NoBrush)
        painter.setPen(QPen(QColor(255, 255, 255, 60), 1.0))
        painter.drawRoundedRect(QRectF(box_x, box_y, box_w, box_h), 6.0, 6.0)
        painter.setPen(QPen(QColor(245, 247, 250, 255)))
        painter.drawText(QRectF(box_x, box_y, box_w, box_h), Qt.AlignCenter, text)

    def render(self, painter, always_show_names: bool = False) -> None:
        render_mode = str(self.model.get("render_mode", "procedural")).lower()
        camouflage_strength = clamp(float(getattr(self, "_camouflage_strength", 0.0)), 0.0, 1.0)
        camouflage_opacity = clamp(1.0 - camouflage_strength * float(self.personality.get("camouflage_opacity_drop", 0.72)), 0.12, 1.0)
        camouflage_saved = False
        if camouflage_opacity < 0.999:
            painter.save()
            try:
                painter.setOpacity(painter.opacity() * camouflage_opacity)
            except Exception:
                painter.setOpacity(camouflage_opacity)
            camouflage_saved = True
        rolling = abs(self.roll_spin) > 1e-4
        if rolling:
            # Spin the whole creature (legs and body) about its centre for a
            # tumble. The name label is drawn afterwards so it stays upright.
            painter.save()
            painter.translate(self.x, self.y)
            painter.rotate(math.degrees(self.roll_spin))
            painter.translate(-self.x, -self.y)
        if render_mode == "sprite_rig":
            self._render_sprite_rig(painter)
        else:
            self._render_procedural(painter)
        if rolling:
            painter.restore()
        # Name labels should obey the same visibility as the spider.  Otherwise a
        # hidden Camouflage spider would still leave a floating readable label.
        self._draw_name_label(painter, always_show_names)
        if camouflage_saved:
            painter.restore()
