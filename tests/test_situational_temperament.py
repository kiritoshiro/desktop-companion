"""One temperament, many situations (DC-60 to DC-63).

The owner, proposing a redesign: *"the temperaments could also be a phases of
most of the spiders. some of them could be consolidated and left only a few
since they kinda look the same ... one temperament would be called balanced
which would be the default and it could do many things and have many
personalities depending if enemies near, if it is hunting or if it is curious
or friend nearby, or mouse trying to catch it repeatedly would make him run
away from mouse more often."*

Most of that machinery already existed and was half-wired.
`BehaviourPhaseScheduler` has always picked a behaviour phase weighted by the
personality's own scores **times a per-situation multiplier**, and the
situation ("focus") could be `none`, `cursor`, `prey`, `creature` or `web`.

What was missing:

* **No `foe`.** An enemy and a friend were both "creature", so a spider
  facing something trying to kill it was weighing up a cuddle. (DC-61)
* **No memory.** Being picked up cost nothing: a spider walked back into the
  pointer as happily the tenth time as the first. (DC-62)
* **No variety within a walk.** DC-56 made the gait follow the activity;
  nothing varied it otherwise. (DC-63)
* **Twenty-one temperaments in the menu** sharing eleven movement profiles
  between them. (DC-60)

The load-bearing design decision, and the owner's: the situation **tilts the
odds, it does not override**. A cautious spider still mostly backs off and a
bold one still mostly closes, because the multipliers are applied to each
temperament's own phase scores. That is what is checked at the bottom, and it
is the whole reason for keeping temperaments at all.
"""

from __future__ import annotations

import random

import pytest
from desktop_bug.content.discovery import discover_models, discover_personalities
from desktop_bug.content.personality_profiles import (
    PHASE_SCORE_PROFILES,
    phase_scores_for,
)
from desktop_bug.creature import Creature
from desktop_bug.creature.constants import (
    CURSOR_PRESSURE_PER_GRAB,
    CURSOR_WARY_THRESHOLD,
    GAIT_BY_STATE,
)
from desktop_bug.creature.phase_scheduler import FOCUS_PHASE_BIAS, normalize_focus
from support import ROOT


@pytest.fixture(scope="module")
def content(qapp):
    models, _ = discover_models(ROOT)
    personalities, _ = discover_personalities(ROOT)
    return models, personalities


def _spider(content, personality_id="balanced", **kwargs):
    models, personalities = content
    return Creature(models["tarantula"], personalities[personality_id], 800, 600, **kwargs)


# ------------------------------------------------------ DC-61: friend and foe

def test_a_foe_is_its_own_situation():
    assert "foe" in FOCUS_PHASE_BIAS
    assert "friend" in FOCUS_PHASE_BIAS
    assert normalize_focus("enemy") == "foe"
    assert normalize_focus("ally") == "friend"


def test_nobody_cuddles_a_foe():
    """They used to share a bias row with friends, which is exactly how two
    spiders fighting to the death ended up cuddling (see DC-59)."""
    assert FOCUS_PHASE_BIAS["foe"]["cuddle"] == 0.0
    assert FOCUS_PHASE_BIAS["foe"]["social_play"] == 0.0
    assert FOCUS_PHASE_BIAS["friend"]["cuddle"] > 1.0


def test_a_foe_pulls_towards_fighting_and_a_friend_away_from_it():
    foe, friend = FOCUS_PHASE_BIAS["foe"], FOCUS_PHASE_BIAS["friend"]
    assert foe["chase"] > friend["chase"]
    assert foe["prepare_jump_attack"] > friend["prepare_jump_attack"]
    assert foe["run_away"] > friend["run_away"]
    assert friend["social_play"] > foe["social_play"]


def test_a_spider_with_a_foe_reports_that_situation(content):
    me = _spider(content)
    them = _spider(content)
    me.x, me.y = 300.0, 300.0
    them.x, them.y = 340.0, 300.0
    assert me._phase_focus_context(-9000.0, -9000.0) == "none"
    me._foe = them
    assert me._phase_focus_context(-9000.0, -9000.0) == "foe"


def test_a_dead_foe_is_not_a_situation(content):
    me, them = _spider(content), _spider(content)
    me._foe = them
    them.dead = True
    assert me._phase_focus_context(-9000.0, -9000.0) != "foe"


def test_the_foe_situation_hands_back_no_mate(content):
    """`_phase_target` returning a mate for a foe is how a cuddle got a
    partner. A fight has a point to move to and nobody to socialise with."""
    me, them = _spider(content), _spider(content)
    me.x, me.y = 300.0, 300.0
    them.x, them.y = 380.0, 300.0
    me._foe = them
    tx, ty, mate = me._phase_target("foe", -9000.0, -9000.0)
    assert (tx, ty) == (them.x, them.y)
    assert mate is None


def test_the_stance_between_teams_decides_friend_or_foe(content):
    """Read from the declared relation, not from team equality: two teams can
    be declared allies, and a spider can carry a per-creature override."""
    me, them = _spider(content), _spider(content)
    me.set_team("hunters")
    them.set_team("hunters")
    assert me._social_focus_for(them) == "friend"
    them.set_team("rivals")
    me.team_stances = {"hunters": {"rivals": "foe"}}
    them.team_stances = me.team_stances
    assert me._social_focus_for(them) == "foe"


# --------------------------------------------------- DC-62: the mouse remembers

