"""Single utility arbiter for "what should this spider do next" (DC-18, C1/C6).

Before this module, four layers independently decided a creature's activity:
hard-coded personality-id branches inside ``_update_state`` for the Idle/Alert
"free choice" points, the probability chains in ``_consider_special_actions``,
``BehaviourPhaseScheduler`` trying itself first and winning outright, and
``CreatureManager._drive_hunt`` reaching in from outside and overwriting a
creature's state every frame. This module replaces the first three with one
scored decision; ``_drive_hunt``'s replacement (publishing a prey candidate
instead of overwriting state) lives beside it in
``creature/behaviour.py::BehaviourMixin._pursue_prey``, described below.

How scoring works
------------------
``decide()`` is called only at a creature's existing "free choice" points --
the same moments ``_activate_scheduled_phase``/``_consider_special_actions``
used to run: Idle whenever ``decision_timer`` clears, Alert whenever
``state_timer`` clears (see ``BehaviourMixin._run_arbiter``). It gathers every
``Candidate`` whose situational gate passes, scores each as::

    score = temperament_weight * focus_bias * situational * mood_factor * noise

then executes the highest scorer. ``temperament_weight`` reuses
``personality_profiles.phase_scores_for()`` wherever a candidate maps onto an
existing behaviour-phase id (wander/jump/roll/zoomies/approach/chase/observe/
prepare_jump_attack/inspect/cuddle/social_play/run_away) -- the same
continuous, trait-derived weight ``BehaviourPhaseScheduler`` already used,
rather than reinventing one. Candidates with no phase-id analogue (web care,
shooting silk, hunting) use a fixed base weight tuned from the probability
constants ``_consider_special_actions`` used to roll independently, since an
argmax-over-candidates needs comparable magnitudes rather than independent
chances. ``focus_bias`` reuses ``phase_scheduler.FOCUS_PHASE_BIAS``. A small
per-candidate multiplicative noise term (``creature.rng.uniform(0.85, 1.15)``)
keeps the choice from going stale/deterministic between ties, using the
creature's own seeded RNG so runs stay reproducible.

Hard priorities above scoring
------------------------------
Per the plan: reflexes, then job duty, then scheduled phase, then idle
flourish. Concretely:

1. **Reflexes** (threat retreat, the nope-escape trigger, being dragged) are
   unconditional and already sit ahead of ``_update_state`` in
   ``Creature.update()`` -- they can fire on any frame regardless of what the
   arbiter would have scored, and none of that code changed here.
2. **Job duty** (``_update_job_state``) still runs before ``_update_state``'s
   personality dispatch and returns early when a job is active, so the
   arbiter never even runs for a spider mid-shift. See
   ``creature/constants.py``'s ``JOB_MODE_STATES`` for the generalised,
   first-class job-mode table (C6) that replaced the id-gated hand-back.
3. **Scheduled phase**: rather than ``BehaviourPhaseScheduler`` trying itself
   first and winning outright (the old ``_activate_scheduled_phase``), it is
   folded in as one scored ``Candidate`` here (``_scheduled_phase_candidate``)
   using ``BehaviourPhaseScheduler.peek_score()`` -- a non-mutating preview of
   what it would draw -- so it competes on the same footing as everything
   else and is only committed to (a real, deck-mutating ``.choose()``) if it
   actually wins.
4. **Idle flourish**: a baseline ``idle``/``wander`` candidate is always
   present with a low but non-zero score, so ``decide()`` never returns
   nothing -- the creature simply goes back to idling if nothing else is
   compelling, exactly the old ``else: self.enter_idle()`` fallback.

Deliberately out of scope, and why
-----------------------------------
Personality-id conditionals that run *inside* an already-committed activity
-- a Hunter's stalk-freeze cadence while already in Alert/Approach/Chase, a
Drifter's mid-slide physics, a Jumper's hop kinematics once launched -- are
not touched. Those are execution of a choice the arbiter (or, for hunting,
``_pursue_prey``) already made, exactly the category the plan places
``enter_*`` methods in ("the existing enter_* methods stay as executors").
Fully deleting ``_is_hunter_personality``/``_is_jumper_personality``/etc. and
converting temperament into pure trait-driven modules is DC-19's explicitly
separate job ("Delete `_is_*_personality`"); doing that here as well would
duplicate DC-19 and risks exactly the kind of stalk-mechanic regression the
plan's own guardrails warn about on the highest-risk package in the plan.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Optional

from .personality_profiles import phase_scores_for
from .phase_scheduler import FOCUS_PHASE_BIAS, normalize_focus


@dataclass
class Candidate:
    """One thing a creature could commit to doing next."""

    action_id: str
    score: float
    execute: Callable[[], None]


def _phase_weight(personality: dict, phase_id: str) -> float:
    """Temperament weight for a candidate with a behaviour-phase analogue."""
    return float(phase_scores_for(personality).get(phase_id, 0.0))


def _focus_bias(focus: str, phase_id: str) -> float:
    bias_map = FOCUS_PHASE_BIAS[normalize_focus(focus)]
    return float(bias_map.get(phase_id, 0.25))


def _noise(creature) -> float:
    return creature.rng.uniform(0.85, 1.15)


def _idle_execute(creature) -> None:
    # Only actually re-enter Idle (which rolls a fresh 1-3s ``state_timer``)
    # when there is a reason to: already-Idle with time left just keeps
    # waiting. Without this guard, "idle" winning by default every time the
    # arbiter runs (5-10 Hz) would re-roll the idle beat before it ever
    # expired, and every other candidate gated on `state_timer_expired`
    # would starve forever -- exactly the old fallback chain's own gating
    # (nothing in it ran before `state_timer <= 0` either).
    if creature.state == "Idle" and creature.state_timer > 0.0:
        return
    creature.enter_idle()


def _idle_candidate(creature) -> Candidate:
    return Candidate("idle", 0.05, lambda: _idle_execute(creature))


def _engage_cursor_candidate(creature, perception, mx: float, my: float,
                              dist_to_cursor: float, focus: str) -> Optional[Candidate]:
    """Should this spider start reacting to the cursor at all.

    Mirrors the old unconditional hunter check ("hunter and dist < reaction")
    generalised to every personality: everyone is *eligible* to react, but a
    Hunter's much larger phase weight for "approach"/"chase" plus its cursor-
    stillness read (via ``perception``) means it still wins this candidate
    far more often, matching the original bias without hard-coding the id.
    """
    reaction = float(creature.personality.get("reaction_radius", 360))
    if dist_to_cursor >= reaction:
        return None
    hunter = creature._is_hunter_personality()
    cursor_still = hunter and creature._cursor_is_still_for_observe()
    closeness = 1.0 - min(1.0, max(0.0, dist_to_cursor / max(1.0, reaction)))
    weight = _phase_weight(creature.personality, "approach") + (2.5 if hunter else 0.0)
    bias = _focus_bias(focus, "approach")
    score = weight * bias * (0.4 + closeness) * _noise(creature)
    if score <= 0.0:
        return None

    def execute() -> None:
        if cursor_still:
            creature.enter_alert(mx, my)
        else:
            creature.enter_approach(mx, my)

    return Candidate("engage_cursor", score, execute)


def _hop_candidate(creature, state_timer_expired: bool) -> Optional[Candidate]:
    if not state_timer_expired or not creature._is_jumper_personality():
        return None
    if creature.rng.random() >= float(creature.personality.get("idle_hop_chance", 0.55)):
        return None
    weight = _phase_weight(creature.personality, "jump") + 2.0
    score = weight * _noise(creature)

    def execute() -> None:
        import math
        tx = creature.x + math.cos(creature.heading + creature.rng.uniform(-0.65, 0.65)) * creature.size * creature.rng.uniform(1.0, 2.0)
        ty = creature.y + math.sin(creature.heading + creature.rng.uniform(-0.65, 0.65)) * creature.size * creature.rng.uniform(1.0, 2.0)
        power = creature.personality.get("hop_power")
        from .math_utils import rand_range
        creature.enter_coil(after="wander", power=rand_range(power, 0.34, 0.58, rng=creature.rng), toward=(tx, ty))

    return Candidate("hop", score, execute)


def _observe_candidate(creature, mx: float, my: float, dist_to_cursor: float, focus: str) -> Optional[Candidate]:
    anchor = creature._observer_anchor(mx, my)
    if anchor is None:
        return None
    weight = _phase_weight(creature.personality, "observe") + 2.5
    bias = _focus_bias(focus, "observe")
    score = weight * bias * _noise(creature)
    if score <= 0.0:
        return None
    ax, ay, target = anchor

    def execute() -> None:
        creature.enter_observe(ax, ay, target)

    return Candidate("observe", score, execute)


def _alert_escalation_candidates(creature, mx: float, my: float, dist_to_cursor: float, focus: str):
    """From Alert only: escalate to Chase, or break off into Retreat.

    Mirrors the old Alert-state fallback chain (threat_radius-gated retreat,
    then a close-range chase roll, folded here as two scored candidates
    instead of a sequential if/elif so they compete with everything else
    the same way).
    """
    if creature.state != "Alert":
        return
    boldness = max(0.0, min(1.0, float(creature.personality.get("boldness", 0.5))))
    reaction = float(creature.personality.get("reaction_radius", 360))
    if creature.has_skill("run_away") and dist_to_cursor < float(creature.personality.get("threat_radius", 180)) * 0.55:
        timid = 1.0 - boldness
        weight = _phase_weight(creature.personality, "run_away") + timid * 3.0
        yield Candidate("alert_retreat", weight * _focus_bias(focus, "run_away") * _noise(creature),
                         lambda: creature.enter_retreat(mx, my))
    if creature.has_skill("chase") and dist_to_cursor < 95.0:
        weight = _phase_weight(creature.personality, "chase") + (0.45 + boldness * 0.45) * 4.0
        yield Candidate("alert_chase", weight * _focus_bias(focus, "chase") * _noise(creature),
                         lambda: creature.enter_chase(mx, my))
    if dist_to_cursor < reaction:
        weight = _phase_weight(creature.personality, "approach") + (0.25 + boldness * 0.65) * 3.0
        yield Candidate("alert_approach", weight * _focus_bias(focus, "approach") * _noise(creature),
                         lambda: creature.enter_approach(mx, my))


def _drift_run_candidate(creature, from_idle: bool) -> Optional[Candidate]:
    if not creature._should_start_drift_run(from_idle=from_idle):
        return None
    return Candidate("drift_run", 6.0 * _noise(creature), creature.enter_drift_run)


def _web_care_candidates(creature, perception, reaction: float):
    """Repair, adopt, weave-new and web-walk, folded from `_consider_special_actions`."""
    if creature.has_skill("weave_web") and creature.cage is None and perception.has_web_world and creature.weave_cooldown <= 0.0:
        webber = creature._is_webber_personality()
        repairable = perception.repairable_web(max_dist=1e9 if webber else reaction * 1.5)
        if repairable is not None:
            score = (9.0 if webber else 2.5) * _noise(creature)
            yield Candidate("repair_web", score, lambda w=repairable: creature._begin_repair(w))

        adoptable = perception.adoptable_web(max_dist=1e9 if webber else reaction * 1.6)
        if adoptable is not None:
            score = (8.5 if webber else 2.2) * _noise(creature)
            yield Candidate("adopt_web", score, lambda w=adoptable: creature._begin_adopt(w))

        intact = perception.intact_web_count()
        satiation = max(0.12, min(1.0, 1.0 - intact * float(creature.personality.get("web_satiation_per_web", 0.22))))
        m = creature.mood
        base = (6.2 + m.curiosity * 2.8) if webber else 0.3
        score = base * satiation * _noise(creature)
        yield Candidate("weave_new", score, creature._begin_weave)

    if creature.has_skill("web_walk") and creature.cage is None and perception.has_web_world and creature.web_walk_cooldown <= 0.0:
        walkable = perception.walkable_web(max_dist=reaction * 1.8)
        if walkable is not None:
            webber = creature._is_webber_personality()
            m = creature.mood
            score = (4.5 if webber else 1.6 + m.curiosity * 2.0) * _noise(creature)
            yield Candidate("web_walk", score, lambda w=walkable: creature._begin_web_walk(w))


def _shoot_web_candidate(creature, dist_to_cursor: float, reaction: float, mx: float, my: float) -> Optional[Candidate]:
    if dist_to_cursor >= reaction:
        return None
    if not (creature.has_skill("shoot_web") or creature.has_skill("wall_web")):
        return None
    if creature.web_shot_cooldown > 0.0:
        return None
    score = 3.0 * _noise(creature)
    return Candidate("shoot_web_cursor", score, lambda: creature._maybe_shoot_web_at_cursor(dist_to_cursor, mx, my))


def _social_candidates(creature, focus: str):
    hunter = creature._is_hunter_personality()
    if not creature.allow_social or hunter or creature.social_cooldown > 0.0:
        return
    reaction = float(creature.personality.get("reaction_radius", 360))
    social_range = max(reaction * 0.85, creature.size * 12.0)
    mate = creature._find_social_target(social_range)
    if mate is None:
        return
    d = ((creature.x - mate.x) ** 2 + (creature.y - mate.y) ** 2) ** 0.5
    m = creature.mood

    play_w = 0.25 + m.arousal * 0.6 + max(0.0, m.valence) * 0.4
    inspect_w = 0.25 + m.curiosity * 0.8
    cuddle_w = 0.1 + m.affection * 0.8

    play_weight = _phase_weight(creature.personality, "social_play") + play_w * 2.0
    inspect_weight = _phase_weight(creature.personality, "inspect") + inspect_w * 2.0
    cuddle_weight = _phase_weight(creature.personality, "cuddle") + cuddle_w * 2.0

    def play_execute(mate=mate, d=d) -> None:
        if d > creature.size * 3.0 and m.arousal > 0.45 and creature.rng.random() < 0.5:
            creature.enter_aim(mate.x, mate.y, target=mate, after="play", ranging=(0.4, 0.8), abort_chance=0.1)
        else:
            creature.enter_play(mate)

    yield Candidate("social_play", play_weight * _focus_bias(focus, "social_play") * _noise(creature), play_execute)
    yield Candidate("social_inspect", inspect_weight * _focus_bias(focus, "inspect") * _noise(creature),
                     lambda mate=mate: creature.enter_inspect(mate.x, mate.y, target=mate))
    yield Candidate("social_cuddle", cuddle_weight * _focus_bias(focus, "cuddle") * _noise(creature),
                     lambda mate=mate: creature.enter_cuddle(mate.x, mate.y, target=mate))


def _cursor_expressive_candidates(creature, dist_to_cursor: float, mx: float, my: float, focus: str):
    reaction = float(creature.personality.get("reaction_radius", 360))
    if dist_to_cursor >= reaction:
        return
    boldness = max(0.0, min(1.0, float(creature.personality.get("boldness", 0.5))))
    m = creature.mood
    near = dist_to_cursor < creature.size * 6.5
    mid = dist_to_cursor < reaction * 0.8

    if mid and (boldness > 0.6 or m.arousal > 0.6):
        weight = _phase_weight(creature.personality, "prepare_jump_attack") + (boldness + m.arousal) * 2.0
        yield Candidate(
            "cursor_pounce", weight * _focus_bias(focus, "prepare_jump_attack") * _noise(creature),
            lambda: creature.enter_aim(mx, my, target=None, after="outcome",
                                       ranging=(0.55, 1.2), abort_chance=0.22 - boldness * 0.15))
    if mid and m.curiosity > 0.55:
        weight = _phase_weight(creature.personality, "inspect") + m.curiosity * 2.0
        yield Candidate("cursor_inspect", weight * _focus_bias(focus, "inspect") * _noise(creature),
                         lambda: creature.enter_inspect(mx, my, target=None))
    if near and m.affection > 0.55:
        weight = _phase_weight(creature.personality, "cuddle") + m.affection * 2.0
        yield Candidate("cursor_cuddle", weight * _focus_bias(focus, "cuddle") * _noise(creature),
                         lambda: creature.enter_cuddle(mx, my, target=None))
    if near and m.valence > 0.4 and m.arousal > 0.5:
        weight = _phase_weight(creature.personality, "prepare_jump_attack") + (m.valence + m.arousal)
        yield Candidate("cursor_playful_pounce", weight * _focus_bias(focus, "prepare_jump_attack") * _noise(creature) * 0.6,
                         lambda: creature.enter_aim(mx, my, target=None, after="outcome",
                                                     ranging=(0.35, 0.7), abort_chance=0.1))
    if near and m.valence > 0.5 and m.arousal > 0.55:
        import math
        weight = _phase_weight(creature.personality, "roll") + (m.valence + m.arousal) * 0.5

        def execute(mx=mx, my=my) -> None:
            from .math_utils import angle_to
            creature.enter_roll(direction=angle_to(creature.x, creature.y, mx, my) + math.pi)

        yield Candidate("cursor_roll", weight * _focus_bias(focus, "roll") * _noise(creature) * 0.5, execute)


def _self_flourish_candidates(creature, from_idle: bool, state_timer_expired: bool, focus: str):
    if not (from_idle and state_timer_expired):
        return
    hunter = creature._is_hunter_personality()
    if hunter:
        return
    m = creature.mood
    if not (m.valence > 0.4 and m.arousal > 0.55):
        return
    roll_weight = _phase_weight(creature.personality, "roll")
    spring_weight = _phase_weight(creature.personality, "jump")
    zoomies_weight = _phase_weight(creature.personality, "zoomies")
    yield Candidate("self_roll", roll_weight * _focus_bias(focus, "roll") * _noise(creature) * 0.6,
                     creature.enter_roll)
    yield Candidate(
        "self_spring", spring_weight * _focus_bias(focus, "jump") * _noise(creature) * 0.6,
        lambda: creature.enter_spring(after="idle", power=creature.rng.uniform(0.7, 1.2)))
    yield Candidate("self_zoomies", zoomies_weight * _focus_bias(focus, "zoomies") * _noise(creature) * 0.6,
                     creature.enter_zoomies)


def _scheduled_phase_candidate(creature, focus: str, mx: float, my: float, state_timer_expired: bool) -> Optional[Candidate]:
    if not state_timer_expired or creature.airborne or creature.dragging:
        return None
    scheduler = creature.phase_scheduler
    preview_score = scheduler.peek_score(focus)
    if preview_score <= 0.0:
        return None
    score = preview_score * _noise(creature)

    def execute() -> None:
        # Only now do we actually draw from the deck (mutating), since this
        # candidate won the comparison -- see BehaviourPhaseScheduler.peek_score.
        for _ in range(len(scheduler.phase_ids) + 1):
            plan = scheduler.choose(focus)
            if plan is None:
                creature.enter_idle()
                return
            if creature._dispatch_scheduled_phase(plan.phase_id, focus, mx, my):
                creature.state_timer = plan.duration
                return
            scheduler.finish()
        creature.enter_idle()

    return Candidate("scheduled_phase", score, execute)


def decide(creature, perception, mx: float, my: float, dist_to_cursor: float,
           from_idle: bool, state_timer_expired: bool) -> Candidate:
    """Score every eligible candidate and return the winner.

    Called only from ``BehaviourMixin._run_arbiter`` at a creature's existing
    Idle/Alert decision points; never every frame (see that method's own
    5-10 Hz throttle).
    """
    focus = creature._phase_focus_context(mx, my)
    reaction = float(creature.personality.get("reaction_radius", 360))

    candidates: list[Candidate] = [_idle_candidate(creature)]

    def add(candidate: Optional[Candidate]) -> None:
        if candidate is not None:
            candidates.append(candidate)

    # While locked onto prey, (mx, my) is the fly's position, substituted in
    # by CreatureManager._creature_focus, not the real cursor -- hunting has
    # its own dedicated, more carefully tuned pathway (_pursue_prey, run
    # every frame, not throttled). Without this gate, these cursor-shaped
    # candidates would react to "how close is the fly" as if it were the
    # pointer and compete with _pursue_prey's own Approach/Chase entry
    # through a path with none of its stalk-freeze/pounce-probability
    # tuning -- confirmed as a real effect, not just a theoretical one: it
    # measurably cut a Builder's on-duty fraction in the DC-18 before/after
    # metrics (see the PR description) by pulling it into Chase more often.
    hunting = perception.is_hunting_prey
    if not hunting:
        add(_engage_cursor_candidate(creature, perception, mx, my, dist_to_cursor, focus))
    add(_hop_candidate(creature, state_timer_expired))
    if state_timer_expired:
        add(_observe_candidate(creature, mx, my, dist_to_cursor, focus))
        if not hunting:
            for candidate in _alert_escalation_candidates(creature, mx, my, dist_to_cursor, focus):
                add(candidate)
    add(_drift_run_candidate(creature, from_idle))
    if state_timer_expired:
        for candidate in _web_care_candidates(creature, perception, reaction):
            add(candidate)
        if not hunting:
            add(_shoot_web_candidate(creature, dist_to_cursor, reaction, mx, my))
        for candidate in _social_candidates(creature, focus):
            add(candidate)
        if not hunting:
            for candidate in _cursor_expressive_candidates(creature, dist_to_cursor, mx, my, focus):
                add(candidate)
        for candidate in _self_flourish_candidates(creature, from_idle, state_timer_expired, focus):
            add(candidate)
        add(_scheduled_phase_candidate(creature, focus, mx, my, state_timer_expired))

    return max(candidates, key=lambda c: c.score)
