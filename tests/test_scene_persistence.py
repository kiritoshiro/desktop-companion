"""The desktop comes back the way it was left (DC-23, B4/B5).

Two things used to be forgotten between launches. The arrangement the player
made -- cages, webs and fly nests -- lived only in memory, so every launch
started on a bare desktop. And a spider's saved profile was keyed by its
position in its preset slot (``<namespace>|<slot id>:<member index>``), so
raising a slot's ``count`` handed one spider's level and name to whichever
spider next landed on that index.

Identity is now generated once and owned by the spider, and the scene is
written into the same runtime state file. These tests cover both halves,
plus the migration that lets a colony saved under the old scheme keep what
it had earned.

Plain ``tempfile`` rather than pytest's ``tmp_path`` fixture throughout,
which numbers its directories by scanning a shared parent that can be locked
by another process -- the established convention in this suite.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from desktop_bug.world.cage import Cage
from desktop_bug.manager import CreatureManager
from desktop_bug.state.runtime_state import STATE_SCHEMA_VERSION


def _state_dir(monkeypatch) -> Path:
    """A private state directory, so a real save is never read or written."""
    base = Path(tempfile.mkdtemp(prefix="dc23-scene-"))
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", str(base / "state"))
    return base


def _write_preset(base: Path, count: int = 3, spawner: bool = True) -> Path:
    preset = base / "scene.json"
    preset.write_text(
        json.dumps({
            "name": "scene",
            "slots": [{
                "model": "tarantula",
                "personality": "mellow",
                "count": count,
                "slot_id": "slot-0",
            }],
            "settings": {"flies": {"enabled": False, "spawner": spawner}},
        }),
        encoding="utf-8",
    )
    return preset


def _weave_one_web(manager: CreatureManager):
    """Put a real, fully built web on screen and tear a piece out of it."""
    web = manager.web_world.claim_site(manager.creatures[0], prefer_corner=True)
    assert web is not None, "no web site was free"
    web.built = len(web.strands)
    web.state = "complete"
    web.builder = None
    web.cut.add((0, 0))
    return web


def test_a_saved_scene_comes_back(monkeypatch):
    """The plan's acceptance: a cage, a web and three named spiders return."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base)

    first = CreatureManager(preset, 1280, 720, seed=7)
    assert len(first.creatures) == 3, first.creatures
    names = ["Shelob", "Charlotte", "Boris"]
    for creature, name in zip(first.creatures, names):
        creature.set_name(name)
        creature.progression.level = 4
        creature.progression.total_xp = 400
    ids = [creature.progression_id for creature in first.creatures]

    first.cages.append(Cage(200.0, 150.0, 320.0, 240.0))
    web = _weave_one_web(first)
    web_signature = (web.spec_id, web.pattern, len(web.strands),
                     tuple(web.hub), sorted(web.cut))
    first.fly_world.add_spawner((640.0, 360.0))
    nests = [(round(sp.x, 3), round(sp.y, 3)) for sp in first.fly_world.spawners]
    first.save_runtime_state()

    second = CreatureManager(preset, 1280, 720, seed=7)

    assert [c.progression_id for c in second.creatures] == ids
    assert [c.name for c in second.creatures] == names
    assert [c.level for c in second.creatures] == [c.level for c in first.creatures]

    assert len(second.cages) == 1, second.cages
    assert second.cages[0].rect() == (200.0, 150.0, 320.0, 240.0)

    assert len(second.web_world.webs) == 1, second.web_world.webs
    back = second.web_world.webs[0]
    assert (back.spec_id, back.pattern, len(back.strands),
            tuple(back.hub), sorted(back.cut)) == web_signature
    assert back.is_complete(), back.state
    assert back.builder is None, "a live weaver must not be restored from disk"

    assert [(round(sp.x, 3), round(sp.y, 3)) for sp in second.fly_world.spawners] == nests


