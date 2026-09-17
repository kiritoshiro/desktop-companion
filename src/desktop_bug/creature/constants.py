"""Small, dependency-free helpers and constants shared across the creature
package: no method here touches `self`.
"""

from __future__ import annotations

from ..math_utils import clamp

def smootherstep(t: float) -> float:
    t = clamp(t, 0.0, 1.0)
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)



# States the job layer drives directly. They are not part of the personality
# dispatch, so a spider left in one after its work intent clears would match no
# branch and keep its last speed and pause flag forever.
JOB_STATES = ("JobTravel", "JobBuild", "JobPatrol", "JobGuardAlert")

# Personality states that outrank colony work: fleeing, a jump already
# committed, eating, and silk that is partway through being made or thrown.
# A job is a shift, not ownership of the spider.
JOB_PREEMPTING_STATES = frozenset({
    "Retreat", "Startled",
    "Coil", "Jump", "Land", "Catch",
    "Feed",
    "WebAim", "WebShot",
    "WeaveApproach", "Weave", "RepairApproach", "Repair",
})


VALID_GAIT_STYLES = ("classic", "lively", "skitter")
GAIT_LABELS = {
    "classic": "Classic",
    "lively": "Lively (lifted legs)",
    "skitter": "Skitter (rapid bursts)",
}


def normalize_gait_style(style: str | None) -> str:
    style = str(style or "classic").strip().lower()
    return style if style in VALID_GAIT_STYLES else "classic"
