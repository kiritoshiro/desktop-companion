"""A gait is a phase, skills unlock themselves, and a palette is one choice.

Three of the owner's requests, each small and each in a different place.

**The gait (DC-56).** *"it seems that now they inherit only one of those
movement modes right? ... they should be a phases also the way they move, not
just strickly one way ... if curious then skittle, if runing or chasing then
run."* DC-52 had just made the temperament choose a spider's gait once, for
life. This makes that the baseline and lets what the spider is doing override
it.

**The skills (DC-57).** *"and the skills unlocks as they level up."* The
ability tree, the level gates and the prerequisites all already existed; the
missing part was anyone to spend the points. A skill point was banked on every
level and sat there unless a person opened the inspector and clicked, which no
spider in a colony of five was ever going to get.

**The palettes (DC-58).** *"lets create also some presets and imbed them also
in color picker in the main menu."* Also what a base draws from when it raises
a spider, which is why they are derived from one hue rather than listed as
seven free colours: "random colours" done per key produces a spider with a
green body, pink legs and orange feet.
"""

from __future__ import annotations

import json
import random
from pathlib import Path

import pytest
from desktop_bug.content.discovery import discover_models, discover_personalities
from desktop_bug.content.palettes import (
    NAMED_PALETTES,
    PALETTE_BY_ID,
    PALETTE_KEYS,
    palette_from_hue,
    random_palette,
)
from desktop_bug.content.personality_profiles import personality_gait_style
from desktop_bug.creature import Creature
from desktop_bug.creature.constants import GAIT_BY_STATE
from desktop_bug.state.progression import ABILITY_BY_ID, ABILITY_TREE
from support import ROOT


@pytest.fixture(scope="module")
def content(qapp):
    models, _ = discover_models(ROOT)
    personalities, _ = discover_personalities(ROOT)
    return models, personalities


def _spider(content, personality_id="mellow", **kwargs):
    models, personalities = content
    return Creature(models["tarantula"], personalities[personality_id], 800, 600, **kwargs)


# --------------------------------------------------------------- the gait

def test_what_a_spider_is_doing_overrides_how_it_walks(content):
    spider = _spider(content, gait_style="lively")
    spider.state = "Inspect"
    assert spider.effective_gait_style() == "skitter"
    assert spider._uses_skitter_gait() is True

    spider.state = "Chase"
    assert spider.effective_gait_style() == "lively"
    assert spider._uses_skitter_gait() is False
    assert spider._uses_lively_gait() is True


def test_a_state_nobody_mapped_keeps_the_temperament_s_own_gait(content):
    """Wander is deliberately unmapped: it is the state a spider is in most
    of the time, and forcing it either way would erase the temperament."""
    assert "Wander" not in GAIT_BY_STATE
    for personality_id in ("bold", "mellow"):
        spider = _spider(content, personality_id,
                         gait_style=personality_gait_style(personality_id))
        spider.state = "Wander"
        assert spider.effective_gait_style() == personality_gait_style(personality_id)


def test_a_frightened_spider_runs_whatever_state_it_is_in(content):
    """DC-50's retreat runs under several states; all of them are a run."""
    spider = _spider(content, gait_style="skitter")
    spider.state = "Inspect"
    assert spider.effective_gait_style() == "skitter"
    spider.flee_timer = 3.0
    assert spider.fleeing
    assert spider.effective_gait_style() == "lively"


def test_classic_opts_out_of_phases_entirely(content):
    """It is the original pre-DC-16 gait, kept so a preset that names it
    behaves exactly as its author left it. Phasing it would break that."""
    spider = _spider(content, gait_style="classic")
    for state in list(GAIT_BY_STATE) + ["Wander"]:
        spider.state = state
        assert spider.effective_gait_style() == "classic", state


def test_the_two_phases_are_the_ones_that_were_asked_for():
    """Curious darts; committed-and-fast runs. Stated as data so a future
    entry has to decide which of the two it is."""
    assert GAIT_BY_STATE["Inspect"] == "skitter"
    assert GAIT_BY_STATE["Observe"] == "skitter"
    assert GAIT_BY_STATE["Chase"] == "lively"
    assert GAIT_BY_STATE["Retreat"] == "lively"
    assert set(GAIT_BY_STATE.values()) == {"skitter", "lively"}


# ------------------------------------------------------------- the skills

def test_levelling_up_unlocks_without_anyone_clicking(content):
    spider = _spider(content)
    assert spider.progression.unlocked_abilities == []
    events = spider.gain_experience(400)
    assert spider.progression.level >= 2
    assert spider.progression.unlocked_abilities, events
    assert any("learned" in event for event in events)


def test_it_walks_up_the_tree_in_order(content):
    """Cheapest first by level requirement, so a spider takes the tree the
    way its author laid it out rather than whichever node a set yields."""
    spider = _spider(content)
    spider.gain_experience(6000)
    order = spider.progression.unlocked_abilities
    levels = [ABILITY_BY_ID[ability].level_required for ability in order]
    assert levels == sorted(levels), order


