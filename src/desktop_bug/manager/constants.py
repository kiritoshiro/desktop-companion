"""Values and helpers the manager's mixins share.

Split out of ``manager.py`` by DC-43 so the mixins can import them without
importing each other.
"""

from __future__ import annotations

from ..support.logging_setup import get_logger

log = get_logger("manager")

RANDOM_MODEL_ID = "__random_model__"
RANDOM_PERSONALITY_ID = "__random_personality__"
FEED_XP_REWARD = 110
# How long a background progression change may sit unsaved. Short enough that a
# crash loses at most a few seconds of XP, long enough that a hungry colony does
# not rewrite the state file every frame.
RUNTIME_STATE_FLUSH_SECONDS = 5.0


def _clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))
