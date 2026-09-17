"""Graceful shutdown between the settings window and the overlay process.

The settings window used to stop the overlay with ``Popen.terminate()``, which
on Windows is ``TerminateProcess``.  The overlay is killed outright: Qt's
``aboutToQuit`` hook never runs, so everything dirtied since the last debounced
flush -- freshly earned XP, a renamed spider, base build progress -- is lost.

The two are separate processes, so the settings window cannot save the
overlay's state itself.  It has to ask.  A single small file beside the runtime
state is enough, because the overlay already polls that directory often for
preset changes, and a file works the same way on every platform.

This is deliberately modest.  The planned local socket channel replaces it with
a real two-way link; until then this closes the data-loss hole without adding a
protocol.  The module is free of Qt so both processes and the tests can use it.
"""

from __future__ import annotations

import time
from pathlib import Path


STOP_REQUEST_NAME = "stop-request"

# How long the settings window waits for the overlay to save and exit before
# killing it. Saving is a single small JSON write, so this is generous; it only
# matters when the overlay is wedged.
STOP_GRACE_SECONDS = 5.0
STOP_POLL_SECONDS = 0.1


def stop_request_path(state_dir) -> Path:
    return Path(state_dir) / STOP_REQUEST_NAME


def request_stop(state_dir) -> bool:
    """Ask a running overlay to save and quit.

    Returns whether the request could be left at all; if it could not, the
    caller has no polite option and must fall back to killing the process.
    """
    path = stop_request_path(state_dir)
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(str(int(time.time())), encoding="utf-8")
        return True
    except OSError:
        return False


def consume_stop_request(state_dir) -> bool:
    """Return True exactly once per request, removing it as it is read."""
    path = stop_request_path(state_dir)
    try:
        if not path.is_file():
            return False
        path.unlink()
        return True
    except OSError:
        return False


def clear_stop_request(state_dir) -> None:
    """Drop any pending request.

    Called when an overlay starts and after a stop finishes.  Without it a
    request left behind by a crash would make the next overlay quit the moment
    it finished loading.
    """
    try:
        stop_request_path(state_dir).unlink()
    except OSError:
        return


def stop_process(
    process,
    state_dir,
    grace: float = STOP_GRACE_SECONDS,
    poll: float = STOP_POLL_SECONDS,
    sleep=time.sleep,
) -> str:
    """Stop the overlay, preferring a save over a kill.

    Returns ``"not-running"``, ``"graceful"`` if the overlay saved and exited
    on its own, or ``"terminated"`` if it had to be killed.  ``sleep`` is
    injectable so the settings window can keep repainting while it waits and so
    tests do not spend real time.
    """
    if process is None or process.poll() is not None:
        clear_stop_request(state_dir)
        return "not-running"

    if not request_stop(state_dir):
        # Nowhere to leave the request, so there is no polite option left.
        process.terminate()
        return "terminated"

    waited = 0.0
    step = max(0.01, float(poll))
    while waited < float(grace):
        if process.poll() is not None:
            clear_stop_request(state_dir)
            return "graceful"
        sleep(step)
        waited += step

    # The overlay is wedged or ignoring the request. Take the old behaviour,
    # accepting the loss, rather than leaving a process running forever.
    clear_stop_request(state_dir)
    process.terminate()
    return "terminated"
