"""Shared setup for the whole suite.

Every test file used to open with the same six lines: put `src/` on the path,
force Qt offscreen, and point the runtime state somewhere harmless. That is
why `ruff` reported forty-seven E402 findings -- the package import had to come
after the bootstrapping -- and it is why adding a test meant remembering a
ritual. It happens once, here, before pytest imports any test module.

The two environment variables matter for more than tidiness. Without
`QT_QPA_PLATFORM` a headless runner has no display and Qt aborts; without
`DESKTOP_BUG_STATE_DIR` a test run rewrites the developer's own saved spiders,
which it did until DC-03 noticed.
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault(
    "DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-tests-")
)
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

# DC-14: Qt reads Qt.AA_EnableHighDpiScaling while it builds the platform
# integration for the *first* QApplication/QGuiApplication in the process and
# ignores a later change, so this has to run here -- before any test module
# gets a chance to construct one some other way -- rather than inside the
# `qapp` fixture below, which only runs on the first test that requests it.
from desktop_bug.dpi import enable_high_dpi_scaling  # noqa: E402

enable_high_dpi_scaling()

# Qt allows exactly one application object per process, and it must outlive
# every widget and every QFontMetrics built from it. Binding it to a fixture
# alone is not enough: a session fixture's value is released at the end of the
# session while Qt objects are still being torn down, and a local that goes out
# of scope takes the application with it -- which is an access violation, not
# an exception. This module-level reference is what keeps it alive.
_APP = None


@pytest.fixture(scope="session")
def qapp():
    """The one QApplication for the process.

    A QApplication rather than a QGuiApplication, because some checks build
    real widgets. The narrower class cannot be upgraded afterwards: Qt refuses
    a second application object, so a test that created a QGuiApplication first
    made every widget test in the same process abort.
    """
    global _APP
    if _APP is None:
        from PyQt5.QtWidgets import QApplication

        _APP = QApplication.instance() or QApplication(sys.argv[:1])
    return _APP


@pytest.fixture(scope="session")
def root() -> Path:
    """The repository root, for tests that read shipped data or source files."""
    return ROOT


@pytest.fixture
def state_dir(tmp_path, monkeypatch) -> Path:
    """A private runtime-state directory for one test.

    The session-wide default is shared, which is fine for tests that only read.
    A test that writes state, migrates it or counts what is in it needs its own
    directory or it inherits whatever ran before it.
    """
    folder = tmp_path / "state"
    folder.mkdir()
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(folder))
    return folder
