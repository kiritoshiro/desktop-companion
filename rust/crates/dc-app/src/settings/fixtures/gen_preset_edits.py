"""Dump what Python's settings window writes for a few slot edits.

The oracle for dc-app's `settings::tests::preset_edits_match_python`. It opens
`ConfigWindow` on Qt's offscreen platform (no window reaches the desktop),
with a throw-away state directory, so the shipped `presets/default.json` is
loaded exactly as the real window loads it. Then:

* `unchanged`: Save straight away (the retired settings are dropped).
* `edited`: slot 0 gets the `plush_snow_hybrid_2` skin, the `curious`
  temperament and a count of 4; slot 2 the `cautious` temperament; slot 3 the
  Jumper category (its skin falls to the category's first entry). Save.
* `colony`: load the shipped `presets/colony.json` and Save it.
* `reset`: `reset_saved_progress` on a grown `creatures.json` (what the
  window does when no overlay is listening): `reset_input` before, `reset`
  after.

Run from the worktree root:

    set QT_QPA_PLATFORM=offscreen
    <venv>\\Scripts\\python.exe -I rust/crates/dc-app/src/settings/fixtures/gen_preset_edits.py

It writes `preset_edits.json` beside itself.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
WORKTREE = HERE.parents[5]
sys.path.insert(0, str(WORKTREE / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ["DESKTOP_BUG_STATE_DIR"] = str(Path(tempfile.mkdtemp(prefix="dc-preset-edits-")) / "state")

from PyQt5.QtWidgets import QApplication  # noqa: E402

from desktop_bug.state.runtime_state import reset_saved_progress  # noqa: E402
from desktop_bug.app.config_ui import (  # noqa: E402
    COL_CATEGORY,
    COL_COUNT,
    COL_MODEL,
    COL_TEMPERAMENT,
    ConfigWindow,
)


def pick(combo, value) -> None:
    index = combo.findData(value)
    assert index >= 0, f"{value!r} is not offered"
    combo.setCurrentIndex(index)


def main() -> None:
    app = QApplication.instance() or QApplication(sys.argv[:1])
    window = ConfigWindow()
    table = window.table

    window.save_current_preset()
    saved = Path(os.environ["DESKTOP_BUG_STATE_DIR"]) / "presets" / "Default.json"
    unchanged = saved.read_text(encoding="utf-8")

    pick(table.cellWidget(0, COL_MODEL), "plush_snow_hybrid_2")
    pick(table.cellWidget(0, COL_TEMPERAMENT), "curious")
    table.cellWidget(0, COL_COUNT).setValue(4)
    pick(table.cellWidget(2, COL_TEMPERAMENT), "cautious")
    pick(table.cellWidget(3, COL_CATEGORY), "jumper")
    jumper_skin = table.cellWidget(3, COL_MODEL).currentData()
    window.save_current_preset()
    edited = saved.read_text(encoding="utf-8")

    window.load_preset_path(WORKTREE / "presets" / "colony.json")
    window.save_current_preset()
    colony = (saved.parent / "Colony.json").read_text(encoding="utf-8")

    reset_input = {
        "schema_version": 3,
        "launch": 7,
        "creatures": {"spider-abc": {"name": "Shelob", "progression": {"level": 7, "xp": 900.0}}},
        "bases": [{"site": "porch", "build_progress": 80.0}],
        "scene": {"cages": [{"x": 10, "y": 20, "w": 200, "h": 150}], "webs": [], "extra": [1]},
        "unknown_top_level": True,
    }
    state_file = Path(os.environ["DESKTOP_BUG_STATE_DIR"]) / "creatures.json"
    state_file.write_text(json.dumps(reset_input, indent=2) + "\n", encoding="utf-8")
    assert reset_saved_progress(state_file)
    reset = state_file.read_text(encoding="utf-8")

    out = {
        "source": "src/desktop_bug/app/config_ui.py ConfigWindow.save_current_preset (offscreen)",
        "unchanged": unchanged,
        "edited": edited,
        "colony": colony,
        "reset_input": reset_input,
        "reset": reset,
        "jumper_skin": jumper_skin,
    }
    (HERE / "preset_edits.json").write_text(json.dumps(out, indent=2) + "\n", encoding="utf-8")
    window.close()
    app.quit()


if __name__ == "__main__":
    main()
