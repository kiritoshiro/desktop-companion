"""The README must describe the repository that exists.

It promised six showcase presets that have never existed in git history,
described both a one-file and a one-folder build in different sections, and
listed a project structure that omitted every module added since the original
import. Documentation drifts silently; this makes it fail loudly instead.
"""

import re
from support import ROOT


README = ROOT / "README.md"
SRC = ROOT / "src" / "desktop_bug"
PRESETS = ROOT / "presets"

TEXT = README.read_text(encoding="utf-8")

# "The **Colony** preset ..." or "... preset **Colony**".
PRESET_CLAIM = re.compile(r"\*\*([A-Z][A-Za-z0-9 -]{1,28}?)\*\*\s+preset\b|\bpreset\s+\*\*([A-Z][A-Za-z0-9 -]{1,28}?)\*\*")


def preset_slug(name: str) -> str:
    return name.strip().lower().replace(" ", "-")


def test_single_title() -> None:
    titles = [line for line in TEXT.splitlines() if line.startswith("# ")]
    assert len(titles) == 1, f"expected one document title, found {titles}"


def test_named_presets_exist() -> None:
    """Every preset the README names by title must be in presets/."""
    available = {q.stem for q in PRESETS.glob("*.json")}
    missing = []
    for match in PRESET_CLAIM.finditer(TEXT):
        name = match.group(1) or match.group(2)
        slug = preset_slug(name)
        if slug not in available:
            missing.append(f"{name!r} -> presets/{slug}.json")
    assert not missing, (
        "the README names presets that do not exist:\n  " + "\n  ".join(missing)
        + f"\navailable: {sorted(available)}"
    )


def test_referenced_preset_paths_exist() -> None:
    """Every presets/<file>.json path written out must resolve."""
    missing = []
    for path in sorted(set(re.findall(r"presets[\\/]([A-Za-z0-9_.-]+\.json)", TEXT))):
        if not (PRESETS / path).is_file():
            missing.append(f"presets/{path}")
    assert not missing, "the README references preset files that do not exist:\n  " + "\n  ".join(missing)


def test_one_build_story() -> None:
    """The README must describe the build that is actually performed.

    The spec file is the single definition of the build; the batch file and the
    workflows run it rather than repeating its flags, so that is what the
    README has to agree with.
    """
    batch = (ROOT / "build_exe.bat").read_text(encoding="utf-8")
    spec = (ROOT / "DesktopBugCompanion.spec").read_text(encoding="utf-8")
    assert "DesktopBugCompanion.spec" in batch, "build_exe.bat no longer builds from the spec"
    # One-file means a single EXE with the data inlined and no COLLECT step.
    assert "COLLECT" not in spec, "the spec builds one-folder; the README says one-file"
    assert "one-folder" not in TEXT, "the README still describes a one-folder build"
    # The one-folder layout puts the exe inside a folder of the same name.
    assert "DesktopBugCompanion\\DesktopBugCompanion.exe" not in TEXT, (
        "the README still points at the one-folder executable path"
    )
    assert "dist\\DesktopBugCompanion.exe" in TEXT, "the README does not name the built executable"


def test_the_package_folders_it_names_exist() -> None:
    """DC-86 cut the README's 120-line module listing at the owner's request
    ("readme ... getting too big, reduce unnecessary text"). That listing was
    what this test kept in step with the code; with no listing there is
    nothing to drift. What the README does name -- the package's folders by
    concern -- must still be real."""
    named = re.findall(r"`([a-z_]+)/`", TEXT[TEXT.index("Code lives in"):])
    assert named, "the README no longer says where the code lives"
    for folder in named:
        assert (SRC / folder).is_dir(), f"the README names src/desktop_bug/{folder}/, which does not exist"


def test_named_paths_exist() -> None:
    """Top-level files the README tells a user to run must be present."""
    for name in ("run_dev.bat", "build_exe.bat", "requirements.txt", "launcher.py"):
        assert name in TEXT, f"the README no longer mentions {name}"
        assert (ROOT / name).is_file(), f"the README names {name}, which does not exist"


def test_module_references_exist() -> None:
    """Any src/desktop_bug/<module>.py the prose points at must exist."""
    missing = []
    for name in sorted(set(re.findall(r"src[\\/]desktop_bug[\\/]([a-z0-9_]+\.py)", TEXT))):
        # The prose names a module, not its folder, so it counts as present
        # wherever in the package tree it lives.
        if not any(SRC.rglob(name)):
            missing.append(name)
    assert not missing, "the README references modules that do not exist:\n  " + "\n  ".join(missing)
