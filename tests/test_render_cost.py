"""Ten spiders must be affordable, and look exactly the same as before.

DC-12 measured where a frame goes: drawing is about four fifths of it, and it
scales linearly with the colony, which is why ten spiders stuttered. Profiling
inside that found the cost was not Qt but Python recomputing the same answers --
the gait tuning rebuilt seventy-two times per spider per frame, the heading
basis four hundred times, every leg chain solved twice.

So DC-35 is caching, not redrawing. That makes the important test not a
stopwatch but an identity: the same seed, the same frames, rendered with the
caches and with every one of them disabled, must produce pixel-identical images.
A speed-up that changes what you see is not a speed-up, it is a different
program.

The second half counts how often each cached thing is really recomputed.
Timings vary with the machine and would make a flaky test; a rebuild count is
exact everywhere, and it is what actually went wrong.
"""

import json
import math
import random


from PyQt5.QtGui import QColor, QImage, QPainter

from desktop_bug.creature import Creature
from desktop_bug.manager import CreatureManager
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

SCREEN = (1280, 720)
DT = 1.0 / 60.0
SEED = 20260917


class NeverCaches(dict):
    """A dict that forgets immediately, for disabling a cache in place."""

    def __setitem__(self, key, value):
        return None

    def get(self, key, default=None):
        return default


def uncached_basis(self):
    return (math.cos(self.heading), math.sin(self.heading),
            -math.sin(self.heading), math.cos(self.heading))


def uncached_qcolor(self, key, alpha=255):
    red, green, blue = self._blend_palette_color(key)
    return QColor(red, green, blue, alpha)


def uncached_triplet(self, rgb, alpha=255):
    red, green, blue = self._blend_triplet((float(rgb[0]), float(rgb[1]), float(rgb[2])))
    return QColor(red, green, blue, alpha)


def build_manager(count: int = 3) -> CreatureManager:
    random.seed(SEED)
    manager = CreatureManager(ROOT / "presets" / "default.json", *SCREEN)
    manager.reload_from_preset_data({
        "name": "Render cost",
        "slots": [{"model": "tarantula", "personality": "balanced", "count": count,
                   "team": "pack_a", "job": "none"}],
        "settings": {"size_scale": 1.0, "interferable": True, "mood_mode": "auto",
                     "social_play": True, "gait_style": "lively",
                     "flies": {"enabled": False}},
    })
    for index, creature in enumerate(manager.creatures):
        angle = (index / max(1, len(manager.creatures))) * math.tau
        creature.x = SCREEN[0] * 0.5 + math.cos(angle) * 260.0
        creature.y = SCREEN[1] * 0.5 + math.sin(angle) * 200.0
        creature._initialize_legs()
    return manager


def run_frames(frames: int, cached: bool) -> list:
    """Render a fixed run, with the caches on or every one of them disabled."""
    saved = (Creature._basis, Creature._qcolor, Creature._qcolor_triplet,
             Creature._spider_gait_config)
    try:
        if not cached:
            Creature._basis = uncached_basis
            Creature._qcolor = uncached_qcolor
            Creature._qcolor_triplet = uncached_triplet
            Creature._spider_gait_config = Creature._build_spider_gait_config
        manager = build_manager()
        if not cached:
            for creature in manager.creatures:
                creature._chain_points_cache = NeverCaches()
                creature._reach_cache = NeverCaches()
        images = []
        for frame in range(frames):
            manager.update(DT, -100000.0, -100000.0)
            image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
            image.fill(QColor(0, 0, 0, 0))
            painter = QPainter(image)
            painter.setRenderHint(QPainter.Antialiasing, True)
            manager.render(painter)
            painter.end()
            if frame >= frames - 3:
                images.append(image)
        return images
    finally:
        (Creature._basis, Creature._qcolor, Creature._qcolor_triplet,
         Creature._spider_gait_config) = saved


def test_caching_changed_nothing_on_screen() -> None:
    with_caches = run_frames(40, cached=True)
    without = run_frames(40, cached=False)
    assert len(with_caches) == len(without) == 3
    for index, (left, right) in enumerate(zip(with_caches, without)):
        assert left == right, (
            f"frame {index} differs with the caches on: the optimisation changed "
            "what is drawn, which makes it a different program rather than a "
            "faster one"
        )
    # And the comparison has to be capable of failing, or it proves nothing.
    different = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
    different.fill(QColor(1, 2, 3, 255))
    assert with_caches[0] != different, "image comparison is not comparing anything"


def test_gait_tuning_is_built_once() -> None:
    """Seventy-two rebuilds per spider per frame was the single worst cost."""
    manager = build_manager(count=1)
    creature = manager.creatures[0]

    builds = [0]
    original = Creature._build_spider_gait_config

    def counted(self):
        builds[0] += 1
        return original(self)

    Creature._build_spider_gait_config = counted
    try:
        image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
        for _ in range(20):
            manager.update(DT, -100000.0, -100000.0)
            painter = QPainter(image)
            manager.render(painter)
            painter.end()
        assert builds[0] <= 1, (
            f"the gait tuning was rebuilt {builds[0]} times over twenty frames"
        )

        # But a swapped model must not keep serving the old tuning. Counted as
        # a difference, because the cache may already have been warm before the
        # counter was installed and the absolute number would then be zero.
        before_swap = builds[0]
        creature.model = dict(creature.model)
        creature._spider_gait_config()
        assert builds[0] == before_swap + 1, (
            "changing the model did not rebuild the gait tuning exactly once: "
            f"{before_swap} -> {builds[0]}"
        )
    finally:
        Creature._build_spider_gait_config = original


