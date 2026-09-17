"""Headless checks for separate job roles and shared colony bases."""

from pathlib import Path
from types import SimpleNamespace
import random
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

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


def main() -> int:
    assert JOB_IDS == ("none", "hunter", "builder", "guard", "scout", "webber")
    assert normalize_job_id("weaver") == "webber"
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    guard = DummySpider("guard", "pack_a", 230, 200, 2)
    foe = DummySpider("none", "rivals", 246, 200, 3)
    world = BaseWorld(800, 600, rng=random.Random(4))
    world.update(1.0 / 60.0, [builder, guard, foe])
    assert len(world.bases) == 1
    site = next(iter(world.bases.values()))
    assert builder.job_base_id == site.id
    assert builder.job_mode == "build"
    assert guard.job_mode == "guard_alert"
    assert site.alert > 0.0
    saved = world.to_dict()
    restored = BaseWorld(800, 600, saved)
    assert restored.to_dict() == saved

    # A base under construction reports partial completion, not a full ring the
    # moment its first structure level lands.
    assert 0.0 < site.completion < 0.25, site.completion

    # Work is a shift, not ownership. The builder must hand itself back to its
    # temperament periodically instead of standing on the site for the whole
    # roughly-80-second build.
    dt = 1.0 / 60.0
    modes = []
    for _ in range(int(60 * (BUILD_DUTY_ON[1] + BUILD_DUTY_OFF[1] + 2.0))):
        world.update(dt, [builder, guard, foe])
        modes.append(builder.job_mode)
    assert "build" in modes, "builder never worked"
    assert "idle" in modes, "builder never came off duty"
    off_duty_frames = modes.count("idle")
    assert 60 * BUILD_DUTY_OFF[0] <= off_duty_frames, off_duty_frames

    # A spider that is fleeing, eating or mid-jump reports job_busy, and its
    # base must not gain progress from work it is not doing.
    builder.job_busy = True
    busy_progress = site.build_progress
    for _ in range(600):
        world.update(dt, [builder, guard, foe])
        assert builder.job_mode != "build", "a busy builder still claimed build work"
    assert site.build_progress == busy_progress, site.build_progress
    builder.job_busy = False

    # Over a long run the base still finishes, and then the builder is released
    # for good rather than pinned motionless on the completed site.
    for _ in range(60 * 600):
        world.update(dt, [builder, guard, foe])
        if site.build_progress >= MAX_BUILD_PROGRESS and site.integrity >= site.max_integrity:
            break
    assert site.build_progress == MAX_BUILD_PROGRESS, site.build_progress
    assert site.integrity == site.max_integrity, (site.integrity, site.max_integrity)
    assert site.completion == 1.0, site.completion
    for _ in range(int(60 * (BUILD_DUTY_ON[1] + BUILD_DUTY_OFF[1] + 2.0))):
        world.update(dt, [builder, guard, foe])
        assert builder.job_mode == "idle", builder.job_mode
    assert builder.job_target is None

    # Serious damage is an emergency: it cancels the break immediately rather
    # than waiting out an off-duty stretch.
    site.integrity = site.max_integrity * 0.4
    world.update(dt, [builder, guard, foe])
    assert builder.job_mode in ("build", "build_travel"), builder.job_mode
    assert world.duty_state(builder).on_duty

    # With no intruder the guard takes breaks too, and the patrol ring keeps
    # turning while it does, so it rejoins the route where the route now is.
    foe.x, foe.y = 780.0, 580.0
    guard_modes = []
    angles = []
    for _ in range(int(60 * (PATROL_DUTY_ON[1] + PATROL_DUTY_OFF[1] + 2.0))):
        world.update(dt, [builder, guard, foe])
        guard_modes.append(guard.job_mode)
        if guard.job_mode == "idle":
            angles.append(site.patrol_angle)
    assert "patrol" in guard_modes, "guard never patrolled"
    assert "idle" in guard_modes, "guard never came off duty"
    assert len(set(angles)) > 1, "patrol ring stalled while the guard was off duty"

    # An intruder ends the break on the frame it appears.
    foe.x, foe.y = site.x + 20.0, site.y
    world.update(dt, [builder, guard, foe])
    assert guard.job_mode == "guard_alert", guard.job_mode
    assert world.duty_state(guard).on_duty

    # A guard that is fleeing or eating must not be dragged back onto patrol.
    guard.job_busy = True
    world.update(dt, [builder, guard, foe])
    assert guard.job_mode == "idle", guard.job_mode
    guard.job_busy = False

    print(
        f"jobs-smoke: jobs={len(JOB_IDS)} bases={len(world.bases)} alert={site.alert:.3f} "
        f"builder_off_duty_frames={off_duty_frames} guard_off_duty_frames={guard_modes.count('idle')}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
