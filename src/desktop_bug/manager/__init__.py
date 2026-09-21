"""The manager: it owns the creatures and every world they share.

Split from a single 2,194-line module into mixins by concern (DC-43),
following the same composition the ``creature`` package already uses.
"""

from .core import CreatureManager

__all__ = ["CreatureManager"]
