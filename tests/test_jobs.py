"""Headless checks for separate job roles and shared colony bases."""

from types import SimpleNamespace
import random

import pytest


from desktop_bug.jobs import (
    BUILD_DUTY_OFF,
    BUILD_DUTY_ON,
    PATROL_DUTY_OFF,
    PATROL_DUTY_ON,
    MAX_BUILD_PROGRESS,
    BaseWorld,
    JOB_IDS,
    normalize_job_id,
)


class DummySpider:
    def __init__(self, job, team, x, y, ident):
        self.job_id = job
        self.x = x
        self.y = y
        self.index = ident
        self.progression_id = f"test:{ident}"
        self.progression = SimpleNamespace(team_id=team)
        self.dragging = False
        self.airborne = False
        self.job_busy = False
        self.level = 1

    def relation_to(self, other):
        return "foe" if other.progression.team_id == "rivals" else ("friend" if self.progression.team_id == other.progression.team_id else "neutral")


DT = 1.0 / 60.0


@pytest.fixture
def colony():
    """A builder and a guard of one team, and an intruder, after one frame."""
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    guard = DummySpider("guard", "pack_a", 230, 200, 2)
    foe = DummySpider("none", "rivals", 246, 200, 3)
    world = BaseWorld(800, 600, rng=random.Random(4))
    world.update(DT, [builder, guard, foe])
    return world, next(iter(world.bases.values())), builder, guard, foe


def test_the_jobs_are_the_six_a_spider_can_hold():
    assert JOB_IDS == ("none", "hunter", "builder", "guard", "scout", "webber")
    assert normalize_job_id("weaver") == "webber"


def test_a_builder_founds_a_base_and_a_guard_answers_the_intruder(colony):
    world, site, builder, guard, _foe = colony
    assert len(world.bases) == 1
    assert builder.job_base_id == site.id
    assert builder.job_mode == "build"
    assert guard.job_mode == "guard_alert"
    assert site.alert > 0.0


def test_a_world_survives_a_round_trip(colony):
    world, _site, _builder, _guard, _foe = colony
    saved = world.to_dict()
    assert BaseWorld(800, 600, saved).to_dict() == saved


def test_a_base_under_construction_reports_partial_completion(colony):
    """Not a full ring the moment its first structure level lands."""
    _world, site, _builder, _guard, _foe = colony
    assert 0.0 < site.completion < 0.25, site.completion


def test_work_is_a_shift_not_ownership(colony):
    """A builder hands itself back to its temperament during the long build."""
    world, _site, builder, guard, foe = colony
    modes = []
    for _ in range(int(60 * (BUILD_DUTY_ON[1] + BUILD_DUTY_OFF[1] + 2.0))):
        world.update(DT, [builder, guard, foe])
        modes.append(builder.job_mode)
    assert "build" in modes, "builder never worked"
    assert "idle" in modes, "builder never came off duty"
    assert 60 * BUILD_DUTY_OFF[0] <= modes.count("idle"), modes.count("idle")


def test_a_busy_builder_earns_its_base_nothing(colony):
    """Fleeing, eating or mid-jump reports job_busy; progress must not accrue."""
    world, site, builder, guard, foe = colony
    builder.job_busy = True
    busy_progress = site.build_progress
    for _ in range(600):
        world.update(DT, [builder, guard, foe])
        assert builder.job_mode != "build", "a busy builder still claimed build work"
    assert site.build_progress == busy_progress, site.build_progress


@pytest.mark.slow
def test_a_finished_base_releases_its_builder(colony):
    world, site, builder, guard, foe = colony
    for _ in range(60 * 600):
        world.update(DT, [builder, guard, foe])
        if site.build_progress >= MAX_BUILD_PROGRESS and site.integrity >= site.max_integrity:
            break
    assert site.build_progress == MAX_BUILD_PROGRESS, site.build_progress
    assert site.integrity == site.max_integrity, (site.integrity, site.max_integrity)
    assert site.completion == 1.0, site.completion

    # Released for good, rather than pinned motionless on the completed site.
    for _ in range(int(60 * (BUILD_DUTY_ON[1] + BUILD_DUTY_OFF[1] + 2.0))):
        world.update(DT, [builder, guard, foe])
        assert builder.job_mode == "idle", builder.job_mode
    assert builder.job_target is None

    # Serious damage is an emergency: it cancels the break immediately rather
    # than waiting out an off-duty stretch.
    site.integrity = site.max_integrity * 0.4
    world.update(DT, [builder, guard, foe])
    assert builder.job_mode in ("build", "build_travel"), builder.job_mode
    assert world.duty_state(builder).on_duty


def test_the_patrol_ring_keeps_turning_while_the_guard_rests(colony):
    """So the guard rejoins the route where the route now is."""
    world, site, builder, guard, foe = colony
    foe.x, foe.y = 780.0, 580.0
    modes, angles = [], []
    for _ in range(int(60 * (PATROL_DUTY_ON[1] + PATROL_DUTY_OFF[1] + 2.0))):
        world.update(DT, [builder, guard, foe])
        modes.append(guard.job_mode)
        if guard.job_mode == "idle":
            angles.append(site.patrol_angle)
    assert "patrol" in modes, "guard never patrolled"
    assert "idle" in modes, "guard never came off duty"
    assert len(set(angles)) > 1, "patrol ring stalled while the guard was off duty"


def test_an_intruder_ends_the_break_on_the_frame_it_appears(colony):
    world, site, builder, guard, foe = colony
    foe.x, foe.y = 780.0, 580.0
    for _ in range(int(60 * (PATROL_DUTY_ON[1] + PATROL_DUTY_OFF[1] + 2.0))):
        world.update(DT, [builder, guard, foe])

    foe.x, foe.y = site.x + 20.0, site.y
    world.update(DT, [builder, guard, foe])
    assert guard.job_mode == "guard_alert", guard.job_mode
    assert world.duty_state(guard).on_duty


def test_a_busy_guard_is_not_dragged_back_onto_patrol(colony):
    world, site, builder, guard, foe = colony
    foe.x, foe.y = site.x + 20.0, site.y
    guard.job_busy = True
    world.update(DT, [builder, guard, foe])
    assert guard.job_mode == "idle", guard.job_mode
