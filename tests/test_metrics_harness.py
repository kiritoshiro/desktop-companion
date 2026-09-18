"""The behaviour metrics harness reports sane, real numbers (DC-24).

`metrics.py` is a script, not a test module (it takes no `test_` prefix, so
pytest does not collect it -- the same convention as `movement.py` and
`support.py`). This file exercises its accumulator logic directly and runs
a short, real colony simulation to check the numbers it reports are
internally consistent, without pinning down the exact behaviour values
DC-18 is going to change.
"""

from __future__ import annotations

import math

import pytest

from support import ROOT

from metrics import DT, SpiderMetrics, run_metrics

COLONY = ROOT / "presets" / "colony.json"


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a manager constructs Creatures, which need a QApplication."""


class _FakeCreature:
    def __init__(self, index, job_id, state, job_mode):
        self.index = index
        self.job_id = job_id
        self.state = state
        self.job_mode = job_mode
        self.personality = {"id": "test"}


def test_state_distribution_sums_to_one_after_observing_several_frames():
    metrics = SpiderMetrics(_FakeCreature(0, "none", "Idle", "idle"))
    for state in ("Idle", "Idle", "Chase", "Idle", "Aim"):
        metrics.observe(_FakeCreature(0, "none", state, "idle"), DT)
    total = sum(metrics.state_distribution().values())
    assert math.isclose(total, 1.0, rel_tol=1e-9)


def test_job_on_duty_fraction_is_none_without_a_job():
    metrics = SpiderMetrics(_FakeCreature(0, "none", "Idle", "idle"))
    metrics.observe(_FakeCreature(0, "none", "Idle", "idle"), DT)
    assert metrics.job_on_duty_fraction() is None


def test_job_on_duty_fraction_reflects_idle_time():
    creature = _FakeCreature(0, "builder", "Idle", "idle")
    metrics = SpiderMetrics(creature)
    for mode in ("build", "build", "build", "idle"):
        metrics.observe(_FakeCreature(0, "builder", "JobBuild", mode), DT)
    assert metrics.job_on_duty_fraction() == pytest.approx(0.75)


def test_social_entries_count_transitions_not_frames():
    metrics = SpiderMetrics(_FakeCreature(0, "none", "Idle", "idle"))
    # Three consecutive Play frames is one interaction, not three.
    for state in ("Idle", "Play", "Play", "Play", "Idle", "Cuddle"):
        metrics.observe(_FakeCreature(0, "none", state, "idle"), DT)
    assert metrics.social_entries == 2


def test_longest_run_excludes_feed_and_weave():
    metrics = SpiderMetrics(_FakeCreature(0, "none", "Idle", "idle"))
    # A long Feed run should not win against a shorter Chase run.
    for state in ("Feed",) * 100 + ("Chase",) * 10:
        metrics.observe(_FakeCreature(0, "none", state, "idle"), DT)
    metrics.finish(DT)
    assert metrics.longest_run_seconds == pytest.approx(10 * DT)


def test_the_harness_is_not_tied_to_one_specific_preset():
    result = run_metrics(ROOT / "presets" / "default.json", minutes=0.05, seed=1)
    assert result["spiders"]
    assert result["duration_seconds"] > 0.0


def test_a_real_colony_run_produces_internally_consistent_numbers():
    result = run_metrics(COLONY, minutes=0.5, seed=20260918, warmup_seconds=0.5)

    assert result["spiders"], "the colony preset has no spiders to measure"
    for spider in result["spiders"].values():
        total = sum(spider["state_distribution"].values())
        assert math.isclose(total, 1.0, rel_tol=1e-6), spider
        assert spider["social_interactions_per_minute"] >= 0.0
        assert spider["longest_run_seconds"] >= 0.0
        duty = spider["job_on_duty_fraction"]
        if spider["job_id"] == "none":
            assert duty is None
        else:
            assert 0.0 <= duty <= 1.0

    for site in result["bases"].values():
        assert 0 <= site["level"]
        assert 0.0 <= site["build_progress"] <= 500.0

    assert result["frame_mean_ms"] > 0.0
    # A generous ceiling: this only catches something grossly broken (an
    # infinite loop, an O(n^2) blow-up), not a real regression -- that is
    # tools/benchmark.py's job, against its own committed baseline.
    assert result["frame_mean_ms"] < 50.0
