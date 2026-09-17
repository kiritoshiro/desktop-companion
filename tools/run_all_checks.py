"""Run every check the project has, in one place.

The test list used to be typed out in `ci.yml` and nowhere else, so the release
workflow validated data and ran no tests at all: a release could ship code that
CI would have rejected. Adding a test also meant remembering to add a line to a
workflow, and a forgotten line fails silently by simply not running anything.

Both workflows and a developer now call this script. Since DC-08 the tests
themselves are pytest modules under `tests/`, so discovery is pytest's job: a
new `tests/test_<name>.py` is picked up everywhere the moment it exists, and
the runner adds what pytest does not do -- compiling every source file,
validating the shipped data, and linting.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def child_env() -> dict:
    env = dict(os.environ)
    env["PYTHONPATH"] = str(ROOT / "src") + os.pathsep + env.get("PYTHONPATH", "")
    # Qt must not need a display, and a check run must never rewrite a real
    # player's saved spiders.
    env.setdefault("QT_QPA_PLATFORM", "offscreen")
    env.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-checks-"))
    return env


def run(label: str, args: list[str], env: dict, quiet: bool) -> tuple[str, bool, float]:
    started = time.monotonic()
    result = subprocess.run(
        args,
        cwd=str(ROOT),
        env=env,
        capture_output=quiet,
        text=True,
    )
    elapsed = time.monotonic() - started
    ok = result.returncode == 0
    if not ok and quiet:
        # Only the failing output is worth printing; a green run stays quiet.
        sys.stdout.write(result.stdout or "")
        sys.stderr.write(result.stderr or "")
    print(f"{'PASS' if ok else 'FAIL'}  {label}  ({elapsed:.1f}s)")
    return label, ok, elapsed


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Run compile, data validation and every smoke test")
    parser.add_argument("--verbose", action="store_true", help="Show output from checks that pass too")
    args = parser.parse_args(argv)
    quiet = not args.verbose

    env = child_env()
    python = [sys.executable]
    results: list[tuple[str, bool, float]] = []

    results.append(run("compile", python + ["-m", "compileall", "-q", "src", "tools", "launcher.py"], env, quiet))

    models = sorted((ROOT / "models").glob("*/model.json"))
    presets = sorted((ROOT / "presets").glob("*.json"))
    if not models or not presets:
        print("FAIL  data discovery: no models or no presets found")
        return 1

    ok = True
    started = time.monotonic()
    for path in models:
        if subprocess.run(python + ["tools/validate_model.py", str(path)], cwd=str(ROOT), env=env,
                          capture_output=quiet, text=True).returncode != 0:
            print(f"      invalid model: {path.parent.name}")
            ok = False
    print(f"{'PASS' if ok else 'FAIL'}  validate {len(models)} models  ({time.monotonic() - started:.1f}s)")
    results.append(("validate models", ok, 0.0))

    ok = True
    started = time.monotonic()
    for path in presets:
        if subprocess.run(python + ["tools/validate_preset.py", str(path)], cwd=str(ROOT), env=env,
                          capture_output=quiet, text=True).returncode != 0:
            print(f"      invalid preset: {path.name}")
            ok = False
    print(f"{'PASS' if ok else 'FAIL'}  validate {len(presets)} presets  ({time.monotonic() - started:.1f}s)")
    results.append(("validate presets", ok, 0.0))

    modules = sorted((ROOT / "tests").glob("test_*.py"))
    if not modules:
        print("FAIL  no test modules were discovered, which is never correct")
        return 1
    results.append(run("pytest", python + ["-m", "pytest"] + ([] if quiet else ["-v"]),
                       env, quiet))

    # Lint is part of "one command runs everything": a finding that only shows
    # up when somebody remembers to run ruff by hand is a finding nobody sees.
    results.append(run("ruff", python + ["-m", "ruff", "check", "src", "tests", "tools"],
                       env, quiet))

    failed = [label for label, ok, _ in results if not ok]
    total = sum(elapsed for _, _, elapsed in results)
    print()
    print(f"{len(results) - len(failed)}/{len(results)} checks passed in {total:.0f}s "
          f"({len(modules)} test modules, {len(models)} models, {len(presets)} presets)")
    if failed:
        print("failed: " + ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