def test_the_second_pass_reuses_the_first() -> None:
    """A procedural spider asks for every leg chain twice a frame.

    Once for the leg, once for the sockets and knuckles drawn over it. For a
    planted leg the two asks are identical and the second is free. For a leg
    that is lifting or airborne they are not: the leg pass raises the foot
    before solving and the socket pass does not, so that one is two real solves
    and always will be.

    So this compares solves against asks rather than against a fixed number.
    Before the cache the two were equal; a tetrapod gait lifts at most three of
    eight legs, so the ratio cannot exceed about 0.7 now. An exact bound was
    tried first and failed on CI at 86 solves against a limit of 80 -- the
    machine happened to catch a leg mid-swing, which is correct behaviour.
    """
    manager = build_manager(count=1)
    creature = manager.creatures[0]
    legs = len(creature.legs)
    assert legs >= 4, legs
    # DC-83 switched the tarantula's socket overlay -- the second pass --
    # off, since its legs now come out from under the carapace rim. The pass
    # still exists for any model that enables it; switch it on here.
    import copy
    creature.model = copy.deepcopy(creature.model)
    creature.model["appearance"].setdefault("leg_connections", {})["enabled"] = True

    solves = [0]
    asks = [0]

    class CountingCache(dict):
        def __setitem__(self, key, value):
            solves[0] += 1
            dict.__setitem__(self, key, value)

    creature._chain_points_cache = CountingCache(creature._chain_points_cache)

    original = Creature._sprite_leg_chain_points

    def counted(self, *args, **kwargs):
        asks[0] += 1
        return original(self, *args, **kwargs)

    Creature._sprite_leg_chain_points = counted
    try:
        image = QImage(*SCREEN, QImage.Format_ARGB32_Premultiplied)
        frames = 10
        for _ in range(frames):
            manager.update(DT, -100000.0, -100000.0)
            painter = QPainter(image)
            manager.render(painter)
            painter.end()
    finally:
        Creature._sprite_leg_chain_points = original

    assert asks[0] > legs * frames, (
        f"only {asks[0]} chain requests over {frames} frames for {legs} legs; "
        "the second drawing pass seems to have gone away, so this is measuring "
        "nothing"
    )
    assert solves[0] > 0, "no leg was solved at all, so this is measuring nothing"
    assert solves[0] <= asks[0] * 0.8, (
        f"{solves[0]} solves for {asks[0]} requests: the second pass is "
        "re-solving what the first already worked out"
    )


def test_the_chain_cache_hands_out_copies() -> None:
    """A shared list would let one caller corrupt the next frame."""
    manager = build_manager(count=1)
    creature = manager.creatures[0]
    config = creature._sprite_leg_chain_config()
    if not config:
        return  # this model does not use chained legs
    leg = creature.legs[0]
    ax, ay = creature._leg_attach(leg)
    solves = [0]

    class CountingCache(dict):
        def __setitem__(self, key, value):
            solves[0] += 1
            dict.__setitem__(self, key, value)

    creature._chain_points_cache = CountingCache()
    first = creature._sprite_leg_chain_points(leg, ax, ay, ax + 30.0, ay + 20.0, config)
    second = creature._sprite_leg_chain_points(leg, ax, ay, ax + 30.0, ay + 20.0, config)
    # The deterministic half of the claim, with no gait phase involved: asking
    # twice for the same leg at the same place solves once.
    assert solves[0] == 1, f"the same request was solved {solves[0]} times"
    assert first == second, "the same inputs gave two different chains"
    assert first is not second, "the cache handed out the same list twice"
    first[0] = (0.0, 0.0)
    third = creature._sprite_leg_chain_points(leg, ax, ay, ax + 30.0, ay + 20.0, config)
    assert third[0] != (0.0, 0.0), "a caller was able to corrupt the cached chain"


def test_colour_and_reach_caches_invalidate() -> None:
    manager = build_manager(count=1)
    creature = manager.creatures[0]

    plain = creature._qcolor("body", 255)
    assert creature._qcolor("body", 255) == plain
    # Alpha is not part of the cached value, so it still varies per call.
    assert creature._qcolor("body", 40).alpha() == 40

    # Camouflage has to reach the colour, or a hiding spider would not fade.
    creature._camouflage_color = [255, 0, 255]
    creature._camouflage_strength = 1.0
    creature.personality["camouflage_color_blend"] = 1.0
    hidden = creature._qcolor("body", 255)
    assert hidden != plain, "the colour cache survived a change of camouflage"
    creature._camouflage_strength = 0.0
    assert creature._qcolor("body", 255) == plain, "the colour did not come back"

    leg = creature.legs[0]
    before = creature._leg_max_reach(leg)
    assert creature._leg_max_reach(leg) == before
    creature.size *= 2.0
    assert creature._leg_max_reach(leg) != before, (
        "the reach cache survived the spider changing size"
    )


def test_the_recorded_baseline_still_describes_this_code() -> None:
    """The benchmark's baseline is DC-12's measurement, from before this work.

    Leaving it in place would report a permanent 20% improvement and never fail
    again, so DC-35 re-records it. This only checks that it was re-recorded, not
    how fast the machine is.
    """
    data = json.loads((ROOT / "tools" / "benchmark_baseline.json").read_text(encoding="utf-8"))
    note = str(data.get("note", "")).lower()
    assert "dc-35" in note, (
        "the committed baseline still describes the code before DC-35: " + note
    )
