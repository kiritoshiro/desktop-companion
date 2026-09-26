"""Visual enemy catalogue, independent of Qt and gameplay statistics.

Tiers are minimum map tiers: later maps can reuse earlier kinds. Boss queries
are separate. The caller applies size_scale with Creature.set_size_scale after
spawning; model base sizes stay comparable to the companion's 27px tarantula.
"""
from dataclasses import dataclass
import random

from .palettes import palette_from_hue


@dataclass(frozen=True)
class EnemyKind:
    id: str
    name: str
    model_id: str
    skins: tuple[dict, ...]
    size_scale: float = 1.0
    boss: bool = False
    tier: int = 1
    tags: tuple[str, ...] = ()


def _skin(hue: float, saturation: float, value: float, pale: bool = False) -> dict:
    palette = palette_from_hue(hue, saturation, value)
    if pale:
        palette["body"] = [min(255, c + 145) for c in palette["body"]]
        palette["legs"] = [min(255, c + 100) for c in palette["legs"]]
    if pale:
        palette["leg_band"] = [int(c * .45) for c in palette["leg_band"]]
        palette["highlight"] = [int(c * .48) for c in palette["highlight"]]
        palette["eyes"] = [int(c * .30) for c in palette["eyes"]]
    palette.update(rim=list(palette["highlight"]), marking=list(palette["leg_band"]),
                   fluff_color=list(palette["highlight"]))
    return palette


ENEMY_KINDS: dict[str, EnemyKind] = {
    "redback_raider": EnemyKind("redback_raider", "Redback raider", "enemy_redback_raider",
        (_skin(0.0, 0.8, 0.9), _skin(0.08, 0.8, 0.9), _skin(0.92, 0.6, 0.9)),
        size_scale=1.0, boss=False, tier=1, tags=('fast',)),
    "ash_wolf": EnemyKind("ash_wolf", "Ash wolf spider", "enemy_ash_wolf",
        (_skin(0.6, 0.12, 0.8), _skin(0.09, 0.45, 0.8), _skin(0.46, 0.3, 0.8)),
        size_scale=1.0, boss=False, tier=1, tags=('hunter',)),
    "jumping_skirmisher": EnemyKind("jumping_skirmisher", "Jumping skirmisher", "enemy_jumping_skirmisher",
        (_skin(0.48, 0.65, 0.95), _skin(0.79, 0.6, 0.95), _skin(0.12, 0.7, 0.95)),
        size_scale=0.9, boss=False, tier=1, tags=('fast',)),
    "harvest_stalker": EnemyKind("harvest_stalker", "Harvest stalker", "enemy_harvest_stalker",
        (_skin(0.08, 0.5, 0.8), _skin(0.35, 0.45, 0.8), _skin(0.65, 0.4, 0.9)),
        size_scale=1.0, boss=False, tier=2, tags=('ranged',)),
    "trapdoor_brute": EnemyKind("trapdoor_brute", "Trapdoor brute", "enemy_trapdoor_brute",
        (_skin(0.08, 0.65, 0.8), _skin(0.62, 0.45, 0.8), _skin(0.01, 0.6, 0.8)),
        size_scale=1.15, boss=False, tier=2, tags=('heavy',)),
    "pale_cave": EnemyKind("pale_cave", "Pale cave spider", "enemy_pale_cave",
        (_skin(0.13, 0.12, 0.98, pale=True), _skin(0.52, 0.2, 0.98, pale=True), _skin(0.78, 0.18, 0.98, pale=True)),
        size_scale=1.0, boss=False, tier=2, tags=('cave',)),
    "crab_reaver": EnemyKind("crab_reaver", "Crab reaver", "enemy_crab_reaver",
        (_skin(0.1, 0.7, 0.95), _skin(0.46, 0.65, 0.85), _skin(0.94, 0.6, 0.9)),
        size_scale=1.0, boss=False, tier=3, tags=('heavy',)),
    "thorn_weaver": EnemyKind("thorn_weaver", "Thorn orb-weaver", "enemy_thorn_weaver",
        (_skin(0.32, 0.65, 0.9), _skin(0.78, 0.6, 0.9), _skin(0.05, 0.75, 0.95)),
        size_scale=1.05, boss=False, tier=3, tags=('ranged',)),
    "ember_matriarch": EnemyKind("ember_matriarch", "Ember matriarch", "enemy_ember_matriarch",
        (_skin(0.01, 0.8, 0.95), _skin(0.1, 0.8, 0.95), _skin(0.85, 0.65, 0.95)),
        size_scale=1.65, boss=True, tier=1, tags=('heavy',)),
    "ivory_regent": EnemyKind("ivory_regent", "Ivory crab regent", "enemy_ivory_regent",
        (_skin(0.12, 0.16, 0.98, pale=True), _skin(0.51, 0.24, 0.98, pale=True), _skin(0.94, 0.2, 0.98, pale=True)),
        size_scale=1.7, boss=True, tier=2, tags=('heavy',)),
    "thorn_crown": EnemyKind("thorn_crown", "Thorn crown", "enemy_thorn_crown",
        (_skin(0.76, 0.75, 0.95), _skin(0.4, 0.7, 0.95), _skin(0.02, 0.8, 0.95)),
        size_scale=1.8, boss=True, tier=3, tags=('ranged',)),
}


def kinds_for_tier(tier: int, boss: bool = False) -> list[EnemyKind]:
    """Kinds unlocked at or before tier, filtered to bosses or ordinary enemies."""
    return [kind for kind in ENEMY_KINDS.values() if kind.tier <= tier and kind.boss == boss]


def pick_skin(kind: EnemyKind, seed: int) -> dict:
    """Stable per-spawn colour, with no global RNG use or shared mutable lists."""
    skin = random.Random(seed).choice(kind.skins)
    return {key: list(rgb) for key, rgb in skin.items()}
