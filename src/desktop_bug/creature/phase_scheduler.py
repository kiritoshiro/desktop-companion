"""Weighted, focus-aware behaviour phase scheduling.

The existing creature FSM owns the concrete animation and movement states.  This
module only answers a higher-level question: which behaviour should get the next
short period of attention?  Keeping that decision separate makes personality
tuning data-driven without turning abilities into fake personality traits.
"""

from __future__ import annotations

from dataclasses import dataclass
import random
from typing import Iterable

from ..content.personality_profiles import (
    BEHAVIOUR_PHASE_IDS,
    phase_duration_multiplier_for,
    phase_scores_for,
)


# Base durations are deliberately broad.  The personality score and profile
# multiplier shape these ranges, then a per-creature RNG adds harmless variety.
PHASE_DURATION_RANGES = {
    "wander": (1.2, 3.4),
    "jump": (0.10, 0.24),
    "roll": (0.72, 1.20),
    "zoomies": (0.40, 0.82),
    "approach": (0.55, 1.35),
    "chase": (0.45, 1.25),
    "observe": (2.4, 5.8),
    "prepare_jump_attack": (0.55, 1.30),
    "inspect": (1.8, 4.8),
    "cuddle": (2.4, 6.2),
    "social_play": (2.1, 5.4),
    "run_away": (0.55, 1.35),
}

# A state can be entered by older code as well as by the scheduler.  The
# mapping lets the scheduler adopt those transitions without rewriting the FSM.
PHASE_STATE_IDS = {
    "Wander": "wander",
    "Coil": "jump",
    "Jump": "jump",
    "Roll": "roll",
    "Zoom": "zoomies",
    "Approach": "approach",
    "Chase": "chase",
    "Observe": "observe",
    "Aim": "prepare_jump_attack",
    "Inspect": "inspect",
    "Cuddle": "cuddle",
    "Play": "social_play",
    "Retreat": "run_away",
}


# Focus changes usefulness, not personality.  A zero personality score or a
# disabled behaviour always wins over these multipliers.  A small non-zero
# fallback keeps a creature from becoming inert when it is looking at an
# object that is not a natural match for its next phase.
FOCUS_PHASE_BIAS = {
    "none": {
        "wander": 1.8, "jump": 1.5, "roll": 1.3, "zoomies": 1.5,
        "approach": 0.0, "chase": 0.0, "observe": 0.0,
        "prepare_jump_attack": 0.0, "inspect": 0.0, "cuddle": 0.0,
        "social_play": 0.0, "run_away": 0.0,
    },
    "cursor": {
        "wander": 0.45, "jump": 0.65, "roll": 0.45, "zoomies": 0.55,
        "approach": 1.7, "chase": 2.2, "observe": 1.5,
        "prepare_jump_attack": 1.9, "inspect": 1.25, "cuddle": 0.75,
        "social_play": 0.20, "run_away": 0.65,
    },
    # DC-62: the same pointer, to a spider that has been caught a few times.
    # Approach and chase drop away, run_away climbs, and observing it from a
    # distance goes up -- wary, not panicked.
    "cursor_wary": {
        "wander": 0.90, "jump": 0.55, "roll": 0.40, "zoomies": 0.55,
        "approach": 0.20, "chase": 0.15, "observe": 1.9,
        "prepare_jump_attack": 0.15, "inspect": 0.35, "cuddle": 0.10,
        "social_play": 0.30, "run_away": 2.6,
    },
    "prey": {
        "wander": 0.20, "jump": 1.0, "roll": 0.10, "zoomies": 0.65,
        "approach": 1.8, "chase": 2.8, "observe": 1.45,
        "prepare_jump_attack": 2.5, "inspect": 0.70, "cuddle": 0.05,
        "social_play": 0.05, "run_away": 0.10,
    },
    # Another spider, with nothing declared between them. Sociable by
    # default, which is what "creature" has always meant here.
    "creature": {
        "wander": 0.25, "jump": 1.0, "roll": 0.75, "zoomies": 0.75,
        "approach": 1.4, "chase": 0.65, "observe": 1.6,
        "prepare_jump_attack": 1.0, "inspect": 1.9, "cuddle": 2.3,
        "social_play": 2.8, "run_away": 0.20,
    },
    # DC-61: a declared ally, and a declared enemy. Both used to be
    # "creature", so a spider facing something that wanted to kill it was
    # weighing up a cuddle.
    #
    # These **tilt the odds; they do not override.** The owner chose that
    # deliberately: a cautious spider should still mostly back off and a bold
    # one should still mostly close, or every temperament looks the same in a
    # fight and the whole point of having temperaments goes. The numbers are
    # multipliers on a personality's own phase score, so a `hunter` profile
    # (chase 10, run_away 1) and an `escape` profile (chase 0, run_away 10)
    # come out of the same row facing opposite directions.
    "friend": {
        "wander": 0.30, "jump": 1.0, "roll": 0.85, "zoomies": 0.85,
        "approach": 1.6, "chase": 0.70, "observe": 1.3,
        "prepare_jump_attack": 0.55, "inspect": 1.5, "cuddle": 2.6,
        "social_play": 3.0, "run_away": 0.10,
    },
    "foe": {
        "wander": 0.10, "jump": 0.85, "roll": 0.20, "zoomies": 0.35,
        "approach": 1.5, "chase": 2.6, "observe": 1.2,
        "prepare_jump_attack": 2.4, "inspect": 0.25, "cuddle": 0.0,
        "social_play": 0.0, "run_away": 2.0,
    },
    "web": {
        "wander": 0.75, "jump": 0.15, "roll": 0.05, "zoomies": 0.10,
        "approach": 0.75, "chase": 0.10, "observe": 1.7,
        "prepare_jump_attack": 0.10, "inspect": 1.8, "cuddle": 0.05,
        "social_play": 0.05, "run_away": 0.25,
    },
}


