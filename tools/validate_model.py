import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.content.discovery import validate_model  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a Desktop Bug Companion model JSON file")
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    with args.path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    ok, message = validate_model(data, args.path)
    if ok:
        print(f"OK: {args.path}")
        return 0
    print(message)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
