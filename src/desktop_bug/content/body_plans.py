"""The body plans every spider is built on.

Forty-nine models used to carry forty-one distinct leg rigs, nearly all of
them hand-tuned copies that nothing kept in step. Four plans now cover
forty-seven of them.

**A correction worth keeping.** The first pass at this claimed three plans
covering all forty-nine, from a metric that compared raw parameter
differences. That metric was wrong: `attach_angle` varies across the fleet
with a standard deviation of 98 while `coxa_len` varies by 0.022, a factor of
four and a half thousand, so a max-absolute-difference was measuring almost
nothing but the angles. Under it, `plush_snow_hybrid_2` looked like an
ordinary bug; normalised per parameter it is the single most distinctive
model in the set, because its legs socket on the lateral edge at
`attach_side` 0.50 rather than 0.22. The failing test that caught this was
`test_drifter.py::test_every_socket_sits_on_the_lateral_edge`, which had
encoded exactly that anatomy.

Normalising each parameter by its spread across the fleet gives the four
plans below, and two models that genuinely do not fit any of them and keep
their own rigs: `plush_snow_hybrid_2` and `knuckle_skitter_stalker`. Six
rigs, not forty-one, and none of them an accident.

The plans are lifted verbatim from the models that define them, so those four
are unchanged.

Each plan also carries its own gait, so an archetype is a way of moving and
not only a shape -- previously only three models of forty-nine enabled the
anatomical gait controller at all.

Abilities are deliberately *not* attached to a plan. What a spider can do
comes from its temperament and its job, which are independent of how it
looks; two systems granting abilities would fight over the same decision.
"""

from __future__ import annotations

from copy import deepcopy
import math


# Lifted from models/spider/model.json, unchanged.
_BUG_LEGS = [
    {
        "name": "front_left",
        "side": "left",
        "gait_group": 0,
        "attach_angle": -35,
        "rest_angle": -50,
        "reach": 1.776,
        "upper_len": 0.92,
        "lower_len": 1.15,
        "attach_forward": 0.36,
        "attach_side": 0.22,
        "rest_forward": 1.32,
        "rest_side": 1.154,
        "rest_jitter": 0.014,
        "coxa_len": 0.26,
        "stride_forward": 0.538
    },
    {
        "name": "front_right",
        "side": "right",
        "gait_group": 1,
        "attach_angle": 35,
        "rest_angle": 50,
        "reach": 1.776,
        "upper_len": 0.92,
        "lower_len": 1.15,
        "attach_forward": 0.36,
        "attach_side": 0.22,
        "rest_forward": 1.32,
        "rest_side": 1.154,
        "rest_jitter": 0.014,
        "coxa_len": 0.26,
        "stride_forward": 0.538
    },
    {
        "name": "mid_front_left",
        "side": "left",
        "gait_group": 1,
        "attach_angle": -75,
        "rest_angle": -92,
        "reach": 1.651,
        "upper_len": 0.88,
        "lower_len": 1.16,
        "attach_forward": 0.12,
        "attach_side": 0.31,
        "rest_forward": 0.46,
        "rest_side": 1.317,
        "rest_jitter": 0.016,
        "coxa_len": 0.25,
        "stride_forward": 0.461
    },
    {
        "name": "mid_front_right",
        "side": "right",
        "gait_group": 0,
        "attach_angle": 75,
        "rest_angle": 92,
        "reach": 1.651,
        "upper_len": 0.88,
        "lower_len": 1.16,
        "attach_forward": 0.12,
        "attach_side": 0.31,
        "rest_forward": 0.46,
        "rest_side": 1.317,
        "rest_jitter": 0.014,
        "coxa_len": 0.25,
        "stride_forward": 0.461
    },
    {
        "name": "mid_rear_left",
        "side": "left",
        "gait_group": 0,
        "attach_angle": -108,
        "rest_angle": -118,
        "reach": 1.632,
        "upper_len": 0.9,
        "lower_len": 1.12,
        "attach_forward": -0.12,
        "attach_side": 0.31,
        "rest_forward": -0.46,
        "rest_side": 1.273,
        "rest_jitter": 0.014,
        "coxa_len": 0.25,
        "stride_forward": 0.435
    },
    {
        "name": "mid_rear_right",
        "side": "right",
        "gait_group": 1,
        "attach_angle": 108,
        "rest_angle": 118,
        "reach": 1.632,
        "upper_len": 0.9,
        "lower_len": 1.12,
        "attach_forward": -0.12,
        "attach_side": 0.31,
        "rest_forward": -0.46,
        "rest_side": 1.273,
        "rest_jitter": 0.018,
        "coxa_len": 0.25,
        "stride_forward": 0.435
    },
    {
        "name": "rear_left",
        "side": "left",
        "gait_group": 1,
        "attach_angle": -142,
        "rest_angle": -150,
        "reach": 1.632,
        "upper_len": 0.92,
        "lower_len": 1.12,
        "attach_forward": -0.36,
        "attach_side": 0.23,
        "rest_forward": -1.28,
        "rest_side": 1.006,
        "rest_jitter": 0.016,
        "coxa_len": 0.26,
        "stride_forward": 0.397
    },
    {
        "name": "rear_right",
        "side": "right",
        "gait_group": 0,
        "attach_angle": 142,
        "rest_angle": 150,
        "reach": 1.632,
        "upper_len": 0.92,
        "lower_len": 1.12,
        "attach_forward": -0.36,
        "attach_side": 0.23,
        "rest_forward": -1.28,
        "rest_side": 1.006,
        "rest_jitter": 0.014,
        "coxa_len": 0.26,
        "stride_forward": 0.397
    }
]