@dataclass
class PhasePlan:
    """One selected behaviour phase and its remaining active time."""

    phase_id: str
    focus: str
    score: float
    duration: float
    remaining: float


def normalize_focus(value: str | None) -> str:
    """Normalize object-focus labels used by the creature and tests."""
    value = str(value or "none").strip().lower()
    aliases = {
        "mouse": "cursor",
        "pointer": "cursor",
        "fly": "prey",
        "target": "cursor",
        "spider": "creature",
        "mate": "creature",
        "ally": "friend",
        "teammate": "friend",
        "enemy": "foe",
        "silk": "web",
    }
    value = aliases.get(value, value)
    return value if value in FOCUS_PHASE_BIAS else "none"


def phase_id_for_state(state: str | None) -> str | None:
    """Return the high-level phase represented by an FSM state, if any."""
    return PHASE_STATE_IDS.get(str(state or ""))


class BehaviourPhaseScheduler:
    """Choose weighted, randomized behaviour periods for one creature.

    A deck is built at creation from one slot per score point.  This gives a
    score of zero a hard exclusion, makes ten the strongest and guaranteed
    presence in every deck, and still lets each individual spider receive a
    different order.  Focus bias is applied only when drawing from that deck.
    """

    def __init__(
        self,
        personality: dict | str | None,
        enabled_behaviours: Iterable[str],
        rng: random.Random | None = None,
    ) -> None:
        self.rng = rng or random.Random()
        self.scores = phase_scores_for(personality)
        self.duration_multiplier = phase_duration_multiplier_for(personality)
        self.enabled = {str(value).strip().lower() for value in enabled_behaviours}
        self.deck: list[str] = []
        self.current: PhasePlan | None = None
        self.last_phase_id: str | None = None
        self.cycles_completed = 0
        self._refill_deck()

    @property
    def phase_ids(self) -> tuple[str, ...]:
        """All phase ids known to the scheduler, in stable catalog order."""
        return tuple(BEHAVIOUR_PHASE_IDS)

    @property
    def current_phase_id(self) -> str | None:
        return self.current.phase_id if self.current is not None else None

    @property
    def remaining(self) -> float:
        return self.current.remaining if self.current is not None else 0.0

    def is_allowed(self, phase_id: str) -> bool:
        """Return whether score and enabled behaviour allow this phase."""
        phase_id = str(phase_id).strip().lower()
        return (
            phase_id in self.scores
            and self.scores[phase_id] > 0.0
            and phase_id in self.enabled
        )

    def tick(self, dt: float) -> None:
        """Advance only the scheduler clock; FSM timers remain its concern."""
        if self.current is not None:
            self.current.remaining = max(0.0, self.current.remaining - max(0.0, float(dt)))

    def cancel(self) -> None:
        """Cancel a phase when an external mode takes control."""
        if self.current is not None:
            self.last_phase_id = self.current.phase_id
        self.current = None

    def finish(self) -> None:
        """Finish the current period and allow the next deck draw."""
        if self.current is not None:
            self.last_phase_id = self.current.phase_id
        self.current = None

    def _refill_deck(self) -> None:
        slots: list[str] = []
        for phase_id in BEHAVIOUR_PHASE_IDS:
            if not self.is_allowed(phase_id):
                continue
            # A positive score always gets at least one chance. Ten therefore
            # has ten chances per cycle and is guaranteed to be represented.
            slots.extend([phase_id] * max(1, int(round(self.scores[phase_id]))))
        self.rng.shuffle(slots)
        self.deck = slots
        self.cycles_completed += 1

    def _duration_for(self, phase_id: str) -> float:
        low, high = PHASE_DURATION_RANGES.get(phase_id, (0.5, 1.5))
        score = max(0.0, min(10.0, float(self.scores.get(phase_id, 0.0))))
        score_scale = 0.72 + score / 10.0 * 0.56
        low *= self.duration_multiplier * score_scale
        high *= self.duration_multiplier * score_scale
        return self.rng.uniform(low, max(low, high))

    def choose(self, focus: str = "none") -> PhasePlan | None:
        """Draw the next phase using score frequency and focus usefulness."""
        focus = normalize_focus(focus)
        for _ in range(2):
            candidates: list[tuple[str, int, float]] = []
            bias_map = FOCUS_PHASE_BIAS[focus]
            candidate_ids = {
                phase_id
                for phase_id in self.deck
                if self.is_allowed(phase_id)
                and float(bias_map.get(phase_id, 0.25)) > 0.0
            }
            for index, phase_id in enumerate(self.deck):
                if not self.is_allowed(phase_id):
                    continue
                bias = max(0.0, float(bias_map.get(phase_id, 0.25)))
                if bias <= 0.0:
                    continue
                if phase_id == self.last_phase_id and len(candidate_ids) > 1:
                    continue
                candidates.append((phase_id, index, bias))
            if candidates:
                total = sum(weight for _phase_id, _index, weight in candidates)
                pick = self.rng.uniform(0.0, total)
                selected = candidates[-1]
                for candidate in candidates:
                    pick -= candidate[2]
                    if pick <= 0.0:
                        selected = candidate
                        break
                phase_id, deck_index, _bias = selected
                self.deck.pop(deck_index)
                score = float(self.scores[phase_id])
                duration = self._duration_for(phase_id)
                self.current = PhasePlan(phase_id, focus, score, duration, duration)
                return self.current
            self._refill_deck()
        return None

    def peek_score(self, focus: str = "none") -> float:
        """Preview the score ``choose(focus)`` would likely produce.

        Non-mutating: it does not touch the deck or ``self.current``. Used by
        ``arbiter.py`` (DC-18) to treat "the phase scheduler has something it
        wants to do" as one scored candidate among many instead of a chooser
        that runs first and wins outright, per that module's own docstring.
        Returns 0.0 when nothing currently eligible in the deck has a usable
        focus bias, matching ``choose()`` returning ``None`` in that case.
        """
        focus = normalize_focus(focus)
        bias_map = FOCUS_PHASE_BIAS[focus]
        best = 0.0
        seen: set[str] = set()
        for phase_id in self.deck:
            if phase_id in seen or not self.is_allowed(phase_id):
                continue
            seen.add(phase_id)
            bias = max(0.0, float(bias_map.get(phase_id, 0.25)))
            if bias <= 0.0:
                continue
            best = max(best, float(self.scores[phase_id]) * bias)
        return best

    def adopt(self, phase_id: str, focus: str = "none") -> float | None:
        """Adopt a phase entered by legacy FSM code and return its duration."""
        phase_id = str(phase_id).strip().lower()
        if not self.is_allowed(phase_id):
            self.cancel()
            return None
        duration = self._duration_for(phase_id)
        focus = normalize_focus(focus)
        self.current = PhasePlan(phase_id, focus, float(self.scores[phase_id]), duration, duration)
        return duration
