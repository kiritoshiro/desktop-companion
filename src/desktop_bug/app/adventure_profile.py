"""The Adventure hero, saved apart from the Companion colony.

The owner: *"on adventure mode should allow to name the spider. and the spider
should begin with the first level. and as progress keep it level xp and skills
chosen saved."* One small file, ``adventure-hero.json`` in the state folder,
read by both the settings window (to name the hero and spend its points) and
the overlay (to play it):

- ``name``: what the hero is called;
- ``progression``: level, XP, unspent points, chosen skills and armour, or
  missing for a hero who has not played yet -- who then starts at level 1;
- ``missions``: per mission, victories, defeats and the fastest win;
- ``armoury``: the armour the player owns, shared by the hero and every
  companion -- which pieces, their levels, spare duplicates and amber;
- ``companions``: the companions unlocked so far, each with its own name and
  progression (level, skills, what it wears);
- ``selected_map``: the map the next raid is played on.

Version 1 files (the first mission build) held only ``progression``, version 2
files had no armoury or companions; both still load. A piece is worn by one
spider at a time: the armoury holds one of each, plus spares.
"""
from __future__ import annotations

import json
from pathlib import Path

from ..content.discovery import state_dir
from ..state.progression import ARMOR_BY_ID, MAX_ITEM_LEVEL, ProgressionState
from .campaign import COMPANION_BY_ID, DEFAULT_MAP, MAP_BY_ID, STARTING_COMPANIONS

PROFILE_VERSION = 3
HERO = "hero"
DEFAULT_HERO_NAME = "Wayfarer"
MAX_NAME_LENGTH = 24


def profile_path() -> Path:
    return state_dir() / "adventure-hero.json"


def clean_name(value) -> str:
    text = " ".join(str(value or "").split())[:MAX_NAME_LENGTH]
    return text or DEFAULT_HERO_NAME


def fresh_armoury(owned=None) -> dict:
    owned = list(ProgressionState().inventory if owned is None else owned)
    return {"owned": owned, "levels": {}, "spares": {}, "amber": 0}


def fresh_profile() -> dict:
    return {"version": PROFILE_VERSION, "name": DEFAULT_HERO_NAME, "progression": None,
            "missions": {}, "armoury": fresh_armoury(),
            "companions": {cid: {"name": COMPANION_BY_ID[cid].name, "progression": None}
                           for cid in STARTING_COMPANIONS},
            "selected_map": DEFAULT_MAP}


def _count(value, low=0, high=None) -> int:
    try:
        number = max(low, int(value))
    except (TypeError, ValueError):
        return low
    return number if high is None else min(high, number)


def _clean_armoury(raw, progression) -> dict:
    """The armoury from a file; from an older file, built from the hero's inventory."""
    if not isinstance(raw, dict):
        owned = (progression or {}).get("inventory") if isinstance(progression, dict) else None
        armoury = fresh_armoury(owned if isinstance(owned, list) else None)
        if isinstance(progression, dict) and isinstance(progression.get("item_levels"), dict):
            raw = {"levels": progression["item_levels"]}
        else:
            raw = {}
    else:
        armoury = fresh_armoury([])
        armoury["owned"] = list(raw.get("owned") or [])
    armoury["owned"] = list(dict.fromkeys(i for i in armoury["owned"]
                                          if isinstance(i, str) and i in ARMOR_BY_ID))
    armoury["levels"] = {str(k): _count(v, 1, MAX_ITEM_LEVEL) for k, v in (raw.get("levels") or {}).items()
                         if str(k) in armoury["owned"] and _count(v, 1, MAX_ITEM_LEVEL) > 1}
    armoury["spares"] = {str(k): _count(v) for k, v in (raw.get("spares") or {}).items()
                         if str(k) in ARMOR_BY_ID and _count(v) > 0}
    armoury["amber"] = _count(raw.get("amber", 0))
    return armoury


def _clean_companions(raw) -> dict:
    companions = {cid: {"name": COMPANION_BY_ID[cid].name, "progression": None}
                  for cid in STARTING_COMPANIONS}
    if isinstance(raw, dict):
        for cid, entry in raw.items():
            if cid not in COMPANION_BY_ID or not isinstance(entry, dict):
                continue
            progression = entry.get("progression")
            companions[cid] = {
                "name": clean_name(entry.get("name") or COMPANION_BY_ID[cid].name),
                "progression": (ProgressionState.from_dict(progression).to_dict()
                                if isinstance(progression, dict) else None),
            }
    return companions


