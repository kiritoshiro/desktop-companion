import json
import os
import shutil
import sys
from pathlib import Path
from typing import Dict, List, Tuple

from .personality_profiles import (
    ABILITY_BUNDLES,
    BEHAVIOUR_MODULE_IDS,
    BEHAVIOUR_PHASE_IDS,
    MOVEMENT_PROFILES,
    TEMPERAMENT_TRAIT_IDS,
    annotate_personality,
    canonical_personality_definitions,
)
from .skills import ABILITY_SKILL_IDS, BEHAVIOUR_SKILL_IDS, SKILL_BY_ID


def app_root() -> Path:
    """Return the writable/editable data root.

    In development this is the project folder. In a PyInstaller build this is
    the folder containing the executable, so models/personalities/presets can
    remain editable beside the .exe.
    """
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    # content/discovery.py -> content -> desktop_bug -> src -> the project.
    # Counted from this file, so moving this module changes it: DC-43 moved
    # discovery.py one level deeper and every preset and model path silently
    # resolved inside src/ until this was corrected.
    return Path(__file__).resolve().parents[3]


def _unique_paths(paths: List[Path]) -> List[Path]:
    seen = set()
    result: List[Path] = []
    for path in paths:
        try:
            key = path.resolve()
        except Exception:
            key = path
        if key in seen:
            continue
        seen.add(key)
        result.append(path)
    return result


def _added_time(path: Path) -> float:
    """Return the platform's file creation time for newest-first listings."""
    try:
        return path.stat().st_ctime
    except OSError:
        return 0.0


def _newest_first(path: Path):
    return (-_added_time(path), path.parent.name.casefold(), path.name.casefold())


def candidate_roots(root: Path = None) -> List[Path]:
    """Return places where data folders may live.

    Order matters: editable folders beside the exe/project win over bundled
    PyInstaller data. This lets users override bundled models by placing edited
    files next to the executable.
    """
    roots: List[Path] = []
    if root is not None:
        roots.append(Path(root))

    # Anything the user saved comes first, so their own copy of a preset
    # shadows the one that shipped rather than overwriting it.
    roots.append(user_data_root())

    writable_root = app_root()
    roots.append(writable_root)

    # PyInstaller one-folder builds often place --add-data contents under
    # sys._MEIPASS, which is usually the _internal folder beside the executable.
    bundled = getattr(sys, "_MEIPASS", None)
    if bundled:
        roots.append(Path(bundled))

    # Explicit fallback for PyInstaller 6 one-folder layout.
    roots.append(writable_root / "_internal")

    # Helpful when launched from a shortcut whose working directory is still the
    # original project folder.
    roots.append(Path.cwd())

    return _unique_paths(roots)


def data_path(*parts: str) -> Path:
    """Return a writable path beside the project/executable."""
    return app_root().joinpath(*parts)


def user_data_root() -> Path:
    """Return the writable root for things the user creates.

    Saving used to derive a filename from the preset's *name*, so a preset
    called ``Default`` was written to ``presets/Default.json`` -- which on
    Windows is the same file as the shipped ``presets/default.json``. A user's
    first Save therefore overwrote data that ships with the build. User presets
    now live beside the runtime state, which is writable, already excluded from
    version control, and redirectable for tests.
    """
    return state_dir()


def user_presets_dir() -> Path:
    """Return the directory a user's own presets are written to."""
    return user_data_root() / "presets"


def shipped_presets_dirs() -> List[Path]:
    """Return the preset directories that came with the build.

    These are read-only as far as the application is concerned.
    """
    dirs: List[Path] = [app_root() / "presets"]
    bundled = getattr(sys, "_MEIPASS", None)
    if bundled:
        dirs.append(Path(bundled) / "presets")
    dirs.append(app_root() / "_internal" / "presets")
    return _unique_paths(dirs)


def is_shipped_preset(path) -> bool:
    """Return whether a path names a preset that came with the build.

    Compared case-insensitively, because the collision that caused this was
    ``Default.json`` against ``default.json`` on a case-insensitive filesystem.
    """
    try:
        candidate = Path(path).resolve()
    except Exception:
        candidate = Path(path)
    for directory in shipped_presets_dirs():
        try:
            resolved = directory.resolve()
        except Exception:
            resolved = directory
        if str(candidate.parent).casefold() == str(resolved).casefold():
            return True
    return False


