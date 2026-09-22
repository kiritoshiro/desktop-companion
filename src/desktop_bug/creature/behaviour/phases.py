"""Scheduled phases, and the one call into the arbiter.

Split out of the single ``behaviour.py`` by DC-43; a pure move.
"""
from __future__ import annotations

from typing import TYPE_CHECKING, Tuple

if TYPE_CHECKING:
    from ..core import Creature

from ...support.math_utils import (
    distance,
)
from ..phase_scheduler import phase_id_for_state
from .. import arbiter


class PhaseMixin:
    """Scheduled phases, and the one call into the arbiter."""

    def _phase_focus_context(self, mx: float, my: float) -> str:
        """Classify the object currently most relevant to this spider."""
        prey = self._prey
        if self._hunting_prey and prey is not None and getattr(prey, "alive", False):
            return "prey"
        if (
            self.state in ("WeaveApproach", "Weave", "WebApproach", "WebWalk", "WebAim", "WebShot", "RepairApproach", "Repair")
            or self.web_target is not None
            or self.weaving_web is not None
            or self.repairing_web is not None
        ):
            return "web"
        # DC-61: a published foe outranks everything social. It does not
        # outrank prey or silk above -- a spider already committed to a hunt
        # or halfway through a web finishes that first, the same way the job
        # layer treats those as committed states.
        foe = getattr(self, "_foe", None)
        if foe is not None and not getattr(foe, "dead", False):
            return "foe"
        if self.social_target is not None and not getattr(self.social_target, "dragging", False):
            return self._social_focus_for(self.social_target)
        reaction = float(self.personality.get("reaction_radius", 360.0))
        social_range = max(self.size * 12.0, reaction * 0.85)
        mate = self._find_social_target(social_range) if self.allow_social else None
        cursor_distance = distance(self.x, self.y, mx, my)
        if mate is not None and (cursor_distance > reaction * 0.85 or distance(self.x, self.y, mate.x, mate.y) < cursor_distance * 1.15):
            return self._social_focus_for(mate)
        if cursor_distance < reaction:
            return "cursor_wary" if self.wary_of_cursor else "cursor"
        return "none"

    def _social_focus_for(self, other) -> str:
        """Ally, enemy, or just another spider.

        Read from the declared stance rather than from team equality, because
        two teams can be declared allies and a spider can carry its own
        per-creature override (DC-33).
        """
        try:
            relation = self.relation_to(other)
        except Exception:
            return "creature"
        if relation == "foe":
            return "foe"
        if relation == "friend":
            return "friend"
        return "creature"

    def _phase_target(self, focus: str, mx: float, my: float) -> Tuple[float, float, "Creature" | None]:
        """Resolve a phase focus to coordinates and, when applicable, a mate."""
        if focus == "prey":
            prey = self._prey
            if prey is not None and getattr(prey, "alive", False) and not getattr(prey, "eaten", False):
                return prey.x, prey.y, None
        if focus == "foe":
            foe = getattr(self, "_foe", None)
            if foe is not None and not getattr(foe, "dead", False):
                # No mate handed back: a foe is not someone to cuddle, and
                # returning one here is how a fight became a cuddle before.
                return foe.x, foe.y, None
        if focus in ("creature", "friend"):
            mate = self.social_target
            if mate is None or getattr(mate, "dragging", False) or getattr(mate, "airborne", False):
                mate = self._find_social_target(max(self.size * 12.0, float(self.personality.get("reaction_radius", 360.0))))
            if mate is not None:
                return mate.x, mate.y, mate
        return mx, my, None

    def _phase_allowed(self, phase_id: str) -> bool:
        scheduler = getattr(self, "phase_scheduler", None)
        return scheduler is None or scheduler.is_allowed(phase_id)

    def _dispatch_scheduled_phase(self, phase_id: str, focus: str, mx: float, my: float) -> bool:
        """Translate one scheduled phase into the existing FSM entry helpers."""
        target_phases = {
            "approach",
            "chase",
            "observe",
            "prepare_jump_attack",
            "inspect",
            "cuddle",
            "social_play",
            "run_away",
        }
        if phase_id in target_phases and focus == "none":
            return False
        tx, ty, target = self._phase_target(focus, mx, my)
        if phase_id == "wander":
            self.enter_wander()
        elif phase_id == "jump":
            self.enter_spring(after="idle", power=self.rng.uniform(0.72, 1.15))
        elif phase_id == "roll":
            self.enter_roll()
        elif phase_id == "zoomies":
            self.enter_zoomies()
        elif phase_id == "approach":
            self.enter_approach(tx, ty)
        elif phase_id == "chase":
            self.enter_chase(tx, ty)
        elif phase_id == "observe":
            self.enter_observe(tx, ty, target)
        elif phase_id == "prepare_jump_attack":
            self.enter_aim(tx, ty, target=target, after="outcome")
        elif phase_id == "inspect":
            self.enter_inspect(tx, ty, target)
        elif phase_id == "cuddle":
            self.enter_cuddle(tx, ty, target)
        elif phase_id == "social_play":
            if target is None:
                return False
            self.enter_play(target)
        elif phase_id == "run_away":
            self.enter_retreat(mx, my)
        else:
            return False
        return phase_id_for_state(self.state) == phase_id

    def _sync_scheduled_phase(self, focus: str | None = None) -> None:
        """Adopt legacy FSM entries so their periods also use personality pacing."""
        phase_id = phase_id_for_state(self.state)
        if phase_id is None:
            if self.phase_scheduler.current is not None:
                self.phase_scheduler.cancel()
            return
        if self.phase_scheduler.current_phase_id == phase_id:
            return
        duration = self.phase_scheduler.adopt(phase_id, focus or "none")
        if duration is not None:
            self.state_timer = duration

    def _finish_expired_scheduled_phase(self, mx: float, my: float) -> bool:
        """End a high-level period cleanly when its scheduled time is exhausted."""
        scheduler = self.phase_scheduler
        if scheduler.current is None or scheduler.remaining > 0.0:
            return False
        # Jump preparation and playful rolls are allowed to complete their
        # physical hand-off; the next grounded state will start or cancel the
        # following period.  A roll has two clocks: the personality phase timer
        # and the actual curl/spin animation. Ending the phase first leaves
        # ``roll_tuck``/``roll_spin`` active while the state is already Idle.
        if self.state in ("Coil", "Aim", "Jump", "Land", "Roll"):
            return False
        scheduler.finish()
        if self.state in ("Wander", "Approach", "Chase", "Observe", "Inspect", "Cuddle", "Play", "Zoom", "Roll", "Retreat"):
            self.enter_idle()
            return True
        return False

    def _run_arbiter(self, mx: float, my: float, dist_to_cursor: float, *, from_idle: bool) -> None:
        """Score every candidate action and commit to the winner (DC-18, C1).

        This is the single utility arbiter's only entry point: called from
        the Idle/Alert decision points below, throttled to 5-10 Hz by always
        resetting ``decision_timer`` here (rather than the assortment of
        per-branch resets the old personality/`_consider_special_actions`/
        scheduled-phase code used), never from every frame. See
        ``arbiter.py``'s module docstring for what is and is not folded in
        here, and why.
        """
        self.decision_timer = self.rng.uniform(0.1, 0.2)  # 5-10 Hz
        state_timer_expired = self.state_timer <= 0.0
        winner = arbiter.decide(self, self.perception, mx, my, dist_to_cursor,
                                 from_idle=from_idle, state_timer_expired=state_timer_expired)
        winner.execute()

