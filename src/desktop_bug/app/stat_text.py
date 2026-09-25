"""What a skill or an armour piece gives, in words a player can use.

The owner: *"for skills also write what each skill gives when hovering mouse
about it."* Every number is the one the game applies (creature.core adds these
effects to the spider's stats); the explanations name what it changes in play.
"""
from __future__ import annotations

from html import escape

from .adventure import PlayerController
from ..state.progression import (ABILITY_BY_ID, ARMOR_SETS, ArmorItem, set_pieces_worn)

GOOD = "#3f6b1f"
BAD = "#a2371f"
DIM = "#6a4a2c"
SLOT_NAMES = {"head": "Head", "carapace": "Carapace", "abdomen": "Abdomen",
              "legs": "Legs", "pedipalps": "Pedipalps"}
ITEM_STATS = ("armor", "max_hp", "damage", "max_energy", "speed")


def _num(value: float) -> str:
    return f"{value:+.1f}".rstrip("0").rstrip(".") if value % 1 else f"{value:+.0f}"


def effect_lines(effects: dict) -> list[tuple[str, bool]]:
    """(text, is_good) for each effect, in the order a player cares about."""
    web, pounce = PlayerController.WEB_ENERGY, PlayerController.JUMP_ENERGY
    lines = []
    for key in ("max_hp", "armor", "damage", "max_energy", "energy_regen", "speed", "web_homing"):
        value = float(effects.get(key, 0.0))
        if abs(value) < 1e-9:
            continue
        good = value > 0
        if key == "max_hp":
            text = f"{_num(value)} max health"
        elif key == "armor":
            text = f"{_num(value)} armour: every hit you take is {abs(value):g} lower"
        elif key == "damage":
            text = f"{_num(value)} damage on every bite"
        elif key == "max_energy":
            text = f"{_num(value)} max stamina (a web costs {web:g}, a pounce {pounce:g})"
        elif key == "energy_regen":
            text = f"{_num(value)} stamina per second (base 8)"
        elif key == "speed":
            text = f"{_num(value * 100)}% speed"
        else:
            text = "Thrown silk steers after a moving target"
        lines.append((text, good))
    return lines


def effects_html(effects: dict) -> str:
    return "<br>".join(f"<span style='color:{GOOD if good else BAD}'>{'▲' if good else '▼'} "
                       f"{escape(text)}</span>" for text, good in effect_lines(effects))


def item_effects(item: ArmorItem) -> dict:
    return {key: getattr(item, key) for key in ITEM_STATS if getattr(item, key)}


def skill_tooltip(node, state) -> str:
    """Name, cost, what it needs, what it does and exactly what it gives."""
    parts = [f"<b style='font-size:11pt'>{escape(node.name)}</b>",
             f"<span style='color:{DIM}'>Level {node.level_required} · "
             f"{node.cost} skill point{'s' if node.cost != 1 else ''}</span>"]
    if node.prerequisites:
        names = ", ".join(ABILITY_BY_ID[r].name for r in node.prerequisites)
        parts.append(f"<span style='color:{DIM}'>Needs {escape(names)}</span>")
    parts.append(escape(node.description))
    parts.append("<b>Gives</b><br>" + effects_html(node.effects))
    opens = [n.name for n in ABILITY_BY_ID.values() if node.id in n.prerequisites]
    if opens:
        parts.append(f"<span style='color:{DIM}'>Opens {escape(', '.join(opens))}</span>")
    if state is not None:
        if node.id in state.unlocked_abilities:
            parts.append(f"<i style='color:{GOOD}'>Learned</i>")
        elif state.can_unlock(node.id):
            parts.append(f"<i style='color:{GOOD}'>Click to learn</i>")
    return "<p>" + "<br>".join(parts) + "</p>"


def short_effect(text: str) -> str:
    """"+2 armour: every hit ..." -> "+2 armour", for one-line summaries."""
    return text.split(":")[0].split(" (")[0]


def set_line(set_id: str, state, dim: str = DIM, good: str = GOOD) -> str:
    armor_set = ARMOR_SETS[set_id]
    worn = set_pieces_worn(state, set_id) if state is not None else 0
    total = len(armor_set.pieces)
    bonus = ", ".join(short_effect(text) for text, _ in effect_lines(armor_set.effects))
    colour = good if worn == total else dim
    return (f"<span style='color:{colour}'><b>{escape(armor_set.name)} {worn}/{total}</b>"
            f" · all {total}: {escape(bonus)}</span>")


def item_tooltip(item: ArmorItem, state=None) -> str:
    parts = [f"<b style='font-size:11pt'>{escape(item.name)}</b>",
             f"<span style='color:{DIM}'>{SLOT_NAMES.get(item.slot, item.slot)}</span>",
             escape(item.description),
             effects_html(item_effects(item))]
    if item.set_id in ARMOR_SETS:
        parts.append(set_line(item.set_id, state))
    return "<p>" + "<br>".join(parts) + "</p>"