def test_being_grabbed_leaves_a_mark(content):
    spider = _spider(content)
    assert spider.cursor_pressure == 0.0
    assert spider.wary_of_cursor is False
    spider.start_drag(0.0, 0.0)
    assert spider.cursor_pressure == pytest.approx(CURSOR_PRESSURE_PER_GRAB)


def test_a_few_grabs_make_a_spider_wary(content):
    spider = _spider(content)
    grabs = 0
    while not spider.wary_of_cursor and grabs < 10:
        spider.start_drag(0.0, 0.0)
        spider.dragging = False
        grabs += 1
    assert 1 < grabs <= 3, grabs
    assert spider.cursor_pressure >= CURSOR_WARY_THRESHOLD


def test_a_wary_spider_sees_the_pointer_differently(content):
    spider = _spider(content)
    spider.x, spider.y = 400.0, 300.0
    near = (spider.x + 40.0, spider.y)
    assert spider._phase_focus_context(*near) == "cursor"
    spider.cursor_pressure = 1.0
    assert spider._phase_focus_context(*near) == "cursor_wary"


def test_wary_means_run_more_and_approach_less():
    calm, wary = FOCUS_PHASE_BIAS["cursor"], FOCUS_PHASE_BIAS["cursor_wary"]
    assert wary["run_away"] > calm["run_away"] * 3.0
    assert wary["approach"] < calm["approach"]
    assert wary["chase"] < calm["chase"]
    # Wary, not panicked: it still watches the thing.
    assert wary["observe"] > calm["observe"]


def test_a_spider_left_alone_forgives(content):
    spider = _spider(content)
    spider.cursor_pressure = 1.0
    for _ in range(int(200.0 * 60)):
        spider.update(1.0 / 60.0, -9000.0, -9000.0, 800, 600)
    assert spider.cursor_pressure == 0.0
    assert spider.wary_of_cursor is False


def test_it_does_not_forgive_while_still_being_held(content):
    """A long drag must not end with the spider calmer than it started."""
    spider = _spider(content)
    spider.start_drag(0.0, 0.0)
    before = spider.cursor_pressure
    for _ in range(int(30.0 * 60)):
        spider.update(1.0 / 60.0, 0.0, 0.0, 800, 600)
    assert spider.cursor_pressure == pytest.approx(before)


# -------------------------------------------------- DC-63: the movement pipeline

def test_the_pipeline_runs_most_specific_last(content):
    """Temperament, then a random spell, then the activity. The activity
    wins, because what a spider is doing has to beat what it fancies."""
    spider = _spider(content, gait_style="lively")
    spider.state = "Wander"
    spider.gait_spell = None
    assert spider.effective_gait_style() == "lively"
    spider.gait_spell = "skitter"
    assert spider.effective_gait_style() == "skitter"
    spider.state = "Chase"
    assert spider.effective_gait_style() == GAIT_BY_STATE["Chase"]


def test_a_spell_actually_happens(content):
    spider = _spider(content, gait_style="lively")
    spider.state = "Idle"
    seen = set()
    for _ in range(int(120.0 * 60)):
        spider.update(1.0 / 60.0, -9000.0, -9000.0, 800, 600)
        if spider.state not in GAIT_BY_STATE:
            seen.add(spider.effective_gait_style())
    assert seen == {"lively", "skitter"}, seen


def test_the_spell_has_its_own_random_stream(content):
    """Not `self.rng`. Drawing from the shared stream -- even once, in
    __init__ -- shifts every later value in it, and a seeded run is supposed
    to replay exactly. `test_tarantula.py` caught this by failing a leg
    geometry invariant purely because the walk began from a different phase
    seed."""
    spider = _spider(content)
    assert spider._gait_rng is not spider.rng
    assert isinstance(spider._gait_rng, random.Random)


def test_classic_opts_out_of_the_whole_pipeline(content):
    spider = _spider(content, gait_style="classic")
    for _ in range(int(120.0 * 60)):
        spider.update(1.0 / 60.0, -9000.0, -9000.0, 800, 600)
        assert spider.effective_gait_style() == "classic"
        assert spider.gait_spell is None


# ------------------------------------------- the point of keeping temperaments

def test_the_same_situation_moves_two_temperaments_in_opposite_directions():
    """The owner chose "tilt the odds" over "override everything", and this
    is what that buys: one `foe` row, two opposite outcomes, because the
    multipliers apply to each temperament's own scores.

    If this ever fails, situations have started dictating behaviour and every
    temperament looks the same in a fight -- which is the failure mode the
    "override" option was rejected for.
    """
    foe = FOCUS_PHASE_BIAS["foe"]
    bold = PHASE_SCORE_PROFILES["bold"]
    timid = PHASE_SCORE_PROFILES["escape"]

    def weight(profile, phase):
        return profile.get(phase, 3) * foe[phase]

    assert weight(bold, "chase") > weight(bold, "run_away")
    assert weight(timid, "run_away") > weight(timid, "chase")


def test_balanced_is_the_default_and_does_a_bit_of_everything(content):
    """"one temperament would be called balanced which would be the default
    and it could do many things"."""
    _models, personalities = content
    balanced = personalities["balanced"]
    scores = phase_scores_for(balanced)
    # Nothing at zero: whatever the situation asks for, balanced can offer it.
    assert all(score > 0.0 for score in scores.values()), scores
    # And nothing dominant enough to make it a specialist.
    assert max(scores.values()) < 9.0, scores
