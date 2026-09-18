"""Two-way live channel between the settings window and the running overlay.

DC-16: the two used to talk one way, by the overlay polling the launched
preset file's mtime every 700 ms. Tray-driven changes -- mood, size, flies,
social play, a spider's name, its team -- were never written back anywhere,
so the settings window's idea of the session silently drifted from what the
overlay was actually doing (C7), and the next Save from the settings window
would overwrite them. This exercises the ``QLocalServer``/``QLocalSocket``
channel that adds the missing direction, alongside the file-based paths it
falls back to.
"""

from __future__ import annotations

import json
import tempfile
import time
from pathlib import Path

import pytest
from PyQt5.QtWidgets import QApplication

from desktop_bug.config_ui import ConfigWindow
from desktop_bug.engine import OverlayWindow
from desktop_bug.live_channel import (
    PROTOCOL_VERSION,
    OverlayChannelServer,
    SettingsChannelClient,
    channel_name,
)


def _fresh_state_dir(monkeypatch) -> Path:
    """A private state directory, without pytest's shared `tmp_path` root.

    That root (`pytest-of-<user>`) has been intermittently PermissionError'd
    by another process holding it (antivirus, or another worktree's pytest
    run) on this machine before -- see tests/test_state_location.py and
    tests/test_creature_render_golden.py for the same workaround.
    """
    folder = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(folder))
    return folder


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """See test_shutdown.py: one QApplication for the whole suite."""


def _pump(condition, timeout: float = 3.0) -> bool:
    """Run the Qt event loop until `condition()` is true or time runs out.

    Local-socket I/O is delivered through the event loop, and nothing here
    calls ``app.exec_()``, so the test has to pump it by hand.
    """
    app = QApplication.instance()
    deadline = time.monotonic() + timeout
    while not condition():
        if time.monotonic() > deadline:
            return False
        app.processEvents()
        time.sleep(0.005)
    return True


def _write_preset(directory: Path, name: str = "Channel") -> Path:
    path = directory / f"{name.lower()}.json"
    path.write_text(
        json.dumps(
            {
                "name": name,
                "slots": [
                    {"model": "tarantula", "personality": "mellow", "count": 1, "slot_id": "slot-0"}
                ],
                "settings": {},
            }
        ),
        encoding="utf-8",
    )
    return path


def test_protocol_round_trip(monkeypatch) -> None:
    """The building blocks: connect, send each direction, see the message."""
    state_dir = _fresh_state_dir(monkeypatch)
    name = channel_name(state_dir)
    server = OverlayChannelServer(name)
    client = SettingsChannelClient(name)
    try:
        assert server.listen(), "server could not bind the channel"
        received_by_server = []
        received_by_client = []
        server.message_received.connect(received_by_server.append)
        client.message_received.connect(received_by_client.append)

        assert client.try_connect(500), "client could not reach the server"
        assert _pump(lambda: server.client_count == 1)
        # The client's own "hello" on connect is the first thing the server sees.
        assert _pump(lambda: len(received_by_server) >= 1)
        assert received_by_server[0]["type"] == "hello"
        assert received_by_server[0]["v"] == PROTOCOL_VERSION

        assert client.send({"type": "preset_update", "preset_path": "x", "data": {"a": 1}})
        assert _pump(lambda: len(received_by_server) >= 2)
        assert received_by_server[1] == {
            "v": PROTOCOL_VERSION,
            "type": "preset_update",
            "preset_path": "x",
            "data": {"a": 1},
        }

        assert server.broadcast({"type": "session_state", "state": {"mood_mode": "playful"}}) == 1
        assert _pump(lambda: len(received_by_client) >= 1)
        assert received_by_client[0]["state"]["mood_mode"] == "playful"
    finally:
        client.close()
        server.close()


def test_protocol_version_mismatch_disconnects(monkeypatch) -> None:
    """An incompatible message closes the connection rather than being guessed at."""
    state_dir = _fresh_state_dir(monkeypatch)
    name = channel_name(state_dir)
    server = OverlayChannelServer(name)
    client = SettingsChannelClient(name)
    try:
        assert server.listen()
        assert client.try_connect(500)
        assert _pump(lambda: server.client_count == 1)
        disconnected = []
        client.disconnected.connect(lambda: disconnected.append(True))
        # Bypass SettingsChannelClient.send (which always stamps the current
        # version) to simulate a message from an incompatible build.
        client._socket.write(b'{"v": 999, "type": "preset_update"}\n')
        client._socket.flush()
        assert _pump(lambda: bool(disconnected)), "an unversioned message should have been rejected"
    finally:
        client.close()
        server.close()


