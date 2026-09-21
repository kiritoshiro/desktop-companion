"""Small, dependency-free helpers and constants shared across the creature
package: no method here touches `self`.
"""

from __future__ import annotations

from ..support.math_utils import clamp

def smootherstep(t: float) -> float:
    t = clamp(t, 0.0, 1.0)
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)



# States the job layer drives directly. They are not part of the personality
# dispatch, so a spider left in one after its work intent clears would match no
# branch and keep its last speed and pause flag forever.
JOB_STATES = (
    "JobTravel", "JobBuild", "JobPatrol", "JobGuardAlert",
    # DC-20: Scout, Webber and Hunter job states.
    "JobScoutTravel", "JobScoutReport",
    "JobWebTravel", "JobWebRepair", "JobWebWeave",
    "JobHuntPatrol", "JobHuntReturn",
)

# Data-driven job-mode -> state mapping (DC-18, C6): the single place a new
# job mode registers its state. ``_update_job_state`` used to gate itself on
# ``self.job_id not in ("builder", "guard")`` before doing anything else, so
# a future job (a Scout roam state, say) that forgot to add its id to that
# tuple would return early *before* reaching the hand-back call at all, and a
# leftover job state would then match no branch in ``_update_state`` and
# freeze forever -- the exact bug this finding warns about. Keying off
# ``job_mode`` through this table instead removes the id gate entirely:
# ``_update_job_state`` runs unconditionally every tick for every creature,
# looks up whatever mode is currently published, and calls
# ``_release_job_state()`` for anything the table does not recognise. A new
# job mode is one entry here (plus a matching state name in JOB_STATES above);
# there is no separate hand-back call left to forget.
JOB_MODE_STATES = {
    "build_travel": "JobTravel",
    "build": "JobBuild",
    "patrol": "JobPatrol",
    "guard_alert": "JobGuardAlert",
    # DC-20. "hunting" has no entry on purpose: a hunter mid-hunt is always
    # outranked by personality (see ``_job_outranked_by_personality``), so
    # that mode never needs a state of its own to move the creature -- see
    # ``jobs.py::BaseWorld._update_hunter``.
    "scout_travel": "JobScoutTravel",
    "scout_report": "JobScoutReport",
    "web_travel": "JobWebTravel",
    "web_repair": "JobWebRepair",
    "web_weave": "JobWebWeave",
    "hunt_patrol": "JobHuntPatrol",
    "hunt_return": "JobHuntReturn",
}

# Hunting: which states a locked-on spider must finish before joining/leaving
# a hunt. Shared between CreatureManager (target selection) and the creature's
# own BehaviourMixin._pursue_prey (DC-18: hunting execution moved onto the
# creature so the manager stops overwriting creature state from outside).
HUNT_COMMITTED_STATES = frozenset((
    "Aim", "Coil", "Jump", "Land", "Catch", "Feed", "Roll", "DriftRun",
    "Play", "Cuddle", "Inspect", "WebAim", "WebShot",
    "Weave", "WeaveApproach", "Repair", "RepairApproach",
))
# States the spider should be allowed to finish before it will pick up a hunt.
HUNT_BUSY_STATES = frozenset((
    "WebAim", "WebShot", "Weave", "WeaveApproach", "Repair", "RepairApproach",
))

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
