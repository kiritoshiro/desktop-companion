"""The build must be defined once, and a release must not ship untested code.

Four places used to describe "the build": build_exe.bat, both GitHub workflows,
and DesktopBugCompanion.spec, which sat unused beside them. The release
workflow validated data and ran no tests at all, so it could publish code the
CI workflow would have rejected. Both are the sort of drift nobody notices
until a release is already out.
"""

import re


from desktop_bug import __version__
from support import ROOT

SPEC = ROOT / "DesktopBugCompanion.spec"
BATCH = ROOT / "build_exe.bat"
CI = ROOT / ".github" / "workflows" / "ci.yml"
RELEASE = ROOT / ".github" / "workflows" / "release-windows.yml"
REQUIREMENTS = ROOT / "requirements.txt"
RUNNER = ROOT / "tools" / "run_all_checks.py"
PYPROJECT = ROOT / "pyproject.toml"
TESTS = ROOT / "tests"


def test_one_build_definition() -> None:
    """Everything that builds must go through the spec, not its own flag list."""
    assert SPEC.is_file(), "the spec file is gone; something else now defines the build"
    spec = SPEC.read_text(encoding="utf-8")
    # One-file and windowed live in the spec: no COLLECT step, console off.
    assert "COLLECT" not in spec, "the spec builds one-folder; the README and docs say one-file"
    assert "console=False" in spec, "the spec no longer builds a windowed executable"
    assert "desktop_bug.app.engine" in spec, "the spec lost the hidden import the overlay needs"
    for data in ("models", "personalities", "presets"):
        assert f"'{data}'" in spec, f"the spec no longer bundles {data}"

    for path in (BATCH, CI, RELEASE):
        text = path.read_text(encoding="utf-8")
        assert "DesktopBugCompanion.spec" in text, f"{path.name} does not build from the spec"
        # A second flag list is exactly the drift this prevents.
        assert "--onefile" not in text, f"{path.name} spells the build out again instead of using the spec"
        assert "--add-data" not in text, f"{path.name} spells the bundled data out again"


def test_release_runs_the_tests() -> None:
    """A release must run what CI runs."""
    assert RUNNER.is_file(), "the shared checks runner is missing"
    for path in (CI, RELEASE):
        text = path.read_text(encoding="utf-8")
        assert "tools/run_all_checks.py" in text, f"{path.name} does not run the shared checks"
        # The old per-test lists are what drifted; they must not come back,
        # in either the old smoke-script spelling or the pytest one.
        listed = re.findall(r"python (?:-m pytest )?tools/([a-z0-9_]+_smoke\.py)", text)
        listed += re.findall(r"pytest\s+tests/(test_[a-z0-9_]+\.py)", text)
        assert not listed, f"{path.name} lists individual tests again: {listed}"


def test_runner_delegates_discovery_to_pytest() -> None:
    """The runner must find tests by looking, not by being told.

    Discovery moved from a glob in this script to pytest in DC-08. What must
    not come back is a list: a forgotten entry in one fails silently by simply
    running nothing, which is how a release once shipped with no tests at all.
    """
    runner = RUNNER.read_text(encoding="utf-8")
    assert '"-m", "pytest"' in runner, "the checks runner no longer runs pytest"
    assert '"ruff", "check"' in runner, "the checks runner no longer lints"

    config = PYPROJECT.read_text(encoding="utf-8")
    assert "[tool.pytest.ini_options]" in config, "pytest is not configured in pyproject.toml"
    assert 'testpaths = ["tests"]' in config, "pytest is not pointed at tests/"
    assert "[tool.ruff]" in config, "ruff is not configured in pyproject.toml"

    found = sorted(p.name for p in TESTS.glob("test_*.py"))
    assert len(found) >= 15, f"only {len(found)} test modules found, which suggests a broken layout"
    assert (TESTS / "conftest.py").is_file(), "the shared test setup is missing"


def test_the_tools_ci_needs_are_pinned() -> None:
    """CI installs one requirements file; what it runs has to be in it."""
    text = REQUIREMENTS.read_text(encoding="utf-8")
    for tool in ("pytest", "ruff"):
        assert f"{tool}==" in text, f"{tool} is not pinned, so CI cannot run it"


def test_requirements_are_pinned() -> None:
    lines = [
        line.strip()
        for line in REQUIREMENTS.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]
    assert lines, "requirements.txt has no requirements"
    for line in lines:
        assert "==" in line, f"requirement is not pinned to an exact version: {line}"
        assert ">=" not in line and "<" not in line, f"requirement carries a range: {line}"


def test_python_versions_agree() -> None:
    versions = set()
    for path in (CI, RELEASE):
        found = re.findall(r"python-version:\s*'([^']+)'", path.read_text(encoding="utf-8"))
        assert found, f"{path.name} does not pin a Python version"
        versions.update(found)
    assert len(versions) == 1, f"the workflows disagree about Python: {sorted(versions)}"


def test_release_guards_the_tag() -> None:
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
