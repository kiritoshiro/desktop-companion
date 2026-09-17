"""The `Creature` package: see core.py for why it is split this way (DC-11)."""

from __future__ import annotations

from .constants import (
    GAIT_LABELS,
    JOB_PREEMPTING_STATES,
    JOB_STATES,
    VALID_GAIT_STYLES,
    normalize_gait_style,
    smootherstep,
)
from .core import Creature
from .kinematics import LegState

__all__ = [
    "Creature",
    "LegState",
    "JOB_STATES",
    "JOB_PREEMPTING_STATES",
    "VALID_GAIT_STYLES",
    "GAIT_LABELS",
    "normalize_gait_style",
    "smootherstep",
]
