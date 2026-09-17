from __future__ import annotations

"""Skill registry for creature abilities.

The animation/AI state machine still lives in :mod:`desktop_bug.creature` so the
existing behaviour stays stable.  This module is the small, explicit skill layer
used by presets, the settings UI, and the runtime context menu to decide which
abilities a creature is allowed to enter.
"""

from dataclasses import dataclass
from typing import Iterable, Tuple

from .personality_profiles import ability_ids_for


@dataclass(frozen=True)
class CreatureSkill:
    """One selectable creature ability.

    A skill is intentionally metadata plus an id.  The concrete behaviour is the
    corresponding state-machine branch in ``Creature``; keeping that logic in one
    place avoids risky inheritance/refactor bugs while still making skills
    independently selectable.
    """

    id: str
    display_name: str
    description: str
    category: str = "Behaviour"


class JumpSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "jump",
            "Jump / hop",
            "Allows small hops and the airborne part of pounces.",
            "Movement",
        )


class RollSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "roll",
            "Roll / tumble",
            "Allows happy curl-up tumbles and rolling flourishes.",
            "Movement",
        )


class ChaseSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "chase",
            "Chase",
            "Allows burst chasing of the cursor or a playmate.",
            "Interaction",
        )


class ObserveSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "observe",
            "Observe / orbit",
            "Allows observer spiders to circle and watch a cursor or another spider.",
            "Interaction",
        )


class RunAwaySkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "run_away",
            "Run away / retreat",
            "Allows threat-triggered retreats away from fast cursor movement.",
            "Survival",
        )


class PrepareJumpAttackSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "prepare_jump_attack",
            "Prepare jump attack",
            "Allows the crouch, rangefinder feeler pose, and pounce wind-up.",
            "Interaction",
        )


class InspectSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "inspect",
            "Inspect",
            "Allows curious close inspection and studying behaviour.",
            "Interaction",
        )


class CuddleSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "cuddle",
            "Cuddle",
            "Allows affectionate approach, snuggling, and boop behaviour.",
            "Social",
        )


class SocialPlaySkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "social_play",
            "Social play",
            "Allows direct play bouts between spiders when social play is enabled.",
            "Social",
        )


class ZoomiesSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "zoomies",
            "Zoomies",
            "Allows short playful sprint bursts.",
            "Movement",
        )


class WanderSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "wander",
            "Wander / roam",
            "Allows idle roaming between cursor interactions.",
            "Movement",
        )


class DriftSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "drift",
            "Drift / slide",
            "Allows momentum-based drift sliding: builds speed first, breaks traction, leans/counter-steers through wide circle or corner skids, and keeps sliding after throws.",
            "Ability",
        )


class ApproachSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "approach",
            "Approach / stalk",
            "Allows cautious movement toward the cursor before chasing or inspecting.",
            "Interaction",
        )


class WeaveWebSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "weave_web",
            "Weave web",
            "Allows building silk webs thread by thread in screen corners, and "
            "finishing an abandoned unfinished web even if another spider began it.",
            "Ability",
        )


class WebWalkSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "web_walk",
            "Walk on webs",
            "Allows walking onto a finished web and plucking it to test its bounce.",
            "Ability",
        )


class ShootWebSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "shoot_web",
            "Shoot trapping web",
            "Allows aiming and firing a glob of sticky silk that pins its target "
            "in place, whether the mouse pointer or a fly. Wiggle a trapped "
            "pointer to break it free.",
            "Ability",
        )


class WallWebSkill(CreatureSkill):
    def __init__(self) -> None:
        super().__init__(
            "wall_web",
            "Web-shove to wall",
            "Allows firing a web that shoves its target to the nearest wall and "
            "pins it there, whether the mouse pointer or a fly. Wiggle a trapped "
            "pointer to peel it off.",
            "Ability",
        )


SKILL_CLASSES = (
    ApproachSkill,
    WanderSkill,
    DriftSkill,
    JumpSkill,
    RollSkill,
    ChaseSkill,
    ObserveSkill,
    RunAwaySkill,
    PrepareJumpAttackSkill,
    InspectSkill,
    CuddleSkill,
    SocialPlaySkill,
    ZoomiesSkill,
    WeaveWebSkill,
    WebWalkSkill,
    ShootWebSkill,
    WallWebSkill,
)

