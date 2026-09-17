"""Regression checks for Snowpuff-2's Drifter motion and leg envelope."""

import json
import math
import random
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from desktop_bug.creature import Creature


def main() -> None:
    model = json.loads((ROOT / "models" / "plush_snow_hybrid_2" / "model.json").read_text(encoding="utf-8"))
    personality = json.loads((ROOT / "personalities" / "drifter.json").read_text(encoding="utf-8"))
    random.seed(7)
    spider = Creature(model, personality, 2400, 1400, gait_style="skitter")
    spider.x, spider.y = 1200.0, 700.0
    spider.heading = spider.target_heading = 0.0
    spider._initialize_legs()

    # The fluffy shell is intentionally broad, so every socket must start on
    # its lateral edge and the neutral stance must remain visibly long.
    for leg in spider.legs:
        assert float(leg.definition.get("attach_side", 0.0)) >= 0.50
    min_stance_radius = min(
        math.hypot(leg.foot_x - spider.x, leg.foot_y - spider.y)
        for leg in spider.legs
    )
    assert min_stance_radius >= spider.size * 1.35, min_stance_radius

    # A Drifter can carry momentum through a curve, but it must not moonwalk
    # behind its own facing direction when the slide changes arc.
    spider.enter_drift_run("circle")
    minimum_forward_component = 1.0
    for _ in range(480):
        dt = 1.0 / 60.0
        spider._update_drift_run(dt, -1000.0, -1000.0)
        spider._move_body(dt)
        spider._update_legs(dt)
        speed = math.hypot(spider.vel_x, spider.vel_y)
        if speed > 35.0:
            fx, fy, _, _ = spider._basis()
            minimum_forward_component = min(
                minimum_forward_component,
                (spider.vel_x * fx + spider.vel_y * fy) / speed,
            )
    assert minimum_forward_component >= math.cos(math.radians(78.0)) - 1e-6, minimum_forward_component

    print(
        "OK: Drifter travel direction guarded and Snowpuff-2 stance envelope is long "
        f"(min_forward={minimum_forward_component:.3f}, min_radius={min_stance_radius:.1f})"
    )


if __name__ == "__main__":
    main()
