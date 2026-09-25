"""`Creature`: composes the mixins below into the one public class.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern. This
module holds `__init__` and the methods that are neither state machine, leg
solver, mood expression, nor renderer -- progression, stats, direct
manipulation, and the small per-frame `update`/`render` entry points that
call into the other mixins.
"""

from __future__ import annotations

from PyQt5.QtCore import QRectF, Qt
from PyQt5.QtGui import QBrush, QColor, QFont, QFontMetrics, QPen

import math
import random
from typing import List, Tuple

from ..support.math_utils import (
    angle_lerp,
    angle_to,
    clamp,
    clamp_point,
    cursor_is_threatening,
    rand_range,
)
from .mood import Mood
from .perception import build_perception
from .phase_scheduler import BehaviourPhaseScheduler
from ..state.progression import (
    ABILITY_BY_ID,
    ABILITY_TREE,
    ARMOR_BY_ID,
    MAX_LEVEL,
    ProgressionState,
    equipped_items,
    growth_multipliers,
    normalize_team_id,
    relation_between,
    xp_to_next_level,
)
from ..content.skills import SkillSet, default_skills_for_personality
from ..world.jobs import normalize_job_id
from .constants import (
    FLEE_SPEED_MULT,
    WEBBED_HOLD_SECONDS,
    WEBBED_SECONDS,
    WEBBED_SPEED_MULT,
    CURSOR_PRESSURE_PER_GRAB,
    CURSOR_WARY_THRESHOLD,
    GAIT_SPELL_GAP,
    normalize_gait_style,
)
from .behaviour import BehaviourMixin
from .expression import ExpressionMixin
from .kinematics import KinematicsMixin, LegState
from .render_procedural import RenderProceduralMixin
from .render_sprite import RenderSpriteMixin


