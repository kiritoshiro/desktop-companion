"""DC-21: closing the resource loop.

Building used to be self-sufficient -- ``site.resources`` replenished from
flat builder work time, completely decoupled from ``build_progress``, which
advanced on its own flat ``dt * rate`` regardless of whether any food existed.
This module proves the replacement: a base's construction now spends
``site.resources`` (the team's banked food, filled by eaten flies), a base
with an empty larder stalls no matter how much builder time passes, and the
same base completes once food starts arriving.

It also covers the two disclosed side effects DC-21 adds on top of that: a
leveled base gives its guard a wider threat-response ring, and heals nearby
teammates -- both are the "visible team benefit" the plan asks a base level
to grant. And it covers the widened "eaten flies credit team food": every
catch by any team member now tops the base up a little, not only a Hunter's
own deliberate carry-home trip.
"""

from __future__ import annotations

import json
import os
import random
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest

from desktop_bug.world.flies import Fly
from desktop_bug.world.jobs import (
    BASE_REGEN_HP_PER_LEVEL,
    BUILD_RESOURCE_COST_PER_PROGRESS,
    GUARD_ALERT_RADIUS_PAD,
    GUARD_ALERT_RADIUS_PER_LEVEL,
    HUNTER_CARRY_FOOD_AMOUNT,
    FLY_CATCH_RESOURCE_AMOUNT,
    BaseWorld,
)
from desktop_bug.manager import CreatureManager

DT = 1.0 / 60.0


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
        self.state = "Idle"
        self._hunting_prey = False

    def relation_to(self, other):
        return "foe" if other.progression.team_id == "rivals" else (
            "friend" if self.progression.team_id == other.progression.team_id else "neutral"
        )


class HealableDummy(DummySpider):
    """A dummy that also exposes the ``heal`` surface a real Creature has."""

    def __init__(self, *args, hp=10.0, max_hp=100.0, **kwargs):
        super().__init__(*args, **kwargs)
        self.hp = hp
        self.max_hp = max_hp

    def heal(self, amount):
        before = self.hp
        self.hp = min(self.max_hp, self.hp + max(0.0, float(amount)))
        return self.hp - before


# -- Build progress spends resources, it does not self-replenish ----------


def test_build_progress_stalls_with_an_empty_larder():
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    world = BaseWorld(800, 600, rng=random.Random(4))
    for _ in range(60 * 60):
        world.update(DT, [builder])
    site = next(iter(world.bases.values()))
    assert site.resources == 0.0
    assert site.build_progress == 0.0, site.build_progress
    assert site.completion == 0.0


def test_build_progress_resumes_and_spends_resources_once_food_arrives():
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    world = BaseWorld(800, 600, rng=random.Random(4))
    for _ in range(60 * 5):
        world.update(DT, [builder])
    site = next(iter(world.bases.values()))
    assert site.build_progress == 0.0, "sanity: still stalled before food arrives"

    site.resources = 50.0
    for _ in range(60 * 5):
        world.update(DT, [builder])
    assert 0.0 < site.build_progress <= 50.0, site.build_progress
    # Resources were spent, at ``BUILD_RESOURCE_COST_PER_PROGRESS`` per point.
    spent = 50.0 - site.resources
    assert spent == pytest.approx(site.build_progress * BUILD_RESOURCE_COST_PER_PROGRESS, abs=0.01)


def test_a_base_cannot_outrun_its_larder_no_matter_how_long_it_waits():
    """Revert-proof for the gate itself: without it this runs to completion."""
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    world = BaseWorld(800, 600, rng=random.Random(4))
    for _ in range(60 * 600):
        world.update(DT, [builder])
    site = next(iter(world.bases.values()))
    assert site.build_progress == 0.0, site.build_progress
    assert site.level == 0


# -- A leveled base's visible team benefits --------------------------------


def test_a_leveled_base_widens_its_guards_alert_radius():
    world = BaseWorld(800, 600, rng=random.Random(1))
    guard = DummySpider("guard", "pack_a", 200, 200, 2)
    site = world.ensure_site(guard)
    site.level = 3
    pad = GUARD_ALERT_RADIUS_PAD + site.level * GUARD_ALERT_RADIUS_PER_LEVEL
    # Just inside the level-3 ring, but well past the level-0 one.
    foe = DummySpider("none", "rivals", site.x + site.radius + pad - 1.0, site.y, 3)
    assert (foe.x - site.x) > site.radius + GUARD_ALERT_RADIUS_PAD
    world.update(DT, [guard, foe])
    assert guard.job_mode == "guard_alert", guard.job_mode


