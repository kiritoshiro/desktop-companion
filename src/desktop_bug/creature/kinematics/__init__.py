"""Leg IK, gait scheduling and grounded-locomotion body solving.

Split from a single 2,226-line module into mixins by concern (DC-43).
"""

from .core import KinematicsMixin
from .legstate import LegState

__all__ = ["KinematicsMixin", "LegState"]
