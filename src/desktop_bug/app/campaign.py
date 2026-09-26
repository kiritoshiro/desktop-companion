"""The Adventure campaign: maps, companions, what enemies wear and what they drop.

The owner: *"i want the enemies to wear the armor. and on other maps better
armor that way user could get new armor dropped from killing enemies. also he
could stack these armor to upgrade them for 5 levels. any other duplicates
could be sold and at some point a shop could be made ... also ability to put
armor on companion/s. and more companions could be obtained from further
maps. also will need bosses who will wear legendary armor."*

Pure data and small rules, no Qt: the mission, the Adventure page and the
character window all read it.
"""
from __future__ import annotations

import random
from dataclasses import dataclass

from ..state.progression import ARMOR_CATALOG, ARMOR_SETS, ARMOR_TIERS, MAX_ITEM_LEVEL


@dataclass(frozen=True)
class MapInfo:
    id: str
    title: str
    blurb: str
    tier: int                       # 1 = first map; later maps are tougher
    unlock_after: str | None        # the map that must be won first
    enemy_tiers: tuple[str, ...]    # armour qualities enemies wear, and so drop
    guardian: str                   # the boss at the Thorn nest
    guardian_set: str               # the armour set it wears complete
    reward_companion: str | None    # joins you the first time the map is won
    # "raid": the woodland buildings; "reclaim": the frozen desktop itself,
    # which acid melts and heavy spiders crack (app/reclaim.py).
    kind: str = "raid"


MAPS = (
    MapInfo("territory", "Take back the desktop",
            "Capture Food or Silk, seal the Hatchery, survive the counterattack "
            "and claim Thorn nest.",
            1, None, ("common", "uncommon"), "Thorn guardian", "forager", "weaver"),
    MapInfo("swarm", "Fly swarm",
            "Flies pour from nests on every screen. Pin them with silk and eat "
            "them before the rivals do.",
            1, None, ("common",), "Swarm", "forager", None, kind="swarm"),
    MapInfo("ember", "Ember hollow",
            "Raiders in bronze hold the hollow. Their warden wears the full "
            "Warden plate.",
            2, "territory", ("uncommon", "rare"), "Hollow warden", "warden", "hunter"),
    MapInfo("reclaim", "Reclaim the desktop",
            "Your desktop is frozen and infested. Destroy the nest on every screen "
            "before the acid eats it. Esc gives it back at once.",
            2, "territory", ("uncommon", "rare"), "The Devourer", "brood", None, kind="reclaim"),
    MapInfo("storm", "Fly storm",
            "A storm of fast flies, more rivals, less time. Rival dens wait on "
            "your other screens.",
            3, "ember", ("rare", "epic"), "Storm", "brood", None, kind="swarm"),
    MapInfo("obsidian", "Obsidian deep",
            "Rune-cut raiders and a brood matriarch in black glass.",
            3, "ember", ("rare", "epic"), "Brood matriarch", "brood", "sentinel"),
    MapInfo("queen", "Queen of thorns",
            "The nest mother herself, in Sunforged regalia. Her guard wear "
            "crystal and obsidian.",
            4, "obsidian", ("epic", "legendary"), "Queen of thorns", "sun", None),
)
MAP_BY_ID = {m.id: m for m in MAPS}
DEFAULT_MAP = MAPS[0].id


@dataclass(frozen=True)
class CompanionInfo:
    id: str
    name: str
    style: str      # how it fights: "scout" bites, "weaver" shoots silk, "hunter" pounces
    blurb: str


COMPANIONS = (
    CompanionInfo("scout", "Scout", "scout", "Your first companion. Follows, guards, bites."),
    CompanionInfo("weaver", "Silk weaver", "weaver", "Keeps its distance and pins foes with silk."),
    CompanionInfo("hunter", "Hunter", "hunter", "Closes in with pounces."),
    CompanionInfo("sentinel", "Sentinel", "scout", "A heavy biter that holds the line."),
)
COMPANION_BY_ID = {c.id: c for c in COMPANIONS}
STARTING_COMPANIONS = ("scout",)
# How many companions come on a raid, first unlocked first.
PARTY_SIZE = 3

# Amber paid for one spare piece, by quality. A shop can spend it later.
SELL_PRICES = {"common": 5, "uncommon": 12, "rare": 30, "epic": 70, "legendary": 160}
# Chance an enemy drops one of the pieces it wore when it dies.
DROP_CHANCE = 0.35


def find_map(map_id) -> MapInfo | None:
    """A built-in map, or one made in the map editor (custom_maps.py)."""
    if map_id in MAP_BY_ID:
        return MAP_BY_ID[map_id]
    from . import custom_maps

    data = custom_maps.load_map(map_id)
    return custom_maps.map_info(data) if data is not None else None


def chosen_map(profile: dict) -> MapInfo:
    """The map the next raid is played on: the one chosen on the Adventure
    page, or the first when that one is unknown or still locked."""
    chosen = profile.get("selected_map", DEFAULT_MAP)
    admin = bool(profile.get("admin", False))
    if not map_unlocked(profile.get("missions") or {}, chosen, admin):
        chosen = DEFAULT_MAP
    return find_map(chosen) or MAP_BY_ID[DEFAULT_MAP]


def map_unlocked(records: dict, map_id: str, admin: bool = False) -> bool:
    """``records``: mission id -> {"victories": n, ...} from the profile.
    Your own maps are always open; in admin mode every map is."""
    info = find_map(map_id)
    if info is None:
        return False
    if admin or info.unlock_after is None:
        return True
    return int((records.get(info.unlock_after) or {}).get("victories", 0)) > 0


def pieces_of_tiers(tiers) -> list:
    return [item for item in ARMOR_CATALOG if item.tier in tiers]


def enemy_loadout(info: MapInfo, role: str, rng: random.Random) -> tuple[list[str], int]:
    """(armour ids to wear, their level) for a mission enemy.

    The guardian wears its map's set complete; everyone else a few pieces of
    the map's qualities, more and better on later maps. At most one per slot.
    """
    level = max(1, min(MAX_ITEM_LEVEL, info.tier))
    if role == "guardian":
        # The boss wears the best there is on its map: its whole set, a
        # level above what its guard wear (the owner: "boses wears best armor").
        return list(ARMOR_SETS[info.guardian_set].pieces), min(MAX_ITEM_LEVEL, level + 1)
    # The owner: "in first levels no armor on enemy ... only in higher maps
    # should they wear better armor." Nothing on the first maps, a piece or
    # two on the second, most of a suit by the last.
    counts = {1: (0, 0), 2: (0, 2), 3: (2, 3), 4: (3, 5)}
    low, high = counts.get(info.tier, (1, 3))
    pool = pieces_of_tiers(info.enemy_tiers)
    rng.shuffle(pool)
    wanted = rng.randint(low, high)
    chosen, slots = [], set()
    for item in pool:
        if len(chosen) >= wanted:
            break
        if item.slot not in slots:
            chosen.append(item.id)
            slots.add(item.slot)
    return chosen, max(1, level - 1)


def roll_drop(worn: list[str], guardian: bool, rng: random.Random) -> list[str]:
    """What a dead enemy leaves behind: one worn piece, sometimes; a guardian
    always leaves two."""
    if not worn:
        return []
    if guardian:
        return rng.sample(worn, min(2, len(worn)))
    return [rng.choice(worn)] if rng.random() < DROP_CHANCE else []


def tier_rank(tier: str) -> int:
    return ARMOR_TIERS.index(tier) if tier in ARMOR_TIERS else 0
