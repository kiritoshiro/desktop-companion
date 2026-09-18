"""A job is a shift, not ownership of the spider.

Checks that Builder/Guard work hands control back: that an urgent personality
state outranks work, that a released worker returns to its own state machine
instead of freezing in a job-only state, and that a colony still reads as a set
of spiders rather than a set of workers.
"""

import collections
import json
import random
import tempfile
from pathlib import Path

# Keep a test run from rewriting a real player's saved spiders.


from desktop_bug.creature import JOB_PREEMPTING_STATES, JOB_STATES, Creature
from desktop_bug.manager import CreatureManager
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """


def build_guard():
    model = json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text(encoding="utf-8"))
    personality = json.loads((ROOT / "personalities" / "curious.json").read_text(encoding="utf-8"))
    return Creature(model, personality, 1200, 800, index=0, job_id="guard")


def test_preemption() -> None:
    dt = 1.0 / 60.0
    guard = build_guard()
    guard.job_mode = "patrol"
    guard.job_target = (600.0, 400.0)

    # Ordinary case: the shift owns the spider.
    guard.state = "Idle"
    assert guard._update_job_state(dt, 0.0, 0.0) is True
    assert guard.state == "JobPatrol"
    assert guard.job_busy is False

    # Every preempting state must keep control and be left untouched.
    for state in sorted(JOB_PREEMPTING_STATES):
        guard.state = state
        assert guard._update_job_state(dt, 0.0, 0.0) is False, state
        assert guard.state == state, state
        assert guard.job_busy is True, state

    # Prey outranks patrol, but not a guard answering an intruder at its base.
    guard.state = "Idle"
    guard._hunting_prey = True
    assert guard._update_job_state(dt, 0.0, 0.0) is False
    assert guard.job_busy is True
    guard.job_mode = "guard_alert"
    assert guard._update_job_state(dt, 0.0, 0.0) is True
    assert guard.state == "JobGuardAlert"
    assert guard.job_busy is False
    guard._hunting_prey = False

    # ...but nothing outranks fleeing or a jump already in the air, not even an
    # intruder standing on the guard's own base.
    for state in ("Retreat", "Startled", "Jump", "Land"):
        guard.state = state
        assert guard._update_job_state(dt, 0.0, 0.0) is False, state
        assert guard.state == state, state


def test_release() -> None:
    """A job state has no branch in the personality dispatch.

    A spider left in one after its work intent clears matched nothing, kept its
    last speed and ``motion_paused`` flag, and stayed frozen until some unrelated
    reflex happened to move it.
    """
    dt = 1.0 / 60.0
    for state, paused in (("JobBuild", True), ("JobPatrol", False), ("JobTravel", False), ("JobGuardAlert", False)):
        guard = build_guard()
        guard.state = state
        guard.motion_paused = paused
        guard.speed = 40.0
        guard.job_mode = "idle"
        guard.job_target = None
        assert guard._update_job_state(dt, 0.0, 0.0) is False, state
        assert guard.state not in JOB_STATES, (state, guard.state)
        assert guard.state == "Idle", (state, guard.state)
        assert guard.motion_paused is False, state


def test_colony_behaviour(monkeypatch) -> int:
    """A working colony must still behave like spiders over a long run."""
    # DC-21: build progress now depends on the team's banked food (state that
    # persists across runs, per ``BaseSite``), so this test must not read or
    # write whatever real save happens to sit at the machine's actual
    # ``DESKTOP_BUG_STATE_DIR`` -- a leftover base from an earlier real launch
    # (or an earlier test run) would make the ``build_progress`` assertion
    # below pass or fail depending on unrelated history instead of on this
    # run. A private directory via tempfile.mkdtemp(), like
    # test_creature_render_golden.py already does for the same reason
    # (pytest's own tmp_path root has intermittently PermissionError'd on
    # this machine when several worktrees run pytest concurrently), makes
    # this deterministic and independent of that lock.
    tmp_path = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(tmp_path / "state"))
    random.seed(5)
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1600, 900)
    manager.base_world.clear()
    watched = {creature.job_id: creature for creature in manager.creatures}
    assert "builder" in watched and "guard" in watched, sorted(watched)
    seen = {job: collections.Counter() for job in watched}

    dt = 1.0 / 60.0
    # The cursor is parked far off screen: nothing here may depend on the mouse
    # reflex accidentally shaking a frozen spider loose.
    for _ in range(60 * 240):
        manager.update(dt, -5000.0, -5000.0)
        for job, creature in watched.items():
            seen[job][creature.state] += 1

    for job in ("builder", "guard"):
        counts = seen[job]
        total = sum(counts.values())
        job_frames = sum(count for state, count in counts.items() if state in JOB_STATES)
        own_frames = total - job_frames
        assert job_frames > total * 0.05, (job, job_frames, total)
        assert own_frames > total * 0.25, (job, own_frames, total)
        # Off-shift time must be spent doing something, not held in one state.
        off_states = {state for state in counts if state not in JOB_STATES}
        assert len(off_states) >= 4, (job, sorted(off_states))

    site = next(iter(manager.base_world.bases.values()))
    # DC-21: build progress spends the team's banked food now, and
    # `colony.json` deliberately puts its Hunter on a base-less rival team
    # (see this file's own module docstring context and DC-20's notes), so
    # `pack_a` has no dedicated food-runner. This still passes because DC-21
    # widened crediting: the Builder/Guard/Scout/Webber occasionally eat a
    # wandering fly themselves (`FLY_CATCH_RESOURCE_AMOUNT`), which is enough
    # over four minutes at the preset's default fly rate to fund some
    # progress, just slower than a colony with its own Hunter would see.
    assert site.build_progress > 0.0, site.build_progress
    builder_job = sum(c for s, c in seen["builder"].items() if s in JOB_STATES)
    guard_job = sum(c for s, c in seen["guard"].items() if s in JOB_STATES)
    builder_total = sum(seen["builder"].values())
    print(
        f"job-blend-smoke: builder_on_job={builder_job * 100 // builder_total}% "
        f"guard_on_job={guard_job * 100 // sum(seen['guard'].values())}% "
        f"base_progress={site.build_progress:.0f} states={len(seen['builder'])}"
    )
