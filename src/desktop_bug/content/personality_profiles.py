"""Compact temperament catalog for creature personalities.

Personality means stable temperament and phase preference. Jobs/professions live
in :mod:`desktop_bug.jobs`, and true capabilities live in :mod:`desktop_bug.skills`.
Legacy JSON files remain readable, but new behavior work should compose one of
the compact temperament definitions instead of adding another specialist label.
"""

from __future__ import annotations

from copy import deepcopy


# Personality is a stable temperament, not a profession or a one-off action.
# Every value is intentionally human-readable: 0 means "almost never" and 10
# means "very often/strongly".  Jobs and true capabilities live in jobs.py and
# skills.py respectively.
TEMPERAMENT_TRAITS = (
    ("energy", "Energy", "How often the spider chooses active movement."),
    ("curiosity", "Curiosity", "How strongly it investigates new targets."),
    ("boldness", "Boldness", "How readily it approaches risk or a target."),
    ("sociability", "Sociability", "How often it seeks contact with friends."),
    ("patience", "Patience", "How long it watches or holds a calm phase."),
    ("caution", "Caution", "How readily it retreats from sudden threats."),
)
TEMPERAMENT_TRAIT_IDS = tuple(item[0] for item in TEMPERAMENT_TRAITS)

# Six deliberately broad presets replace the old collection of overlapping
# labels in the launch menu.  The old JSON IDs remain loadable as compatibility
# aliases, so existing presets do not break.
COMPACT_TEMPERAMENTS = {
    "balanced": {
        "display_name": "Balanced",
        "description": "A steady mix of roaming, curiosity, and caution.",
        "traits": {"energy": 5, "curiosity": 5, "boldness": 5, "sociability": 5, "patience": 5, "caution": 5},
    },
    "playful": {
        "display_name": "Playful",
        "description": "Energetic, social, and prone to cheerful movement phases.",
        "traits": {"energy": 8, "curiosity": 6, "boldness": 6, "sociability": 8, "patience": 3, "caution": 3},
    },
    "curious": {
        "display_name": "Curious",
        "description": "Investigates objects and pauses to study them.",
        "traits": {"energy": 6, "curiosity": 10, "boldness": 5, "sociability": 5, "patience": 7, "caution": 4},
    },
    "bold": {
        "display_name": "Bold",
        "description": "Confident and quick to approach or pursue.",
        "traits": {"energy": 8, "curiosity": 6, "boldness": 10, "sociability": 4, "patience": 2, "caution": 1},
    },
    "cautious": {
        "display_name": "Cautious",
        "description": "Patient and observant, with a strong threat response.",
        "traits": {"energy": 4, "curiosity": 5, "boldness": 2, "sociability": 3, "patience": 8, "caution": 10},
    },
    "social": {
        "display_name": "Social",
        "description": "Friendly and connection-seeking without being frantic.",
        "traits": {"energy": 5, "curiosity": 6, "boldness": 4, "sociability": 10, "patience": 7, "caution": 4},
    },
}
COMPACT_TEMPERAMENT_IDS = tuple(COMPACT_TEMPERAMENTS)


def _clamp_trait(value) -> float:
    try:
        return max(0.0, min(10.0, float(value)))
    except (TypeError, ValueError):
        return 5.0


def temperament_for(personality) -> dict[str, float]:
    """Return the six clear 0..10 temperament values for any personality.

    New compact definitions use ``temperament`` directly. Legacy definitions
    are projected into the same space by their old id/flags, which makes the
    scheduler and future jobs independent of legacy specialist names.
    """
    pid = str(personality.get("id", "") if isinstance(personality, dict) else personality or "").strip().lower()
    if isinstance(personality, dict):
        raw = personality.get("temperament", personality.get("traits"))
        if isinstance(raw, dict):
            return {key: _clamp_trait(raw.get(key, 5.0)) for key in TEMPERAMENT_TRAIT_IDS}
    if pid in COMPACT_TEMPERAMENTS:
        return {key: float(value) for key, value in COMPACT_TEMPERAMENTS[pid]["traits"].items()}
    # A compact projection for old specialist ids. Their old runtime flags are
    # still honored elsewhere, but no longer define the new menu taxonomy.
    legacy = {
        "bashful": "cautious", "shy": "cautious", "skittish": "cautious", "nope": "cautious",
        "mellow": "balanced", "sleepy": "balanced", "camouflage": "cautious", "grumpy": "bold",
        "clingy": "social", "cuddly": "social", "explorer": "curious", "observer": "curious",
        "jumper": "playful", "playful": "playful", "zoomy": "playful", "drifter": "playful",
        "hunter": "bold", "trapper": "bold", "webber": "curious", "weaver": "curious", "webslinger": "bold",
    }
    profile_id = legacy.get(pid, "balanced")
    return {key: float(value) for key, value in COMPACT_TEMPERAMENTS[profile_id]["traits"].items()}


