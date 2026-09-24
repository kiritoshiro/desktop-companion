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

DC-81 changed how it gets between posts. DC-42's guard swept back and forth
along an arc of the standoff ring while facing outwards -- sideways to its own
motion. The owner: *"guards move weird when defending. they shouldnt orbit
like that, just walk in strigh line or throught the base not orbit
sideways"*. Measured over a minute, one guard and two: on main a moving
patrolling guard's heading was a median 89 degrees off its direction of
travel, and over 45 degrees for 79% of moving frames. It now walks a straight
line through the base between two ends on that same ring, facing the way it
walks (median 0, 8% over 45 -- the turns at each end), and stands watch at
each end facing outwards. The standoff, the alert ring and the outward watch
below are DC-42's and still hold.
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
    GUARD_STANDOFF_PAD,
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
    watch_frames: dict[int, int] = {}
    for _ in range(int(seconds * 60)):
        manager.update(DT, *AWAY)
        for guard in manager.creatures:
            # How long this guard has been standing its current watch.
            if getattr(guard, "job_facing", None) is not None and guard.state == "JobPatrol":
                watch_frames[id(guard)] = watch_frames.get(id(guard), 0) + 1
            else:
                watch_frames[id(guard)] = 0
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
                "axis": site.patrol_angle,
                "watch_frames": watch_frames.get(id(guard), 0),
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
    # DC-84: judged after the first second of each watch. DC-81's watches are
    # 2.5-4.5 s, where DC-42's post was held indefinitely, and a guard that
    # reaches an end facing inwards (walking back to its line from outside
    # after a break) turns round on the spot for about a second -- 21% of
    # on-post frames on the merged DC-81 + DC-83 main, against a bar of 20%.
    on_post = [s["facing_error"] for s in _on_station(manager, site)
               if s["on_post"] and s["watch_frames"] > 60]
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
    """Every place it stands watch is on one line through the base.

    The two ends of the line are opposite each other, so the outward
    direction flips by half a turn between them; modulo half a turn, all the
    watch positions agree. An orbit would spread them all the way round.
    """
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    # The line itself turns slowly (GUARD_POST_DRIFT, about 27 degrees in
    # 40 s) so no approach stays unwatched, so each watch is judged against
    # the line as it stood at that moment.
    offsets = [s["outward"] - s["axis"] for s in _on_station(manager, site, 40.0) if s["on_post"]]
    assert offsets, "the guard never stood watch"
    worst = max(abs(math.atan2(math.sin(2 * o), math.cos(2 * o))) / 2 for o in offsets)
    assert worst < math.radians(12.0), (
        f"watch positions spread {math.degrees(worst):.0f} deg off one line"
    )


def _patrol_motion(manager, site, seconds: float = 40.0):
    """(heading-vs-travel error in degrees, distance from centre in radii)
    for every frame a patrolling guard is actually moving."""
    prev = {id(c): (c.x, c.y) for c in manager.creatures}
    out = []
    for _ in range(int(seconds * 60)):
        manager.update(DT, *AWAY)
        for guard in manager.creatures:
            px, py = prev[id(guard)]
            vx, vy = (guard.x - px) / DT, (guard.y - py) / DT
            prev[id(guard)] = (guard.x, guard.y)
            if guard.state != "JobPatrol" or math.hypot(vx, vy) <= 12.0:
                continue
            d = math.atan2(vy, vx) - guard.heading
            out.append((abs(math.degrees(math.atan2(math.sin(d), math.cos(d)))),
                        math.hypot(guard.x - site.x, guard.y - site.y) / site.radius))
    return out


@pytest.mark.parametrize("guards", (1, 2))
def test_a_guard_walks_the_way_it_faces(monkeypatch, guards):
    """DC-81: the fault the owner saw. On main the median was 89 degrees."""
    manager, site = _guard_colony(monkeypatch, guards=guards)
    _settle(manager)
    errors = sorted(error for error, _ in _patrol_motion(manager, site))
    assert errors, "no guard ever walked"
    median = errors[len(errors) // 2]
    sideways = sum(1 for error in errors if error > 45.0) / len(errors)
    assert median < 15.0, f"a patrolling guard typically walked {median:.0f} deg off its heading"
    assert sideways < 0.2, f"{100 * sideways:.0f}% of its walking was sideways"


def test_a_guard_walks_through_the_base(monkeypatch):
    manager, site = _guard_colony(monkeypatch)
    _settle(manager)
    nearest = min(radius for _, radius in _patrol_motion(manager, site))
    assert nearest < 0.35, f"it never came nearer the centre than {nearest:.2f} radii"


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
