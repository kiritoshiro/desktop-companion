"""Desktop icon awareness is opt-in, off by default (D1, DC-13).

Reading real desktop icon positions means OpenProcess/ReadProcessMemory
against Explorer -- the specific pattern antivirus heuristics flag, and
there used to be no way to turn it off. Window occlusion (EnumWindows, no
process memory) is a different, cheaper thing and is unaffected.
"""

from __future__ import annotations

import ctypes
from unittest.mock import patch

import pytest
from desktop_bug.desktop_environment import snapshot_desktop_surfaces
from desktop_bug.manager import CreatureManager
from support import ROOT


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a manager constructs Creatures, which need a QApplication."""


def test_a_new_manager_starts_with_icon_probing_off():
    manager = CreatureManager(ROOT / "presets" / "default.json", 1200, 800)
    assert manager.desktop_icons_enabled is False


def test_turning_it_on_and_off_reports_which():
    manager = CreatureManager(ROOT / "presets" / "default.json", 1200, 800)
    assert "on" in manager.set_desktop_icons_enabled(True)
    assert manager.desktop_icons_enabled is True
    assert "off" in manager.set_desktop_icons_enabled(False)
    assert manager.desktop_icons_enabled is False


def test_icons_disabled_never_opens_a_process_handle():
    """The acceptance test DC-13 asks for: patch the ctypes entry point."""
    with patch.object(ctypes.windll.kernel32, "OpenProcess") as open_process:
        snapshot_desktop_surfaces(screen_w=1200, screen_h=800, include_desktop_icons=False)
    open_process.assert_not_called()
