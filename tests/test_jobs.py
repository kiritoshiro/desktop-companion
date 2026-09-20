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
from desktop_bug.webs import Web, WebWorld, _plan_orb


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
        # DC-20: Hunter job bookkeeping reads these two off the real Creature;
        # a dummy needs them too so the hunter loop can be driven headlessly.
        self.state = "Idle"
        self._hunting_prey = False

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
    # DC-21: build progress now spends ``site.resources`` (see
    # test_jobs_economy.py for that gating itself). This fixture is shared by
    # duty-cycle/timing tests that predate the economy and are not about it,
    # so the site is founded and seeded at the field's own cap (``BaseSite.from_dict``
    # clamps to 1000.0 too, so the round-trip test stays exact) -- comfortably
    # more than the 500 total a full build spends -- *before* the first
    # ``update``, so their behaviour is exactly as it was before DC-21 from
    # frame one.
    site = world.ensure_site(builder)
    site.resources = 1000.0
    world.update(DT, [builder, guard, foe])
    return world, site, builder, guard, foe


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


# -- DC-20: Scout -----------------------------------------------------------


def test_a_scout_covers_sectors_and_reports_to_the_blackboard(colony):
    """A scout travels to a sector, then holds a beat to publish a report.

    ``BaseWorld`` never moves a creature itself (that is ``Creature``'s job in
    the real app), so this teleports the dummy onto its own current target
    each tick -- the same scripting other tests here already use for an
    intruder's position -- to observe what happens once a scout actually
    arrives, rather than simulating a walk.
    """
    world, site, builder, guard, foe = colony
    scout = DummySpider("scout", "pack_a", site.x, site.y, 5)
    world.update(DT, [builder, guard, foe, scout])
    assert scout.job_mode == "scout_travel", scout.job_mode
    target = scout.job_target
    assert target is not None

    modes = set()
    for _ in range(60 * 8):
        scout.x, scout.y = target
        world.update(DT, [builder, guard, foe, scout])
        modes.add(scout.job_mode)
        target = scout.job_target or target
    assert "scout_report" in modes, modes
    assert site.points_of_interest, "scout never published anything to the team blackboard"


def test_a_scout_reports_a_foe_it_finds_in_a_sector(colony):
    world, site, builder, guard, foe = colony
    scout = DummySpider("scout", "pack_a", site.x, site.y, 5)
    world.update(DT, [builder, guard, foe, scout])
    target = scout.job_target
    # The intruder happens to be sitting exactly where the scout is headed.
    foe.x, foe.y = target

    for _ in range(60 * 8):
        scout.x, scout.y = target
        world.update(DT, [builder, guard, foe, scout])
        target = scout.job_target or target
    kinds = {poi["kind"] for poi in site.points_of_interest}
    assert "foe" in kinds, site.points_of_interest


def test_a_busy_scout_does_not_claim_a_sector(colony):
    world, site, builder, guard, foe = colony
    scout = DummySpider("scout", "pack_a", site.x, site.y, 5)
    scout.job_busy = True
    world.update(DT, [builder, guard, foe, scout])
    assert scout.job_mode == "idle", scout.job_mode


# -- DC-20: Webber ------------------------------------------------------------


def _web_at(x: float, y: float, screen=(800, 600)) -> Web:
    strands, hub, wob = _plan_orb((x, y), 80.0, *screen)
    web = Web("orb", strands, hub, wob, f"test_{x}_{y}", 20.0, *screen)
    web.state = "complete"
    web.built = len(web.strands)
    return web


def test_a_webber_travels_to_a_torn_web_and_mends_it(colony):
    world, site, builder, guard, foe = colony
    web_world = WebWorld(800, 600)
    web = _web_at(site.x + 60.0, site.y)
    web_world.webs.append(web)
    si, segi, _length = web._tearable[0]
    web.cut.add((si, segi))
    assert web.is_damaged()

    webber = DummySpider("webber", "pack_a", site.x + 300.0, site.y + 200.0, 6)
    world.update(DT, [builder, guard, foe, webber], web_world)
    assert webber.job_mode == "web_travel", webber.job_mode
    target = webber.job_target

    modes = set()
    for _ in range(60 * 10):
        webber.x, webber.y = target
        world.update(DT, [builder, guard, foe, webber], web_world)
        modes.add(webber.job_mode)
        target = webber.job_target or target
    assert "web_repair" in modes, modes
    assert not web.is_damaged(), "webber never finished mending the torn web"


