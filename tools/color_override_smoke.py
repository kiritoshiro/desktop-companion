import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.creature import Creature  # noqa: E402
from desktop_bug.preset_io import validate_preset  # noqa: E402


def preset(colors=None):
    slot = {"model": "tarantula", "personality": "curious", "count": 1}
    if colors is not None:
        slot["colors"] = colors
    return {"name": "Color smoke", "slots": [slot]}


def assert_rejected(data):
    try:
        validate_preset(data)
    except ValueError:
        return
    raise AssertionError("malformed color override was accepted")


def main() -> int:
    validate_preset(preset())
    palette = {"body": [220, 40, 80], "leg_band": [40, 200, 160]}
    validate_preset(preset(palette))
    assert_rejected(preset({"body": [1, 2]}))
    assert_rejected(preset({"body": [1, True, 3]}))
    assert_rejected(preset({"body": [1, 2, 300]}))

    model = json.loads((ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    original = copy.deepcopy(model["colors"])
    personality = {"id": "curious", "speed_multiplier": 1.0, "reaction_radius": 120, "boldness": 0.5, "wander_frequency": 0.0}
    first = Creature(model, personality, 800, 600, color_overrides=palette)
    second = Creature(model, personality, 800, 600, color_overrides=palette)
    if first.colors["body"] != palette["body"] or first.colors["leg_band"] != palette["leg_band"]:
        raise AssertionError("override did not reach Creature.colors")
    if model["colors"] != original:
        raise AssertionError("color override mutated shared model metadata")
    first.colors["body"][0] = 0
    if second.colors["body"][0] != palette["body"][0]:
        raise AssertionError("creatures share a mutable palette")
    print("OK: color override validation, precedence, and copy isolation")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
