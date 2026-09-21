"""A base is a pile of dirt mounds, built one at a time (DC-41).

A base used to draw as a ring of nodes that turned on the spot, its rotation
driven by the same `patrol_angle` that moves the guard. It read as a progress
indicator rather than as something being built. It is now a cluster of dirt
mounds: each one is finished before the next is started, they are placed once
and stay where they were piled, and nothing about them rotates.

Placement is derived from the site's own id rather than saved, so these tests
lean on that: the same base must lay its earth out identically in a fresh
process, which is what makes the pile survive a restart.
"""

from __future__ import annotations

import re

from desktop_bug.world.jobs import (
    MAX_BUILD_PROGRESS,
    MOUNDS_PER_BASE,
    BaseSite,
)
from support import ROOT

JOBS_SRC = (ROOT / "src" / "desktop_bug" / "world" / "jobs.py").read_text(encoding="utf-8")


def _site(site_id: str = "demo|1", completion: float = 1.0, level: int = 0) -> BaseSite:
    return BaseSite(
        id=site_id, owner_id="owner", team_id="hunters", x=400.0, y=300.0,
        build_progress=MAX_BUILD_PROGRESS * completion, level=level,
    )


def test_a_finished_base_is_a_full_pile_and_an_unstarted_one_is_bare():
    assert _site(completion=0.0).mounds() == []
    assert len(_site(completion=1.0).mounds()) == MOUNDS_PER_BASE


def test_mounds_are_finished_one_at_a_time():
    """The plan's acceptance: finished mounds match progress, one in progress."""
    site = _site(completion=0.5)
    mounds = site.mounds()
    assert len(mounds) == MOUNDS_PER_BASE // 2
    # Every mound but the last is complete; the last is the one being piled.
    assert all(built == 1.0 for *_rest, built in mounds[:-1])

    part = _site(completion=0.54)
    built_values = [built for *_rest, built in part.mounds()]
    assert built_values.count(1.0) == 6, built_values
    assert 0.0 < built_values[-1] < 1.0, built_values


def test_progress_only_adds_mounds_and_never_moves_one():
    """Earth already piled stays where it was piled."""
    site = _site(completion=0.25)
    early = [(round(x, 6), round(y, 6)) for x, y, *_rest in site.mounds()]
    site.build_progress = MAX_BUILD_PROGRESS
    later = [(round(x, 6), round(y, 6)) for x, y, *_rest in site.mounds()]
    assert later[:len(early)] == early
    assert len(later) > len(early)


def test_nothing_about_a_mound_rotates():
    """`patrol_angle` drives the guard, and must no longer drive the base."""
    site = _site(completion=1.0)
    before = site.mounds()
    for step in range(12):
        site.patrol_angle = step * 0.5
        assert site.mounds() == before, "a mound moved when the patrol angle turned"


def test_the_renderer_no_longer_reads_the_patrol_angle():
    """The rotating element is gone from the drawing code, not merely slowed."""
    render_src = JOBS_SRC[JOBS_SRC.index("    def render(self, painter"):]
    assert "patrol_angle" not in render_src, (
        "the base renderer still reads patrol_angle, so something still turns"
    )


def test_the_same_base_piles_its_earth_in_the_same_place_every_time():
    """Stability across instances is what lets the pile survive a restart.

    A hash-based layout would pass inside one process and quietly re-place
    every mound on the next launch, because `hash()` of a string is salted
    per process.
    """
    first = _site("team:hunters|base-7", completion=1.0).mounds()
    second = _site("team:hunters|base-7", completion=1.0).mounds()
    assert first == second
    assert _site("team:rivals|base-7", completion=1.0).mounds() != first


def test_the_layout_is_seeded_from_the_id_rather_than_hashed():
    """Guards the reason for the rule above, not just today's behaviour."""
    plan = JOBS_SRC[JOBS_SRC.index("def _plan_mounds"):]
    plan = plan[:plan.index("\n\n\n")]
    assert "random.Random(f\"{site_id}" in plan, plan[:400]
    # The docstring explains why a hash is wrong, so only the code is checked.
    body = plan.split('"""')[2]
    assert not re.search(r"\bhash\(", body), "a salted hash would not survive a restart"


def test_a_level_up_enlarges_the_pile_instead_of_rearranging_it():
    small = _site(completion=1.0, level=0)
    large = _site(completion=1.0, level=4)
    assert large.radius > small.radius

    def shape(site):
        return [
            (round((x - site.x) / site.radius, 6), round((y - site.y) / site.radius, 6))
            for x, y, *_rest in site.mounds()
        ]

    assert shape(small) == shape(large), "levelling up moved the mounds instead of growing them"
    assert [round(m[2] / small.radius, 6) for m in small.mounds()] == \
           [round(m[2] / large.radius, 6) for m in large.mounds()]


def test_a_finished_pile_stays_inside_the_base_ring():
    """A base must not spill its earth outside the territory it claims."""
    for level in range(6):
        site = _site(completion=1.0, level=level)
        for x, y, size, aspect, _built in site.mounds():
            reach = ((x - site.x) ** 2 + (y - site.y) ** 2) ** 0.5 + size * max(1.0, aspect)
            assert reach <= site.radius, (level, reach, site.radius)
