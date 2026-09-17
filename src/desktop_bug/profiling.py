"""Per-system frame timing, so the overlay's cost is measured rather than guessed.

Nothing in the frame loop was instrumented, so every claim about what makes the
overlay expensive was a hunch. This module is the measurement: named sections
accumulate per frame, a rolling window keeps the recent history, and the numbers
come out as a table for the benchmark or a small on-screen HUD.

It is deliberately Qt-free so the benchmark and the tests can use it without a
display, and it is off unless `DESKTOP_BUG_PROFILE` asks for it. When it is off
every call goes through `NullProfiler`, whose section objects are shared and do
nothing, so an unprofiled frame pays a handful of attribute lookups.
"""

from __future__ import annotations

import os
import time
from collections import deque
from typing import Callable, Deque, Dict, Iterable, List, Tuple

PROFILE_ENV = "DESKTOP_BUG_PROFILE"
HUD_ENV = "DESKTOP_BUG_PROFILE_HUD"
# Two seconds of history at 60 FPS. Long enough that one slow frame does not
# dominate the mean, short enough that the HUD reacts while you watch it.
DEFAULT_WINDOW = 120
# The order sections appear in the HUD and the benchmark table. A section that
# is not listed still records; it simply sorts after these.
SECTION_ORDER = (
    "frame",
    "behaviour",
    "creatures",
    "render",
    "paint",
    "repaint-region",
    "webs",
    "mouse-webs",
    "flies",
    "jobs",
    "desktop",
    "desktop-probe",
)


def _truthy(value) -> bool:
    if value is None:
        return False
    return str(value).strip().lower() not in ("", "0", "false", "no", "off")


def profiling_requested(env: dict | None = None) -> bool:
    """True when the environment asks for timing."""
    source = os.environ if env is None else env
    return _truthy(source.get(PROFILE_ENV))


def hud_requested(env: dict | None = None) -> bool:
    """True when the on-screen HUD is wanted as well as the timing."""
    source = os.environ if env is None else env
    return profiling_requested(source) and _truthy(source.get(HUD_ENV, "1"))


def percentile(samples: Iterable[float], fraction: float) -> float:
    """Nearest-rank percentile; 0.0 for no samples.

    Nearest-rank rather than an interpolating variant because these are frame
    times, and a frame that really happened is a more honest answer than a
    number that never did.
    """
    ordered = sorted(samples)
    if not ordered:
        return 0.0
    fraction = min(1.0, max(0.0, float(fraction)))
    rank = int(round(fraction * (len(ordered) - 1)))
    return ordered[rank]


class Section:
    """A named span, reused across frames so timing allocates nothing."""

    __slots__ = ("name", "_profiler", "_started")

    def __init__(self, name: str, profiler: "FrameProfiler") -> None:
        self.name = name
        self._profiler = profiler
        self._started = 0.0

    def __enter__(self) -> "Section":
        self._started = self._profiler.clock()
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self._profiler.add(self.name, self._profiler.clock() - self._started)
        return False


class _NullSection:
    """Shared do-nothing span used when profiling is off."""

    __slots__ = ()

    def __enter__(self) -> "_NullSection":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        return False


_NULL_SECTION = _NullSection()


