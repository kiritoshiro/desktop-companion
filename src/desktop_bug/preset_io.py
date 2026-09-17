import json
import re
from pathlib import Path
from typing import Dict

from .discovery import is_shipped_preset, user_presets_dir
from .skills import SKILL_BY_ID
from .jobs import JOB_BY_ID

_SAFE_NAME_RE = re.compile(r"[^A-Za-z0-9_. -]+")


def _validate_rgb_overrides(value, label: str) -> None:
    """Validate an optional preset palette without tying it to one model."""
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    for key, rgb in value.items():
        if not isinstance(key, str) or not key.strip():
            raise ValueError(f"{label} keys must be non-empty strings")
        if not isinstance(rgb, (list, tuple)) or len(rgb) != 3:
            raise ValueError(f"{label}.{key} must be an RGB triplet")
        for channel in rgb:
            if isinstance(channel, bool):
                raise ValueError(f"{label}.{key} channels must be numbers from 0 to 255")
            try:
                numeric = float(channel)
            except (TypeError, ValueError) as exc:
                raise ValueError(f"{label}.{key} channels must be numbers from 0 to 255") from exc
            if not 0.0 <= numeric <= 255.0:
                raise ValueError(f"{label}.{key} channels must be numbers from 0 to 255")


def safe_preset_filename(name: str) -> str:
    clean = _SAFE_NAME_RE.sub("_", name.strip()).strip(" ._")
    if not clean:
        clean = "preset"
    return f"{clean}.json"


