"""Paths and data loaders shared by the tests.

Not a test module: pytest collects `test_*.py`, so nothing here runs on its
own. It exists because `ROOT` was recomputed in twenty-eight files, and a
handful of them loaded the same shipped model and personality with the same
four lines.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
MODELS = ROOT / "models"
PERSONALITIES = ROOT / "personalities"
PRESETS = ROOT / "presets"


def load_model(name: str = "tarantula") -> dict:
    with (MODELS / name / "model.json").open(encoding="utf-8") as handle:
        return json.load(handle)


def load_personality(name: str = "mellow") -> dict:
    with (PERSONALITIES / f"{name}.json").open(encoding="utf-8") as handle:
        return json.load(handle)


def load_pair(model: str = "tarantula", personality: str = "mellow") -> tuple[dict, dict]:
    return load_model(model), load_personality(personality)
