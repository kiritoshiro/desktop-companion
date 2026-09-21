import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.content.preset_io import validate_preset  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a Desktop Bug Companion preset JSON file")
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    with args.path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    validate_preset(data)
    print(f"OK: {args.path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(exc)
        raise SystemExit(1)