def load_profile(path: Path | None = None) -> dict:
    path = Path(path) if path is not None else profile_path()
    profile = fresh_profile()
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return profile
    if not isinstance(raw, dict):
        return profile
    profile["name"] = clean_name(raw.get("name"))
    if isinstance(raw.get("progression"), dict):
        profile["progression"] = ProgressionState.from_dict(raw["progression"]).to_dict()
    profile["armoury"] = _clean_armoury(raw.get("armoury"), raw.get("progression"))
    profile["companions"] = _clean_companions(raw.get("companions"))
    selected = str(raw.get("selected_map") or DEFAULT_MAP)
    profile["selected_map"] = selected if selected in MAP_BY_ID else DEFAULT_MAP
    missions = raw.get("missions")
    if isinstance(missions, dict):
        for mission_id, record in missions.items():
            if isinstance(record, dict):
                profile["missions"][str(mission_id)] = _clean_record(record)
    return profile


def _clean_record(record: dict) -> dict:
    def count(key):
        try:
            return max(0, int(record.get(key, 0)))
        except (TypeError, ValueError):
            return 0
    best = record.get("best_seconds")
    try:
        best = float(best) if best is not None else None
    except (TypeError, ValueError):
        best = None
    return {"victories": count("victories"), "defeats": count("defeats"), "best_seconds": best}


def save_profile(profile: dict, path: Path | None = None) -> bool:
    path = Path(path) if path is not None else profile_path()
    data = {
        "version": PROFILE_VERSION,
        "name": clean_name(profile.get("name")),
        "progression": profile.get("progression"),
        "missions": {k: _clean_record(v) for k, v in (profile.get("missions") or {}).items()},
        "armoury": profile.get("armoury") or fresh_armoury(),
        "companions": profile.get("companions") or _clean_companions(None),
        "selected_map": profile.get("selected_map") or DEFAULT_MAP,
    }
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        temp = path.with_suffix(".tmp")
        temp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        temp.replace(path)
        return True
    except OSError:
        return False


def _dress(state: ProgressionState, profile: dict) -> ProgressionState:
    """Give a spider's progression the shared armoury: what is owned and at what
    level. Pieces it wore that are no longer owned come off."""
    armoury = profile.get("armoury") or fresh_armoury()
    state.inventory = list(armoury["owned"])
    state.item_levels = dict(armoury.get("levels") or {})
    state.equipped = {slot: item for slot, item in state.equipped.items() if item in armoury["owned"]}
    return state


def hero_progression(profile: dict) -> ProgressionState:
    """The saved hero, or a new one at level 1, dressed from the armoury."""
    raw = profile.get("progression")
    state = ProgressionState.from_dict(raw) if isinstance(raw, dict) else ProgressionState()
    return _dress(state, profile)


def companion_progression(profile: dict, companion_id: str) -> ProgressionState:
    """A companion's saved progression; a new one starts at the hero's level."""
    entry = (profile.get("companions") or {}).get(companion_id) or {}
    raw = entry.get("progression")
    if isinstance(raw, dict):
        state = ProgressionState.from_dict(raw)
    else:
        state = ProgressionState(level=hero_progression(profile).level)
        state.equipped = {}
    return _dress(state, profile)


def spider_progression(profile: dict, who: str) -> ProgressionState:
    return hero_progression(profile) if who == HERO else companion_progression(profile, who)


def store_progression(profile: dict, who: str, state: ProgressionState) -> None:
    if who == HERO:
        profile["progression"] = state.to_dict()
    else:
        entry = profile.setdefault("companions", {}).setdefault(
            who, {"name": COMPANION_BY_ID[who].name, "progression": None})
        entry["progression"] = state.to_dict()


def unlock_companion(profile: dict, companion_id: str) -> bool:
    """Add a companion, at the hero's level. False if it was already unlocked."""
    companions = profile.setdefault("companions", {})
    if companion_id in companions or companion_id not in COMPANION_BY_ID:
        return False
    companions[companion_id] = {"name": COMPANION_BY_ID[companion_id].name, "progression": None}
    store_progression(profile, companion_id, companion_progression(profile, companion_id))
    return True


def record_result(profile: dict, mission_id: str, won: bool, seconds: float) -> dict:
    record = _clean_record(profile.setdefault("missions", {}).get(mission_id, {}))
    if won:
        record["victories"] += 1
        if record["best_seconds"] is None or seconds < record["best_seconds"]:
            record["best_seconds"] = round(float(seconds), 1)
    else:
        record["defeats"] += 1
    profile["missions"][mission_id] = record
    return record


def mission_record(profile: dict, mission_id: str) -> dict:
    return _clean_record((profile.get("missions") or {}).get(mission_id, {}))
