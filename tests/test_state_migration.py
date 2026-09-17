"""Headless checks for runtime state identity, migration and eviction.

Covers two defects: a preset namespace that was case-sensitive on a
case-insensitive filesystem, so one spider could hold two saved profiles, and a
state file that never dropped an entry.
"""

import json
import tempfile
from pathlib import Path

# Keep a test run from reading or rewriting a real player's saved spiders; the
# manager round trip below redirects again to its own temporary file.


from desktop_bug.manager import CreatureManager
from desktop_bug.runtime_state import (
    RETAIN_LAUNCHES,
    STATE_SCHEMA_VERSION,
    entry_rank,
    evict,
    load_payload,
    migrate_creatures,
    next_launch,
    normalize_namespace,
    normalize_state_key,
)
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


def entry(level: int, name: str = "", last_seen=None) -> dict:
    data = {
        "name": name,
        "model": "tarantula",
        "personality": "mellow",
        "progression": {"level": level, "xp": 0, "total_xp": level * 100},
    }
    if last_seen is not None:
        data["last_seen"] = last_seen
    return data


def test_keys() -> None:
    assert normalize_namespace("Default") == "default"
    assert normalize_namespace("  COLONY  ") == "colony"
    assert normalize_namespace("") == "default"
    assert normalize_namespace(None) == "default"

    assert normalize_state_key("Default|slot-0:0") == "default|slot-0:0"
    assert normalize_state_key("default|slot-0:0") == "default|slot-0:0"
    # The part after the namespace is an opaque id and keeps its case.
    assert normalize_state_key("Default|slot-3FE0:1") == "default|slot-3FE0:1"
    # Keys from the scheme that predated preset scoping cannot be attributed.
    assert normalize_state_key("runtime:0") is None
    assert normalize_state_key("") is None
    assert normalize_state_key("default|") is None

    assert entry_rank(entry(5)) > entry_rank(entry(2))
    # An entry with no recorded level is a level 1 spider, which still
    # outranks a malformed entry that is not a mapping at all.
    assert entry_rank({}) == (1, 0)
    assert entry_rank("nonsense") == (0, 0)
    assert entry_rank({}) > entry_rank("nonsense")
    # Older entries stored progression fields at the top level.
    assert entry_rank({"level": 4, "total_xp": 400}) == (4, 400)


def test_migration() -> None:
    raw = {
        "Default|slot-0:0": entry(5, "Webster"),
        "default|slot-0:0": entry(2),
        "runtime:0": entry(9, "orphan"),
        "colony|slot-1:0": entry(3),
    }
    migrated = migrate_creatures(raw, launch=7)
    assert set(migrated) == {"default|slot-0:0", "colony|slot-1:0"}, migrated.keys()
    # The collision keeps the more advanced spider, not whichever came last.
    assert migrated["default|slot-0:0"]["progression"]["level"] == 5
    assert migrated["default|slot-0:0"]["name"] == "Webster"
    assert all(e["last_seen"] == 7 for e in migrated.values())
    # The source payload is not mutated.
    assert "last_seen" not in raw["Default|slot-0:0"]

    # Order must not decide the winner.
    flipped = migrate_creatures(
        {"default|slot-0:0": entry(2), "Default|slot-0:0": entry(5, "Webster")}, launch=7
    )
    assert flipped["default|slot-0:0"]["progression"]["level"] == 5

    # Migrating an already-migrated payload changes nothing.
    again = migrate_creatures(migrated, launch=8)
    assert again == migrated, "migration is not idempotent"

    # An existing sighting is preserved rather than advanced on load.
    kept = migrate_creatures({"default|a:0": entry(1, last_seen=3)}, launch=9)
    assert kept["default|a:0"]["last_seen"] == 3

    assert migrate_creatures(None, 1) == {}
    assert migrate_creatures({"default|a:0": "nonsense"}, 1) == {}


def test_launch_and_eviction() -> None:
    assert next_launch(None) == 1
    assert next_launch({}) == 1
    assert next_launch({"launch": 4}) == 5
    assert next_launch({"launch": "nonsense"}) == 1
    assert next_launch({"launch": -3}) == 1

    states = {
        "default|fresh:0": entry(1, last_seen=100),
        "default|edge:0": entry(1, last_seen=100 - RETAIN_LAUNCHES),
        "default|stale:0": entry(1, last_seen=100 - RETAIN_LAUNCHES - 1),
    }
    kept = evict(states, launch=100)
    assert "default|fresh:0" in kept
    assert "default|edge:0" in kept, "eviction boundary is off by one"
    assert "default|stale:0" not in kept, "stale entry was not retired"

    # A missing sighting counts as now, so nothing is lost to a bad field.
    assert evict({"default|x:0": entry(1)}, launch=100) != {}

    payload_states, bases = load_payload({"creatures": {"A|s:0": entry(1)}, "bases": [{"id": "b"}]}, 3)
    assert set(payload_states) == {"a|s:0"}
    assert bases == [{"id": "b"}]
    assert load_payload(None, 1) == ({}, [])
    assert load_payload({"creatures": "nonsense", "bases": "nonsense"}, 1) == ({}, [])


def test_manager_round_trip() -> None:

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        # A capital initial exercises the case fold: on Windows this is the
        # same file as default.json and must share its saved profile.
        preset_path = tmp_path / "Default.json"
        preset_path.write_text(
            json.dumps(
                {
                    "name": "Default",
                    "slots": [
                        {"model": "tarantula", "personality": "mellow", "count": 1, "slot_id": "slot-0"}
                    ],
                    "settings": {},
                }
            ),
            encoding="utf-8",
        )

        manager = CreatureManager(preset_path, 1280, 720)
        assert manager._progression_namespace == "default", manager._progression_namespace
        assert manager.creatures, "preset produced no creatures"
        assert manager.creatures[0].progression_id == "default|slot-0:0"

        # Redirect persistence at a temporary file so the repository's own
        # state is never touched, and isolate it from the live spiders.
        state_path = tmp_path / "creatures.json"
        state_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "creatures": {
                        "Default|slot-0:0": entry(5, "Webster"),
                        "default|slot-0:0": entry(2),
                        "runtime:0": entry(9, "orphan"),
                    },
                    "bases": [],
                }
            ),
            encoding="utf-8",
        )
        manager._progression_state_path = state_path
        manager.creatures.clear()
        manager._progression_states = manager._load_progression_states()

        assert set(manager._progression_states) == {"default|slot-0:0"}
        assert manager._state_launch == 1

        manager.save_runtime_state()
        saved = json.loads(state_path.read_text(encoding="utf-8"))
        assert saved["schema_version"] == STATE_SCHEMA_VERSION
        assert saved["launch"] == 1
        assert set(saved["creatures"]) == {"default|slot-0:0"}
        assert saved["creatures"]["default|slot-0:0"]["progression"]["level"] == 5
        assert all("|" in key for key in saved["creatures"]), "a bare legacy key survived"

        # A second launch reads the migrated file, advances the counter, and
        # does not change the data again.
        manager._progression_states = manager._load_progression_states()
        assert manager._state_launch == 2
        assert set(manager._progression_states) == {"default|slot-0:0"}

        # The surviving entry is the one a respawned spider inherits.
        model = manager.models["tarantula"]
        personality = manager.personalities[manager.creatures[0].personality["id"]] if manager.creatures else None
        if personality is None:
            personality = next(iter(manager.personalities.values()))
        spider = manager._create_creature(model, personality, 0, progression_id="default|slot-0:0")
        assert spider.level == 5, spider.level
        assert spider.name == "Webster", spider.name
