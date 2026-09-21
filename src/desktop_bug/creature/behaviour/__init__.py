"""The personality state machine, composed from its parts.

DC-11 split this out of creature.py; DC-43 split the 2,610-line result
into mixins by concern. ``BehaviourMixin`` is what ``Creature`` composes.
"""

from .core import BehaviourMixin

__all__ = ["BehaviourMixin"]
