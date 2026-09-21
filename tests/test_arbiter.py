"""Headless checks for the single utility arbiter (DC-18, C1/C6).

Four layers used to independently decide a creature's activity: hard-coded
personality-id branches in `_update_state`, the probability chains in
`_consider_special_actions`, `BehaviourPhaseScheduler` trying itself first,
and `CreatureManager._drive_hunt` overwriting a creature's state from
outside. This module checks all four are actually gone (not merely renamed),
that the replacement arbiter runs at a throttled rate rather than every
frame, that job duty still outranks it, and that a job state can no longer
be left frozen by a future job author forgetting an explicit hand-back call
(C6) -- proved here by reintroducing the id-gated version of the old code
and confirming this file's own test then fails.
"""

from __future__ import annotations

import json
import random

import pytest

from desktop_bug.creature import arbiter
from desktop_bug.creature import Creature, JOB_STATES
from desktop_bug.creature.constants import JOB_MODE_STATES
from desktop_bug.manager import CreatureManager
from support import ROOT

BEHAVIOUR_SRC = (ROOT / "src" / "desktop_bug" / "creature" / "behaviour.py").read_text(encoding="utf-8")
# The manager is a package of mixins since DC-43, so "not in the manager"
# means not in any of its modules.
MANAGER_SRC = "\n".join(
    path.read_text(encoding="utf-8")
    for path in sorted((ROOT / "src" / "desktop_bug" / "manager").glob("*.py"))
)

