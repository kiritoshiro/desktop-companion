"""A thrown spider must slide to a stop, not run on under its own power.

Friction already carried a throw for about a second. What followed was the
problem: `enter_startled` set a target up to 300 px further along the throw
heading, so once the slide ended the spider *walked* the rest of the way at
74 px/s for several more seconds. It read as a spider running away in the
direction it was thrown rather than as a spider being thrown.
"""

import json
import math
import os
import random
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.creature import Creature  # noqa: E402

DT = 1.0 / 60.0
SCREEN = (2400, 1400)
# Far enough that the cursor cannot provoke a reaction and confuse the measurement.
CURSOR = (-100000.0, -100000.0)


def load(name: str = "mellow") -> tuple[dict, dict]:
    with (ROOT / "models" / "tarantula" / "model.json").open(encoding="utf-8") as handle:
        model = json.load(handle)
    with (ROOT / "personalities" / f"{name}.json").open(encoding="utf-8") as handle:
        personality = json.load(handle)
    return model, personality


def throw(spider: Creature, vx: float, vy: float) -> None:
    """Reproduce a release with hand velocity, the way the manager does."""
    spider.dragging = True
    spider.drag_vel_x = vx
    spider.drag_vel_y = vy
    spider.release_drag(*CURSOR)


def simulate(seconds: float, spider: Creature) -> list[tuple[float, float, float, str, float]]:
    """Return per-frame (t, x, y, state, inertia) samples."""
    samples = []
    steps = int(seconds / DT)
    for step in range(steps):
        spider.update(DT, CURSOR[0], CURSOR[1], *SCREEN)
        samples.append((step * DT, spider.x, spider.y, spider.state, spider.inertia_timer))
    return samples


