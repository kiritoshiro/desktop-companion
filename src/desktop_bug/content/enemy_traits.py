"""What each enemy kind is like, so it can be recognised by its looks.

The owner: *"based on the way it looks make them unique in the abilities they
have. ones might be fast, others much health, others with abilities. and embed
also personalities based on the looks, so that it would be recognisable. some
defenders, defend the outpost most and don't chase much. some aggressive and
chase forever."*

Every kind has body numbers (speed, health, bite, shell), a **temperament**
(how far it strays and how long it chases) and a few **abilities**. Pure data;
the mission brain (app/mission.py) and the squad tactics (world/tactics.py)
read it. The same traits suit the strategy mode later, where a team is a team
whichever side it is on.

Temperaments:

- ``sentinel``  -- holds its post; fights only what comes close and goes back.
- ``territorial`` -- defends a wider ring round its post.
- ``stalker``   -- follows a foe far from its post before giving up.
- ``aggressive`` -- once it has seen a foe it chases it anywhere, for ever.
- ``skirmisher`` -- darts in, bites, and springs back out.
- ``ambusher``  -- waits motionless until a foe is right on it, then pounces.
- ``sniper``    -- holds a spot at range and shoots; backs off if rushed.

Abilities (all built from the player's own rules -- bite, silk, acid, jump):

- ``pounce`` (a leap onto its target), ``web`` (shoots silk), ``spit`` (acid);
- ``charge``: a burst of speed at a foe it has just spotted;
- ``regen``: slowly heals while it fights;
- ``enrage``: bites harder and runs faster once badly hurt;
- ``hit_and_run``: backs away after every strike;
- ``rally``: nearby allies bite harder while it lives;
- ``thick_shell``: extra armour.
"""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Temperament:
    id: str
    label: str
    leash: float           # how far from its post it fights
    notice: float          # how close a foe must come before it reacts
    chase_forever: bool = False
    give_up: float = 0.0   # seconds out of reach before it goes home (0: never)


TEMPERAMENTS = {
    "sentinel": Temperament("sentinel", "Sentinel: holds its post", leash=170.0, notice=200.0, give_up=1.5),
    "territorial": Temperament("territorial", "Territorial", leash=260.0, notice=260.0, give_up=3.0),
    "stalker": Temperament("stalker", "Stalker: follows you far", leash=520.0, notice=340.0, give_up=8.0),
    "aggressive": Temperament("aggressive", "Aggressive: chases for ever", leash=10_000.0, notice=380.0,
                              chase_forever=True),
    "skirmisher": Temperament("skirmisher", "Skirmisher: hit and run", leash=320.0, notice=300.0, give_up=4.0),
    "ambusher": Temperament("ambusher", "Ambusher: waits, then strikes", leash=220.0, notice=150.0, give_up=2.0),
    "sniper": Temperament("sniper", "Sniper: shoots from range", leash=300.0, notice=320.0, give_up=3.0),
}

ABILITIES = ("pounce", "web", "spit", "charge", "regen", "enrage", "hit_and_run", "rally", "thick_shell")

# What a squad role means to the tactics (world/tactics.py).
SQUAD_ROLES = ("tank", "melee", "fast", "ranged", "support")


@dataclass(frozen=True)
class EnemyTraits:
    temperament: str
    abilities: tuple[str, ...] = ()
    speed: float = 1.0       # x the walking pace
    health: float = 1.0      # x health
    bite: float = 1.0        # x damage
    shell: float = 0.0       # armour added
    squad_role: str = "melee"
    style: str | None = None  # "hunter", "weaver", "spitter", "guard": how it fights; None: by its role
    look: str = ""            # the one-line reason, for the map editor and tests

    @property
    def temper(self) -> Temperament:
        return TEMPERAMENTS[self.temperament]

    def has(self, ability: str) -> bool:
        return ability in self.abilities


