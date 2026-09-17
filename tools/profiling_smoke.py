"""Frame cost must be measurable, and the overlay must be a good guest.

Nothing in the frame loop was timed, so "ten spiders are laggy" had no cause
attached to it and every proposed fix was a guess. These checks cover the two
halves of that: the profiler and the instrumentation that feeds it, and the
policy that drops the frame rate when nothing can see the overlay anyway.

The load-bearing check is `check_manager_records_its_systems`: it runs the real
manager and asserts the sections actually appear. Testing the profiler alone
would pass happily with every `with profiler.section(...)` deleted from the
manager, which is exactly the regression worth catching.
"""

import io
import json
import os
import random
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "tools"))

from desktop_bug import frame_policy  # noqa: E402
from desktop_bug import profiling  # noqa: E402
from desktop_bug.manager import CreatureManager  # noqa: E402


def check_profiler_accumulates_and_windows() -> None:
    clock = [0.0]
    profiler = profiling.FrameProfiler(window=3, clock=lambda: clock[0])

    def spend(name: str, ms: float) -> None:
        with profiler.section(name):
            clock[0] += ms / 1000.0

    profiler.begin_frame()
    # Entered twice in one frame, as a per-creature section is: the frame's
    # cost for that system is the sum, not the last one.
    spend("creatures", 2.0)
    spend("creatures", 3.0)
    profiler.end_frame()
    mean, _p95, worst = profiler.stats("creatures")
    assert abs(mean - 5.0) < 1e-6, mean
    assert abs(worst - 5.0) < 1e-6, worst
    assert abs(profiler.stats("frame")[0] - 5.0) < 1e-6, profiler.stats("frame")

    # The window forgets: four frames into a window of three leaves the first behind.
    for ms in (10.0, 20.0, 30.0):
        profiler.begin_frame()
        spend("creatures", ms)
        profiler.end_frame()
    mean, _p95, worst = profiler.stats("creatures")
    assert abs(mean - 20.0) < 1e-6, f"the rolling window kept an expired frame: {mean}"
    assert abs(worst - 30.0) < 1e-6, worst
    assert profiler.frames == 4, profiler.frames

    assert abs(profiling.percentile([1.0, 2.0, 3.0, 4.0], 0.95) - 4.0) < 1e-6
    assert profiling.percentile([], 0.95) == 0.0


def check_painting_is_recorded_after_the_frame() -> None:
    """Qt delivers the paint event after `tick` returns, so it is not in a frame.

    Accumulating it into the open frame dict was the first attempt, and it lost
    every paint sample: the next `begin_frame` cleared the dict before anything
    had read it.
    """
    clock = [0.0]
    profiler = profiling.FrameProfiler(window=8, clock=lambda: clock[0])
    for _ in range(3):
        profiler.begin_frame()
        with profiler.section("creatures"):
            clock[0] += 0.001
        profiler.end_frame()
        # After the frame, the way a paint event really arrives.
        with profiler.section("paint"):
            clock[0] += 0.004
    mean, _p95, _worst = profiler.stats("paint")
    assert abs(mean - 4.0) < 1e-6, f"painting was not recorded: {mean}"
    # And it did not contaminate the frame total, which never contained it.
    assert abs(profiler.stats("frame")[0] - 1.0) < 1e-6, profiler.stats("frame")


def check_null_profiler_is_inert() -> None:
    profiling.stop_profiling()
    profiler = profiling.get_profiler()
    assert profiler.enabled is False
    with profiler.section("anything"):
        pass
    profiler.begin_frame()
    profiler.end_frame()
    assert profiler.table() == []
    assert profiler.hud_lines() == []
    assert profiler.stats("anything") == (0.0, 0.0, 0.0)
    # The same object every time, so an unprofiled frame allocates nothing.
    assert profiler.section("a") is profiler.section("b")