SKILLS: Tuple[CreatureSkill, ...] = tuple(cls() for cls in SKILL_CLASSES)
SKILL_BY_ID = {skill.id: skill for skill in SKILLS}
DEFAULT_SKILL_IDS: Tuple[str, ...] = tuple(skill.id for skill in SKILLS)
ABILITY_SKILL_IDS: Tuple[str, ...] = tuple(skill.id for skill in SKILLS if skill.category == "Ability")
BEHAVIOUR_SKILL_IDS: Tuple[str, ...] = tuple(skill.id for skill in SKILLS if skill.category != "Ability")

# These are the ordinary state-machine behaviours shared by a normal spider.
# True capabilities live in COMMON_ABILITY_IDS and the compact personality
# bundles below, keeping movement style separate from what a spider can do.
COMMON_BEHAVIOUR_IDS: Tuple[str, ...] = (
    "approach",
    "wander",
    "jump",
    "roll",
    "chase",
    "observe",
    "run_away",
    "prepare_jump_attack",
    "inspect",
    "cuddle",
    "social_play",
    "zoomies",
)
COMMON_ABILITY_IDS: Tuple[str, ...] = ("web_walk",)
COMMON_SKILL_IDS: Tuple[str, ...] = COMMON_BEHAVIOUR_IDS + COMMON_ABILITY_IDS


def default_skills_for_personality(personality) -> list:
    """Compose behaviours and abilities for a compact personality definition.

    Legacy explicit ``skills`` arrays still take precedence. New profiles can
    select ``behaviours``, ``abilities``, and named ``ability_bundles`` without
    repeating the common catalog in every personality JSON file.
    """

    if personality is None:
        return list(DEFAULT_SKILL_IDS)
    if isinstance(personality, dict):
        if personality.get("skills") is not None:
            return normalize_skill_ids(personality.get("skills"))
        raw_behaviours = personality.get("behaviours")
        if isinstance(raw_behaviours, (list, tuple)):
            wanted = [str(item).strip().lower() for item in raw_behaviours]
        else:
            wanted = list(COMMON_BEHAVIOUR_IDS)
        if bool(personality.get("include_common_abilities", True)):
            wanted.extend(COMMON_ABILITY_IDS)
        for extra in ability_ids_for(personality):
            if extra not in wanted:
                wanted.append(extra)
        return normalize_skill_ids(wanted)
    # A bare personality id string.
    pid = str(personality).strip().lower()
    wanted = list(COMMON_BEHAVIOUR_IDS) + list(COMMON_ABILITY_IDS)
    for extra in ability_ids_for(pid):
        if extra not in wanted:
            wanted.append(extra)
    return normalize_skill_ids(wanted)

# Used when showing a compact button/summary.  These are deliberately short so a
# slot table row does not become unreadable.
SHORT_LABELS = {
    "approach": "approach",
    "wander": "wander",
    "drift": "drift",
    "jump": "jump",
    "roll": "roll",
    "chase": "chase",
    "observe": "observe",
    "run_away": "run away",
    "prepare_jump_attack": "jump attack",
    "inspect": "inspect",
    "cuddle": "cuddle",
    "social_play": "play",
    "zoomies": "zoomies",
    "weave_web": "weave",
    "web_walk": "web walk",
    "shoot_web": "web trap",
    "wall_web": "wall pin",
}


def normalize_skill_ids(value: Iterable[str] | None) -> list[str]:
    """Return known skill ids in registry order.

    ``None`` means "use the full default set" for backwards compatibility with
    presets created before skills existed.  An explicit empty list is respected,
    leaving the spider with only passive/idle behaviour.
    """

    if value is None:
        return list(DEFAULT_SKILL_IDS)
    wanted = {str(item).strip().lower() for item in value if str(item).strip()}
    return [skill_id for skill_id in DEFAULT_SKILL_IDS if skill_id in wanted]


