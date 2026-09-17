"""A failure must leave a trace a user can send and an agent can read.

The packaged build is windowed, so it has no console. Diagnostics went to a
handful of ``print`` calls that only a developer running from source would ever
see, and an unhandled exception in a Qt slot terminated the app in silence.
"""

import logging
import re
import sys
import tempfile
from pathlib import Path


from desktop_bug import __version__
from desktop_bug.logging_setup import (
    BACKUP_COUNT,
    MAX_BYTES,
    configure_logging,
    get_logger,
    install_excepthook,
    log_path,
    reset_for_tests,
    restore_excepthook,
)
from support import ROOT

SRC = ROOT / "src" / "desktop_bug"
# ``footprint(`` and friends contain "print(", so a bare substring search
# reports two dozen calls that do not exist. Require a word boundary.
PRINT_CALL = re.compile(r"(?:^|[^A-Za-z0-9_.])print\(")


def test_configures_once() -> None:
    reset_for_tests()
    with tempfile.TemporaryDirectory() as tmp:
        path = configure_logging(tmp, logging.DEBUG)
        assert path == log_path(tmp), path
        assert path.parent.is_dir()

        logger = get_logger()
        file_handlers = [h for h in logger.handlers if isinstance(h, logging.handlers.RotatingFileHandler)]
        assert len(file_handlers) == 1, logger.handlers
        assert file_handlers[0].maxBytes == MAX_BYTES
        assert file_handlers[0].backupCount == BACKUP_COUNT
        # The application owns its handlers rather than leaning on the root.
        assert logger.propagate is False

        # Configuring twice must not double every line.
        again = configure_logging(tmp, logging.DEBUG)
        assert again == path
        assert len([h for h in logger.handlers if isinstance(h, logging.handlers.RotatingFileHandler)]) == 1

        get_logger("test").info("hello %s", __version__)
        for handler in logger.handlers:
            handler.flush()
        written = path.read_text(encoding="utf-8")
        assert f"hello {__version__}" in written, written
        assert "desktop_bug.test" in written, written
        # Release the file before the directory is removed; Windows refuses to
        # delete a log an open handler still holds.
        reset_for_tests()


def test_unwritable_directory() -> None:
    """Losing the log must not stop the overlay running."""
    reset_for_tests()
    # A path whose parent is a file cannot be turned into a directory.
    with tempfile.TemporaryDirectory() as tmp:
        blocker = Path(tmp) / "blocker"
        blocker.write_text("not a directory", encoding="utf-8")
        assert configure_logging(blocker / "state") is None
        # Logging still works; it simply goes nowhere persistent.
        get_logger("test").info("this must not raise")
        reset_for_tests()


def test_excepthook_logs_and_notifies() -> None:
    reset_for_tests()
    with tempfile.TemporaryDirectory() as tmp:
        path = configure_logging(tmp, logging.DEBUG)
        notified = []
        hook = install_excepthook(notify=notified.append)
        try:
            assert sys.excepthook is hook

            try:
                raise ValueError("a spider fell over")
            except ValueError:
                sys.excepthook(*sys.exc_info())

            for handler in get_logger().handlers:
                handler.flush()
            written = path.read_text(encoding="utf-8")
            assert "Unhandled ValueError: a spider fell over" in written, written
            assert "Traceback (most recent call last)" in written, "no traceback was recorded"
            assert notified == ["ValueError: a spider fell over"], notified

            # A failure in the frame loop repeats 60 times a second. The log
            # must count repeats rather than fill up with them, and the user
            # must be told once rather than 60 times.
            for _ in range(40):
                try:
                    raise ValueError("a spider fell over")
                except ValueError:
                    sys.excepthook(*sys.exc_info())
            for handler in get_logger().handlers:
                handler.flush()
            written = path.read_text(encoding="utf-8")
            assert written.count("Traceback (most recent call last)") == 1, "the traceback was repeated"
            assert "(seen 10 times)" in written, written
            assert len(notified) == 1, notified

            # A different failure is still reported in full.
            try:
                raise KeyError("another thing")
            except KeyError:
                sys.excepthook(*sys.exc_info())
            for handler in get_logger().handlers:
                handler.flush()
            written = path.read_text(encoding="utf-8")
            assert written.count("Traceback (most recent call last)") == 2, written
            assert len(notified) == 2, notified

            # A failing notifier must not mask the crash it is reporting.
            def broken(_summary):
                raise RuntimeError("the tray is gone")

            broken_hook = install_excepthook(notify=broken)
            try:
                try:
                    raise IndexError("third thing")
                except IndexError:
                    sys.excepthook(*sys.exc_info())
            finally:
                restore_excepthook(broken_hook)
        finally:
            restore_excepthook(hook)
        assert sys.excepthook is not hook
        reset_for_tests()


def test_keyboard_interrupt_passes_through() -> None:
    reset_for_tests()
    with tempfile.TemporaryDirectory() as tmp:
        configure_logging(tmp, logging.DEBUG)
        seen = []
        original = sys.excepthook
        sys.excepthook = lambda *exc: seen.append(exc[0])
        try:
            hook = install_excepthook(notify=None)
            try:
                raise KeyboardInterrupt()
            except KeyboardInterrupt:
                sys.excepthook(*sys.exc_info())
            assert seen == [KeyboardInterrupt], seen
            restore_excepthook(hook)
        finally:
            sys.excepthook = original
            reset_for_tests()


def test_no_prints_left() -> None:
    """Guard against diagnostics going back to a console nobody sees."""
    offenders = []
    for path in sorted(SRC.glob("*.py")):
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if PRINT_CALL.search(line):
                offenders.append(f"{path.name}:{number}: {line.strip()}")
    assert not offenders, "print() in application code:\n" + "\n".join(offenders)


def test_version_is_surfaced() -> None:
    engine_src = (SRC / "engine.py").read_text(encoding="utf-8")
    config_src = (SRC / "config_ui.py").read_text(encoding="utf-8")
    assert "__version__" in engine_src, "the overlay does not log its version"
    assert "setWindowTitle(f\"Desktop Bug Companion {__version__}\")" in config_src, (
        "the settings window does not show its version"
    )
    assert re.fullmatch(r"\d+\.\d+\.\d+", __version__), __version__
