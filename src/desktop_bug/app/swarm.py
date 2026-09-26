"""Fly swarm levels: catch the flies before they scatter, and before rivals do.

The owner: *"also add levels with flies. and also utilise multiple screens
for missions."*

Fly nests stand on every screen and release flies all over the monitors. The
flies are the Companion's own (world/flies.py): they buzz, panic from
spiders, and are pinned by silk. Your spiders catch them -- a pinned fly is
eaten by walking to it; a flying one only by a spider right on top of it --
and every fly heals, refills a little silk and pays XP. Rival spiders hunt
the same flies. Catch enough before the time runs out, and before the rivals
catch as many; a fly left alone too long flies off the screens for good.
Rival dens on the other screens (encounters.py, profile "swarm") keep sending
hunters until you take them.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

from .mission import MissionSite, TerritoryMission
from ..world.flies import Fly


@dataclass(frozen=True)
class SwarmRules:
    target: int          # flies to catch
    seconds: float       # before the swarm is gone
    rivals: int          # rival hunters at the start
    max_flies: int       # flies on the screens at once
    spawn_every: float   # seconds between flies, per nest
    escape_after: float  # seconds a free fly stays before leaving


RULES = {
    "swarm": SwarmRules(target=18, seconds=150.0, rivals=2, max_flies=10, spawn_every=3.2, escape_after=26.0),
    "storm": SwarmRules(target=30, seconds=170.0, rivals=4, max_flies=16, spawn_every=2.4, escape_after=20.0),
}


class FlySwarmMission(TerritoryMission):
    ENCOUNTER = "swarm"
    EAT_REACH_PINNED = 1.7     # body sizes, for a fly stuck in silk
    EAT_REACH_FLYING = 0.8     # body sizes, for one still in the air
    FLY_HEAL = 6.0
    FLY_SILK = 1.0
    FLY_XP = 5

    def __init__(self, manager, controls, area=None, layout=None):
        self.caught = 0
        self.rival_caught = 0
        self.escaped = 0
        self.fly_age = {}
        self.nest_clock = 0.0
        super().__init__(manager, controls, area=area, layout=layout)
        self.rules = RULES.get(self.map_info.id, RULES["swarm"])
        self.time_left = self.rules.seconds
        world = manager.fly_world
        world.playfield = self.playfield
        world.enabled = False
        world.use_spawner = False
        for _ in range(4):
            self._release_fly()

    # -- layout ------------------------------------------------------------
    def _build_sites(self, area):
        """Your burrow on the main screen and a fly nest on every screen."""
        main = self.layout.primary
        x, y = main.anchor(0.14, 0.62, 110)
        sites = [MissionSite("home", "Home burrow", x, y, owned=True, screen=main.index)]
        spots = ((0.62, 0.35), (0.40, 0.72), (0.78, 0.70))
        for screen in self.layout.screens:
            fx, fy = spots[screen.index % len(spots)]
            nx, ny = screen.anchor(fx, fy, 120)
            # Owned, so it is never "captured": a fly nest is nobody's.
            sites.append(MissionSite("flynest", "Fly nest", nx, ny, owned=True, screen=screen.index))
        return sites

    def _opening_notice(self):
        where = "every screen" if self.layout.multi else "the screen"
        return (f"{self.map_info.title}: flies swarm from nests on {where}. Pin them with silk, "
                "then eat them - before the rivals do.")

    def _opening_spawns(self):
        rules = RULES.get(self.map_info.id, RULES["swarm"])
        main = self.layout.primary
        for i in range(rules.rivals):
            x, y = main.anchor(0.55 + 0.1 * (i % 3), 0.25 + 0.2 * (i // 3), 100)
            self._spawn("hunter", (x, y), raider=False)

    @property
    def nests(self):
        return [s for s in self.sites if s.kind == "flynest"]

    # -- rules -------------------------------------------------------------
    @property
    def objective(self):
        if self.state == "victory":
            return "VICTORY - the swarm is yours"
        if self.state == "defeat":
            if self.hero.dead:
                return "RAID ENDED - your spider has fallen"
            return "DEFEAT - the swarm got away"
        rules = self.rules if hasattr(self, "rules") else RULES["swarm"]
        left = max(0, int(getattr(self, "time_left", rules.seconds)))
        return (f"Catch flies {self.caught}/{rules.target}  ·  {left // 60}:{left % 60:02d} left"
                f"  ·  rivals {self.rival_caught}/{rules.target}")

    def actor_goal(self, actor):
        """Rivals hunt the nearest fly; your companions hunt pinned ones."""
        flies = [f for f in self.manager.fly_world.flies if f.alive or f.trapped]
        if not flies:
            return None
        c = actor.creature
        if actor.role == "ally":
            flies = [f for f in flies if f.trapped]
            if not flies:
                return None
        fly = min(flies, key=lambda f: math.hypot(f.x - c.x, f.y - c.y))
        if math.hypot(fly.x - c.x, fly.y - c.y) > 520:
            return None
        return fly.x, fly.y

    def update(self, dt):
        super().update(dt)
        if self.state != "active":
            return
        dt = min(0.05, max(0.0, dt))
        self.time_left = max(0.0, self.time_left - dt)
        self._flies(dt)
        if self.caught >= self.rules.target:
            self._end(True, "The swarm is caught")
        elif self.rival_caught >= self.rules.target or self.time_left <= 0:
            self._end(False, "The swarm got away")

    def _end(self, won, text):
        self.state = "victory" if won else "defeat"
        if won:
            self.hero.gain_experience(80, "swarm caught")
        self.announce(text)
        self.player.clear_keys()
        self.finish(won=won)

    def _spawning(self, dt):
        """No hatchery and no guardian here: the flies are the whole fight."""

    def _release_fly(self):
        world = self.manager.fly_world
        nests = self.nests
        if not nests or sum(1 for f in world.flies if not f.eaten) >= self.rules.max_flies:
            return None
        nest = self.rng.choice(nests)
        angle = self.rng.uniform(-math.pi, math.pi)
        fly = Fly(nest.x + math.cos(angle) * 30, nest.y + math.sin(angle) * 20,
                  self.manager.screen_w, self.manager.screen_h, heading=angle, scale=1.0, rng=self.rng)
        fly.playfield = self.playfield
        world.flies.append(fly)
        self.fly_age[id(fly)] = 0.0
        return fly

    def _flies(self, dt):
        world = self.manager.fly_world
        self.nest_clock -= dt * max(1, len(self.nests))
        if self.nest_clock <= 0:
            self.nest_clock = self.rules.spawn_every
            self._release_fly()
        spiders = [c for c in self.manager.creatures if not c.dead]
        for fly in list(world.flies):
            fly.update(dt, spiders, self.manager.web_world)
            if fly.eaten or fly.removable:
                continue
            age = self.fly_age.get(id(fly), 0.0) + (0.0 if fly.trapped else dt)
            self.fly_age[id(fly)] = age
            if age >= self.rules.escape_after:
                fly.state = "eaten"          # gone: off the screens
                self.escaped += 1
                continue
            eater = self._eater(fly, spiders)
            if eater is not None:
                self._eat(fly, eater)
        world.flies = [f for f in world.flies if not f.eaten and not f.removable]

    def _eater(self, fly, spiders):
        for spider in spiders:
            if spider.airborne:
                continue
            reach = spider.size * (self.EAT_REACH_PINNED if fly.trapped else self.EAT_REACH_FLYING) + fly.size
            if math.hypot(spider.x - fly.x, spider.y - fly.y) <= reach:
                return spider
        return None

    def _eat(self, fly, spider):
        fly.begin_eaten()
        spider.heal(self.FLY_HEAL)
        if spider.progression.team_id == "adventurers":
            self.caught += 1
            control = self.player if spider is self.hero else next(
                (a for a in self.actors if a.creature is spider), None)
            if control is not None:
                control.silk = min(control.silk_capacity, control.silk + self.FLY_SILK)
            spider.gain_experience(self.FLY_XP, "fly caught")
            if self.caught in (1, 5, 10) or self.caught == self.rules.target - 3:
                self.announce(f"{self.caught} of {self.rules.target} flies caught")
        else:
            self.rival_caught += 1
            if self.rival_caught == self.rules.target - 3:
                self.announce("The rivals are 3 flies from taking the swarm!")
