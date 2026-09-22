"""A base raises new spiders (DC-55).

Since DC-47 a beaten spider dies for good, so a session could only ever lose
spiders: the colony shrank until the desktop went quiet and only a relaunch
refilled it. The owner chose the option that had been sitting unanswered in
the project's open questions -- *"yes lets implement that having a base it
raises a new spiders. however it should be capped at 5 spiders per team."*

The rules, and why each one is where it is:

* **A base spends food, not time.** The resource loop DC-21 built already
  turns hunting into building; this makes it turn hunting into numbers too,
  which is what makes losing a base expensive rather than merely untidy.
* **Food is split.** A base banks 30% of what it is fed into a larder that
  only growth can spend, and the rest into the pool building spends. Without
  the split, building consumes every scrap until a base is finished and no
  colony would ever raise anything: a full base costs 120 food and a colony
  earns roughly 14 a minute.
* **Five per team, living.** The owner's cap. Counted over living spiders, so
  a colony that loses two can replace two.
* **A half-dug base is enough.** Raising nothing until a base is finished
  would mean raising nothing for the first ten minutes, which is most of a
  session.
* **New spiders are tarantulas**, with a random temperament and a random
  palette, per the owner: the tarantula is the model the project is now built
  around, and a random *palette* rather than seven random colours is what
  keeps a raised spider looking like an animal (see `content/palettes`).
"""

from __future__ import annotations

import math

from ..content.palettes import random_palette

# The owner's number.
TEAM_POPULATION_CAP = 5
# What one spider costs its colony. Roughly two and a half minutes of a
# working colony's whole income, against a base that costs 120.
RAISE_FOOD_COST = 32.0
# How long a committed spider takes to appear. Long enough that it reads as
# something the base did rather than as a spider blinking into existence.
RAISE_SECONDS = 8.0
# How much of a base has to exist before it can raise anything.
RAISE_MIN_COMPLETION = 0.35
# Which model a base raises. The owner: "for now default new spiders are
# tarantulas, as for what personalities they are they should be random".
RAISED_MODEL_ID = "tarantula"


class ColonyGrowthMixin:
    """Bases spend banked food to raise spiders, up to a cap per team."""

    def _living_team_members(self, team_id: str) -> int:
        return sum(1 for creature in self.creatures
                   if not getattr(creature, "dead", False)
                   and str(getattr(creature.progression, "team_id", "neutral")) == team_id)

    def _update_colony_growth(self, dt: float) -> None:
        base_world = getattr(self, "base_world", None)
        if base_world is None or not base_world.bases:
            return
        timers = self._raise_timers
        for site in list(base_world.bases.values()):
            if site.team_id == "neutral":
                continue
            if self._living_team_members(site.team_id) >= TEAM_POPULATION_CAP:
                # Full. Drop any part-finished timer rather than holding one
                # that would deliver a sixth spider the moment somebody dies.
                timers.pop(site.id, None)
                continue
            if site.completion < RAISE_MIN_COMPLETION or site.larder < RAISE_FOOD_COST:
                timers.pop(site.id, None)
                continue
            elapsed = timers.get(site.id, 0.0) + max(0.0, float(dt))
            if elapsed < RAISE_SECONDS:
                timers[site.id] = elapsed
                continue
            timers.pop(site.id, None)
            # Paid for on arrival, not on commitment: a timer lost to a reload
            # would otherwise have charged a colony for a spider it never got.
            site.larder -= RAISE_FOOD_COST
            self._raise_spider_at(site)

    def _raise_spider_at(self, site) -> None:
        model = self.models.get(RAISED_MODEL_ID)
        if model is None:
            # A checkout without the model this names should not crash a
            # colony; it simply cannot grow.
            return
        personality = self._valid_personality_for_model(model)
        if personality is None:
            return
        creature = self._create_creature(
            model,
            personality,
            index=len(self.creatures),
            pos=self._birth_point(site),
            color_overrides=random_palette(self._rng),
            team_id=site.team_id,
            job_id="none",
        )
        self.creatures.append(creature)
        self._refresh_neighbor_links()
        self.save_runtime_state()

    def _birth_point(self, site) -> tuple:
        """Just off the centre of the base, inside the dug earth."""
        angle = self._rng.uniform(0.0, math.tau)
        radius = site.radius * self._rng.uniform(0.20, 0.55)
        return (
            max(20.0, min(self.screen_w - 20.0, site.x + math.cos(angle) * radius)),
            max(20.0, min(self.screen_h - 20.0, site.y + math.sin(angle) * radius * 0.7)),
        )
