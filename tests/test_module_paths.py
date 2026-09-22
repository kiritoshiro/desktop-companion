"""Every module path in the package resolves to a module that exists.

DC-43 moved every module into subpackages. The suite went green, ruff was
clean, and two references were still broken -- because neither kind is
checked by anything that runs:

* ``from .creature import Creature`` inside a function in
  ``app/config_ui.py``. Moving the file one level deeper turned it into
  ``desktop_bug.app.creature``, which does not exist. It sat inside a
  ``try``/``except Exception`` that drew a fallback icon, so every model in
  the settings window quietly lost its legs instead of raising.
* ``"desktop_bug.engine"`` as a string in the subprocess command that
  launches the overlay. A module path in a string is invisible to the import
  machinery and to ruff, so pressing "Save and launch overlay" reported
  success while the child process died immediately.

Both were found by a person using the application, two days after the move.
These two tests are the cheap version of that person.
"""

from __future__ import annotations

import ast
import importlib.util
import re
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parents[1] / "src" / "desktop_bug"

# A dotted path written in a string literal. Anchored on the package name so
# this does not try to resolve arbitrary prose.
MODULE_STRING = re.compile(r"""["'](desktop_bug(?:\.[A-Za-z_][A-Za-z0-9_]*)+)["']""")


def _module_exists(name: str) -> bool:
    try:
        return importlib.util.find_spec(name) is not None
    except (ImportError, AttributeError, ValueError):
        return False


def _package_of(path: Path) -> str:
    """The package a module's relative imports are resolved against."""
    parts = list(path.relative_to(PACKAGE_ROOT.parent).with_suffix("").parts)
    if path.name == "__init__.py":
        parts = parts[:-1]
    else:
        parts = parts[:-1]
    return ".".join(parts)


def _source_files() -> list[Path]:
    files = sorted(PACKAGE_ROOT.rglob("*.py"))
    assert len(files) > 40, f"expected the whole package, found {len(files)} files"
    return files


def test_every_relative_import_resolves():
    """Including imports inside functions, which no import of the package runs."""
    broken = []
    for path in _source_files():
        package = _package_of(path)
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if not isinstance(node, ast.ImportFrom) or not node.level:
                continue
            base = package.split(".")
            climb = node.level - 1
            anchor = base[: len(base) - climb] if climb else base
            target = ".".join(anchor + ([node.module] if node.module else []))
            if not _module_exists(target):
                written = "." * node.level + (node.module or "")
                broken.append(f"{path.name}:{node.lineno}  {written!r} -> {target}")
    assert not broken, "relative imports that do not resolve:\n  " + "\n  ".join(broken)


def test_every_module_path_written_as_a_string_resolves():
    """A path in a string is not checked by the import machinery or by ruff."""
    broken = []
    for path in _source_files():
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in MODULE_STRING.finditer(line):
                name = match.group(1)
                if not _module_exists(name):
                    broken.append(f"{path.name}:{lineno}  {name!r}")
    assert not broken, "module paths in strings that do not resolve:\n  " + "\n  ".join(broken)
