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

# ---------------------------------------------------------------------------
# DC-66: fighting is worth something.
#
# The owner: *"also give xp for damaging enemies other things or killing
# foes."* Until now eating a fly was the only thing in the project that earned
# any XP at all, so a spider could win every fight on the desktop and stay at
# level one, while one that never left the nest levelled up on flies. Since
# DC-57 a level also spends itself on the skill tree, which means combat now
# feeds straight into what a spider can *do* -- a veteran actually becomes
# more dangerous.
#
# Paid per point of damage that lands rather than per blow, so a heavy hit is
# worth more than a glancing one and armour matters at both ends.
DAMAGE_XP_PER_POINT = 4.0
# The killing blow, on top of the damage that caused it. Deliberately below
# FEED_XP_REWARD: hunting is the safe living and should stay the backbone of a
# colony's economy, while a fight is a gamble that can also cost a level --
# DC-47 made death permanent and `_forget_progression` drops the profile.
KILL_XP_REWARD = 90
# How long a background progression change may sit unsaved. Short enough that a
# crash loses at most a few seconds of XP, long enough that a hungry colony does
# not rewrite the state file every frame.
RUNTIME_STATE_FLUSH_SECONDS = 5.0


def _clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))
