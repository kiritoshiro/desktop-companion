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

from ..world.carcass import CARCASS_FOOD_AMOUNT, carcass_for, eaters_near
from .constants import log

# How close two foes must be, relative to their combined size, to trade hits.
CONTACT_REACH = 0.85
# A spider lands at most one hit this often, so a brawl reads as exchanges
# rather than as hp draining smoothly to zero.
ATTACK_INTERVAL = 0.85
# DC-47: a beaten spider dies. What it leaves behind, and for how long,
# lives in `world/carcass.py`.


class CombatMixin:
    """Contact damage between declared foes, and knock-out recovery."""

    def _resolve_combat(self, dt: float) -> None:
        """Trade hits between touching foes, then clear away the beaten.

        The carcass pass runs even when conflict has been switched off
        mid-session: remains already on the desktop should still be eaten and
        gone rather than lying there because a setting changed.
        """
        self._update_carcasses(dt)
        self._bury_the_dead()
        if not self.conflict_enabled:
            return

        live = [
            creature for creature in self.creatures
            if not creature.dead and not getattr(creature, "dragging", False)
        ]
        for index, attacker in enumerate(live):
            if attacker.attack_cooldown > 0.0:
                continue
            for defender in live[index + 1:]:
                if defender.dead:
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
        if defender.dead:
            log.info("%s killed %s", attacker.display_name, defender.display_name)
            return
        # The defender hits back, but only if it is not already mid-swing at
        # someone else -- otherwise being attacked is a free extra attack.
        if defender.attack_cooldown <= 0.0:
            defender.attack_cooldown = ATTACK_INTERVAL
            attacker.take_damage(defender.damage, defender)

    def _bury_the_dead(self) -> None:
        """Take the beaten out of the colony and leave remains where they fell.

        Done as a sweep at the end of the tick rather than the moment hp hits
        zero, so nothing iterating over the colony has the list change under
        it mid-frame.
        """
        fallen = [creature for creature in self.creatures if creature.dead]
        if not fallen:
            return
        for creature in fallen:
            self.carcasses.append(carcass_for(creature))
            self._forget_progression(creature)
            if self.dragged_creature is creature:
                self.dragged_creature = None
                self.dragging = False
            for other in self.creatures:
                if getattr(other, "job_alert_target", None) is creature:
                    other.job_alert_target = None
                if getattr(other, "_prey", None) is creature:
                    other._prey = None
        self.creatures = [c for c in self.creatures if not c.dead]
        self._refresh_neighbor_links()
        self._refresh_render_order()
        self.mark_runtime_state_dirty()

    def _forget_progression(self, creature) -> None:
        """Drop a dead spider's saved profile, so death costs its level too.

        Without this the next launch would hand the slot's saved entry back
        to whichever spider filled the gap, and dying would cost nothing that
        survives a restart.
        """
        key = str(getattr(creature, "progression_id", "") or "")
        if key:
            self._progression_states.pop(key, None)

    def _update_carcasses(self, dt: float) -> None:
        """Let the remains be eaten, and credit whoever is eating them."""
        if not self.carcasses:
            return
        base_world = getattr(self, "base_world", None)
        left = []
        for carcass in self.carcasses:
            eaters = eaters_near(carcass, self.creatures)
            finished = carcass.update(dt, eaters)
            if not finished:
                left.append(carcass)
                continue
            # A spider that ate its way through a body fed its colony doing
            # it, in the same banked food DC-21 spends on building.
            if eaters and base_world is not None:
                for creature in self.creatures:
                    if eaters_near(carcass, [creature]):
                        base_world.credit_team_food(
                            getattr(creature.progression, "team_id", None),
                            CARCASS_FOOD_AMOUNT / max(1, eaters),
                        )
        self.carcasses = left

    def carcass_dirty_rects(self):
        """Footprints the overlay must repaint while remains are on screen."""
        return [carcass.footprint() for carcass in self.carcasses]

    def render_carcasses(self, painter) -> None:
        for carcass in self.carcasses:
            carcass.draw(painter)
