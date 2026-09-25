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
# DC-22: how a hit resolves, and what being knocked out costs.
# Armour subtracts from a hit but never cancels it: a maxed-out defender
# that a low-level attacker literally cannot scratch reads as a bug rather
# than as toughness, so this fraction always lands.
MIN_DAMAGE_FRACTION = 0.15
# DC-47: a beaten spider dies rather than being knocked out. This replaces
# the knock-out and respawn DC-22 shipped with -- see the decision record for
# why the earlier "never permanent death" constraint was reversed. What is
# left behind, and how long it lasts, belongs to `world/carcass.py`.


# DC-45: silk landed on a spider by another spider. Long enough that
# trapping is worth a shot and the shooter can close, short enough that
# being pinned is a setback rather than a sentence.
WEBBED_SECONDS = 4.6
WEBBED_SPEED_MULT = 0.35
# DC-50: for the first part of that, silk *holds* rather than slows. Watching
# it run, a merely-slowed spider still walked away looking unbothered, so the
# skill read as nothing at all -- the owner reported that webbing a spider did
# not immobilise it the way webbing the pointer does. A held spider cannot
# translate; it can still turn, flinch and be attacked, so it struggles in
# place instead of freezing like a statue.
WEBBED_HOLD_SECONDS = 2.2
# Struggling against silk (web_net.py): bursts of STRUGGLE_DUTY of every
# STRUGGLE_PERIOD seconds, each second of a burst wearing STRUGGLE_DRAIN extra
# seconds off the pin. Struggling throughout frees a spider in about 3.1 s
# instead of 4.6.
STRUGGLE_PERIOD = 0.55
STRUGGLE_DUTY = 0.62
STRUGGLE_DRAIN = 0.75

# ---------------------------------------------------------------------------
# DC-50: nerve. A spider that fights to the death regardless of the odds reads
# as a machine, and the colony grinds itself down to nothing in a few minutes.
# ---------------------------------------------------------------------------
# Below this fraction of max hp, a spider breaks off and runs.
FLEE_HEALTH_FRACTION = 0.35
# It stops running once patched back up to here, so it does not yo-yo in and
# out of a fight at the threshold.
RALLY_HEALTH_FRACTION = 0.72
# Foes within this radius are counted when deciding whether it is outnumbered.
THREAT_SCAN_RADIUS = 260.0
# Strictly greater than this many foes for each friend nearby, and it withdraws
# rather than engaging. Two-on-one is a fight worth avoiding; one-on-one is not.
OUTNUMBERED_RATIO = 1.5
# Fleeing is a sprint, not a stroll.
FLEE_SPEED_MULT = 1.45

# ---------------------------------------------------------------------------
# DC-64: a retreat has to end.
#
# The owner, watching a colony: *"after initiating runing after low health,
# they get stuck in that position where they runinng, and not stoping, so end
# up mostly to top right corner."*
#
# DC-50 gave a spider only one way out of a retreat: heal back up to
# RALLY_HEALTH_FRACTION. The one thing that heals is a base, so a spider whose
# team has not built one -- or that runs the wrong way -- can never satisfy it.
# It pins itself in a screen corner and runs on the spot forever. Measured
# before the fix: 7200 of 7200 frames fleeing over two minutes, ending 856px
# from a threat whose scan radius is 260.
#
# Escaping is the missing exit. A spider runs until it is *safe*, not until it
# is well, which is also what running is for.
#
# The radius is wider than the one that starts a retreat, for the same reason
# RALLY sits well above FLEE: leaving by the same line you entered by makes a
# spider oscillate on the boundary.
ESCAPED_RADIUS = THREAT_SCAN_RADIUS * 1.6
# ...and it has to stay clear for this long, so one frame of a foe clipping
# out of range does not call off a retreat mid-stride.
ESCAPED_SECONDS = 1.1

# Once safe but still hurt, a spider walks home to heal instead of resuming
# its rounds at 12% hp. This is the calm half of the same behaviour: no
# sprint, no panic, just somewhere better to be. It ends at RALLY_HEALTH_
# FRACTION, the same bar a retreat ends at.
RECOVER_ARRIVE_RADIUS = 40.0


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


# DC-56: a gait is a phase, not a fixed trait.
#
# The owner: *"it seems that now they inherit only one of those movement
# modes right? ... i think they should be a phases also the way they move, not
# just strickly one way so to make it more interesting they should choose one
# based on their goals to acheive it quicker. if curious then skittle, if
# runing or chasing then run."*
#
# So the temperament still sets a spider's baseline (DC-52), and what it is
# *doing* overrides it. A state absent from this map keeps the baseline,
# which is why Wander is not listed: it is the state a spider is in most of
# the time, and forcing it either way would erase the temperament entirely.
GAIT_BY_STATE = {
    # Looking into something. Short darting bursts with tiny pauses, which is
    # what "skitter" already is and what an interested spider actually does.
    "Inspect": "skitter",
    "Observe": "skitter",
    "Alert": "skitter",
    "WebWalk": "skitter",
    "WebApproach": "skitter",
    # Going somewhere, and meaning it. A continuous lifted stride rather than
    # burst-and-stop, because stopping is exactly what you do not do while
    # running from something.
    "Chase": "lively",
    "Approach": "lively",
    "Retreat": "lively",
    "Startled": "lively",
    "Zoom": "lively",
    "DriftRun": "lively",
    "Play": "lively",
}

