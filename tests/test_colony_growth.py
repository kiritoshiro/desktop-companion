"""Every team owns a base, and a base raises spiders (DC-54, DC-55).

Two of the owner's requests, and they are one mechanism.

*"as many teams there are that many bases supposed to be."* A base only ever
existed where a **Builder** happened to settle, so a team without one had
nowhere to heal, nowhere to bank food and nothing for its Guards or Scouts to
do -- and the Teams panel could show two teams with one base between them.

*"lets implement that having a base it raises a new spiders. however it should
be capped at 5 spiders per team."* This is the answer to the repopulation
question that had been open since DC-47 made death permanent: a session could
only ever lose spiders until a relaunch refilled it.

The part that is not obvious, and that the first attempt got wrong: **food had
to be split into two pools.** Building spends continuously until a base is
finished, and a full base costs `MAX_BUILD_PROGRESS` in food against a colony
income of roughly fourteen a minute -- so with one pool every scrap went into
the ground and nothing was ever raised. `GROWTH_FOOD_SHARE` is the slice
building cannot reach.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.manager.colony import (
    RAISED_MODEL_ID,
    RAISE_FOOD_COST,
    RAISE_MIN_COMPLETION,
    RAISE_SECONDS,
    TEAM_POPULATION_CAP,
)
from desktop_bug.world.jobs import (
    GROWTH_FOOD_SHARE,
    MAX_BUILD_PROGRESS,
    BaseSite,
    BaseWorld,
)

DT = 1.0 / 60.0
SCREEN = (1400, 900)
AWAY = (-9000.0, -9000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _colony(monkeypatch, slots=None):
    scratch = Path(tempfile.mkdtemp(prefix="dc55-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "c.json"
    preset.write_text(json.dumps({
        "name": "c",
        "slots": slots or [
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "a", "team": "hunters"},
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "b", "team": "rivals"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    return CreatureManager(preset, *SCREEN, seed=5)


def _sites(manager):
    return {site.team_id: site for site in manager.base_world.bases.values()}


# ------------------------------------------------------- one base per team

def test_every_named_team_gets_a_base_without_a_builder(monkeypatch):
    """Neither slot has a builder; both teams still own a base."""
    manager = _colony(monkeypatch)
    assert all(getattr(c, "job_id", "none") == "none" for c in manager.creatures)
    manager.update(DT, *AWAY)
    assert set(_sites(manager)) == {"hunters", "rivals"}


def test_a_team_gets_exactly_one_base_however_many_members(monkeypatch):
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": 4,
         "slot_id": "a", "team": "hunters"},
    ])
    for _ in range(10):
        manager.update(DT, *AWAY)
    assert len(manager.base_world.bases) == 1


def test_a_solo_spider_founds_nothing():
    """"Neutral / solo" is the absence of a team. Giving it a base would
    either found one shared base for every unaffiliated spider on the
    desktop, or one each."""
    world = BaseWorld(*SCREEN)
    loner = SimpleNamespace(x=100.0, y=100.0, index=0, progression_id="x",
                            progression=SimpleNamespace(team_id="neutral"),
                            dead=False)
    world.ensure_team_sites([loner])
    assert world.bases == {}


def test_a_base_is_founded_at_the_team_s_centre(monkeypatch):
    """Not on whichever member the list happened to hold first."""
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": 2,
         "slot_id": "a", "team": "hunters"},
    ])
    left, right = manager.creatures[0], manager.creatures[1]
    left.x, left.y = 200.0, 300.0
    right.x, right.y = 600.0, 500.0
    manager.base_world.clear()
    manager.update(DT, *AWAY)
    site = _sites(manager)["hunters"]
    assert site.x == pytest.approx(400.0)
    assert site.y == pytest.approx(400.0)


def test_a_dead_spider_does_not_found_a_base_for_its_team():
    world = BaseWorld(*SCREEN)
    ghost = SimpleNamespace(x=100.0, y=100.0, index=0, progression_id="x",
                            progression=SimpleNamespace(team_id="hunters"),
                            dead=True)
    world.ensure_team_sites([ghost])
    assert world.bases == {}


# ------------------------------------------------------------- the larder

def test_food_is_split_between_building_and_growth():
    world = BaseWorld(*SCREEN)
    site = BaseSite(id="team:hunters", owner_id="o", team_id="hunters", x=100.0, y=100.0)
    world.bases[site.id] = site
    world.credit_team_food("hunters", 100.0)
    assert site.larder == pytest.approx(100.0 * GROWTH_FOOD_SHARE)
    assert site.resources == pytest.approx(100.0 * (1.0 - GROWTH_FOOD_SHARE))


def test_the_larder_survives_a_save_and_reload():
    world = BaseWorld(*SCREEN)
    site = BaseSite(id="team:hunters", owner_id="o", team_id="hunters",
                    x=100.0, y=100.0, larder=41.0)
    world.bases[site.id] = site
    reloaded = BaseWorld(*SCREEN, world.to_dict())
    assert next(iter(reloaded.bases.values())).larder == pytest.approx(41.0)


def test_building_cannot_spend_the_larder(monkeypatch):
    """The whole reason there are two pools."""
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": 1,
         "slot_id": "a", "team": "hunters", "job": "builder"},
    ])
    manager.update(DT, *AWAY)
    site = _sites(manager)["hunters"]
    site.resources = 60.0
    site.larder = 40.0
    for _ in range(60 * 30):
        manager.update(DT, *AWAY)
        if site.resources <= 0.0:
            break
    assert site.larder == pytest.approx(40.0), "building ate the growth larder"


# ------------------------------------------------------ raising a spider

def _ready_to_raise(manager, team="hunters"):
    manager.update(DT, *AWAY)
    site = _sites(manager)[team]
    site.build_progress = MAX_BUILD_PROGRESS * (RAISE_MIN_COMPLETION + 0.05)
    site.larder = RAISE_FOOD_COST + 1.0
    return site


def test_a_base_raises_a_spider(monkeypatch):
    manager = _colony(monkeypatch)
    site = _ready_to_raise(manager)
    before = len(manager.creatures)

    for _ in range(int((RAISE_SECONDS + 1.0) * 60)):
        manager.update(DT, *AWAY)
        if len(manager.creatures) > before:
            break

    assert len(manager.creatures) == before + 1
    born = manager.creatures[-1]
    assert born.progression.team_id == "hunters"
    assert born.model["id"] == RAISED_MODEL_ID
    assert site.larder == pytest.approx(1.0), "the food was not spent"


def test_a_raised_spider_has_its_own_random_colours(monkeypatch):
    from desktop_bug.content.palettes import PALETTE_KEYS

    manager = _colony(monkeypatch)
    _ready_to_raise(manager)
    before = len(manager.creatures)
    for _ in range(int((RAISE_SECONDS + 1.0) * 60)):
        manager.update(DT, *AWAY)
        if len(manager.creatures) > before:
            break
    born = manager.creatures[-1]
    assert set(born.color_overrides) == set(PALETTE_KEYS)


def test_it_takes_a_moment_rather_than_appearing_at_once(monkeypatch):
    """Otherwise a spider blinks into existence the frame the food lands."""
    manager = _colony(monkeypatch)
    _ready_to_raise(manager)
    before = len(manager.creatures)
    for _ in range(int(RAISE_SECONDS * 60) - 10):
        manager.update(DT, *AWAY)
    assert len(manager.creatures) == before


def test_nothing_is_raised_without_enough_food(monkeypatch):
    manager = _colony(monkeypatch)
    site = _ready_to_raise(manager)
    site.larder = RAISE_FOOD_COST - 1.0
    before = len(manager.creatures)
    for _ in range(int((RAISE_SECONDS + 2.0) * 60)):
        manager.update(DT, *AWAY)
    assert len(manager.creatures) == before


def test_nothing_is_raised_from_a_base_that_is_barely_dug(monkeypatch):
    manager = _colony(monkeypatch)
    site = _ready_to_raise(manager)
    site.build_progress = MAX_BUILD_PROGRESS * (RAISE_MIN_COMPLETION - 0.05)
    before = len(manager.creatures)
    for _ in range(int((RAISE_SECONDS + 2.0) * 60)):
        manager.update(DT, *AWAY)
    assert len(manager.creatures) == before


def test_the_cap_is_five_living_spiders_per_team(monkeypatch):
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": TEAM_POPULATION_CAP,
         "slot_id": "a", "team": "hunters"},
    ])
    site = _ready_to_raise(manager)
    site.larder = RAISE_FOOD_COST * 4
    before = len(manager.creatures)
    for _ in range(int((RAISE_SECONDS + 2.0) * 60)):
        manager.update(DT, *AWAY)
    assert len(manager.creatures) == before
    assert site.larder == pytest.approx(RAISE_FOOD_COST * 4), "a full team paid anyway"


def test_a_team_that_loses_one_can_replace_one(monkeypatch):
    """What the cap counts is the living, which is the point of the feature."""
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": TEAM_POPULATION_CAP,
         "slot_id": "a", "team": "hunters"},
    ])
    _ready_to_raise(manager)
    manager.creatures[0].dead = True
    before = sum(1 for c in manager.creatures if not c.dead)

    for _ in range(int((RAISE_SECONDS + 2.0) * 60)):
        manager.update(DT, *AWAY)
        if sum(1 for c in manager.creatures if not c.dead) > before:
            break
    assert sum(1 for c in manager.creatures if not c.dead) == before + 1


def test_a_solo_spiders_base_raises_nothing(monkeypatch):
    """There is no such base, and the growth pass must not invent one."""
    manager = _colony(monkeypatch, slots=[
        {"model": "tarantula", "personality": "mellow", "count": 1,
         "slot_id": "a", "team": "neutral"},
    ])
    before = len(manager.creatures)
    for _ in range(int((RAISE_SECONDS + 2.0) * 60)):
        manager.update(DT, *AWAY)
    assert len(manager.creatures) == before


def test_the_wait_restarts_rather_than_charging_for_a_lost_spider(monkeypatch):
    """Food is spent on arrival, not on commitment.

    The raise timers are transient -- a reload starts the wait again -- so
    charging up front would bill a colony for a spider it never received.
    """
    manager = _colony(monkeypatch)
    site = _ready_to_raise(manager)
    for _ in range(int(RAISE_SECONDS * 60) - 20):
        manager.update(DT, *AWAY)
    assert site.larder == pytest.approx(RAISE_FOOD_COST + 1.0)
    manager._raise_timers.clear()          # what a reload does
    assert site.larder == pytest.approx(RAISE_FOOD_COST + 1.0)