class FrameProfiler:
    """Accumulates named section times per frame and keeps a rolling window."""

    enabled = True

    def __init__(self, window: int = DEFAULT_WINDOW,
                 clock: Callable[[], float] = time.perf_counter) -> None:
        self.clock = clock
        self.window = max(1, int(window))
        self._sections: Dict[str, Section] = {}
        self._history: Dict[str, Deque[float]] = {}
        self._current: Dict[str, float] = {}
        self._frame_started = 0.0
        self._frame_open = False
        self.frames = 0

    # -- recording ---------------------------------------------------
    def section(self, name: str) -> Section:
        span = self._sections.get(name)
        if span is None:
            span = Section(name, self)
            self._sections[name] = span
        return span

    def add(self, name: str, seconds: float) -> None:
        """Record time against a section.

        Inside a frame, sections accumulate rather than overwrite, so one
        entered once per creature reports what that system cost across the whole
        frame. Outside a frame it records a sample of its own: Qt delivers the
        paint event after `tick` has returned, so painting is not inside any
        frame and would otherwise be silently dropped.
        """
        if self._frame_open:
            self._current[name] = self._current.get(name, 0.0) + seconds
        else:
            self._record(name, seconds * 1000.0)

    def _record(self, name: str, milliseconds: float) -> None:
        history = self._history.get(name)
        if history is None:
            history = deque(maxlen=self.window)
            self._history[name] = history
        history.append(milliseconds)

    def begin_frame(self) -> None:
        self._current = {}
        self._frame_started = self.clock()
        self._frame_open = True

    def end_frame(self) -> None:
        self._current["frame"] = self.clock() - self._frame_started
        self._frame_open = False
        for name, seconds in self._current.items():
            self._record(name, seconds * 1000.0)
        self._current = {}
        self.frames += 1

    def reset(self) -> None:
        self._history.clear()
        self._current = {}
        self._frame_open = False
        self.frames = 0

    # -- reporting ---------------------------------------------------
    def names(self) -> List[str]:
        """Section names, the well-known ones first and the rest alphabetical."""
        known = [name for name in SECTION_ORDER if name in self._history]
        rest = sorted(name for name in self._history if name not in SECTION_ORDER)
        return known + rest

    def stats(self, name: str) -> Tuple[float, float, float]:
        """Return (mean_ms, p95_ms, max_ms) over the rolling window."""
        history = self._history.get(name)
        if not history:
            return (0.0, 0.0, 0.0)
        return (sum(history) / len(history), percentile(history, 0.95), max(history))

    def table(self) -> List[Tuple[str, float, float, float]]:
        return [(name,) + self.stats(name) for name in self.names()]

    def hud_lines(self) -> List[str]:
        """Short lines for the on-screen HUD, costliest system first."""
        rows = self.table()
        if not rows:
            return []
        frame = [row for row in rows if row[0] == "frame"]
        others = sorted((row for row in rows if row[0] != "frame"),
                        key=lambda row: row[1], reverse=True)
        lines = []
        for name, mean, p95, _worst in frame + others[:6]:
            lines.append(f"{name:<14} {mean:5.2f} ms  p95 {p95:5.2f}")
        if frame and frame[0][1] > 0.0:
            lines.append(f"{'~fps':<14} {1000.0 / frame[0][1]:5.1f}")
        return lines


class NullProfiler:
    """The profiler used when nothing is being measured."""

    enabled = False
    frames = 0

    def section(self, name: str) -> _NullSection:
        return _NULL_SECTION

    def add(self, name: str, seconds: float) -> None:
        return None

    def begin_frame(self) -> None:
        return None

    def end_frame(self) -> None:
        return None

    def reset(self) -> None:
        return None

    def names(self) -> List[str]:
        return []

    def stats(self, name: str) -> Tuple[float, float, float]:
        return (0.0, 0.0, 0.0)

    def table(self) -> List[Tuple[str, float, float, float]]:
        return []

    def hud_lines(self) -> List[str]:
        return []


NULL_PROFILER = NullProfiler()

_active: object = NULL_PROFILER


def get_profiler():
    """The profiler the frame loop should record into."""
    return _active


def set_profiler(profiler) -> None:
    global _active
    _active = profiler if profiler is not None else NULL_PROFILER


def start_profiling(window: int = DEFAULT_WINDOW) -> FrameProfiler:
    """Switch measurement on and return the profiler now in use."""
    profiler = FrameProfiler(window=window)
    set_profiler(profiler)
    return profiler


def stop_profiling() -> None:
    set_profiler(NULL_PROFILER)


def profiler_from_env(env: dict | None = None):
    """A real profiler when the environment asks for one, otherwise the null one."""
    if profiling_requested(env):
        return start_profiling()
    return get_profiler()