def test_an_unleveled_bases_guard_does_not_get_the_wider_ring():
    world = BaseWorld(800, 600, rng=random.Random(1))
    guard = DummySpider("guard", "pack_a", 200, 200, 2)
    site = world.ensure_site(guard)
    assert site.level == 0
    pad = GUARD_ALERT_RADIUS_PAD + 3 * GUARD_ALERT_RADIUS_PER_LEVEL
    foe = DummySpider("none", "rivals", site.x + site.radius + pad - 1.0, site.y, 3)
    world.update(DT, [guard, foe])
    assert guard.job_mode != "guard_alert", guard.job_mode


def test_creatures_near_a_leveled_base_slowly_regenerate():
    world = BaseWorld(800, 600, rng=random.Random(1))
    guard = DummySpider("guard", "pack_a", 200, 200, 2)
    site = world.ensure_site(guard)
    site.level = 2
    healable = HealableDummy("none", "pack_a", site.x + 10.0, site.y, 5, hp=10.0)
    for _ in range(60):
        world.update(DT, [guard, healable])
    expected = min(100.0, 10.0 + 1.0 * BASE_REGEN_HP_PER_LEVEL * site.level)
    assert healable.hp == pytest.approx(expected, abs=0.05)
    assert healable.hp > 10.0


def test_no_regen_at_base_level_zero():
    world = BaseWorld(800, 600, rng=random.Random(1))
    guard = DummySpider("guard", "pack_a", 200, 200, 2)
    site = world.ensure_site(guard)
    assert site.level == 0
    healable = HealableDummy("none", "pack_a", site.x + 10.0, site.y, 5, hp=10.0)
    for _ in range(60):
        world.update(DT, [guard, healable])
    assert healable.hp == 10.0


def test_regen_does_not_touch_a_dragged_creature():
    world = BaseWorld(800, 600, rng=random.Random(1))
    guard = DummySpider("guard", "pack_a", 200, 200, 2)
    site = world.ensure_site(guard)
    site.level = 2
    healable = HealableDummy("none", "pack_a", site.x + 10.0, site.y, 5, hp=10.0)
    healable.dragging = True
    for _ in range(60):
        world.update(DT, [guard, healable])
    assert healable.hp == 10.0


# -- credit_team_food: the plumbing behind "any catch counts" -------------


def test_credit_team_food_tops_up_the_matching_teams_base():
    world = BaseWorld(800, 600, rng=random.Random(1))
    builder = DummySpider("builder", "pack_a", 200, 200, 1)
    site = world.ensure_site(builder)
    assert site.resources == 0.0
    world.credit_team_food("pack_a", FLY_CATCH_RESOURCE_AMOUNT)
    assert site.resources == pytest.approx(FLY_CATCH_RESOURCE_AMOUNT)


def test_credit_team_food_is_a_no_op_with_no_base_or_neutral_team():
    world = BaseWorld(800, 600, rng=random.Random(1))
    # No base exists for "pack_a" yet -- nothing to credit, nothing raises.
    world.credit_team_food("pack_a", 10.0)
    world.credit_team_food("neutral", 10.0)
    world.credit_team_food(None, 10.0)
    assert world.bases == {}


# -- Broadened crediting: any job's catch feeds the base a little ---------


def _write_preset(tmp_path: Path, slots: list[dict], flies: dict) -> Path:
    preset = {
        "name": "Econ",
        "slots": slots,
        "settings": {
            "flies": flies,
            "teams": {"pack_a": {"name": "Home", "color": "#4fa3d1"}},
        },
    }
    path = tmp_path / "econ.json"
    path.write_text(json.dumps(preset), encoding="utf-8")
    return path


def test_a_non_hunter_job_eating_a_fly_still_credits_team_food(monkeypatch):
    """DC-21 widens crediting past the Hunter's own carry-home mechanic:
    catch resolution is job-agnostic already (``CreatureManager._resolve_fly_catches``
    iterates every creature), so a plain "none"-job spider that happens to eat
    a fly must top the team's base up too -- just by the smaller incidental
    amount, not the Hunter's larger deliberate one."""
    tmp_path = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(tmp_path / "state"))
    preset = _write_preset(
        tmp_path,
        [{"model": "spider", "personality": "balanced", "count": 1, "team": "pack_a", "job": "none"}],
        {"enabled": False, "spawner": False},
    )
    manager = CreatureManager(preset, 800, 600, seed=1)
    creature = manager.creatures[0]
    site = manager.base_world.ensure_site(creature)
    fly = Fly(creature.x, creature.y, 800, 600)
    manager.fly_world.flies.append(fly)

    manager._resolve_fly_catches()

    assert site.resources == pytest.approx(FLY_CATCH_RESOURCE_AMOUNT)
    # Confirms this is genuinely the smaller, incidental top-up, distinct from
    # the Hunter's own larger, deliberate carry-home amount.
    assert FLY_CATCH_RESOURCE_AMOUNT < HUNTER_CARRY_FOOD_AMOUNT


