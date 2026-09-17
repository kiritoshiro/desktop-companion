"""Silk is thrown, not guided.

Both web shots steered onto their target for the whole flight, so neither could
be dodged and both read as guided missiles rather than thrown webs. They now
aim once, leading a moving target by where it is actually going, and fly
straight. Correcting in flight is what the Silk tracking ability restores, so a
spider earns it instead of starting with it.
"""

import math
import os
import sys
import tempfile
from types import SimpleNamespace

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
from pathlib import Path  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.flies import WebShotProjectile  # noqa: E402
from desktop_bug.mouse_webs import MouseWebWorld, _Projectile  # noqa: E402
from desktop_bug.progression import ABILITY_BY_ID, ProgressionState  # noqa: E402

DT = 1.0 / 120.0


def fly_path(projectile: _Projectile, pointer, steps: int = 400):
    """Run a cursor glob against a moving pointer; return the outcome."""
    for step in range(steps):
        mx, my = pointer(step * DT)
        result = projectile.update(DT, mx, my)
        if result != "fly":
            return result, step * DT
    return "fly", steps * DT


def check_ability_exists() -> None:
    node = ABILITY_BY_ID.get("silk_tracking")
    assert node is not None, "the Silk tracking ability is missing"
    assert node.effects.get("web_homing", 0.0) > 0.0, node.effects
    assert node.level_required >= 5, "tracking silk should not be a starting ability"
    assert "web_crafter" in node.prerequisites, node.prerequisites

    # A fresh spider has nothing unlocked, so it throws straight.
    state = ProgressionState()
    assert "silk_tracking" not in state.unlocked_abilities


def closest_approach(projectile: _Projectile, pointer, steps: int = 900):
    """Return (outcome, nearest distance to the pointer during the flight)."""
    nearest = float("inf")
    for step in range(steps):
        mx, my = pointer(step * DT)
        result = projectile.update(DT, mx, my)
        nearest = min(nearest, math.dist(projectile.pos, (mx, my)))
        if result != "fly":
            return result, nearest
    return "fly", nearest


def check_straight_shot_misses_a_dodge() -> None:
    """The point of the change: a moving target can get out of the way.

    Tracking is asserted as *following*, not as a guaranteed hit. Measured
    against these dodges it closes to roughly half the distance a straight shot
    manages but still misses, so the old behaviour was never an undodgeable
    missile either -- it simply curved, which is what read wrongly.
    """
    def dodging(t: float):
        # Sits still until the glob is committed, then leaves.
        return (900.0, 400.0 + (0.0 if t < 0.03 else 500.0 * (t - 0.03)))

    plain = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=0.0)
    straight_outcome, straight_near = closest_approach(plain, dodging)
    assert straight_outcome == "miss", f"a straight shot still tracked a dodging pointer: {straight_outcome}"

    tracking = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=1.0)
    _outcome, tracking_near = closest_approach(tracking, dodging)
    assert tracking_near < straight_near * 0.7, (
        f"Silk tracking did not follow the pointer: closest {tracking_near:.0f}px "
        f"against {straight_near:.0f}px for a straight shot"
    )


def check_straight_shot_still_hits_a_still_target() -> None:
    """Removing homing must not make the shot useless."""
    still = lambda _t: (900.0, 400.0)  # noqa: E731
    for homing in (0.0, 1.0):
        shot = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=homing)
        outcome, _ = fly_path(shot, still)
        assert outcome == "hit", f"a shot at a stationary pointer missed (homing={homing})"


def check_lead_is_applied() -> None:
    """Aimed where the target is going, not where it was."""
    straight = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=0.0)
    assert abs(straight.vel[1]) < 1e-6, f"a shot at a still target was aimed off-axis: {straight.vel}"

    led = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=0.0, lead=(0.0, 600.0))
    assert led.vel[1] > 0.0, "the shot was not led toward where the target is heading"

    # A led straight shot connects with a target moving steadily.
    def steady(t: float):
        return (900.0, 400.0 + 600.0 * t)

    shot = _Projectile((100.0, 400.0), (900.0, 400.0), "trap", homing=0.0, lead=(0.0, 600.0))
    outcome, _ = fly_path(shot, steady)
    assert outcome == "hit", f"a led shot missed a steadily moving pointer: {outcome}"


def check_world_defaults_to_straight() -> None:
    world = MouseWebWorld(1920, 1080, can_control=False)
    assert world.shoot((100.0, 400.0), (900.0, 400.0)) is True
    assert world.projectile.homing == 0.0, "the world fires a homing shot by default"

    world.clear()
    assert world.shoot((100.0, 400.0), (900.0, 400.0), homing=1.0) is True
    assert world.projectile.homing == 1.0


def check_fly_glob_matches() -> None:
    shooter = SimpleNamespace(x=100.0, y=400.0, heading=0.0, size=12.0)
    fly = SimpleNamespace(x=900.0, y=400.0, vx=0.0, vy=900.0, alive=True, eaten=False,
                          dragging=False, trapped=False)

    plain = WebShotProjectile(shooter, fly, "trap", homing=0.0)
    led = math.atan2(plain.vel[1], plain.vel[0])
    assert led > 0.01, "the fly glob was not led toward where the fly is going"

    straight_at_still = WebShotProjectile(
        shooter,
        SimpleNamespace(x=900.0, y=400.0, vx=0.0, vy=0.0, alive=True, eaten=False,
                        dragging=False, trapped=False),
        "trap",
        homing=0.0,
    )
    assert abs(straight_at_still.vel[1]) < 1e-6, "a shot at a hovering fly was aimed off-axis"
    assert straight_at_still.homing == 0.0

    # Flight, not just aim. The first version of this check only looked at the
    # constructor, so it passed with the in-flight correction left switched on.
    def dodging_fly():
        return SimpleNamespace(x=900.0, y=400.0, vx=0.0, vy=0.0, alive=True,
                               eaten=False, dragging=False, trapped=False)

    def closest(homing: float) -> float:
        target = dodging_fly()
        shot = WebShotProjectile(shooter, target, "trap", homing=homing)
        nearest = float("inf")
        for step in range(400):
            # The fly bolts once the glob is committed.
            if step * DT > 0.03:
                target.vy = 520.0
                target.y += target.vy * DT
            shot.update(DT)
            nearest = min(nearest, math.dist(shot.pos, (target.x, target.y)))
            if shot.done:
                break
        return nearest

    straight_near = closest(0.0)
    tracking_near = closest(1.0)
    assert tracking_near < straight_near * 0.7, (
        f"the fly glob did not follow its target when tracking was granted: "
        f"closest {tracking_near:.0f}px against {straight_near:.0f}px straight"
    )


def main() -> int:
    check_ability_exists()
    check_straight_shot_misses_a_dodge()
    check_straight_shot_still_hits_a_still_target()
    check_lead_is_applied()
    check_world_defaults_to_straight()
    check_fly_glob_matches()
    print("straight shot smoke: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
