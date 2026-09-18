"""A creature's per-tick view of the shared world (DC-17, finding C1).

``Creature.update()`` builds one ``Perception`` for itself at the start of
every tick, so the state-machine code below it asks "is there a web to
repair nearby" through this object instead of reaching into the manager's
shared ``WebWorld``/``FlyWorld`` through ``self.web_world``/``self.fly_world``
directly. Each query still executes lazily, under the same skill/cooldown
gates the calling code already applied, so introducing this costs nothing
extra per frame -- it is a facade, not a pre-computed scan of every web on
screen.

Scope, disclosed rather than silently narrowed: this covers the *decision*
reads -- "what could I do" -- that ``_consider_special_actions`` and
``_can_shoot_web`` used to make directly. It deliberately does not cover
*executing* a decision once made (``claim_site``, ``adopt``, ``claim_repair``,
``release_repair``, ``abandon``, ``launch_web_shot``) or the per-frame
validity check on a web a creature already holds a reference to (the
``web not in self.web_world.webs`` guards in the WeaveApproach/Weave/
RepairApproach/Repair/WebApproach/WebWalk state updates). Those are shared-
state mutations and liveness checks on an object the creature is already
committed to, not perception of the wider world, and a frozen snapshot taken
once at the top of the tick cannot safely stand in for either: two creatures
can still race for the same site the instant a decision is acted on. Folding
those into a single arbiter that owns both perceiving and acting is DC-18's
job, not this one.

``base_world`` does not appear here because nothing in ``Creature`` reads it
directly today -- the base only ever reads *from* creatures
(``BaseWorld.update(dt, self.creatures)``), never the other way around, so
there was nothing to decouple. Prey targeting (``self._prey``,
``self._hunting_prey``) and neighbour relations (``self.neighbors``,
``Creature.relation_to``) are likewise left alone: they already arrive as
manager-injected, world-object-free state rather than as a `self.fly_world`
read, and the actual remaining coupling there is ``CreatureManager._drive_hunt``
reaching in and overwriting a creature's state from outside, which is the
specific problem DC-18's arbiter is designed to replace.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class Perception:
    """Read-only, per-tick facade over one creature's web/fly world references."""

    _web_world: Optional[object]
    _fly_world: Optional[object]
    _creature: object

    @property
    def has_web_world(self) -> bool:
        return self._web_world is not None

    def repairable_web(self, max_dist: float):
        """The nearest torn, finished web worth mending, if any."""
        if self._web_world is None:
            return None
        return self._web_world.find_repairable_web(self._creature, max_dist=max_dist)

    def adoptable_web(self, max_dist: float):
        """The nearest abandoned, unfinished web worth taking over, if any."""
        if self._web_world is None:
            return None
        return self._web_world.find_adoptable_web(self._creature, max_dist=max_dist)

    def walkable_web(self, max_dist: float):
        """The nearest finished web worth walking onto to bounce-test, if any."""
        if self._web_world is None:
            return None
        return self._web_world.find_walkable_web(self._creature, max_dist=max_dist)

    def intact_web_count(self) -> int:
        """How many complete, undamaged webs currently exist."""
        if self._web_world is None:
            return 0
        return self._web_world.intact_complete_count()

    def can_shoot_prey_web(self) -> bool:
        """Whether a fly world exists to fire a trapping shot through."""
        return self._fly_world is not None


def build_perception(creature) -> Perception:
    """Snapshot the world references ``creature`` can currently query.

    Cheap by design: it captures references, not results, so it can be built
    unconditionally at the top of every ``Creature.update()`` tick without
    scanning anything itself.
    """
    return Perception(creature.web_world, creature.fly_world, creature)
