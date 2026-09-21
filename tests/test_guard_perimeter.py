"""A guard holds a line at a standoff, facing outwards (DC-42).

A guard used to orbit at 0.78 of its base radius -- inside its own wall,
circling the site continuously, facing the way it walked. It read as pacing a
boundary rather than watching an approach.

It now takes a post on a ring outside the base but inside its own alert
radius, so it already stands between the base and anything coming in; it
tracks a little way along that line rather than going round it; and once on
station it faces outwards, away from what it is guarding.

Both locomotion paths (the legacy body path and the spider gait path) have to
agree about that, which is why they share one predicate rather than each
deciding for themselves.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager
from desktop_bug.world.jobs import (
    GUARD_ALERT_RADIUS_PAD,
    GUARD_FACE_OUT_DIST,
    GUARD_POST_DRIFT,
    GUARD_STANDOFF_PAD,
    GUARD_SWEEP_ARC,
)

DT = 1.0 / 60.0
SCREEN = (1200, 800)
AWAY = (-5000.0, -5000.0)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


def _guard_colony(monkeypatch, guards: int = 1):
    """One base with `guards` guards on it, and nothing else to distract them.

    Flies are off: a live fly outranks job duty (the disclosed
    `_job_outranked_by_personality` dynamic DC-18 and DC-20 both recorded),
    and this package is about where a guard stands, not about that priority.
    """
    base = Path(tempfile.mkdtemp(prefix="dc42-guard-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(base / "state"))
    preset = base / "guard.json"
    preset.write_text(json.dumps({
        "name": "guard",
        "slots": [{"model": "tarantula", "personality": "mellow", "count": guards,
                   "slot_id": "s0", "team": "pack_a", "job": "guard"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")

    manager = CreatureManager(preset, *SCREEN, seed=4)
    site = manager.base_world.ensure_site(manager.creatures[0])
    site.x, site.y = 600.0, 400.0
    site.build_progress = 500.0
    site.level = 3
    for creature in manager.creatures:
        creature.x, creature.y = site.x + 6.0, site.y + 6.0
    return manager, site


def _settle(manager, seconds: float = 8.0):
    for _ in range(int(seconds * 60)):
        manager.update(DT, *AWAY)


def _on_station(manager, site, seconds: float = 30.0):
    """Sample every guard while it is actually patrolling.

    Every field is read at sample time, `job_facing` included: it is cleared
    and rewritten every frame, so reading it afterwards reports one arbitrary
    frame rather than the frame the sample came from.
    """
    samples = []
    for _ in range(int(seconds * 60)):
        manager.update(DT, *AWAY)
        for guard in manager.creatures:
            if guard.state != "JobPatrol":
                continue
            outward = math.atan2(guard.y - site.y, guard.x - site.x)
            samples.append({
                "guard": id(guard),
                "past_ring": math.hypot(guard.x - site.x, guard.y - site.y) - site.radius,
                "outward": outward,
                "facing_error": abs(math.atan2(math.sin(guard.heading - outward),
                                               math.cos(guard.heading - outward))),
                "on_post": getattr(guard, "job_facing", None) is not None,
            })
    return samples


def test_a_guard_stands_off_the_base_instead_of_hugging_its_wall(monkeypatch):
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    samples = _on_station(manager, site)
    assert samples, "the guard never patrolled"

    # Judged while it is actually on its post. A guard that has just come back
    # on shift may still be walking out through the base, which is fine.
    on_post = [s["past_ring"] for s in samples if s["on_post"]]
    assert on_post, "the guard never reached its post"
    assert min(on_post) > 0.0, f"a guard on station was inside the wall: {min(on_post):.1f}"
    average = sum(on_post) / len(on_post)
    assert abs(average - GUARD_STANDOFF_PAD) < 18.0, average
    # The old ring sat at 0.78 of the radius, measured from the centre, so it
    # was always inside the wall. Compare like for like.
    assert average + site.radius > site.radius * 0.78 + 20.0, average


def test_a_guard_still_stands_inside_the_ring_it_reacts_from(monkeypatch):
    """Standing off is pointless if it puts the guard outside its own alert ring."""
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    alert_reach = GUARD_ALERT_RADIUS_PAD + site.level * 15.0
    for sample in _on_station(manager, site, seconds=12.0):
        if sample["on_post"]:
            assert sample["past_ring"] < alert_reach, (sample["past_ring"], alert_reach)


def test_a_guard_on_station_faces_outwards(monkeypatch):
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    on_post = [s["facing_error"] for s in _on_station(manager, site) if s["on_post"]]
    assert on_post, "the guard never reached its post"

    # Arriving at a post means turning from the way it walked to facing out,
    # which takes about a second, so the worst single frame is a spider
    # mid-turn rather than a guard looking the wrong way. Judge the settled
    # posture: most of the time it is pointed out, and typically dead on.
    settled = sorted(on_post)
    median = math.degrees(settled[len(settled) // 2])
    assert median < 12.0, f"a guard on station typically faced {median:.0f} deg off"
    facing_out = sum(1 for error in on_post if error < math.radians(25.0))
    assert facing_out / len(on_post) > 0.8, (
        f"only {100 * facing_out / len(on_post):.0f}% of on-post frames faced outwards"
    )


def test_a_guard_holds_its_line_rather_than_orbiting(monkeypatch):
    """The old ring went round every ~15 s. A post should barely move."""
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    seconds = 40.0
    angles = [s["outward"] for s in _on_station(manager, site, seconds) if s["on_post"]]
    assert angles

    # How far round the base it got, not how much path it covered: sweeping
    # back and forth along one line covers ground without going anywhere.
    unwrapped = [angles[0]]
    for current in angles[1:]:
        step = math.atan2(math.sin(current - unwrapped[-1]), math.cos(current - unwrapped[-1]))
        unwrapped.append(unwrapped[-1] + step)
    extent = max(unwrapped) - min(unwrapped)
    allowed = GUARD_POST_DRIFT * seconds + GUARD_SWEEP_ARC * 2.0
    assert extent < allowed * 1.6, (
        f"a guard covered {math.degrees(extent):.0f} deg of the base in {seconds:.0f} s; "
        f"holding a line allows about {math.degrees(allowed):.0f}"
    )
    # The ring it replaced went right round in roughly fifteen seconds.
    assert extent < math.pi, f"{math.degrees(extent):.0f} deg is still orbiting"


def test_two_guards_watch_different_approaches(monkeypatch):
    """Posts are spread around the base, not stacked on one another."""
    manager, site = _guard_colony(monkeypatch, guards=2)
    _settle(manager, seconds=12.0)
    seen: dict[int, float] = {}
    for sample in _on_station(manager, site, seconds=10.0):
        if sample["on_post"]:
            seen[sample["guard"]] = sample["outward"]
    assert len(seen) == 2, "both guards must have patrolled"
    first, second = seen.values()
    apart = abs(math.atan2(math.sin(first - second), math.cos(first - second)))
    assert apart > 1.0, f"two guards only {math.degrees(apart):.0f} deg apart"


def test_walking_to_a_far_post_still_faces_the_way_it_walks(monkeypatch):
    """Facing outwards is for standing a post, not for crossing the screen.

    A spider striding along while facing square across its own path reads as
    broken, so the job only publishes a facing once the guard has arrived.
    """
    manager, site = _guard_colony(monkeypatch)
    guard = manager.creatures[0]
    manager.update(DT, *AWAY)
    guard.x, guard.y = site.x + 420.0, site.y - 380.0
    manager.update(DT, *AWAY)
    if guard.state == "JobPatrol":
        target = guard.job_target
        assert target is not None
        far = math.hypot(target[0] - guard.x, target[1] - guard.y)
        assert far > GUARD_FACE_OUT_DIST
        assert guard.job_facing is None, "it asked to face outwards while still walking there"


def test_both_locomotion_paths_share_one_rule_about_facing(monkeypatch):
    """A legacy-body spider and a gait spider must not disagree here."""
    root = Path(__file__).resolve().parents[1] / "src" / "desktop_bug" / "creature" / "kinematics"
    gait = (root / "gait.py").read_text(encoding="utf-8")
    movement = (root / "movement.py").read_text(encoding="utf-8")
    for name, text in (("gait", gait), ("movement", movement)):
        assert "_walks_while_facing_elsewhere()" in text, name
        assert 'self.state == "Observe" and self._acts_as_observer()' not in text, (
            f"{name} still decides for itself which states face elsewhere"
        )