def resolve_preset_path(value) -> Path:
    """Resolve a preset argument for *reading*, in source and frozen builds.

    Reading and writing resolve differently and must not be confused. A read
    searches every candidate root, including the directory a one-file build
    extracts itself into, because that is where bundled presets live. A write
    goes only to the writable root beside the project or executable, because
    the extraction directory is temporary and is discarded when the app exits.

    Resolving a relative path against the writable root alone is what made the
    packaged executable crash on ``--preset presets/colony.json``: it looked
    beside the ``.exe``, where a one-file build keeps no presets at all.

    A path that cannot be found is returned unchanged rather than raised on, so
    the caller reports the name the user actually typed.
    """
    path = Path(value)
    if path.is_absolute():
        return path

    parts = path.parts
    if not parts:
        return find_data_file("presets", "default.json")

    found = find_data_file(*parts)
    if found.exists():
        return found

    # A bare name is almost certainly one of the presets rather than a file in
    # the application root, so try that before giving up.
    if len(parts) == 1:
        bundled = find_data_file("presets", parts[0])
        if bundled.exists():
            return bundled

    return found


def is_portable_install() -> bool:
    """True when a ``portable.txt`` marker sits beside the project/executable.

    Dropping this file is how a user asks to keep everything, state included,
    in one folder they can move or delete as a unit -- the alternative to the
    per-user default below, not a detection heuristic.
    """
    return (app_root() / "portable.txt").exists()


def default_state_root() -> Path:
    """Return the per-user root state lives under when not portable.

    ``state`` beside the project or executable (the previous, only, default)
    fails silently in a read-only install location such as Program Files (D3):
    the write raises, is swallowed, and progress is lost with no message.
    ``%LOCALAPPDATA%`` is writable by the user who is running the program
    without needing elevation, regardless of where the program itself lives.
    """
    local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
    base = Path(local_app_data) if local_app_data else Path.home() / "AppData" / "Local"
    return base / "DesktopBugCompanion"


def state_dir() -> Path:
    """Return the directory holding runtime state and session control files.

    ``DESKTOP_BUG_STATE_DIR`` overrides everything else, which the headless
    tests use so a test run cannot rewrite a real player's saved spiders. A
    ``portable.txt`` marker keeps state beside the project or executable, as
    every version before DC-15 always did. Otherwise state lives under
    ``%LOCALAPPDATA%\\DesktopBugCompanion`` (D3), which survives an install to
    a location the user cannot write to. Both the overlay and the settings
    window resolve it here so they cannot disagree.
    """
    override = os.environ.get("DESKTOP_BUG_STATE_DIR", "").strip()
    if override:
        return Path(override)
    if is_portable_install():
        return app_root() / "state"
    return default_state_root() / "state"


def migrate_legacy_state_dir() -> None:
    """Copy an existing beside-the-executable state folder to its new home.

    Runs once, at startup, before anything opens a file under `state_dir()`.
    Copies rather than moves, so a crash partway through leaves the old
    folder intact and this simply runs again next launch. Never touches a
    portable install or a test's overridden directory, since neither one's
    state has moved.
    """
    if os.environ.get("DESKTOP_BUG_STATE_DIR", "").strip():
        return
    if is_portable_install():
        return
    new_dir = state_dir()
    legacy_dir = app_root() / "state"
    if legacy_dir == new_dir or not legacy_dir.is_dir():
        return
    if (new_dir / "creatures.json").exists():
        return
    from ..support.logging_setup import get_logger

    log = get_logger("discovery")
    try:
        new_dir.mkdir(parents=True, exist_ok=True)
        for item in legacy_dir.iterdir():
            if item.is_file():
                shutil.copy2(item, new_dir / item.name)
        log.info("Migrated legacy state from %s to %s", legacy_dir, new_dir)
    except OSError:
        log.warning(
            "Could not migrate legacy state from %s to %s", legacy_dir, new_dir, exc_info=True
        )


def state_dir_is_writable() -> bool:
    """Probe whether `state_dir()` can actually be created and written to.

    `save_runtime_state` already catches a write failure and logs it (D3), but
    a windowed build has no console, so a silent log line is silent in
    practice. This lets a caller with tray access warn instead.
    """
    directory = state_dir()
    try:
        directory.mkdir(parents=True, exist_ok=True)
        probe = directory / ".write_test"
        probe.write_text("", encoding="utf-8")
        probe.unlink()
        return True
    except OSError:
        return False


def data_dirs(folder_name: str, root: Path = None) -> List[Path]:
    """Return existing data directories named folder_name from candidate roots."""
    dirs = []
    for base in candidate_roots(root):
        path = base / folder_name
        if path.exists() and path.is_dir():
            dirs.append(path)
    return _unique_paths(dirs)