def canonical_personality_definitions() -> dict[str, dict]:
    """Build menu-ready personality data without six duplicate JSON files."""
    result = {}
    for pid, definition in COMPACT_TEMPERAMENTS.items():
        traits = deepcopy(definition["traits"])
        energy = traits["energy"]
        caution = traits["caution"]
        result[pid] = {
            "id": pid,
            "display_name": definition["display_name"],
            "description": definition["description"],
            "mood": pid if pid in ("playful", "curious") else ("calm" if pid in ("balanced", "cautious") else "cuddly"),
            "temperament": traits,
            "speed_multiplier": 0.82 + energy * 0.035,
            "reaction_radius": 260 + traits["curiosity"] * 34,
            "threat_radius": 95 + caution * 10,
            "threat_cursor_speed": 950 + caution * 75,
            "boldness": traits["boldness"] / 10.0,
            "wander_frequency": 0.18 + energy * 0.045,
            "idle_time": [max(0.35, 2.8 - energy * 0.18), max(1.0, 5.2 - energy * 0.25)],
            "move_time": [0.65, 1.2 + energy * 0.15],
            "phase_duration_multiplier": 0.78 + traits["patience"] * 0.05,
            "movement_profile": "curious" if traits["curiosity"] >= 8 else ("bold" if traits["boldness"] >= 8 else "gentle"),
            "include_common_abilities": True,
            "_canonical": True,
        }
    return result


def selectable_personality_ids(personalities: dict | None = None, include_id: str | None = None) -> tuple[str, ...]:
    """Return the compact menu plus every legacy personality that actually ships.

    DC-60 narrows this back to the six, at the owner's request: *"some of
    them could be consolidated and left only a few since they kinda look the
    same."* Twenty-one personality files already shared eleven movement
    profiles between them, so most of the list was distinctions without a
    visible difference.

    Nothing is deleted. Every legacy id still loads, still resolves to its
    own traits, abilities and movement profile, and is still offered *in the
    row that already uses it* through ``include_id`` -- so a preset or a
    saved spider keeps exactly the personality its author chose, and loading
    one then saving does not silently rewrite it. They are simply no longer
    offered as fresh choices.

    (This reverses DC-19's C5 fix, which widened the list because the UI and
    the data disagreed about what existed. They agree again, in the other
    direction: the menu offers what a person should pick from, and
    ``include_id`` keeps it honest about what is already in use.)
    """
    values = list(COMPACT_TEMPERAMENT_IDS)
    legacy_id = str(include_id or "").strip().lower()
    if legacy_id and legacy_id not in values and (not personalities or legacy_id in personalities):
        values.append(legacy_id)
    return tuple(values)


# Behaviour modules replace the personality-id conditionals that used to sit
# inside ``creature/behaviour.py`` (DC-19, C5: the trait system above already
# projects every personality into continuous temperament, but the legacy
# specialist flags still drove behaviour through hard-coded id branches --
# two sources of truth). A module is selected either by an explicit
# ``behaviour_modules`` list in the personality JSON (every personality
# shipped in ``personalities/`` that needs one now carries it), or, for an
# older personality file that has not been migrated, by the same legacy
# boolean flags/id equality the old personality-id branches in
# ``creature/behaviour.py`` checked -- preserved here unchanged so an
# un-migrated custom personality keeps behaving exactly as it did before
# this package.
BEHAVIOUR_MODULE_IDS = ("hunter", "jumper", "observer", "nope", "drifter", "webber", "web_shooter")

