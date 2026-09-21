"""Working a job: pursuing prey, and how job state yields to temperament.

Split out of the single ``behaviour.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass

from ...support.math_utils import (
    angle_to,
    distance,
)
from ..constants import (
    HUNT_COMMITTED_STATES,
    JOB_MODE_STATES,
    JOB_PREEMPTING_STATES,
    JOB_STATES,
)


class JobBehaviourMixin:
    """Working a job: pursuing prey, and how job state yields to temperament."""

    def _pursue_prey(self, dt: float, mx: float, my: float) -> None:
        """Act on the prey candidate ``perception`` publishes (DC-18, C1).

        Every locked-on spider commits to closing on its prey using its own
        abilities, with the odd pounce so it visibly leaps onto it; web
        shooters fling the same trapping silk they use on the cursor to pin
        it from range first.

        Moved here from ``CreatureManager._drive_hunt``, which used to call
        ``enter_approach``/``enter_chase`` and write ``target_x``/``speed``/
        ``motion_paused`` on this creature directly from outside, every frame
        -- the exact outside-in overwrite the plan calls out. The manager
        still decides *which* fly (if any) this spider is locked onto
        (``self._prey``, set by ``CreatureManager._update_prey_targets``) --
        that is target selection among the manager's own flies, the kind of
        manager-injected state DC-17's docstring already treats as
        acceptable, not a decision about this creature's own behaviour. This
        method is that decision: it reads the published candidate through
        ``self.perception.prey()`` and reacts to it with its own seeded
        ``self.rng`` instead of the manager's.

        Called once per tick from ``Creature.update()``, before the dragging
        branch (a spider held in the hand can still fire silk at prey it is
        locked onto), not gated by the arbiter's 5-10 Hz throttle: closing on
        a moving fly needs the same every-frame reactivity Approach/Chase
        already give a spider stalking the cursor, so this is execution of
        an already-standing hunt commitment, not a fresh "what should I do"
        choice -- see arbiter.py's module docstring for the same distinction
        applied to Alert-state stalking.
        """
        prey = self.perception.prey()
        if prey is None:
            return
        if self.state in HUNT_COMMITTED_STATES or self.airborne:
            return
        if self.dragging:
            if not prey.trapped and (self.has_skill("shoot_web") or self.has_skill("wall_web")):
                self._maybe_shoot_web_at_prey(prey)
            return

        # Web shooters: pin a still-loose fly with silk from range (the same
        # shot used on the cursor); HUNT_COMMITTED_STATES above lets the shot
        # finish before the spider closes in to eat it.
        if not prey.trapped and (self.has_skill("shoot_web") or self.has_skill("wall_web")):
            if self._maybe_shoot_web_at_prey(prey):
                return

        d = distance(self.x, self.y, mx, my)
        strike = self.size * 5.0 + prey.size
        hunter = self._acts_as_hunter()
        prey_moving = bool(getattr(prey, "is_moving", False))

        # A Hunter uses the fly's stop-and-go rhythm as camouflage: it advances
        # only during a walking bout and freezes while the fly pauses or turns.
        # A trapped fly is an exception because its struggling web is already a
        # strong signal and it otherwise could never be reached.
        if hunter and not prey_moving and not prey.trapped:
            self.target_x, self.target_y = mx, my
            self.target_heading = math.atan2(my - self.y, mx - self.x)
            self.motion_paused = True
            self.speed = 0.0
            return

        # Pounce only at loose prey; a trapped fly cannot escape, so a forced
        # jump here would make every capture look identical and prevent the
        # normal contact catcher from doing its job.
        if (not prey.trapped and d <= strike and self.has_skill("jump")
                and self.has_skill("prepare_jump_attack")
                and getattr(self, "_pounce_cooldown", 0.0) <= 0.0
                and self.rng.random() < (0.5 if hunter else 0.4)):
            self._pounce_cooldown = self.rng.uniform(0.8, 1.5)
            self.enter_aim(mx, my, target=None, after="outcome",
                           ranging=(0.4, 0.85), abort_chance=0.06)
            return

        # Hunters stalk in Approach so the normal still-target observation
        # logic can freeze them. Other personalities retain the direct chase.
        if hunter and self.has_skill("approach"):
            if self.state != "Approach":
                self.enter_approach(mx, my)
        elif self.has_skill("chase"):
            if self.state != "Chase":
                self.enter_chase(mx, my)
        elif self.has_skill("approach") and self.state != "Approach":
            self.enter_approach(mx, my)

    def _update_job_state(self, dt: float, mx: float, my: float) -> bool:
        """Apply a job's work intent before ordinary personality FSM logic.

        Data-driven over ``JOB_MODE_STATES`` (DC-18, C6) rather than an
        if/elif hard-coded to specific job ids: this method used to start
        with ``if self.job_id not in ("builder", "guard"): return False``,
        so a future job with any other id would return *before* ever
        reaching the hand-back call below, leaving a leftover job state
        frozen. That id gate is gone: this now runs unconditionally for
        every creature every tick, keyed only on ``job_mode``, and
        ``_release_job_state()`` (via its own ``state in JOB_STATES`` check)
        hands back anything the table does not recognise -- see
        ``tests/test_arbiter.py::test_unrecognized_job_id_still_hands_back_a_leftover_job_state``,
        proved by reverting to the old id-gated form.
        """
        mode = str(getattr(self, "job_mode", "idle") or "idle")
        target = getattr(self, "job_target", None)
        state_for_mode = JOB_MODE_STATES.get(mode)
        self.job_busy = self._job_outranked_by_personality(mode)
        if self.job_busy or state_for_mode is None or (state_for_mode != "JobBuild" and target is None):
            # Either temperament is mid-something more urgent, this spider is
            # off shift, or its job published a mode this table does not (yet)
            # know how to move for. Hand it back rather than overwriting
            # whatever state it is already in.
            self._release_job_state()
            return False

        self.state = state_for_mode
        if state_for_mode == "JobTravel":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 52.0 * self._speed_mult()
        elif state_for_mode == "JobBuild":
            self.motion_paused = True
            self.speed = 0.0
            self.target_x, self.target_y = self.x, self.y
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 0.9)
        elif state_for_mode == "JobPatrol":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            # DC-42: a guard on station looks outwards, across its own patrol
            # line rather than along it. The job publishes that facing only
            # once the guard has arrived; while it is still walking there,
            # `job_facing` is None and it faces the way it is going.
            facing = getattr(self, "job_facing", None)
            self.target_heading = (
                float(facing) if facing is not None
                else angle_to(self.x, self.y, self.target_x, self.target_y)
            )
            self.speed = 38.0 * self._speed_mult()
        elif state_for_mode == "JobGuardAlert":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 72.0 * self._speed_mult()
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 2.0)
        elif state_for_mode == "JobScoutTravel":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 44.0 * self._speed_mult()
        elif state_for_mode == "JobScoutReport":
            self.motion_paused = True
            self.speed = 0.0
            self.target_x, self.target_y = self.x, self.y
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 0.7)
        elif state_for_mode == "JobWebTravel":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 50.0 * self._speed_mult()
        elif state_for_mode in ("JobWebRepair", "JobWebWeave"):
            self.motion_paused = True
            self.speed = 0.0
            self.target_x, self.target_y = self.x, self.y
            self.aim_intent = min(1.0, self.aim_intent + max(0.0, dt) * 0.9)
        elif state_for_mode == "JobHuntPatrol":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 60.0 * self._speed_mult()
        elif state_for_mode == "JobHuntReturn":
            self.motion_paused = False
            self.target_x, self.target_y = float(target[0]), float(target[1])
            self.target_heading = angle_to(self.x, self.y, self.target_x, self.target_y)
            self.speed = 85.0 * self._speed_mult()
        return True

    def _job_outranked_by_personality(self, mode: str) -> bool:
        """Return whether temperament currently beats this spider's job."""
        if self.state in JOB_PREEMPTING_STATES:
            return True
        # A guard answering an intruder at its own base outranks an ordinary
        # hunt, but nothing outranks fleeing or a jump already in the air.
        # A hunter walking already-caught food home (DC-21) gets the same
        # exception: in a fly-rich scene it re-locks onto its next target
        # (``_hunting_prey`` goes back to True) the instant it finishes
        # eating, so without this a catch is banked as "carrying" and then
        # never actually delivered -- confirmed empirically, resources
        # stayed at zero through a whole busy-colony run. This is not a
        # change to how a fresh hunt is prioritised against job duty in
        # general (jobs.py already lets a fresh hunt win over starting a
        # new delivery trip); it only protects a trip already committed to.
        if self._hunting_prey and mode not in ("guard_alert", "hunt_return"):
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

