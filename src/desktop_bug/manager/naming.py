"""Naming a spider, and the hover that drives its name label.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations

from typing import Optional

from ..creature import Creature
from ..state.progression import RELATIONS




class NamingMixin:
    """Naming a spider, and the hover that drives its name label."""

    # ------------------------------------------------------------------
    # Naming / hover
    # ------------------------------------------------------------------
    def update_hover(self, mx: float, my: float) -> Optional[Creature]:
        """Mark the spider under the cursor as hovered (drives the name label)."""
        hovered = self.creature_at(mx, my)
        for creature in self.creatures:
            creature._hovered = creature is hovered
        return hovered

    def clear_hover(self) -> None:
        for creature in self.creatures:
            creature._hovered = False

    def set_naming_enabled(self, enabled: bool) -> str:
        self.naming_enabled = bool(enabled)
        if not self.naming_enabled:
            self.clear_hover()
        return f"Right-click naming turned {'on' if self.naming_enabled else 'off'}."

    def set_always_show_names(self, enabled: bool) -> str:
        self.always_show_names = bool(enabled)
        return f"Spider names {'always shown' if self.always_show_names else 'shown on hover only'}."

    # ------------------------------------------------------------------
    # Scene-wide label switches. A spider could already be given its level or
    # its health bar one at a time through the inspector, which is tedious
    # for a colony; these turn it on for everybody at once. They are kept on
    # the manager and pushed onto each creature rather than written into a
    # spider's saved progression, so switching them off does not wipe a pin
    # somebody set deliberately on one spider.
    # ------------------------------------------------------------------

    def _apply_label_overrides(self) -> None:
        for creature in self.creatures:
            creature.force_show_level = self.always_show_levels
            creature.force_show_health = self.always_show_health
            creature.force_show_xp = getattr(self, "always_show_xp", False)
            creature.force_show_stamina = getattr(self, "always_show_stamina", False)

    def set_always_show_levels(self, enabled: bool) -> str:
        self.always_show_levels = bool(enabled)
        self._apply_label_overrides()
        state = "on every spider" if self.always_show_levels else "only where it was pinned"
        return f"Level now shown {state}."

    def set_always_show_health(self, enabled: bool) -> str:
        self.always_show_health = bool(enabled)
        self._apply_label_overrides()
        state = "on every spider" if self.always_show_health else "only where it was pinned"
        return f"Health bar now shown {state}."

    def set_always_show_xp(self, enabled: bool) -> str:
        self.always_show_xp = bool(enabled)
        self._apply_label_overrides()
        return f"XP bar {'shown on every spider' if self.always_show_xp else 'hidden'}."

    def set_always_show_stamina(self, enabled: bool) -> str:
        self.always_show_stamina = bool(enabled)
        self._apply_label_overrides()
        return f"Stamina bar {'shown on every spider' if self.always_show_stamina else 'hidden'}."

    def name_creature(self, creature: Creature, name: str) -> str:
        if creature is None:
            return "No spider there to name."
        creature.set_name(name)
        self.save_runtime_state()
        if creature.name:
            return f"Named this spider \u201c{creature.name}\u201d."
        return "Cleared this spider's name."

    def set_creature_level_pin(self, creature: Creature, enabled: bool) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_level_label_pinned(enabled)
        self.save_runtime_state()
        return f"Level display {'pinned above' if enabled else 'removed from'} this spider's name."

    def set_creature_health_pin(self, creature: Creature, enabled: bool) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_health_label_pinned(enabled)
        self.save_runtime_state()
        return f"Health bar {'pinned above' if enabled else 'removed from'} this spider."

    def set_creature_team(self, creature: Creature, team_id: str) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_team(team_id)
        self.save_runtime_state()
        return f"Assigned this spider to team {creature.progression.team_id!r}."

    def set_creature_relation(self, creature: Creature, other: Creature, relation: str) -> str:
        if creature is None or other is None or creature is other:
            return "No pair of spiders selected."
        relation = str(relation).strip().lower()
        if relation not in RELATIONS:
            return "Relation must be friend, neutral, or foe."
        left_id = str(getattr(other, "progression_id", other.index))
        right_id = str(getattr(creature, "progression_id", creature.index))
        # Store pair choices in both directions so UI and future combat checks
        # cannot disagree about who is a friend or foe.
        creature.progression.relation_overrides[left_id] = relation
        other.progression.relation_overrides[right_id] = relation
        self.save_runtime_state()
        return f"Set {creature.display_name} and {other.display_name} to {relation}."

    def equip_creature_item(self, creature: Creature, item_id: str) -> str:
        if creature is None:
            return "No spider there to equip."
        ok, message = creature.equip_item(item_id)
        if ok:
            self.save_runtime_state()
        return message

    def unequip_creature_item(self, creature: Creature, slot: str) -> str:
        if creature is None:
            return "No spider there to unequip."
        ok, message = creature.unequip_item(slot)
        if ok:
            self.save_runtime_state()
        return message

    def unlock_creature_ability(self, creature: Creature, ability_id: str) -> str:
        if creature is None:
            return "No spider there to unlock."
        ok, message = creature.unlock_progression_ability(ability_id)
        if ok:
            self.save_runtime_state()
        return message
