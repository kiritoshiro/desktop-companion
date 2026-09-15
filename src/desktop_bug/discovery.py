import json
import sys
from pathlib import Path
from typing import Dict, List, Tuple


def app_root() -> Path:
    """Return the writable/editable data root.

    In development this is the project folder. In a PyInstaller build this is
    the folder containing the executable, so models/personalities/presets can
    remain editable beside the .exe.
    """
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[2]


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
                personality_id = data["id"]
                if personality_id in personalities:
                    continue
                data["_path"] = str(path)
                personalities[personality_id] = data
            except Exception as exc:
                warnings.append(f"{path}: {exc}")
    return personalities, warnings


def discover_presets(root: Path = None) -> List[Path]:
    writable_presets_dir = (Path(root) if root is not None else app_root()) / "presets"
    writable_presets_dir.mkdir(parents=True, exist_ok=True)

    presets: List[Path] = []
    seen_stems = set()
    for presets_dir in data_dirs("presets", root):
        for path in sorted(presets_dir.glob("*.json"), key=_newest_first):
            # Avoid showing duplicate bundled presets when an editable copy exists.
            if path.stem in seen_stems:
                continue
            seen_stems.add(path.stem)
            presets.append(path)
    return presets
