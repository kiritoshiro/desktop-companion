"""Paths and data loaders shared by the tests.

Not a test module: pytest collects `test_*.py`, so nothing here runs on its
own. It exists because `ROOT` was recomputed in twenty-eight files, and a
handful of them loaded the same shipped model and personality with the same
four lines.
"""

from __future__ import annotations

import json
from pathlib import Path

from desktop_bug.content.body_plans import resolve_body_plan

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
MODELS = ROOT / "models"
PERSONALITIES = ROOT / "personalities"
PRESETS = ROOT / "presets"


def load_model(name: str = "tarantula") -> dict:
    """A shipped model, with its body plan resolved.

    DC-49 let a model name a body plan instead of restating a skeleton, and
    `discover_models` fills the rig in on load. A test that reads the JSON
    itself skips that step and gets a spider with no legs -- which is how
    twenty-four tests failed at once, all of them reading the file directly.
    Going through here is the single correct way to load one.
    """
    with (MODELS / name / "model.json").open(encoding="utf-8") as handle:
        return resolve_body_plan(json.load(handle))


def load_personality(name: str = "mellow") -> dict:
    with (PERSONALITIES / f"{name}.json").open(encoding="utf-8") as handle:
        return json.load(handle)


def load_pair(model: str = "tarantula", personality: str = "mellow") -> tuple[dict, dict]:
    return load_model(model), load_personality(personality)