_LEGACY_MODULE_FLAGS = {
    "mouse_hunter": "hunter",
    "constant_small_hops": "jumper",
    "horizontal_orbit_observer": "observer",
    "nope_escape": "nope",
    "drifter": "drifter",
    "drift_movement": "drifter",
    "web_weaver": "webber",
    "webber": "webber",
    "web_shooter": "web_shooter",
}

_LEGACY_MODULE_IDS = {
    "hunter": "hunter",
    "jumper": "jumper",
    "observer": "observer",
    "nope": "nope",
    "drifter": "drifter",
    "webber": "webber",
    "weaver": "webber",
    "trapper": "web_shooter",
    "webslinger": "web_shooter",
}


def _flag_enabled(personality: dict, key: str) -> bool:
    value = personality.get(key, False)
    if isinstance(value, str):
        return value.strip().lower() in ("1", "true", "yes", "on")
    return bool(value)


def behaviour_modules_for(personality) -> frozenset[str]:
    """Return the behaviour-module ids a personality selects.

    This does not include job-linked modules (a Hunter job or a Scout job
    granting hunter/observer regardless of personality) or the runtime skill
    gate (``has_skill(...)``): both stay dynamic, per-creature checks in
    ``creature/behaviour.py`` alongside this, exactly as the personality-id
    branches this replaces used to check ``job_id``/``has_skill`` themselves.
    """
    if not isinstance(personality, dict):
        pid = str(personality or "").strip().lower()
        module = _LEGACY_MODULE_IDS.get(pid)
        return frozenset({module}) if module else frozenset()

    explicit = personality.get("behaviour_modules")
    if isinstance(explicit, (list, tuple, set, frozenset)):
        return frozenset(
            str(item).strip().lower() for item in explicit
            if str(item).strip().lower() in BEHAVIOUR_MODULE_IDS
        )

    pid = str(personality.get("id", "")).strip().lower()
    modules = set()
    id_module = _LEGACY_MODULE_IDS.get(pid)
    if id_module:
        modules.add(id_module)
    for flag, module_id in _LEGACY_MODULE_FLAGS.items():
        if _flag_enabled(personality, flag):
            modules.add(module_id)
    return frozenset(modules)

# Reusable movement/behavior aspects.  These are not abilities: they describe
# how a creature moves or chooses states.  A full personality can combine one
# of these with one or more ability bundles below.
MOVEMENT_PROFILES = {
    "gentle": "slow, cautious, low-pressure roaming",
    "social": "friendly approach and close-range interaction",
    "curious": "exploration, inspection, and measured pursuit",
    "bold": "confident pursuit with stronger acceleration",
    "hunter": "persistent cursor/prey pursuit",
    "observer": "orbiting and deliberate watching",
    "jumper": "playful hop and pounce movement",
    "escape": "rapid threat avoidance and evasive movement",
    "drift": "momentum-based sliding and wide turns",
    "camouflage": "quiet movement with desktop hiding",
    "webber": "calm movement around web construction",
}


# High-level behaviour phases.  These are intentionally not abilities: they
# describe a temporary action period chosen by the scheduler.  Scores are on a
# 0..10 scale; zero removes a phase from the schedule, while ten gives it the
# strongest occurrence weight and guarantees it is represented in a cycle.
BEHAVIOUR_PHASE_IDS = (
    "wander",
    "jump",
    "roll",
    "zoomies",
    "approach",
    "chase",
    "observe",
    "prepare_jump_attack",
    "inspect",
    "cuddle",
    "social_play",
    "run_away",
)

DEFAULT_PHASE_SCORES = {phase_id: 3.0 for phase_id in BEHAVIOUR_PHASE_IDS}