def check_env_switches() -> None:
    assert profiling.profiling_requested({}) is False
    assert profiling.profiling_requested({"DESKTOP_BUG_PROFILE": "0"}) is False
    assert profiling.profiling_requested({"DESKTOP_BUG_PROFILE": "off"}) is False
    assert profiling.profiling_requested({"DESKTOP_BUG_PROFILE": "1"}) is True
    # The HUD follows profiling and can be turned off on its own.
    assert profiling.hud_requested({"DESKTOP_BUG_PROFILE": "1"}) is True
    assert profiling.hud_requested({"DESKTOP_BUG_PROFILE": "1",
                                    "DESKTOP_BUG_PROFILE_HUD": "0"}) is False
    assert profiling.hud_requested({"DESKTOP_BUG_PROFILE_HUD": "1"}) is False


def check_hud_lines_lead_with_the_worst() -> None:
    clock = [0.0]
    profiler = profiling.FrameProfiler(window=4, clock=lambda: clock[0])
    profiler.begin_frame()
    for name, ms in (("webs", 0.5), ("render", 9.0), ("creatures", 3.0)):
        with profiler.section(name):
            clock[0] += ms / 1000.0
    profiler.end_frame()
    lines = profiler.hud_lines()
    assert lines[0].startswith("frame"), lines
    assert lines[1].startswith("render"), f"the costliest system was not first: {lines}"
    assert any(line.startswith("~fps") for line in lines), lines


def check_manager_records_its_systems() -> None:
    """The instrumentation must be in the manager, not only in the profiler."""
    random.seed(4)
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1600, 900)
    profiler = profiling.start_profiling(window=30)
    try:
        for _ in range(12):
            profiler.begin_frame()
            manager.update(1.0 / 60.0, -100000.0, -100000.0)
            profiler.end_frame()
    finally:
        profiling.stop_profiling()

    recorded = set(profiler.names())
    for name in ("frame", "creatures", "behaviour", "jobs", "webs", "mouse-webs",
                 "flies", "desktop"):
        assert name in recorded, f"the manager never timed {name}: {sorted(recorded)}"
    # Simulating four spiders has to cost something, or the clock is not running.
    assert profiler.stats("creatures")[0] > 0.0, "the creature section recorded no time"
    assert profiler.stats("frame")[0] >= profiler.stats("creatures")[0], (
        "a frame cannot be cheaper than one of the systems inside it"
    )

    # And with profiling off, the same update records nothing at all.
    before = profiler.frames
    for _ in range(4):
        manager.update(1.0 / 60.0, -100000.0, -100000.0)
    assert profiler.frames == before, "the manager recorded frames after profiling stopped"


def check_frame_rate_policy() -> None:
    decide = frame_policy.decide_fps
    assert decide(60.0) == 60.0
    assert decide(60.0, on_battery=True) == frame_policy.BATTERY_FPS
    assert decide(60.0, fullscreen=True) == frame_policy.FULLSCREEN_FPS
    # Fullscreen is the stronger signal: nothing drawn can be seen at all.
    assert decide(60.0, fullscreen=True, on_battery=True) == frame_policy.FULLSCREEN_FPS
    # Never faster than what the user asked for.
    assert decide(20.0, on_battery=True) == 20.0
    assert decide(20.0) == 20.0


def check_policy_restores_the_rate() -> None:
    state = {"fullscreen": False, "battery": False}
    policy = frame_policy.FramePolicy(
        60.0, poll_ms=100,
        fullscreen_probe=lambda hwnd=None: state["fullscreen"],
        battery_probe=lambda: state["battery"],
    )
    assert policy.poll(0.0) is None, "an unchanged machine should not restart the timer"
    assert policy.current_fps == 60.0

    state["fullscreen"] = True
    assert policy.poll(50.0) is None, "the policy polled before its interval elapsed"
    assert policy.poll(200.0) == frame_policy.FULLSCREEN_FPS
    assert "fullscreen" in policy.reason

    # Asking again with nothing changed must not churn the frame timer.
    assert policy.poll(400.0) is None

    state["fullscreen"] = False
    assert policy.poll(600.0) == 60.0, "the frame rate was not restored"
    assert policy.reason == "target"

    # A user choice is the ceiling the policy works from, not a one-off.
    state["battery"] = True
    assert policy.poll(800.0) == frame_policy.BATTERY_FPS
    assert policy.set_target_fps(20.0) == 20.0