def find_data_file(*parts: str, root: Path = None) -> Path:
    """Find an existing bundled/editable data file, or return writable fallback."""
    for base in candidate_roots(root):
        path = base.joinpath(*parts)
        if path.exists():
            return path
    return data_path(*parts)


def _read_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def validate_model(data: dict, path: Path) -> Tuple[bool, str]:
    required = ["id", "display_name", "base_size", "default_personality", "colors", "legs"]
    missing = [key for key in required if key not in data]
    if missing:
        return False, f"{path}: missing required model field(s): {', '.join(missing)}"
    if not isinstance(data.get("legs"), list) or not data["legs"]:
        return False, f"{path}: legs must be a non-empty list"
    for index, leg in enumerate(data["legs"]):
        for key in ["name", "side", "gait_group", "attach_angle", "rest_angle", "reach", "upper_len", "lower_len"]:
            if key not in leg:
                return False, f"{path}: leg {index} missing {key}"
    appearance = data.get("appearance", {})
    if appearance is not None and not isinstance(appearance, dict):
        return False, f"{path}: appearance must be an object"
    chain = appearance.get("leg_chain") if isinstance(appearance, dict) else None
    if chain is not None:
        if not isinstance(chain, dict):
            return False, f"{path}: appearance.leg_chain must be an object"
        if bool(chain.get("enabled", False)):
            if chain.get("segment_count", 4) not in (4, 5):
                return False, f"{path}: appearance.leg_chain.segment_count must be 4 or 5"
            if not isinstance(chain.get("extra_segment_asset", "leg_knuckle"), str) or not chain.get("extra_segment_asset", "leg_knuckle").strip():
                return False, f"{path}: appearance.leg_chain.extra_segment_asset must be a non-empty string"
            segment_count = int(chain.get("segment_count", 4))
            for key, expected_length in [
                ("segment_scales", segment_count),
                ("segment_lengths", segment_count),
                ("joint_scales", segment_count - 1),
                ("bend_profile", segment_count - 1),
                ("joint_phase_offsets", segment_count - 1),
                ("bend_directions", segment_count - 1),
            ]:
                values = chain.get(key)
                if values is not None and (not isinstance(values, list) or len(values) != expected_length):
                    return False, f"{path}: appearance.leg_chain.{key} must contain {expected_length} values"
    return True, ""


def validate_personality(data: dict, path: Path) -> Tuple[bool, str]:
    required = ["id", "display_name", "speed_multiplier", "reaction_radius", "boldness", "wander_frequency"]
    missing = [key for key in required if key not in data]
    if missing:
        return False, f"{path}: missing required personality field(s): {', '.join(missing)}"
    for key in ("skills", "behaviours", "abilities", "ability_bundles"):
        if key in data:
            values = data[key]
            if key == "skills" and values is None:
                continue
            if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
                return False, f"{path}: {key} must be a list of strings"
            if key in ("skills", "behaviours", "abilities"):
                allowed = SKILL_BY_ID
                if key == "behaviours":
                    allowed = {skill_id: SKILL_BY_ID[skill_id] for skill_id in BEHAVIOUR_SKILL_IDS}
                elif key == "abilities":
                    allowed = {skill_id: SKILL_BY_ID[skill_id] for skill_id in ABILITY_SKILL_IDS}
                unknown = [value for value in values if value.strip().lower() not in allowed]
                if unknown:
                    return False, f"{path}: unknown {key} id(s): {', '.join(unknown)}"
            elif key == "ability_bundles":
                unknown = [value for value in values if value.strip().lower() not in ABILITY_BUNDLES]
                if unknown:
                    return False, f"{path}: unknown ability bundle(s): {', '.join(unknown)}"
    if "include_common_abilities" in data and not isinstance(data["include_common_abilities"], bool):
        return False, f"{path}: include_common_abilities must be true or false"
    if "behaviour_modules" in data:
        modules = data["behaviour_modules"]
        if not isinstance(modules, list) or not all(isinstance(value, str) for value in modules):
            return False, f"{path}: behaviour_modules must be a list of strings"
        unknown = [value for value in modules if value.strip().lower() not in BEHAVIOUR_MODULE_IDS]
        if unknown:
            return False, f"{path}: unknown behaviour_modules id(s): {', '.join(unknown)}"
    if "movement_profile" in data:
        movement_profile = str(data["movement_profile"]).strip().lower()
        if movement_profile not in MOVEMENT_PROFILES:
            return False, f"{path}: unknown movement_profile: {data['movement_profile']}"
    if "temperament" in data:
        traits = data["temperament"]
        if not isinstance(traits, dict):
            return False, f"{path}: temperament must be an object of 0..10 values"
        for trait_id in TEMPERAMENT_TRAIT_IDS:
            if trait_id not in traits:
                return False, f"{path}: temperament is missing {trait_id}"
            value = traits[trait_id]
            if isinstance(value, bool):
                return False, f"{path}: temperament values must be numbers from 0 to 10"
            try:
                numeric = float(value)
            except (TypeError, ValueError):
                return False, f"{path}: temperament values must be numbers from 0 to 10"
            if not 0.0 <= numeric <= 10.0:
                return False, f"{path}: temperament value for {trait_id} must be between 0 and 10"
    if "phase_scores" in data:
        scores = data["phase_scores"]
        if not isinstance(scores, dict):
            return False, f"{path}: phase_scores must be an object of phase ids to 0..10 values"
        for phase_id, value in scores.items():
            if str(phase_id).strip().lower() not in BEHAVIOUR_PHASE_IDS:
                return False, f"{path}: unknown behaviour phase: {phase_id}"
            if isinstance(value, bool):
                return False, f"{path}: phase_scores values must be numbers from 0 to 10"
            try:
                numeric = float(value)
            except (TypeError, ValueError):
                return False, f"{path}: phase_scores values must be numbers from 0 to 10"
            if not 0.0 <= numeric <= 10.0:
                return False, f"{path}: phase score for {phase_id} must be between 0 and 10"
    if "phase_duration_multiplier" in data:
        value = data["phase_duration_multiplier"]
        if isinstance(value, bool):
            return False, f"{path}: phase_duration_multiplier must be between 0.25 and 2.5"
        try:
            numeric = float(value)
        except (TypeError, ValueError):
            return False, f"{path}: phase_duration_multiplier must be between 0.25 and 2.5"
        if not 0.25 <= numeric <= 2.5:
            return False, f"{path}: phase_duration_multiplier must be between 0.25 and 2.5"
    return True, ""


