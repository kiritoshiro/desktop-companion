"""The settings window can forget every saved stat and base (DC-85).

The owner: *"make it possible to remove all saved stats in the main menu. all
the bases."* A running overlay holds its state in memory and writes it back,
so it resets itself when asked over the live channel; with no overlay
listening, the file is cleared directly. Either way the scene the player
arranged -- cages, webs, nests -- is kept.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from desktop_bug.manager import CreatureManager
from desktop_bug.state.runtime_state import reset_saved_progress
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _grown_colony(monkeypatch):
    state = Path(tempfile.mkdtemp(prefix="dc85-reset-")) / "state"
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(state))
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1200, 800, seed=3)
    for creature in manager.creatures:
        creature.progression.level = 7
        creature.progression.xp = 900.0
    site = manager.base_world.ensure_site(manager.creatures[0])
    site.build_progress = 80.0
    manager.save_runtime_state()
    return manager, state / "creatures.json"


def test_the_running_overlay_forgets_stats_and_bases(monkeypatch):
    manager, path = _grown_colony(monkeypatch)
    count = len(manager.creatures)
    manager.reset_saved_progress()
    assert len(manager.creatures) == count, "the same colony comes back"
    assert all(c.progression.level < 7 for c in manager.creatures)
    assert not manager.base_world.bases
    saved = json.loads(path.read_text(encoding="utf-8"))
    assert saved["bases"] == []
    assert all(entry["progression"]["level"] < 7 for entry in saved["creatures"].values())


def test_clearing_the_file_keeps_the_scene(monkeypatch):
    manager, path = _grown_colony(monkeypatch)
    data = json.loads(path.read_text(encoding="utf-8"))
    data["scene"]["cages"] = [{"x": 10, "y": 20, "w": 200, "h": 150}]
    path.write_text(json.dumps(data), encoding="utf-8")
    assert reset_saved_progress(path)
    after = json.loads(path.read_text(encoding="utf-8"))
    assert after["creatures"] == {} and after["bases"] == []
    assert after["scene"]["cages"] == [{"x": 10, "y": 20, "w": 200, "h": 150}]
    assert after["launch"] == data["launch"]


def test_no_file_is_nothing_to_reset():
    # tempfile rather than pytest's tmp_path, which cannot create its
    # directory on this machine (see test_creature_render_golden).
    assert reset_saved_progress(Path(tempfile.mkdtemp(prefix="dc85-none-")) / "creatures.json")


def test_the_settings_window_asks_the_overlay_when_it_is_listening(monkeypatch):
    from desktop_bug.app.config_ui import ConfigWindow

    sent = []

    class Fake:
        status = type("S", (), {"setText": lambda self, text: None})()
        overlay_process = None
        _channel_client = type("C", (), {"send": lambda self, msg: sent.append(msg)})()

        def _ensure_channel_connected(self):
            return self.listening

    fake = Fake()
    fake.listening = True
    assert ConfigWindow.reset_saved_progress(fake, confirm=False) == "overlay"
    assert sent == [{"type": "reset_progress"}]

    fake.listening = False
    written = []
    monkeypatch.setattr("desktop_bug.app.config_ui.reset_saved_progress",
                        lambda path: written.append(path) or True)
    assert ConfigWindow.reset_saved_progress(fake, confirm=False) == "file"
    assert written and written[0].name == "creatures.json"