# Lifted from models/giant_copper/model.json, unchanged.
_SEGMENTED_LEGS = [
    {
        "name": "front_left",
        "side": "left",
        "gait_group": 0,
        "phase_offset": 0.0,
        "attach_angle": -30,
        "rest_angle": -43,
        "reach": 2.18,
        "upper_len": 1.02,
        "lower_len": 1.25,
        "attach_forward": 0.42,
        "attach_side": 0.27,
        "rest_forward": 1.62,
        "rest_side": 0.98,
        "coxa_len": 0.31,
        "stride_forward": 0.5
    },
    {
        "name": "front_right",
        "side": "right",
        "gait_group": 1,
        "phase_offset": 0.5,
        "attach_angle": 30,
        "rest_angle": 43,
        "reach": 2.18,
        "upper_len": 1.02,
        "lower_len": 1.25,
        "attach_forward": 0.42,
        "attach_side": 0.27,
        "rest_forward": 1.62,
        "rest_side": 0.98,
        "coxa_len": 0.31,
        "stride_forward": 0.5
    },
    {
        "name": "mid_front_left",
        "side": "left",
        "gait_group": 1,
        "phase_offset": 0.74,
        "attach_angle": -70,
        "rest_angle": -84,
        "reach": 2.12,
        "upper_len": 0.98,
        "lower_len": 1.28,
        "attach_forward": 0.14,
        "attach_side": 0.37,
        "rest_forward": 0.54,
        "rest_side": 1.2,
        "coxa_len": 0.32,
        "stride_forward": 0.47
    },
    {
        "name": "mid_front_right",
        "side": "right",
        "gait_group": 0,
        "phase_offset": 0.24,
        "attach_angle": 70,
        "rest_angle": 84,
        "reach": 2.12,
        "upper_len": 0.98,
        "lower_len": 1.28,
        "attach_forward": 0.14,
        "attach_side": 0.37,
        "rest_forward": 0.54,
        "rest_side": 1.2,
        "coxa_len": 0.32,
        "stride_forward": 0.47
    },
    {
        "name": "mid_rear_left",
        "side": "left",
        "gait_group": 0,
        "phase_offset": 0.44,
        "attach_angle": -110,
        "rest_angle": -119,
        "reach": 2.09,
        "upper_len": 1.0,
        "lower_len": 1.25,
        "attach_forward": -0.14,
        "attach_side": 0.37,
        "rest_forward": -0.54,
        "rest_side": 1.18,
        "coxa_len": 0.32,
        "stride_forward": 0.45
    },
    {
        "name": "mid_rear_right",
        "side": "right",
        "gait_group": 1,
        "phase_offset": 0.94,
        "attach_angle": 110,
        "rest_angle": 119,
        "reach": 2.09,
        "upper_len": 1.0,
        "lower_len": 1.25,
        "attach_forward": -0.14,
        "attach_side": 0.37,
        "rest_forward": -0.54,
        "rest_side": 1.18,
        "coxa_len": 0.32,
        "stride_forward": 0.45
    },
    {
        "name": "rear_left",
        "side": "left",
        "gait_group": 1,
        "phase_offset": 0.66,
        "attach_angle": -147,
        "rest_angle": -151,
        "reach": 2.15,
        "upper_len": 1.05,
        "lower_len": 1.23,
        "attach_forward": -0.42,
        "attach_side": 0.27,
        "rest_forward": -1.6,
        "rest_side": 0.98,
        "coxa_len": 0.31,
        "stride_forward": 0.43
    },
    {
        "name": "rear_right",
        "side": "right",
        "gait_group": 0,
        "phase_offset": 0.16,
        "attach_angle": 147,
        "rest_angle": 151,
        "reach": 2.15,
        "upper_len": 1.05,
        "lower_len": 1.23,
        "attach_forward": -0.42,
        "attach_side": 0.27,
        "rest_forward": -1.6,
        "rest_side": 0.98,
        "coxa_len": 0.31,
        "stride_forward": 0.43
    }
]

