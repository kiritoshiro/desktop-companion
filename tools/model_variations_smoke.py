import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.discovery import discover_models  # noqa: E402


EXPECTED = {
    "mini_marble": {"size": "small", "fluffy": False},
    "velvet_cloud": {"size": "medium", "fluffy": True},
    "giant_copper": {"size": "large", "fluffy": True},
    "sunset_fuzzball": {"size": "large", "fluffy": True},
    "blue_jewel": {"size": "large", "fluffy": False},
}


def main() -> int:
    models, warnings = discover_models(ROOT)
    if warnings:
        raise AssertionError("model discovery warnings: " + " | ".join(warnings))
    for model_id, traits in EXPECTED.items():
        model = models.get(model_id)
        if model is None:
            raise AssertionError(f"missing new model: {model_id}")
        if model.get("render_mode") != "procedural":
            raise AssertionError(f"{model_id} must use the recolorable procedural renderer")
        if len(model.get("legs", [])) != 8:
            raise AssertionError(f"{model_id} must have eight articulated legs")
        appearance = model.get("appearance", {})
        if not appearance.get("segmented_legs") or not appearance.get("leg_connections", {}).get("enabled"):
            raise AssertionError(f"{model_id} is missing visible segmented leg roots")
        fluffiness = float(appearance.get("fluffiness", 0.0))
        if traits["fluffy"] and fluffiness < 0.75:
            raise AssertionError(f"{model_id} should be a fluffy variation")
        if not traits["fluffy"] and fluffiness > 0.40:
            raise AssertionError(f"{model_id} should be a sleeker variation")
    sizes = {model_id: models[model_id]["base_size"] for model_id in EXPECTED}
    if not sizes["mini_marble"] < sizes["blue_jewel"] < sizes["giant_copper"]:
        raise AssertionError(f"size range is not represented: {sizes}")
    print(f"OK: validated {len(EXPECTED)} new procedural model variations ({sizes})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
