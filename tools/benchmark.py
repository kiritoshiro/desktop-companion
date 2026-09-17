"""Measure what a frame actually costs, at several colony sizes.

Ten spiders make the overlay stutter, and until now nothing in the loop was
timed, so every explanation for that was a guess. This runs the real manager
headless -- the same update and the same render, through a real QPainter -- and
reports mean and 95th-percentile frame time per system for 1, 5, 10 and 20
spiders.

Two things it deliberately does *not* do. It does not run the Qt event loop, so
compositing the transparent overlay onto the desktop is not included; that cost
belongs to Windows and cannot be measured from in here. And it does not move a
pointer around, so pointer-driven behaviour stays out of the numbers.

What it does model is the partial repaint: it rebuilds the same dirty region the
engine does, with the same padding, clips to it, and reports how much of the
screen that region covers. That fraction is itself evidence -- it is the
difference between "rendering is expensive" and "we are asking for too many
pixels".

    python tools/benchmark.py                      # the standard sweep
    python tools/benchmark.py --check              # and fail on a regression
    python tools/benchmark.py --update-baseline    # record a new baseline
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import random
import statistics
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-bench-"))

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtCore import QRect  # noqa: E402
from PyQt5.QtGui import QGuiApplication, QPainter, QPixmap, QRegion  # noqa: E402

from desktop_bug.manager import CreatureManager  # noqa: E402
from desktop_bug.profiling import FrameProfiler, percentile, set_profiler, stop_profiling  # noqa: E402

BASELINE_PATH = ROOT / "tools" / "benchmark_baseline.json"
DEFAULT_COUNTS = (1, 5, 10, 20)
DEFAULT_FRAMES = 300
DEFAULT_WARMUP = 60
SCREEN = (2560, 1440)
DT = 1.0 / 60.0
# The plan's acceptance threshold: a change that costs more than this against a
# recorded baseline is a regression and the benchmark says so.
REGRESSION_TOLERANCE = 0.15
# Mirrors engine.CREATURE_REPAINT_EXTRA_PAD_PX / _SIZE_MULT. Imported values
# would be better, but engine.py pulls in the whole Qt widget stack and a tray
# icon; these two numbers are asserted equal in tools/profiling_smoke.py.
REPAINT_PAD_PX = 28
REPAINT_PAD_SIZE_MULT = 1.9


def machine_fingerprint() -> str:
    """Identify the hardware closely enough to know a baseline applies here.

    Deliberately not the hostname: a baseline is committed to the repository and
    a machine name is personal, while the processor and core count are what the
    numbers actually depend on.
    """
    return "|".join((
        platform.machine(),
        platform.processor() or "unknown-cpu",
        f"{os.cpu_count() or 0} cores",
        f"py{sys.version_info.major}.{sys.version_info.minor}",
    ))


def preset_for(count: int) -> dict:
    """A colony of `count` spiders, identical every run so runs compare.

    One slot with a count, rather than several: a benchmark should vary the one
    thing it is measuring, and mixing models would make a change in the numbers
    ambiguous between "slower" and "different spiders".
    """
    return {
        "name": "Benchmark",
        "slots": [{
            "model": "tarantula",
            "personality": "balanced",
            "count": int(count),
            "team": "pack_a",
            "job": "none",
        }],
        "settings": {
            "size_scale": 1.0,
            "interferable": True,
            "mood_mode": "auto",
            "social_play": True,
            "gait_style": "lively",
            "flies": {"enabled": True, "min_interval": 4.0, "max_interval": 9.0,
                      "max_flies": 6, "spawner_enabled": True},
        },
    }


def build_manager(count: int, seed: int) -> CreatureManager:
    random.seed(seed)
    manager = CreatureManager(ROOT / "presets" / "default.json", *SCREEN)
    manager.reload_from_preset_data(preset_for(count))
    # Spread them out so they are not all resolving the same collisions on
    # frame one, which would flatter the small colonies.
    for index, creature in enumerate(manager.creatures):
        angle = (index / max(1, len(manager.creatures))) * math.tau
        creature.x = SCREEN[0] * 0.5 + math.cos(angle) * SCREEN[1] * 0.3
        creature.y = SCREEN[1] * 0.5 + math.sin(angle) * SCREEN[1] * 0.3
    return manager


def dirty_region(manager: CreatureManager) -> QRegion:
    """The same repaint region the engine builds, so render cost is realistic."""
    region = QRegion()
    for creature in manager.creatures:
        x0, y0, x1, y1 = creature.bounding_rect(manager.always_show_names)
        pad = int(max(REPAINT_PAD_PX, creature.size * REPAINT_PAD_SIZE_MULT))
        rect = QRect(int(math.floor(x0)) - pad, int(math.floor(y0)) - pad,
                     int(math.ceil(x1 - x0)) + 1 + pad * 2,
                     int(math.ceil(y1 - y0)) + 1 + pad * 2)
        region += rect
    for world_name in ("web_world", "mouse_web_world", "fly_world"):
        world = getattr(manager, world_name, None)
        if world is None:
            continue
        for x, y, w, h in world.dirty_rects():
            region += QRect(int(math.floor(x)), int(math.floor(y)),
                            int(math.ceil(w)) + 1, int(math.ceil(h)) + 1)
    return region


def region_area(region: QRegion) -> int:
    try:
        rects = region.rects()
    except Exception:
        rects = []
    return sum(r.width() * r.height() for r in rects)


def run_one(count: int, frames: int, warmup: int, seed: int) -> dict:
    """Run one colony size and return its timings."""
    manager = build_manager(count, seed)
    canvas = QPixmap(*SCREEN)

    profiler = FrameProfiler(window=max(frames, 1))
    set_profiler(profiler)
    coverage = []
    try:
        for frame in range(warmup + frames):
            if frame == warmup:
                # Discard the warm-up: the first frames build leg poses, pick
                # targets and populate caches, and are not what a running
                # overlay costs.
                profiler.reset()
                coverage.clear()
            profiler.begin_frame()
            # The pointer sits off screen, so nothing reacts to it and the
            # numbers describe a colony left to its own devices.
            manager.update(DT, -100000.0, -100000.0)
            region = dirty_region(manager)
            coverage.append(region_area(region))
            painter = QPainter(canvas)
            painter.setRenderHint(QPainter.Antialiasing, True)
            painter.setClipRegion(region)
            manager.render(painter)
            painter.end()
            profiler.end_frame()
    finally:
        stop_profiling()

    screen_px = float(SCREEN[0] * SCREEN[1])
    return {
        "count": count,
        "frames": profiler.frames,
        "sections": {name: dict(zip(("mean_ms", "p95_ms", "max_ms"), profiler.stats(name)))
                     for name in profiler.names()},
        "repaint_fraction_mean": (statistics.fmean(coverage) / screen_px) if coverage else 0.0,
        "repaint_fraction_p95": (percentile(coverage, 0.95) / screen_px) if coverage else 0.0,
    }


def print_report(results: list[dict]) -> None:
    print()
    print(f"machine: {machine_fingerprint()}")
    print(f"screen:  {SCREEN[0]}x{SCREEN[1]}   budget at 60 FPS: 16.7 ms/frame")
    print()
    header = f"{'spiders':>7}  {'frame mean':>10}  {'frame p95':>10}  {'worst':>8}  {'repaint':>8}  {'headroom':>9}"
    print(header)
    print("-" * len(header))
    for result in results:
        frame = result["sections"].get("frame", {})
        mean = frame.get("mean_ms", 0.0)
        budget = 1000.0 / 60.0
        print(f"{result['count']:>7}  {mean:>9.2f}ms  {frame.get('p95_ms', 0.0):>9.2f}ms  "
              f"{frame.get('max_ms', 0.0):>7.1f}ms  {result['repaint_fraction_mean'] * 100:>7.1f}%  "
              f"{(budget - mean) / budget * 100:>8.0f}%")

    print()
    print("where the time goes (mean ms per frame)")
    names = []
    for result in results:
        for name in result["sections"]:
            if name != "frame" and name not in names:
                names.append(name)
    # Costliest first at the largest colony, which is the size that hurts.
    largest = results[-1]["sections"] if results else {}
    names.sort(key=lambda n: largest.get(n, {}).get("mean_ms", 0.0), reverse=True)
    counts = "".join(f"{result['count']:>9}" for result in results)
    print(f"{'section':<16}{counts}")
    print("-" * (16 + 9 * len(results)))
    for name in names:
        row = "".join(f"{result['sections'].get(name, {}).get('mean_ms', 0.0):>9.2f}"
                      for result in results)
        print(f"{name:<16}{row}")
    print()


def compare_to_baseline(results: list[dict], baseline: dict) -> int:
    """Report regressions against a recorded baseline; return an exit code."""
    recorded = baseline.get("machine")
    here = machine_fingerprint()
    if recorded != here:
        print("baseline was recorded on different hardware, so the numbers are not")
        print("comparable and nothing is being failed:")
        print(f"  baseline: {recorded}")
        print(f"  here:     {here}")
        print("Record one for this machine with --update-baseline.")
        return 0

    runs = baseline.get("runs", {})
    regressions = []
    for result in results:
        previous = runs.get(str(result["count"]))
        if not previous:
            print(f"  {result['count']:>3} spiders: no baseline entry, skipped")
            continue
        was = float(previous.get("mean_ms", 0.0))
        now = float(result["sections"].get("frame", {}).get("mean_ms", 0.0))
        if was <= 0.0:
            continue
        change = (now - was) / was
        verdict = "ok"
        if change > REGRESSION_TOLERANCE:
            verdict = "REGRESSION"
            regressions.append((result["count"], was, now, change))
        elif change < -REGRESSION_TOLERANCE:
            verdict = "faster"
        print(f"  {result['count']:>3} spiders: {was:6.2f}ms -> {now:6.2f}ms  "
              f"{change * 100:+6.1f}%  {verdict}")

    if regressions:
        print()
        print(f"FAIL  {len(regressions)} colony size(s) got more than "
              f"{REGRESSION_TOLERANCE * 100:.0f}% slower than the baseline.")
        return 1
    print()
    print(f"PASS  no colony size is more than {REGRESSION_TOLERANCE * 100:.0f}% "
          "slower than the baseline.")
    return 0


def write_baseline(results: list[dict], path: Path, note: str) -> None:
    payload = {
        "machine": machine_fingerprint(),
        "note": note,
        "screen": list(SCREEN),
        "runs": {
            str(result["count"]): {
                "mean_ms": round(result["sections"].get("frame", {}).get("mean_ms", 0.0), 3),
                "p95_ms": round(result["sections"].get("frame", {}).get("p95_ms", 0.0), 3),
                "repaint_fraction_mean": round(result["repaint_fraction_mean"], 4),
            }
            for result in results
        },
    }
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"baseline written to {path.relative_to(ROOT)}")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--counts", default=",".join(str(c) for c in DEFAULT_COUNTS),
                        help="Colony sizes to measure, comma separated")
    parser.add_argument("--frames", type=int, default=DEFAULT_FRAMES,
                        help="Measured frames per colony size")
    parser.add_argument("--warmup", type=int, default=DEFAULT_WARMUP,
                        help="Frames to run and discard before measuring")
    parser.add_argument("--seed", type=int, default=20260917,
                        help="Seed, so two runs are comparable")
    parser.add_argument("--json", type=Path, default=None,
                        help="Also write the full result to this file")
    parser.add_argument("--check", action="store_true",
                        help="Fail if a colony size regressed against the baseline")
    parser.add_argument("--update-baseline", action="store_true",
                        help="Record these numbers as the baseline for this machine")
    parser.add_argument("--note", default="", help="Note stored with a new baseline")
    args = parser.parse_args(argv)

    counts = [int(part) for part in str(args.counts).split(",") if part.strip()]
    if not counts:
        print("no colony sizes were given")
        return 1

    app = QGuiApplication.instance() or QGuiApplication(sys.argv[:1])
    results = []
    for count in counts:
        print(f"measuring {count} spider(s) over {args.frames} frames...", flush=True)
        results.append(run_one(count, args.frames, args.warmup, args.seed))
    print_report(results)
    del app

    if args.json is not None:
        args.json.write_text(json.dumps(
            {"machine": machine_fingerprint(), "results": results}, indent=2) + "\n",
            encoding="utf-8")
        print(f"full result written to {args.json}")

    if args.update_baseline:
        write_baseline(results, BASELINE_PATH, args.note or "recorded by tools/benchmark.py")
        return 0

    if args.check:
        if not BASELINE_PATH.exists():
            print("no baseline exists yet; record one with --update-baseline")
            return 0
        try:
            baseline = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            print(f"the baseline could not be read: {exc}")
            return 1
        return compare_to_baseline(results, baseline)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