def discover_models(root: Path = None) -> Tuple[Dict[str, dict], List[str]]:
    models: Dict[str, dict] = {}
    warnings: List[str] = []
    for models_dir in data_dirs("models", root):
        for path in sorted(models_dir.glob("*/model.json"), key=_newest_first):
            try:
                data = _read_json(path)
                ok, message = validate_model(data, path)
                if not ok:
                    warnings.append(message)
                    continue
                model_id = data["id"]
                # Keep the first copy found. Editable folders are searched first,
                # bundled folders second.
                if model_id in models:
                    continue
                data["_path"] = str(path)
                data["_folder"] = str(path.parent)
                models[model_id] = data
            except Exception as exc:
                warnings.append(f"{path}: {exc}")
    return models, warnings


def discover_personalities(root: Path = None) -> Tuple[Dict[str, dict], List[str]]:
    personalities: Dict[str, dict] = {}
    warnings: List[str] = []
    for personalities_dir in data_dirs("personalities", root):
        for path in sorted(personalities_dir.glob("*.json")):
            try:
                data = _read_json(path)
                ok, message = validate_personality(data, path)
                if not ok:
                    warnings.append(message)
                    continue
                data = annotate_personality(data)
                personality_id = data["id"]
                if personality_id in personalities:
                    continue
                data["_path"] = str(path)
                personalities[personality_id] = data
            except Exception as exc:
                warnings.append(f"{path}: {exc}")
    # The compact temperament catalog is code/data-driven rather than six more
    # duplicate JSON files. Keep legacy files above so old ids remain valid,
    # then add missing canonical choices for the new launch menu.
    for personality_id, personality in canonical_personality_definitions().items():
        if personality_id in personalities:
            continue
        data = annotate_personality(personality)
        data["_path"] = "<built-in temperament catalog>"
        personalities[personality_id] = data
    return personalities, warnings


def discover_presets(root: Path = None) -> List[Path]:
    writable_presets_dir = (Path(root) if root is not None else app_root()) / "presets"
    writable_presets_dir.mkdir(parents=True, exist_ok=True)

    presets: List[Path] = []
    seen_stems = set()
    for presets_dir in data_dirs("presets", root):
        for path in sorted(presets_dir.glob("*.json"), key=_newest_first):
            # Avoid showing duplicate bundled presets when an editable copy
            # exists. Case-folded: a user's "Colony" and the shipped "colony"
            # are one preset on Windows, and listing both is just confusing.
            stem = path.stem.casefold()
            if stem in seen_stems:
                continue
            seen_stems.add(stem)
            presets.append(path)
    return presets
