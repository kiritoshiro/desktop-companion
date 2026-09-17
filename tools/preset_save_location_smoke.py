"""Saving a preset must never overwrite one that ships with the build.

The saved filename came from the preset's *name*, so a preset called
``Default`` was written to ``presets/Default.json`` -- the same file as the
shipped ``presets/default.json`` on a case-insensitive filesystem. The first
Save a user pressed replaced version-controlled data with their own
configuration, silently. It happened twice during development before the
mechanism was understood.
"""

import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.discovery import (  # noqa: E402
    discover_presets,
    is_shipped_preset,
    resolve_preset_path,
    shipped_presets_dirs,
    user_presets_dir,
)
from desktop_bug.preset_io import load_preset, save_preset  # noqa: E402

SHIPPED = ROOT / "presets"


def preset(name: str, model: str = "tarantula") -> dict:
    return {
        "name": name,
        "slots": [{"model": model, "personality": "mellow", "count": 1, "slot_id": "slot-0"}],
        "settings": {},
    }


def check_shipped_detection() -> None:
    dirs = [str(d).casefold() for d in shipped_presets_dirs()]
    assert str(SHIPPED).casefold() in dirs, dirs

    # Both spellings, because that collision is the whole bug.
    assert is_shipped_preset(SHIPPED / "default.json")
    assert is_shipped_preset(SHIPPED / "Default.json")
    assert is_shipped_preset(SHIPPED / "colony.json")
    assert not is_shipped_preset(user_presets_dir() / "Default.json")
    assert not is_shipped_preset(Path(tempfile.gettempdir()) / "elsewhere" / "default.json")


def check_save_goes_to_the_user_folder() -> None:
    """The exact sequence that destroyed the shipped preset twice."""
    before = (SHIPPED / "default.json").read_bytes()

    saved = save_preset(preset("Default"))
    try:
        assert saved.parent == user_presets_dir(), saved
        assert saved.name == "Default.json", saved
        assert saved.is_file()

        after = (SHIPPED / "default.json").read_bytes()
        assert after == before, "saving a preset named Default overwrote the shipped one"

        # And it round-trips.
        assert load_preset(saved)["name"] == "Default"
    finally:
        saved.unlink(missing_ok=True)


def check_shipped_path_is_refused() -> None:
    """Even an explicit path into the shipped folder must be rejected.

    The write is restored in a ``finally`` rather than merely asserted about.
    If the guard is broken -- which is exactly when this test matters -- the
    save succeeds, and without the restore the test would destroy the very
    repository data it exists to protect. It did, once.
    """
    target_file = SHIPPED / "colony.json"
    before = target_file.read_bytes()
    try:
        for target in (target_file, SHIPPED / "Colony.json"):
            try:
                save_preset(preset("Colony"), target)
            except ValueError as exc:
                assert "ships with the application" in str(exc), exc
            else:
                raise AssertionError(f"writing {target.name} was allowed")
        assert target_file.read_bytes() == before
    finally:
        if target_file.read_bytes() != before:
            target_file.write_bytes(before)


def check_user_preset_shadows_shipped() -> None:
    """A user's copy wins over the shipped one rather than replacing it."""
    saved = save_preset(preset("Colony", model="spider"))
    try:
        assert saved.parent == user_presets_dir()

        resolved = resolve_preset_path("presets/Colony.json")
        assert resolved.is_file(), resolved
        assert resolved.samefile(saved), f"the user copy did not shadow the shipped one: {resolved}"
        assert load_preset(resolved)["slots"][0]["model"] == "spider"

        # The shipped one is still there, untouched, for anyone who wants it.
        assert (SHIPPED / "colony.json").is_file()
        assert load_preset(SHIPPED / "colony.json")["slots"][0]["model"] != "spider"

        # And it appears in the list the settings window offers, exactly once.
        listed = discover_presets()
        stems = [p.stem.casefold() for p in listed]
        assert stems.count("colony") == 1, f"Colony listed {stems.count('colony')} times"
        chosen = next(p for p in listed if p.stem.casefold() == "colony")
        assert chosen.samefile(saved), "the preset list offers the shipped copy over the user's"
    finally:
        saved.unlink(missing_ok=True)


def check_shipped_presets_are_intact() -> None:
    """A blunt guard: every shipped preset still loads and names a real model."""
    models = {p.parent.name for p in (ROOT / "models").glob("*/model.json")}
    for path in sorted(SHIPPED.glob("*.json")):
        data = load_preset(path)
        assert data.get("slots"), f"{path.name} has no slots"
        for slot in data["slots"]:
            assert slot.get("model") in models, f"{path.name} names a missing model: {slot.get('model')}"


def main() -> int:
    check_shipped_detection()
    check_save_goes_to_the_user_folder()
    check_shipped_path_is_refused()
    check_user_preset_shadows_shipped()
    check_shipped_presets_are_intact()
    print(f"preset save location smoke: OK (user presets -> {user_presets_dir()})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
