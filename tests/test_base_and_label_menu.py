"""Removing a base, and showing every spider's label at once.

Two things the project owner asked for after watching a colony run.

A base is placed by a Builder wherever it happens to settle, and nothing
could move or clear one: a colony that built in an awkward corner was stuck
with it for the life of the save. Right-clicking it and removing it is the
counterpart to "Add a cage here" -- a direct edit of the scene rather than a
game action, which is why it does not cost the team anything.

Levels and health bars could already be pinned one spider at a time through
the inspector, which does not scale to a colony. The scene-wide switches are
deliberately *not* written into a spider's saved progression, so turning them
off again does not wipe a pin somebody set on purpose.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
from desktop_bug.manager import CreatureManager

ROOT = Path(__file__).resolve().parents[1]
SCREEN = (1400, 900)


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a CreatureManager constructs Qt-backed sprite state."""


@pytest.fixture
def colony(monkeypatch):
    scratch = Path(tempfile.mkdtemp(prefix="dc-bases-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(scratch / "state"))
    preset = scratch / "two_teams.json"
    preset.write_text(json.dumps({
        "name": "two_teams",
        "slots": [
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "a", "team": "hunters", "job": "builder"},
            {"model": "tarantula", "personality": "mellow", "count": 1,
             "slot_id": "b", "team": "rivals", "job": "builder"},
        ],
        "settings": {"flies": {"enabled": False, "spawner": False}},
    }), encoding="utf-8")
    manager = CreatureManager(preset, *SCREEN, seed=4)
    # Found a base for each team at a known spot rather than waiting for the
    # builders to wander somewhere.
    world = manager.base_world
    for index, creature in enumerate(manager.creatures):
        creature.x, creature.y = 300.0 + index * 600.0, 400.0
        world.ensure_site(creature)
    return manager


def _sites(manager):
    return list(manager.base_world.bases.values())


def test_a_click_on_a_base_finds_it(colony):
    sites = _sites(colony)
    assert len(sites) == 2, sites
    for site in sites:
        assert colony.base_at(site.x, site.y) is site


def test_a_click_on_empty_desktop_finds_nothing(colony):
    assert colony.base_at(10.0, 10.0) is None


def test_the_nearest_base_wins_when_two_overlap(colony):
    """Clicking where two territories meet must remove the one under the
    cursor, not whichever the dictionary happens to list first."""
    first, second = _sites(colony)
    second.x, second.y = first.x + 12.0, first.y
    assert colony.base_at(first.x - 4.0, first.y) is first
    assert colony.base_at(second.x + 4.0, second.y) is second


def test_removing_a_base_removes_exactly_one(colony):
    first, second = _sites(colony)
    message = colony.remove_base(first)
    assert "Removed" in message, message
    assert _sites(colony) == [second]


def test_removing_a_base_leaves_its_team_intact(colony):
    """A scene edit, not a defeat: nobody dies and nothing is confiscated."""
    first = _sites(colony)[0]
    before = len(colony.creatures)
    colony.remove_base(first)
    assert len(colony.creatures) == before
    assert all(not c.dead for c in colony.creatures)


def test_the_colony_keeps_running_after_its_base_goes(colony):
    """Jobs look their site up by team each frame, so a worker whose base has
    gone should simply find nothing to do rather than crash or freeze."""
    colony.remove_base(_sites(colony)[0])
    for _ in range(240):
        colony.update(1.0 / 60.0, -5000.0, -5000.0)
    assert colony.creatures


def test_removing_a_base_that_is_already_gone_says_so(colony):
    first = _sites(colony)[0]
    colony.remove_base(first)
    assert "no base" in colony.remove_base(first).lower()
    assert colony.remove_base(None).lower().startswith("there is no base")


def test_remove_every_base(colony):
    assert "2" in colony.remove_bases()
    assert _sites(colony) == []
    assert "no bases" in colony.remove_bases().lower()


# ----------------------------------------------------------------------
# The scene-wide label switches
# ----------------------------------------------------------------------

def _labels_off(colony):
    """The owner turned names, levels, health and stamina on by default, so
    the tests about what a switch *adds* start from everything off."""
    for setter in (colony.set_always_show_levels, colony.set_always_show_health,
                   colony.set_always_show_xp, colony.set_always_show_stamina):
        setter(False)


def test_levels_are_on_by_default(colony):
    """Was "off until asked for"; the owner asked for them on by default."""
    assert colony.always_show_levels is True
    assert all(c.level_label_pinned for c in colony.creatures)
    assert colony.always_show_health and colony.always_show_stamina
    assert colony.always_show_xp is False, "XP is shown only on request"


def test_one_switch_shows_every_level(colony):
    colony.set_always_show_levels(True)
    assert all(c.level_label_pinned for c in colony.creatures)
    assert all("Lv " in c._label_text() for c in colony.creatures)


def test_one_switch_shows_every_health_bar(colony):
    colony.set_always_show_health(True)
    assert all(c.health_label_pinned for c in colony.creatures)
    assert all(c.label_visible(False) for c in colony.creatures)


def test_turning_it_off_does_not_wipe_a_deliberate_pin(colony):
    """The reason these live on the manager, not in saved progression."""
    pinned, other = colony.creatures[0], colony.creatures[1]
    pinned.set_level_label_pinned(True)

    colony.set_always_show_levels(True)
    assert other.level_label_pinned
    colony.set_always_show_levels(False)

    assert pinned.level_label_pinned, "a hand-pinned spider lost its level"
    assert not other.level_label_pinned


def test_a_spider_born_later_joins_the_setting(colony):
    """Otherwise the new arrival is the only one missing a label."""
    colony.set_always_show_levels(True)
    colony.set_always_show_health(True)
    born = colony._create_creature(
        colony.creatures[0].model, colony.creatures[0].personality, index=9)
    assert born.force_show_level and born.force_show_health
    assert born.level_label_pinned and born.health_label_pinned


def test_always_show_names_shows_the_ones_nobody_named(colony):
    """The switch said "always" and meant "always, if it has a name".

    `label_visible` began with `bool(self.name or pinned) and ...`, so a
    colony nobody had named by hand answered the switch with nothing at all
    -- which is what the owner was looking at when they asked for these
    checkboxes again. `display_name` already falls back to the model's own
    name, so there was always something to draw.
    """
    _labels_off(colony)
    for creature in colony.creatures:
        assert not creature.name, "this test needs unnamed spiders"
        assert creature.display_name
        assert creature.label_visible(False) is False
        assert creature.label_visible(True) is True


def test_hovering_an_unnamed_spider_still_says_nothing(colony):
    """A label appearing under the cursor is not the same as one the owner
    asked for, so the hover path deliberately did not change."""
    _labels_off(colony)
    creature = colony.creatures[0]
    creature._hovered = True
    assert creature.label_visible(False) is False
    creature.set_name("Bramble")
    assert creature.label_visible(False) is True
