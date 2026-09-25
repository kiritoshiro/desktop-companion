"""The Adventure hero, saved apart from the Companion colony.

The owner: *"on adventure mode should allow to name the spider. and the spider
should begin with the first level. and as progress keep it level xp and skills
chosen saved."* One small file, ``adventure-hero.json`` in the state folder,
read by both the settings window (to name the hero and spend its points) and
the overlay (to play it):

- ``name``: what the hero is called;
- ``progression``: level, XP, unspent points, chosen skills and armour, or
  missing for a hero who has not played yet -- who then starts at level 1;
- ``missions``: per mission, victories, defeats and the fastest win.

Version 1 files (the first mission build) held only ``progression`` and still
load.
"""
from __future__ import annotations

import json
from pathlib import Path

from ..content.discovery import state_dir
from ..state.progression import ProgressionState

PROFILE_VERSION = 2
DEFAULT_HERO_NAME = "Wayfarer"
MAX_NAME_LENGTH = 24


def profile_path() -> Path:
    return state_dir() / "adventure-hero.json"


def clean_name(value) -> str:
    text = " ".join(str(value or "").split())[:MAX_NAME_LENGTH]
    return text or DEFAULT_HERO_NAME


def fresh_profile() -> dict:
    return {"version": PROFILE_VERSION, "name": DEFAULT_HERO_NAME, "progression": None, "missions": {}}


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
    }
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        temp = path.with_suffix(".tmp")
        temp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        temp.replace(path)
        return True
    except OSError:
        return False


def hero_progression(profile: dict) -> ProgressionState:
    """The saved hero, or a new one at level 1."""
    raw = profile.get("progression")
    return ProgressionState.from_dict(raw) if isinstance(raw, dict) else ProgressionState()


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