def validate_preset(data: dict) -> None:
    if not isinstance(data, dict):
        raise ValueError("Preset must be a JSON object")
    if not isinstance(data.get("name"), str) or not data["name"].strip():
        raise ValueError("Preset requires a non-empty name")
    if not isinstance(data.get("slots"), list):
        raise ValueError("Preset requires a slots list")

    settings = data.get("settings", {})
    if settings is not None:
        if not isinstance(settings, dict):
            raise ValueError("Preset settings must be an object")
        if "size_scale" in settings:
            try:
                size_scale = float(settings["size_scale"])
            except Exception as exc:
                raise ValueError("Preset settings.size_scale must be a number") from exc
            if size_scale < 0.45 or size_scale > 2.25:
                raise ValueError("Preset settings.size_scale must be between 0.45 and 2.25")
        if "interferable" in settings and not isinstance(settings["interferable"], bool):
            raise ValueError("Preset settings.interferable must be true or false")
        if "social_play" in settings and not isinstance(settings["social_play"], bool):
            raise ValueError("Preset settings.social_play must be true or false")
        if "mood_mode" in settings:
            valid_moods = {"auto", "playful", "cuddly", "curious", "calm"}
            if not isinstance(settings["mood_mode"], str) or settings["mood_mode"].lower() not in valid_moods:
                raise ValueError("Preset settings.mood_mode must be auto, playful, cuddly, curious, or calm")
        if "team_relations" in settings and settings["team_relations"] is not None:
            relations = settings["team_relations"]
            if not isinstance(relations, dict):
                raise ValueError("Preset settings.team_relations must be an object")
            for left, row in relations.items():
                if not isinstance(left, str) or not left.strip():
                    raise ValueError("Preset settings.team_relations keys must be team names")
                if not isinstance(row, dict):
                    raise ValueError(f"Preset settings.team_relations.{left} must be an object")
                for right, relation in row.items():
                    if not isinstance(right, str) or not right.strip():
                        raise ValueError(f"Preset settings.team_relations.{left} keys must be team names")
                    if not isinstance(relation, str) or relation.strip().lower() not in ("friend", "neutral", "foe"):
                        raise ValueError(
                            f"Preset settings.team_relations.{left}.{right} must be friend, neutral, or foe"
                        )
        if "flies" in settings and settings["flies"] is not None:
            flies = settings["flies"]
            if not isinstance(flies, dict):
                raise ValueError("Preset settings.flies must be an object")
            if "enabled" in flies and not isinstance(flies["enabled"], bool):
                raise ValueError("Preset settings.flies.enabled must be true or false")
            for key in ("min_interval", "max_interval"):
                if key in flies:
                    try:
                        value = float(flies[key])
                    except Exception as exc:
                        raise ValueError(f"Preset settings.flies.{key} must be a number") from exc
                    if value <= 0 or value > 600:
                        raise ValueError(f"Preset settings.flies.{key} must be between 0 and 600 seconds")
            if "max_flies" in flies:
                try:
                    max_flies = int(flies["max_flies"])
                except Exception as exc:
                    raise ValueError("Preset settings.flies.max_flies must be an integer") from exc
                if max_flies < 0 or max_flies > 40:
                    raise ValueError("Preset settings.flies.max_flies must be between 0 and 40")
            if "spawner" in flies and not isinstance(flies["spawner"], bool):
                raise ValueError("Preset settings.flies.spawner must be true or false")

    for index, slot in enumerate(data["slots"]):
        if not isinstance(slot, dict):
            raise ValueError(f"Slot {index + 1} must be an object")
        for key in ["model", "personality", "count"]:
            if key not in slot:
                raise ValueError(f"Slot {index + 1} missing {key}")
        if not isinstance(slot["model"], str) or not slot["model"].strip():
            raise ValueError(f"Slot {index + 1} model must be a non-empty string")
        if not isinstance(slot["personality"], str) or not slot["personality"].strip():
            raise ValueError(f"Slot {index + 1} personality must be a non-empty string")
        if "slot_id" in slot and (not isinstance(slot["slot_id"], str) or not slot["slot_id"].strip()):
            raise ValueError(f"Slot {index + 1} slot_id must be a non-empty string")
        for team_key in ("team", "team_id"):
            if team_key in slot and (not isinstance(slot[team_key], str) or not slot[team_key].strip()):
                raise ValueError(f"Slot {index + 1} {team_key} must be a non-empty string")
        if "job" in slot:
            if not isinstance(slot["job"], str) or not slot["job"].strip():
                raise ValueError(f"Slot {index + 1} job must be a non-empty string")
            if slot["job"].strip().lower() not in JOB_BY_ID:
                raise ValueError(f"Slot {index + 1} has unknown job: {slot['job']}")
        if "count_random" in slot and not isinstance(slot["count_random"], bool):
            raise ValueError(f"Slot {index + 1} count_random must be true or false")
        if "skills" in slot:
            if not isinstance(slot["skills"], list):
                raise ValueError(f"Slot {index + 1} skills must be a list")
            for skill in slot["skills"]:
                if not isinstance(skill, str):
                    raise ValueError(f"Slot {index + 1} skills must contain only strings")
                if skill.strip().lower() not in SKILL_BY_ID:
                    raise ValueError(f"Slot {index + 1} has unknown skill: {skill}")
        if "abilities" in slot:
            if not isinstance(slot["abilities"], list):
                raise ValueError(f"Slot {index + 1} abilities must be a list")
            for ability in slot["abilities"]:
                if not isinstance(ability, str):
                    raise ValueError(f"Slot {index + 1} abilities must contain only strings")
                skill = SKILL_BY_ID.get(ability.strip().lower())
                if skill is None:
                    raise ValueError(f"Slot {index + 1} has unknown ability: {ability}")
                if skill.category != "Ability":
                    raise ValueError(f"Slot {index + 1} entry is not an ability: {ability}")
        if "colors" in slot:
            _validate_rgb_overrides(slot["colors"], f"Slot {index + 1} colors")
        try:
            count = int(slot["count"])
        except Exception as exc:
            raise ValueError(f"Slot {index + 1} count must be an integer") from exc
        if count < 1 or count > 50:
            raise ValueError(f"Slot {index + 1} count must be between 1 and 50")


def load_preset(path: Path) -> Dict:
    path = Path(path)
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    validate_preset(data)
    return data


def save_preset(data: Dict, path: Path = None) -> Path:
    """Write a preset the user owns, never one that shipped with the build.

    The saved filename comes from the preset's *name*, so a preset called
    ``Default`` used to be written to ``presets/Default.json`` -- the same file
    as the shipped ``presets/default.json`` on a case-insensitive filesystem.
    The first Save a user pressed silently replaced data that came with the
    application. Saves now go to the user's own preset directory, which is read
    before the shipped one, so their copy shadows it instead.
    """
    validate_preset(data)
    presets_dir = user_presets_dir()
    presets_dir.mkdir(parents=True, exist_ok=True)
    path = Path(path) if path else presets_dir / safe_preset_filename(data["name"])
    if not path.is_absolute():
        path = presets_dir / path
    if is_shipped_preset(path):
        raise ValueError(
            f"{path.name} ships with the application and is not writable. "
            "Saved presets go to your own preset folder instead."
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2)
        handle.write("\n")
    return path
