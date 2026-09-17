"""A preset path must resolve the same way in a source and a frozen build.

The packaged executable crashed on `--preset presets/colony.json` because a
relative path was resolved against the folder holding the .exe, while a
one-file build keeps its bundled presets in the directory it extracts itself
into. Reads must search both; writes must not.
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

from desktop_bug.discovery import resolve_preset_path  # noqa: E402


class FakeFrozen:
    """Pretend to be a PyInstaller one-file build during the block."""

    def __init__(self, exe_dir: Path, meipass: Path):
        self.exe_dir = exe_dir
        self.meipass = meipass
        self._saved = {}

    def __enter__(self):
        self._saved = {
            "frozen": getattr(sys, "frozen", None),
            "executable": sys.executable,
            "_MEIPASS": getattr(sys, "_MEIPASS", None),
        }
        sys.frozen = True
        sys.executable = str(self.exe_dir / "DesktopBugCompanion.exe")
        sys._MEIPASS = str(self.meipass)
        return self

    def __exit__(self, *exc):
        if self._saved["frozen"] is None:
            del sys.frozen
        else:
            sys.frozen = self._saved["frozen"]
        sys.executable = self._saved["executable"]
        if self._saved["_MEIPASS"] is None:
            if hasattr(sys, "_MEIPASS"):
                del sys._MEIPASS
        else:
            sys._MEIPASS = self._saved["_MEIPASS"]
        return False


def write_preset(path: Path, name: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
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


def check_source_build() -> None:
    # The ordinary development case: a relative path finds the project's own
    # preset, and the resolved file actually exists.
    resolved = resolve_preset_path("presets/colony.json")
    assert resolved.is_file(), resolved
    assert resolved.name == "colony.json", resolved
    assert resolved.samefile(ROOT / "presets" / "colony.json"), resolved

    # An absolute path is handed back untouched.
    absolute = (ROOT / "presets" / "default.json").resolve()
    assert resolve_preset_path(absolute) == absolute
    assert resolve_preset_path(str(absolute)) == absolute

    # The default argparse value resolves to a real file.
    assert resolve_preset_path("presets/default.json").is_file()


def check_frozen_build() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        exe_dir = tmp_path / "dist"
        meipass = tmp_path / "_MEI12345"
        exe_dir.mkdir(parents=True, exist_ok=True)

        # Deliberately a name that exists nowhere in the repository, so a fall
        # back to the working directory could not accidentally satisfy it.
        write_preset(meipass / "presets" / "bundled_only.json", "BundledOnly")

        with FakeFrozen(exe_dir, meipass):
            # This is the exact call that used to crash: nothing lives beside
            # the executable, so the bundled copy must be found instead.
            assert not (exe_dir / "presets" / "bundled_only.json").exists()
            resolved = resolve_preset_path("presets/bundled_only.json")
            assert resolved.is_file(), resolved
            assert resolved.samefile(meipass / "presets" / "bundled_only.json"), resolved

            # A bare name means a preset, not a file in the application root.
            bare = resolve_preset_path("bundled_only.json")
            assert bare.is_file(), bare
            assert bare.samefile(meipass / "presets" / "bundled_only.json"), bare

            # An editable copy beside the executable still wins over the
            # bundled one, which is the documented override behaviour.
            write_preset(exe_dir / "presets" / "bundled_only.json", "Edited")
            resolved = resolve_preset_path("presets/bundled_only.json")
            assert resolved.samefile(exe_dir / "presets" / "bundled_only.json"), resolved
            assert json.loads(resolved.read_text(encoding="utf-8"))["name"] == "Edited"

            # A name that exists nowhere comes back as a path rather than an
            # exception, so the caller can report what the user typed.
            missing = resolve_preset_path("presets/no_such_preset.json")
            assert not missing.exists()
            assert missing.name == "no_such_preset.json", missing


def check_engine_uses_it() -> None:
    # Guard against the engine quietly going back to resolving against the
    # writable root only, which is what produced the crash.
    engine_src = (ROOT / "src" / "desktop_bug" / "engine.py").read_text(encoding="utf-8")
    assert "resolve_preset_path(args.preset)" in engine_src, "engine no longer resolves via discovery"
    assert "app_root() / preset" not in engine_src, "engine resolves a preset against the writable root again"

    # Saving is the opposite case and must keep using the writable root: the
    # extraction directory is temporary and discarded when the app exits.
    preset_io_src = (ROOT / "src" / "desktop_bug" / "preset_io.py").read_text(encoding="utf-8")
    assert "root = app_root()" in preset_io_src, "saving no longer targets the writable root"


def main() -> int:
    check_source_build()
    check_frozen_build()
    check_engine_uses_it()
    print("preset path smoke: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
