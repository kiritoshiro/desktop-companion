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
    clamp,
    distance,
)
from ..constants import (
    COMBAT_SPACING,
    COMBAT_SPACING_SLACK,
    ESCAPED_RADIUS,
    HUNT_COMMITTED_STATES,
    JOB_MODE_STATES,
    JOB_PREEMPTING_STATES,
    JOB_STATES,
    RECOVER_ARRIVE_RADIUS,
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

    def _flee_from_danger(self, dt: float) -> bool:
        """Run: home if there is a base to run to, away if there is not.

        Three of the owner's observations are one behaviour. "when nearing
        low health they should try to run away", "if outnumbered they should
        try to avoid conflict, and run quickly away", and "at the base they
        can reheal, when low health they should seek their bases" -- so a
        retreat is not merely away from the danger, it is *towards* the one
        place that puts health back on. Base regen already exists (DC-21);
        nothing was ever steering a hurt spider to it.

        A spider with no base, or one too far to matter, simply runs
        directly away, which is the honest fallback for a team that has not
        built anything.
        """
        threat = self.flee_from
        home = self._own_base_point()

        if home is not None:
            target_x, target_y = home
            # Refuse a "retreat" that runs through the thing being fled from.
            if threat is not None:
                to_home = math.atan2(target_y - self.y, target_x - self.x)
                to_threat = math.atan2(threat.y - self.y, threat.x - self.x)
                gap = abs((to_home - to_threat + math.pi) % math.tau - math.pi)
                if gap < 0.7:
                    home = None

        if home is None:
            if threat is None:
                return False
            away = math.atan2(self.y - threat.y, self.x - threat.x)
            reach = float(self.personality.get("reaction_radius", 360)) * 0.9
            target_x = self.x + math.cos(away) * reach
            target_y = self.y + math.sin(away) * reach

        target_x = clamp(target_x, 12.0, float(self.screen_w) - 12.0)
        target_y = clamp(target_y, 12.0, float(self.screen_h) - 12.0)

        if self.has_skill("chase"):
            if self.state != "Chase":
                self.enter_chase(target_x, target_y)
            else:
                self.target_x, self.target_y = target_x, target_y
            return True
        if self.has_skill("approach"):
            if self.state != "Approach":
                self.enter_approach(target_x, target_y)
            else:
                self.target_x, self.target_y = target_x, target_y
            return True
        # Nothing that can run. Facing the danger at least reads as cornered
        # rather than as oblivious.
        if threat is not None:
            self.target_heading = math.atan2(threat.y - self.y, threat.x - self.x)
        return False

    def _walk_home_to_heal(self, dt: float) -> bool:
        """Head for the base to heal, at walking pace (DC-64).

        The calm half of DC-50's retreat. A sprint is for getting away; this
        is what a spider does once it has, while still too hurt to be any use
        in a fight. Returns False when there is nowhere to go, so a team that
        has built nothing simply carries on rather than standing still --
        there is no base to reach, and pretending otherwise would swap one
        stuck spider for another.

        Deliberately reuses the ordinary travel states rather than adding one:
        a limping spider should look like a spider walking somewhere, and
        every state it could be in already animates.
        """
        home = self._own_base_point()
        if home is None:
            self.recovering = False
            return False
        target_x, target_y = home
        if self._foe_near(target_x, target_y, ESCAPED_RADIUS):
            # Home is where the enemy is. Walking into it would put the
            # spider straight back over the flee threshold, and it would
            # bounce between running and limping home for as long as the foe
            # stayed -- measured at 2104 of 7200 frames before this guard.
            # Stay out of it and get on with things until the base is clear.
            return False
        if math.hypot(target_x - self.x, target_y - self.y) <= RECOVER_ARRIVE_RADIUS:
            # Arrived. Sit in the regen radius and let the base do its work;
            # the manager drops `recovering` once hp is back up.
            self.motion_paused = True
            self.speed = 0.0
            self.target_heading = angle_to(self.x, self.y, target_x, target_y)
            return True
        if self.has_skill("approach"):
            if self.state != "Approach":
                self.enter_approach(target_x, target_y)
            else:
                self.target_x, self.target_y = target_x, target_y
            return True
        if self.has_skill("chase"):
            if self.state != "Chase":
                self.enter_chase(target_x, target_y)
            else:
                self.target_x, self.target_y = target_x, target_y
            return True
        return False

    def _foe_near(self, x: float, y: float, radius: float) -> bool:
        """Whether any hostile spider is within ``radius`` of a point."""
        for other in (getattr(self, "neighbors", None) or ()):
            if other is self or getattr(other, "dead", False):
                continue
            try:
                if self.relation_to(other) != "foe":
                    continue
            except Exception:
                continue
            if math.hypot(other.x - x, other.y - y) <= radius:
                return True
        return False

    def _own_base_point(self):
        """This spider's own team's base, if it has one, as (x, y)."""
        base_world = getattr(self, "base_world", None)
        if base_world is None:
            return None
        team = str(getattr(self.progression, "team_id", "neutral") or "neutral")
        if team == "neutral":
            return None
        site = getattr(base_world, "bases", {}).get(f"team:{team}")
        if site is None:
            return None
        return float(site.x), float(site.y)

    def _pursue_foe(self, dt: float) -> bool:
        """Fight the foe the manager has published, with this spider's own kit.

        DC-45. Before this, conflict was contact damage and nothing else:
        spiders hurt each other only by happening to collide while doing
        something unrelated. A fight now uses the skills the spider actually
        has -- silk to pin from range, a pounce to close, a chase or an
        approach otherwise -- so two spiders with different kits fight
        visibly differently without a single personality-id branch. Which
        skills a spider has is already data (DC-19).

        Returns True when it has taken the tick, so the caller leaves the
        ordinary personality FSM alone. Structured like ``_pursue_prey``,
        which does the same job for a fly, and for the same reason: closing
        on something that is moving needs every-frame reactivity rather than
        the arbiter's 5-10 Hz throttle.
        """
        foe = self._foe
        if foe is None or getattr(foe, "dead", False) or self.dead:
            return False
        if self.dragging or self.airborne or self.state in HUNT_COMMITTED_STATES:
            return False

        fx, fy = foe.x, foe.y
        d = distance(self.x, self.y, fx, fy)

        # Pin it from range first, if this spider throws silk at all. A foe
        # that is already webbed is left alone -- a second glob buys nothing
        # and the shooter should be closing in instead.
        if not foe.webbed and (self.has_skill("shoot_web") or self.has_skill("wall_web")):
            if self._maybe_shoot_web_at_foe(foe, d):
                return True

        standoff = (self.size + foe.size) * COMBAT_SPACING
        # A pinned foe cannot dodge, so a pounce onto it is worth the
        # commitment; a loose one is pounced at less eagerly.
        #
        # Only ever launched from *outside* arm's length, because a pounce is
        # how a spider closes. Without the lower bound the two spent the
        # whole brawl nose to nose re-aiming at each other: measured, 234 of
        # 364 frames overlapping and 99 of them with both spiders in `Aim`,
        # which pauses motion and is a committed state, so no spacing rule
        # could ever reach them.
        strike = self.size * 4.5 + foe.size
        if (standoff * 0.95 <= d <= strike
                and self.has_skill("jump") and self.has_skill("prepare_jump_attack")
                and getattr(self, "_pounce_cooldown", 0.0) <= 0.0
                and self.rng.random() < (0.65 if foe.webbed else 0.32)):
            self._pounce_cooldown = self.rng.uniform(0.9, 1.6)
            # "strike", not "outcome". `_resolve_pounce_outcome` is the
            # *social* resolution -- catch, cuddle or flee -- written for
            # pouncing on a friend, and a pounce on an enemy went through it
            # too. Measured over a 364-frame brawl: 170 frames in Cuddle,
            # both spiders glued together and unable to be spaced by anything
            # `_pursue_foe` did, because `Cuddle` is a committed state and
            # this method hands the tick back whenever it is in one. That is
            # the whole of the "a fight is one clump of overlapping bodies"
            # report, and it was never a spacing problem.
            self.enter_aim(fx, fy, target=None, after="strike",
                           ranging=(0.4, 0.85), abort_chance=0.05)
            return True

        # DC-59: close to arm's length and hold there, rather than walking
        # into the foe. The first real-hardware session reported that a fight
        # reads as one clump of overlapping bodies, and this is why: both
        # spiders aimed at each other's centre, so both kept walking until
        # they were on top of each other and stayed there for the rest of the
        # brawl. `CONTACT_REACH` in manager/combat.py decides whether a blow
        # connects and was raised to sit above this band, so holding at the
        # standoff distance does not stop the fight -- a spider strikes with
        # its front legs out, not with the width of its body.
        span = max(1e-4, d)
        # Unit vector from the foe towards this spider, so the same two
        # numbers serve closing in and backing off.
        away_x, away_y = (self.x - fx) / span, (self.y - fy) / span
        if d < standoff * (1.0 - COMBAT_SPACING_SLACK):
            # Too close. Holding still here is not enough: a pounce, a shove
            # or the foe's own approach puts them on top of each other, and
            # measured over a 364-frame brawl they spent 171 frames closer
            # than one combined size -- the "fight is one clump" complaint,
            # still true with a hold-only rule. Back out to the ring.
            # Walked to, not fled to. `enter_retreat` flees 220-380px from
            # the threat, which would end the fight rather than space it.
            stop_x = fx + away_x * standoff
            stop_y = fy + away_y * standoff
            self._walk_towards_in_fight(stop_x, stop_y)
            self.focus_x, self.focus_y = fx, fy
            return True
        if d <= standoff * (1.0 + COMBAT_SPACING_SLACK):
            # At the right distance: hold the ground and face it. The stance,
            # the strikes and the silk all still run -- this only stops the
            # walking.
            self.target_x, self.target_y = self.x, self.y
            self.focus_x, self.focus_y = fx, fy
            return True
        # Aim at the near edge of the foe rather than at its middle.
        stop_x = fx + away_x * standoff
        stop_y = fy + away_y * standoff
        self.focus_x, self.focus_y = fx, fy
        if self._walk_towards_in_fight(stop_x, stop_y):
            return True
        # No way to close and nothing to throw: stand its ground rather than
        # pretending to fight.
        return False

    def _walk_towards_in_fight(self, x: float, y: float) -> bool:
        """Move to a point during a fight, with whatever this spider has.

        One path for closing in and for backing off, because they are the
        same act: a fighting spider is always walking to a spot on a ring
        around its foe, and only the side of the ring changes.
        """
        if self.has_skill("chase"):
            if self.state != "Chase":
                self.enter_chase(x, y)
            else:
                self.target_x, self.target_y = x, y
            return True
        if self.has_skill("approach"):
            if self.state != "Approach":
                self.enter_approach(x, y)
            else:
                self.target_x, self.target_y = x, y
            return True
        return False

    def _maybe_shoot_web_at_foe(self, foe, d: float) -> bool:
        """Roll to fling trapping silk at a foe spider. True if it committed.

        The same shot, the same cooldown and the same range rules the spider
        already uses on prey and on the cursor -- a web-shooter does it
        eagerly, anyone else with the skill does it sometimes.
        """
        # Webbed: cannot fight back (the owner, 2026-09-25).
        if self.webbed_held:
            return False
        if self.web_shot_cooldown > 0.0 or self.fly_world is None:
            return False
        can_trap = self.has_skill("shoot_web")
        can_wall = self.has_skill("wall_web")
        if not (can_trap or can_wall):
            return False
        reaction = float(self.personality.get("reaction_radius", 360))
        web_shooter = self._acts_as_web_shooter()
        range_mult = float(self.personality.get("web_shot_range_mult",
                                                0.85 if web_shooter else 0.5))
        shot_range = max(self.size * 4.0, reaction * range_mult)
        if d > shot_range or d < self.size * 1.5:
            return False
        chance = float(self.personality.get("web_shot_chance", 0.6 if web_shooter else 0.12))
        chance = clamp(chance + self.mood.arousal * 0.15, 0.0, 0.97)
        if self.rng.random() >= chance:
            return False
        kind = "trap"
        if can_wall and (not can_trap or
                         self.rng.random() < float(self.personality.get("wall_web_bias", 0.3))):
            kind = "wall"
        self.enter_web_aim(foe.x, foe.y, kind, prey=None, foe=foe)
        return True

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

