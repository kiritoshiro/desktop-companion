"""Teams must be able to be hostile, not merely unrelated.

Team identity only ever implied friendship, so two different teams were
neutral to each other. A Guard reacts only to a declared foe, which left the
shipped Colony preset unable to demonstrate the behaviour it advertises: over a
real 135 second run its base alert never rose above zero.
"""

import json
import random


from desktop_bug.jobs import BaseWorld
from desktop_bug.manager import CreatureManager
from desktop_bug.preset_io import validate_preset
from desktop_bug.progression import (
    ProgressionState,
    normalize_team_id,
    normalize_team_stances,
    relation_between,
    team_stance,
)
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """


def test_team_ids() -> None:
    assert normalize_team_id("Rivals") == "rivals"
    assert normalize_team_id("  PACK_A ") == "pack_a"
    assert normalize_team_id("") == "neutral"
    assert normalize_team_id(None) == "neutral"


def test_default_stances() -> None:
    # A team is friendly with itself and unrelated to others by default, which
    # keeps an ordinary scene peaceful.
    assert team_stance("pack_a", "pack_a") == "friend"
    assert team_stance("pack_a", "pack_b") is None

    # Except "rivals", so choosing it in the settings window means something
    # without editing relations pair by pair.
    assert team_stance("pack_a", "rivals") == "foe"
    assert team_stance("rivals", "pack_a") == "foe", "stance must be symmetric"
    assert team_stance("rivals", "rivals") == "friend", "rivals still band together"

    # A solo spider belongs to no team and takes no side.
    assert team_stance("neutral", "rivals") is None
    assert team_stance("rivals", "neutral") is None
    assert team_stance("neutral", "neutral") is None


def test_declared_stances() -> None:
    stances = normalize_team_stances({"pack_a": {"pack_b": "foe"}})
    # Declared in both directions, because a stance is mutual.
    assert stances["pack_a"]["pack_b"] == "foe"
    assert stances["pack_b"]["pack_a"] == "foe"
    assert team_stance("pack_a", "pack_b", stances) == "foe"
    assert team_stance("pack_b", "pack_a", stances) == "foe"

    # A declaration overrides the rivals default in both directions.
    friendly = normalize_team_stances({"rivals": {"pack_a": "friend"}})
    assert team_stance("pack_a", "rivals", friendly) == "friend"

    # Case and whitespace do not create a second team.
    mixed = normalize_team_stances({" Pack_A ": {"RIVALS": "foe"}})
    assert team_stance("pack_a", "rivals", mixed) == "foe"

    # Malformed input is ignored rather than raising.
    assert normalize_team_stances(None) == {}
    assert normalize_team_stances({"pack_a": "nonsense"}) == {}
    assert normalize_team_stances({"pack_a": {"pack_b": "enemy"}}) == {}


def test_pair_override_wins() -> None:
    left = ProgressionState(team_id="pack_a")
    right = ProgressionState(team_id="rivals")
    stances = normalize_team_stances({"pack_a": {"rivals": "foe"}})

    assert relation_between(left, right, "right-key", stances) == "foe"

    # A choice made in the runtime inspector still beats the team stance.
    left.relation_overrides["right-key"] = "friend"
    assert relation_between(left, right, "right-key", stances) == "friend"

    # Without stances, teams fall back to identity alone.
    plain = ProgressionState(team_id="pack_b")
    assert relation_between(ProgressionState(team_id="pack_a"), plain, None) == "neutral"
    assert relation_between(ProgressionState(team_id="pack_b"), plain, None) == "friend"


def test_preset_validation() -> None:
    base = {"name": "T", "slots": [], "settings": {}}

    good = dict(base, settings={"team_relations": {"pack_a": {"rivals": "foe"}}})
    validate_preset(good)

    for bad_settings in (
        {"team_relations": "nonsense"},
        {"team_relations": {"pack_a": "nonsense"}},
        {"team_relations": {"pack_a": {"rivals": "enemy"}}},
        {"team_relations": {"": {"rivals": "foe"}}},
    ):
        try:
            validate_preset(dict(base, settings=bad_settings))
        except ValueError:
            continue
        raise AssertionError(f"validation accepted {bad_settings}")


def test_guard_reacts() -> None:
    """The acceptance case: a Guard must alert on an intruder from a foe team."""
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1600, 900)
    assert manager.team_stances, "colony preset declared no team relations"

    guards = [c for c in manager.creatures if c.job_id == "guard"]
    rivals = [c for c in manager.creatures if c.progression.team_id == "rivals"]
    assert guards and rivals, "colony preset no longer has a guard and a rival"
    guard, rival = guards[0], rivals[0]

    assert guard.relation_to(rival) == "foe", "the guard does not consider the rival hostile"
    assert rival.relation_to(guard) == "foe", "hostility is not mutual"
    friend = [c for c in manager.creatures if c.job_id == "builder"][0]
    assert guard.relation_to(friend) == "friend", "team mates are no longer friends"

    # Put the intruder on the base and run the job layer. Placing it removes
    # the luck of waiting for a wandering spider to arrive.
    world = BaseWorld(1600, 900, rng=random.Random(11))
    world.update(1 / 60.0, manager.creatures)
    site = next(iter(world.bases.values()))
    rival.x, rival.y = site.x, site.y

    alerted = False
    for _ in range(120):
        world.update(1 / 60.0, manager.creatures)
        if site.alert > 0.0 and guard.job_mode == "guard_alert":
            alerted = True
            break
    assert alerted, f"guard never alerted; alert={site.alert} mode={guard.job_mode}"

    # And the guard's work intent actually points at the intruder.
    assert guard.job_alert_target is rival, guard.job_alert_target


def test_live_reload() -> None:
    """Changing team relations must reach spiders already on screen."""
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1600, 900)
    guard = [c for c in manager.creatures if c.job_id == "guard"][0]
    rival = [c for c in manager.creatures if c.progression.team_id == "rivals"][0]
    assert guard.relation_to(rival) == "foe"

    manager.set_team_stances({"pack_a": {"rivals": "friend"}})
    assert guard.relation_to(rival) == "friend", "a live change did not reach the spiders"
    assert guard.team_stances is manager.team_stances


def test_settings_round_trip() -> None:
    """The settings window must not drop a setting it has no widget for."""
    from desktop_bug.config_ui import ConfigWindow

    window = ConfigWindow()
    try:
        preset = json.loads((ROOT / "presets" / "colony.json").read_text(encoding="utf-8"))
        window.apply_settings_to_ui(preset["settings"])
        saved = window.current_settings_data()
        assert saved.get("team_relations") == {"pack_a": {"rivals": "foe"}}, saved.get("team_relations")
        # The widget-backed settings still win over the remembered copy.
        assert "gait_style" in saved and "flies" in saved
    finally:
        window.close()
        window.deleteLater()