def test_changing_a_slot_count_keeps_each_spider_its_level_and_name(monkeypatch):
    """The plan's other acceptance half: ``count`` no longer reshuffles profiles."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=3)

    first = CreatureManager(preset, 1280, 720, seed=11)
    for offset, creature in enumerate(first.creatures):
        creature.set_name(f"Spider{offset}")
        creature.progression.level = offset + 2
        creature.progression.total_xp = 200 * (offset + 1)
    before = {c.progression_id: (c.name, c.level, c.progression.total_xp)
              for c in first.creatures}
    first.save_runtime_state()

    # Raise the count: the three saved spiders come back unchanged, and the
    # newcomer is genuinely new rather than an inheritor of someone's profile.
    _write_preset(base, count=5)
    grown = CreatureManager(preset, 1280, 720, seed=11)
    assert len(grown.creatures) == 5
    returning = [c for c in grown.creatures if c.progression_id in before]
    assert len(returning) == 3, [c.progression_id for c in grown.creatures]
    for creature in returning:
        name, level, total_xp = before[creature.progression_id]
        assert (creature.name, creature.level, creature.progression.total_xp) == (
            name, level, total_xp), creature.progression_id
    for creature in grown.creatures:
        if creature.progression_id not in before:
            assert creature.progression.total_xp == 0, "a new spider inherited XP"
    grown.save_runtime_state()

    # Lower it again: the youngest are dropped, the originals still keep theirs.
    _write_preset(base, count=3)
    shrunk = CreatureManager(preset, 1280, 720, seed=11)
    assert len(shrunk.creatures) == 3
    assert {c.progression_id for c in shrunk.creatures} == set(before)
    for creature in shrunk.creatures:
        name, level, total_xp = before[creature.progression_id]
        assert (creature.name, creature.level) == (name, level)


def test_a_colony_saved_under_the_old_slot_keys_keeps_what_it_earned(monkeypatch):
    """A version 2 file migrates without anyone losing a level or a name."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=2)
    state_dir = base / "state"
    state_dir.mkdir(parents=True, exist_ok=True)
    (state_dir / "creatures.json").write_text(
        json.dumps({
            "schema_version": 2,
            "launch": 4,
            "creatures": {
                "scene|slot-0:0": {"name": "Elder", "last_seen": 4,
                                   "progression": {"level": 9, "total_xp": 900}},
                "scene|slot-0:1": {"name": "Younger", "last_seen": 4,
                                   "progression": {"level": 3, "total_xp": 120}},
            },
            "bases": [],
        }),
        encoding="utf-8",
    )

    manager = CreatureManager(preset, 1280, 720, seed=3)
    by_name = {c.name: c for c in manager.creatures}
    assert set(by_name) == {"Elder", "Younger"}, [c.name for c in manager.creatures]
    assert by_name["Elder"].level == 9
    assert by_name["Younger"].level == 3
    # Member order is the order they were saved in, not the order the old keys
    # happen to sort in once identity stops living in the key.
    assert [c.name for c in manager.creatures] == ["Elder", "Younger"]

    manager.save_runtime_state()
    saved = json.loads((state_dir / "creatures.json").read_text(encoding="utf-8"))
    assert saved["schema_version"] == STATE_SCHEMA_VERSION
    assert {e["name"] for e in saved["creatures"].values()} == {"Elder", "Younger"}


def test_a_corrupt_scene_costs_the_scene_and_not_the_launch(monkeypatch):
    """A hand-edited or truncated scene must not stop the overlay starting."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=1)
    state_dir = base / "state"
    state_dir.mkdir(parents=True, exist_ok=True)
    (state_dir / "creatures.json").write_text(
        json.dumps({
            "schema_version": STATE_SCHEMA_VERSION,
            "launch": 2,
            "creatures": {},
            "bases": [],
            "scene": {
                "cages": [{"x": "nonsense"}, None, {"x": 10, "y": 10, "w": 200, "h": 200}],
                "webs": ["not a web", {"strands": []}, {"hub": [1.0]}],
                "nests": [{"x": None}, {"x": 100.0, "y": 120.0}],
            },
        }),
        encoding="utf-8",
    )

    manager = CreatureManager(preset, 1280, 720, seed=5)
    assert len(manager.cages) == 1, manager.cages
    assert manager.web_world.webs == []
    assert [(sp.x, sp.y) for sp in manager.fly_world.spawners] == [(100.0, 120.0)]


def test_a_web_that_no_longer_fits_the_screen_is_dropped(monkeypatch):
    """A web anchored off the desktop must not come back hanging off it."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=1)

    wide = CreatureManager(preset, 1920, 1080, seed=13)
    _weave_one_web(wide)
    assert len(wide.web_world.webs) == 1
    wide.save_runtime_state()

    narrow = CreatureManager(preset, 640, 480, seed=13)
    for web in narrow.web_world.webs:
        assert web.contains_region(640, 480), web.bbox()


def test_an_empty_scene_is_written_so_clearing_the_desktop_sticks(monkeypatch):
    """Removing every cage has to survive a restart, not be treated as no data."""
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=1)

    first = CreatureManager(preset, 1280, 720, seed=17)
    first.cages.append(Cage(100.0, 100.0, 200.0, 200.0))
    first.save_runtime_state()
    assert len(CreatureManager(preset, 1280, 720, seed=17).cages) == 1

    second = CreatureManager(preset, 1280, 720, seed=17)
    second.cages.clear()
    second.save_runtime_state()
    assert CreatureManager(preset, 1280, 720, seed=17).cages == []


def test_a_seeded_run_is_not_disturbed_by_minting_identities(monkeypatch):
    """Generating ids must not shift the simulation's own random stream.

    Ids are drawn from a dedicated stream. Taking them from the shared one
    made the scene depend on how many ids the save file left to mint, so the
    same seed laid the same colony out differently on a second launch.
    """
    base = _state_dir(monkeypatch)
    preset = _write_preset(base, count=3)

    first = CreatureManager(preset, 1280, 720, seed=23)
    layout = [(c.progression_id, round(c.x, 6), round(c.y, 6)) for c in first.creatures]
    first.save_runtime_state()

    # Second launch: every id is reused rather than minted, and the colony
    # still lands exactly where it did before.
    second = CreatureManager(preset, 1280, 720, seed=23)
    assert [(c.progression_id, round(c.x, 6), round(c.y, 6))
            for c in second.creatures] == layout
