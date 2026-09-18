"""Behaviour decisions read the world through a snapshot, not directly (DC-17, C1).

``Creature.update()`` now builds a ``Perception`` for itself at the top of
every tick. ``_consider_special_actions`` and ``_can_shoot_web`` ask that
object whether there is a web worth mending/adopting/walking or a fly world
to shoot through, instead of calling ``self.web_world``/``self.fly_world``
directly. Executing a decision once made (claiming a site, firing a shot) and
the per-frame liveness check on a web a creature already holds a reference to
still go through the live world object -- see ``perception.py``'s module
docstring for why -- so this only asserts the specific decision-time reads
are gone, not that ``web_world``/``fly_world`` never appear in the file.
"""

from __future__ import annotations

import pytest

from desktop_bug.creature import Creature
from desktop_bug.perception import Perception, build_perception
from support import load_pair, ROOT

BEHAVIOUR_SRC = (ROOT / "src" / "desktop_bug" / "creature" / "behaviour.py").read_text(encoding="utf-8")


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Building a Creature constructs Qt-backed sprite state."""


class FakeWebWorld:
    def __init__(self):
        self.repairable_calls = []
        self.adoptable_calls = []
        self.walkable_calls = []
        self.intact_count = 3

    def find_repairable_web(self, creature, max_dist=1e9):
        self.repairable_calls.append((creature, max_dist))
        return "a-repairable-web"

    def find_adoptable_web(self, creature, max_dist=1e9):
        self.adoptable_calls.append((creature, max_dist))
        return "an-adoptable-web"

    def find_walkable_web(self, creature, max_dist=1e9):
        self.walkable_calls.append((creature, max_dist))
        return "a-walkable-web"

    def intact_complete_count(self):
        return self.intact_count


def build_creature() -> Creature:
    model, personality = load_pair()
    return Creature(model, personality, 1600, 900)


def test_perception_delegates_to_the_web_world_lazily():
    creature = build_creature()
    world = FakeWebWorld()
    perception = Perception(world, None, creature)

    assert perception.has_web_world is True
    assert perception.repairable_web(max_dist=250.0) == "a-repairable-web"
    assert perception.adoptable_web(max_dist=300.0) == "an-adoptable-web"
    assert perception.walkable_web(max_dist=400.0) == "a-walkable-web"
    assert perception.intact_web_count() == 3
    assert perception.can_shoot_prey_web() is False

    # The creature and the caller's own max_dist reach the world unchanged.
    assert world.repairable_calls == [(creature, 250.0)]
    assert world.adoptable_calls == [(creature, 300.0)]
    assert world.walkable_calls == [(creature, 400.0)]


def test_perception_with_no_world_answers_safely_instead_of_scanning():
    creature = build_creature()
    perception = Perception(None, None, creature)

    assert perception.has_web_world is False
    assert perception.repairable_web(max_dist=1e9) is None
    assert perception.adoptable_web(max_dist=1e9) is None
    assert perception.walkable_web(max_dist=1e9) is None
    assert perception.intact_web_count() == 0
    assert perception.can_shoot_prey_web() is False


def test_can_shoot_prey_web_reflects_the_fly_world():
    creature = build_creature()
    assert Perception(None, None, creature).can_shoot_prey_web() is False
    assert Perception(None, object(), creature).can_shoot_prey_web() is True


def test_creature_update_builds_a_fresh_perception_every_tick():
    creature = build_creature()
    assert creature.perception is None  # declared but not yet built (DC-10)

    creature.update(1.0 / 60.0, 0.0, 0.0, 1600, 900)
    first = creature.perception
    assert first is not None
    assert first.has_web_world is False  # nothing wired it up

    world = FakeWebWorld()
    creature.web_world = world
    creature.fly_world = object()
    creature.update(1.0 / 60.0, 0.0, 0.0, 1600, 900)
    second = creature.perception

    assert second is not first  # a fresh snapshot, not a stale one
    assert second.has_web_world is True
    assert second.can_shoot_prey_web() is True
    assert build_perception(creature).intact_web_count() == world.intact_count


@pytest.mark.parametrize(
    "needle",
    [
        "self.web_world.find_repairable_web(",
        "self.web_world.find_adoptable_web(",
        "self.web_world.find_walkable_web(",
        "self.web_world.intact_complete_count(",
        "return self.fly_world is not None",
    ],
)
def test_behaviour_no_longer_makes_this_direct_world_query(needle):
    assert needle not in BEHAVIOUR_SRC, (
        f"{needle!r} should now go through self.perception (see perception.py)"
    )


def test_behaviour_still_reads_perception_for_its_web_decisions():
    # The replacements exist -- this fails the same way the above would pass
    # for the wrong reason if the whole block had simply been deleted.
    assert "self.perception.repairable_web(" in BEHAVIOUR_SRC
    assert "self.perception.adoptable_web(" in BEHAVIOUR_SRC
    assert "self.perception.walkable_web(" in BEHAVIOUR_SRC
    assert "self.perception.intact_web_count()" in BEHAVIOUR_SRC
    assert "self.perception.can_shoot_prey_web()" in BEHAVIOUR_SRC
