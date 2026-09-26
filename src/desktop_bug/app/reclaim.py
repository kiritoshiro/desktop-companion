"""Reclaim the desktop: a raid fought on the frozen desktop itself.

The owner: *"we could make a mission section called reclaim the desktop and
explanation that you have no control of the desktop anymore. but of course by
leaving the raid it would retake the control of the screen."*

The overlay covers every screen with a picture of the desktop taken as the
raid starts (desktop_capture.py) and takes all the input: the desktop is
frozen. On it:

- an **infestation** on every screen (encounters.py, profile "reclaim"),
  guarded, sending raiders until destroyed;
- **spitters** whose acid burns spiders and melts the picture -- a window
  first, then the desktop behind it, then nothing (desktop_surface.py);
- **heavy spiders** crack the screen's glass when they land, bosses as they
  walk; enough cracks and a screen's glass shatters and falls away;
- **words** on the frozen screen your spiders eat as they walk over them,
  and spin into silk.

Destroy every infestation, then the Devourer that comes for you. If acid eats
too much of the desktop, the raid is lost. Win, lose or press Esc and leave:
the overlay closes and the real desktop, never touched, is yours again.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

from .adventure import PlayerController
from .desktop_surface import DesktopSurface
from .mission import MissionSite, TerritoryMission


@dataclass
class SilkThread:
    """Letters unravelling from an eaten word into the spider eating it."""
    x0: float
    y0: float
    spider: object
    age: float = 0.0


class ReclaimMission(TerritoryMission):
    ENCOUNTER = "reclaim"
    FREEZES_DESKTOP = True
    # The raid is lost when less than this share of the desktop is left.
    LOST_BELOW = 0.45
    # Acid holes, by the size of the splash.
    MELT_RADIUS = 30.0
    # Eating a word: seconds for one, and what it gives.
    EAT_SECONDS = 0.8
    WORD_HEAL = 2.0
    WORD_SILK = 1.0
    WORD_XP = 2
    INTRO_SECONDS = 7.0

    def __init__(self, manager, controls, layout, surface: DesktopSurface):
        self.surface = surface
        self.words_eaten = 0
        self.threads = []
        self.intro_time = self.INTRO_SECONDS
        self.guardian_called = False
        self.lost_desktop = False
        super().__init__(manager, controls, layout=layout)
        # The guardian of this map is a pouncing boss: its landings crack glass.
        self.announce("The desktop is frozen. Esc and Leave gives it back at once.")

    def restarted(self):
        return type(self)(self.manager, self.controls, self.layout,
                          DesktopSurface(self.surface.snapshot, find_text=True))

    # -- layout ------------------------------------------------------------
    def _build_sites(self, area):
        """One foothold for you on the main screen; the infestations are the
        director's, on every screen."""
        main = self.layout.primary
        x, y = main.anchor(0.13, 0.62, 110)
        return [MissionSite("home", "Last foothold", x, y, owned=True, screen=main.index)]

    def _opening_notice(self):
        count = self.layout.count
        where = "every screen" if count > 1 else "the screen"
        return f"Reclaim the desktop: destroy the nest on {where}. Walk over words to eat them for silk."

    def _opening_spawns(self):
        """Nothing beyond the infestations' own guards."""

    # -- rules -------------------------------------------------------------
    @property
    def objective(self):
        if self.state == "victory":
            return "VICTORY - the desktop is yours again"
        if self.state == "defeat":
            return ("DESKTOP LOST - the acid ate it" if self.lost_desktop
                    else "RAID ENDED - your spider has fallen")
        taken = sum(s.owned for s in self.outposts)
        if taken < len(self.outposts):
            return f"01 / Destroy the nests  ({taken}/{len(self.outposts)})"
        return "02 / Defeat the Devourer"

    def update(self, dt):
        self.surface.update(dt)
        self.intro_time = max(0.0, self.intro_time - dt)
        for thread in self.threads:
            thread.age += dt
        self.threads = [t for t in self.threads if t.age < 0.7]
        super().update(dt)
        if self.state != "active":
            return
        self._eat_words(min(0.05, max(0.0, dt)))
        if self.surface.integrity < self.LOST_BELOW:
            self.lost_desktop = True
            self.state = "defeat"
            self.player.clear_keys()
            self.finish(won=False)
            return
        if self.guardian is not None and self.guardian.dead:
            self.state = "victory"
            self.hero.gain_experience(100, "desktop reclaimed")
            self.player.clear_keys()
            self.finish(won=True)

    def _spawning(self, dt):
        """The Devourer comes once every nest is destroyed and their raiders dead."""
        if self.guardian is not None:
            return
        if not self.outposts or not all(s.owned for s in self.outposts):
            return
        if any(not a.creature.dead and a.role != "ally" for a in self.actors):
            return
        if self.guardian_warning is None:
            self.guardian_warning = 4.0
            self.announce("Something vast is crawling up from under the desktop. 4 seconds.")
        self.guardian_warning = max(0.0, self.guardian_warning - dt)
        if self.guardian_warning > 0:
            return
        # It rises on the screen furthest from the hero, and comes for them.
        here = self.layout.screen_at(self.hero.x, self.hero.y)
        lair = max(self.layout.screens, key=lambda s: (s.index != here.index,
                                                       math.dist(s.centre, (self.hero.x, self.hero.y))))
        x, y = lair.anchor(0.5, 0.45, 120)
        if math.hypot(self.hero.x - x, self.hero.y - y) < 180:
            x, y = lair.anchor(0.8, 0.3, 120)
        self.guardian = self._spawn("guardian", (x, y), raider=True)
        if self.guardian is not None:
            for actor in self.actors:
                if actor.creature is self.guardian:
                    actor.style = "hunter"     # it pounces; every landing cracks the glass
            self.announce(f"{self.guardian.display_name} has risen on {lair.name}. Its landings crack the glass.")

    def _eat_words(self, dt):
        """The hero and companions eat the words they walk over."""
        spiders = [(self.hero, self.player)] + [(a.creature, a) for a in self.actors
                                                if a.role == "ally" and not a.creature.dead]
        for spider, control in spiders:
            if spider.dead or spider.airborne:
                continue
            word = self.surface.word_near(spider.x, spider.y, spider.size * 0.9)
            if word is None:
                continue
            if self.surface.eat(word, dt / self.EAT_SECONDS):
                self.words_eaten += 1
                spider.heal(self.WORD_HEAL)
                control.silk = min(control.silk_capacity, control.silk + self.WORD_SILK)
                spider.gain_experience(self.WORD_XP, "words eaten")
                cx, cy = word.centre
                self.threads.append(SilkThread(cx, cy, spider))
                if spider is self.hero and self.words_eaten in (1, 10, 25, 50, 100):
                    self.announce(f"{self.words_eaten} word{'s' if self.words_eaten != 1 else ''} "
                                  "eaten and spun into silk")

    def silk_capacity_for(self, spider) -> int:
        return PlayerController.SILK_CAPACITY

    # -- the desktop takes damage -------------------------------------------
    def on_acid(self, x, y):
        melted = self.surface.melt(x, y, self.MELT_RADIUS)
        if melted == "window" and self.rng.random() < 0.25:
            self.announce("Acid is melting through the windows")

    def on_landing(self, c):
        if c.size_scale >= self.HEAVY_LANDING:
            force = min(1.0, 0.25 + (c.size_scale - 1.0) * 0.9)
            if self.surface.crack(c.x, c.y, force):
                self.announce(f"The glass of {self.layout.screen_at(c.x, c.y).name} shattered")

    def on_heavy_step(self, c):
        if self.surface.crack(c.x, c.y, 0.16):
            self.announce(f"The glass of {self.layout.screen_at(c.x, c.y).name} shattered")
