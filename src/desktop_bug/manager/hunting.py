"""Which spider chases which fly, and the killing bite.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations

import math

from ..creature import Creature
from ..creature.constants import HUNT_BUSY_STATES
from ..support.math_utils import distance
from ..world.jobs import FLY_CATCH_RESOURCE_AMOUNT


from .constants import (
    FEED_XP_REWARD,
    _clamp,
)


class HuntingMixin:
    """Which spider chases which fly, and the killing bite."""

    # ------------------------------------------------------------------
    # Fly hunting: which spider chases which fly, and the killing bite.
    #
    # DC-18 (C1): the manager still owns *target selection* below -- which
    # fly (if any) a spider locks onto -- the same manager-injected state
    # DC-17's docstring already treats as acceptable. Acting on that lock
    # used to be CreatureManager._drive_hunt, which called
    # enter_approach/enter_chase and wrote target_x/speed/motion_paused on
    # the creature directly, every frame, from outside -- the outside-in
    # overwrite the plan calls out. That reaction moved onto the creature
    # itself as BehaviourMixin._pursue_prey, reading the candidate through
    # Perception.prey() and using the creature's own seeded rng.
    # ------------------------------------------------------------------

    def _creature_can_hunt(self, creature: Creature) -> bool:
        if creature.airborne:
            return False
        if self._is_fully_hidden(creature):
            return False
        if getattr(creature, "_feed_cooldown", 0.0) > 0.0:
            return False
        # A spider being carried can still lock onto a fly it spots and web it,
        # it just cannot run it down; so dragging does not block acquisition.
        # DC-22/DC-47: being dead does block it, unlike being carried.
        if creature.dead:
            return False
        if creature.dragging or creature is self.dragged_creature:
            return True
        if creature.state in HUNT_BUSY_STATES:
            return False
        # Needs some way to actually close on prey.
        return (creature.has_skill("chase") or creature.has_skill("approach")
                or creature.has_skill("jump"))

    def _hunt_sense_radius(self, creature: Creature, trapped: bool) -> float:
        reaction = float(creature.personality.get("reaction_radius", 360))
        sense = _clamp(reaction * 0.9, 240.0, 900.0)
        if trapped:
            # A fly thrashing on a web telegraphs itself: the wider radius is the
            # "the web is moving, come and get it" signal to nearby spiders.
            sense = sense * 1.5 + 160.0
        return sense

    def _update_prey_targets(self, dt: float) -> None:
        # Tick down the per-spider hunt cooldowns.
        for creature in self.creatures:
            creature._feed_cooldown = max(0.0, getattr(creature, "_feed_cooldown", 0.0) - dt)
            creature._pounce_cooldown = max(0.0, getattr(creature, "_pounce_cooldown", 0.0) - dt)
            creature._trap_shot_cooldown = max(0.0, getattr(creature, "_trap_shot_cooldown", 0.0) - dt)
            creature._prey_recheck = max(0.0, getattr(creature, "_prey_recheck", 0.0) - dt)

        flies = [f for f in self.fly_world.flies if f.alive and not f.eaten and not f.dragging]
        if not self.flies_enabled or not flies:
            for creature in self.creatures:
                prey = getattr(creature, "_prey", None)
                if prey is not None:
                    prey.hunters.discard(creature)
                    creature._prey = None
            return

        for creature in self.creatures:
            prey = getattr(creature, "_prey", None)

            # Drop a target that died, was eaten, or that we can no longer hunt.
            if prey is not None and (prey not in flies or not self._creature_can_hunt(creature)):
                prey.hunters.discard(creature)
                creature._prey = None
                prey = None

            # Keep the current target if it is still reasonably close (hysteresis),
            # so spiders do not jitter between flies every frame.
            if prey is not None:
                keep = self._hunt_sense_radius(creature, prey.trapped) * 1.6
                if distance(creature.x, creature.y, prey.x, prey.y) > keep:
                    prey.hunters.discard(creature)
                    creature._prey = None
                    prey = None

            if not self._creature_can_hunt(creature):
                continue

            # Re-pick periodically (or immediately when we have no target).
            if prey is not None and creature._prey_recheck > 0.0:
                continue
            creature._prey_recheck = self._rng.uniform(0.2, 0.4)

            best = None
            best_score = -1e18
            for fly in flies:
                sense = self._hunt_sense_radius(creature, fly.trapped)
                d = distance(creature.x, creature.y, fly.x, fly.y)
                if d > sense and fly is not prey:
                    continue
                # Higher score = better target.  Prefer trapped flies (easy and
                # signalled), closer flies, and flies fewer spiders already chase.
                score = (sense - d)
                if fly.trapped:
                    score += 500.0
                claimers = len(fly.hunters - {creature})
                score -= claimers * 140.0
                if fly is prey:
                    score += 90.0  # mild stickiness toward the current target
                if score > best_score:
                    best_score = score
                    best = fly

            if best is not prey:
                if prey is not None:
                    prey.hunters.discard(creature)
                if best is not None:
                    best.hunters.add(creature)
                creature._prey = best

    def _creature_focus(self, creature: Creature, mx: float, my: float):
        """Return (focus_x, focus_y, hunting) for this spider's update call."""
        prey = getattr(creature, "_prey", None)
        if prey is not None and prey.alive and not prey.eaten:
            return prey.x, prey.y, True
        return mx, my, False

    def _resolve_fly_catches(self) -> None:
        for fly in self.fly_world.flies:
            if not fly.alive or fly.eaten or fly.dragging:
                continue
            # Keep a tether visually attached to a living trapper that is still
            # closing in, so the silk does not dangle from empty space.
            if fly.trapped and fly.tether_from is not None:
                trapper = getattr(fly, "_trapper", None)
                if trapper is not None and trapper in self.creatures and not trapper.dragging:
                    fly.tether_from = (
                        trapper.x + math.cos(trapper.heading) * trapper.size * 0.6,
                        trapper.y + math.sin(trapper.heading) * trapper.size * 0.6,
                    )

            best = None
            best_d = 1e18
            for creature in self.creatures:
                if creature.dragging or creature.state == "Feed":
                    continue
                if self._is_fully_hidden(creature):
                    continue
                catch = creature.size * 1.5 + fly.size + 4.0
                d = distance(creature.x, creature.y, fly.x, fly.y)
                if d <= catch and d < best_d:
                    best_d = d
                    best = creature
            if best is not None:
                # Leave a little pile of fading remains where the fly was eaten.
                self.fly_world.add_remains((fly.x, fly.y), scale=fly.size / 9.0)
                fly.begin_eaten()
                # This is the single authoritative feeding hook.  It runs only
                # after the fly transitions to ``eaten`` so repeated collision
                # checks cannot award duplicate XP.
                self.award_feed_xp(best, FEED_XP_REWARD, "fly")
                # DC-21: every eaten fly tops up its eater's team food a
                # little, regardless of job -- except a Hunter's own catch,
                # which already credits the larger, deliberate
                # HUNTER_CARRY_FOOD_AMOUNT once it carries this same catch
                # home (jobs.py::_update_hunter's fed-state rising edge).
                # Crediting both here would double-count one catch: this
                # incidental top-up is for every *other* job's catch, not an
                # addition to the Hunter's.
                base_world = getattr(self, "base_world", None)
                if base_world is not None and getattr(best, "job_id", "none") != "hunter":
                    base_world.credit_team_food(
                        getattr(best.progression, "team_id", None), FLY_CATCH_RESOURCE_AMOUNT,
                    )
                # Free every hunter that was locked onto this fly.
                for hunter in list(fly.hunters):
                    hunter._prey = None
                    hunter._hunting_prey = False
                fly.hunters.clear()
                best.enter_feed(fly.x, fly.y)
                best._feed_cooldown = self._rng.uniform(0.6, 1.2)

