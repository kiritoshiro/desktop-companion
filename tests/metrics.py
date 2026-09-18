"""Behaviour changes can be argued about with numbers (DC-24).

Runs a preset headlessly for N simulated minutes and reports, per spider,
which states it spent its time in, how productive its job was, and how
often it played with another spider; and, for the colony as a whole, how
far its base got and what a frame costs.

Built ahead of DC-18 so an arbiter rewrite has a *before* to compare its
*after* against, per the plan's own "build this before DC-18" instruction --
this establishes that baseline, it does not reproduce one that already
existed. Searching the 2026-09-17 work-log entries for a state-distribution,
job-productivity or social-interaction number to reproduce found none: the
only 2026-09-17 numbers that carry a stated band are DC-12's frame-cost
table in `tools/benchmark_baseline.json`, which this harness's frame-cost
figures are checked against for the same order of magnitude, not exact
reproduction -- `presets/colony.json` runs jobs, webs and flies that DC-12's
synthetic single-slot benchmark preset does not, so the two are not the same
measurement.

Not built here, and deliberately not: the per-spider overlay panel the plan
also asks for, showing "the top three candidate scores." No candidate-scoring
system exists yet -- that is DC-18's arbiter. Building a panel for numbers
that do not exist yet would be speculative; it belongs with DC-18, once there
is something real to show.

    python tests/metrics.py presets/colony.json --minutes 4
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from collections import Counter
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-metrics-"))

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from desktop_bug.manager import CreatureManager  # noqa: E402
from desktop_bug.profiling import FrameProfiler, set_profiler, stop_profiling  # noqa: E402

SCREEN = (1600, 900)
DT = 1.0 / 60.0
OFFSCREEN = (-100000.0, -100000.0)
SOCIAL_STATES = frozenset({"Play", "Cuddle", "Inspect"})
# A state can legitimately run long without anything being stuck; excluded
# from "longest single state" for the same reason the Phase 2 acceptance
# table (development plan) excludes them there.
LONG_RUNNING_STATES = frozenset({"Feed", "Weave"})
DEFAULT_WARMUP_SECONDS = 2.0


class SpiderMetrics:
    """Accumulated per-tick observations for one spider over one run."""

    def __init__(self, creature) -> None:
        self.index = creature.index
        self.job_id = creature.job_id
        self.personality_id = creature.personality.get("id", "?")
        self.state_frames: Counter = Counter()
        self.job_mode_frames: Counter = Counter()
        self.social_entries = 0
        self.total_frames = 0
        self._last_state: str | None = None
        self._run_state: str | None = None
        self._run_frames = 0
        self.longest_run_seconds = 0.0

    def observe(self, creature, dt: float) -> None:
        self.total_frames += 1
        state = creature.state
        self.state_frames[state] += 1
        self.job_mode_frames[getattr(creature, "job_mode", "idle")] += 1
        if state in SOCIAL_STATES and state != self._last_state:
            self.social_entries += 1
        self._last_state = state

        if state == self._run_state:
            self._run_frames += 1
        else:
            self._close_run(dt)
            self._run_state = state
            self._run_frames = 1

    def _close_run(self, dt: float) -> None:
        if self._run_state is not None and self._run_state not in LONG_RUNNING_STATES:
            self.longest_run_seconds = max(self.longest_run_seconds, self._run_frames * dt)

    def finish(self, dt: float) -> None:
        self._close_run(dt)
        self._run_state = None
        self._run_frames = 0

    def state_distribution(self) -> dict[str, float]:
        if self.total_frames == 0:
            return {}
        return {state: count / self.total_frames for state, count in self.state_frames.items()}

    def top_states(self, n: int = 3) -> list[tuple[str, float]]:
        return sorted(self.state_distribution().items(), key=lambda kv: kv[1], reverse=True)[:n]

    def job_on_duty_fraction(self) -> float | None:
        """None if this spider has no job -- "on duty" would be meaningless."""
        if self.job_id == "none" or self.total_frames == 0:
            return None
        idle = self.job_mode_frames.get("idle", 0)
        return 1.0 - idle / self.total_frames

    def social_interactions_per_minute(self, duration_seconds: float) -> float:
        if duration_seconds <= 0:
            return 0.0
        return self.social_entries / (duration_seconds / 60.0)

    def as_dict(self, duration_seconds: float) -> dict:
        return {
            "index": self.index,
            "job_id": self.job_id,
            "personality_id": self.personality_id,
            "total_frames": self.total_frames,
            "state_distribution": self.state_distribution(),
            "job_on_duty_fraction": self.job_on_duty_fraction(),
            "longest_run_seconds": self.longest_run_seconds,
            "social_interactions_per_minute": self.social_interactions_per_minute(duration_seconds),
        }


def run_metrics(preset_path: Path, minutes: float, seed: int,
                 warmup_seconds: float = DEFAULT_WARMUP_SECONDS) -> dict:
    """Run one preset headlessly and return every metric this module tracks."""
    manager = CreatureManager(preset_path, *SCREEN, seed=seed)
    warmup_frames = int(warmup_seconds / DT)
    measured_frames = max(1, int(minutes * 60.0 / DT))

    profiler = FrameProfiler(window=warmup_frames + measured_frames)
    set_profiler(profiler)
    per_spider: dict[int, SpiderMetrics] = {}
    try:
        for frame in range(warmup_frames + measured_frames):
            if frame == warmup_frames:
                # Discard the warm-up: initial target/leg-pose settling is not
                # behaviour worth measuring.
                profiler.reset()
                per_spider = {c.index: SpiderMetrics(c) for c in manager.creatures}
            profiler.begin_frame()
            manager.update(DT, *OFFSCREEN)
            profiler.end_frame()
            if frame >= warmup_frames:
                for creature in manager.creatures:
                    per_spider[creature.index].observe(creature, DT)
    finally:
        stop_profiling()

    for metrics in per_spider.values():
        metrics.finish(DT)

    duration_seconds = measured_frames * DT
    frame_mean_ms, frame_p95_ms, frame_max_ms = profiler.stats("frame")
    return {
        "preset": str(preset_path),
        "seed": seed,
        "duration_seconds": duration_seconds,
        "spiders": {index: metrics.as_dict(duration_seconds) for index, metrics in per_spider.items()},
        "bases": {key: site.to_dict() for key, site in manager.base_world.bases.items()},
        "frame_mean_ms": frame_mean_ms,
        "frame_p95_ms": frame_p95_ms,
        "frame_max_ms": frame_max_ms,
    }


def print_report(result: dict) -> None:
    print()
    print(f"preset: {result['preset']}   seed: {result['seed']}   "
          f"duration: {result['duration_seconds']:.0f}s")
    print()
    for index in sorted(result["spiders"]):
        spider = result["spiders"][index]
        top = ", ".join(f"{state} {frac * 100:.0f}%" for state, frac in
                         sorted(spider["state_distribution"].items(), key=lambda kv: kv[1], reverse=True)[:3])
        duty = spider["job_on_duty_fraction"]
        duty_text = f"{duty * 100:.0f}% on duty" if duty is not None else "no job"
        print(f"  spider {index} ({spider['personality_id']}, job={spider['job_id']}): {duty_text}")
        print(f"    top states: {top}")
        print(f"    longest single state: {spider['longest_run_seconds']:.1f}s"
              f"   social interactions/min: {spider['social_interactions_per_minute']:.2f}")

    print()
    if result["bases"]:
        for key, site in result["bases"].items():
            completion = site["build_progress"] / 500.0
            print(f"  base {key}: level {site['level']}, {completion * 100:.0f}% built, "
                  f"alert {site['alert']:.2f}")
    else:
        print("  no bases founded")

    print()
    print(f"  frame cost: mean {result['frame_mean_ms']:.2f}ms  "
          f"p95 {result['frame_p95_ms']:.2f}ms  max {result['frame_max_ms']:.2f}ms")
    print()


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("preset", type=Path, help="Preset file to run")
    parser.add_argument("--minutes", type=float, default=4.0, help="Simulated minutes to measure")
    parser.add_argument("--seed", type=int, default=20260918, help="Seed, so two runs are comparable")
    parser.add_argument("--json", type=Path, default=None, help="Also write the full result to this file")
    args = parser.parse_args(argv)

    if not args.preset.exists():
        print(f"preset not found: {args.preset}")
        return 1

    result = run_metrics(args.preset, args.minutes, args.seed)
    print_report(result)

    if args.json is not None:
        args.json.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(f"full result written to {args.json}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
