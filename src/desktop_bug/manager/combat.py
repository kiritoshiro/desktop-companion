"""Conflict: who hurts whom, and what happens when one of them goes down.

DC-22. Everything here is gated on ``CreatureManager.conflict_enabled``. That
gate is the whole safety property: with conflict off, nothing in the project
reduces any creature's hp, because this module owns the only calls that do.

Deliberately small. Taking a base (DC-44), fighting with skills rather than
by contact (DC-45) and coordinating a capture (DC-46) all build on the
primitives here and each get their own package; this one only has to make a
hostile encounter hurt, end, and be recovered from.
"""

from __future__ import annotations

import math

from ..creature.constants import KNOCKOUT_SECONDS
from .constants import log

# How close two foes must be, relative to their combined size, to trade hits.
CONTACT_REACH = 0.85
# A spider lands at most one hit this often, so a brawl reads as exchanges
# rather than as hp draining smoothly to zero.
ATTACK_INTERVAL = 0.85
# A downed spider recovers at its own base when its team has one, so a fight
# lost away from home costs the walk back as well.
REVIVE_AT_BASE_PAD = 34.0


class CombatMixin:
    """Contact damage between declared foes, and knock-out recovery."""

    def _resolve_combat(self, dt: float) -> None:
        """Trade hits between touching foes, then bring back anyone recovered.

        Recovery runs even when conflict has been switched off mid-session:
        leaving a spider face-down forever because the setting changed would
        be a worse outcome than the fight it lost.
        """
        self._revive_recovered(dt)
        if not self.conflict_enabled:
            return

        live = [
            creature for creature in self.creatures
            if not creature.knocked_out and not getattr(creature, "dragging", False)
        ]
        for index, attacker in enumerate(live):
            if attacker.attack_cooldown > 0.0:
                continue
            for defender in live[index + 1:]:
                if defender.knocked_out:
                    continue
                try:
                    if attacker.relation_to(defender) != "foe":
                        continue
                except Exception:
                    continue
                reach = (attacker.size + defender.size) * CONTACT_REACH
                if math.hypot(attacker.x - defender.x, attacker.y - defender.y) > reach:
                    continue
                self._trade_blow(attacker, defender)
                break

    def _trade_blow(self, attacker, defender) -> None:
        """One exchange: the aggressor hits, and is hit back if still standing.

        Both sides pay a cooldown even though only the aggressor chose to
        swing, so two foes in contact cannot each hit on every frame of the
        overlap and flatten one another in well under a second.
        """
        attacker.attack_cooldown = ATTACK_INTERVAL
        landed = defender.take_damage(attacker.damage, attacker)
        if landed <= 0.0:
            return
        if defender.knocked_out:
            log.info(
                "%s knocked out %s", attacker.display_name, defender.display_name,
            )
            return
        # The defender hits back, but only if it is not already mid-swing at
        # someone else -- otherwise being attacked is a free extra attack.
        if defender.attack_cooldown <= 0.0:
            defender.attack_cooldown = ATTACK_INTERVAL
            attacker.take_damage(defender.damage, defender)

    def _revive_recovered(self, dt: float) -> None:
        """Bring back anyone whose knock-out has run its course."""
        for creature in self.creatures:
            if not creature.knocked_out:
                continue
            if creature.knockout_timer > 0.0:
                continue
            creature.revive(self._revive_point(creature))

    def _revive_point(self, creature):
        """Where a spider comes back: its own base if its team holds one."""
        base_world = getattr(self, "base_world", None)
        if base_world is None:
            return None
        site = base_world._site_for_team(creature)
        if site is None:
            return None
        angle = creature.rng.uniform(0.0, math.tau)
        reach = site.radius + REVIVE_AT_BASE_PAD
        return (site.x + math.cos(angle) * reach, site.y + math.sin(angle) * reach)

    def knockout_seconds(self) -> float:
        """Exposed so the settings window and tests agree on the recovery time."""
        return KNOCKOUT_SECONDS
