"""Where each map's buildings stand, and which buildings it has.

The owner: *"missions are all the same. make different configuration of the
buildings. and in different places ... also utilise multiple screens for
missions. and build some other buildings that would have some other functions
too."*

Every raid map lists its buildings here. A building has a place on the main
screen as fractions of it, or on the **far** screen -- the other monitor --
with a fallback place on the main screen for players with only one. Some
buildings exist only when there is a second screen.

Building kinds and what holding one does, for whichever side holds it:

- ``home``: heals and refills silk (yours from the start);
- ``food``: heals from a supply; ``silk``: more silk and fast refills;
- ``hatchery``: sends enemy reinforcements until you seal it;
- ``nest``: the enemy's home; claim it last, after its guardian;
- ``venom``  -- Venom den: the holder's side deals 20% more damage;
- ``lookout`` -- Lookout: the holder's side shoots silk and acid 30% farther;
- ``amber``  -- Amber mine: mines amber for you while you hold it;
- ``nursery`` -- Nursery: hatches spiderlings for its holder, two at a time.

Pure data, no Qt.
"""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Placement:
    kind: str
    name: str
    fx: float
    fy: float
    guards: tuple[str, ...] = ()
    reserves: int = 0
    owned: bool = False
    far: bool = False                        # on the other screen when there is one
    alt: tuple[float, float] | None = None   # its main-screen place without one; None: skipped
    supply: float = 180.0                    # healing a Food cache holds


# What holding a building gives, for the status line under it.
EFFECTS = {
    "venom": "+20% damage",
    "lookout": "+30% silk and acid range",
    "amber": "Mines amber",
    "nursery": "Hatches spiderlings",
}

KINDS_WITH_EFFECTS = tuple(EFFECTS)

LAYOUTS = {
    # The first map keeps its familiar layout; a second screen adds a mine.
    "territory": (
        Placement("home", "Home burrow", .16, .57, owned=True),
        Placement("food", "Food cache", .39, .29, guards=("guard",)),
        Placement("silk", "Silk loom", .39, .70, guards=("weaver",)),
        Placement("hatchery", "Hatchery", .64, .47, guards=("hunter",), reserves=6),
        Placement("nest", "Thorn nest", .83, .30),
        Placement("amber", "Amber mine", .50, .55, guards=("guard",), far=True),
    ),
    # Ember hollow: home high on the left, a venom den to fight over, the
    # nest across on the other screen.
    "ember": (
        Placement("home", "Home burrow", .10, .22, owned=True),
        Placement("venom", "Venom den", .28, .78, guards=("guard",)),
        Placement("food", "Food cache", .46, .20, guards=("hunter",)),
        Placement("hatchery", "Hatchery", .60, .66, guards=("hunter",), reserves=7),
        Placement("lookout", "Lookout", .80, .80, guards=("weaver",)),
        Placement("nest", "Hollow nest", .60, .40, far=True, alt=(.88, .26)),
        Placement("amber", "Amber mine", .25, .70, guards=("guard",), far=True),
    ),
    # Obsidian deep: home low on the left; the hatchery and nest both on the
    # far screen, a nursery in the middle.
    "obsidian": (
        Placement("home", "Home burrow", .12, .82, owned=True),
        Placement("silk", "Silk loom", .28, .30, guards=("weaver",)),
        Placement("nursery", "Nursery", .50, .68, guards=("guard",)),
        Placement("amber", "Amber mine", .66, .20, guards=("guard",)),
        Placement("hatchery", "Hatchery", .30, .60, guards=("hunter", "weaver"), reserves=8,
                  far=True, alt=(.74, .58)),
        Placement("nest", "Obsidian nest", .75, .30, far=True, alt=(.90, .84)),
        Placement("lookout", "Lookout", .86, .45, guards=("weaver",)),
    ),
    # Queen of thorns: home at the bottom middle, every kind of building
    # round it, the queen's nest across on the far screen.
    "queen": (
        Placement("home", "Home burrow", .50, .92, owned=True),
        Placement("food", "Food cache", .18, .60, guards=("guard",)),
        Placement("silk", "Silk loom", .82, .60, guards=("weaver",)),
        Placement("venom", "Venom den", .30, .16, guards=("hunter",)),
        Placement("lookout", "Lookout", .70, .16, guards=("weaver",)),
        Placement("nursery", "Nursery", .50, .45, guards=("guard",)),
        Placement("hatchery", "Hatchery", .30, .62, guards=("hunter", "guard"), reserves=9,
                  far=True, alt=(.10, .22)),
        Placement("nest", "Queen's nest", .66, .38, far=True, alt=(.90, .22)),
        Placement("amber", "Amber mine", .20, .25, guards=("guard",), far=True),
    ),
}


def layout_for(map_id: str) -> tuple[Placement, ...]:
    return LAYOUTS.get(map_id, LAYOUTS["territory"])
