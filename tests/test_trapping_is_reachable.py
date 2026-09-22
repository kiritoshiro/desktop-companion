"""A spider a person can actually configure must be able to throw silk.

DC-45 taught a fighting spider to pin its enemy with silk first, proved it
with a duel, and shipped. Watching a real colony afterwards, it never once
happened -- and the reason was not the cooldown that was recorded as the
suspect at the time.

Of twenty-one temperaments, only the legacy "Trapper" grants ``shoot_web``,
and no job granted it at all. Every one of the thirty-six combinations of the
six temperaments the settings window offers and the six jobs produced a
spider that could never throw silk, so DC-45's headline behaviour was
reachable only by hand-ticking the ability per slot. The duel test passed the
whole time because it enables the skill explicitly.

That is the gap these tests close: not "the mechanism works" -- DC-45 covers
that -- but "an ordinary spider can reach it".
"""

from __future__ import annotations

import json
from pathlib import Path

from desktop_bug.content.skills import default_ability_ids
from desktop_bug.world.jobs import JOB_IDS, job_ability_ids

ROOT = Path(__file__).resolve().parents[1]
SILK = {"shoot_web", "wall_web"}


def _temperaments() -> dict[str, dict]:
    return {p.stem: json.loads(p.read_text(encoding="utf-8"))
            for p in sorted((ROOT / "personalities").glob("*.json"))}


def test_at_least_one_job_grants_silk():
    """The regression in one line: before this, none did."""
    granting = [j for j in JOB_IDS if SILK & set(job_ability_ids(j))]
    assert granting, (
        "no job grants shoot_web or wall_web, so a spider can only throw silk "
        "if someone hand-ticks the ability for its slot"
    )


def test_the_silk_specialist_can_throw_silk():
    """Web tender could weave a web but not shoot one, which is incoherent."""
    abilities = set(job_ability_ids("webber"))
    assert "weave_web" in abilities, abilities
    assert "shoot_web" in abilities, (
        "the Web tender job does not grant shoot_web", abilities,
    )


def test_a_guard_can_pin_an_intruder():
    """Guards are the spiders that actually meet foes, so this is where
    the owner would see trapping happen."""
    assert "shoot_web" in set(job_ability_ids("guard")), job_ability_ids("guard")


def test_a_default_colony_spider_can_throw_silk():
    """The real check: walk every temperament against every job.

    Written as a sweep rather than a spot check because the gap was only
    visible as a sweep -- each individual combination looked like a
    reasonable spider, and the absence was in the whole grid.
    """
    reachable = []
    for name, traits in _temperaments().items():
        base = set(default_ability_ids(traits))
        for job in JOB_IDS:
            if SILK & (base | set(job_ability_ids(job))):
                reachable.append(f"{name}+{job}")
    assert reachable, (
        "no temperament and job combination produces a spider that can throw "
        "silk; DC-45's pinning is unreachable without editing abilities by hand"
    )
    # And specifically through a job, so it does not depend on picking the one
    # legacy temperament that happens to carry it.
    via_job = [c for c in reachable if not (SILK & set(default_ability_ids(
        _temperaments()[c.split("+")[0]])))]
    assert via_job, "silk is reachable only by temperament, never by choosing a job"


def test_choosing_a_job_does_not_quietly_remove_an_ability():
    """A job adds to the temperament's defaults; it must not replace them."""
    traits = _temperaments()["trapper"]
    base = set(default_ability_ids(traits))
    assert SILK & base, "the Trapper temperament no longer grants silk"
    for job in JOB_IDS:
        combined = base | set(job_ability_ids(job))
        assert base <= combined, (job, sorted(base - combined))