# Lifted from models/tarantula/model.json, unchanged.
_TARANTULA_LEGS = [
    {
        "name": "front_left",
        "side": "left",
        "gait_group": 0,
        "phase_offset": 0,
        "attach_angle": -32,
        "rest_angle": -27,
        "reach": 2.24,
        "upper_len": 1.04,
        "lower_len": 1.29,
        "attach_forward": 0.459,
        "attach_side": 0.174,
        "rest_forward": 1.443,
        "rest_side": 0.735,
        "rest_jitter": 0.006,
        "coxa_len": 0.3,
        "stride_forward": 0.5
    },
    {
        "name": "front_right",
        "side": "right",
        "gait_group": 1,
        "phase_offset": 0.5,
        "attach_angle": 32,
        "rest_angle": 27,
        "reach": 2.24,
        "upper_len": 1.04,
        "lower_len": 1.29,
        "attach_forward": 0.459,
        "attach_side": 0.174,
        "rest_forward": 1.443,
        "rest_side": 0.735,
        "rest_jitter": 0.006,
        "coxa_len": 0.3,
        "stride_forward": 0.5
    },
    {
        "name": "mid_front_left",
        "side": "left",
        "gait_group": 1,
        "phase_offset": 0.72,
        "attach_angle": -48,
        "rest_angle": -60,
        "reach": 2.085,
        "upper_len": 0.968,
        "lower_len": 1.201,
        "attach_forward": 0.301,
        "attach_side": 0.261,
        "rest_forward": 0.754,
        "rest_side": 1.306,
        "rest_jitter": 0.006,
        "coxa_len": 0.31,
        "stride_forward": 0.46
    },
    {
        "name": "mid_front_right",
        "side": "right",
        "gait_group": 0,
        "phase_offset": 0.22,
        "attach_angle": 48,
        "rest_angle": 60,
        "reach": 2.085,
        "upper_len": 0.968,
        "lower_len": 1.201,
        "attach_forward": 0.301,
        "attach_side": 0.261,
        "rest_forward": 0.754,
        "rest_side": 1.306,
        "rest_jitter": 0.006,
        "coxa_len": 0.31,
        "stride_forward": 0.46
    },
    {
        "name": "mid_rear_left",
        "side": "left",
        "gait_group": 0,
        "phase_offset": 0.44,
        "attach_angle": -71,
        "rest_angle": -120,
        "reach": 1.962,
        "upper_len": 0.911,
        "lower_len": 1.13,
        "attach_forward": 0.113,
        "attach_side": 0.254,
        "rest_forward": -0.71,
        "rest_side": 1.229,
        "rest_jitter": 0.006,
        "coxa_len": 0.31,
        "stride_forward": 0.44
    },
    {
        "name": "mid_rear_right",
        "side": "right",
        "gait_group": 1,
        "phase_offset": 0.94,
        "attach_angle": 71,
        "rest_angle": 120,
        "reach": 1.962,
        "upper_len": 0.911,
        "lower_len": 1.13,
        "attach_forward": 0.113,
        "attach_side": 0.254,
        "rest_forward": -0.71,
        "rest_side": 1.229,
        "rest_jitter": 0.006,
        "coxa_len": 0.31,
        "stride_forward": 0.44
    },
    {
        "name": "rear_left",
        "side": "left",
        "gait_group": 1,
        "phase_offset": 0.66,
        "attach_angle": -104,
        "rest_angle": -153,
        "reach": 2.372,
        "upper_len": 1.101,
        "lower_len": 1.366,
        "attach_forward": -0.035,
        "attach_side": 0.155,
        "rest_forward": -1.529,
        "rest_side": 0.779,
        "rest_jitter": 0.006,
        "coxa_len": 0.3,
        "stride_forward": 0.41
    },
    {
        "name": "rear_right",
        "side": "right",
        "gait_group": 0,
        "phase_offset": 0.16,
        "attach_angle": 104,
        "rest_angle": 153,
        "reach": 2.372,
        "upper_len": 1.101,
        "lower_len": 1.366,
        "attach_forward": -0.035,
        "attach_side": 0.155,
        "rest_forward": -1.529,
        "rest_side": 0.779,
        "rest_jitter": 0.006,
        "coxa_len": 0.3,
        "stride_forward": 0.41
    }
]

