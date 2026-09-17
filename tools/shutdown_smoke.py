"""Headless checks that stopping the overlay saves instead of losing progress.

The settings window used to call ``Popen.terminate()``, which on Windows kills
the overlay outright, so Qt's ``aboutToQuit`` save hook never ran.
"""

import json
import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtWidgets import QApplication  # noqa: E402

from desktop_bug.discovery import state_dir  # noqa: E402
from desktop_bug.engine import OverlayWindow  # noqa: E402
from desktop_bug.session_control import (  # noqa: E402
    clear_stop_request,
    consume_stop_request,
    request_stop,
    stop_process,
    stop_request_path,
)


class FakeProcess:
    """Stands in for Popen: exits after ``exits_after`` polls, or never."""

    def __init__(self, exits_after=None):
        self.polls = 0
        self.exits_after = exits_after
        self.terminated = False

    def poll(self):
        self.polls += 1
        if self.exits_after is None:
            return None
        return 0 if self.polls > self.exits_after else None

    def terminate(self):
        self.terminated = True


def check_request_file() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        assert consume_stop_request(tmp) is False, "no request should read as none"
        assert request_stop(tmp) is True
        assert stop_request_path(tmp).is_file()
        # Exactly once: a second overlay must not also quit.
        assert consume_stop_request(tmp) is True
        assert consume_stop_request(tmp) is False
        assert not stop_request_path(tmp).exists()

        request_stop(tmp)
        clear_stop_request(tmp)
        assert consume_stop_request(tmp) is False, "clear left a stale request"
        # Clearing nothing is not an error.
        clear_stop_request(tmp)


def check_stop_process() -> None:
    slept = []

    def fake_sleep(seconds):
        slept.append(seconds)

    with tempfile.TemporaryDirectory() as tmp:
        # The overlay notices the request and exits on its own.
        proc = FakeProcess(exits_after=2)
        assert stop_process(proc, tmp, sleep=fake_sleep) == "graceful"
        assert proc.terminated is False, "a responsive overlay was killed anyway"
        assert not stop_request_path(tmp).exists(), "request outlived the stop"

        # A responsive overlay is not waited on any longer than it needs.
        assert len(slept) <= 2, slept

        # A wedged overlay is still killed rather than left running.
        slept.clear()
        proc = FakeProcess(exits_after=None)
        assert stop_process(proc, tmp, grace=0.5, poll=0.1, sleep=fake_sleep) == "terminated"
        assert proc.terminated is True
        assert not stop_request_path(tmp).exists()

        # Waiting is bounded by the grace period rather than unbounded.
        assert len(slept) == 5, slept
        assert all(step == 0.1 for step in slept), slept

        # Nothing to stop.
        assert stop_process(None, tmp, sleep=fake_sleep) == "not-running"
        already = FakeProcess(exits_after=0)
        assert stop_process(already, tmp, sleep=fake_sleep) == "not-running"
        assert already.terminated is False


def check_overlay_saves_on_stop() -> None:
    app = QApplication.instance() or QApplication(sys.argv[:1])
    assert app is not None

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        preset_path = tmp_path / "shutdown.json"
        preset_path.write_text(
            json.dumps(
                {
                    "name": "Shutdown",
                    "slots": [
                        {"model": "tarantula", "personality": "mellow", "count": 1, "slot_id": "slot-0"}
                    ],
                    "settings": {},
                }
            ),
            encoding="utf-8",
        )

        window = OverlayWindow(preset_path)
        try:
            window.manager._progression_state_path = tmp_path / "creatures.json"
            window._state_dir = tmp_path
            state_path = window.manager._progression_state_path
            assert not state_path.exists()

            # Earn something that only the debounced flush would have written.
            spider = window.manager.creatures[0]
            spider.set_name("Shelob")
            spider.gain_experience(250, "test")
            window.manager.mark_runtime_state_dirty()

            # No request yet, so the overlay keeps running.
            assert window._check_stop_request() is False
            assert not state_path.exists()

            request_stop(tmp_path)
            assert window._check_stop_request() is True
            assert window._stop_requested is True
            assert state_path.is_file(), "stopping did not save runtime state"

            saved = json.loads(state_path.read_text(encoding="utf-8"))
            entry = saved["creatures"]["shutdown|slot-0:0"]
            assert entry["name"] == "Shelob", entry
            assert entry["progression"]["total_xp"] >= 250, entry

            # Once handled, it stays handled and does not re-enter.
            assert window._check_stop_request() is True
        finally:
            window.timer.stop()
            window.style_timer.stop()
            window.close()
            window.deleteLater()


def check_startup_clears_stale_request() -> None:
    app = QApplication.instance() or QApplication(sys.argv[:1])
    assert app is not None

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        preset_path = tmp_path / "stale.json"
        preset_path.write_text(
            json.dumps(
                {
                    "name": "Stale",
                    "slots": [
                        {"model": "tarantula", "personality": "mellow", "count": 1, "slot_id": "slot-0"}
                    ],
                    "settings": {},
                }
            ),
            encoding="utf-8",
        )

        # A crashed session leaves a request behind; the next overlay must not
        # quit the moment it finishes loading.
        request_stop(state_dir())
        window = OverlayWindow(preset_path)
        try:
            assert not stop_request_path(state_dir()).exists(), "stale request survived startup"
            assert window._check_stop_request() is False
            assert window._stop_requested is False
        finally:
            window.timer.stop()
            window.style_timer.stop()
            window.close()
            window.deleteLater()


def main() -> int:
    check_request_file()
    check_stop_process()
    check_overlay_saves_on_stop()
    check_startup_clears_stale_request()
    print("shutdown smoke: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