def test_prerequisites_are_still_honoured(content):
    spider = _spider(content)
    spider.gain_experience(6000)
    unlocked = spider.progression.unlocked_abilities
    for index, ability in enumerate(unlocked):
        for required in ABILITY_BY_ID[ability].prerequisites:
            assert required in unlocked[:index], (ability, required)


def test_nothing_is_unlocked_above_the_spider_s_level(content):
    spider = _spider(content)
    spider.gain_experience(300)
    for ability in spider.progression.unlocked_abilities:
        assert ABILITY_BY_ID[ability].level_required <= spider.progression.level


def test_a_maxed_spider_has_the_whole_tree(content):
    spider = _spider(content)
    spider.gain_experience(200000)
    assert set(spider.progression.unlocked_abilities) == {node.id for node in ABILITY_TREE}


def test_the_unlocks_actually_change_the_spider(content):
    """Vitality is +18 max hp. If the stats are not reapplied, unlocking is
    bookkeeping and nothing else."""
    spider = _spider(content)
    before = spider.max_hp
    spider.gain_experience(400)
    assert "vitality" in spider.progression.unlocked_abilities
    assert spider.max_hp > before


# ----------------------------------------------------------- the palettes

def test_every_palette_fills_every_colour():
    """A partly-filled palette blends a preset with whatever the model had,
    which is how the first version produced mismatched feet."""
    for palette in NAMED_PALETTES:
        assert set(palette.colors) == set(PALETTE_KEYS), palette.id
        for key, rgb in palette.colors.items():
            assert len(rgb) == 3 and all(0 <= channel <= 255 for channel in rgb), (palette.id, key)


def test_a_palette_hands_out_copies():
    """It is stored on a Qt widget and edited in place by the dialog."""
    first = PALETTE_BY_ID["jade"].as_overrides()
    first["body"][0] = 0
    assert PALETTE_BY_ID["jade"].colors["body"][0] != 0


def test_the_named_palettes_are_actually_different():
    bodies = {tuple(palette.colors["body"]) for palette in NAMED_PALETTES}
    assert len(bodies) == len(NAMED_PALETTES)


def test_a_palette_reads_as_one_animal():
    """The body is the darkest part and the band the brightest, which is the
    relationship the shipped models use and the thing seven free colour
    pickers lose."""
    for palette in list(NAMED_PALETTES) + [
            type(NAMED_PALETTES[0])("x", "x", palette_from_hue(hue))
            for hue in (0.0, 0.25, 0.5, 0.75)]:
        colors = palette.colors
        body = sum(colors["body"])
        assert sum(colors["leg_band"]) > body, palette.id
        assert sum(colors["highlight"]) > body, palette.id
        assert sum(colors["leg_tip"]) <= sum(colors["legs"]), palette.id


def test_a_random_palette_is_a_palette_not_seven_random_colours():
    rng = random.Random(11)
    rolled = [random_palette(rng) for _ in range(40)]
    for colors in rolled:
        assert set(colors) == set(PALETTE_KEYS)
        assert sum(colors["leg_band"]) > sum(colors["body"])
    # And it does roll: forty spiders should not share one body colour.
    assert len({tuple(colors["body"]) for colors in rolled}) > 25


def test_a_random_palette_is_reproducible_from_its_seed():
    assert random_palette(random.Random(3)) == random_palette(random.Random(3))


def test_the_settings_window_offers_every_palette(qapp, monkeypatch):
    # mkdtemp rather than pytest's tmp_path: the shared pytest-of-win root on
    # this machine is intermittently locked. Established convention here.
    import tempfile

    from desktop_bug.app.config_ui import RANDOM_PALETTE_ID, ConfigWindow

    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR",
                       str(Path(tempfile.mkdtemp(prefix="dc58-")) / "state"))
    window = ConfigWindow()
    try:
        source = (ROOT / "src" / "desktop_bug" / "app"
                  / "config_ui.py").read_text(encoding="utf-8")
        # The picker is built inside a modal dialog, so this checks the one
        # thing a test outside that dialog can: every palette reaches it, and
        # "Surprise me" is offered alongside them.
        assert "for palette in NAMED_PALETTES:" in source
        assert RANDOM_PALETTE_ID in source
        assert window._palette_icon(PALETTE_BY_ID["cocoa"].colors) is not None
    finally:
        window.close()


def test_a_raised_spider_uses_the_same_catalogue():
    """One place to change how a spider can look, whether a person picked it
    or a base rolled it."""
    source = (ROOT / "src" / "desktop_bug" / "manager"
              / "colony.py").read_text(encoding="utf-8")
    assert "from ..content.palettes import random_palette" in source
    assert json.dumps(sorted(PALETTE_KEYS))  # the keys are the contract
