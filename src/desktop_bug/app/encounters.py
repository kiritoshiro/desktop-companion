"""Where extra enemies come from, by the kind of mission and the screens there are.

The owner: *"allow to move to them and even create additional enemy based on
the type of mission, so this function must be implemented smartly so that it
could be utilised in various later scenarios."*

A mission names an **encounter profile** ("raid", "reclaim", ...). The
director reads it with the monitor layout and the map's tier and says:

- which **outposts** to place -- one on each extra screen, and for some
  mission kinds on the main screen too -- with their defenders;
- when an outpost sends **reinforcements** and who they are.

The mission owns the spiders; the director only plans. A new scenario is a new
``EncounterProfile`` in ``PROFILES``, not new spawning code. Pure data and
small rules, no Qt.
"""
from __future__ import annotations

import random
from dataclasses import dataclass, field

from ..world.screen_layout import ScreenLayout

# Screen area, in pixels, that earns one more defender: a 4K monitor holds
# more than a laptop panel.
AREA_PER_EXTRA_DEFENDER = 2_600_000.0


@dataclass(frozen=True)
class EncounterProfile:
    kind: str
    outpost_kind: str                   # the site kind (its art and rules)
    outpost_names: tuple[str, ...]      # one per outpost, cycled
    defenders: tuple[str, ...]          # roles guarding every outpost
    tier_defenders: tuple[str, ...]     # one more per map tier above the first, cycled
    wave_roles: tuple[str, ...]         # who an outpost sends, cycled
    wave_every: float                   # seconds between an outpost's waves
    reserves: int                       # waves an outpost holds
    warning: float = 3.0                # seconds of "emerging" before a wave
    on_main_screen: int = 0             # outposts on the main screen as well
    # Where on a screen outposts stand, as fractions of it, tried in order.
    spots: tuple[tuple[float, float], ...] = ((0.55, 0.45), (0.30, 0.70), (0.78, 0.68), (0.25, 0.30))


PROFILES = {
    # The territory maps: the main screen keeps its five buildings; every
    # other screen gets an optional outpost that raids you until taken.
    "raid": EncounterProfile(
        "raid", "outpost", ("Far burrow", "Border web", "Distant den"),
        defenders=("guard", "weaver"), tier_defenders=("hunter", "guard", "weaver"),
        wave_roles=("hunter", "guard"), wave_every=38.0, reserves=3),
    # Reclaim the desktop: every screen is infested, the main one included,
    # and the infestations spit acid at the desktop.
    "reclaim": EncounterProfile(
        "reclaim", "infestation", ("Acid nest", "Glass burrow", "Pixel hive", "Void den"),
        defenders=("spitter", "hunter"), tier_defenders=("spitter", "guard", "hunter"),
        wave_roles=("spitter", "hunter", "guard"), wave_every=30.0, reserves=4,
        on_main_screen=1, spots=((0.72, 0.42), (0.40, 0.62), (0.62, 0.72), (0.30, 0.35))),
}


@dataclass
class OutpostPlan:
    screen: int
    x: float
    y: float
    kind: str
    name: str
    defenders: tuple[str, ...]
    reserves: int


@dataclass
class _Clock:
    time: float
    warning: float = 0.0


@dataclass
class EncounterDirector:
    profile: EncounterProfile
    layout: ScreenLayout
    tier: int = 1
    rng: random.Random = field(default_factory=random.Random)

    def __post_init__(self):
        self._clocks: dict[int, _Clock] = {}
        self._waves_sent: dict[int, int] = {}

    # -- placing -----------------------------------------------------------
    def outposts(self, avoid: list[tuple[float, float]] = (), clearance: float = 240.0) -> list[OutpostPlan]:
        """The outposts for this layout: one per extra screen, plus the main
        screen's share. ``avoid`` holds points (the mission's own buildings)
        an outpost must keep ``clearance`` from."""
        p = self.profile
        plans = []
        screens = [(s, 1) for s in self.layout.others]
        if p.on_main_screen:
            screens.insert(0, (self.layout.primary, p.on_main_screen))
        taken = list(avoid)
        for screen, count in screens:
            for _ in range(count):
                spot = self._free_spot(screen, taken, clearance)
                if spot is None:
                    continue
                taken.append(spot)
                name = p.outpost_names[len(plans) % len(p.outpost_names)]
                plans.append(OutpostPlan(screen.index, spot[0], spot[1], p.outpost_kind, name,
                                         self.defenders_for(screen), p.reserves))
        for index in range(len(plans)):
            # Staggered, so outposts do not all send their waves at once.
            self._clocks[index] = _Clock(p.wave_every * (0.6 + 0.25 * index))
        return plans

    def _free_spot(self, screen, taken, clearance):
        margin = 110.0
        for fx, fy in self.profile.spots:
            x, y = screen.anchor(fx, fy, margin)
            if all((x - tx) ** 2 + (y - ty) ** 2 >= clearance ** 2 for tx, ty in taken):
                return x, y
        return None

    def defenders_for(self, screen) -> tuple[str, ...]:
        p = self.profile
        roles = list(p.defenders)
        for i in range(max(0, self.tier - 1)):
            roles.append(p.tier_defenders[i % len(p.tier_defenders)])
        extra = int(screen.rect.w * screen.rect.h // AREA_PER_EXTRA_DEFENDER)
        for i in range(extra):
            roles.append(p.tier_defenders[(self.tier + i) % len(p.tier_defenders)])
        return tuple(roles)

    # -- reinforcements ----------------------------------------------------
    def update(self, dt: float, outposts) -> list[tuple[int, str]]:
        """``outposts``: the mission's live outpost sites, in plan order, each
        with ``owned``, ``reserves`` and ``warning``. Returns (outpost index,
        role) for every wave due now, and sets the warnings that precede
        them. The mission spawns them (and may decline when the arena is full,
        which returns the wave: the outpost keeps its reserve)."""
        p = self.profile
        due = []
        for index, site in enumerate(outposts):
            clock = self._clocks.setdefault(index, _Clock(p.wave_every))
            if site.owned or site.reserves <= 0:
                continue
            if clock.warning > 0:
                clock.warning = max(0.0, clock.warning - dt)
                site.warning = clock.warning
                if clock.warning <= 0:
                    sent = self._waves_sent.get(index, 0)
                    due.append((index, p.wave_roles[sent % len(p.wave_roles)]))
                    self._waves_sent[index] = sent + 1
                    clock.time = p.wave_every
                continue
            clock.time -= dt
            if clock.time <= 0:
                clock.warning = p.warning
                site.warning = p.warning
        return due

    def retry(self, index: int) -> None:
        """A wave the mission could not spawn is tried again shortly."""
        clock = self._clocks.get(index)
        if clock is not None:
            clock.time = 2.0
            self._waves_sent[index] = max(0, self._waves_sent.get(index, 1) - 1)
