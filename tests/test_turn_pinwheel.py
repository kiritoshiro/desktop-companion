"""A tarantula turning on the spot does not wind its legs round its body (DC-77).

The owner, with a screenshot of one of their guard tarantulas: *"the spiders
look really werid. their legs are now just arks formed in a circle."*

It was not the leg drawing. A guard ends a patrol, drops to Idle, and turns
to a new heading with its feet planted. The tarantula's gait allowed the body
to rotate up to `support_turn_limit` = 1.2 rad (69 degrees) under a planted
foot before that foot had to step, at up to `max_body_turn_rate` = 8.0 rad/s
-- the top of the parser's range. Measured on the owner's own slot (seed 11):
heading -3 to +61 degrees in twenty frames, planted feet absorbing -68
degrees, so every leg swept the same way round the body.

At 0.35 rad and 4.0 rad/s, over four seeded runs of 2400 frames:

    worst rotation a planted foot absorbs   69 -> 20 deg
    worst left/right leg asymmetry           85 -> 59 deg (p99)
    steps per second, all eight legs       10.7 -> 12.5
    turn speed, 90th percentile              38 -> 45 deg/s
    largest heading change in one frame     7.1 -> 5.3 deg

The turn is not slower; the feet replant during it instead of the body
spinning over them, which is also what the reference note says a tarantula
does -- slow and deliberate, stepping.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest

from desktop_bug.manager import CreatureManager


def _guard_run(qapp, monkeypatch, seed=11, frames=1500):
    scratch = Path(tempfile.mkdtemp(prefix="dc77-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "guard.json"
    preset.write_text(json.dumps({
        "name": "guard",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": 1,
                   "slot_id": "a", "team": "hunters", "job": "guard"}],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, 1600, 900, seed=seed)
    manager.set_flies_enabled(False)
    spider = manager.creatures[0]
    worst_planted = 0.0
    turned = 0.0
    previous = spider.heading
    for _ in range(frames):
        manager.update(1.0 / 60.0, -1e5, -1e5)
        for leg in spider.legs:
            if not leg.stepping:
                worst_planted = max(worst_planted, abs(getattr(leg, "stance_turn", 0.0)))
        turned += abs(((spider.heading - previous + math.pi) % math.tau) - math.pi)
        previous = spider.heading
    return math.degrees(worst_planted), math.degrees(turned)


@pytest.mark.parametrize("seed", (11, 12))
def test_planted_feet_do_not_absorb_a_big_turn(qapp, monkeypatch, seed):
    worst, turned = _guard_run(qapp, monkeypatch, seed=seed)
    assert turned > 180.0, "the guard never turned, so this proves nothing"
    assert worst < 25.0, f"a planted foot absorbed {worst:.0f} degrees of body turn"