class Creature(BehaviourMixin, KinematicsMixin, ExpressionMixin, RenderProceduralMixin, RenderSpriteMixin):
    """One data-driven desktop creature with procedural or hybrid sprite-rig rendering."""

    SPRITE_CACHE = {}
    _SIDE_SIGNS: dict = {}
    # Every spider's bars are this wide (DC-87). They used to stretch to the
    # name box, so a long name meant a long bar and two spiders at full health
    # looked different; the owner asked for "same width for all spiders".
    LABEL_BAR_WIDTH = 44.0
    # Pale gold under the level; not the health bar's half-health amber.
    XP_BAR_COLOR = (242, 212, 128)
    # Sky blue: the health bar is green when full, so stamina must not be.
    STAMINA_BAR_COLOR = (92, 176, 226)
    BASE_SIZE = 28.0

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
        color_overrides: dict | None = None,
        progression_state: dict | None = None,
        progression_id: str | None = None,
        job_id: str = "none",
        seed: int | None = None,
    ):
        self.model = model
        # (model, config) for _spider_gait_config, which is otherwise the
        # most expensive call in a frame. Keyed on the model object so a
        # swapped model rebuilds rather than serving a stale gait.
        self._gait_config_cache = None
        # (heading, basis) and (camouflage signature, {key: rgb}). Both are pure
        # caches of values recomputed hundreds of times per frame; see the
        # methods that use them for why each key is the right one.
        self._basis_cache = None
        self._qcolor_cache = None
        # Leg chain solves for the frame being drawn, cleared by `render`.
        self._chain_points_cache: dict = {}
        self._reach_cache: dict = {}
        self._triplet_cache = None
        self.personality = personality
        self.screen_w = max(200, int(screen_w))
        self.screen_h = max(200, int(screen_h))
        self.margin = 50.0
        self.index = index
        self.progression_id = str(progression_id or f"runtime:{index}")

        # A caller that wants a replayable run passes a seed; it is combined
        # with this spider's own identity so that two spiders given the same
        # run seed still diverge from each other, and the same spider always
        # gets the same stream across a run regardless of how many other
        # spiders were constructed first. A caller that does not care about
        # replay gets `random` itself, so every existing behaviour that seeds
        # the module-level generator (tests, mainly) is unaffected.
        self.rng = random if seed is None else random.Random(f"{seed}:{self.progression_id}")

        self.skills = SkillSet(
            skills if skills is not None else default_skills_for_personality(personality)
        )
        # High-level behaviour periods are planned independently of the concrete
        # FSM.  With no seed, the scheduler keeps its own independent RNG (as
        # before) so two identical spiders do not march through the same
        # personality sequence and so the module-level random stream that an
        # unseeded caller may itself be seeding is not disturbed. A seeded
        # caller gets the scheduler tied to the same replayable stream.
        self.phase_scheduler = BehaviourPhaseScheduler(
            personality, self.skills.ids(), rng=self.rng if seed is not None else None
        )

        # Runtime progression is deliberately separate from the preset's model
        # and personality.  Older callers can omit it and receive a fresh level
        # one spider; a manager can restore it from a saved profile or slot.
        self.progression = ProgressionState.from_dict(progression_state)
        self.job_id = normalize_job_id(job_id)
        # BaseWorld writes these small intents before update(); they let jobs
        # drive locomotion without pretending that a profession is a personality.
        self.job_mode = "idle"
        self.job_target = None
        self.job_alert_target = None
        self.job_base_id = None
        # DC-42: a job may ask for a facing that is not the way the spider is
        # walking -- a guard holding a line looks outwards, across its own
        # path, rather than along it. None means "face where you are going".
        self.job_facing = None
        # Published back to the job layer: True while a personality state
        # outranks this spider's work, so a base cannot make progress from a
        # worker that is busy fleeing or eating.
        self.job_busy = False
        # Stances declared between teams by the loaded preset. The manager
        # shares one mapping across the scene; empty means teams only imply
        # friendship among their own members.
        self.team_stances: dict = {}
        # Who each team is, shared by the manager the same way. A spider wears a
        # small ring in its team's colour so a scene with two teams looks like a
        # scene with two teams.
        self.team_profiles: dict = {}

        self.size_scale = clamp(float(size_scale), 0.45, 2.25)
        # Keep the old random draw in the seeded stream so removing size
        # variation does not also change each spider's starting position.
        self.rng.random()
        self._progression_base_size = self.BASE_SIZE
        self.size = self._progression_base_size * self.size_scale
        self.x = self.rng.uniform(self.margin, self.screen_w - self.margin)
        self.y = self.rng.uniform(self.margin, self.screen_h - self.margin)
        self.heading = self.rng.uniform(-math.pi, math.pi)
        self.target_heading = self.heading
        self.target_x = self.x
        self.target_y = self.y
        self.speed = 0.0
        self.current_speed = 0.0
        self.vel_x = 0.0
        self.vel_y = 0.0
        self.strafe_observe = False
        self.turn_rate = self.rng.uniform(4.0, 6.0)
        self.state = "Idle"
        self.player_control = None
        # Set by the player's turn-and-walk controls while backing up: walk
        # towards the target but keep facing the other way.
        self.reverse_walk = False
        self.state_timer = rand_range(personality.get("idle_time"), 1.0, 3.0, rng=self.rng)
        self.decision_timer = self.rng.uniform(0.2, 0.5)
        self.motion_paused = False
        self.chase_timer = 0.0
        self.hop_timer = rand_range(personality.get("hop_interval"), 0.25, 0.65, rng=self.rng)
        self.nope_repeats = 0
        self.nope_zigzag_dir = self.rng.choice((-1.0, 1.0))
        self.nope_cooldown = 0.0

        # Direct manipulation / throw physics. The overlay stays click-through,
        # so the manager polls the global mouse button and calls these methods.
        self.dragging = False
        self.grab_offset_x = 0.0
        self.grab_offset_y = 0.0
        self.drag_vel_x = 0.0
        self.drag_vel_y = 0.0
        self.held_leg_relax = 0.0
        self.held_pose_clock = 0.0
        self.held_drag_response = 0.0
        self.held_drag_sway_x = 0.0
        self.held_drag_sway_y = 0.0
        self.held_prev_drag_vx = 0.0
        self.held_prev_drag_vy = 0.0
        self.held_drag_accel_x = 0.0
        self.held_drag_accel_y = 0.0
        self.held_release_timer = 0.0
        self.inertia_vx = 0.0
        self.inertia_vy = 0.0
        self.inertia_timer = 0.0
        # Beat held on the spot after a throw has skidded to a halt.
        self.throw_recovery = 0.0
        self.startled_timer = 0.0
        # Pickup/release uses the pose to communicate a timid reaction. Keep
        # the leg palette unchanged during that transition; color should not
        # flash when the spider is grabbed or set down.
        self._startle_highlight_suppression = 0.0
        self.startle_phase = self.rng.random() * math.tau

        # Drift movement: a deliberate sideways slip layered on top of normal
        # running/retreat/throw motion.  It only becomes active for personalities
        # that opt in (for example the Drifter personality) and when the Drift
        # skill is enabled.
        self.drift_phase = self.rng.random() * math.tau
        self.drift_dir = self.rng.choice((-1.0, 1.0))
        self.drift_boost = 0.0
        self.drift_flip_timer = rand_range(personality.get("drift_switch_time"), 0.55, 1.35, rng=self.rng)
        self.last_drift_amount = 0.0
        self.drift_momentum = 0.0
        self.drift_lean = 0.0
        self.drift_slide_timer = 0.0

        # Autonomous Drifter flourishes.  These are separate from normal wander so
        # the Drifter personality can occasionally tear around the desktop in a
        # committed drift-run: circling in place, carving arcs along screen edges,
        # or skidding around corners.
        self.drift_run_cooldown = rand_range(personality.get("drift_run_interval"), 4.5, 10.5, rng=self.rng)
        self.drift_run_mode = "circle"
        self.drift_run_center_x = self.x
        self.drift_run_center_y = self.y
        self.drift_run_radius = max(self.size * 2.8, 70.0)
        self.drift_run_angle = self.heading
        self.drift_run_dir = self.rng.choice((-1.0, 1.0))
        self.drift_run_turn_rate = 2.8
        self.drift_run_speed = 150.0
        self.drift_run_corner_index = 0
        self.drift_run_phase = "charge"
        self.drift_run_phase_timer = 0.0

        # Separate animation clocks. Leg gait is tied to speed, but body breathing is alive even at rest.
        self.bob_phase = self.rng.random() * math.tau
        self.breath_phase = self.rng.random() * math.tau
        self.body_bob = 0.0
        self.body_sway = 0.0
        self.abdomen_pulse = 0.0
        self.ceph_pulse = 0.0

        self.prev_mx = None
        self.prev_my = None
        self.prev_cursor_vx = 0.0
        self.prev_cursor_vy = 0.0

        # Model palettes are shared discovery data. Copy and sanitize them so a
        # per-slot override cannot mutate the model definition or leak into
        # another creature spawned from the same slot.
        self.colors = {}
        for key, raw in (model.get("colors", {}) or {}).items():
            if isinstance(raw, (list, tuple)) and len(raw) >= 3:
                try:
                    self.colors[str(key)] = [int(clamp(float(raw[i]), 0, 255)) for i in range(3)]
                except (TypeError, ValueError):
                    continue
        # Scene-wide label switches, set by the manager rather than saved per
        # spider: "show every spider's level / health". Declared here rather
        # than discovered by getattr, per DC-10.
        self.force_show_level = False
        self.force_show_health = False
        self.force_show_xp = False
        self.force_show_stamina = False
        # DC-50: nerve. Counts down while this spider is running from a fight;
        # the manager sets it, the behaviour acts on it, and _speed_mult reads
        # it so every state it could be in runs at the same panicked pace.
        self.flee_timer = 0.0
        self.flee_from = None
        # DC-64: how long this spider has been clear of every foe. A retreat
        # ends when it has got away, not only when it has healed -- without
        # this a spider that cannot heal runs into a corner and stays there.
        self.escaped_timer = 0.0
        # Safe, but still hurt: walking home to heal rather than sprinting.
        self.recovering = False
        # Counts down the minimum time a fight is held for; see
        # manager/combat.py::ENGAGEMENT_COMMITMENT.
        self.engagement_timer = 0.0
        self.color_overrides = {}
        if isinstance(color_overrides, dict):
            for key, raw in color_overrides.items():
                if not isinstance(raw, (list, tuple)) or len(raw) != 3:
                    continue
                try:
                    rgb = [int(clamp(float(raw[i]), 0, 255)) for i in range(3)]
                except (TypeError, ValueError):
                    continue
                self.color_overrides[str(key)] = rgb
                self.colors[str(key)] = list(rgb)
        self.legs: List[LegState] = [LegState(definition=dict(item)) for item in model.get("legs", [])]
        for leg in self.legs:
            leg.phase_seed = self.rng.random() * math.tau
            leg.gait_phase_offset = self.rng.uniform(-0.42, 0.42)
        self.gait_groups = sorted({int(leg.definition.get("gait_group", 0)) for leg in self.legs}) or [0]
        self.active_gait_index = self.rng.randrange(len(self.gait_groups))
        self.stepping_group = None
        # Movement style. "classic" keeps the original reactive gait. "lively"
        # drives a clearer alternating-tetrapod cadence, lifts the swinging legs
        # visibly off the ground, and probes with front feelers. "skitter" is
        # based on lively but runs in quick burst-burst-stop successions, like
        # a small jumping spider in a macro video.
        self.gait_style = normalize_gait_style(gait_style)
        # DC-63: the current spell, and how long until it changes.
        #
        # Its own random stream, not `self.rng`. Drawing from the shared one
        # -- even once, in __init__ -- shifts every later value in it, and a
        # seeded run is supposed to replay exactly (DC-09). Caught by
        # `test_tarantula.py::test_a_walk_stays_inside_its_limits`, which
        # started failing a leg-geometry invariant purely because the walk
        # now began from a different phase seed. Same reasoning, and the same
        # fix, as the `_id_rng` split in the manager.
        self._gait_rng = random.Random(f"{self.progression_id}:gait-spell")
        self.gait_spell = None
        self.gait_spell_timer = self._gait_rng.uniform(*GAIT_SPELL_GAP)
        self._lively_gait_phase = self.rng.random()
        self._skitter_burst_timer = self.rng.uniform(0.14, 0.34)
        self._skitter_pause_timer = 0.0
        self._skitter_phase = self.rng.random() * math.tau
        self._skitter_burst_jitter = self.rng.uniform(0.94, 1.16)
        self._prev_heading_gait = self.heading
        # The cursor is an intent signal, not a torque command.  Keep a
        # filtered heading target so a zig-zagging mouse produces one graceful
        # pursuit arc instead of reversing the body every render frame.
        self._spider_heading_filter = self.heading
        self._spider_locomotion_active = False
        self._spider_gait_frame_updated = False
        # Turn speed is ramped independently from the target bearing.  A quick
        # cursor change should still produce a quick pivot, but not a series of
        # full-angle jumps whenever the support set changes.
        self._turn_speed_smooth = 0.0
        # Feeler-probe pacing (lively style only): a gap timer between probes and
        # the current probe pulse envelope.
        self._feeler_clock = self.rng.uniform(0.4, 1.4)
        self._feeler_pulse = 0.0
        self._feeler_pulse_dur = 0.0
        self._feeler_pulse_t = 0.0
        # Per-leg angular territories for the lively gait (built below once the
        # leg list and size are known).  Keeps legs from crossing into a pinwheel.
        self._leg_sectors = {}
        self.idle_twitch_timer = self.rng.uniform(0.6, 1.8)
        self.turn_rehome_pressure = 0.0

        # ------------------------------------------------------------------
        # Emotion, expression and body language
        # ------------------------------------------------------------------
        self.mood_mode = str(personality.get("mood", "auto")).lower()
        self.mood = Mood()
        self.mood.set_baseline_named(self.mood_mode if self.mood_mode != "auto" else personality.get("id", "auto"))
        # Slight per-individual emotional temperament so a group is not uniform.
        self.mood.bump(
            valence=self.rng.uniform(-0.12, 0.12),
            arousal=self.rng.uniform(-0.10, 0.10),
            affection=self.rng.uniform(-0.10, 0.10),
            curiosity=self.rng.uniform(-0.10, 0.10),
        )
        self.expression_blink = 0.0
        self.blink_timer = self.rng.uniform(1.5, 5.0)
        self.head_tilt = 0.0            # body-local cosmetic head roll
        self.look_fwd = 1.0             # pupil gaze direction in body-local forward
        self.look_side = 0.0

        # Posture channels that move the body shell without sliding planted feet.
        self.body_wiggle = 0.0          # whole-body wag (radians), legs stay put
        self.abdomen_wag = 0.0          # extra abdomen sway (radians)
        self.crouch = 0.0               # 0 upright .. 1 coiled/low (jump prep, play bow)
        self.rear = 0.0                 # 0 flat .. 1 reared front (alert/excited)
        # DC-59: the fighting pose. `combat_stance` is 0..1, how squared-up
        # this spider is at a foe; `lunge` is a signed 0..1 impulse along the
        # line to that foe -- positive for a blow thrown, negative for one
        # taken. Both are pure animation: nothing here changes a number that
        # decides a fight.
        # DC-62: how wary of the pointer this spider has become.
        self.cursor_pressure = 0.0
        self.combat_stance = 0.0
        self.lunge = 0.0
        # The bite animation (see ExpressionMixin.begin_strike): seconds into
        # it, or None, and the unit direction it goes.
        self.strike_clock = None
        self.strike_face = (1.0, 0.0)
        self.combat_face_x = 0.0
        self.combat_face_y = 0.0
        self.squash = 1.0               # landing squash-and-stretch (1 = neutral)
        self.wiggle_phase = self.rng.random() * math.tau
        self.wiggle_amp = 0.0           # target amplitude driven by mood/intent
        self.wiggle_burst = 0.0         # transient extra wiggle (aim waggle, excitement)

        # Vertical hop fakery for a top-down view.
        self.jump_z = 0.0               # >0 == airborne height in pixels
        self.airborne = False
        self.jump_t = 0.0
        self.jump_duration = 0.0
        self.jump_peak = 0.0
        self.jump_from = (self.x, self.y)

        # True from the moment a tumble begins until something has tidied up
        # after it, whether that was the roll finishing or anything cutting it
        # short.
        self._rolling = False
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
        self.antenna_phase = [self.rng.random() * math.tau, self.rng.random() * math.tau]
        # Some models use these as sensory front legs rather than thin cartoon
        # antennae.  Each side keeps its own per-segment pose so proximal lift,
        # knee flexion, and distal probing can travel through the chain in order.
        self.antenna_segment_angles: List[List[float]] = [[], []]
        self.antenna_joint_lifts: List[List[float]] = [[], []]
        self.antenna_extension = [0.0, 0.0]
        self.antenna_hand_targets: List[Tuple[float, float]] = [(0.0, 0.0), (0.0, 0.0)]
        self.antenna_hand_grips = [0.0, 0.0]
        self.aim_intent = 0.0           # smoothed 0..1 hunting-aim weight
        self.inspect_intent = 0.0       # smoothed 0..1 inspecting weight
        self.cuddle_intent = 0.0        # smoothed 0..1 cuddling weight

        # Social play with other creatures.
        self.neighbors: List["Creature"] = []
        self.social_target: "Creature" | None = None
        self.social_role = "none"       # "chase" / "flee" / "play"
        self.social_cooldown = self.rng.uniform(1.5, 5.0)
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
        self._desktop_seek_timer = rand_range(personality.get("desktop_hide_seek_interval"), 28.0, 64.0, rng=self.rng)
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
        self._folder_portal_cooldown = rand_range(personality.get("folder_portal_cooldown"), 24.0, 55.0, rng=self.rng)
        self._folder_portal_dwell = 0.0
        self._folder_portal_threshold = rand_range(personality.get("folder_portal_dwell"), 0.85, 1.39, rng=self.rng)

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
        self._camouflage_visible_timer = rand_range(personality.get("camouflage_initial_visible_time"), 3.5, 6.0, rng=self.rng)

        # Gameplay stats are derived values.  The behaviour code can continue to
        # assign ordinary movement speeds while ``_speed_mult`` folds progression
        # and equipment bonuses into those assignments.
        self.max_hp = 100.0
        self.hp = self.max_hp
        self.max_energy = 100.0
        self.energy = self.max_energy
        self.armor = 0.0
        self.damage = 8.0
        # DC-22/DC-47: conflict state. A spider that loses a fight dies --
        # it stops being part of the scene and leaves a carcass, which is
        # eaten and gone shortly after. The manager owns removing it; this
        # flag is what it reads, and what keeps a dead spider from being
        # hunted, guarded against or played with in the frame before that.
        self.dead = False
        self.last_attacker = None
        # DC-45: silk that another spider has pinned this one with. It slows
        # a fight down and makes the pinned spider easier to land a hit on,
        # which is what makes trapping worth a web-shooter's time.
        self.webbed_timer = 0.0
        self.webbed_by = None
        # Which foe this spider is currently fighting, published by the
        # manager the same way `_prey` is for a fly.
        self._foe = None
        # Fades after a hit, so the render layer can show one without needing
        # to know anything about combat.
        self.hurt_flash = 0.0
        self.attack_cooldown = 0.0
        combat_cfg = model.get("combat", {})
        if not isinstance(combat_cfg, dict):
            combat_cfg = {}
        def combat_number(key: str, default: float, minimum: float) -> float:
            try:
                return max(minimum, float(combat_cfg.get(key, default)))
            except (TypeError, ValueError):
                return default
        self._combat_base_hp = combat_number("base_hp", 100.0, 1.0)
        self._combat_base_energy = combat_number("base_energy", 100.0, 1.0)
        self._combat_base_armor = combat_number("base_armor", 0.0, 0.0)
        self._combat_base_damage = combat_number("base_damage", 8.0, 0.1)
        self._apply_progression_stats(reset_resources=True)

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
        weave_bias = 4.0 if self._acts_as_webber() else 1.0
        # Webbers come off cooldown quickly and often; everyone else rarely.
        self.weave_cooldown = rand_range(personality.get("weave_cooldown"),
                                         6.0, 14.0, rng=self.rng) / weave_bias
        self.web_walk_cooldown = rand_range(personality.get("web_walk_cooldown"), 8.0, 20.0, rng=self.rng)
        self._weave_speed = rand_range(personality.get("weave_speed"), 250.0, 330.0, rng=self.rng)

        # Cursor-trapping silk. ``mouse_web_world`` is the shared world object set
        # by the manager after construction. The spider shoots a glob of sticky
        # silk at the real pointer that pins it (trap) or shoves it to a wall.
        self.mouse_web_world = None
        self._web_shot_kind = "trap"      # which shot the current aim will fire
        shot_bias = 5.0 if self._acts_as_web_shooter() else 1.0
        self.web_shot_cooldown = rand_range(personality.get("web_shot_cooldown"),
                                            8.0, 18.0, rng=self.rng) / shot_bias

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
        self._pounce_cooldown = self.rng.uniform(0.0, 0.7)
        self._trap_shot_cooldown = self.rng.uniform(0.4, 1.6)
        # A short intense bout of working caught prey; set for real by
        # enter_feed, which runs long before the Feed state ever reads them.
        self._feed_frenzy = 0.0
        self._feed_anchor = (self.x, self.y)
        self._feed_paw = 0.0
        # Set for real on the first locomotion tick and the first gait update;
        # declared here so the state of a creature is visible in one place (C4)
        # rather than only appearing once those first ticks have run.
        self._spider_solver_dt = 0.016
        self._spider_turn_speed = 0.0
        # A web-walk anchor point on whichever web is currently being walked;
        # None until _begin_web_walk (or similar) sets a real one, since there
        # is no web to derive a default point from until then.
        self._web_walk_point = None
        # The fly world (set by the manager) plus the fly a web shot is aimed at,
        # so the spider can fling its trapping silk at prey, not just the cursor.
        self.fly_world = None
        # DC-50: read only to find the way home when fleeing. DC-17 noted that
        # Creature never read base_world at all; a hurt spider running for the
        # one place that heals it is the first thing that needs to.
        self.base_world = None
        self._web_shot_prey = None
        self._web_shot_foe = None

        # A read-only snapshot of what this spider can currently query about the
        # shared world (DC-17); rebuilt at the top of every update() tick so
        # decision code reads it instead of self.web_world/self.fly_world
        # directly. None only before the first tick.
        self.perception = None

        self._compute_leg_sectors()
        self._initialize_legs()

    def _speed_mult(self) -> float:
        # DC-45: silk slows whatever it lands on. Applied here rather than at
        # each assignment so every state a pinned spider could be in -- fleeing,
        # chasing, working a job -- is slowed by it without each one
        # remembering to ask.
        # DC-50: the first WEBBED_HOLD_SECONDS of a pin hold the spider in
        # place rather than merely slowing it, which is what the owner
        # expected from watching silk land on the pointer. After that it
        # works free and is only slowed for the remainder.
        if self.webbed_held:
            webbed = 0.0
        elif self.webbed:
            webbed = WEBBED_SPEED_MULT
        else:
            webbed = 1.0
        fleeing = FLEE_SPEED_MULT if self.fleeing else 1.0
        return float(self.personality.get("speed_multiplier", 1.0)) * float(
            self._progression_speed_multiplier
        ) * webbed * fleeing

    @property
    def level(self) -> int:
        return int(self.progression.level)

    @property
    def xp(self) -> int:
        return int(self.progression.xp)

    def _progression_effect(self, key: str) -> float:
        value = 0.0
        for ability_id in self.progression.unlocked_abilities:
            node = ABILITY_BY_ID.get(ability_id)
            if node is not None:
                value += float(node.effects.get(key, 0.0))
        for item in equipped_items(self.progression):
            value += float(getattr(item, key, 0.0))
        return value

    def _apply_progression_stats(self, reset_resources: bool = False,
                                 carry_wounds: bool = False) -> None:
        """Recompute level and equipment-derived stats without changing pose.

        Three ways to treat hp and energy when the ceiling moves:

        * ``reset_resources`` -- refill. Used when a spider is being built or
          rebuilt and has no history worth keeping.
        * ``carry_wounds`` -- the ceiling rises and the damage already taken
          stays taken (DC-66). This is what a level-up does.
        * neither -- keep the same *fraction*, which is right for a change of
          equipment: the spider is not hurt any differently, its pool simply
          changed shape.

        The middle one exists because of DC-66. A level-up used to refill,
        which was harmless while eating a fly was the only way to earn XP --
        no spider ever levelled mid-fight. Now that damage earns XP, a refill
        would mean a hard fight heals both sides: measured immediately, two
        foes brawling for three seconds both ended *above* the hp they
        started with (115.2 and 123.0 against 100.0), and neither could ever
        have been finished.
        """
        size_mult, growth_speed = growth_multipliers(self.progression.level)
        self._progression_size_multiplier = size_mult
        self._progression_speed_multiplier = clamp(
            growth_speed + self._progression_effect("speed"), 0.72, 2.40
        )
        level = self.progression.level
        old_max_hp = float(getattr(self, "max_hp", 100.0))
        old_max_energy = float(getattr(self, "max_energy", 100.0))
        self.max_hp = max(1.0, self._combat_base_hp + (level - 1) * 5.0 + self._progression_effect("max_hp"))
        self.max_energy = max(1.0, self._combat_base_energy + (level - 1) * 3.0 + self._progression_effect("max_energy"))
        self.armor = max(0.0, self._combat_base_armor + (level - 1) * 0.18 + self._progression_effect("armor"))
        self.damage = max(0.1, self._combat_base_damage + (level - 1) * 0.35 + self._progression_effect("damage"))
        self.size = self._progression_base_size * self.size_scale * size_mult
        if reset_resources:
            self.hp = self.max_hp
            self.energy = self.max_energy
        elif carry_wounds:
            self.hp = clamp(float(getattr(self, "hp", old_max_hp)) + (self.max_hp - old_max_hp),
                            0.0, self.max_hp)
            self.energy = clamp(
                float(getattr(self, "energy", old_max_energy)) + (self.max_energy - old_max_energy),
                0.0, self.max_energy)
        else:
            hp_ratio = float(getattr(self, "hp", old_max_hp)) / max(1.0, old_max_hp)
            energy_ratio = float(getattr(self, "energy", old_max_energy)) / max(1.0, old_max_energy)
            self.hp = clamp(hp_ratio * self.max_hp, 0.0, self.max_hp)
            self.energy = clamp(energy_ratio * self.max_energy, 0.0, self.max_energy)

    def gain_experience(self, amount: int | float, reason: str = "") -> list[str]:
        """Award XP and return human-readable level-up events.

        XP is stored as progress toward the next level, while ``total_xp`` keeps
        the lifetime amount for future profile/stat screens.  The level cap is a
        hard bound; excess XP is retained only up to the visible cap bar.
        """
        try:
            amount = max(0, int(round(float(amount))))
        except (TypeError, ValueError):
            amount = 0
        if amount <= 0:
            return []
        self.progression.total_xp += amount
        if self.progression.level >= MAX_LEVEL:
            self.progression.xp = xp_to_next_level(MAX_LEVEL)
            return []
        self.progression.xp += amount
        events = []
        while self.progression.level < MAX_LEVEL:
            threshold = xp_to_next_level(self.progression.level)
            if self.progression.xp < threshold:
                break
            self.progression.xp -= threshold
            self.progression.level += 1
            self.progression.skill_points += 1
            self._apply_progression_stats(carry_wounds=True)
            events.append(f"reached level {self.progression.level}")
            if self.player_control is None:
                events.extend(self._spend_skill_points())
        if self.progression.level >= MAX_LEVEL:
            # One large award can carry a remainder past the last threshold.
            # Leaving it unclamped overfills the inspector's XP bar.
            self.progression.xp = xp_to_next_level(MAX_LEVEL)
        return events

    def _spend_skill_points(self) -> list[str]:
        """Unlock whatever this level just made available (DC-57).

        The owner: *"and the skills unlocks as they level up."* The tree, the
        level gates and the prerequisites all already existed; what did not
        was anyone to spend the points. A skill point was awarded on every
        level and then sat there unless a person opened the inspector and
        clicked, which no spider in a colony of five was ever going to get.

        Cheapest-first by level requirement, then in tree order, so a spider
        walks up the tree the way its author laid it out rather than taking
        whichever node a set happened to yield first. `can_unlock` is the same
        gate the inspector uses, so the two cannot drift.
        """
        unlocked = []
        while self.progression.skill_points > 0:
            available = [node for node in ABILITY_TREE
                         if self.progression.can_unlock(node.id)]
            if not available:
                break
            node = min(available, key=lambda n: (n.level_required, ABILITY_TREE.index(n)))
            self.progression.skill_points -= node.cost
            self.progression.unlocked_abilities.append(node.id)
            unlocked.append(f"learned {node.name}")
        if unlocked:
            # Wounds carry here too. These unlocks happen *inside* a level-up
            # (DC-57 spends the point the level just awarded), so applying the
            # fraction-preserving mode would hand back most of what the
            # level-up had just been made to keep: a spider on 60 of 100 came
            # out of one level on 111 hp.
            self._apply_progression_stats(carry_wounds=True)
        return unlocked

    def unlock_progression_ability(self, ability_id: str) -> tuple[bool, str]:
        ability_id = str(ability_id).strip().lower()
        node = ABILITY_BY_ID.get(ability_id)
        if node is None:
            return False, "Unknown progression ability."
        if ability_id in self.progression.unlocked_abilities:
            return False, f"{node.name} is already unlocked."
        if self.progression.level < node.level_required:
            return False, f"{node.name} unlocks at level {node.level_required}."
        if self.progression.skill_points < node.cost:
            return False, "This spider has no skill points available."
        missing = [req for req in node.prerequisites if req not in self.progression.unlocked_abilities]
        if missing:
            return False, "Unlock the prerequisite ability first."
        self.progression.skill_points -= node.cost
        self.progression.unlocked_abilities.append(ability_id)
        self._apply_progression_stats()
        return True, f"Unlocked {node.name}."

    def equip_item(self, item_id: str) -> tuple[bool, str]:
        item = ARMOR_BY_ID.get(str(item_id).strip().lower())
        if item is None:
            return False, "Unknown armor item."
        if item.id not in self.progression.inventory:
            return False, "That item is not in this spider's inventory."
        if not (item.size_min <= self.size_scale <= item.size_max):
            return False, "That armor does not fit this spider's size."
        self.progression.equip(item.id)
        self._apply_progression_stats()
        return True, f"Equipped {item.name}."

    def unequip_item(self, slot: str) -> tuple[bool, str]:
        if not self.progression.unequip(slot):
            return False, "Nothing is equipped in that anatomy slot."
        self._apply_progression_stats()
        return True, f"Unequipped {str(slot).replace('_', ' ')} armor."

    def add_inventory_item(self, item_id: str) -> bool:
        return self.progression.add_item(str(item_id).strip().lower())

    def take_damage(self, amount: float, source=None) -> float:
        """Apply a hit against this spider's armour, returning what landed.

        This and ``heal`` have existed since the progression work, but until
        DC-22 nothing in the running application called them -- only a test
        did. Conflict is what finally uses them, and what made a spider
        running out of hp mean something: it is knocked out, never killed.
        Whether a hit may be dealt at all is the manager's decision, gated on
        the conflict setting; this method does not police its callers.
        """
        if self.dead:
            return 0.0
        try:
            incoming = max(0.0, float(amount))
        except (TypeError, ValueError):
            return 0.0
        dealt = max(0.1, incoming - self.armor) if incoming > 0.0 else 0.0
        self.hp = clamp(self.hp - dealt, 0.0, self.max_hp)
        if dealt > 0.0:
            self.last_attacker = source
            self.hurt_flash = 1.0
        if self.hp <= 0.0:
            self._die()
        return dealt

    @property
    def webbed(self) -> bool:
        """Whether this spider is currently pinned by another's silk."""
        return self.webbed_timer > 0.0

    @property
    def webbed_held(self) -> bool:
        """Whether the silk is still holding it outright, not just slowing it.

        The hold is the front of the pin, so a spider works free gradually:
        stuck fast, then labouring, then loose.
        """
        return self.webbed_timer > (WEBBED_SECONDS - WEBBED_HOLD_SECONDS)

    @property
    def fleeing(self) -> bool:
        """Whether this spider has broken off and is running (DC-50)."""
        return self.flee_timer > 0.0

    def web_pinned(self, kind: str, by=None) -> None:
        """Take a hit of trapping silk from another spider (DC-45)."""
        if self.dead:
            return
        self.webbed_timer = max(self.webbed_timer, WEBBED_SECONDS)
        self.webbed_by = by
        self.mood.bump(arousal=0.3, valence=-0.25)

    def _die(self) -> None:
        """Mark this spider beaten. The manager removes it and leaves remains.

        Nothing is torn down here: the creature object stays intact for the
        rest of the frame so whatever is mid-iteration over the colony does
        not trip over it, and `CreatureManager` clears it out at the end of
        the tick.
        """
        self.dead = True
        self.hp = 0.0
        self.state = "Downed"
        self.motion_paused = True
        self.speed = 0.0
        self.current_speed = 0.0
        self.job_mode = "idle"
        self.job_target = None
        self.job_alert_target = None
        self.job_facing = None
        self._prey = None
        self._hunting_prey = False

    def heal(self, amount: float) -> float:
        before = self.hp
        try:
            self.hp = clamp(self.hp + max(0.0, float(amount)), 0.0, self.max_hp)
        except (TypeError, ValueError):
            return 0.0
        return self.hp - before

    def spend_energy(self, amount: float) -> bool:
        try:
            cost = max(0.0, float(amount))
        except (TypeError, ValueError):
            return False
        if cost > self.energy:
            return False
        self.energy -= cost
        return True

    def _regenerate_energy(self, dt: float) -> None:
        if self.energy >= self.max_energy or self.dragging:
            return
        regen = 8.0 + self._progression_effect("energy_regen")
        self.energy = clamp(self.energy + max(0.0, float(dt)) * regen, 0.0, self.max_energy)

    def set_team(self, team_id: str) -> None:
        self.progression.team_id = normalize_team_id(team_id)

    def set_job(self, job_id: str) -> None:
        """Change the profession without changing temperament or abilities."""
        self.job_id = normalize_job_id(job_id)
        self.job_mode = "idle"
        self.job_target = None
        self.job_alert_target = None
        self.job_base_id = None
        self.job_facing = None

    def job_label(self) -> str:
        from ..world.jobs import job_definition
        return job_definition(self.job_id).display_name

    def relation_to(self, other: "Creature") -> str:
        if other is None:
            return "neutral"
        # ``team_stances`` comes from the loaded preset and is shared by every
        # spider in the scene; a per-pair choice made in the inspector still
        # wins over it.
        return relation_between(
            self.progression,
            other.progression,
            str(getattr(other, "progression_id", other.index)),
            getattr(self, "team_stances", None),
        )

    def progression_snapshot(self) -> dict:
        data = self.progression.to_dict()
        data.update({
            "job": self.job_id,
            "level": self.level,
            "xp_to_next": xp_to_next_level(self.level),
            "max_hp": round(self.max_hp, 2),
            "hp": round(self.hp, 2),
            "max_energy": round(self.max_energy, 2),
            "energy": round(self.energy, 2),
            "armor": round(self.armor, 2),
            "damage": round(self.damage, 2),
        })
        return data

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

    def set_size_scale(self, size_scale: float) -> None:
        """Scale the creature and rehome its legs for clean live size changes."""
        new_scale = clamp(float(size_scale), 0.45, 2.25)
        if abs(new_scale - self.size_scale) < 1e-4:
            return
        self.size_scale = new_scale
        self._apply_progression_stats()
        self.x, self.y = clamp_point(self.x, self.y, self.margin * 0.4, self.screen_w, self.screen_h)
        self.target_x, self.target_y = clamp_point(self.target_x, self.target_y, self.margin * 0.4, self.screen_w, self.screen_h)
        self._initialize_legs()
        self.startled_timer = max(self.startled_timer, 0.45)

    @property
    def wary_of_cursor(self) -> bool:
        """Has this spider been caught often enough to mind the pointer?

        A threshold rather than a gradient, because the behaviour model it
        feeds is a table of per-situation multipliers: a wary spider is in a
        different situation, not in the same one by a smaller amount.
        """
        return self.cursor_pressure >= CURSOR_WARY_THRESHOLD

    def start_drag(self, mx: float, my: float) -> None:
        # DC-62: remember it. Bumped here rather than on release, so a grab
        # counts even if the spider is put straight back down.
        self.cursor_pressure = min(1.0, self.cursor_pressure + CURSOR_PRESSURE_PER_GRAB)
        self.dragging = True
        self.phase_scheduler.cancel()
        self.state = "Dragged"
        self.motion_paused = True
        self.speed = 0.0
        self.current_speed = 0.0
        self.inertia_vx = 0.0
        self.inertia_vy = 0.0
        self.inertia_timer = 0.0
        self.startled_timer = 1.2
        self._startle_highlight_suppression = 1.35
        self.held_leg_relax = 0.0
        self.held_pose_clock = 0.0
        self.held_drag_response = 0.0
        self.held_drag_sway_x = 0.0
        self.held_drag_sway_y = 0.0
        self.held_prev_drag_vx = 0.0
        self.held_prev_drag_vy = 0.0
        self.held_drag_accel_x = 0.0
        self.held_drag_accel_y = 0.0
        self.held_release_timer = 0.0
        self.grab_offset_x = self.x - mx
        self.grab_offset_y = self.y - my
        self.drag_vel_x = 0.0
        self.drag_vel_y = 0.0
        self.state_timer = 0.25
        self.register_camouflage_touch()
        # A carried spider is not walking. Freeze the gait at pickup so a leg
        # that happened to be mid-step cannot keep twitching beneath the body;
        # release_drag will rehome the feet before grounded locomotion resumes.
        for leg in self.legs:
            leg.stepping = False
            leg.pending_step = False
            leg.lift = 0.0
            leg.held_spring_x = 0.0
            leg.held_spring_y = 0.0
            leg.held_spring_vx = 0.0
            leg.held_spring_vy = 0.0
            leg.contact_state = "air"
            leg.support_weight = 0.0
            leg.contact_age = 0.0
            leg.step_cooldown = 0.0

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
            if self._acts_as_drifter():
                self._prime_drift(clamp(drag_speed / 700.0, 0.25, 1.15))
                self.target_heading += self.drift_dir * clamp(drag_speed / 900.0, 0.0, 0.56)
        self.heading = angle_lerp(self.heading, self.target_heading, self.turn_rate * 2.2 * dt)
        # If it has locked onto a fly while being carried, it turns to face the
        # prey (and, if it can, will web it from the hand via the hunt driver).
        prey = self._prey
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
        if drag_speed > 45.0 and self.rng.random() < 0.82:
            self._panic_rehome_legs(clamp(drag_speed / 800.0, 0.35, 1.0))

    def release_drag(self, mx: float, my: float) -> None:
        if not self.dragging:
            return
        self.dragging = False
        # The first paint after release must not expose a carried contact that
        # is still outside the grounded chain envelope. Keep a short safety
        # window while the gait scheduler starts its visible replant steps.
        self.held_release_timer = 0.24
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
        # Keep the normal startle response (eyes/body) but suppress the leg
        # highlight for the entire pickup/release transition.
        self._startle_highlight_suppression = max(
            self._startle_highlight_suppression, 1.35
        )
        for leg in self.legs:
            self._soft_limit_leg_state(leg, blend=0.45)
        self._panic_rehome_legs(clamp(speed / 850.0, 0.55, 1.0))

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
        self.hurt_flash = max(0.0, self.hurt_flash - dt * 1.6)
        self.attack_cooldown = max(0.0, self.attack_cooldown - dt)
        self.webbed_timer = max(0.0, self.webbed_timer - dt)
        self.flee_timer = max(0.0, self.flee_timer - dt)
        if self.flee_timer <= 0.0:
            self.flee_from = None
        if not self.webbed:
            self.webbed_by = None
        if self.dead:
            # Beaten. Nothing decides anything from here; the manager sweeps
            # it out of the colony at the end of this tick and leaves a
            # carcass where it fell.
            self.motion_paused = True
            self.current_speed = 0.0
            return
        if self.player_control is not None:
            self._regenerate_energy(dt)
            self.player_control.update(dt)
            return
        self.perception = build_perception(self)
        if self._hunting_prey:
            # A spider locked onto a fly reacts to that candidate here, before
            # the dragging branch below, so it can still fire silk at prey
            # while held (DC-18, C1 -- see BehaviourMixin._pursue_prey).
            self._pursue_prey(dt, mx, my)
        elif self._foe is not None:
            # DC-45: a fight is execution of a standing commitment, like a
            # hunt, so it runs every frame here rather than through the
            # arbiter's throttle. Hunting wins the tick when both are live:
            # a fly is food and a foe will still be there in a moment.
            self._pursue_foe(dt)

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
        self._feed_cooldown = max(0.0, self._feed_cooldown - dt)
        self._pounce_cooldown = max(0.0, self._pounce_cooldown - dt)
        self._trap_shot_cooldown = max(0.0, self._trap_shot_cooldown - dt)
        self._regenerate_energy(dt)
        if not self.dragging:
            self.phase_scheduler.tick(dt)

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
        self._reconcile_roll()

        # Hidden spiders are behind a real desktop window, so they are no longer
        # in the same visible interaction layer as the cursor or other spiders.
        if self._desktop_fully_hidden:
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
            self.held_leg_relax = min(1.0, self.held_leg_relax + dt * 4.5)
            # A carried spider is quiet, but not a frozen sticker. Feed the
            # smoothed hand velocity into a heavily damped pose response: fast
            # drags create a small shared lag and slow settling bounce, while a
            # stopped cursor lets the legs become still again without tremble.
            response_target = clamp(math.hypot(self.drag_vel_x, self.drag_vel_y) / 420.0, 0.0, 1.0)
            response_blend = 1.0 - math.exp(-dt * 7.0)
            self.held_drag_response += (response_target - self.held_drag_response) * response_blend
            self.held_pose_clock += dt * (1.15 + self.held_drag_response * 2.35)
            drag_accel_x = (self.drag_vel_x - self.held_prev_drag_vx) / dt
            drag_accel_y = (self.drag_vel_y - self.held_prev_drag_vy) / dt
            self.held_prev_drag_vx = self.drag_vel_x
            self.held_prev_drag_vy = self.drag_vel_y
            accel_blend = 1.0 - math.exp(-dt * 10.0)
            self.held_drag_accel_x += (drag_accel_x - self.held_drag_accel_x) * accel_blend
            self.held_drag_accel_y += (drag_accel_y - self.held_drag_accel_y) * accel_blend
            sway_target_x = clamp(self.drag_vel_x / 520.0, -1.0, 1.0) * self.size * 0.15
            sway_target_y = clamp(self.drag_vel_y / 520.0, -1.0, 1.0) * self.size * 0.07
            sway_blend = 1.0 - math.exp(-dt * 8.0)
            self.held_drag_sway_x += (sway_target_x - self.held_drag_sway_x) * sway_blend
            self.held_drag_sway_y += (sway_target_y - self.held_drag_sway_y) * sway_blend
            self._update_held_leg_springs(dt)
            # Being held is a quiet, timid pose: preserve gentle breathing but
            # remove the high-frequency bob/sway that previously looked like a
            # frightened tremor and made the legs shake against their roots.
            self.bob_phase += dt * 1.2
            self.breath_phase += dt * (1.45 + speed01 * 0.20)
            self.body_bob = 0.0
            self.body_sway = 0.0
            self.abdomen_pulse = math.sin(self.breath_phase * 1.05) * 0.032
            self.ceph_pulse = -math.sin(self.breath_phase * 0.92 + 0.85) * 0.016
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
        self.held_release_timer = max(0.0, self.held_release_timer - dt)
        self._startle_highlight_suppression = max(
            0.0, self._startle_highlight_suppression - dt
        )

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
            self._aim_at_a_real_screen()
            if self._spider_grounded_mode_allowed():
                self._update_spider_grounded_frame(dt)
            else:
                if self._spider_locomotion_active:
                    self._spider_locomotion_active = False
                self._move_body(dt)
        # Again after the state machine, and outside the airborne branch,
        # because the roll may have been left during this very frame -- by the
        # spider jumping out of it, among other things -- and the frame is
        # about to be drawn.
        self._reconcile_roll()
        if self.cage is not None and not self.dragging:
            self._apply_cage_bounds()
        self._update_legs(dt)
        self._update_mood(dt)
        self._update_posture(dt)
        self._update_antennae(dt)
        self._sync_scheduled_phase(self._phase_focus_context(mx, my))

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

    def set_name(self, name: str) -> None:
        self.name = (name or "").strip()

    @property
    def display_name(self) -> str:
        return self.name or str(self.model.get("display_name", "Spider"))

    @property
    def level_label_pinned(self) -> bool:
        # Either this spider was pinned individually, or the scene has been
        # asked to show every spider's level. The per-spider pin survives the
        # global switch being turned off again, because it is stored on the
        # progression and this only reads alongside it.
        return bool(self.progression.pin_level or self.force_show_level)

    def set_level_label_pinned(self, enabled: bool) -> None:
        self.progression.pin_level = bool(enabled)

    @property
    def health_label_pinned(self) -> bool:
        return bool(self.progression.pin_health or self.force_show_health)

    def set_health_label_pinned(self, enabled: bool) -> None:
        self.progression.pin_health = bool(enabled)

    def health_fraction(self) -> float:
        return clamp(float(self.hp) / max(1.0, float(self.max_hp)), 0.0, 1.0)

    @property
    def xp_label_pinned(self) -> bool:
        return bool(self.force_show_xp)

    @property
    def stamina_label_pinned(self) -> bool:
        return bool(self.force_show_stamina)

    def stamina_fraction(self) -> float:
        return clamp(float(self.energy) / max(1.0, float(self.max_energy)), 0.0, 1.0)

    def xp_fraction(self) -> float:
        """Progress toward the next level; full at the level cap."""
        if self.level >= MAX_LEVEL:
            return 1.0
        return clamp(float(self.xp) / max(1.0, float(xp_to_next_level(self.level))), 0.0, 1.0)

    def _label_font(self):
        font = QFont("Segoe UI")
        font.setPointSizeF(10.0 + (self.level - 1) * 0.1)
        font.setWeight(QFont.DemiBold)
        return font

    def label_visible(self, always_show: bool) -> bool:
        """Whether the label above this spider is drawn at all.

        "Always show names" used to mean "always show the names of the
        spiders that have one", because of an `and bool(self.name)` that sat
        in front of everything else: a colony nobody had named by hand
        answered the switch by showing nothing. It shows `display_name`
        instead, which falls back to the model's own name, so the switch now
        does what its label says. Hover is unchanged -- an unnamed spider
        still says nothing when the pointer passes over it, because that is a
        label appearing under the cursor rather than one the owner asked for.
        """
        pinned = (self.level_label_pinned or self.health_label_pinned or self.xp_label_pinned
                  or self.stamina_label_pinned)
        if always_show or pinned:
            return True
        return bool(self.name) and self._hovered

    def _label_border_color(self, QColor):
        """The label is edged in the team colour -- white when there is no team.

        DC-51 removed the coloured ring that used to be painted on the ground
        under every spider on a team. The owner asked for it to go ("remove
        that under the spider circle, it should not be visible") and, in the
        same breath, for teams to be shown "just by the color. no need for
        titles. white would be neutral". This border is where that colour
        lives now, so it is drawn at full strength rather than as a hint, and
        a spider on no team gets white rather than a faint grey.
        """
        team_id = str(getattr(self.progression, "team_id", "neutral") or "neutral")
        if team_id.strip().lower() in ("", "neutral"):
            return QColor(255, 255, 255, 215)
        from ..state.teams import team_color

        red, green, blue = team_color(team_id, getattr(self, "team_profiles", None))
        return QColor(red, green, blue, 235)

    def _label_text(self) -> str:
        return self.display_name + self._level_suffix()

    def _level_suffix(self) -> str:
        """"  ·  Lv N", shown for the level switch or for XP, which sits under it."""
        if self.level_label_pinned or self.xp_label_pinned:
            return f"  ·  Lv {self.level}"
        return ""

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

            fm = QFontMetrics(self._label_font())
            text = self._label_text()
            half_w = max(fm.horizontalAdvance(text) * 0.5 + 12.0,
                         self.LABEL_BAR_WIDTH * 0.5 + 2.0)
            label_h = fm.height() + 12.0 + self._label_bars_height()
            min_x = min(min_x, self.x - half_w)
            max_x = max(max_x, self.x + half_w)
            # Label floats above the highest drawn point.
            min_y -= label_h + 6.0

        self._bbox = (min_x, min_y, max_x, max_y)
        return self._bbox

    def _health_bar_height(self) -> float:
        return max(4.0, min(7.0, self.size * 0.20))

    def _xp_bar_height(self) -> float:
        return max(3.0, self._health_bar_height() - 1.0)

    def _label_bars_height(self) -> float:
        """Height the bars under the label add, gaps included."""
        height = 0.0
        if self.health_label_pinned:
            height += self._health_bar_height() + 3.0
        if self.stamina_label_pinned:
            height += self._xp_bar_height() + 2.0
        return height

    @staticmethod
    def health_bar_color(fraction: float, QColor):
        """Green, amber, red. Colour alone is the reading at spider size."""
        fraction = clamp(float(fraction), 0.0, 1.0)
        if fraction > 0.6:
            return QColor(104, 194, 108, 235)
        if fraction > 0.3:
            return QColor(226, 176, 74, 235)
        return QColor(214, 84, 76, 235)

    def _draw_health_bar(self, painter, left: float, top: float, width: float,
                         QRectF, Qt, QColor, QBrush, QPen) -> None:
        fraction = self.health_fraction()
        self._draw_label_bar(painter, left, top, width, self._health_bar_height(),
                             fraction, self.health_bar_color(fraction, QColor),
                             QRectF, Qt, QColor, QBrush, QPen)

    def _draw_label_bar(self, painter, left: float, top: float, width: float,
                        height: float, fraction: float, fill,
                        QRectF, Qt, QColor, QBrush, QPen) -> None:
        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(QColor(18, 18, 22, 215)))
        painter.drawRoundedRect(QRectF(left, top, width, height), 2.0, 2.0)
        if fraction > 0.0:
            painter.setBrush(QBrush(fill))
            painter.drawRoundedRect(
                QRectF(left + 1.0, top + 1.0, max(1.0, (width - 2.0) * fraction),
                       height - 2.0), 1.5, 1.5)
        painter.setBrush(Qt.NoBrush)
        painter.setPen(QPen(QColor(255, 255, 255, 70), 1.0))
        painter.drawRoundedRect(QRectF(left, top, width, height), 2.0, 2.0)

    def _draw_name_label(self, painter, always_show_names: bool) -> None:
        if not self.label_visible(always_show_names):
            return

        font = self._label_font()
        painter.setFont(font)
        fm = QFontMetrics(font)
        text = self._label_text()
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
        box_y = top_extent - box_h - 4.0 - self._label_bars_height()

        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(QColor(18, 18, 22, 205)))
        painter.drawRoundedRect(QRectF(box_x, box_y, box_w, box_h), 6.0, 6.0)
        painter.setBrush(Qt.NoBrush)
        # Two pixels, not one: this border is the only place a team's colour
        # is shown on a spider now, so it has to survive being looked at over
        # a busy desktop.
        painter.setPen(QPen(self._label_border_color(QColor), 2.0))
        painter.drawRoundedRect(QRectF(box_x, box_y, box_w, box_h), 6.0, 6.0)
        painter.setPen(QPen(QColor(245, 247, 250, 255)))
        painter.drawText(QRectF(box_x, box_y, box_w, box_h), Qt.AlignCenter, text)
        if self.xp_label_pinned:
            # XP as a thin brass line under "Lv N", inside the label: the
            # owner wanted only health and stamina hanging below the name.
            suffix = self._level_suffix().lstrip(" \u00b7")
            level_w = fm.horizontalAdvance(suffix)
            level_x = box_x + pad_x + tw - level_w
            line_y = box_y + box_h - 3.5
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(QColor(0, 0, 0, 150)))
            painter.drawRoundedRect(QRectF(level_x, line_y, level_w, 2.5), 1.2, 1.2)
            fill = level_w * self.xp_fraction()
            if fill > 0.5:
                painter.setBrush(QBrush(QColor(*self.XP_BAR_COLOR, 255)))
                painter.drawRoundedRect(QRectF(level_x, line_y, fill, 2.5), 1.2, 1.2)
            painter.setBrush(Qt.NoBrush)
        bar_w = self.LABEL_BAR_WIDTH
        bar_x = self.x - bar_w * 0.5
        bar_y = box_y + box_h + 3.0
        if self.health_label_pinned:
            self._draw_health_bar(painter, bar_x, bar_y, bar_w,
                                  QRectF, Qt, QColor, QBrush, QPen)
            bar_y += self._health_bar_height() + 2.0
        if self.stamina_label_pinned:
            self._draw_label_bar(painter, bar_x, bar_y, bar_w, self._xp_bar_height(),
                                 self.stamina_fraction(), QColor(*self.STAMINA_BAR_COLOR, 235),
                                 QRectF, Qt, QColor, QBrush, QPen)

    def render(self, painter, always_show_names: bool = False) -> None:
        # One frame's worth of solved leg chains. Cleared here rather than
        # grown forever, because the key includes foot positions that change
        # every frame and would otherwise never be looked up again.
        self._chain_points_cache.clear()
        render_mode = str(self.model.get("render_mode", "procedural")).lower()
        camouflage_strength = clamp(float(self._camouflage_strength), 0.0, 1.0)
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