def check_probes_are_safe_when_they_cannot_tell() -> None:
    """Both probes answer "no" rather than raising, on any platform."""
    assert frame_policy.foreground_window_is_fullscreen(exclude_hwnd=None) in (True, False)
    assert frame_policy.on_battery() in (True, False)
    # The desktop itself covers the screen permanently and is not an app.
    assert "progman" in frame_policy.DESKTOP_CLASSES
    assert "workerw" in frame_policy.DESKTOP_CLASSES


def check_overlay_times_its_frames_and_draws_the_hud() -> None:
    """The engine half: real ticks record, and the HUD really appears.

    Drawing the HUD is only half of it. The panel changes every frame, so its
    rectangle has to be added to the dirty region as well, or it is painted into
    a backing store that is never cleared underneath it and the old numbers stay
    behind. Grabbing the window is what catches that.
    """
    from PyQt5.QtWidgets import QApplication

    os.environ["DESKTOP_BUG_PROFILE"] = "1"
    os.environ["DESKTOP_BUG_PROFILE_HUD"] = "1"
    try:
        app = QApplication.instance() or QApplication(sys.argv[:1])
        from desktop_bug.engine import OverlayWindow

        random.seed(7)
        window = OverlayWindow(ROOT / "presets" / "colony.json")
        try:
            assert window.profiler.enabled, "the overlay ignored DESKTOP_BUG_PROFILE"
            assert window.show_profile_hud, "the overlay ignored DESKTOP_BUG_PROFILE_HUD"
            for _ in range(8):
                window.tick()
            recorded = set(window.profiler.names())
            for name in ("frame", "repaint-region", "creatures"):
                assert name in recorded, f"the engine never timed {name}: {sorted(recorded)}"

            window.resize(900, 600)
            image = window.grab().toImage()
            rect = window._hud_rect()
            assert rect.right() < image.width() and rect.bottom() < image.height()

            def opacity(left, top, right, bottom):
                seen = solid = 0
                for y in range(top, bottom, 3):
                    for x in range(left, right, 3):
                        seen += 1
                        if image.pixelColor(x, y).alpha() > 100:
                            solid += 1
                return solid, seen

            solid, seen = opacity(rect.left() + 2, rect.top() + 2,
                                  rect.right() - 2, rect.bottom() - 2)
            assert seen > 100 and solid == seen, (
                f"the HUD panel was not drawn: {solid} of {seen} samples were opaque"
            )
            # The overlay is transparent everywhere else, so an opaque HUD is the
            # HUD and not a background that happens to cover the whole window.
            far_solid, far_seen = opacity(image.width() - 160, image.height() - 120,
                                          image.width() - 4, image.height() - 4)
            assert far_seen > 100 and far_solid < far_seen * 0.5, (
                f"the overlay was opaque away from the HUD too: {far_solid} of {far_seen}"
            )

            # The HUD panel must be exposed for repainting every frame, or it is
            # drawn into a backing store that is never cleared beneath it.
            #
            # Two traps here. `QRegion.contains(QRect)` is true for a partial
            # overlap, so containment has to be spelled out by subtraction. And
            # the spiders' repaint padding is generous enough to cover the panel
            # by accident, which made the first version of this check pass with
            # the HUD rect removed; so they are taken out of the picture while it
            # is asked.
            from PyQt5.QtGui import QRegion

            creatures = list(window.manager.creatures)
            window.manager.creatures.clear()
            try:
                window.show_profile_hud = False
                without_hud = window._current_paint_region()
                window.show_profile_hud = True
                with_hud = window._current_paint_region()
            finally:
                window.manager.creatures.extend(creatures)
            assert QRegion(rect).subtracted(with_hud).isEmpty(), (
                "the HUD rectangle is not fully part of the dirty region, so its "
                "old numbers would stay on the backing store"
            )
            assert not QRegion(rect).subtracted(without_hud).isEmpty(), (
                "something else already covers the HUD area, so this check would "
                "pass whether or not the panel is exposed"
            )
        finally:
            window.timer.stop()
            window.style_timer.stop()
            window.deleteLater()
        del app
    finally:
        os.environ.pop("DESKTOP_BUG_PROFILE", None)
        os.environ.pop("DESKTOP_BUG_PROFILE_HUD", None)
        profiling.stop_profiling()