# Lifted from models/silk_peacock_jumper/model.json, unchanged.
_JUMPER_LEGS = [
    {
        "name": "front_left",
        "side": "left",
        "gait_group": 0,
        "attach_angle": -35,
        "rest_angle": -50,
        "reach": 1.253,
        "upper_len": 0.68,
        "lower_len": 0.831,
        "attach_forward": 0.36,
        "attach_side": 0.189,
        "rest_forward": 1.32,
        "rest_side": 0.844,
        "rest_jitter": 0.017,
        "coxa_len": 0.239,
        "stride_forward": 0.519
    },
    {
        "name": "front_right",
        "side": "right",
        "gait_group": 1,
        "attach_angle": 35,
        "rest_angle": 50,
        "reach": 1.253,
        "upper_len": 0.68,
        "lower_len": 0.831,
        "attach_forward": 0.36,
        "attach_side": 0.189,
        "rest_forward": 1.32,
        "rest_side": 0.844,
        "rest_jitter": 0.017,
        "coxa_len": 0.239,
        "stride_forward": 0.519
    },
    {
        "name": "mid_front_left",
        "side": "left",
        "gait_group": 1,
        "attach_angle": -75,
        "rest_angle": -92,
        "reach": 1.165,
        "upper_len": 0.65,
        "lower_len": 0.838,
        "attach_forward": 0.12,
        "attach_side": 0.267,
        "rest_forward": 0.46,
        "rest_side": 0.963,
        "rest_jitter": 0.02,
        "coxa_len": 0.23,
        "stride_forward": 0.446
    },
    {
        "name": "mid_front_right",
        "side": "right",
        "gait_group": 0,
        "attach_angle": 75,
        "rest_angle": 92,
        "reach": 1.165,
        "upper_len": 0.65,
        "lower_len": 0.838,
        "attach_forward": 0.12,
        "attach_side": 0.267,
        "rest_forward": 0.46,
        "rest_side": 0.963,
        "rest_jitter": 0.017,
        "coxa_len": 0.23,
        "stride_forward": 0.446
    },
    {
        "name": "mid_rear_left",
        "side": "left",
        "gait_group": 0,
        "attach_angle": -108,
        "rest_angle": -118,
        "reach": 1.152,
        "upper_len": 0.665,
        "lower_len": 0.809,
        "attach_forward": -0.12,
        "attach_side": 0.267,
        "rest_forward": -0.46,
        "rest_side": 0.931,
        "rest_jitter": 0.017,
        "coxa_len": 0.23,
        "stride_forward": 0.421
    },
    {
        "name": "mid_rear_right",
        "side": "right",
        "gait_group": 1,
        "attach_angle": 108,
        "rest_angle": 118,
        "reach": 1.152,
        "upper_len": 0.665,
        "lower_len": 0.809,
        "attach_forward": -0.12,
        "attach_side": 0.267,
        "rest_forward": -0.46,
        "rest_side": 0.931,
        "rest_jitter": 0.022,
        "coxa_len": 0.23,
        "stride_forward": 0.421
    },
    {
        "name": "rear_left",
        "side": "left",
        "gait_group": 1,
        "attach_angle": -142,
        "rest_angle": -150,
        "reach": 1.152,
        "upper_len": 0.68,
        "lower_len": 0.809,
        "attach_forward": -0.36,
        "attach_side": 0.198,
        "rest_forward": -1.28,
        "rest_side": 0.735,
        "rest_jitter": 0.02,
        "coxa_len": 0.239,
        "stride_forward": 0.384
    },
    {
        "name": "rear_right",
        "side": "right",
        "gait_group": 0,
        "attach_angle": 142,
        "rest_angle": 150,
        "reach": 1.152,
        "upper_len": 0.68,
        "lower_len": 0.809,
        "attach_forward": -0.36,
        "attach_side": 0.198,
        "rest_forward": -1.28,
        "rest_side": 0.735,
        "rest_jitter": 0.017,
        "coxa_len": 0.239,
        "stride_forward": 0.384
    }
]