# DC-59: what a fight looks like.
#
# The owner: *"next thing i want a good looking fight between spiders. how
# they move their legs and body as in an attack stance and focus, and so on."*
# Before this, two spiders fighting were two spiders running into each other:
# every number was right -- hits landed on a cooldown, silk pinned, the loser
# died -- and none of it was visible, because nothing about a fighting spider
# was drawn differently from a walking one.
#
# Four things, in the order they read on screen:
#
# 1. **Spacing.** A spider stops at arm's length instead of walking into its
#    foe. This is the standing "a fight is one clump of overlapping bodies"
#    complaint from the first real-hardware session.
# 2. **Stance.** Squared up: front legs raised and spread, body reared back
#    and turned to face, which is what a threatened tarantula actually does.
# 3. **Focus.** The head and eyes stay on the foe even while the body is
#    backing off or circling.
# 4. **Lunge and recoil.** A landed blow throws the attacker's body forward
#    over its planted feet and knocks the defender's back. The feet stay put,
#    so the legs stretch and compress -- which is the whole effect, and it is
#    free, because the legs are already solved in world space.

# How far apart two fighting spiders stand, as a multiple of their combined
# size. Below about 1.1 they overlap and the fight is a clump again.
COMBAT_SPACING = 1.35
# Hysteresis on that, so a spider at exactly the standoff distance does not
# shuffle in and out on alternate frames.
COMBAT_SPACING_SLACK = 0.22
# How quickly a spider squares up, and how quickly it drops the stance once
# the fight is over. Rising fast reads as reacting; falling slowly keeps it
# from flickering while a foe crosses in and out of range.
STANCE_RISE_PER_SECOND = 4.5
STANCE_FALL_PER_SECOND = 1.6
# How far a landed blow throws a body, as a fraction of the spider's size,
# and how fast that decays.
LUNGE_REACH = 0.42
RECOIL_REACH = 0.26
LUNGE_DECAY_PER_SECOND = 4.2
# A bite, start to finish (the owner: "when attacking it should show attacking
# movement, maybe the bite"). The front rises and the body draws back, then
# snaps forward past its feet with the front legs thrown at the target, then
# settles. Seconds.
STRIKE_WINDUP = 0.09
STRIKE_SNAP = 0.07
STRIKE_DURATION = 0.38
STRIKE_DRAW_BACK = 0.45     # of LUNGE_REACH, during the wind-up
STRIKE_OVERSHOOT = 1.6      # of LUNGE_REACH, at the end of the snap

# DC-62: how much a spider minds being picked up.
#
# The owner: *"mouse trying to catch it repeatedly would make him run away
# from mouse more often."* Being grabbed was entirely without consequence --
# a spider was startled for a second and then walked back into the pointer as
# happily as before, however many times it had been caught.
#
# `cursor_pressure` runs 0..1, rises on each grab, and bleeds away over about
# two minutes of being left alone, so a spider that was pestered and then
# ignored forgives.
CURSOR_PRESSURE_PER_GRAB = 0.34
CURSOR_PRESSURE_DECAY_PER_SECOND = 1.0 / 120.0
# Above this, the spider treats the pointer as a threat rather than as
# something interesting. Deliberately a threshold rather than a gradient: the
# whole behaviour model here is a table of per-situation multipliers, and a
# spider that is wary of the pointer is in a different situation, not in the
# same one by a smaller amount.
CURSOR_WARY_THRESHOLD = 0.5

# DC-63: an occasional change of step, so walking is not monotonous.
#
# The owner: *"to make more varied movements make them in pipline of common
# spider movement so that they could be changed randomly or based on activity
# they do."* DC-56 did the activity half. This is the random half: now and
# then a spider adopts the other gait for a few seconds.
#
# It applies **only where no activity has an opinion** -- a spider running
# from something does not stop to try a different walk. That ordering is the
# pipeline, and it is what keeps "random variety" from undoing "the way it
# moves means something".
GAIT_SPELL_GAP = (9.0, 26.0)
GAIT_SPELL_LENGTH = (1.8, 4.5)

VALID_GAIT_STYLES = ("classic", "lively", "skitter")
GAIT_LABELS = {
    "classic": "Classic",
    "lively": "Lively (lifted legs)",
    "skitter": "Skitter (rapid bursts)",
}


def normalize_gait_style(style: str | None) -> str:
    style = str(style or "classic").strip().lower()
    return style if style in VALID_GAIT_STYLES else "classic"