TRAITS: dict[str, EnemyTraits] = {
    # Red and lean: fast and reckless, it never lets go.
    "redback_raider": EnemyTraits("aggressive", ("charge", "enrage"), speed=1.30, health=0.80, bite=1.05,
                                  squad_role="fast", style="hunter",
                                  look="red and lean: fast, reckless, chases for ever"),
    # A grey wolf spider: a patient hunter that follows far and leaps.
    "ash_wolf": EnemyTraits("stalker", ("pounce", "charge"), speed=1.15, health=1.0, bite=1.0,
                            squad_role="melee", style="hunter",
                            look="grey wolf spider: follows far, then leaps"),
    # Small and bright: darts in and springs away.
    "jumping_skirmisher": EnemyTraits("skirmisher", ("pounce", "hit_and_run"), speed=1.25, health=0.70,
                                      bite=0.90, squad_role="fast", style="hunter",
                                      look="small and bright: darts in, springs back"),
    # Long legs: keeps its distance and shoots silk.
    "harvest_stalker": EnemyTraits("sniper", ("web",), speed=1.05, health=0.80, bite=0.85,
                                   squad_role="ranged", style="weaver",
                                   look="long legs: keeps its distance and shoots silk"),
    # Thick-bodied: sits by its burrow, hits very hard, hard to kill.
    "trapdoor_brute": EnemyTraits("ambusher", ("thick_shell", "pounce"), speed=0.75, health=1.80, bite=1.30,
                                  shell=3.0, squad_role="tank", style="guard",
                                  look="thick-bodied: waits by its burrow, hits hard"),
    # Pale and cave-born: spits acid and slowly mends.
    "pale_cave": EnemyTraits("territorial", ("spit", "regen"), speed=0.95, health=1.10, bite=0.90,
                             squad_role="ranged", style="spitter",
                             look="pale cave dweller: spits acid, slowly heals"),
    # Crab-shaped and armoured: a wall in front of what it guards.
    "crab_reaver": EnemyTraits("sentinel", ("thick_shell", "charge"), speed=0.85, health=1.55, bite=1.10,
                               shell=4.0, squad_role="tank", style="guard",
                               look="crab-armoured: a wall in front of its post"),
    # A thorned orb-weaver: shoots silk and rallies the others.
    "thorn_weaver": EnemyTraits("sniper", ("web", "rally"), speed=0.95, health=0.95, bite=0.90,
                                squad_role="support", style="weaver",
                                look="thorned orb-weaver: shoots silk, rallies the others"),
    # Bosses.
    "ember_matriarch": EnemyTraits("aggressive", ("charge", "enrage", "rally"), speed=1.05, health=1.0,
                                   bite=1.10, squad_role="tank",
                                   look="ember matriarch: charges and rages when hurt"),
    "ivory_regent": EnemyTraits("territorial", ("thick_shell", "regen", "charge"), speed=0.9, health=1.15,
                                shell=5.0, squad_role="tank",
                                look="ivory crab regent: armoured and self-mending"),
    "thorn_crown": EnemyTraits("stalker", ("web", "rally", "enrage"), speed=1.0, health=1.05, bite=1.05,
                               squad_role="support",
                               look="thorn crown: silk, a war cry, and fury"),
}

# A spider with no kind (an old model, an editor spec without one) fights by
# its role, with the role's usual temper.
ROLE_TRAITS = {
    "guard": EnemyTraits("sentinel", (), squad_role="tank"),
    "hunter": EnemyTraits("stalker", ("pounce",), squad_role="melee"),
    "weaver": EnemyTraits("sniper", ("web",), squad_role="ranged"),
    "spitter": EnemyTraits("territorial", ("spit",), squad_role="ranged"),
    "guardian": EnemyTraits("territorial", ("charge", "enrage"), squad_role="tank"),
}


def traits_for(kind_id: str | None, role: str) -> EnemyTraits:
    """The kind's traits; without a kind, the role's."""
    if kind_id in TRAITS:
        return TRAITS[kind_id]
    return ROLE_TRAITS.get(role, ROLE_TRAITS["guard"])
