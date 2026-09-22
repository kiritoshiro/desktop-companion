"""Fly settings and nests, and the XP a spider earns for eating one.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations


from ..creature import Creature


from .constants import (
    FEED_XP_REWARD,
)


class PreyMixin:
    """Fly settings and nests, and the XP a spider earns for eating one."""

    # ------------------------------------------------------------------
    # Flies
    # ------------------------------------------------------------------
    def award_feed_xp(self, creature: Creature, amount: int = FEED_XP_REWARD,
                      source: str = "feed") -> list[str]:
        """The one place XP is awarded, whatever earned it.

        Named for flies because they were the only source when it was
        written; since DC-66 combat pays through it too (damage landed, and
        finishing a foe). Everything routes here because this is what marks
        runtime state dirty, so a level is a level however it was won and all
        of them survive a restart.
        """
        if creature is None or creature not in self.creatures:
            return []
        events = creature.gain_experience(amount, reason=source)
        self.mark_runtime_state_dirty()
        return events

    def apply_fly_settings(self, settings: dict | None) -> None:
        """Read the optional ``flies`` block of a preset's settings."""
        if not isinstance(settings, dict):
            return
        flies = settings.get("flies")
        if not isinstance(flies, dict):
            return
        if "enabled" in flies:
            self.flies_enabled = bool(flies.get("enabled"))
        try:
            if "min_interval" in flies:
                self.fly_min_interval = max(0.3, float(flies.get("min_interval")))
            if "max_interval" in flies:
                self.fly_max_interval = max(0.4, float(flies.get("max_interval")))
        except Exception:
            self.warnings.append("Preset settings.flies interval was invalid; using defaults.")
        if self.fly_max_interval < self.fly_min_interval:
            self.fly_max_interval = self.fly_min_interval
        try:
            if "max_flies" in flies:
                self.fly_max = max(0, min(40, int(flies.get("max_flies"))))
        except Exception:
            self.warnings.append("Preset settings.flies max_flies was invalid; using default.")
        if "spawner" in flies:
            self.flies_spawner = bool(flies.get("spawner"))
        self.fly_world.configure(
            enabled=self.flies_enabled,
            min_interval=self.fly_min_interval,
            max_interval=self.fly_max_interval,
            max_flies=self.fly_max,
            scale=self.size_scale,
            use_spawner=self.flies_spawner,
        )

    def set_flies_enabled(self, enabled: bool) -> str:
        self.flies_enabled = bool(enabled)
        self.fly_world.configure(enabled=self.flies_enabled)
        if not self.flies_enabled:
            self.fly_world.clear()
            for creature in self.creatures:
                creature._prey = None
                creature._hunting_prey = False
        return f"Flies turned {'on' if self.flies_enabled else 'off'}."

    def set_fly_spawn_interval(self, min_interval: float, max_interval: float) -> str:
        self.fly_min_interval = max(0.3, float(min_interval))
        self.fly_max_interval = max(self.fly_min_interval, float(max_interval))
        self.fly_world.configure(min_interval=self.fly_min_interval,
                                 max_interval=self.fly_max_interval)
        return f"Flies now spawn every {self.fly_min_interval:.0f}-{self.fly_max_interval:.0f}s."

    def set_fly_max(self, max_flies: int) -> str:
        self.fly_max = max(0, min(40, int(max_flies)))
        self.fly_world.configure(max_flies=self.fly_max)
        return f"Up to {self.fly_max} flies at once."

    def set_fly_rate_preset(self, name: str) -> str:
        """Quick spawn-rate presets used by the tray menu."""
        presets = {
            "off": (False, self.fly_min_interval, self.fly_max_interval, self.fly_max),
            "sparse": (True, 9.0, 18.0, 3),
            "normal": (True, 4.0, 9.0, 6),
            "swarm": (True, 1.0, 3.0, 14),
        }
        key = str(name).strip().lower()
        if key not in presets:
            return "Unknown fly setting."
        enabled, lo, hi, cap = presets[key]
        self.flies_enabled = bool(enabled)
        if key != "off":
            self.fly_min_interval = float(lo)
            self.fly_max_interval = float(hi)
            self.fly_max = int(cap)
        self.fly_world.configure(
            enabled=self.flies_enabled,
            min_interval=self.fly_min_interval,
            max_interval=self.fly_max_interval,
            max_flies=self.fly_max,
        )
        if not self.flies_enabled:
            self.fly_world.clear()
            for creature in self.creatures:
                creature._prey = None
                creature._hunting_prey = False
            return "Flies turned off."
        labels = {"sparse": "Sparse", "normal": "Normal", "swarm": "Swarm"}
        return f"Flies set to {labels.get(key, key)}."

    def release_fly(self) -> str:
        """Spawn one fly immediately, regardless of the spawn timer."""
        return self.fly_world.spawn_now()

    def set_fly_spawner_enabled(self, enabled: bool) -> str:
        self.flies_spawner = bool(enabled)
        self.fly_world.configure(use_spawner=self.flies_spawner)
        if self.flies_spawner:
            return "Flies now emerge from a movable nest."
        return "Flies now drift in from the screen edges."

    def add_fly_spawner(self) -> str:
        self.flies_spawner = True
        self.fly_world.add_spawner()
        return f"Added a nest ({len(self.fly_world.spawners)} on screen)."

    def reset_fly_spawner(self) -> str:
        self.flies_spawner = True
        self.fly_world.reset_spawners()
        return "Nest reset to one in its default spot."