# Each plan's gait. A heavy, forward-legged body swings low and slowly with a
# long stance; a light bug-bodied one picks its feet up faster and turns more
# sharply; the segmented plan has long legs, a big stride and an unhurried
# cadence; a jumper is quick, high-stepping and turns on the spot.
_BUG_GAIT = {
    "enabled": True, "profile": "bug", "max_airborne": 3,
    "cycle_hz": 2.45, "swing_fraction": 0.30, "swing_height": 0.58,
    "stride_gain": 1.00, "turn_gain": 1.15, "max_body_turn_rate": 3.10,
    "stance_deadband": 0.34,
}
_SEGMENTED_GAIT = {
    "enabled": True, "profile": "segmented", "max_airborne": 3,
    "cycle_hz": 2.00, "swing_fraction": 0.31, "swing_height": 0.62,
    "stride_gain": 1.22, "front_stride_bias": 0.20, "turn_gain": 0.95,
    "max_body_turn_rate": 2.40, "stance_deadband": 0.40,
}
_TARANTULA_GAIT = {
    "enabled": True, "profile": "tarantula", "max_airborne": 3,
    "cycle_hz": 1.80, "swing_fraction": 0.28, "swing_height": 0.50,
    "stride_gain": 0.92, "front_stride_bias": 0.10, "turn_gain": 0.85,
    "max_body_turn_rate": 2.10, "stance_deadband": 0.44,
}
_JUMPER_GAIT = {
    "enabled": True, "profile": "jumper", "max_airborne": 3,
    "cycle_hz": 2.90, "swing_fraction": 0.33, "swing_height": 0.70,
    "stride_gain": 1.05, "front_stride_bias": 0.24, "turn_gain": 1.35,
    "max_body_turn_rate": 3.60, "stance_deadband": 0.30,
}

