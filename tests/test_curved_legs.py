"""A tarantula's legs curve; they are not spokes (DC-76).

The owner, after DC-75: *"merge both and do the curved legs"* -- the
follow-up DC-75 left open. [[Tarantula Reference - Brachypelma hamorii]]:
seen from above, the femur leaves the body swung toward the flank and the
distal half curves back in, forward on legs I-II and backward on III-IV,
because the knee is raised and the tarsus is planted.

Measured on main before this package, on the hand-set stance below (bearing
0 = straight ahead, 90 = out to the side, 180 = straight back):

    leg   femur  distal  turn   knee out
    I       29     19    -10      54%
    II      80     84     +4      54%
    III    118    121     +3      54%
    IV     158    166     +8      52%

Near-straight legs, and the knee still drawn over halfway out: DC-75 put the
anatomy into `segment_lengths`, but the joints were seeded from
`segment_scales`, a width list that gives the patella the largest share, so
the new lengths only ever acted as a cap. A model that opts into `leg_curve`
is now seeded from its anatomy as well. With the tarantula's `leg_curve` of 35:

    leg   femur  distal  turn   knee out
    I       46     10    -36      42%
    II      88     79     -8      43%
    III    110    125    +15      42%
    IV     140    175    +35      39%

The stance is set by hand rather than walked, for the reason
test_leg_symmetry gives: a walk is not reproducible enough to measure shape.
"""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

import pytest
from PyQt5.QtGui import QImage

import test_creature_render_golden as G
from test_leg_symmetry import _standing
from desktop_bug.content.discovery import (
    discover_models, discover_personalities, validate_model,
)
from desktop_bug.creature import render_procedural as RP
from support import ROOT

STRAIGHT = Path(__file__).parent / "golden" / "creature_render_straight_legs.png"
PAIRS = ("front", "mid_front", "mid_rear", "rear")


@pytest.fixture(scope="module")
def content(qapp):
    models, _ = discover_models(ROOT)
    personalities, _ = discover_personalities(ROOT)
    return models, personalities


@pytest.fixture(autouse=True)
def _curves_back_on():
    yield
    RP.CURVED_LEGS = True


def _shape(content, heading=0.0):
    """name -> (femur bearing, distal bearing, knee fraction of drawn length)."""
    spider = _standing(content, heading_deg=heading, scale=1.0)
    chain = spider._sprite_leg_chain_config()
    shape = {}
    for leg in spider.legs:
        ax, ay = spider._leg_attach(leg)
        points = spider._sprite_leg_chain_points(leg, ax, ay, leg.foot_x, leg.foot_y, chain)
        side = spider._side_sign(leg.definition.get("side", "right"))
        local = [spider._world_to_body_local(x, y) for x, y in points]

        def bearing(p, q):
            return math.degrees(math.atan2((q[1] - p[1]) * side, q[0] - p[0]))

        lengths = [math.dist(local[i], local[i + 1]) for i in range(len(local) - 1)]
        shape[leg.definition["name"]] = (
            bearing(local[0], local[1]),
            bearing(local[2], local[-1]),
            sum(lengths[:2]) / sum(lengths),
        )
    return shape


def _pair(shape, stem):
    return shape[f"{stem}_right"]


# ------------------------------------------------------------------ the curve

def test_front_legs_curve_forward_and_rear_legs_curve_back(content):
    shape = _shape(content)
    for stem in ("front", "mid_front"):
        femur, distal, _ = _pair(shape, stem)
        assert distal < femur - 5.0, f"{stem}: femur {femur:.0f}, distal {distal:.0f}"
    for stem in ("mid_rear", "rear"):
        femur, distal, _ = _pair(shape, stem)
        assert distal > femur + 5.0, f"{stem}: femur {femur:.0f}, distal {distal:.0f}"


def test_legs_one_and_four_curve_the_most(content):
    shape = _shape(content)
    turn = {stem: abs(_pair(shape, stem)[1] - _pair(shape, stem)[0]) for stem in PAIRS}
    assert min(turn["front"], turn["rear"]) > max(turn["mid_front"], turn["mid_rear"]), turn
    # a curve, not a hook: legs I and IV turn through a visible angle but
    # neither folds back on itself
    assert 25.0 < turn["front"] < 60.0 and 25.0 < turn["rear"] < 60.0, turn


def test_rear_tarsi_do_not_run_past_straight_back(content):
    """At a larger curve legs IV end parallel, like a beetle's; the reference
    has them trailing *close to* parallel with the abdomen."""
    assert _pair(_shape(content), "rear")[1] < 179.0


def test_the_femurs_of_the_middle_legs_do_not_bunch(content):
    """DC-72 existed because legs II and III crowded together."""
    shape = _shape(content)
    assert _pair(shape, "mid_rear")[0] - _pair(shape, "mid_front")[0] > 15.0


def test_the_knee_sits_nearer_the_body_than_the_middle(content):
    """What DC-75 claimed and, until this package, did not draw."""
    # < 0.45 until DC-83 put the real segment shares back (knee at 41% before
    # the curve); the curve moves the front knee to about 46%.
    for name, (_, _, knee) in _shape(content).items():
        assert knee < 0.50, f"{name}: knee {100 * knee:.0f}% of the way out"
    curved = sum(knee for _, _, knee in _shape(content).values())
    RP.CURVED_LEGS = False
    straight = sum(knee for _, _, knee in _shape(content).values())
    # Pinned main's 52-54% until DC-83; that figure came from the old width
    # list. What matters is that the curve, and the anatomical seeding that
    # comes with it, is what brings the knee in.
    assert curved < straight, (curved / 8, straight / 8)


def test_left_and_right_curve_alike(content):
    shape = _shape(content)
    for stem in PAIRS:
        left, right = shape[f"{stem}_left"], shape[f"{stem}_right"]
        for a, b in zip(left, right):
            assert a == pytest.approx(b, abs=0.01), (stem, left, right)


@pytest.mark.parametrize("heading", (30.0, 90.0, 135.0, 200.0, 270.0))
def test_the_curve_does_not_depend_on_heading(content, heading):
    base, turned = _shape(content), _shape(content, heading)
    for name in base:
        for a, b in zip(base[name], turned[name]):
            assert a == pytest.approx(b, abs=0.01), (name, heading)


def test_turning_the_curve_off_straightens_the_legs(content):
    """Prove the curve is what the tests above are measuring."""
    RP.CURVED_LEGS = False
    shape = _shape(content)
    for stem in PAIRS:
        femur, distal, _ = _pair(shape, stem)
        assert abs(distal - femur) < 12.0, (stem, femur, distal)


# ----------------------------------------------------------------- the scope

def test_only_the_tarantula_opts_in(content):
    models, _ = content
    curved = sorted(mid for mid, model in models.items()
                    if (model.get("appearance") or {}).get("leg_chain", {}).get("leg_curve", 0))
    assert curved == ["tarantula"], curved


def test_turning_the_curve_off_matches_the_straight_reference(monkeypatch):
    """With curves off, match the straight-leg image at the current spider size."""
    monkeypatch.setenv("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="dc76-"))
    RP.CURVED_LEGS = False
    before = QImage(str(STRAIGHT))
    assert not before.isNull()
    assert G.max_channel_difference(G.render_reference_frame(), before) == 0


def test_a_bad_leg_curve_is_rejected_by_the_validator():
    path = ROOT / "models" / "tarantula" / "model.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    assert validate_model(data, path)[0]
    data["appearance"]["leg_chain"]["leg_curve"] = "35"
    ok, message = validate_model(data, path)
    assert not ok and "leg_curve" in message
