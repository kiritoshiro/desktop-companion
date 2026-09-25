"""Runtime progression, equipment, and team data for desktop creatures.

The overlay intentionally keeps this module free of Qt and rendering concerns.
That makes the rules usable by the live UI, headless smoke tests, and future
feeding/combat systems without making a preset migration mandatory.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Iterable


MAX_LEVEL = 30
RELATIONS = ("friend", "neutral", "foe")

# Preset-time and runtime team pickers must offer the same ids, or a team
# chosen in the settings window and one chosen in the inspector silently
# describe different groups.
TEAM_OPTIONS = (
    ("Neutral / solo", "neutral"),
    ("Pack A", "pack_a"),
    ("Pack B", "pack_b"),
    ("Hunters", "hunters"),
    ("Rivals", "rivals"),
)

# Picking "Rivals" in the settings window should mean something on its own, so
# it is hostile to every other named team unless a preset's team_relations says
# otherwise. Every other pairing stays unrelated until it is declared, which
# keeps the default scene peaceful.
DEFAULT_HOSTILE_TEAMS = ("rivals",)


@dataclass(frozen=True)
class AbilityNode:
    id: str
    name: str
    description: str
    level_required: int
    cost: int = 1
    prerequisites: tuple[str, ...] = ()
    effects: dict[str, float] = field(default_factory=dict)


ABILITY_TREE = (
    AbilityNode("vitality", "Vitality", "A sturdier body: you can take more hits before you fall.", 2, effects={"max_hp": 18.0}),
    AbilityNode("quick_step", "Quick step", "Quicker legs: you walk and turn faster.", 3, effects={"speed": 0.06}),
    AbilityNode("silk_sense", "Silk sense", "A deeper silk reserve: more stamina for webs and pounces, and it refills faster.", 4, effects={"max_energy": 12.0, "energy_regen": 0.8}),
    AbilityNode("carapace_harden", "Hardened carapace", "A denser shell: every hit you take lands softer.", 5, effects={"armor": 2.0}, prerequisites=("vitality",)),
    AbilityNode("power_strike", "Power strike", "Stronger chelicerae: every bite hits harder.", 6, effects={"damage": 2.0}),
    AbilityNode("web_crafter", "Web crafter", "Practised spinning: more stamina, and it opens Silk tracking.", 8, effects={"max_energy": 8.0}, prerequisites=("silk_sense",)),
    AbilityNode("silk_tracking", "Silk tracking", "Thrown silk corrects itself in flight and follows a moving target.", 9, effects={"web_homing": 1.0}, prerequisites=("web_crafter",)),
    AbilityNode("long_stride", "Long stride", "Each step reaches further: you move faster still.", 10, effects={"speed": 0.08}, prerequisites=("quick_step",)),
    AbilityNode("apex_predator", "Apex predator", "The peak of the hunt: much harder bites and a deep stamina reserve.", 15, effects={"damage": 4.0, "max_energy": 16.0}, prerequisites=("power_strike", "carapace_harden")),
)

ABILITY_BY_ID = {node.id: node for node in ABILITY_TREE}


@dataclass(frozen=True)
class ArmorItem:
    id: str
    name: str
    slot: str
    description: str
    armor: float = 0.0
    max_hp: float = 0.0
    max_energy: float = 0.0
    damage: float = 0.0
    speed: float = 0.0
    size_min: float = 0.55
    size_max: float = 2.25
    # Pieces of one set share an id; wearing all of them adds ARMOR_SETS' bonus.
    set_id: str = ""


# These are light, anatomy-aware pieces rather than generic humanoid armour.
# ``slot`` names also provide a stable hook for future sprites or model assets.
ARMOR_CATALOG = (
    ArmorItem("silk_carapace", "Woven carapace", "carapace", "A flexible silk shell over the carapace.", armor=2.0, max_hp=8.0, speed=-0.015),
    ArmorItem("fluffy_mantle", "Soft setae mantle", "abdomen", "A warm, light mantle that cushions impacts.", armor=1.0, max_hp=12.0, max_energy=4.0, speed=-0.02),
    ArmorItem("leg_guard_set", "Leg guard set", "legs", "Small guards fitted around the leg joints.", armor=1.0, speed=0.025),
    ArmorItem("pedipalp_cuffs", "Pedipalp cuffs", "pedipalps", "Tiny protective cuffs that leave the hands free.", armor=0.5, max_energy=5.0, damage=0.5, speed=-0.01),
    ArmorItem("chelicerae_cap", "Fang cap", "head", "A narrow guard around the small head and chelicerae.", armor=1.0, damage=2.0, speed=-0.008),
    # The Warden set is cut from a moulted exoskeleton and follows a
    # tarantula's own anatomy (the owner: "one armor set that would actually be
    # logical anatomically for tarantula"). Every piece leaves free what the
    # spider needs: the eyes, the fovea, the knees, the tarsal claws and the
    # spinnerets. See [[Tarantula Reference - Brachypelma hamorii]].
    ArmorItem("warden_crest", "Warden ocular crest", "head",
              "A ridged visor over the eye mound; the eight eyes stay clear and the chelicerae get capped tips.",
              armor=1.0, damage=2.5, set_id="warden"),
    ArmorItem("warden_carapace", "Warden carapace plate", "carapace",
              "A shield moulded to the prosoma, the fovea left open; the orange rim still shows around it.",
              armor=2.5, max_hp=12.0, speed=-0.02, set_id="warden"),
    ArmorItem("warden_tergites", "Warden tergite bands", "abdomen",
              "Overlapping bands over the soft abdomen, the most vulnerable part; they flex as it breathes and leave the spinnerets free.",
              armor=1.5, max_hp=20.0, speed=-0.02, set_id="warden"),
    ArmorItem("warden_greaves", "Warden femur greaves", "legs",
              "Plates on all eight femurs, the long upper leg; the red knees and the gripping tarsi stay bare.",
              armor=1.0, max_hp=6.0, speed=-0.01, set_id="warden"),
    ArmorItem("warden_bracers", "Warden palp bracers", "pedipalps",
              "Bracers on both pedipalps; the palp tips stay free to feel and to hold prey.",
              armor=0.5, damage=1.0, max_energy=6.0, set_id="warden"),
)

ARMOR_BY_ID = {item.id: item for item in ARMOR_CATALOG}


@dataclass(frozen=True)
class ArmorSet:
    id: str
    name: str
    description: str
    effects: dict[str, float] = field(default_factory=dict)

    @property
    def pieces(self) -> tuple[str, ...]:
        return tuple(item.id for item in ARMOR_CATALOG if item.set_id == self.id)


ARMOR_SETS = {
    "warden": ArmorSet("warden", "Warden set",
                       "Wear all five pieces: the plates brace one another.",
                       effects={"max_hp": 10.0, "armor": 0.5, "energy_regen": 1.5}),
}


def set_pieces_worn(state: "ProgressionState", set_id: str) -> int:
    worn = set(state.equipped.values())
    return sum(piece in worn for piece in ARMOR_SETS[set_id].pieces)


def set_bonus_effects(state: "ProgressionState") -> dict[str, float]:
    """The bonuses of every armour set worn complete."""
    totals: dict[str, float] = {}
    for armor_set in ARMOR_SETS.values():
        if armor_set.pieces and set_pieces_worn(state, armor_set.id) == len(armor_set.pieces):
            for key, value in armor_set.effects.items():
                totals[key] = totals.get(key, 0.0) + value
    return totals


def xp_to_next_level(level: int) -> int:
    """Return the XP bank needed to advance from ``level`` to ``level + 1``."""
    level = max(1, min(MAX_LEVEL, int(level)))
    # Starts at 100 XP and rises gently, so a casual feeder can see progress
    # without making the last levels effectively unreachable.
    return int(round(92.0 + level * 21.0 + level * level * 2.1))


def growth_multipliers(level: int) -> tuple[float, float]:
    """Return bounded size and speed multipliers for a level 1..30 spider."""
    level = max(1, min(MAX_LEVEL, int(level)))
    step = level - 1
    return 1.0 + step * 0.012, 1.0 + step * 0.018


@dataclass
class ProgressionState:
    level: int = 1
    xp: int = 0
    total_xp: int = 0
    skill_points: int = 0
    unlocked_abilities: list[str] = field(default_factory=list)
    inventory: list[str] = field(default_factory=lambda: ["silk_carapace", "leg_guard_set"])
    equipped: dict[str, str] = field(default_factory=dict)
    team_id: str = "neutral"
    relation_overrides: dict[str, str] = field(default_factory=dict)
    pin_level: bool = False
    pin_health: bool = False

    @classmethod
    def from_dict(cls, value: dict | None) -> "ProgressionState":
        if not isinstance(value, dict):
            return cls()
        state = cls()
        try:
            state.level = max(1, min(MAX_LEVEL, int(value.get("level", 1))))
        except (TypeError, ValueError):
            state.level = 1
        try:
            state.xp = max(0, int(value.get("xp", 0)))
            state.total_xp = max(state.xp, int(value.get("total_xp", state.xp)))
            state.skill_points = max(0, int(value.get("skill_points", 0)))
        except (TypeError, ValueError):
            state.xp = state.total_xp = state.skill_points = 0
        raw_abilities = value.get("unlocked_abilities", [])
        if isinstance(raw_abilities, list):
            state.unlocked_abilities = [a for a in raw_abilities if isinstance(a, str) and a in ABILITY_BY_ID]
        raw_inventory = value.get("inventory", state.inventory)
        if isinstance(raw_inventory, list):
            state.inventory = [i for i in raw_inventory if isinstance(i, str) and i in ARMOR_BY_ID]
        raw_equipped = value.get("equipped", {})
        if isinstance(raw_equipped, dict):
            state.equipped = {str(slot): str(item) for slot, item in raw_equipped.items()
                              if str(item) in ARMOR_BY_ID and str(item) in state.inventory}
        # Case-folded for the same reason preset namespaces are: two
        # spellings of one team must not become two teams.
        state.team_id = normalize_team_id(value.get("team_id", "neutral"))
        raw_relations = value.get("relation_overrides", {})
        if isinstance(raw_relations, dict):
            state.relation_overrides = {str(key): str(rel).lower() for key, rel in raw_relations.items()
                                        if str(rel).lower() in RELATIONS}
        state.pin_level = bool(value.get("pin_level", False))
        state.pin_health = bool(value.get("pin_health", False))
        return state

    def to_dict(self) -> dict:
        return {
            "level": int(self.level),
            "xp": int(self.xp),
            "total_xp": int(self.total_xp),
            "skill_points": int(self.skill_points),
            "unlocked_abilities": list(self.unlocked_abilities),
            "inventory": list(self.inventory),
            "equipped": dict(self.equipped),
            "team_id": self.team_id,
            "relation_overrides": dict(self.relation_overrides),
            "pin_level": bool(self.pin_level),
            "pin_health": bool(self.pin_health),
        }

    def add_item(self, item_id: str) -> bool:
        if item_id not in ARMOR_BY_ID or item_id in self.inventory:
            return False
        self.inventory.append(item_id)
        return True

    def equip(self, item_id: str) -> bool:
        item = ARMOR_BY_ID.get(item_id)
        if item is None or item_id not in self.inventory:
            return False
        self.equipped[item.slot] = item_id
        return True

    def unequip(self, slot: str) -> bool:
        return self.equipped.pop(str(slot), None) is not None

    def can_unlock(self, ability_id: str) -> bool:
        node = ABILITY_BY_ID.get(ability_id)
        if node is None or ability_id in self.unlocked_abilities:
            return False
        return (self.level >= node.level_required and self.skill_points >= node.cost
                and all(req in self.unlocked_abilities for req in node.prerequisites))


def equipped_items(state: ProgressionState) -> Iterable[ArmorItem]:
    for item_id in state.equipped.values():
        item = ARMOR_BY_ID.get(item_id)
        if item is not None:
            yield item


def normalize_team_id(value) -> str:
    """Return a canonical team id. Case-insensitive, like preset namespaces.

    Whitespace collapses to an underscore as well, so a team written as
    ``Porch Guard`` in a hand-edited preset, one typed as ``porch guard`` in the
    inspector, and the ``porch_guard`` the settings window generates from a name
    are one team rather than three that merely look alike.
    """
    text = "_".join(str(value or "neutral").strip().lower().split())
    return text[:32] or "neutral"


def normalize_team_stances(raw) -> dict:
    """Clean a preset's ``team_relations`` block into ``{team: {team: rel}}``.

    Each declaration is stored in both directions. A stance between two teams
    is mutual, and a half-declared one would let a Guard and its intruder
    disagree about whether anything hostile is happening.
    """
    stances: dict = {}
    if not isinstance(raw, dict):
        return stances
    for left, row in raw.items():
        if not isinstance(row, dict):
            continue
        left_id = normalize_team_id(left)
        for right, relation in row.items():
            rel = str(relation or "").strip().lower()
            if rel not in RELATIONS:
                continue
            right_id = normalize_team_id(right)
            stances.setdefault(left_id, {})[right_id] = rel
            stances.setdefault(right_id, {})[left_id] = rel
    return stances


def team_stance(left_team, right_team, stances: dict | None = None) -> str | None:
    """Return the relation declared between two teams, or None if unrelated.

    Team identity used to imply only friendship, so two different teams were
    merely unrelated. A Guard reacts only to a declared foe, which left the
    shipped Colony preset unable to demonstrate the behaviour it advertises.
    """
    left_id = normalize_team_id(left_team)
    right_id = normalize_team_id(right_team)
    if left_id == "neutral" or right_id == "neutral":
        # A solo spider belongs to no team and takes no side.
        return None
    if stances:
        row = stances.get(left_id)
        if row and right_id in row:
            return row[right_id]
    if left_id == right_id:
        return "friend"
    if left_id in DEFAULT_HOSTILE_TEAMS or right_id in DEFAULT_HOSTILE_TEAMS:
        return "foe"
    return None


def relation_between(
    left: ProgressionState,
    right: ProgressionState,
    right_key: str | None = None,
    stances: dict | None = None,
) -> str:
    """Resolve a pair relation.

    Order: an explicit per-pair choice made in the runtime inspector, then the
    stance declared between the two teams, then no relation at all.
    """
    if right_key and right_key in left.relation_overrides:
        return left.relation_overrides[right_key]
    stance = team_stance(left.team_id, right.team_id, stances)
    if stance:
        return stance
    return "neutral"