# Shared movement templates keep similar personalities compact.  A personality
# id selects one movement profile below; a JSON ``phase_scores`` object can
# override individual values later without creating another personality class.
PHASE_SCORE_PROFILES = {
    "gentle": {
        "wander": 8, "approach": 4, "observe": 5, "inspect": 5,
        "cuddle": 4, "social_play": 2, "chase": 1,
        "jump": 1, "roll": 1, "zoomies": 1, "prepare_jump_attack": 1, "run_away": 1,
    },
    "social": {
        "wander": 5, "approach": 5, "observe": 3, "inspect": 6,
        "cuddle": 9, "social_play": 10, "chase": 3,
        "jump": 4, "roll": 4, "zoomies": 3, "prepare_jump_attack": 2, "run_away": 1,
    },
    "curious": {
        "wander": 5, "approach": 7, "observe": 7, "inspect": 10,
        "cuddle": 3, "social_play": 4, "chase": 3,
        "jump": 2, "roll": 2, "zoomies": 2, "prepare_jump_attack": 3, "run_away": 1,
    },
    "bold": {
        "wander": 4, "approach": 6, "observe": 2, "inspect": 3,
        "cuddle": 2, "social_play": 5, "chase": 9,
        "jump": 5, "roll": 3, "zoomies": 8, "prepare_jump_attack": 8, "run_away": 1,
    },
    "hunter": {
        "wander": 2, "approach": 9, "observe": 6, "inspect": 3,
        # A hunter still has the pounce/attack phases, but does not randomly
        # tumble, zoom, cuddle, or hop when no prey is available.
        "cuddle": 0, "social_play": 0, "chase": 10,
        "jump": 0, "roll": 0, "zoomies": 0, "prepare_jump_attack": 8, "run_away": 1,
    },
    "observer": {
        "wander": 4, "approach": 4, "observe": 10, "inspect": 8,
        "cuddle": 2, "social_play": 3, "chase": 1,
        "jump": 1, "roll": 1, "zoomies": 1, "prepare_jump_attack": 1, "run_away": 1,
    },
    "jumper": {
        "wander": 4, "approach": 4, "observe": 2, "inspect": 2,
        "cuddle": 2, "social_play": 5, "chase": 5,
        "jump": 10, "roll": 6, "zoomies": 8, "prepare_jump_attack": 9, "run_away": 2,
    },
    "escape": {
        "wander": 1, "approach": 0, "observe": 0, "inspect": 0,
        "cuddle": 0, "social_play": 0, "chase": 0,
        "jump": 8, "roll": 0, "zoomies": 0, "prepare_jump_attack": 7, "run_away": 10,
    },
    "drift": {
        "wander": 5, "approach": 3, "observe": 2, "inspect": 2,
        "cuddle": 1, "social_play": 2, "chase": 4,
        "jump": 3, "roll": 6, "zoomies": 9, "prepare_jump_attack": 2, "run_away": 2,
    },
    "camouflage": {
        "wander": 7, "approach": 3, "observe": 6, "inspect": 5,
        "cuddle": 1, "social_play": 1, "chase": 1,
        "jump": 0, "roll": 0, "zoomies": 0, "prepare_jump_attack": 0, "run_away": 4,
    },
    "webber": {
        "wander": 6, "approach": 4, "observe": 5, "inspect": 7,
        "cuddle": 2, "social_play": 2, "chase": 1,
        "jump": 1, "roll": 1, "zoomies": 1, "prepare_jump_attack": 1, "run_away": 1,
    },
}

# Profile-level pacing is deliberately separate from occurrence scores.  For
# example, an observer can choose a phase less often but remain in it longer.
PHASE_DURATION_MULTIPLIERS = {
    "gentle": 1.15,
    "social": 1.00,
    "curious": 1.10,
    "bold": 0.82,
    "hunter": 0.72,
    "observer": 1.30,
    "jumper": 0.62,
    "escape": 0.48,
    "drift": 0.70,
    "camouflage": 1.25,
    "webber": 1.18,
}


# True capabilities live here, separately from movement personality.  The ids
# are skill ids so the existing settings UI and runtime context menu can expose
# them without inventing another permission system.
ABILITY_BUNDLES = {
    "web_walker": ("web_walk",),
    "web_weaver": ("weave_web",),
    "web_trapper": ("shoot_web", "wall_web"),
    "drifter": ("drift",),
}


