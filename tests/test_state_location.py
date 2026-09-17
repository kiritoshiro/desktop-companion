"""Where runtime state lives, and how it gets there (DC-15).

State used to live only beside the project or executable. Under a read-only
install location -- Program Files, say -- that write fails, is swallowed, and
a player's progress is gone with no message (D3). State now defaults to
`%LOCALAPPDATA%\\DesktopBugCompanion`, a location the user who is running the
program can always write to regardless of where the program itself lives. A
`portable.txt` marker opts back into the old beside-the-executable behaviour,
and an existing beside-the-executable folder is copied into the new location
on first run rather than abandoned.
"""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import pytest
from desktop_bug import discovery


@pytest.fixture
def fake_install(monkeypatch):
    """An app_root and a LOCALAPPDATA, both empty, neither the real project.

    Plain tempfile rather than pytest's tmp_path fixture, which numbers its
    directories by scanning a shared parent that can be locked by another
    process (antivirus, a leftover handle) -- a state-location test should
    not fail over that.
    """
    base = Path(tempfile.mkdtemp(prefix="dc15-install-"))
    root = base / "install"
    root.mkdir()
    local_app_data = base / "AppData" / "Local"
    monkeypatch.setattr(discovery, "app_root", lambda: root)
    monkeypatch.setenv("LOCALAPPDATA", str(local_app_data))
    monkeypatch.delenv("DESKTOP_BUG_STATE_DIR", raising=False)
    try:
        yield root
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_the_default_location_is_per_user(fake_install):
    resolved = discovery.state_dir()
    assert resolved == discovery.default_state_root() / "state"
    assert "AppData" in str(resolved)


def test_a_portable_marker_keeps_state_beside_the_install(fake_install):
    (fake_install / "portable.txt").write_text("", encoding="utf-8")
    assert discovery.is_portable_install()
    assert discovery.state_dir() == fake_install / "state"


def test_the_env_override_still_wins_over_everything(fake_install, monkeypatch):
    (fake_install / "portable.txt").write_text("", encoding="utf-8")
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(fake_install / "elsewhere"))
    assert discovery.state_dir() == fake_install / "elsewhere"


def test_a_legacy_state_folder_migrates_to_the_new_location(fake_install):
    legacy = fake_install / "state"
    legacy.mkdir()
    payload = {"schema_version": 2, "creatures": {"default|slot-0:0": {"name": "Legacy"}}}
    (legacy / "creatures.json").write_text(json.dumps(payload), encoding="utf-8")

    discovery.migrate_legacy_state_dir()

    new_dir = discovery.state_dir()
    assert new_dir != legacy
    migrated = json.loads((new_dir / "creatures.json").read_text(encoding="utf-8"))
    assert migrated == payload
    # Copied, not moved: the old copy is still there for anything that has not
    # been updated yet, and so a crash mid-copy cannot lose the only copy.
    assert (legacy / "creatures.json").exists()


def test_migration_does_not_overwrite_an_already_migrated_file(fake_install):
    legacy = fake_install / "state"
    legacy.mkdir()
    (legacy / "creatures.json").write_text(json.dumps({"v": 1}), encoding="utf-8")
    discovery.migrate_legacy_state_dir()

    new_dir = discovery.state_dir()
    (new_dir / "creatures.json").write_text(json.dumps({"v": "already migrated, kept"}), encoding="utf-8")
    (legacy / "creatures.json").write_text(json.dumps({"v": "changed after migration"}), encoding="utf-8")

    discovery.migrate_legacy_state_dir()

    assert json.loads((new_dir / "creatures.json").read_text(encoding="utf-8")) == {
        "v": "already migrated, kept"
    }


def test_migration_is_a_no_op_with_nothing_to_migrate(fake_install):
    discovery.migrate_legacy_state_dir()
    assert not discovery.state_dir().exists()


def test_migration_leaves_a_portable_install_alone(fake_install):
    (fake_install / "portable.txt").write_text("", encoding="utf-8")
    legacy = fake_install / "state"
    legacy.mkdir()
    (legacy / "creatures.json").write_text("{}", encoding="utf-8")

    discovery.migrate_legacy_state_dir()

    # Nothing to migrate INTO: state_dir() already points at legacy for a
    # portable install, so this must not, say, copy the folder into itself.
    assert discovery.state_dir() == legacy
    assert list(legacy.iterdir()) == [legacy / "creatures.json"]


def test_a_writable_directory_reports_writable(fake_install):
    assert discovery.state_dir_is_writable()
    assert discovery.state_dir().is_dir()


def test_an_unwritable_directory_reports_unwritable(fake_install, monkeypatch):
    # A file sitting where state_dir() needs a directory makes mkdir fail --
    # cheap and portable stand-in for a real permissions failure.
    blocker = discovery.default_state_root()
    blocker.parent.mkdir(parents=True)
    blocker.write_text("", encoding="utf-8")
    assert not discovery.state_dir_is_writable()