# Enemy-only rigs. Roots sit underneath the carapace, while the crab's
# first two pairs reach sideways and the orb-weaver braces its broad abdomen.
def _enemy_legs(crab: bool) -> list:
    legs = deepcopy(_BUG_LEGS)
    for index, leg in enumerate(legs):
        pair = index // 2
        angle = (58, 82, 120, 152)[pair] if crab else (35, 66, 118, 155)[pair]
        reach = (2.25, 2.15, 1.28, 1.15)[pair] if crab else (1.85, 1.65, 1.50, 1.95)[pair]
        leg.update(
            attach_forward=(0.48, 0.29, 0.09, -0.10)[pair],
            attach_side=(0.25, 0.36, 0.36, 0.25)[pair] if crab else (0.19, 0.27, 0.27, 0.19)[pair],
            rest_angle=angle * (-1 if leg["side"] == "left" else 1),
            rest_forward=math.cos(math.radians(angle)) * reach,
            rest_side=math.sin(math.radians(angle)) * reach,
            reach=reach, upper_len=reach * 0.48, lower_len=reach * 0.62,
        )
    return legs


BODY_PLANS = {
    "bug": {"legs": _BUG_LEGS, "spider_gait": _BUG_GAIT},
    "segmented": {"legs": _SEGMENTED_LEGS, "spider_gait": _SEGMENTED_GAIT},
    "tarantula": {"legs": _TARANTULA_LEGS, "spider_gait": _TARANTULA_GAIT},
    "jumper": {"legs": _JUMPER_LEGS, "spider_gait": _JUMPER_GAIT},
    "enemy_crab": {"legs": _enemy_legs(True),
                   "spider_gait": {**_BUG_GAIT, "cycle_hz": 2.15}},
    "enemy_orb": {"legs": _enemy_legs(False),
                  "spider_gait": {**_BUG_GAIT, "cycle_hz": 1.65}},
}

BODY_PLAN_IDS = tuple(BODY_PLANS)

DEFAULT_BODY_PLAN = "bug"


def body_plan_legs(plan_id: str) -> list:
    """A fresh copy of the plan's rig.

    Deep-copied because `Creature` keeps each leg definition and models are
    long-lived: handing out the shared list would let one spider's tuning
    leak into every other spider of that plan.
    """
    plan = BODY_PLANS.get(str(plan_id or "").strip().lower())
    return deepcopy(plan["legs"]) if plan else []


def body_plan_gait(plan_id: str) -> dict | None:
    plan = BODY_PLANS.get(str(plan_id or "").strip().lower())
    return deepcopy(plan["spider_gait"]) if plan else None


def resolve_body_plan(data: dict) -> dict:
    """Fill in a model's rig and gait from the plan it names.

    A model keeps whatever it states for itself: an explicit ``legs`` array
    still wins, and an ``appearance.spider_gait`` still overrides the plan's.
    That is what lets `plush_snow_hybrid_2` and `knuckle_skitter_stalker`
    keep skeletons no plan describes, without reopening the archetypes or
    pretending every model must fit one.
    """
    if not isinstance(data, dict):
        return data
    plan_id = str(data.get("body_plan", "") or "").strip().lower()
    if not plan_id or plan_id not in BODY_PLANS:
        return data
    if not data.get("legs"):
        data["legs"] = body_plan_legs(plan_id)
    appearance = data.get("appearance")
    if not isinstance(appearance, dict):
        appearance = {}
        data["appearance"] = appearance
    if "spider_gait" not in appearance:
        appearance["spider_gait"] = body_plan_gait(plan_id)
    return data