# Compact descriptions of the shipped personalities.  Legacy JSON flags still
# work; this table is the canonical place to add a new composition later.
PERSONALITY_COMPONENTS = {
    "bashful": {"movement": "gentle"},
    "bold": {"movement": "bold"},
    "camouflage": {"movement": "camouflage"},
    "clingy": {"movement": "social"},
    "cuddly": {"movement": "social"},
    "curious": {"movement": "curious"},
    "drifter": {"movement": "drift", "abilities": ("drifter",)},
    "explorer": {"movement": "curious"},
    "grumpy": {"movement": "gentle"},
    "hunter": {"movement": "hunter"},
    "jumper": {"movement": "jumper"},
    "mellow": {"movement": "gentle"},
    "nope": {"movement": "escape"},
    "observer": {"movement": "observer"},
    "playful": {"movement": "jumper"},
    "shy": {"movement": "gentle"},
    "skittish": {"movement": "escape"},
    "sleepy": {"movement": "gentle"},
    "trapper": {"movement": "hunter", "abilities": ("web_trapper",)},
    "webber": {"movement": "webber", "abilities": ("web_weaver",)},
    "weaver": {"movement": "webber", "abilities": ("web_weaver",)},
    "webslinger": {"movement": "hunter", "abilities": ("web_trapper",)},
    "zoomy": {"movement": "bold"},
}


# Backward-compatible flag aliases.  New definitions should use ``abilities``
# or a bundle name above; old editable personality files need not change all at
# once.
ABILITY_FLAGS = {
    "web_weaver": ("web_weaver",),
    "webber": ("web_weaver",),
    "web_shooter": ("web_trapper",),
    "drifter": ("drifter",),
    "drift_movement": ("drifter",),
}


def movement_profile_for(personality) -> str:
    """Return the compact movement profile id for a personality object."""
    if isinstance(personality, dict):
        explicit = str(personality.get("movement_profile", "")).strip().lower()
        if explicit in MOVEMENT_PROFILES:
            return explicit
        pid = str(personality.get("id", "")).strip().lower()
    else:
        pid = str(personality or "").strip().lower()
    return str(PERSONALITY_COMPONENTS.get(pid, {}).get("movement", "curious"))


# DC-52: which walking animation a temperament uses.
#
# The gait used to be one scene-wide dropdown -- Classic, Lively, Skitter --
# sitting beside a Temperament column that already decided how each spider
# moves. The owner asked for the two to be consolidated, so the temperament
# picks. Classic is deliberately unreachable from here: it is the original
# pre-DC-16 gait, kept only so an old preset that names it still loads.
GAIT_BY_MOVEMENT: dict[str, str] = {
    # Quick, twitchy movers get the burst-and-stop gait.
    "bold": "skitter",
    "escape": "skitter",
    "hunter": "skitter",
    "jumper": "skitter",
    # Everything else lifts its legs and feels its way around.
    "camouflage": "lively",
    "curious": "lively",
    "drift": "lively",
    "gentle": "lively",
    "observer": "lively",
    "social": "lively",
    "webber": "lively",
}


def personality_gait_style(personality) -> str:
    """The walking animation this temperament uses."""
    return GAIT_BY_MOVEMENT.get(movement_profile_for(personality), "lively")