class _FakeRunningProcess:
    """Stands in for the `Popen` handle `_overlay_running()` checks.

    The overlay under test here is a real in-process `OverlayWindow`, not a
    subprocess the settings window actually launched, so there is no real
    `Popen` for `_apply_live_if_running()` to see. This is the same
    substitution `test_shutdown.py`'s `FakeProcess` makes for the same reason.
    """

    def poll(self):
        return None


def test_dc16_acceptance(monkeypatch) -> None:
    """The plan's own acceptance line, driven end to end.

    Two in-process endpoints: `OverlayWindow` plays the overlay, `ConfigWindow`
    plays the settings window. A mood change from the tray side must reach
    the settings window's model, and a slot change from the settings side
    must reach the running overlay.
    """
    _fresh_state_dir(monkeypatch)
    preset_dir = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    preset_path = _write_preset(preset_dir)
    overlay = OverlayWindow(preset_path)
    settings = ConfigWindow()
    try:
        assert overlay._channel_server.is_listening(), "overlay could not open its live channel"
        settings.load_preset_path(preset_path)
        settings.launched_preset_path = str(preset_path)
        settings.overlay_process = _FakeRunningProcess()

        assert settings._ensure_channel_connected(500), "settings window could not reach the overlay"
        assert _pump(lambda: overlay._channel_server.client_count >= 1)

        # --- tray side changes mood; the settings-window-side model updates ---
        assert settings.mood_combo.currentData() != "playful"
        overlay._announce(overlay.manager.set_mood_mode("playful"))
        assert _pump(lambda: settings.mood_combo.currentData() == "playful"), (
            "settings window mood did not follow the tray-driven change"
        )
        assert settings._live_creature_state, "no per-creature state arrived with the snapshot"

        # --- settings side changes a slot; the overlay reloads ---
        assert len(overlay.manager.creatures) == 1
        settings.add_slot(model_id="tarantula", personality_id="mellow", count=1)
        assert settings._apply_live_if_running(), "settings window did not think the overlay was running"
        # A short timeout, well under PRESET_WATCH_MS (700 ms): the overlay
        # also polls the preset file it was launched from, so a generous
        # timeout here would let that fallback quietly satisfy this assertion
        # even if the channel push were broken, proving nothing about the
        # channel itself.
        assert _pump(lambda: len(overlay.manager.creatures) == 2, timeout=0.3), (
            "overlay did not reload the preset pushed over the live channel"
        )
    finally:
        # Clear the fake process before closing: closeEvent would otherwise
        # pop a modal "stop the overlay too?" prompt, which has nothing to
        # answer it under the offscreen platform.
        settings.overlay_process = None
        overlay.timer.stop()
        overlay.style_timer.stop()
        overlay.close()
        overlay.deleteLater()
        settings.close()
        settings.deleteLater()


def test_dc16_stop_request_over_channel_saves_state(monkeypatch) -> None:
    """DC-04's guarantee must survive DC-16: a channel stop still saves first.

    Starts the engine in-process, dirties its runtime state, triggers the
    graceful path through the new channel instead of the stop-request file,
    and asserts the state file was written -- the same shape of proof
    `test_shutdown.py::test_overlay_saves_on_stop` uses for the file-based
    path, so neither path can regress without a test noticing.
    """
    _fresh_state_dir(monkeypatch)
    preset_dir = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    preset_path = _write_preset(preset_dir, "StopChannel")
    overlay = OverlayWindow(preset_path)
    settings = ConfigWindow()
    try:
        overlay.manager._progression_state_path = preset_dir / "creatures.json"
        state_path = overlay.manager._progression_state_path
        assert not state_path.exists()

        spider = overlay.manager.creatures[0]
        spider.set_name("Shelob")
        spider.gain_experience(250, "test")
        overlay.manager.mark_runtime_state_dirty()

        assert settings._ensure_channel_connected(500), "settings window could not reach the overlay"
        assert _pump(lambda: overlay._channel_server.client_count >= 1)

        assert overlay._stop_requested is False
        assert settings._channel_client.send({"type": "stop_request"})
        assert _pump(lambda: overlay._stop_requested is True), "stop_request over the channel was not handled"
        assert state_path.is_file(), "the channel-based stop did not save runtime state"

        saved = json.loads(state_path.read_text(encoding="utf-8"))
        entry = saved["creatures"]["stopchannel|slot-0:0"]
        assert entry["name"] == "Shelob", entry
        assert entry["progression"]["total_xp"] >= 250, entry
    finally:
        overlay.timer.stop()
        overlay.style_timer.stop()
        overlay.close()
        overlay.deleteLater()
        settings.close()
        settings.deleteLater()