DT = 1.0 / 60.0
SCREEN = (1600, 900)
AWAY = (-100000.0, -100000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a Creature/CreatureManager constructs Qt-backed sprite state."""


def build_guard(job_id: str = "guard") -> Creature:
    model = json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text(encoding="utf-8"))
    personality = json.loads((ROOT / "personalities" / "curious.json").read_text(encoding="utf-8"))
    return Creature(model, personality, *SCREEN, index=0, job_id=job_id)


def build_playful(seed: int = 1) -> Creature:
    random.seed(seed)
    model = json.loads((ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    traits = json.loads((ROOT / "personalities" / "playful.json").read_text(encoding="utf-8"))
    creature = Creature(model, traits, *SCREEN, index=0, progression_id=f"arbiter:{seed}")
    creature.x, creature.y = 800.0, 450.0
    creature._initialize_legs()
    for _ in range(60):
        creature.update(DT, *AWAY, *SCREEN)
    return creature


# ---------------------------------------------------------------------------
# The four old decision paths are actually gone, not merely renamed.
# ---------------------------------------------------------------------------

def test_consider_special_actions_is_deleted():
    assert "def _consider_special_actions(" not in BEHAVIOUR_SRC


def test_activate_scheduled_phase_is_deleted():
    """The phase scheduler no longer tries itself first and wins outright."""
    assert "def _activate_scheduled_phase(" not in BEHAVIOUR_SRC


def test_drive_hunt_is_gone_from_the_manager():
    assert "def _drive_hunt(" not in MANAGER_SRC
    assert not hasattr(CreatureManager, "_drive_hunt")


def test_behaviour_now_defers_decisions_to_the_arbiter():
    assert "arbiter.decide(" in BEHAVIOUR_SRC


# ---------------------------------------------------------------------------
# Prey candidate: the manager publishes, the creature decides (C1).
# ---------------------------------------------------------------------------

def test_pursue_prey_replaces_the_manual_drive_hunt_and_still_catches_flies():
    """Hunting must still work end to end after the outside-in overwrite is gone."""
    assert hasattr(Creature, "_pursue_prey")
    manager = CreatureManager(ROOT / "presets" / "colony.json", *SCREEN, seed=3)
    manager.base_world.clear()
    caught = 0
    seen_ids = set()
    # Cursor parked off screen: only hunting can drive engagement here.
    for _ in range(60 * 90):
        manager.update(DT, *AWAY)
        for fly in manager.fly_world.flies:
            if fly.eaten and id(fly) not in seen_ids:
                seen_ids.add(id(fly))
                caught += 1
    assert caught > 0, "no fly was caught in 90 simulated seconds without the manager driving state directly"


def test_perception_publishes_the_prey_candidate():
    manager = CreatureManager(ROOT / "presets" / "colony.json", *SCREEN, seed=3)
    creature = manager.creatures[0]
    creature.update(DT, *AWAY, *SCREEN)  # perception is built lazily by update()
    assert creature.perception.prey() is None
    assert creature.perception.is_hunting_prey is False
    # Flies spawn over time rather than existing from frame one; run the
    # manager forward (off-screen cursor) until its nest has produced one.
    for _ in range(60 * 20):
        if manager.fly_world.flies:
            break
        manager.update(DT, *AWAY)
    fly = manager.fly_world.flies[0] if manager.fly_world.flies else None
    assert fly is not None, "colony.json spawned no flies within 20 simulated seconds"
    creature._prey = fly
    creature._hunting_prey = True
    creature.update(DT, fly.x, fly.y, *SCREEN)
    assert creature.perception.prey() is fly
    assert creature.perception.is_hunting_prey is True


# ---------------------------------------------------------------------------
# Throttle: arbiter decisions run at 5-10 Hz, not every frame.
# ---------------------------------------------------------------------------

def test_arbiter_runs_at_a_throttled_rate_not_every_frame(monkeypatch):
    creature = build_playful(seed=11)
    calls = []
    original = arbiter.decide

    def counting(*args, **kwargs):
        calls.append(1)
        return original(*args, **kwargs)

    monkeypatch.setattr(arbiter, "decide", counting)
    frames = 300  # 5 simulated seconds
    for _ in range(frames):
        creature.update(DT, *AWAY, *SCREEN)
    # 5-10 Hz over 5s is 25-50 calls; well under one per frame (300).
    assert 0 < len(calls) < frames / 4, (
        f"{len(calls)} arbiter decisions over {frames} frames is not throttled"
    )


# ---------------------------------------------------------------------------
# Job duty still outranks the arbiter: a working spider is not overridden.
# ---------------------------------------------------------------------------

def test_job_duty_outranks_the_arbiter():
    guard = build_guard()
    guard.job_mode = "patrol"
    guard.job_target = (600.0, 400.0)
    guard.state = "Idle"
    guard.decision_timer = 0.0  # would otherwise be ready to run the arbiter
    guard.state_timer = 0.0
    for _ in range(30):
        guard.update(DT, *AWAY, *SCREEN)
        assert guard.state == "JobPatrol", (
            "the arbiter must never run while a job is actively claiming the tick"
        )


# ---------------------------------------------------------------------------
# Job-mode table: first-class job states (C6).
# ---------------------------------------------------------------------------

def test_job_mode_states_table_covers_every_job_state():
    assert set(JOB_MODE_STATES.values()) == set(JOB_STATES)


def test_unrecognized_job_id_still_hands_back_a_leftover_job_state():
    """DC-18, C6: `_update_job_state` no longer gates on `job_id` at all.

    Before this package, hand-back only happened after a check gating on
    `job_id in ("builder", "guard")`. A future job (say, a Scout with its own
    roam state) that used any other id would return *before* ever reaching
    the hand-back call, leaving a leftover job state frozen -- speed and
    motion_paused held at whatever the job last set them to. Proved by
    reverting: temporarily reintroducing that id gate here makes this test
    fail (see the PR description for the before/after run).
    """
    guard = build_guard(job_id="some-future-job-id")
    guard.state = "JobPatrol"
    guard.motion_paused = False
    guard.speed = 40.0
    guard.job_mode = "idle"
    guard.job_target = None
    assert guard._update_job_state(DT, 0.0, 0.0) is False
    assert guard.state == "Idle"
    assert guard.motion_paused is False


# ---------------------------------------------------------------------------
# Idle must not re-roll its own beat every time the arbiter runs.
# ---------------------------------------------------------------------------

def test_idle_does_not_re_roll_its_timer_before_it_expires():
    """The arbiter runs at 5-10 Hz, far more often than one Idle beat (1-3s).

    "idle" is always a candidate so `decide()` never returns nothing; if
    winning by default re-entered Idle (rolling a fresh 1-3s timer) every
    single time, no creature would ever leave Idle on its own, because the
    timer would never survive long enough to actually expire. Proved by
    reverting: see `arbiter._idle_execute`'s guard, without which this fails.
    """
    creature = build_playful(seed=5)
    seen_states = set()
    for _ in range(600):  # 10 simulated seconds, several idle beats' worth
        creature.update(DT, *AWAY, *SCREEN)
        seen_states.add(creature.state)
    assert seen_states != {"Idle"}, "creature never left Idle in 10 simulated seconds"


def test_unrecognized_job_id_can_still_do_its_job_when_mode_matches_the_table():
    """The mirror case: a non-builder/guard id whose mode *is* in the table
    still gets its state entered -- job dispatch is keyed on `job_mode`, not
    on a hard-coded id allowlist.
    """
    guard = build_guard(job_id="some-future-job-id")
    guard.state = "Idle"
    guard.job_mode = "patrol"
    guard.job_target = (500.0, 500.0)
    assert guard._update_job_state(DT, 0.0, 0.0) is True
    assert guard.state == "JobPatrol"