def check_benchmark_pads_match_the_engine() -> None:
    """The benchmark copies two engine constants; they must not drift apart."""
    import benchmark

    from desktop_bug import engine
    assert benchmark.REPAINT_PAD_PX == engine.CREATURE_REPAINT_EXTRA_PAD_PX
    assert benchmark.REPAINT_PAD_SIZE_MULT == engine.CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT


def check_benchmark_runs_and_reports() -> None:
    """End to end, small: it must produce real numbers, not an empty table."""
    result = subprocess.run(
        [sys.executable, "tools/benchmark.py", "--counts", "2", "--frames", "12",
         "--warmup", "4"],
        cwd=str(ROOT), capture_output=True, text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "where the time goes" in result.stdout, result.stdout
    assert "render" in result.stdout, result.stdout
    # A measured frame that costs nothing would mean the clock never ran.
    import re
    match = re.search(r"^\s*2\s+([0-9.]+)ms", result.stdout, re.MULTILINE)
    assert match, f"no row for the colony size that was measured:\n{result.stdout}"
    assert float(match.group(1)) > 0.0, result.stdout


def check_regression_detection() -> None:
    """The threshold has to bite, and only on the machine it was recorded for."""
    import benchmark

    def result(count, mean):
        return {"count": count, "sections": {"frame": {"mean_ms": mean, "p95_ms": mean}},
                "repaint_fraction_mean": 0.1}

    baseline = {"machine": benchmark.machine_fingerprint(),
                "runs": {"10": {"mean_ms": 10.0, "p95_ms": 12.0}}}

    out = io.StringIO()
    with redirect_stdout(out):
        code = benchmark.compare_to_baseline([result(10, 11.0)], baseline)
    assert code == 0, f"a 10% change should not fail:\n{out.getvalue()}"

    out = io.StringIO()
    with redirect_stdout(out):
        code = benchmark.compare_to_baseline([result(10, 12.0)], baseline)
    assert code == 1, f"a 20% regression should fail:\n{out.getvalue()}"
    assert "REGRESSION" in out.getvalue()

    # Another machine's numbers are not evidence about this one.
    out = io.StringIO()
    with redirect_stdout(out):
        code = benchmark.compare_to_baseline(
            [result(10, 40.0)], {"machine": "some other cpu", "runs": baseline["runs"]})
    assert code == 0, "a baseline from different hardware must not fail a run"
    assert "different hardware" in out.getvalue()


def check_committed_baseline_is_usable() -> None:
    import benchmark

    assert benchmark.BASELINE_PATH.exists(), "DC-12 is meant to commit a baseline"
    data = json.loads(benchmark.BASELINE_PATH.read_text(encoding="utf-8"))
    assert data.get("machine"), data
    # A hostname is personal and this file is committed; the fingerprint is the
    # hardware the numbers depend on and nothing else.
    assert "|" in data["machine"] and "cores" in data["machine"], data["machine"]
    runs = data.get("runs", {})
    for count in ("1", "5", "10", "20"):
        assert count in runs, f"the baseline is missing the {count}-spider run"
        assert float(runs[count]["mean_ms"]) > 0.0, runs[count]


def main() -> int:
    check_profiler_accumulates_and_windows()
    check_painting_is_recorded_after_the_frame()
    check_null_profiler_is_inert()
    check_env_switches()
    check_hud_lines_lead_with_the_worst()
    check_manager_records_its_systems()
    check_frame_rate_policy()
    check_policy_restores_the_rate()
    check_probes_are_safe_when_they_cannot_tell()
    check_overlay_times_its_frames_and_draws_the_hud()
    check_benchmark_pads_match_the_engine()
    check_benchmark_runs_and_reports()
    check_regression_detection()
    check_committed_baseline_is_usable()
    print("profiling smoke: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