def check_slide_decelerates() -> None:
    random.seed(11)
    model, personality = load()
    spider = Creature(model, personality, *SCREEN, index=0, progression_id="throw:0")
    spider.x, spider.y = 400.0, 700.0
    spider._initialize_legs()
    throw(spider, 900.0, 0.0)

    assert spider.inertia_timer > 0.0, "a hard throw produced no inertia at all"
    samples = simulate(3.0, spider)

    # While sliding, each frame must cover less ground than the one before.
    sliding = [s for s in samples if s[4] > 0.0]
    assert len(sliding) > 8, f"the slide lasted {len(sliding)} frames"
    steps = [
        math.dist((sliding[i][1], sliding[i][2]), (sliding[i - 1][1], sliding[i - 1][2]))
        for i in range(1, len(sliding))
    ]
    # Allow a little noise from the drift flourish, but the trend must be down.
    first_third = sum(steps[: len(steps) // 3]) / max(1, len(steps) // 3)
    last_third = sum(steps[-len(steps) // 3:]) / max(1, len(steps) // 3)
    assert last_third < first_third * 0.5, f"the slide did not decelerate: {first_third:.2f} -> {last_third:.2f}"

    # And it must travel roughly the way it was thrown.
    dx = sliding[-1][1] - sliding[0][1]
    dy = sliding[-1][2] - sliding[0][2]
    assert dx > 60.0, f"the throw did not carry the spider forward: dx={dx:.1f}"
    assert abs(dy) < abs(dx), f"the spider went sideways more than forward: dx={dx:.1f} dy={dy:.1f}"


def distance_after_slide(seed: int, seconds: float = 6.0, speed: float = 900.0,
                         cursor=None) -> float:
    """Ground covered after friction has finished: the thing being fixed.

    The cursor defaults to the point of release, because that is where a hand
    actually leaves it. Measuring with the cursor parked off screen hides the
    problem: the spider then has nothing to react to and stops anyway.
    """
    cursor = cursor if cursor is not None else (400.0, 700.0)
    random.seed(seed)
    model, personality = load()
    spider = Creature(model, personality, *SCREEN, index=0, progression_id="throw:1")
    spider.x, spider.y = 400.0, 700.0
    spider._initialize_legs()
    spider.dragging = True
    spider.drag_vel_x, spider.drag_vel_y = speed, 0.0
    spider.release_drag(*cursor)

    travelled = 0.0
    previous = None
    for _ in range(int(seconds / DT)):
        spider.update(DT, cursor[0], cursor[1], *SCREEN)
        if spider.inertia_timer <= 0.0:
            if previous is None:
                previous = (spider.x, spider.y)
            else:
                travelled += math.dist(previous, (spider.x, spider.y))
                previous = (spider.x, spider.y)
    return travelled


SEEDS = (7, 11, 23, 42, 99)


def check_does_not_run_on() -> None:
    """The worst case is what a user notices, so bound that rather than a mean.

    Before the fix, `enter_startled` projected a target up to 300 px further
    along the throw heading and the spider walked there once the slide ended.
    Measured over these seeds with the cursor at the release point, the worst
    case was 245 px of walking after the slide; afterwards it is under 70.
    Outcomes vary by seed because whether the spider finishes near enough to
    the cursor to retreat is itself random, so the bound is generous.
    """
    travelled = [distance_after_slide(seed) for seed in SEEDS]
    worst = max(travelled)
    # Deliberately loose. The precise assertion is check_no_projected_target
    # below; this one only catches a gross regression, because how far the
    # spider goes also depends on whether it happens to finish near enough to
    # the cursor to retreat, and that is random.
    assert worst < 150.0, (
        f"after the slide the spider still covered up to {worst:.0f}px "
        f"(per seed: {[f'{v:.0f}' for v in travelled]}); it is running on rather "
        "than sliding to a stop"
    )


def check_no_projected_target() -> None:
    """The precise change: a throw must not aim the spider anywhere.

    This is what actually caused the run-on, and unlike the distance
    measurement it does not depend on any later random choice.
    """
    for seed in SEEDS:
        random.seed(seed)
        model, personality = load()
        spider = Creature(model, personality, *SCREEN, index=0, progression_id="throw:5")
        spider.x, spider.y = 400.0, 700.0
        spider._initialize_legs()
        spider.dragging = True
        spider.drag_vel_x, spider.drag_vel_y = 900.0, 0.0
        spider.release_drag(400.0, 700.0)

        aimed = math.dist((spider.x, spider.y), (spider.target_x, spider.target_y))
        assert aimed < 1.0, (
            f"a throw aimed the spider {aimed:.0f}px further along the throw heading; "
            "friction alone should carry it"
        )
        assert spider.throw_recovery > 0.0, "a throw scheduled no recovery beat"


def check_recovery_beat_is_held() -> None:
    random.seed(3)
    model, personality = load()
    spider = Creature(model, personality, *SCREEN, index=0, progression_id="throw:2")
    spider.x, spider.y = 400.0, 700.0
    spider._initialize_legs()
    throw(spider, 850.0, 0.0)
    assert spider.throw_recovery > 0.0, "a throw scheduled no recovery beat"

    held = None
    for _ in range(int(3.0 / DT)):
        spider.update(DT, CURSOR[0], CURSOR[1], *SCREEN)
        if spider.inertia_timer <= 0.0 and spider.throw_recovery > 0.0 and held is None:
            held = (spider.x, spider.y)
        if spider.inertia_timer <= 0.0 and spider.throw_recovery <= 0.0:
            break
    assert held is not None, "the recovery beat was never entered"
    settled = math.dist(held, (spider.x, spider.y))
    assert settled < 12.0, f"the spider drifted {settled:.1f}px during the recovery beat"


def check_gentle_drop_still_scurries() -> None:
    """Only a real throw changes behaviour; a drop keeps the old startle.

    Two thresholds matter and they are not the same. Any release above 1 px/s
    carries a little inertia, but only one above 60 px/s counts as a throw and
    earns the recovery beat. A drop in between must still scurry away.
    """
    random.seed(5)
    model, personality = load("skittish")

    # Placed still, with no measurable hand movement at all.
    spider = Creature(model, personality, *SCREEN, index=0, progression_id="throw:3")
    spider.x, spider.y = 1200.0, 700.0
    spider._initialize_legs()
    throw(spider, 0.5, 0.0)
    assert spider.inertia_timer == 0.0, "a still release should not create inertia"
    assert spider.throw_recovery == 0.0, "a still release should not schedule a recovery beat"
    assert math.dist((spider.x, spider.y), (spider.target_x, spider.target_y)) > 40.0, (
        "a dropped spider was given nowhere to scurry to"
    )

    # A nudge: enough for a little inertia, not enough to be a throw.
    nudged = Creature(model, personality, *SCREEN, index=1, progression_id="throw:4")
    nudged.x, nudged.y = 1200.0, 700.0
    nudged._initialize_legs()
    throw(nudged, 30.0, 0.0)
    assert nudged.inertia_timer > 0.0, "a nudge should carry some inertia"
    assert nudged.throw_recovery == 0.0, "a nudge is not a throw and needs no recovery beat"
    assert math.dist((nudged.x, nudged.y), (nudged.target_x, nudged.target_y)) > 40.0, (
        "a nudged spider was given nowhere to scurry to"
    )


def main() -> int:
    check_slide_decelerates()
    check_does_not_run_on()
    check_no_projected_target()
    check_recovery_beat_is_held()
    check_gentle_drop_still_scurries()
    worst = max(distance_after_slide(seed) for seed in SEEDS)
    print(f"throw slide smoke: OK (worst case {worst:.0f}px travelled after the slide)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