def phase_scores_for(personality) -> dict[str, float]:
    """Return clamped 0..10 occurrence scores for the behaviour phases."""
    traits = temperament_for(personality)
    pid = str(personality.get("id", "") if isinstance(personality, dict) else personality or "").strip().lower()
    if pid in COMPACT_TEMPERAMENTS or (isinstance(personality, dict) and personality.get("_canonical")):
        # The phase deck is the personality's *temporary action rhythm*. Each
        # phase is derived from stable temperament values, making the meaning of
        # the 0..10 editor transparent instead of hiding specialist jobs in it.
        scores = {
            "wander": 3.0 + traits["energy"] * 0.35 + traits["patience"] * 0.20,
            "jump": traits["energy"] * 0.52 + traits["boldness"] * 0.18 - traits["caution"] * 0.18,
            "roll": traits["energy"] * 0.28 + traits["sociability"] * 0.18,
            "zoomies": traits["energy"] * 0.62 + traits["boldness"] * 0.20 - traits["patience"] * 0.22,
            "approach": 2.0 + traits["boldness"] * 0.48 + traits["curiosity"] * 0.18 - traits["caution"] * 0.22,
            "chase": traits["boldness"] * 0.58 + traits["energy"] * 0.28 - traits["caution"] * 0.24,
            "observe": 2.0 + traits["patience"] * 0.48 + traits["curiosity"] * 0.28,
            "prepare_jump_attack": traits["boldness"] * 0.46 + traits["energy"] * 0.22 - traits["caution"] * 0.18,
            "inspect": 2.0 + traits["curiosity"] * 0.62 + traits["patience"] * 0.16,
            "cuddle": traits["sociability"] * 0.62 + traits["patience"] * 0.18 - traits["caution"] * 0.12,
            "social_play": traits["sociability"] * 0.66 + traits["energy"] * 0.22,
            "run_away": traits["caution"] * 0.78 - traits["boldness"] * 0.20,
        }
        scores = {key: max(0.0, min(10.0, round(value, 2))) for key, value in scores.items()}
        if isinstance(personality, dict):
            overrides = personality.get("phase_scores")
            if isinstance(overrides, dict):
                for phase_id, value in overrides.items():
                    phase_id = str(phase_id).strip().lower()
                    if phase_id in scores:
                        scores[phase_id] = _clamp_trait(value)
        return scores
    profile = movement_profile_for(personality)
    scores = dict(DEFAULT_PHASE_SCORES)
    scores.update(PHASE_SCORE_PROFILES.get(profile, {}))
    if isinstance(personality, dict):
        overrides = personality.get("phase_scores")
        if isinstance(overrides, dict):
            for phase_id, value in overrides.items():
                phase_id = str(phase_id).strip().lower()
                if phase_id not in scores:
                    continue
                try:
                    scores[phase_id] = max(0.0, min(10.0, float(value)))
                except (TypeError, ValueError):
                    continue
    return scores


def phase_duration_multiplier_for(personality) -> float:
    """Return the personality's overall phase pacing multiplier."""
    profile = movement_profile_for(personality)
    default = PHASE_DURATION_MULTIPLIERS.get(profile, 1.0)
    if not isinstance(personality, dict) or "phase_duration_multiplier" not in personality:
        return default
    try:
        return max(0.25, min(2.5, float(personality["phase_duration_multiplier"])))
    except (TypeError, ValueError):
        return default


def ability_bundle_ids_for(personality) -> list[str]:
    """Return ability-bundle ids selected by a personality definition."""
    if not isinstance(personality, dict):
        pid = str(personality or "").strip().lower()
        component = PERSONALITY_COMPONENTS.get(pid, {})
        return list(component.get("abilities", ()))

    bundles: list[str] = []
    explicit = personality.get("ability_bundles")
    if isinstance(explicit, (list, tuple)):
        bundles.extend(str(item).strip().lower() for item in explicit)
    pid = str(personality.get("id", "")).strip().lower()
    bundles.extend(PERSONALITY_COMPONENTS.get(pid, {}).get("abilities", ()))
    for flag, aliases in ABILITY_FLAGS.items():
        value = personality.get(flag, False)
        enabled = value.strip().lower() in ("1", "true", "yes", "on") if isinstance(value, str) else bool(value)
        if enabled:
            bundles.extend(aliases)
    return list(dict.fromkeys(bundle for bundle in bundles if bundle in ABILITY_BUNDLES))


def ability_ids_for(personality) -> list[str]:
    """Expand explicit ability ids and compact bundle ids into skill ids."""
    values: list[str] = []
    if isinstance(personality, dict):
        explicit = personality.get("abilities")
        if isinstance(explicit, (list, tuple)):
            values.extend(str(item).strip().lower() for item in explicit)
    for bundle_id in ability_bundle_ids_for(personality):
        values.extend(ABILITY_BUNDLES[bundle_id])
    return list(dict.fromkeys(value for value in values if value))


def annotate_personality(personality: dict) -> dict:
    """Add non-invasive composition metadata to a loaded personality."""
    data = dict(personality)
    data.setdefault("temperament", temperament_for(data))
    data.setdefault("movement_profile", movement_profile_for(data))
    data.setdefault("phase_scores", phase_scores_for(data))
    data.setdefault("phase_duration_multiplier", phase_duration_multiplier_for(data))
    data.setdefault("ability_ids", ability_ids_for(data))
    return data
