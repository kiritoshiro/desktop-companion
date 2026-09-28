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
- **heavy spiders** crack the glass when they land, bosses as they walk:
  one window at a time, and when a window's glass gives it cracks right
  across and falls away, showing the window behind it; on the bare desktop
  the cracks wear down the **desktop's health** (a bar on the HUD), as acid
  eating it does;
- **words** on the frozen screen, which the enemy devours letter by letter
  to heal -- the destroyers eat the desktop's text, never your spiders (the
  owner: "my spider shouldn't eat the letters off the screen. it should be
  done by enemy spiders only as they are the evil ones destroying").

Destroy every infestation, then the Devourer that comes for you. If the
desktop's health runs out it breaks: a blue screen instead of DEFEAT (the
owner: "then the windows desktop breaks and ... put then blue screen of death
instead of defeated ... if that happens nothing what was gained in that
mission is kept. or lost"), and back to the Adventure window. Win, lose or
press Esc and leave: the overlay closes and the real desktop, never touched,
is yours again.
"""
from __future__ import annotations

import copy
import math
from dataclasses import dataclass

from .adventure import PlayerController
from .adventure_profile import save_profile
from .desktop_surface import DesktopSurface
from .mission import MissionSite, TerritoryMission


@dataclass
class SilkThread:
    """Letters torn from a devoured word into the enemy eating it."""
    x0: float
    y0: float
    spider: object
    age: float = 0.0


class ReclaimMission(TerritoryMission):
    ENCOUNTER = "reclaim"
    FREEZES_DESKTOP = True
    # The blue screen: the glass falls first, then it shows this long.
    CRASH_GLASS_SECONDS = 1.2
    CRASH_SCREEN_SECONDS = 7.0
    # Acid holes, by the size of the splash.
    MELT_RADIUS = 30.0
    # An enemy devouring a word: seconds for one, and the health it gains.
    EAT_SECONDS = 1.1
    WORD_HEAL = 3.0
    # How far from its post a defender strays for a word; raiders roam.
    WORD_LEASH = 280.0
    WORD_ROAM = 600.0
    INTRO_SECONDS = 7.0

    def __init__(self, manager, controls, layout, surface: DesktopSurface):
        self.surface = surface
        self.words_eaten = 0
        self.threads = []
        self.intro_time = self.INTRO_SECONDS
        self.guardian_called = False
        self.lost_desktop = False
        super().__init__(manager, controls, layout=layout)
        # The profile as the raid began: a crash puts it back as it was.
        self.start_profile = copy.deepcopy(self.profile)
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
        return f"Reclaim the desktop: destroy the nest on {where} before they devour its words and melt it."

    def _opening_spawns(self):
        """Nothing beyond the infestations' own guards."""

    # -- rules -------------------------------------------------------------
    @property
    def objective(self):
        if self.state == "victory":
            return "VICTORY - the desktop is yours again"
        if self.state == "crashed":
            return "DESKTOP BROKEN - nothing gained, nothing lost"
        if self.state == "defeat":
            return "RAID ENDED - your spider has fallen"
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
        if self.surface.desktop_hp <= 0.0:
            self.crash()
            return
        if self.guardian is not None and self.guardian.dead:
            self.state = "victory"
            for player in self.players:
                player.creature.gain_experience(100, "desktop reclaimed")
            self.clear_player_keys()
            self.finish(won=True)

    def crash(self) -> None:
        """The desktop's health is gone: it breaks, a blue screen follows, and
        the profile goes back to how it was when the raid began -- nothing
        found, earned or lost here is kept, and no result is recorded."""
        if self.state == "crashed":
            return
        self.surface.break_desktop()
        self.lost_desktop = True
        self.state = "crashed"
        self.end_clock = 0.0
        self.clear_player_keys()
        self.profile = copy.deepcopy(self.start_profile)
        if save_profile(self.profile, self.progress_path):
            self.saved = True
            self.save_error = ""
        else:
            self.save_error = "The profile could not be restored."

    @property
    def blue_screen(self) -> float:
        """0 before the blue screen shows, then how long it has shown."""
        if self.state != "crashed":
            return 0.0
        return max(0.0, self.end_clock - self.CRASH_GLASS_SECONDS)

    @property
    def end_screen_done(self) -> bool:
        if self.state == "crashed":
            return self.end_clock >= self.CRASH_GLASS_SECONDS + self.CRASH_SCREEN_SECONDS
        return super().end_screen_done

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

    def actor_goal(self, actor):
        """With no one to fight, an enemy goes after the nearest word: a
        defender only near its post, a raider anywhere."""
        if actor.role == "ally":
            return None
        c = actor.creature
        if actor.raider:
            ax, ay, reach = c.x, c.y, self.WORD_ROAM
        else:
            (ax, ay), reach = actor.defend_point, self.WORD_LEASH
        best, best_d = None, reach
        for word in self.surface.living_words():
            wx, wy = word.centre
            if math.hypot(wx - ax, wy - ay) > reach:
                continue
            d = math.hypot(wx - c.x, wy - c.y)
            if d < best_d:
                best, best_d = (wx, wy), d
        return best

    def _eat_words(self, dt):
        """Enemies devour the words they reach; your spiders never eat them."""
        for actor in self.actors:
            spider = actor.creature
            if actor.role == "ally" or spider.dead or spider.airborne:
                continue
            word = self.surface.word_near(spider.x, spider.y, spider.size * 0.9)
            if word is None:
                continue
            if self.surface.eat(word, dt / self.EAT_SECONDS):
                self.words_eaten += 1
                spider.heal(self.WORD_HEAL)
                cx, cy = word.centre
                self.threads.append(SilkThread(cx, cy, spider))
                if self.words_eaten in (1, 10, 25, 50, 100):
                    plural = "s" if self.words_eaten != 1 else ""
                    self.announce(f"The enemy has devoured {self.words_eaten} word{plural} of your desktop")

    def silk_capacity_for(self, spider) -> int:
        return PlayerController.SILK_CAPACITY

    # -- the desktop takes damage -------------------------------------------
    def on_acid(self, x, y):
        melted = self.surface.melt(x, y, self.MELT_RADIUS)
        if melted == "window" and self.rng.random() < 0.25:
            self.announce("Acid is melting through the windows")

    def on_landing(self, c):
        if c.size_scale >= self.HEAVY_LANDING:
            self._cracked(self.surface.crack(c.x, c.y, min(1.0, 0.25 + (c.size_scale - 1.0) * 0.9)))

    def on_heavy_step(self, c):
        self._cracked(self.surface.crack(c.x, c.y, 0.16))

    def _cracked(self, result):
        if result == "window":
            left = len(self.surface.intact_panes()) - 1
            more = f" - {left} window{'s' if left != 1 else ''} left" if left > 0 else " - the desktop is next"
            self.announce(f"A window's glass gave way{more}")
        elif result == "desktop":
            self.announce("The desktop's glass is breaking")
