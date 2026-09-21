"""One leg's live state.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import List



@dataclass
class LegState:
    definition: dict
    foot_x: float = 0.0
    foot_y: float = 0.0
    step_start_x: float = 0.0
    step_start_y: float = 0.0
    step_target_x: float = 0.0
    step_target_y: float = 0.0
    step_control_x: float = 0.0
    step_control_y: float = 0.0
    pending_target_x: float = 0.0
    pending_target_y: float = 0.0
    pending_delay: float = 0.0
    step_timer: float = 0.0
    step_duration: float = 0.24
    stepping: bool = False
    pending_step: bool = False
    lift: float = 0.0
    # Randomised per instance by Creature, after construction, from the
    # creature's own seeded generator -- a dataclass default factory has no
    # access to it.
    phase_seed: float = 0.0
    twitch_clock: float = 0.0
    gait_phase_offset: float = 0.0
    step_cooldown: float = 0.0
    # Grounded locomotion state.  ``foot_x/y`` is the world-space contact;
    # these values describe how that fixed contact is being stroked behind the
    # body while the body pose is solved from all supporting legs.
    contact_state: str = "stance"
    support_weight: float = 1.0
    contact_age: float = 0.0
    stroke_progress: float = 0.0
    stance_base_f: float = 0.0
    stance_base_s: float = 0.0
    stance_stroke_f: float = 0.0
    stance_stroke_s: float = 0.0
    stance_turn: float = 0.0
    last_step_phase: float = 0.0
    last_step_emergency: bool = False
    joint_phase: float = 0.0
    joint_bends: List[float] = field(default_factory=list)
    # Suspended/carry pose dynamics. These are screen-space spring offsets for
    # the visible foot and are deliberately separate from grounded contacts.
    held_spring_x: float = 0.0
    held_spring_y: float = 0.0
    held_spring_vx: float = 0.0
    held_spring_vy: float = 0.0

