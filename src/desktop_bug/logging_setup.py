"""Application logging and the last-resort crash hook.

The packaged build is built ``--windowed``, so it has no console: anything
written to standard output goes nowhere, and a failure leaves the user with a
frozen or vanished spider and nothing to report. Diagnostics went to seven
``print`` calls that only a developer running from source would ever see.

Everything here writes to a rotating file beside the runtime state, so a user
can attach it to a bug report and an agent can read it afterwards. The module
is free of Qt so both processes, and the tests, can use it.
"""

from __future__ import annotations

import logging
import logging.handlers
import sys
from pathlib import Path


LOGGER_NAME = "desktop_bug"
LOG_FILE_NAME = "desktop-bug.log"

# Small enough to attach to a bug report, with a little history behind it.
MAX_BYTES = 512 * 1024
BACKUP_COUNT = 3

_configured = False
_log_path: Path | None = None


def log_path(state_dir) -> Path:
    """Return the log file path for a given state directory."""
    return Path(state_dir) / "logs" / LOG_FILE_NAME


def get_logger(name: str | None = None) -> logging.Logger:
    """Return the application logger, or a named child of it."""
    if not name:
        return logging.getLogger(LOGGER_NAME)
    return logging.getLogger(f"{LOGGER_NAME}.{name}")


def configure_logging(state_dir, level: int = logging.INFO) -> Path | None:
    """Attach a rotating file handler once, and return the path it writes to.

    Returns ``None`` when the directory cannot be written, which is the normal
    case for a read-only install location. Losing the log is not a reason to
    stop the overlay running, so the failure is swallowed deliberately here and
    reported by the return value.
    """
    global _configured, _log_path
    if _configured:
        return _log_path

    logger = get_logger()
    logger.setLevel(level)
    # The application owns its own handlers; it should not also inherit
    # whatever the root logger happens to have.
    logger.propagate = False

    formatter = logging.Formatter(
        "%(asctime)s %(levelname)-7s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    path = log_path(state_dir)
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        handler = logging.handlers.RotatingFileHandler(
            path, maxBytes=MAX_BYTES, backupCount=BACKUP_COUNT, encoding="utf-8"
        )
        handler.setFormatter(formatter)
        logger.addHandler(handler)
        _log_path = path
    except OSError:
        _log_path = None

    # A windowed build has no stderr at all, so guard rather than assume.
    if getattr(sys, "stderr", None) is not None:
        stream = logging.StreamHandler()
        stream.setFormatter(formatter)
        logger.addHandler(stream)

    _configured = True
    return _log_path


def reset_for_tests() -> None:
    """Drop the configured handlers so a test can configure a fresh directory."""
    global _configured, _log_path
    logger = get_logger()
    for handler in list(logger.handlers):
        logger.removeHandler(handler)
        try:
            handler.close()
        except Exception:  # noqa: BLE001 - closing must never raise here
            pass
    _configured = False
    _log_path = None


def install_excepthook(notify=None):
    """Log unhandled exceptions instead of letting them disappear.

    PyQt terminates on an unhandled exception raised inside a slot unless an
    excepthook is installed, and in a windowed build that termination is
    silent. Logging and carrying on is the better trade for a desktop toy: a
    spider behaving oddly is recoverable, a vanished app is not.

    Repeats are counted rather than written out again, because an exception in
    the frame loop would otherwise fill the log 60 times a second.
    """
    logger = get_logger("crash")
    previous = sys.excepthook
    seen: dict[tuple[str, str], int] = {}

    def hook(exc_type, exc, tb):
        if issubclass(exc_type, KeyboardInterrupt):
            previous(exc_type, exc, tb)
            return
        signature = (exc_type.__name__, str(exc))
        count = seen.get(signature, 0) + 1
        seen[signature] = count
        if count == 1:
            logger.error(
                "Unhandled %s: %s", exc_type.__name__, exc, exc_info=(exc_type, exc, tb)
            )
        elif count in (2, 10, 100) or count % 1000 == 0:
            logger.error(
                "Unhandled %s: %s (seen %d times)", exc_type.__name__, exc, count
            )
        if notify is not None and count == 1:
            try:
                notify(f"{exc_type.__name__}: {exc}")
            except Exception:  # noqa: BLE001 - a failing notifier must not mask the crash
                logger.debug("Crash notifier failed", exc_info=True)

    hook.seen = seen  # type: ignore[attr-defined]
    hook.previous = previous  # type: ignore[attr-defined]
    sys.excepthook = hook
    return hook


def restore_excepthook(hook) -> None:
    """Undo :func:`install_excepthook`, for tests and clean shutdowns."""
    previous = getattr(hook, "previous", None)
    if previous is not None and sys.excepthook is hook:
        sys.excepthook = previous