def test_a_webber_tops_up_the_teams_web_count_when_nothing_is_torn(colony):
    world, site, builder, guard, foe = colony
    web_world = WebWorld(800, 600)
    webber = DummySpider("webber", "pack_a", site.x, site.y, 6)
    world.update(DT, [builder, guard, foe, webber], web_world)
    target = webber.job_target

    modes = set()
    for _ in range(60 * 10):
        if target is not None:
            webber.x, webber.y = target
        world.update(DT, [builder, guard, foe, webber], web_world)
        modes.add(webber.job_mode)
        target = webber.job_target or target
    assert "web_weave" in modes, modes
    assert len(web_world.webs) >= 1


def test_a_webber_with_no_web_world_stays_idle_rather_than_crashing(colony):
    world, site, builder, guard, foe = colony
    webber = DummySpider("webber", "pack_a", site.x, site.y, 6)
    world.update(DT, [builder, guard, foe, webber])
    assert webber.job_mode == "idle", webber.job_mode


# -- DC-20: Hunter ------------------------------------------------------------


def test_a_hunter_patrols_close_to_home_when_not_hunting(colony):
    world, site, builder, guard, foe = colony
    hunter = DummySpider("hunter", "pack_a", site.x, site.y, 7)
    modes = set()
    for _ in range(60 * 20):
        world.update(DT, [builder, guard, foe, hunter])
        modes.add(hunter.job_mode)
    assert "hunt_patrol" in modes, modes


def test_a_hunting_spider_still_marks_its_job_on_duty(colony):
    """Personality outranks job duty during a live hunt (disclosed, unchanged
    priority) -- but the job still logs itself on duty for that frame rather
    than reporting idle."""
    world, site, builder, guard, foe = colony
    hunter = DummySpider("hunter", "pack_a", site.x, site.y, 7)
    hunter._hunting_prey = True
    world.update(DT, [builder, guard, foe, hunter])
    assert hunter.job_mode == "hunting", hunter.job_mode


def test_a_hunter_carries_a_catch_home_and_it_feeds_the_base(colony):
    world, site, builder, guard, foe = colony
    hunter = DummySpider("hunter", "pack_a", site.x + 500.0, site.y + 300.0, 7)
    # Simulate a catch exactly like CreatureManager._resolve_fly_catches does
    # on a real creature: the state becomes "Feed" and the hunt flag clears.
    hunter.state = "Feed"
    hunter._hunting_prey = False
    world.update(DT, [builder, guard, foe, hunter])
    hunter.state = "Idle"

    modes = set()
    for _ in range(60 * 20):
        world.update(DT, [builder, guard, foe, hunter])
        modes.add(hunter.job_mode)
        if hunter.job_mode == "hunt_return" and hunter.job_target is not None:
            # Nudge it most of the way home each tick rather than teleporting
            # flush onto the target, so "hunt_return" has more than the one
            # frame it takes to close the last few pixels to actually show up.
            tx, ty = hunter.job_target
            hunter.x += (tx - hunter.x) * 0.5
            hunter.y += (ty - hunter.y) * 0.5
    assert "hunt_return" in modes, modes
    assert site.resources > 0.0, "a delivered catch never credited the base"


def test_a_hunter_with_no_team_base_still_gets_a_home_to_patrol(colony):
    """`presets/colony.json` deliberately puts its Hunter on a base-less
    rival team (DC-30/DC-33's "give the Guard something to guard against"),
    so the job must still produce states without a real ``BaseSite``."""
    world, _site, _builder, _guard, foe = colony
    foe.job_id = "hunter"
    modes = set()
    for _ in range(60 * 20):
        world.update(DT, [foe])
        modes.add(foe.job_mode)
    assert "hunt_patrol" in modes, modes
    assert foe.job_base_id is None