def normalize_ability_ids(value: Iterable[str] | None) -> list[str]:
    """Return only true capability ids, in the stable registry order.

    Behaviour phases are personality-controlled and deliberately cannot be
    toggled from the settings picker.  This helper keeps the persisted
    ``abilities`` field separate while legacy ``skills`` arrays remain valid.
    """
    if value is None:
        return list(ABILITY_SKILL_IDS)
    wanted = {str(item).strip().lower() for item in value if str(item).strip()}
    return [ability_id for ability_id in ABILITY_SKILL_IDS if ability_id in wanted]


def default_ability_ids(personality) -> list[str]:
    """Return only the capabilities a personality grants on its own.

    A preset slot that never customised its abilities must keep these, or a
    webber stops weaving, a trapper stops shooting silk, a drifter stops
    drifting, and nobody can walk a web.  ``normalize_ability_ids`` deliberately
    treats an empty selection as "no abilities", so callers that mean "the
    personality decides" must start from this list instead of an empty one.
    """
    return [
        skill_id
        for skill_id in default_skills_for_personality(personality)
        if skill_id in ABILITY_SKILL_IDS
    ]


def skills_with_selected_abilities(personality, abilities: Iterable[str] | None) -> list[str]:
    """Compose personality behaviours with an explicit capability selection."""
    base = default_skills_for_personality(personality)
    behaviours = [skill_id for skill_id in base if skill_id not in ABILITY_SKILL_IDS]
    return behaviours + normalize_ability_ids(abilities)


def skills_with_default_abilities(personality, extra_abilities: Iterable[str] | None = None) -> list[str]:
    """Compose personality behaviours with its own abilities plus ``extra``.

    This is the "slot made no ability choice" path: job-required abilities are
    added on top of whatever the personality already grants.
    """
    wanted = default_ability_ids(personality) + [
        str(item).strip().lower() for item in (extra_abilities or ())
    ]
    return skills_with_selected_abilities(personality, wanted)


def unknown_skill_ids(value: Iterable[str] | None) -> list[str]:
    if value is None:
        return []
    return [str(item) for item in value if str(item).strip().lower() not in SKILL_BY_ID]


def compact_skill_summary(skill_ids: Iterable[str] | None) -> str:
    ids = normalize_skill_ids(skill_ids)
    if set(ids) == set(DEFAULT_SKILL_IDS):
        return "All skills"
    if not ids:
        return "No optional skills"
    labels = [SHORT_LABELS.get(skill_id, skill_id) for skill_id in ids]
    if len(labels) <= 3:
        return ", ".join(labels)
    return f"{len(labels)} skills: " + ", ".join(labels[:3]) + "…"


def compact_ability_summary(skill_ids: Iterable[str] | None) -> str:
    """Compactly label only the capabilities shown by the settings UI."""
    ids = normalize_ability_ids(skill_ids)
    if set(ids) == set(ABILITY_SKILL_IDS):
        return "All abilities"
    if not ids:
        return "No optional abilities"
    labels = [SHORT_LABELS.get(skill_id, skill_id) for skill_id in ids]
    if len(labels) <= 3:
        return ", ".join(labels)
    return f"{len(labels)} abilities: " + ", ".join(labels[:3]) + "…"


class SkillSet:
    """Mutable allowed-skill set for one live creature."""

    def __init__(self, skill_ids: Iterable[str] | None = None) -> None:
        self._ids = set(normalize_skill_ids(skill_ids))

    def has(self, skill_id: str) -> bool:
        return str(skill_id).strip().lower() in self._ids

    def set_enabled(self, skill_id: str, enabled: bool) -> None:
        skill_id = str(skill_id).strip().lower()
        if skill_id not in SKILL_BY_ID:
            return
        if enabled:
            self._ids.add(skill_id)
        else:
            self._ids.discard(skill_id)

    def replace(self, skill_ids: Iterable[str] | None) -> None:
        self._ids = set(normalize_skill_ids(skill_ids))

    def ids(self) -> list[str]:
        return [skill_id for skill_id in DEFAULT_SKILL_IDS if skill_id in self._ids]

    def summary(self) -> str:
        return compact_skill_summary(self.ids())
