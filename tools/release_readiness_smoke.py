"""The build must be defined once, and a release must not ship untested code.

Four places used to describe "the build": build_exe.bat, both GitHub workflows,
and DesktopBugCompanion.spec, which sat unused beside them. The release
workflow validated data and ran no tests at all, so it could publish code the
CI workflow would have rejected. Both are the sort of drift nobody notices
until a release is already out.
"""

import os
import re
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug import __version__  # noqa: E402

SPEC = ROOT / "DesktopBugCompanion.spec"
BATCH = ROOT / "build_exe.bat"
CI = ROOT / ".github" / "workflows" / "ci.yml"
RELEASE = ROOT / ".github" / "workflows" / "release-windows.yml"
REQUIREMENTS = ROOT / "requirements.txt"
RUNNER = ROOT / "tools" / "run_all_checks.py"


def check_one_build_definition() -> None:
    """Everything that builds must go through the spec, not its own flag list."""
    assert SPEC.is_file(), "the spec file is gone; something else now defines the build"
    spec = SPEC.read_text(encoding="utf-8")
    # One-file and windowed live in the spec: no COLLECT step, console off.
    assert "COLLECT" not in spec, "the spec builds one-folder; the README and docs say one-file"
    assert "console=False" in spec, "the spec no longer builds a windowed executable"
    assert "desktop_bug.engine" in spec, "the spec lost the hidden import the overlay needs"
    for data in ("models", "personalities", "presets"):
        assert f"'{data}'" in spec, f"the spec no longer bundles {data}"

    for path in (BATCH, CI, RELEASE):
        text = path.read_text(encoding="utf-8")
        assert "DesktopBugCompanion.spec" in text, f"{path.name} does not build from the spec"
        # A second flag list is exactly the drift this prevents.
        assert "--onefile" not in text, f"{path.name} spells the build out again instead of using the spec"
        assert "--add-data" not in text, f"{path.name} spells the bundled data out again"


def check_release_runs_the_tests() -> None:
    """A release must run what CI runs."""
    assert RUNNER.is_file(), "the shared checks runner is missing"
    for path in (CI, RELEASE):
        text = path.read_text(encoding="utf-8")
        assert "tools/run_all_checks.py" in text, f"{path.name} does not run the shared checks"
        # The old per-test lists are what drifted; they must not come back.
        listed = re.findall(r"python tools/([a-z0-9_]+_smoke\.py)", text)
        assert not listed, f"{path.name} lists individual smoke tests again: {listed}"


def check_runner_discovers_tests() -> None:
    """The runner must find tests by looking, not by being told."""
    text = RUNNER.read_text(encoding="utf-8")
    assert '"*_smoke.py"' in text, "the checks runner no longer discovers smoke tests"
    found = sorted(p.name for p in (ROOT / "tools").glob("*_smoke.py"))
    assert len(found) >= 15, f"only {len(found)} smoke tests found, which suggests a broken layout"


def check_requirements_are_pinned() -> None:
    lines = [
        line.strip()
        for line in REQUIREMENTS.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]
    assert lines, "requirements.txt has no requirements"
    for line in lines:
        assert "==" in line, f"requirement is not pinned to an exact version: {line}"
        assert ">=" not in line and "<" not in line, f"requirement carries a range: {line}"


def check_python_versions_agree() -> None:
    versions = set()
    for path in (CI, RELEASE):
        found = re.findall(r"python-version:\s*'([^']+)'", path.read_text(encoding="utf-8"))
        assert found, f"{path.name} does not pin a Python version"
        versions.update(found)
    assert len(versions) == 1, f"the workflows disagree about Python: {sorted(versions)}"


def check_release_guards_the_tag() -> None:
    text = RELEASE.read_text(encoding="utf-8")
    # Assert on the machinery, not on the word appearing somewhere: an earlier
    # version of this check passed while the comparison had been gutted,
    # because "__version__" still occurred in the failure message.
    assert "import desktop_bug; print(desktop_bug.__version__)" in text, (
        "the release workflow does not read desktop_bug.__version__, so a tag could "
        "publish a build reporting a different number"
    )
    assert "REF_NAME" in text, "the release workflow does not look at the tag it is building"
    assert '-ne $expected' in text, "the release workflow does not compare the tag with the version"
    assert re.fullmatch(r"\d+\.\d+\.\d+", __version__), f"__version__ is not a semantic version: {__version__}"


def main() -> int:
    check_one_build_definition()
    check_release_runs_the_tests()
    check_runner_discovers_tests()
    check_requirements_are_pinned()
    check_python_versions_agree()
    check_release_guards_the_tag()
    print(f"release readiness smoke: OK (version {__version__})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
