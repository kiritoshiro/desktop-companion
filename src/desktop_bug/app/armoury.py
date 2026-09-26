"""What the player can do with the armour they own.

The armoury lives in the Adventure profile (adventure_profile): one of each
piece owned, its level, spare duplicates and amber. The hero and every
companion dress from it, and a piece is worn by one spider at a time.

- A piece found again becomes a spare.
- A spare stacked onto its piece raises it a level, up to five.
- Spares sell for amber; a shop can spend it later.
"""
from __future__ import annotations

from ..state.progression import ARMOR_BY_ID, MAX_ITEM_LEVEL
from .adventure_profile import HERO, fresh_armoury, spider_progression, store_progression
from .campaign import SELL_PRICES


def armoury(profile: dict) -> dict:
    return profile.setdefault("armoury", fresh_armoury())


def level_of(profile: dict, item_id: str) -> int:
    return int(armoury(profile)["levels"].get(item_id, 1))


def spares_of(profile: dict, item_id: str) -> int:
    return int(armoury(profile)["spares"].get(item_id, 0))


def add_loot(profile: dict, item_id: str) -> str:
    """Put a found piece in the armoury: ``"new"`` or ``"spare"``."""
    if item_id not in ARMOR_BY_ID:
        return ""
    store = armoury(profile)
    if item_id not in store["owned"]:
        store["owned"].append(item_id)
        return "new"
    store["spares"][item_id] = store["spares"].get(item_id, 0) + 1
    return "spare"


def can_upgrade(profile: dict, item_id: str) -> bool:
    return (item_id in armoury(profile)["owned"] and spares_of(profile, item_id) > 0
            and level_of(profile, item_id) < MAX_ITEM_LEVEL)


def upgrade(profile: dict, item_id: str) -> bool:
    """Stack one spare onto the piece: +1 level."""
    if not can_upgrade(profile, item_id):
        return False
    store = armoury(profile)
    store["spares"][item_id] -= 1
    if store["spares"][item_id] <= 0:
        del store["spares"][item_id]
    store["levels"][item_id] = level_of(profile, item_id) + 1
    return True


def sell_price(item_id: str) -> int:
    item = ARMOR_BY_ID.get(item_id)
    return SELL_PRICES.get(item.tier, 0) if item else 0


def sell_spares(profile: dict, item_id: str, count: int = 1) -> int:
    """Sell up to ``count`` spares; returns the amber earned. The piece itself is kept."""
    count = min(max(0, int(count)), spares_of(profile, item_id))
    if count <= 0:
        return 0
    store = armoury(profile)
    store["spares"][item_id] -= count
    if store["spares"][item_id] <= 0:
        del store["spares"][item_id]
    earned = count * sell_price(item_id)
    store["amber"] = int(store.get("amber", 0)) + earned
    return earned


def party(profile: dict) -> list[str]:
    """Everyone who can wear armour: the hero, then each companion."""
    return [HERO] + list((profile.get("companions") or {}).keys())


def worn_by(profile: dict) -> dict[str, str]:
    """item id -> who wears it."""
    worn = {}
    for who in party(profile):
        for item_id in spider_progression(profile, who).equipped.values():
            worn[item_id] = who
    return worn


def wear(profile: dict, who: str, item_id: str) -> bool:
    """Put a piece on ``who``, taking it off whoever wore it before."""
    item = ARMOR_BY_ID.get(item_id)
    if item is None or item_id not in armoury(profile)["owned"] or who not in party(profile):
        return False
    for other in party(profile):
        if other == who:
            continue
        state = spider_progression(profile, other)
        if item_id in state.equipped.values():
            state.equipped = {s: i for s, i in state.equipped.items() if i != item_id}
            store_progression(profile, other, state)
    state = spider_progression(profile, who)
    state.equip(item_id)
    store_progression(profile, who, state)
    return True


def take_off(profile: dict, who: str, slot: str) -> bool:
    state = spider_progression(profile, who)
    if not state.unequip(slot):
        return False
    store_progression(profile, who, state)
    return True