def test_a_hunters_own_catch_is_not_double_credited(monkeypatch):
    """A Hunter-job creature's catch must credit HUNTER_CARRY_FOOD_AMOUNT once,
    not that plus the incidental FLY_CATCH_RESOURCE_AMOUNT on top.

    The universal "any catch counts" top-up above and the Hunter job's own
    deliberate carry-home credit both key off the same event -- a catch
    that puts the eater into ``Feed`` state -- so without an explicit
    exclusion a Hunter's single catch nets both amounts instead of just its
    own larger one."""
    tmp_path = Path(tempfile.mkdtemp(prefix="desktop-bug-tests-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(tmp_path / "state"))
    preset = _write_preset(
        tmp_path,
        [{"model": "spider", "personality": "hunter", "count": 1, "team": "pack_a", "job": "hunter"}],
        {"enabled": False, "spawner": False},
    )
    manager = CreatureManager(preset, 800, 600, seed=1)
    hunter = manager.creatures[0]
    site = manager.base_world.ensure_site(hunter)
    fly = Fly(hunter.x, hunter.y, 800, 600)
    manager.fly_world.flies.append(fly)

    manager._resolve_fly_catches()
    assert site.resources == 0.0, "the incidental top-up must not fire for the Hunter's own catch"

    for _ in range(600):
        manager.update(1.0 / 60.0, -5000.0, -5000.0)

    assert site.resources == pytest.approx(HUNTER_CARRY_FOOD_AMOUNT), site.resources


# -- Acceptance: with flies off a base stalls, with flies on it advances --


def _run_colony(flies_enabled: bool, frames: int, seed: int) -> float:
    """One full headless CreatureManager run; returns the team base's
    ``build_progress`` at the end.

    The Builder's own chase/approach/jump skills are switched off. Every
    creature, including the Builder, instinctively chases a nearby fly on its
    own (``CreatureManager._creature_can_hunt`` -- a job is "a shift, not a
    personality transplant", per jobs.py's own header) -- correct and
    unrelated to DC-21, but at any fly rate fast enough to matter within a
    test's time budget it leaves the Builder job_busy (personality-outranked)
    almost the entire run, on both sides of the comparison, which would make
    the test measure "how much this Builder got distracted" rather than the
    resource loop. Turning those three skills off for this Builder isolates
    the actual thing DC-21 changed; a Builder that also personally hunts
    remains covered by the existing job_busy tests in test_jobs.py.
    """
    tmp_path = Path(tempfile.mkdtemp(prefix=f"desktop-bug-tests-econ-{flies_enabled}-{seed}-"))
    os.environ["DESKTOP_BUG_STATE_DIR"] = str(tmp_path / "state")
    preset = _write_preset(
        tmp_path,
        [
            {"model": "spider", "personality": "balanced", "count": 1, "team": "pack_a", "job": "builder"},
            {"model": "spider", "personality": "hunter", "count": 1, "team": "pack_a", "job": "hunter"},
        ],
        {"enabled": True, "min_interval": 1.5, "max_interval": 3.0, "max_flies": 6, "spawner": True},
    )
    manager = CreatureManager(preset, 1600, 900, seed=seed)
    builder = next(c for c in manager.creatures if c.job_id == "builder")
    for skill in ("chase", "approach", "jump"):
        builder.set_skill_enabled(skill, False)
    manager.set_flies_enabled(flies_enabled)
    for _ in range(frames):
        manager.update(DT, -5000.0, -5000.0)
    site = next(iter(manager.base_world.bases.values()))
    return site.build_progress


@pytest.mark.slow
def test_a_colony_with_flies_disabled_stalls_below_a_colony_with_flies_enabled():
    frames = 60 * 300  # five minutes of simulated time
    seed = 3
    disabled = _run_colony(False, frames, seed)
    enabled = _run_colony(True, frames, seed)

    # Deterministic: with no food ever arriving, the base cannot advance at
    # all -- not merely "less than the other run".
    assert disabled == 0.0, disabled
    # The fly world's own timers use the module-level ``random`` (DC-09's
    # disclosed gap), so the exact number here is not reproducible run to
    # run; the meaningful, reliably-true claim is a solid margin over zero.
    assert enabled > 50.0, enabled
